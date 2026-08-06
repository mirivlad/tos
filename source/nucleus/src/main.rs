// SPDX-License-Identifier: GPL-3.0-or-later
//! TOS Stage 1 nucleus (freestanding, x86_64).
//!
//! Entry convention (boot ABI v1): `rdi` = physical address of a
//! [`BootInfo`]. The nucleus validates the ABI record and the capsule, emits
//! the serial boot-event log (BOOT_ABI_V1.md §6), then halts via the QEMU
//! isa-debug-exit port with a stable result code.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

use tos_boot_protocol::{
    BootInfo, MemoryRange, RESULT_ABI_INVALID, RESULT_CAPSULE_INVALID, RESULT_HALT_OK,
    RESULT_MEMORY_INVALID, RESULT_PANIC, RESULT_PORT,
};
use tos_capsule::parse;
use tos_hash::{Sha256, sha256};
use tos_serial;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    tos_serial::puts(b"TOS.PANIC nucleus\r\n");
    result_port(RESULT_PANIC)
}

/// Write an exit code to the QEMU isa-debug-exit port and stop.
fn result_port(code: u8) -> ! {
    unsafe {
        asm!(
            "out dx, al",
            in("al") code,
            in("dx") (RESULT_PORT as u16),
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn abi_fail() -> ! {
    tos_serial::puts(b"TOS.ABI.FAIL\r\n");
    result_port(RESULT_ABI_INVALID)
}

fn mem_fail() -> ! {
    tos_serial::puts(b"TOS.MEM.FAIL\r\n");
    result_port(RESULT_MEMORY_INVALID)
}

fn cap_fail() -> ! {
    tos_serial::puts(b"TOS.CAPSULE.FAIL\r\n");
    result_port(RESULT_CAPSULE_INVALID)
}

/// First logical line of boot text: the first line that is non-empty and not
/// a comment (`#` after leading whitespace). Returns the trimmed line.
fn first_logical_line(content: &[u8]) -> Option<&[u8]> {
    for raw in content.split(|&b| b == b'\n') {
        let mut line = raw;
        while let Some((&b, rest)) = line.split_first() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                line = rest;
            } else {
                break;
            }
        }
        if line.is_empty() {
            continue;
        }
        if line[0] == b'#' {
            continue;
        }
        return Some(line);
    }
    None
}

#[no_mangle]
#[link_section = ".text.boot_entry"]
pub extern "C" fn boot_entry(bi_raw: *const BootInfo) -> ! {
    tos_serial::init();
    tos_serial::puts(b"TOS.NUCLEUS.ENTRY\r\n");

    // --- 1. validate the boot ABI record over raw bytes ---
    let bi_bytes = unsafe { core::slice::from_raw_parts(bi_raw as *const u8, 224) };
    if BootInfo::validate_bytes(bi_bytes).is_err() {
        abi_fail();
    }
    let bi = unsafe { &*bi_raw };

    // --- 2. memory map ---
    if bi.memory_map_length == 0 || bi.memory_map_length % 24 != 0 {
        mem_fail();
    }
    let desc_count = (bi.memory_map_length / 24) as usize;
    let descs: &[MemoryRange] =
        unsafe { core::slice::from_raw_parts(bi.memory_map_phys as *const MemoryRange, desc_count) };
    if bi.check_memory_map(descs).is_err() {
        mem_fail();
    }
    if bi.check_capsule_in_memory(descs).is_err() {
        mem_fail();
    }

    // --- 3. capsule: plain digest, then full structural validation ---
    if bi.capsule_phys == 0
        || bi.capsule_length == 0
        || bi.capsule_length > usize::MAX as u64
        || bi.capsule_length > bi.memory_map_length
    {
        cap_fail();
    }
    let cap_bytes =
        unsafe { core::slice::from_raw_parts(bi.capsule_phys as *const u8, bi.capsule_length as usize) };
    if sha256(cap_bytes) != bi.capsule_digest {
        cap_fail();
    }
    let cap = match parse(cap_bytes) {
        Ok(c) => c,
        Err(_) => cap_fail(),
    };

    tos_serial::puts(b"TOS.CAPSULE.OK files=");
    tos_serial::put_u32_decimal(cap.file_count());
    tos_serial::puts(b"\r\n");

    // --- 4. canonical boot text ---
    let boot = match cap.boot_file() {
        Some(f) => f,
        None => cap_fail(),
    };
    tos_serial::puts(b"TOS.BOOTTEXT.PATH ");
    tos_serial::puts(boot.name);
    tos_serial::puts(b"\r\n");
    if let Some(line) = first_logical_line(boot.content) {
        tos_serial::puts(b"TOS.BOOTTEXT.LINE ");
        tos_serial::puts(line);
        tos_serial::puts(b"\r\n");
    }
    let mut h = Sha256::new();
    h.update(boot.content);
    let boot_digest = h.finalize();
    tos_serial::puts(b"TOS.BOOTTEXT.DIGEST ");
    tos_serial::put_hex32(&boot_digest);
    tos_serial::puts(b"\r\n");

    // --- 5. identity record ---
    tos_serial::puts(b"TOS.IDENTITY source_kind=detached source_digest=");
    tos_serial::put_hex32(&bi.capsule_source_identity);
    tos_serial::puts(b" capsule_digest=");
    tos_serial::put_hex32(&bi.capsule_digest);
    tos_serial::puts(b" arch=0.2.1 builder=1\r\n");

    // --- 6. halt with success code ---
    tos_serial::puts(b"TOS.HALT ok=0x10\r\n");
    result_port(RESULT_HALT_OK)
}
