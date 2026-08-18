// SPDX-License-Identifier: GPL-3.0-or-later
//! TOS Stage 1 UEFI loader.
//!
//! Reads `\capsule.bin` and `\nucleus.bin` from the loader's ESP, validates the
//! capsule, allocates all handoff structures, converts the EFI memory map to
//! TOS [`MemoryRange`] descriptors, calls `ExitBootServices`, then hands control
//! to the nucleus: `rdi` = physical address of the boot ABI v1 record.
//!
//! Stage 1 assumption (ADR-0005, QEMU/OVMF only): UEFI identity-maps all RAM,
//! so virtual == physical for pool buffers and page allocations.
//!
//! ABI discipline: every firmware entry point and protocol function pointer is
//! `extern "efiapi"` (see `efi.rs`). The loader performs no boot-services calls
//! after `ExitBootServices` succeeds; `ExitBootServices` itself is retried with
//! a freshly re-read memory map when the map key turns stale.

#![cfg_attr(not(test), no_main)]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(test, allow(dead_code, unused_assignments, unused_imports))]

mod efi;

use core::arch::asm;
use core::ffi::c_void;
use core::panic::PanicInfo;
use core::ptr;

use efi::*;
use tos_boot_protocol::{
    BootInfo, MemoryRange, FB_FORMAT_BGRX8, FB_FORMAT_RGBX8, MEM_ACPI_NVS, MEM_ACPI_RECLAIM,
    MEM_MMIO, MEM_RESERVED, MEM_USABLE, RESULT_CAPSULE_INVALID, RESULT_PANIC,
};
use tos_capsule::{parse, CapsError, MAX_CAPSULE_BYTES};
use tos_hash::sha256;

// The nucleus runs the Stage 2 reference path on this stack: a recursive
// descent parser, a checker, a lowerer, the verifier and the engine, each
// recursing over nested source. 32 KiB was sized for a nucleus that only
// validated records, and a stack overflow at boot does not report an error —
// it writes over whatever lies below. The nucleus measures what it actually
// uses and reports it as TOS.RUN.STACK, so this number is checked rather than
// assumed. The region is reserved in the handed-over memory map either way.
const STACK_PAGES: usize = 512; // 2 MiB

/// Fixed physical load address of the nucleus (must match nucleus/linker.ld).
const NUCLEUS_BASE: u64 = 0x2_000_000;
const MAX_FIRMWARE_ENTRY_BYTES: usize = 4096;
const MAX_CONFIGURATION_TABLES: usize = 1024;

#[derive(Clone, Copy)]
struct PlatformHandoff {
    fb_phys: u64,
    fb_width: u32,
    fb_height: u32,
    fb_pitch: u32,
    fb_format: u32,
    acpi_rsdp: u64,
    acpi_version: u8,
    smbios: u64,
    smbios_version: u8,
}

fn byte_sum_is_zero(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn efi_output_is_valid<T>(status: EfiStatus, ptr: *mut T) -> bool {
    efi_success(status) && !ptr.is_null()
}

/// SAFETY: `ptr` must designate at least `len` readable firmware-owned bytes
/// in the UEFI identity map for the returned borrow's use; callers pass only
/// fixed headers or validated declared lengths.
unsafe fn firmware_bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if ptr.is_null() || len > MAX_FIRMWARE_ENTRY_BYTES {
        return None;
    }
    // SAFETY: configuration-table pointers are physical under the UEFI ABI and
    // ADR-0005's QEMU/OVMF identity-map assumption; every caller bounds the
    // fixed/declared header before inspecting it.
    Some(unsafe { core::slice::from_raw_parts(ptr, len) })
}

/// SAFETY: `st` must be the firmware's live SystemTable while boot services
/// remain active, including its bounded ConfigurationTable array.
unsafe fn config_table(st: *mut SystemTable, wanted: Guid) -> Option<*mut c_void> {
    // SAFETY: the caller upholds the live SystemTable contract above.
    let count = unsafe { (*st).number_of_table_entries };
    if count > MAX_CONFIGURATION_TABLES {
        return None;
    }
    // SAFETY: the same SystemTable contract permits reading this table base.
    let tables = unsafe { (*st).configuration_table as *const ConfigurationTable };
    if tables.is_null() {
        return None;
    }
    for i in 0..count {
        // SAFETY: UEFI system table supplies exactly `count` configuration
        // entries for the lifetime of boot services.
        let entry = unsafe { &*tables.add(i) };
        if entry.vendor_guid == wanted {
            return Some(entry.vendor_table);
        }
    }
    None
}

/// SAFETY: `ptr` is a configuration-table address supplied by live firmware;
/// this function bounds every read through firmware_bytes before inspection.
unsafe fn validate_rsdp(ptr: *mut c_void, v2: bool) -> Option<u64> {
    // SAFETY: the function contract limits this to the fixed ACPI v1 prefix.
    let base = unsafe { firmware_bytes(ptr as *const u8, 20) }?;
    if &base[..8] != b"RSD PTR " || !byte_sum_is_zero(base) {
        return None;
    }
    if !v2 {
        return Some(ptr as u64);
    }
    // SAFETY: the function contract limits this to the fixed ACPI v2 prefix.
    let prefix = unsafe { firmware_bytes(ptr as *const u8, 24) }?;
    if prefix[15] < 2 {
        return None;
    }
    let len = u32::from_le_bytes(prefix[20..24].try_into().ok()?) as usize;
    if len < 36 {
        return None;
    }
    // SAFETY: `len` was read from the checked prefix and firmware_bytes caps
    // it at MAX_FIRMWARE_ENTRY_BYTES before making the slice.
    let full = unsafe { firmware_bytes(ptr as *const u8, len) }?;
    if !byte_sum_is_zero(full) {
        return None;
    }
    Some(ptr as u64)
}

