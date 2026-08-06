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

#![no_main]
#![no_std]

mod efi;

use core::arch::asm;
use core::ffi::c_void;
use core::panic::PanicInfo;
use core::ptr;

use efi::*;
use tos_boot_protocol::{
    BootInfo, MemoryRange, MEM_ACPI_NVS, MEM_ACPI_RECLAIM, MEM_MMIO, MEM_RESERVED, MEM_USABLE,
    RESULT_CAPSULE_INVALID, RESULT_PANIC,
};
use tos_capsule::{parse, CapsError};
use tos_hash::sha256;

const STACK_PAGES: usize = 8; // 32 KiB

/// A pool-allocated byte buffer. Never freed (we exit boot services).
struct PoolBuf {
    ptr: *mut u8,
    len: usize,
}

impl PoolBuf {
    fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    fn len(&self) -> usize {
        self.len
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    tos_serial::puts(b"TOS.PANIC loader\r\n");
    result_port(RESULT_PANIC)
}

/// Write an exit code to the QEMU isa-debug-exit port and stop.
fn result_port(code: u8) -> ! {
    unsafe {
        asm!(
            "out dx, al",
            in("al") code,
            in("dx") (tos_boot_protocol::RESULT_PORT as u16),
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
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

/// SimpleTextOutput console message (ASCII -> UTF-16). Diagnostic only; a
/// console failure is non-fatal (serial remains the authoritative channel).
fn conout(st: *mut SystemTable, s: &str) {
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
    unsafe {
        ((*out).output_string)(out, buf.as_ptr());
    }
}

fn error_tag(e: CapsError) -> &'static [u8] {
    match e {
        CapsError::InputTooShort => b"InputTooShort",
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
        CapsError::BadFileFlags => b"BadFileFlags",
        CapsError::NonZeroReservedEntry => b"NonZeroReservedEntry",
        CapsError::UnsortedFileTable => b"UnsortedFileTable",
        CapsError::PayloadOverlap => b"PayloadOverlap",
        CapsError::PayloadGap => b"PayloadGap",
        CapsError::ZeroFileCount => b"ZeroFileCount",
        CapsError::BadDigest => b"BadDigest",
        CapsError::BadWholeDigest => b"BadWholeDigest",
        CapsError::UnsupportedIdentityKind => b"UnsupportedIdentityKind",
        CapsError::LicenceOutOfBounds => b"LicenceOutOfBounds",
        CapsError::MissingBootCanonical => b"MissingBootCanonical",
        CapsError::DuplicateBootCanonical => b"DuplicateBootCanonical",
        CapsError::BadBootCanonicalName => b"BadBootCanonicalName",
        CapsError::DuplicateFileIndex => b"DuplicateFileIndex",
        CapsError::UnreferencedFile => b"UnreferencedFile",
        CapsError::BootCanonicalFlagMismatch => b"BootCanonicalFlagMismatch",
        CapsError::BootCanonicalOnWrongFile => b"BootCanonicalOnWrongFile",
        CapsError::LicenceTailMismatch => b"LicenceTailMismatch",
    }
}

/// Allocate a pool buffer (RuntimeServicesData: survives ExitBootServices).
fn alloc_pool(bt: *mut BootServices, size: usize) -> Option<PoolBuf> {
    let mut p: *mut c_void = ptr::null_mut();
    let st = unsafe { ((*bt).allocate_pool)(MEM_TYPE_RUNTIME_SERVICES_DATA, size, &mut p) };
    if efi_error(st) {
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
    let st = unsafe { ((*root).open)(root, &mut f, wide.as_ptr(), EFI_OPEN_FILE_READ, 0) };
    if efi_success(st) {
        Some(f)
    } else {
        None
    }
}

/// Read a whole file from the ESP root into a pool buffer.
fn read_file(bt: *mut BootServices, root: *mut FileProtocol, name: &str) -> Option<PoolBuf> {
    let file = open_ro(root, name)?;
    unsafe {
        ((*file).set_position)(file, u64::MAX);
    }
    let mut size: u64 = 0;
    unsafe {
        ((*file).get_position)(file, &mut size);
    }
    if size == 0 || size > usize::MAX as u64 {
        unsafe {
            ((*file).close)(file);
        }
        return None;
    }
    let buf = alloc_pool(bt, size as usize)?;
    unsafe {
        ((*file).set_position)(file, 0);
    }
    let mut rd: usize = size as usize;
    let st = unsafe { ((*file).read)(file, &mut rd, buf.ptr as *mut c_void) };
    unsafe {
        ((*file).close)(file);
    }
    if efi_error(st) || rd != size as usize {
        return None;
    }
    Some(buf)
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
        MEM_TYPE_RUNTIME_SERVICES_CODE | MEM_TYPE_RUNTIME_SERVICES_DATA | MEM_TYPE_UNUSABLE
        | MEM_TYPE_PAL_CODE | MEM_TYPE_PERSISTENT => MEM_RESERVED,
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

#[no_mangle]
pub extern "efiapi" fn efi_main(image_handle: *mut c_void, sys_table: *mut SystemTable) -> usize {
    tos_serial::init();
    tos_serial::puts(b"TOS.BOOT.ENTRY\r\n");

    let bt = unsafe { (*sys_table).boot_services };
    if bt.is_null() {
        serial_fatal(b"TOS.BOOT.FAILI no-boot-services");
    }
    conout(sys_table, "TOS boot loader\r\n");

    // --- our device handle via the Loaded Image protocol ---
    let mut loaded: *mut LoadedImageProtocol = ptr::null_mut();
    let st = unsafe {
        ((*bt).handle_protocol)(
            image_handle,
            &GUID_LOADED_IMAGE,
            &mut loaded as *mut *mut LoadedImageProtocol as *mut *mut c_void,
        )
    };
    if efi_error(st) {
        serial_fatal(b"TOS.BOOT.FAILI no-loaded-image");
    }
    let device = unsafe { (*loaded).device_handle };

    // --- file system on that device ---
    let mut fs: *mut SimpleFileSystemProtocol = ptr::null_mut();
    let st = unsafe {
        ((*bt).handle_protocol)(
            device,
            &GUID_SIMPLE_FILE_SYSTEM,
            &mut fs as *mut *mut SimpleFileSystemProtocol as *mut *mut c_void,
        )
    };
    if efi_error(st) {
        serial_fatal(b"TOS.BOOT.FAILI no-fs");
    }
    let mut root: *mut FileProtocol = ptr::null_mut();
    let st = unsafe { ((*fs).open_volume)(fs, &mut root) };
    if efi_error(st) {
        serial_fatal(b"TOS.BOOT.FAILI no-volume");
    }

    let capsule = match read_file(bt, root, "capsule.bin") {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI no-capsule"),
    };
    let nucleus = match read_file(bt, root, "nucleus.bin") {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI no-nucleus"),
    };
    unsafe {
        ((*root).close)(root);
    }

    // --- capsule validation ---
    let cd = sha256(capsule.as_slice());
    let cap = match parse(capsule.as_slice()) {
        Ok(c) => c,
        Err(e) => {
            tos_serial::puts(b"TOS.BOOT.FAILC capsule_err=");
            tos_serial::puts(error_tag(e));
            tos_serial::puts(b"\r\n");
            result_port(RESULT_CAPSULE_INVALID)
        }
    };
    tos_serial::puts(b"TOS.CAPSULE.OK files=");
    tos_serial::put_u32_decimal(cap.file_count());
    tos_serial::puts(b"\r\n");

    // --- nucleus image: executable pages ---
    let nucleus_pages = (nucleus.len() + 0xfff) / 0x1000;
    let mut nucleus_phys: u64 = 0;
    let st = unsafe {
        ((*bt).allocate_pages)(
            ALLOCATE_ANY_PAGES,
            MEM_TYPE_LOADER_CODE,
            nucleus_pages,
            &mut nucleus_phys,
        )
    };
    if efi_error(st) {
        serial_fatal(b"TOS.BOOT.FAILI alloc-nucleus");
    }
    unsafe {
        ptr::copy_nonoverlapping(nucleus.ptr, nucleus_phys as *mut u8, nucleus.len());
    }

    // --- stack pages (non-executable) ---
    let mut stack_phys: u64 = 0;
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
    if desc_size < core::mem::size_of::<EfiMemoryDescriptor>() || desc_size % 8 != 0 {
        // The firmware must report the EFI_MEMORY_DESCRIPTOR stride; refuse
        // anything smaller than the documented 40-byte layout.
        serial_fatal(b"TOS.BOOT.FAILI memmap-descsize");
    }
    // Slack so a map that grows between probe and fill still fits; also gives
    // the ExitBootServices retry loop headroom for one re-read.
    let desc_cap = map_size + 0x8000;
    let desc_buf = match alloc_pool(bt, desc_cap) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-map"),
    };
    let max_descs = desc_cap / desc_size + 8;
    let range_cap = max_descs * core::mem::size_of::<MemoryRange>();
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
        let n = got / desc_size;
        if n > max_descs {
            serial_fatal(b"TOS.BOOT.FAILI memmap-toomany");
        }

        // --- convert EFI descriptors to TOS MemoryRange[] ---
        let ranges = range_buf.ptr as *mut MemoryRange;
        let mut prev_end: u64 = 0;
        for i in 0..n {
            let md = unsafe { &*((desc_buf.ptr as usize + i * desc_size) as *const EfiMemoryDescriptor) };
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
        range_len = n * core::mem::size_of::<MemoryRange>();

        // --- explicit reservations of loader handoff structures ---
        // (also pins firmware runtime regions via tos_memory_type above)
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
        }

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
    bi.capsule_phys = capsule.ptr as u64;
    bi.capsule_length = capsule.len() as u64;
    bi.capsule_digest = cd;
    bi.capsule_identity_kind = cap.header().source_identity_kind;
    bi.capsule_source_identity = cap.header().source_identity_digest;
    bi.memory_map_phys = range_buf.ptr as u64;
    bi.memory_map_length = range_len as u64;
    bi.memory_desc_size = tos_boot_protocol::MEM_DESC_SIZE;
    // `next` stays 0: BOOT_ABI_V1.md reserves it for extension; the nucleus
    // entry address is not an ABI field (control transfers via the entry
    // call, not via BootInfo).
    unsafe {
        (bi_buf.ptr as *mut BootInfo).write(bi);
    }
    let bi_phys = bi_buf.ptr as u64;

    // --- handoff ---
    let stack_top = stack_phys + (STACK_PAGES as u64 * 0x1000);
    unsafe {
        asm!("cli", options(nostack, preserves_flags));
    }
    tos_serial::puts(b"HO nuc=");
    tos_serial::put_hex64(nucleus_phys);
    tos_serial::puts(b" stk=");
    tos_serial::put_hex64(stack_top);
    tos_serial::puts(b"TOS.BOOT.HANDOFF\r\n");
    let entry = nucleus_phys as *const ();
    unsafe {
        asm!(
            "mov rsp, {stack}",
            "and rsp, -16",
            "mov rdi, {bi}",
            "call {entry}",
            stack = in(reg) stack_top,
            bi = in(reg) bi_phys,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
}
