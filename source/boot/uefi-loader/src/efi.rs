// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal hand-written UEFI bindings (x86_64). Only the pieces Stage 1 needs:
//! console output, memory map, pool/page allocation, file system, loaded image,
//! and exit boot services. Field order and offsets follow UEFI 2.10.
//!
//! Calling convention: every firmware entry point and every protocol function
//! pointer uses `extern "efiapi"` (the EFI ABI, stable since Rust 1.68).
//! On x86_64-unknown-uefi this compiles to the MS x64 convention required by
//! the UEFI spec; `extern "C"` must not be used because it silently changes
//! meaning if the code is ever built for a non-UEFI target.
//!
//! Layout discipline: `EFI_TABLE_HEADER` is 24 bytes (Signature 8, Revision 4,
//! HeaderSize 4, CRC32 4, Reserved 4). The compile-time assertions at the
//! bottom pin every offset used by the loader; a struct that merely compiles
//! is not treated as correct.

#![allow(non_camel_case_types)]

use core::ffi::c_void;

// ---------------------------------------------------------------------------
// EFI_STATUS
// ---------------------------------------------------------------------------

pub type EfiStatus = usize;

pub const EFI_SUCCESS: EfiStatus = 0;
pub const EFI_INVALID_PARAMETER: EfiStatus = 0x8000000000000002;
pub const EFI_BUFFER_TOO_SMALL: EfiStatus = 0x8000000000000005;
// Full UEFI 2.10 status set is pinned here for ABI documentation; the loader
// only consumes SUCCESS / INVALID_PARAMETER / BUFFER_TOO_SMALL today.
#[allow(dead_code)]
pub const EFI_NOT_FOUND: EfiStatus = 0x800000000000000E;
#[allow(dead_code)]
pub const EFI_DEVICE_ERROR: EfiStatus = 0x8000000000000007;
#[allow(dead_code)]
pub const EFI_OUT_OF_RESOURCES: EfiStatus = 0x8000000000000009;

/// True when `code` has the EFI error bit (63) set. Warning codes (bits 30-31
/// set, bit 63 clear) are neither success nor error; `efi_success` covers the
/// success range explicitly.
#[inline]
pub const fn efi_error(code: EfiStatus) -> bool {
    code & (1usize << 63) != 0
}

#[inline]
pub const fn efi_success(code: EfiStatus) -> bool {
    code == EFI_SUCCESS
}

pub const EFI_OPEN_FILE_READ: u64 = 0x1;

// ---------------------------------------------------------------------------
// EFI memory types (UEFI 2.10 section 7.2)
// ---------------------------------------------------------------------------

pub const MEM_TYPE_LOADER_CODE: u32 = 1;
pub const MEM_TYPE_LOADER_DATA: u32 = 2;
pub const MEM_TYPE_BOOT_SERVICES_CODE: u32 = 3;
pub const MEM_TYPE_BOOT_SERVICES_DATA: u32 = 4;
pub const MEM_TYPE_RUNTIME_SERVICES_CODE: u32 = 5;
pub const MEM_TYPE_RUNTIME_SERVICES_DATA: u32 = 6;
pub const MEM_TYPE_CONVENTIONAL: u32 = 7;
pub const MEM_TYPE_UNUSABLE: u32 = 8;
pub const MEM_TYPE_ACPI_RECLAIM: u32 = 9;
pub const MEM_TYPE_ACPI_NVS: u32 = 10;
pub const MEM_TYPE_MMIO: u32 = 11;
pub const MEM_TYPE_MMIO_PORT: u32 = 12;
pub const MEM_TYPE_PAL_CODE: u32 = 13;
pub const MEM_TYPE_PERSISTENT: u32 = 14;
/// First vendor-defined memory type.
// Vendor-defined EFI memory types start at 0x8000_0000 (UEFI 2.10 §7.2);
// pinned for the vendor-range arm of tos_memory_type.
#[allow(dead_code)]
pub const MEM_TYPE_VENDOR_BASE: u32 = 0x8000_0000;

pub const ALLOCATE_ANY_PAGES: u32 = 0;
// AllocateAddress: the nucleus is linked STATIC at a fixed physical address
// (see NUCLEUS_BASE in main.rs / nucleus/linker.ld), so the loader must
// request exactly that address rather than ANY_PAGES.
pub const ALLOCATE_ADDRESS: u32 = 2;