/// SAFETY: `ptr` is a configuration-table address supplied by live firmware;
/// this function bounds every entry-point read before inspection.
unsafe fn validate_smbios(ptr: *mut c_void, v3: bool) -> Option<u64> {
    // SAFETY: the function contract limits this to the relevant fixed prefix.
    let head = unsafe { firmware_bytes(ptr as *const u8, if v3 { 24 } else { 31 }) }?;
    let (anchor, min_len) = if v3 {
        (b"_SM3_".as_slice(), 24)
    } else {
        (b"_SM_".as_slice(), 31)
    };
    if &head[..anchor.len()] != anchor {
        return None;
    }
    let len = head[if v3 { 6 } else { 5 }] as usize;
    if len < min_len {
        return None;
    }
    // SAFETY: `len` comes from the checked fixed prefix and is capped by
    // firmware_bytes before the declared entry-point slice is formed.
    let full = unsafe { firmware_bytes(ptr as *const u8, len) }?;
    if !byte_sum_is_zero(full) {
        return None;
    }
    if !v3 && (&full[16..21] != b"_DMI_" || !byte_sum_is_zero(&full[16..31])) {
        return None;
    }
    Some(ptr as u64)
}

/// SAFETY: `st` is the live UEFI SystemTable for the whole configuration-table
/// selection and any returned table pointer remains firmware-owned.
unsafe fn select_acpi(st: *mut SystemTable) -> Option<(u64, u8)> {
    // SAFETY: the function contract supplies the live SystemTable.
    match unsafe { config_table(st, GUID_ACPI_20) } {
        Some(ptr) => {
            // SAFETY: ptr came from the selected live firmware table.
            Some((unsafe { validate_rsdp(ptr, true) }?, 2))
        }
        None => {
            // SAFETY: the function contract supplies the live SystemTable.
            match unsafe { config_table(st, GUID_ACPI_10) } {
                Some(ptr) => {
                    // SAFETY: ptr came from the selected live firmware table.
                    Some((unsafe { validate_rsdp(ptr, false) }?, 1))
                }
                None => Some((0, 0)),
            }
        }
    }
}

/// SAFETY: `st` is the live UEFI SystemTable for the whole configuration-table
/// selection and any returned table pointer remains firmware-owned.
unsafe fn select_smbios(st: *mut SystemTable) -> Option<(u64, u8)> {
    // SAFETY: the function contract supplies the live SystemTable.
    match unsafe { config_table(st, GUID_SMBIOS3) } {
        Some(ptr) => {
            // SAFETY: ptr came from the selected live firmware table.
            Some((unsafe { validate_smbios(ptr, true) }?, 3))
        }
        None => {
            // SAFETY: the function contract supplies the live SystemTable.
            match unsafe { config_table(st, GUID_SMBIOS) } {
                Some(ptr) => {
                    // SAFETY: ptr came from the selected live firmware table.
                    Some((unsafe { validate_smbios(ptr, false) }?, 2))
                }
                None => Some((0, 0)),
            }
        }
    }
}

/// SAFETY: `st` and `bt` are the live firmware tables for the duration of boot
/// services; all protocol pointers are checked before dereference.
unsafe fn collect_platform(st: *mut SystemTable, bt: *mut BootServices) -> Option<PlatformHandoff> {
    let mut raw: *mut c_void = ptr::null_mut();
    // SAFETY: the function contract supplies the live BootServices table and
    // writable local storage for LocateProtocol's output pointer.
    let status =
        unsafe { ((*bt).locate_protocol)(&GUID_GRAPHICS_OUTPUT, ptr::null_mut(), &mut raw) };
    let (fb_phys, fb_width, fb_height, fb_pitch, fb_format) = if status == EFI_NOT_FOUND {
        (0, 0, 0, 0, 0)
    } else {
        if efi_error(status) || raw.is_null() {
            return None;
        }
        // SAFETY: successful LocateProtocol returned the non-null GOP pointer.
        let gop = unsafe { &*(raw as *const GraphicsOutputProtocol) };
        if gop.mode.is_null() {
            return None;
        }
        // SAFETY: the non-null mode pointer belongs to the live GOP protocol.
        let mode = unsafe { &*gop.mode };
        if mode.info.is_null()
            || mode.size_of_info < core::mem::size_of::<GraphicsOutputModeInfo>()
            || mode.framebuffer_base == 0
        {
            return None;
        }
        // SAFETY: size_of_info was checked before dereferencing GOP mode info.
        let info = unsafe { &*mode.info };
        let format = match info.pixel_format {
            PIXEL_RGBX8 => FB_FORMAT_RGBX8,
            PIXEL_BGRX8 => FB_FORMAT_BGRX8,
            PIXEL_BIT_MASK | PIXEL_BLT_ONLY => return None,
            _ => return None,
        };
        let pitch = info.pixels_per_scan_line.checked_mul(4)?;
        let required = (pitch as u64).checked_mul(info.vertical_resolution as u64)?;
        if info.horizontal_resolution == 0
            || info.vertical_resolution == 0
            || pitch < info.horizontal_resolution.checked_mul(4)?
            || usize::try_from(required).ok()? > mode.framebuffer_size
            || mode.framebuffer_base.checked_add(required).is_none()
        {
            return None;
        }
        (
            mode.framebuffer_base,
            info.horizontal_resolution,
            info.vertical_resolution,
            pitch,
            format,
        )
    };
    // SAFETY: the function contract supplies the live SystemTable.
    let (acpi_rsdp, acpi_version) = unsafe { select_acpi(st) }?;
    // SAFETY: the function contract supplies the live SystemTable.
    let (smbios, smbios_version) = unsafe { select_smbios(st) }?;
    Some(PlatformHandoff {
        fb_phys,
        fb_width,
        fb_height,
        fb_pitch,
        fb_format,
        acpi_rsdp,
        acpi_version,
        smbios,
        smbios_version,
    })
}

/// A pool-allocated byte buffer. Never freed (we exit boot services).
struct PoolBuf {
    ptr: *mut u8,
    len: usize,
}

