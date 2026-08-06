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

#![no_main]
#![no_std]

mod efi;

use core::arch::asm;
use core::ffi::c_void;
use core::panic::PanicInfo;
use core::ptr;

use efi::*;
use tos_boot_protocol::{BootInfo, MemoryRange, RESULT_CAPSULE_INVALID, RESULT_PANIC};
use tos_capsule::{CapsError, Capsule};
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
            "mov dx, {port}",
            "mov al, {code}",
            "out dx, al",
            port = in(reg) tos_boot_protocol::RESULT_PORT as u16,
            code = in(reg_byte) code,
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

/// SimpleTextOutput console message (ASCII -> UTF-16).
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
        (out.output_string)(out, buf.as_ptr());
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
    }
}

/// Allocate a pool buffer (RuntimeServicesData: survives ExitBootServices).
fn alloc_pool(bt: *mut BootServices, size: usize) -> Option<PoolBuf> {
    let mut p: *mut c_void = ptr::null_mut();
    let st = unsafe { (bt.allocate_pool)(MEM_TYPE_RUNTIME_SERVICES_DATA, size, &mut p) };
    if st != EFI_SUCCESS {
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
    for b in name.as_bytes() {
        if n >= wide.len() - 1 {
            return None;
        }
        wide[n] = b as u16;
        n += 1;
    }
    wide[n] = 0;
    let mut f: *mut FileProtocol = ptr::null_mut();
    let st = unsafe { (root.open)(root, &mut f, wide.as_ptr(), EfiOpenFileRead, 0) };
    if st == EFI_SUCCESS {
        Some(f)
    } else {
        None
    }
}

/// Read a whole file from the ESP root into a pool buffer.
fn read_file(bt: *mut BootServices, root: *mut FileProtocol, name: &str) -> Option<PoolBuf> {
    let file = open_ro(root, name)?;
    unsafe {
        (file.set_position)(file, u64::MAX);
    }
    let mut size: u64 = 0;
    unsafe {
        (file.get_position)(file, &mut size);
    }
    if size == 0 {
        unsafe {
            (file.close)(file);
        }
        return None;
    }
    let buf = alloc_pool(bt, size as usize)?;
    unsafe {
        (file.set_position)(file, 0);
    }
    let mut rd: usize = size as usize;
    let st = unsafe { (file.read)(file, &mut rd, buf.ptr as *mut c_void) };
    unsafe {
        (file.close)(file);
    }
    if st != EFI_SUCCESS || rd != size as usize {
        return None;
    }
    Some(buf)
}

#[no_mangle]
pub extern "C" fn efi_main(image_handle: *mut c_void, sys_table: *mut SystemTable) -> usize {
    tos_serial::init();
    tos_serial::puts(b"TOS.BOOT.ENTRY\r\n");
    conout(sys_table, "TOS boot loader\r\n");

    let bt = unsafe { (*sys_table).boot_services };

    // --- our device handle via the Loaded Image protocol ---
    let mut loaded: *mut LoadedImageProtocol = ptr::null_mut();
    let st = unsafe {
        (bt.handle_protocol)(
            image_handle,
            &GUID_LOADED_IMAGE,
            &mut loaded as *mut *mut LoadedImageProtocol as *mut *mut c_void,
        )
    };
    if st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI no-loaded-image");
    }
    let device = unsafe { (*loaded).device_handle };

    // --- file system on that device ---
    let mut fs: *mut SimpleFileSystemProtocol = ptr::null_mut();
    let st = unsafe {
        (bt.handle_protocol)(
            device,
            &GUID_SIMPLE_FILE_SYSTEM,
            &mut fs as *mut *mut SimpleFileSystemProtocol as *mut *mut c_void,
        )
    };
    if st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI no-fs");
    }
    let mut root: *mut FileProtocol = ptr::null_mut();
    let st = unsafe { (fs.open_volume)(fs, &mut root) };
    if st != EFI_SUCCESS {
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
        (root.close)(root);
    }

    // --- capsule validation ---
    let cd = sha256(capsule.as_slice());
    tos_serial::puts(b"TOS.BOOT.LOADED cap=");
    tos_serial::put_hex32(&cd);
    tos_serial::puts(b"\r\n");

    let cap = match Capsule::parse(capsule.as_slice()) {
        Ok(c) => c,
        Err(e) => {
            tos_serial::puts(b"TOS.BOOT.FAILC capsule_err=");
            tos_serial::puts(error_tag(e));
            tos_serial::puts(b"\r\n");
            result_port(RESULT_CAPSULE_INVALID)
        }
    };

    // --- nucleus image: executable pages ---
    let nucleus_pages = (nucleus.len() + 0xfff) / 0x1000;
    let mut nucleus_phys: u64 = 0;
    let st = unsafe {
        (bt.allocate_pages)(
            ALLOCATE_ANY_PAGES,
            MEM_TYPE_LOADER_CODE,
            nucleus_pages,
            &mut nucleus_phys,
        )
    };
    if st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI alloc-nucleus");
    }
    unsafe {
        ptr::copy_nonoverlapping(nucleus.ptr, nucleus_phys as *mut u8, nucleus.len());
    }

    // --- stack pages (non-executable) ---
    let mut stack_phys: u64 = 0;
    let st = unsafe {
        (bt.allocate_pages)(
            ALLOCATE_ANY_PAGES,
            MEM_TYPE_LOADER_DATA,
            STACK_PAGES,
            &mut stack_phys,
        )
    };
    if st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI alloc-stack");
    }

    // --- memory map: size probe ---
    let mut map_size: usize = 0;
    let mut map_key: usize = 0;
    let mut desc_size: usize = 0;
    let mut desc_ver: u32 = 0;
    let st = unsafe {
        (bt.get_memory_map)(
            &mut map_size,
            ptr::null_mut(),
            &mut map_key,
            &mut desc_size,
            &mut desc_ver,
        )
    };
    if st != EFI_BUFFER_TOO_SMALL && st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI memmap-probe");
    }
    let desc_cap = map_size + 0x8000;
    let desc_buf = match alloc_pool(bt, desc_cap) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-map"),
    };
    let range_cap = (desc_cap / desc_size + 1) * core::mem::size_of::<MemoryRange>();
    let range_buf = match alloc_pool(bt, range_cap) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-ranges"),
    };

    // --- memory map: fill (final key) ---
    let mut got: usize = desc_cap;
    let st = unsafe {
        (bt.get_memory_map)(
            &mut got,
            desc_buf.ptr as *mut EfiMemoryDescriptor,
            &mut map_key,
            &mut desc_size,
            &mut desc_ver,
        )
    };
    if st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI memmap-fill");
    }

    // --- convert EFI descriptors to TOS MemoryRange[] ---
    let n = got / desc_size;
    let ranges = range_buf.ptr as *mut MemoryRange;
    let mut prev_end: u64 = 0;
    for i in 0..n {
        let md = unsafe { &*((desc_buf.ptr as usize + i * desc_size) as *const EfiMemoryDescriptor) };
        let start = md.physical_start;
        let len = md.number_of_pages * 0x1000;
        if start < prev_end {
            serial_fatal(b"TOS.BOOT.FAILI unsorted-map");
        }
        unsafe {
            ranges.add(i).write(MemoryRange {
                phys_start: start,
                phys_length: len,
                ty: md.ty,
                flags: 0,
            });
        }
        prev_end = start + len;
    }
    let range_len = n * core::mem::size_of::<MemoryRange>();

    // --- BootInfo record ---
    let bi_buf = match alloc_pool(bt, core::mem::size_of::<BootInfo>()) {
        Some(b) => b,
        None => serial_fatal(b"TOS.BOOT.FAILI alloc-bootinfo"),
    };
    let mut bi = BootInfo::new();
    bi.capsule_phys = capsule.ptr as u64;
    bi.capsule_length = capsule.len as u64;
    bi.capsule_digest = cd;
    bi.capsule_identity_kind = cap.header().source_identity_kind;
    bi.capsule_source_identity = cap.header().source_identity_digest;
    bi.memory_map_phys = range_buf.ptr as u64;
    bi.memory_map_length = range_len as u64;
    bi.memory_desc_size = core::mem::size_of::<MemoryRange>() as u64;
    bi.next = nucleus_phys;
    unsafe {
        (bi_buf.ptr as *mut BootInfo).write(bi);
    }
    let bi_phys = bi_buf.ptr as u64;

    // --- exit boot services (key from the final GetMemoryMap) ---
    let st = unsafe { (bt.exit_boot_services)(image_handle, map_key) };
    if st != EFI_SUCCESS {
        serial_fatal(b"TOS.BOOT.FAILI exit-bs");
    }

    // --- handoff ---
    unsafe {
        asm!("cli", options(nostack, preserves_flags));
    }
    tos_serial::puts(b"TOS.BOOT.HANDOFF\r\n");
    let entry = nucleus_phys as *const ();
    let stack_top = stack_phys + (STACK_PAGES as u64 * 0x1000);
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
    unreachable!()
}