// ---------------------------------------------------------------------------
// GUID and protocol GUIDs
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

pub const GUID_LOADED_IMAGE: Guid = Guid {
    data1: 0x5B1B31A1,
    data2: 0x9562,
    data3: 0x11d2,
    data4: [0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

pub const GUID_SIMPLE_FILE_SYSTEM: Guid = Guid {
    data1: 0x964E5B22,
    data2: 0x6459,
    data3: 0x11d2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

pub const GUID_GRAPHICS_OUTPUT: Guid = Guid {
    data1: 0x9042_A9DE,
    data2: 0x23DC,
    data3: 0x4A38,
    data4: [0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A],
};
pub const GUID_ACPI_20: Guid = Guid {
    data1: 0x8868_E871,
    data2: 0xE4F1,
    data3: 0x11D3,
    data4: [0xBC, 0x22, 0x00, 0x80, 0xC7, 0x3C, 0x88, 0x81],
};
pub const GUID_ACPI_10: Guid = Guid {
    data1: 0xEB9D_2D30,
    data2: 0x2D88,
    data3: 0x11D3,
    data4: [0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D],
};
pub const GUID_SMBIOS3: Guid = Guid {
    data1: 0xF2FD_1544,
    data2: 0x9794,
    data3: 0x4A2C,
    data4: [0x99, 0x2E, 0xE5, 0xBB, 0xCF, 0x20, 0xE3, 0x94],
};
pub const GUID_SMBIOS: Guid = Guid {
    data1: 0xEB9D_2D31,
    data2: 0x2D88,
    data3: 0x11D3,
    data4: [0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D],
};

pub const PIXEL_RGBX8: u32 = 0;
pub const PIXEL_BGRX8: u32 = 1;
pub const PIXEL_BIT_MASK: u32 = 2;
pub const PIXEL_BLT_ONLY: u32 = 3;

// ---------------------------------------------------------------------------
// EFI_TABLE_HEADER and EFI_MEMORY_DESCRIPTOR
// ---------------------------------------------------------------------------

/// `EFI_TABLE_HEADER` (UEFI 2.10 section 4.2): 24 bytes. Must be the first
/// field of `SystemTable` and `BootServices`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiTableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

/// `EFI_MEMORY_DESCRIPTOR` (UEFI 2.10 section 7.2): 40 bytes. Entry size is
/// reported by `GetMemoryMap` and may exceed 40 on future firmware; the loader
/// strides by the reported `descriptor_size`, never by `size_of::<Self>()`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiMemoryDescriptor {
    pub ty: u32,
    pub pad: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

// ---------------------------------------------------------------------------
// Protocols
// ---------------------------------------------------------------------------

/// `EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL` (UEFI 2.10 section 11.4): 10 slots.
#[repr(C)]
pub struct SimpleTextOutputProtocol {
    pub reset: extern "efiapi" fn(*mut Self, bool) -> EfiStatus,
    pub output_string: extern "efiapi" fn(*mut Self, *const u16) -> EfiStatus,
    pub test_string: extern "efiapi" fn(*mut Self, *const u16) -> EfiStatus,
    pub query_mode: extern "efiapi" fn(*mut Self, usize, *mut usize, *mut usize) -> EfiStatus,
    pub set_mode: extern "efiapi" fn(*mut Self, usize) -> EfiStatus,
    pub set_attribute: extern "efiapi" fn(*mut Self, usize) -> EfiStatus,
    pub clear_screen: extern "efiapi" fn(*mut Self) -> EfiStatus,
    pub set_cursor_position: extern "efiapi" fn(*mut Self, usize, usize) -> EfiStatus,
    pub enable_cursor: extern "efiapi" fn(*mut Self, bool) -> EfiStatus,
    pub mode: *mut c_void,
}

/// `EFI_LOADED_IMAGE_PROTOCOL` (UEFI 2.10 section 7.4): 12 slots.
#[repr(C)]
pub struct LoadedImageProtocol {
    pub revision: u32,
    pub parent_handle: *mut c_void,
    pub system_table: *mut c_void,
    pub device_handle: *mut c_void,
    pub file_path: *mut c_void,
    pub reserved: *mut c_void,
    pub load_options_size: u32,
    pub load_options: *mut c_void,
    pub image_base: *mut c_void,
    pub image_size: u64,
    pub image_code_type: u32,
    pub image_data_type: u32,
    pub unload: extern "efiapi" fn(*mut Self) -> EfiStatus,
}

/// `EFI_FILE_PROTOCOL` (UEFI 2.10 section 12.5): 11 slots.
#[repr(C)]
pub struct FileProtocol {
    pub revision: u64,
    pub open: extern "efiapi" fn(*mut Self, *mut *mut Self, *const u16, u64, u64) -> EfiStatus,
    pub close: extern "efiapi" fn(*mut Self) -> EfiStatus,
    pub delete: extern "efiapi" fn(*mut Self) -> EfiStatus,
    pub read: extern "efiapi" fn(*mut Self, *mut usize, *mut c_void) -> EfiStatus,
    pub write: extern "efiapi" fn(*mut Self, *mut usize, *mut c_void) -> EfiStatus,
    pub get_position: extern "efiapi" fn(*mut Self, *mut u64) -> EfiStatus,
    pub set_position: extern "efiapi" fn(*mut Self, u64) -> EfiStatus,
    pub get_info: extern "efiapi" fn(*mut Self, *const Guid, *mut c_void, *mut usize) -> EfiStatus,
    pub set_info: extern "efiapi" fn(*mut Self, *const Guid, usize, *mut c_void) -> EfiStatus,
    pub flush: extern "efiapi" fn(*mut Self) -> EfiStatus,
}

/// `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` (UEFI 2.10 section 12.4): 2 slots.
#[repr(C)]
pub struct SimpleFileSystemProtocol {
    pub revision: u64,
    pub open_volume: extern "efiapi" fn(*mut Self, *mut *mut FileProtocol) -> EfiStatus,
}

#[repr(C)]
pub struct GraphicsOutputProtocol {
    pub query_mode: *mut c_void,
    pub set_mode: *mut c_void,
    pub blt: *mut c_void,
    pub mode: *mut GraphicsOutputMode,
}

#[repr(C)]
pub struct GraphicsOutputMode {
    pub max_mode: u32,
    pub mode: u32,
    pub info: *const GraphicsOutputModeInfo,
    pub size_of_info: usize,
    pub framebuffer_base: u64,
    pub framebuffer_size: usize,
}

#[repr(C)]
pub struct GraphicsOutputModeInfo {
    pub version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    pub pixel_format: u32,
    pub pixel_information: [u32; 4],
    pub pixels_per_scan_line: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConfigurationTable {
    pub vendor_guid: Guid,
    pub vendor_table: *mut c_void,
}

// ---------------------------------------------------------------------------
// Boot services function types
// ---------------------------------------------------------------------------

pub type FnGetMemoryMap = extern "efiapi" fn(
    *mut usize,
    *mut EfiMemoryDescriptor,
    *mut usize,
    *mut usize,
    *mut u32,
) -> EfiStatus;
pub type FnAllocatePages = extern "efiapi" fn(u32, u32, usize, *mut u64) -> EfiStatus;
pub type FnAllocatePool = extern "efiapi" fn(u32, usize, *mut *mut c_void) -> EfiStatus;
pub type FnFreePool = extern "efiapi" fn(*mut c_void) -> EfiStatus;
pub type FnHandleProtocol =
    extern "efiapi" fn(*mut c_void, *const Guid, *mut *mut c_void) -> EfiStatus;
pub type FnLocateProtocol =
    extern "efiapi" fn(*const Guid, *mut c_void, *mut *mut c_void) -> EfiStatus;
pub type FnExitBootServices = extern "efiapi" fn(*mut c_void, usize) -> EfiStatus;
pub type FnStall = extern "efiapi" fn(usize) -> EfiStatus;

/// `EFI_BOOT_SERVICES` (UEFI 2.10 section 4.4): `EFI_TABLE_HEADER` followed by
/// 44 service pointers in the exact spec order. The layout is pinned by the
/// compile-time assertions below; every pointer slot is present so the offsets
/// of the consumed services (`handle_protocol` 0x98, `get_memory_map` 0x38,
/// `exit_boot_services` 0xE8) match the firmware table exactly. Note that
/// SetAttribute/ClearScreen/SetCursorPosition/EnableCursor belong to
/// `EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL`, NOT to boot services, and are not
/// present here.
#[repr(C)]
pub struct BootServices {
    pub header: EfiTableHeader,
    pub raise_tpl: extern "efiapi" fn(usize) -> EfiStatus,
    pub restore_tpl: extern "efiapi" fn(usize) -> EfiStatus,
    pub allocate_pages: FnAllocatePages,
    pub free_pages: extern "efiapi" fn(u32, usize, u64) -> EfiStatus,
    pub get_memory_map: FnGetMemoryMap,
    pub allocate_pool: FnAllocatePool,
    pub free_pool: FnFreePool,
    pub create_event: *mut c_void,
    pub set_timer: *mut c_void,
    pub wait_for_event: *mut c_void,
    pub signal_event: *mut c_void,
    pub close_event: *mut c_void,
    pub check_event: *mut c_void,
    pub install_protocol_interface: *mut c_void,
    pub reinstall_protocol_interface: *mut c_void,
    pub uninstall_protocol_interface: *mut c_void,
    pub handle_protocol: FnHandleProtocol,
    pub reserved: *mut c_void,
    pub register_protocol_notify: *mut c_void,
    pub locate_handle: *mut c_void,
    pub locate_device_path: *mut c_void,
    pub install_configuration_table: *mut c_void,
    pub load_image: *mut c_void,
    pub start_image: *mut c_void,
    pub exit: *mut c_void,
    pub unload_image: *mut c_void,
    pub exit_boot_services: FnExitBootServices,
    pub get_next_monotonic_count: *mut c_void,
    pub stall: FnStall,
    pub set_watchdog_timer: *mut c_void,
    pub connect_controller: *mut c_void,
    pub disconnect_controller: *mut c_void,
    pub open_protocol: *mut c_void,
    pub close_protocol: *mut c_void,
    pub open_protocol_information: *mut c_void,
    pub protocols_per_handle: *mut c_void,
    pub locate_handle_buffer: *mut c_void,
    pub locate_protocol: FnLocateProtocol,
    pub install_multiple_protocol_interfaces: *mut c_void,
    pub uninstall_multiple_protocol_interfaces: *mut c_void,
    pub calculate_crc32: *mut c_void,
    pub copy_mem: *mut c_void,
    pub set_mem: *mut c_void,
    pub create_event_ex: *mut c_void,
}

/// `EFI_SYSTEM_TABLE` (UEFI 2.10 section 4.3): `EFI_TABLE_HEADER` followed by
/// 11 pointer/scalar fields; 120 bytes total (observed `header_size = 0x78`
/// on OVMF 4M matches the spec exactly). `runtime_services` at 0x58,
/// `boot_services` at 0x60.
#[repr(C)]
pub struct SystemTable {
    pub header: EfiTableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    pub con_in_handle: *mut c_void,
    pub con_in: *mut c_void,
    pub con_out_handle: *mut c_void,
    pub con_out: *mut SimpleTextOutputProtocol,
    pub stderr_handle: *mut c_void,
    pub stderr: *mut c_void,
    pub runtime_services: *mut c_void,
    pub boot_services: *mut BootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: *mut c_void,
}

// ---------------------------------------------------------------------------
// Compile-time layout assertions (UEFI 2.10 offsets; see header docs)
// ---------------------------------------------------------------------------

const _: () = {
    use core::mem::{offset_of, size_of};

    // EFI_TABLE_HEADER is 24 bytes: Signature(8) Revision(4) HeaderSize(4)
    // CRC32(4) Reserved(4).
    assert!(size_of::<EfiTableHeader>() == 24);
    assert!(offset_of!(EfiTableHeader, crc32) == 16);

    // EFI_MEMORY_DESCRIPTOR: Type(4) pad(4) PhysicalStart(8) VirtualStart(8)
    // NumberOfPages(8) Attribute(8) = 40.
    assert!(size_of::<EfiMemoryDescriptor>() == 40);
    assert!(offset_of!(EfiMemoryDescriptor, physical_start) == 8);

    // SystemTable: header(24) FirmwareVendor(0x18) FirmwareRevision(0x20)
    // ConsoleInHandle(0x28) ConIn(0x30) ConsoleOutHandle(0x38) ConOut(0x40)
    // StandardErrorHandle(0x48) StdErr(0x50) RuntimeServices(0x58)
    // BootServices(0x60) NumberOfTableEntries(0x68) ConfigurationTable(0x70).
    assert!(size_of::<SystemTable>() == 120);
    assert!(offset_of!(SystemTable, header) == 0);
    assert!(offset_of!(SystemTable, firmware_vendor) == 0x18);
    assert!(offset_of!(SystemTable, con_out) == 0x40);
    assert!(offset_of!(SystemTable, runtime_services) == 0x58);
    assert!(offset_of!(SystemTable, boot_services) == 0x60);
    assert!(offset_of!(SystemTable, configuration_table) == 0x70);

    // BootServices: header(24) RaiseTPL(0x18) RestoreTPL(0x20)
    // AllocatePages(0x28) FreePages(0x30) GetMemoryMap(0x38) AllocatePool(0x40)
    // FreePool(0x48) CreateEvent(0x50) SetTimer(0x58) WaitForEvent(0x60)
    // SignalEvent(0x68) CloseEvent(0x70) CheckEvent(0x78)
    // InstallProtocolInterface(0x80) Reinstall(0x88) Uninstall(0x90)
    // HandleProtocol(0x98) Reserved(0xA0) RegisterProtocolNotify(0xA8)
    // LocateHandle(0xB0) LocateDevicePath(0xB8) InstallConfigurationTable(0xC0)
    // LoadImage(0xC8) StartImage(0xD0) Exit(0xD8) UnloadImage(0xE0)
    // ExitBootServices(0xE8) GetNextMonotonicCount(0xF0) Stall(0xF8)
    // SetWatchdogTimer(0x100) ConnectController(0x108)
    // DisconnectController(0x110) OpenProtocol(0x118) CloseProtocol(0x120)
    // OpenProtocolInformation(0x128) ProtocolsPerHandle(0x130)
    // LocateHandleBuffer(0x138) LocateProtocol(0x140)
    // InstallMultipleProtocolInterfaces(0x148)
    // UninstallMultipleProtocolInterfaces(0x150) CalculateCrc32(0x158)
    // CopyMem(0x160) SetMem(0x168) CreateEventEx(0x170); total 0x178.
    assert!(size_of::<BootServices>() == 0x178);
    assert!(offset_of!(BootServices, header) == 0);
    assert!(offset_of!(BootServices, allocate_pages) == 0x28);
    assert!(offset_of!(BootServices, get_memory_map) == 0x38);
    assert!(offset_of!(BootServices, handle_protocol) == 0x98);
    assert!(offset_of!(BootServices, register_protocol_notify) == 0xA8);
    assert!(offset_of!(BootServices, exit_boot_services) == 0xE8);
    assert!(offset_of!(BootServices, stall) == 0xF8);
    assert!(offset_of!(BootServices, set_watchdog_timer) == 0x100);
    assert!(offset_of!(BootServices, locate_protocol) == 0x140);
    assert!(offset_of!(BootServices, calculate_crc32) == 0x158);
    assert!(offset_of!(BootServices, create_event_ex) == 0x170);

    // Protocol layouts.
    assert!(size_of::<SimpleTextOutputProtocol>() == 80); // 9 fn + Mode
    assert!(offset_of!(SimpleTextOutputProtocol, output_string) == 8);
    assert!(size_of::<LoadedImageProtocol>() == 96); // 12 slots
    assert!(offset_of!(LoadedImageProtocol, device_handle) == 0x18);
    assert!(offset_of!(LoadedImageProtocol, image_base) == 0x40);
    assert!(size_of::<FileProtocol>() == 88); // Revision + 10 fn
    assert!(offset_of!(FileProtocol, read) == 0x20);
    assert!(size_of::<SimpleFileSystemProtocol>() == 16);
    assert!(offset_of!(SimpleFileSystemProtocol, open_volume) == 8);
};