impl PoolBuf {
    fn as_slice(&self) -> &[u8] {
        // SAFETY: PoolBuf is created only after successful AllocatePool with
        // this exact size; it is never freed before ExitBootServices handoff.
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    fn len(&self) -> usize {
        self.len
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    tos_serial::puts(b"TOS.PANIC loader\r\n");
    result_port(RESULT_PANIC)
}

/// Write an exit code to the QEMU isa-debug-exit port and stop.
fn result_port(code: u8) -> ! {
    // SAFETY: RESULT_PORT is QEMU's fixed isa-debug-exit port in the declared
    // profile; this byte OUT has no memory operands.
    unsafe {
        asm!(
            "out dx, al",
            in("al") code,
            in("dx") tos_boot_protocol::RESULT_PORT,
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        // SAFETY: this terminal path owns the CPU and external interrupts are
        // not enabled by Stage 1, so HLT cannot resume normal loader work.
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Print a message to serial then halt with the panic code.
fn serial_fatal(msg: &[u8]) -> ! {
    tos_serial::puts(msg);
    tos_serial::puts(b"\r\n");
    result_port(RESULT_PANIC)
}

/// Emit the stable loader capsule-rejection event before the nucleus can run.
fn capsule_fatal(error: CapsError) -> ! {
    tos_serial::puts(b"TOS.BOOT.FAILC capsule_err=");
    tos_serial::puts(error_tag(error));
    tos_serial::puts(b"\r\n");
    result_port(RESULT_CAPSULE_INVALID)
}

/// SimpleTextOutput console message (ASCII -> UTF-16). Diagnostic only; a
/// console failure is non-fatal (serial remains the authoritative channel).
fn conout(st: *mut SystemTable, s: &str) {
    // SAFETY: efi_main's firmware entry contract supplies a live SystemTable;
    // null con_out is checked immediately below.
    let out = unsafe { (*st).con_out };
    if out.is_null() {
        return;
    }
    let mut buf = [0u16; 256];
    let mut n = 0;
    for b in s.bytes() {
        if n == buf.len() - 1 {
            break;
        }
        buf[n] = b as u16;
        n += 1;
    }
    buf[n] = 0;
    // SAFETY: non-null con_out is the firmware SimpleTextOutput protocol and
    // buf is a NUL-terminated stack UTF-16 sequence for the duration of call.
    unsafe {
        ((*out).output_string)(out, buf.as_ptr());
    }
}

fn error_tag(e: CapsError) -> &'static [u8] {
    match e {
        CapsError::InputTooShort => b"InputTooShort",
        CapsError::CapsuleTooLarge => b"CapsuleTooLarge",
        CapsError::FileCountTooLarge => b"FileCountTooLarge",
        CapsError::PathTooLong => b"PathTooLong",
        CapsError::NameArenaTooLarge => b"NameArenaTooLarge",
        CapsError::LicenceNoticeTooLarge => b"LicenceNoticeTooLarge",
        CapsError::BadMagic => b"BadMagic",
        CapsError::BadUuid => b"BadUuid",
        CapsError::BadFormatVersion => b"BadFormatVersion",
        CapsError::BadHeaderSize => b"BadHeaderSize",
        CapsError::BadAlignment => b"BadAlignment",
        CapsError::NonZeroReservedHeader => b"NonZeroReservedHeader",
        CapsError::TotalLengthMismatch => b"TotalLengthMismatch",
        CapsError::BadArchVersion => b"BadArchVersion",
        CapsError::BadBuilderVersion => b"BadBuilderVersion",
        CapsError::BadPathEntrySize => b"BadPathEntrySize",
        CapsError::BadFileEntrySize => b"BadFileEntrySize",
        CapsError::RegionOverflow => b"RegionOverflow",
        CapsError::LayoutMismatch => b"LayoutMismatch",
        CapsError::BadUtf8 => b"BadUtf8",
        CapsError::NulInPath => b"NulInPath",
        CapsError::ControlInPath => b"ControlInPath",
        CapsError::NonAbsolutePath => b"NonAbsolutePath",
        CapsError::TraversalInPath => b"TraversalInPath",
        CapsError::EmptyComponent => b"EmptyComponent",
        CapsError::DuplicatePath => b"DuplicatePath",
        CapsError::UnsortedPathTable => b"UnsortedPathTable",
        CapsError::BadPathFlags => b"BadPathFlags",
        CapsError::PathFileIndexOutOfRange => b"PathFileIndexOutOfRange",
        CapsError::NameOutOfArena => b"NameOutOfArena",
        CapsError::PathTableNotAfterHeader => b"PathTableNotAfterHeader",
        CapsError::UnpackedNameArena => b"UnpackedNameArena",
        CapsError::BadFileFlags => b"BadFileFlags",
        CapsError::NonZeroReservedEntry => b"NonZeroReservedEntry",
        CapsError::UnsortedFileTable => b"UnsortedFileTable",
        CapsError::PayloadOverlap => b"PayloadOverlap",
        CapsError::PayloadGap => b"PayloadGap",
        CapsError::ZeroFileCount => b"ZeroFileCount",
        CapsError::BadDigest => b"BadDigest",
        CapsError::BadWholeDigest => b"BadWholeDigest",
        CapsError::UnsupportedIdentityKind => b"UnsupportedIdentityKind",
        CapsError::NonZeroOidPadding => b"NonZeroOidPadding",
        CapsError::DetachedIdentityMismatch => b"DetachedIdentityMismatch",
        CapsError::LicenceOutOfBounds => b"LicenceOutOfBounds",
        CapsError::MissingBootCanonical => b"MissingBootCanonical",
        CapsError::DuplicateBootCanonical => b"DuplicateBootCanonical",
        CapsError::BadBootCanonicalName => b"BadBootCanonicalName",
        CapsError::NonCanonicalFileIndex => b"NonCanonicalFileIndex",
        CapsError::UnreferencedFile => b"UnreferencedFile",
        CapsError::BootCanonicalFlagMismatch => b"BootCanonicalFlagMismatch",
        CapsError::BootCanonicalOnWrongFile => b"BootCanonicalOnWrongFile",
        CapsError::LicenceTailMismatch => b"LicenceTailMismatch",
    }
}

/// Allocate a pool buffer (RuntimeServicesData: survives ExitBootServices).
fn alloc_pool(bt: *mut BootServices, size: usize) -> Option<PoolBuf> {
    let mut p: *mut c_void = ptr::null_mut();
    // SAFETY: efi_main obtained this live BootServices table from firmware and
    // `p` is writable local storage for AllocatePool's returned allocation.
    let st = unsafe { ((*bt).allocate_pool)(MEM_TYPE_RUNTIME_SERVICES_DATA, size, &mut p) };
    if !efi_output_is_valid(st, p) {
        return None;
    }
    Some(PoolBuf {
        ptr: p as *mut u8,
        len: size,
    })
}

/// Open a file read-only on the root volume.
fn open_ro(root: *mut FileProtocol, name: &str) -> Option<*mut FileProtocol> {
    let mut wide = [0u16; 64];
    let mut n = 0;
    for &b in name.as_bytes() {
        if n >= wide.len() - 1 {
            return None;
        }
        wide[n] = b as u16;
        n += 1;
    }
    wide[n] = 0;
    let mut f: *mut FileProtocol = ptr::null_mut();
    // SAFETY: root is a non-null protocol handle returned by OpenVolume, and
    // wide is a NUL-terminated stack UTF-16 path valid for this call.
    let st = unsafe { ((*root).open)(root, &mut f, wide.as_ptr(), EFI_OPEN_FILE_READ, 0) };
    if efi_output_is_valid(st, f) {
        Some(f)
    } else {
        None
    }
}

enum ReadFileError {
    Missing,
    TooLarge,
}

/// Read a whole file from the ESP root into a pool buffer. A supplied maximum
/// is checked from EFI metadata before allocation or full-file materialization.
fn read_file(
    bt: *mut BootServices,
    root: *mut FileProtocol,
    name: &str,
    max_bytes: Option<usize>,
) -> Result<PoolBuf, ReadFileError> {
    let file = open_ro(root, name).ok_or(ReadFileError::Missing)?;
    // SAFETY: open_ro returns only a non-null live FileProtocol handle.
    unsafe {
        ((*file).set_position)(file, u64::MAX);
    }
    let mut size: u64 = 0;
    // SAFETY: file is the live handle opened above and size is writable local
    // storage required by GetPosition.
    unsafe {
        ((*file).get_position)(file, &mut size);
    }
    if size == 0 || size > usize::MAX as u64 {
        // SAFETY: file remains the live handle opened above until Close.
        unsafe {
            ((*file).close)(file);
        }
        return Err(ReadFileError::Missing);
    }
    if max_bytes.is_some_and(|max| size > max as u64) {
        // SAFETY: file remains the live handle opened above until Close.
        unsafe {
            ((*file).close)(file);
        }
        return Err(ReadFileError::TooLarge);
    }
    let size = usize::try_from(size).map_err(|_| ReadFileError::Missing)?;
    let buf = alloc_pool(bt, size).ok_or(ReadFileError::Missing)?;
    // SAFETY: file is still live and SetPosition only updates its cursor.
    unsafe {
        ((*file).set_position)(file, 0);
    }
    let mut rd = size;
    // SAFETY: file is live; PoolBuf owns exactly `size` writable pool bytes,
    // and rd points to writable local storage for the UEFI read count.
    let st = unsafe { ((*file).read)(file, &mut rd, buf.ptr as *mut c_void) };
    // SAFETY: file is live and no later operation uses it after this Close.
    unsafe {
        ((*file).close)(file);
    }
    if efi_error(st) || rd != size {
        return Err(ReadFileError::Missing);
    }
    Ok(buf)
}

// ---------------------------------------------------------------------------
// UEFI memory type -> TOS memory type (explicit table; unknown types never
// become usable — they fail closed to reserved)
// ---------------------------------------------------------------------------

fn tos_memory_type(efi_ty: u32) -> u32 {
    match efi_ty {
        MEM_TYPE_CONVENTIONAL
        | MEM_TYPE_LOADER_CODE
        | MEM_TYPE_LOADER_DATA
        | MEM_TYPE_BOOT_SERVICES_CODE
        | MEM_TYPE_BOOT_SERVICES_DATA => MEM_USABLE,
        MEM_TYPE_RUNTIME_SERVICES_CODE
        | MEM_TYPE_RUNTIME_SERVICES_DATA
        | MEM_TYPE_UNUSABLE
        | MEM_TYPE_PAL_CODE
        | MEM_TYPE_PERSISTENT => MEM_RESERVED,
        MEM_TYPE_ACPI_RECLAIM => MEM_ACPI_RECLAIM,
        MEM_TYPE_ACPI_NVS => MEM_ACPI_NVS,
        MEM_TYPE_MMIO | MEM_TYPE_MMIO_PORT => MEM_MMIO,
        // EfiReservedMemoryType (0) and vendor types: reserved.
        _ => MEM_RESERVED,
    }
}

/// Mark every descriptor overlapping `[start, start+len)` as reserved. Used to
/// pin the loader's own handoff structures (nucleus image, stack, capsule,
/// BootInfo, converted map) so the nucleus never treats them as reclaimable.
fn reserve_overlapping(ranges: &mut [MemoryRange], start: u64, len: u64) {
    if len == 0 {
        return;
    }
    let end = start + len; // callers pass validated physical ranges
    for r in ranges.iter_mut() {
        let r_end = r.phys_start + r.phys_length;
        if start < r_end && end > r.phys_start {
            r.ty = MEM_RESERVED;
        }
    }
}

// SAFETY (entry-point contract): `image_handle` and `sys_table` are supplied by
// the firmware per UEFI 2.10 §4.1 and are valid for the whole call; their
// validity is the firmware's half of the ABI, not something this image can
// check. The function cannot be an `unsafe fn`: firmware invokes it through the
// efiapi ABI, not through Rust, so `clippy::not_unsafe_ptr_arg_deref` does not
// apply to it. Every dereference below is null-checked where the UEFI spec
// permits a null field (`con_out`, `boot_services`).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[cfg(not(test))]
#[no_mangle]
pub extern "efiapi" fn efi_main(image_handle: *mut c_void, sys_table: *mut SystemTable) -> usize {
    tos_serial::init();
    tos_serial::puts(b"TOS.BOOT.ENTRY\r\n");

    // SAFETY: efi_main's documented firmware entry contract supplies a live
    // SystemTable; boot_services is checked for null before use.
    let bt = unsafe { (*sys_table).boot_services };
    if bt.is_null() {
        serial_fatal(b"TOS.BOOT.FAILI no-boot-services");
    }
    conout(sys_table, "TOS boot loader\r\n");
    // SAFETY: sys_table and the non-null bt came from the live firmware entry.
    let platform = match unsafe { collect_platform(sys_table, bt) } {
        Some(platform) => platform,
        None => serial_fatal(b"TOS.BOOT.FAILI platform"),
    };

    // --- our device handle via the Loaded Image protocol ---
    let mut loaded: *mut LoadedImageProtocol = ptr::null_mut();
    // SAFETY: bt is the live BootServices table; image_handle and the output
    // slot are supplied by the UEFI entry contract and this stack frame.
    let st = unsafe {
        ((*bt).handle_protocol)(
            image_handle,
            &GUID_LOADED_IMAGE,
            &mut loaded as *mut *mut LoadedImageProtocol as *mut *mut c_void,
        )
    };
    if !efi_output_is_valid(st, loaded) {
        serial_fatal(b"TOS.BOOT.FAILI no-loaded-image");
    }
    // SAFETY: successful HandleProtocol returned a non-null LoadedImageProtocol.
    let device = unsafe { (*loaded).device_handle };

    // --- file system on that device ---
    let mut fs: *mut SimpleFileSystemProtocol = ptr::null_mut();
    // SAFETY: bt is live and device came from the live LoadedImage protocol;
    // the output slot is writable local storage.
    let st = unsafe {
        ((*bt).handle_protocol)(
            device,
            &GUID_SIMPLE_FILE_SYSTEM,
            &mut fs as *mut *mut SimpleFileSystemProtocol as *mut *mut c_void,
        )
    };
    if !efi_output_is_valid(st, fs) {
        serial_fatal(b"TOS.BOOT.FAILI no-fs");
    }
    let mut root: *mut FileProtocol = ptr::null_mut();
    // SAFETY: successful HandleProtocol returned non-null fs and root is a
    // writable local output slot for OpenVolume.
    let st = unsafe { ((*fs).open_volume)(fs, &mut root) };
    if !efi_output_is_valid(st, root) {
        serial_fatal(b"TOS.BOOT.FAILI no-volume");
    }

    let capsule = match read_file(bt, root, "capsule.bin", Some(MAX_CAPSULE_BYTES)) {
        Ok(b) => b,
        Err(ReadFileError::TooLarge) => capsule_fatal(CapsError::CapsuleTooLarge),
        Err(ReadFileError::Missing) => serial_fatal(b"TOS.BOOT.FAILI no-capsule"),
    };
    let nucleus = match read_file(bt, root, "nucleus.bin", None) {
        Ok(b) => b,
        Err(_) => serial_fatal(b"TOS.BOOT.FAILI no-nucleus"),
    };
    // The ring-3 runtime image (ADR-0053 option B). It is optional here and
    // nowhere else: a machine that has none boots a nucleus that launches no
    // process and says so, which is a truthful boot. A machine that has one but
    // cannot read it is not the same case, and the difference is why absence is
    // `Missing` and nothing else.
    let runtime = match read_file(bt, root, "runtime.bin", None) {
        Ok(b) => Some(b),
        Err(ReadFileError::Missing) => None,
        Err(_) => serial_fatal(b"TOS.BOOT.FAILI runtime-unreadable"),
    };
    // SAFETY: root is the live FileProtocol handle returned by OpenVolume and
    // is not used after this Close.
    unsafe {
        ((*root).close)(root);
    }

    // --- capsule validation ---
    let cd = sha256(capsule.as_slice());
    let cap = match parse(capsule.as_slice()) {
        Ok(c) => c,
        Err(e) => capsule_fatal(e),
    };
    tos_serial::puts(b"TOS.CAPSULE.OK files=");
    tos_serial::put_u32_decimal(cap.file_count());
    tos_serial::puts(b"\r\n");

    // --- nucleus image: executable pages at the fixed link address ---
    // The nucleus is linked STATIC at NUCLEUS_BASE (see nucleus/linker.ld);
    // ALLOCATE_ANY_PAGES + PIC relocation is not viable for a flat image
    // (GOT slots are never filled), so the loader must get exactly
    // NUCLEUS_BASE. If the firmware cannot honour it, fail closed.
    let nucleus_pages = nucleus.len().div_ceil(0x1000);
    let mut nucleus_phys: u64 = NUCLEUS_BASE;
    // SAFETY: bt is live and nucleus_phys is writable local storage; the
    // fixed address/page count are checked before their raw copy below.
    let st = unsafe {
        ((*bt).allocate_pages)(
            ALLOCATE_ADDRESS,
            MEM_TYPE_LOADER_CODE,
            nucleus_pages,
            &mut nucleus_phys,
        )
    };
    if efi_error(st) || nucleus_phys != NUCLEUS_BASE {
        serial_fatal(b"TOS.BOOT.FAILI alloc-nucleus");
    }
    // SAFETY: AllocatePages returned the non-overlapping fixed destination;
    // PoolBuf owns nucleus.len() initialized source bytes from ReadFile.
    unsafe {
        ptr::copy_nonoverlapping(nucleus.ptr, nucleus_phys as *mut u8, nucleus.len());
    }

    // --- runtime image: reserved pages, digested where they are ---
    // Copied into its own allocation rather than left in the pool buffer,
    // because the pool is loader memory that the nucleus is free to reclaim,
    // and this is memory a process will be built out of. The digest is taken
    // from the copy: digesting the source and mapping the destination would
    // prove something about bytes that are no longer the ones in use.
    let mut runtime_phys: u64 = 0;
    let mut runtime_length: u64 = 0;
    let mut runtime_digest = [0u8; 32];
    if let Some(image) = &runtime {
        let pages = image.len().div_ceil(0x1000);
        // SAFETY: bt is live and runtime_phys is writable local storage for the
        // bounded page count computed from the file length just read.
        let st = unsafe {
            ((*bt).allocate_pages)(
                ALLOCATE_ANY_PAGES,
                MEM_TYPE_LOADER_DATA,
                pages,
                &mut runtime_phys,
            )
        };
        if efi_error(st) {
            serial_fatal(b"TOS.BOOT.FAILI alloc-runtime");
        }
        // SAFETY: AllocatePages returned a fresh non-overlapping range of at
        // least `image.len()` bytes; PoolBuf owns that many initialized source
        // bytes from ReadFile.
        unsafe {
            ptr::copy_nonoverlapping(image.ptr, runtime_phys as *mut u8, image.len());
        }
        // SAFETY: the destination range was just allocated and written above,
        // so it is mapped, initialized and owned by this loader.
        let copied = unsafe { core::slice::from_raw_parts(runtime_phys as *const u8, image.len()) };
        runtime_digest = sha256(copied);
        runtime_length = image.len() as u64;
    }

    // --- stack pages (non-executable) ---
    let mut stack_phys: u64 = 0;
    // SAFETY: bt is live and stack_phys is writable local storage for the
    // bounded fixed number of loader-data pages.
    let st = unsafe {
        ((*bt).allocate_pages)(
            ALLOCATE_ANY_PAGES,
            MEM_TYPE_LOADER_DATA,
            STACK_PAGES,
            &mut stack_phys,
        )
    };
    if efi_error(st) {
        serial_fatal(b"TOS.BOOT.FAILI alloc-stack");
    }

    // --- memory map: size probe (expect EFI_BUFFER_TOO_SMALL) ---
    let mut map_size: usize = 0;
    let mut map_key: usize = 0;
    let mut desc_size: usize = 0;
    let mut desc_ver: u32 = 0;
    // SAFETY: bt is live; the null descriptor buffer is the UEFI size-probe
    // form, while all remaining pointers refer to writable local storage.
    let st = unsafe {
        ((*bt).get_memory_map)(
            &mut map_size,
            ptr::null_mut(),
            &mut map_key,
            &mut desc_size,
            &mut desc_ver,
        )
    };
    if st != EFI_BUFFER_TOO_SMALL && efi_error(st) {
        serial_fatal(b"TOS.BOOT.FAILI memmap-probe");
    }
    if desc_size < core::mem::size_of::<EfiMemoryDescriptor>() || !desc_size.is_multiple_of(8) {
        // The firmware must report the EFI_MEMORY_DESCRIPTOR stride; refuse
        // anything smaller than the documented 40-byte layout.
        serial_fatal(b"TOS.BOOT.FAILI memmap-descsize");
    }
    // Slack so a map that grows between probe and fill still fits; also gives
    // the ExitBootServices retry loop headroom for one re-read.
    let desc_cap = match map_size.checked_add(0x8000) {
        Some(cap) => cap,
        None => serial_fatal(b"TOS.BOOT.FAILI memmap-overflow"),
    };
    let desc_buf = match alloc_pool(bt, desc_cap) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-map"),
    };
    let max_descs = match (desc_cap / desc_size).checked_add(8) {
        Some(count) => count,
        None => serial_fatal(b"TOS.BOOT.FAILI memmap-overflow"),
    };
    let range_cap = match max_descs.checked_mul(core::mem::size_of::<MemoryRange>()) {
        Some(bytes) => bytes,
        None => serial_fatal(b"TOS.BOOT.FAILI memmap-overflow"),
    };
    let range_buf = match alloc_pool(bt, range_cap) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-ranges"),
    };

    // --- BootInfo buffer ---
    let bi_buf = match alloc_pool(bt, core::mem::size_of::<BootInfo>()) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-bootinfo"),
    };

    // --- exit boot services with stale-key retry ---
    // Between GetMemoryMap and ExitBootServices the key can turn stale
    // (EFI_INVALID_PARAMETER). The loop re-reads the map and retries with the
    // fresh key, re-converting the TOS ranges from the latest descriptors.
    // Boot services are never called after a successful exit.
    let mut range_len: usize = 0;
    let mut exited = false;
    for _attempt in 0..3 {
        let mut got: usize = desc_cap;
        // SAFETY: bt is live and desc_buf owns desc_cap writable bytes; all
        // metadata outputs are writable locals supplied to GetMemoryMap.
        let st = unsafe {
            ((*bt).get_memory_map)(
                &mut got,
                desc_buf.ptr as *mut EfiMemoryDescriptor,
                &mut map_key,
                &mut desc_size,
                &mut desc_ver,
            )
        };
        if efi_error(st) {
            serial_fatal(b"TOS.BOOT.FAILI memmap-fill");
        }
        if got > desc_cap {
            serial_fatal(b"TOS.BOOT.FAILI memmap-toobig");
        }
        if !got.is_multiple_of(desc_size) {
            serial_fatal(b"TOS.BOOT.FAILI memmap-stride");
        }
        let n = got / desc_size;
        if n > max_descs {
            serial_fatal(b"TOS.BOOT.FAILI memmap-toomany");
        }

        // --- convert EFI descriptors to TOS MemoryRange[] ---
        let ranges = range_buf.ptr as *mut MemoryRange;
        let mut prev_end: u64 = 0;
        for i in 0..n {
            let offset = match i
                .checked_mul(desc_size)
                .and_then(|offset| (desc_buf.ptr as usize).checked_add(offset))
            {
                Some(offset) => offset,
                None => serial_fatal(b"TOS.BOOT.FAILI memmap-overflow"),
            };
            // SAFETY: UEFI returned got as a whole number of descriptor_size
            // entries; desc_size is at least the 40-byte repr(C) descriptor
            // and 8-byte aligned, while offset is checked within desc_buf.
            let md = unsafe { &*(offset as *const EfiMemoryDescriptor) };
            let start1 = md.physical_start;
            let len1 = match md.number_of_pages.checked_mul(0x1000) {
                Some(l) => l,
                None => serial_fatal(b"TOS.BOOT.FAILI map-overflow"),
            };
            let end1 = match start1.checked_add(len1) {
                Some(e) => e,
                None => serial_fatal(b"TOS.BOOT.FAILI map-overflow"),
            };
            if start1 < prev_end {
                serial_fatal(b"TOS.BOOT.FAILI unsorted-map");
            }
            // SAFETY: n <= max_descs and range_buf owns max_descs contiguous
            // MemoryRange slots, so ranges.add(i) is aligned and in bounds.
            unsafe {
                ranges.add(i).write(MemoryRange {
                    phys_start: start1,
                    phys_length: len1,
                    ty: tos_memory_type(md.ty),
                    flags: 0,
                });
            }
            prev_end = end1;
        }
        range_len = match n.checked_mul(core::mem::size_of::<MemoryRange>()) {
            Some(bytes) => bytes,
            None => serial_fatal(b"TOS.BOOT.FAILI memmap-overflow"),
        };

        // --- explicit reservations of loader handoff structures ---
        // (also pins firmware runtime regions via tos_memory_type above)
        // SAFETY: range_buf owns n initialized, aligned MemoryRange entries;
        // each reservation length was checked when allocated or GOP-validated.
        unsafe {
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                nucleus_phys,
                nucleus.len() as u64,
            );
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                stack_phys,
                (STACK_PAGES as u64) * 0x1000,
            );
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                capsule.ptr as u64,
                capsule.len() as u64,
            );
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                range_buf.ptr as u64,
                range_len as u64,
            );
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                bi_buf.ptr as u64,
                core::mem::size_of::<BootInfo>() as u64,
            );
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                platform.fb_phys,
                u64::from(platform.fb_pitch) * u64::from(platform.fb_height),
            );
            reserve_overlapping(
                core::slice::from_raw_parts_mut(ranges, n),
                runtime_phys,
                runtime_length,
            );
        }

        // SAFETY: bt and image_handle are still live UEFI entry values, and
        // map_key was returned by the immediately preceding GetMemoryMap.
        let st = unsafe { ((*bt).exit_boot_services)(image_handle, map_key) };
        if efi_success(st) {
            exited = true;
            break;
        }
        if st != EFI_INVALID_PARAMETER {
            serial_fatal(b"TOS.BOOT.FAILI exit-bs");
        }
        // stale map key: loop re-reads the map and retries
    }
    if !exited {
        serial_fatal(b"TOS.BOOT.FAILI exit-bs");
    }

    // --- BootInfo record (no boot services after this point) ---
    let mut bi = BootInfo::new();
    bi.framebuffer_phys = platform.fb_phys;
    bi.framebuffer_width = platform.fb_width;
    bi.framebuffer_height = platform.fb_height;
    bi.framebuffer_pitch = platform.fb_pitch;
    bi.framebuffer_format = platform.fb_format;
    bi.acpi_rsdp = platform.acpi_rsdp;
    bi.smbios = platform.smbios;
    bi.capsule_phys = capsule.ptr as u64;
    bi.capsule_length = capsule.len() as u64;
    bi.capsule_digest = cd;
    bi.capsule_identity_kind = cap.header().source_identity_kind;
    bi.capsule_oid_alg = cap.header().source_oid_alg;
    bi.capsule_oid_length = cap.header().source_oid_length;
    bi.capsule_source_identity = cap.header().source_identity_value;
    // Test-only negative-path injection. It is excluded from the default
    // artifact and built in target/test-corrupt-bootinfo/ by the QEMU scenario;
    // production loader semantics copy the header identity unchanged.
    #[cfg(feature = "test-corrupt-bootinfo-identity")]
    {
        bi.capsule_source_identity[0] ^= 0x01;
    }
    bi.runtime_phys = runtime_phys;
    bi.runtime_length = runtime_length;
    bi.runtime_digest = runtime_digest;
    bi.memory_map_phys = range_buf.ptr as u64;
    bi.memory_map_length = range_len as u64;
    bi.memory_desc_size = tos_boot_protocol::MEM_DESC_SIZE;
    // `next` stays 0: BOOT_ABI_V1.md reserves it for extension; the nucleus
    // entry address is not an ABI field (control transfers via the entry
    // call, not via BootInfo).
    // SAFETY: bi_buf is a successful native-aligned AllocatePool buffer whose
    // exact size is BootInfo; no alias to a BootInfo exists before this write.
    unsafe {
        (bi_buf.ptr as *mut BootInfo).write(bi);
    }
    let bi_phys = bi_buf.ptr as u64;

    // --- handoff ---
    let stack_top = match stack_phys.checked_add((STACK_PAGES as u64) * 0x1000) {
        Some(top) => top,
        None => serial_fatal(b"TOS.BOOT.FAILI stack-overflow"),
    };
    // SAFETY: Stage 1 is about to transfer control and has no interrupt policy;
    // disabling maskable interrupts prevents firmware handlers during handoff.
    unsafe {
        asm!("cli", options(nostack, preserves_flags));
    }
    // Boot-event log discipline (BOOT_ABI_V1 §7): every serial line is one
    // `TOS.<EVENT>` identifier followed by structured `key=value` fields. The
    // bring-up form of this trace was an ad-hoc `HO nuc=… stk=…` prefix with no
    // line terminator, which glued a non-conforming line onto the front of
    // TOS.BOOT.HANDOFF; the addresses it carried are kept here as fields of the
    // event they describe.
    tos_serial::puts(b"TOS.BOOT.HANDOFF nucleus=0x");
    tos_serial::put_hex64(nucleus_phys);
    tos_serial::puts(b" stack=0x");
    tos_serial::put_hex64(stack_top);
    tos_serial::puts(b" bootinfo=0x");
    tos_serial::put_hex64(bi_phys);
    tos_serial::puts(b" runtime=0x");
    tos_serial::put_hex64(runtime_phys);
    tos_serial::puts(b" fb_format=");
    tos_serial::put_u32_decimal(platform.fb_format);
    tos_serial::puts(b" fb_width=");
    tos_serial::put_u32_decimal(platform.fb_width);
    tos_serial::puts(b" fb_height=");
    tos_serial::put_u32_decimal(platform.fb_height);
    tos_serial::puts(b" fb_pitch=");
    tos_serial::put_u32_decimal(platform.fb_pitch);
    tos_serial::puts(b" acpi=");
    tos_serial::put_u32_decimal(u32::from(platform.acpi_version));
    tos_serial::puts(b" smbios=");
    tos_serial::put_u32_decimal(u32::from(platform.smbios_version));
    tos_serial::puts(b"\r\n");
    let entry = nucleus_phys as *const ();
    // SAFETY: AllocatePages established the fixed nucleus image at entry, the
    // stack page range is disjoint and bounded, and rdi names the initialized
    // reserved BootInfo record required by the Boot ABI v1 entry convention.
    unsafe {
        // Fixed registers: {stack} may take any caller-saved reg, but rdi
        // and rax are pinned so the entry pointer can never be clobbered by
        // the bi move. A bare `call {entry}` would let LLVM alias entry and
        // bi onto the same register and call the BootInfo address instead of
        // the nucleus (observed in QEMU: jump into UEFI garbage).
        asm!(
            "mov rsp, {stack}",
            "and rsp, -16",
            "call rax",
            stack = in(reg) stack_top,
            in("rax") entry,
            in("rdi") bi_phys,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checksum(bytes: &mut [u8], at: usize) {
        bytes[at] = 0;
        bytes[at] = 0u8.wrapping_sub(bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)));
    }

    fn rsdp(v2: bool) -> [u8; 36] {
        let mut bytes = [0u8; 36];
        bytes[..8].copy_from_slice(b"RSD PTR ");
        bytes[15] = if v2 { 2 } else { 0 };
        checksum(&mut bytes[..20], 8);
        if v2 {
            bytes[20..24].copy_from_slice(&36u32.to_le_bytes());
            checksum(&mut bytes, 32);
        }
        bytes
    }

    fn smbios3() -> [u8; 24] {
        let mut bytes = [0u8; 24];
        bytes[..5].copy_from_slice(b"_SM3_");
        bytes[6] = 24;
        checksum(&mut bytes, 5);
        bytes
    }

    fn smbios2() -> [u8; 31] {
        let mut bytes = [0u8; 31];
        bytes[..4].copy_from_slice(b"_SM_");
        bytes[5] = 31;
        bytes[16..21].copy_from_slice(b"_DMI_");
        checksum(&mut bytes[16..31], 5);
        checksum(&mut bytes, 4);
        bytes
    }

    fn system_table(entries: &mut [ConfigurationTable]) -> SystemTable {
        // SAFETY: test only reads the two fields initialized below.
        let mut st: SystemTable = unsafe { core::mem::zeroed() };
        st.number_of_table_entries = entries.len();
        st.configuration_table = entries.as_mut_ptr().cast();
        st
    }

    #[test]
    fn acpi_prefers_v2_and_does_not_fallback_when_preferred_is_malformed() {
        let mut v1 = rsdp(false);
        let mut v2 = rsdp(true);
        let mut entries = [
            ConfigurationTable {
                vendor_guid: GUID_ACPI_10,
                vendor_table: v1.as_mut_ptr().cast(),
            },
            ConfigurationTable {
                vendor_guid: GUID_ACPI_20,
                vendor_table: v2.as_mut_ptr().cast(),
            },
        ];
        let mut st = system_table(&mut entries);
        // SAFETY: system_table points only at the live local configuration
        // entries and v2 RSDP bytes initialized by this test fixture.
        let selected = unsafe { select_acpi(&mut st) };
        assert_eq!(selected, Some((v2.as_ptr() as u64, 2)));
        v2[0] = b'X';
        // SAFETY: the local fixture remains live; this call exercises the
        // malformed preferred ACPI entry rejection path.
        assert_eq!(unsafe { select_acpi(&mut st) }, None);
        entries[1].vendor_guid = Guid {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        };
        // SAFETY: both local RSDP buffers and the edited entry array remain
        // live; this call exercises the ACPI 1 fallback path.
        let selected = unsafe { select_acpi(&mut st) };
        assert_eq!(selected, Some((v1.as_ptr() as u64, 1)));
    }

    #[test]
    fn smbios_prefers_v3_and_falls_back_to_v2_only_when_absent() {
        let mut v2 = smbios2();
        let mut v3 = smbios3();
        let mut entries = [
            ConfigurationTable {
                vendor_guid: GUID_SMBIOS,
                vendor_table: v2.as_mut_ptr().cast(),
            },
            ConfigurationTable {
                vendor_guid: GUID_SMBIOS3,
                vendor_table: v3.as_mut_ptr().cast(),
            },
        ];
        let mut st = system_table(&mut entries);
        // SAFETY: system_table points only at the live local configuration
        // entries and v3 SMBIOS bytes initialized by this test fixture.
        let selected = unsafe { select_smbios(&mut st) };
        assert_eq!(selected, Some((v3.as_ptr() as u64, 3)));
        v3[0] = b'X';
        // SAFETY: the local fixture remains live; this call exercises the
        // malformed preferred SMBIOS entry rejection path.
        assert_eq!(unsafe { select_smbios(&mut st) }, None);
        entries[1].vendor_guid = Guid {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        };
        // SAFETY: both local SMBIOS buffers and the edited entry array remain
        // live; this call exercises the SMBIOS 2 fallback path.
        let selected = unsafe { select_smbios(&mut st) };
        assert_eq!(selected, Some((v2.as_ptr() as u64, 2)));
    }

    #[test]
    fn firmware_output_requires_success_and_non_null_pointer() {
        let mut value = 0u8;
        assert!(efi_output_is_valid(EFI_SUCCESS, &mut value));
        assert!(!efi_output_is_valid(EFI_NOT_FOUND, &mut value));
        assert!(!efi_output_is_valid(
            EFI_SUCCESS,
            core::ptr::null_mut::<u8>()
        ));
    }
}
