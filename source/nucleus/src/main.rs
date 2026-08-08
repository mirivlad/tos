// SPDX-License-Identifier: GPL-3.0-or-later
//! TOS Stage 1 nucleus (freestanding, x86_64).
//!
//! Entry convention (boot ABI v1): `rdi` = physical address of a
//! [`BootInfo`]. The nucleus validates the ABI record and the capsule, emits
//! the serial boot-event log (BOOT_ABI_V1.md §6), then halts via the QEMU
//! isa-debug-exit port with a stable result code.

#![no_std]
#![no_main]

mod exception;
mod framebuffer;

use core::arch::asm;
use core::panic::PanicInfo;

use tos_boot_protocol::{
    BootInfo, MemoryRange, RESULT_ABI_INVALID, RESULT_CAPSULE_INVALID, RESULT_HALT_OK,
    RESULT_MEMORY_INVALID, RESULT_PANIC, RESULT_PORT, SRC_KIND_DETACHED, SRC_KIND_GIT,
};
use tos_capsule::parse;
use tos_hash::{sha256, Sha256};

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
            in("dx") RESULT_PORT,
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

// SAFETY (entry-point contract): `bi_raw` is the physical address of a BootInfo
// record placed in `rdi` by the loader per BOOT_ABI_V1 §3, in an identity-mapped
// region the loader marked reserved. It is validated as raw bytes
// (`BootInfo::validate_bytes`) before it is ever read as a struct. The function
// cannot be an `unsafe fn`: the loader transfers control to it with a machine
// `call`, not a Rust call, so `clippy::not_unsafe_ptr_arg_deref` does not apply.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
#[link_section = ".text.boot_entry"]
pub extern "C" fn boot_entry(bi_raw: *const BootInfo) -> ! {
    tos_serial::init();
    tos_serial::puts(b"TOS.NUCLEUS.ENTRY\r\n");

    // Install nucleus-owned exception containment before dereferencing or
    // trusting any loader-provided memory. The loader disabled maskable
    // interrupts before handoff; Stage 1 intentionally leaves them disabled.
    unsafe { exception::install() };

    #[cfg(any(feature = "test-exception-ud2", feature = "test-exception-gp"))]
    exception::test_injection();

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
    let descs: &[MemoryRange] = unsafe {
        core::slice::from_raw_parts(bi.memory_map_phys as *const MemoryRange, desc_count)
    };
    if bi.check_memory_map(descs).is_err() {
        mem_fail();
    }
    if bi.check_capsule_in_memory(descs).is_err() {
        mem_fail();
    }

    // --- 3. capsule: plain digest, then full structural validation ---
    // Overflow safety: slice length must fit in usize (x86_64: u64). Range
    // containment within a declared memory map entry is enforced separately by
    // check_capsule_in_memory above (memory_map_length is unrelated to the
    // capsule size, so it is NOT compared against capsule_length).
    if bi.capsule_phys == 0 || bi.capsule_length == 0 || bi.capsule_length > usize::MAX as u64 {
        cap_fail();
    }
    let cap_bytes = unsafe {
        core::slice::from_raw_parts(bi.capsule_phys as *const u8, bi.capsule_length as usize)
    };
    if sha256(cap_bytes) != bi.capsule_digest {
        cap_fail();
    }
    let cap = match parse(cap_bytes) {
        Ok(c) => c,
        Err(_) => cap_fail(),
    };

    // The handoff record only mirrors the capsule's identity fields
    // (BOOT_ABI_V1 §6); it does not prove them. Verify the mirror against the
    // header of the capsule just parsed, so TOS.IDENTITY reports what the
    // artifact carries rather than what the record claimed. Fails closed with
    // RESULT_CAPSULE_INVALID, the same code already used when capsule_digest
    // disagrees with the capsule bytes.
    let ch = cap.header();
    if bi
        .check_capsule_identity(
            ch.source_identity_kind,
            ch.source_oid_alg,
            ch.source_oid_length,
            &ch.source_identity_value,
        )
        .is_err()
    {
        tos_serial::puts(b"TOS.IDENTITY.MISMATCH bootinfo-vs-capsule-header\r\n");
        cap_fail();
    }

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
    // Provenance is read from the parsed capsule header, never hardcoded and
    // never taken on the handoff record's word: the record was proved equal to
    // this header above, and the header belongs to bytes whose digest was
    // verified. The builder writes SRC_KIND_GIT when the capsule was built with
    // --git-commit (or SRC_KIND_DETACHED otherwise).
    let kind: &[u8] = match ch.source_identity_kind {
        SRC_KIND_GIT => b"git",
        SRC_KIND_DETACHED => b"detached",
        // parse() rejects any other value; fall back to a stable error marker
        // rather than a misleading kind.
        _ => b"unknown",
    };
    tos_serial::puts(b"TOS.IDENTITY source_kind=");
    tos_serial::puts(kind);
    tos_serial::puts(b" source_digest=");
    tos_serial::put_hex32(&ch.source_identity_value);
    tos_serial::puts(b" capsule_digest=");
    tos_serial::put_hex32(&bi.capsule_digest);
    tos_serial::puts(b" arch=0.2.1 builder=1\r\n");

    // --- 6. best-effort human-facing diagnostic ---
    // All boot decisions, source identity checks and canonical boot-text work
    // above have succeeded. Rendering cannot affect the result.
    unsafe { framebuffer::render_stage1_status(bi) };

    // --- 7. halt with success code ---
    tos_serial::puts(b"TOS.HALT ok=0x10\r\n");
    result_port(RESULT_HALT_OK)
}
