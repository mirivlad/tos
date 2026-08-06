// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal hand-written UEFI bindings (x86_64). Only the pieces Stage 1 needs:
//! console output, memory map, pool/page allocation, file system, loaded image,
//! and exit boot services. Struct field order follows the UEFI 2.10 spec;
//! unused slots are `*mut c_void` placeholders to keep offsets correct.
//!
//! On x86_64 the EFI calling convention is the SysV C ABI, so `extern "C"`
//! is correct here (the unstable `efiapi` ABI is not required).

#![allow(non_camel_case_types)]

use core::ffi::c_void;

pub type EfiStatus = usize;
pub const EFI_SUCCESS: EfiStatus = 0;
pub const EFI_BUFFER_TOO_SMALL: EfiStatus = 5;

pub const EFI_OPEN_FILE_READ: u64 = 0x1;

// EFI memory type codes (UEFI 2.10 section 7.2): the documented numeric values.
pub const MEM_TYPE_LOADER_CODE: u32 = 1;
pub const MEM_TYPE_LOADER_DATA: u32 = 2;
pub const MEM_TYPE_RUNTIME_SERVICES_DATA: u32 = 6;
pub const ALLOCATE_ANY_PAGES: u32 = 0;

#[repr(C)]
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

#[repr(C)]
pub struct EfiMemoryDescriptor {
    pub ty: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

#[repr(C)]
pub struct SimpleTextOutputProtocol {
    pub reset: extern "C" fn(*mut Self, bool) -> EfiStatus,
    pub output_string: extern "C" fn(*mut Self, *const u16) -> EfiStatus,
    pub test_string: extern "C" fn(*mut Self, *const u16) -> EfiStatus,
    pub query_mode: extern "C" fn(*mut Self, usize, *mut usize, *mut usize) -> EfiStatus,
    pub set_mode: extern "C" fn(*mut Self, usize) -> EfiStatus,
    pub set_attribute: extern "C" fn(*mut Self, usize) -> EfiStatus,
    pub clear_screen: extern "C" fn(*mut Self) -> EfiStatus,
    pub set_cursor_position: extern "C" fn(*mut Self, usize, usize) -> EfiStatus,
    pub enable_cursor: extern "C" fn(*mut Self, bool) -> EfiStatus,
    pub mode: *mut c_void,
}

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
    pub unload: *mut c_void,
}

#[repr(C)]
pub struct FileProtocol {
    pub revision: u64,
    pub open: extern "C" fn(*mut Self, *mut *mut Self, *const u16, u64, u64) -> EfiStatus,
    pub close: extern "C" fn(*mut Self) -> EfiStatus,
    pub delete: extern "C" fn(*mut Self) -> EfiStatus,
    pub read: extern "C" fn(*mut Self, *mut usize, *mut c_void) -> EfiStatus,
    pub write: extern "C" fn(*mut Self, *mut usize, *mut c_void) -> EfiStatus,
    pub get_position: extern "C" fn(*mut Self, *mut u64) -> EfiStatus,
    pub set_position: extern "C" fn(*mut Self, u64) -> EfiStatus,
    pub get_info: extern "C" fn(*mut Self, *const Guid, *mut c_void, *mut usize) -> EfiStatus,
    pub set_info: extern "C" fn(*mut Self, *const Guid, usize, *mut c_void) -> EfiStatus,
    pub flush: extern "C" fn(*mut Self) -> EfiStatus,
}

#[repr(C)]
pub struct SimpleFileSystemProtocol {
    pub revision: u64,
    pub open_volume: extern "C" fn(*mut Self, *mut *mut FileProtocol) -> EfiStatus,
}

pub type FnGetMemoryMap = extern "C" fn(
    *mut usize,
    *mut EfiMemoryDescriptor,
    *mut usize,
    *mut usize,
    *mut u32,
) -> EfiStatus;
pub type FnAllocatePages =
    extern "C" fn(u32, u32, usize, *mut u64) -> EfiStatus;
pub type FnAllocatePool = extern "C" fn(u32, usize, *mut *mut c_void) -> EfiStatus;
pub type FnFreePool = extern "C" fn(*mut c_void) -> EfiStatus;
pub type FnHandleProtocol =
    extern "C" fn(*mut c_void, *const Guid, *mut *mut c_void) -> EfiStatus;
pub type FnExitBootServices = extern "C" fn(*mut c_void, usize) -> EfiStatus;
pub type FnStall = extern "C" fn(usize) -> EfiStatus;

/// Boot services table (subset with full ordering; unused slots are placeholders).
#[repr(C)]
pub struct BootServices {
    pub raise_tpl: *mut c_void,
    pub restore_tpl: *mut c_void,
    pub allocate_pages: FnAllocatePages,
    pub free_pages: *mut c_void,
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
    pub locate_protocol: *mut c_void,
    pub install_multiple_protocol_interfaces: *mut c_void,
    pub uninstall_multiple_protocol_interfaces: *mut c_void,
    pub calculate_crc32: *mut c_void,
    pub copy_mem: *mut c_void,
    pub set_mem: *mut c_void,
    pub create_event_ex: *mut c_void,
}

#[repr(C)]
pub struct SystemTable {
    pub signature: u64,
    pub revision: u32,
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
