// SPDX-License-Identifier: GPL-3.0-or-later
//! Reaching CPL 3 — the mechanism, and the evidence that it is real.
//!
//! This module is test-only and compiled into no production artifact. What it
//! carries is the *excursion*: a user code page, a user stack, and the
//! instruction that leaves ring 0. The production caller of all of this is the
//! first process, which arrives with the runtime image it is supposed to run;
//! until there is something worth running at CPL 3, entering it in a production
//! boot would be theatre.
//!
//! What is not test-only is everything underneath: the user descriptors in the
//! GDT, `TSS.rsp0`, the `syscall` entry and its dispatcher are installed on
//! every boot, because the edge is part of the substrate whether or not anyone
//! has called it yet.

use tos_frames::{Frames, FRAME_SIZE};

core::arch::global_asm!(include_str!("ring3.S"));

use crate::paging::{AddressSpace, PagingRefused};
use crate::process::{self, Ended};

/// Where the excursion's two pages live in the address space.
///
/// Low canonical addresses, far from everything the nucleus identity-maps for
/// itself, so that a payload reaching past its own pages reaches nothing.
const USER_CODE: u64 = 0x4000_0000;
const USER_STACK: u64 = 0x4010_0000;

extern "C" {
    #[cfg(feature = "test-ring3-abi")]
    static ring3_payload_start: u8;
    #[cfg(feature = "test-ring3-abi")]
    static ring3_payload_end: u8;
    #[cfg(feature = "test-ring3-privileged")]
    static ring3_privileged_start: u8;
    #[cfg(feature = "test-ring3-privileged")]
    static ring3_privileged_end: u8;
}

/// Which payload an excursion carries.
pub enum Payload {
    /// Checks what a call preserves and what an unassigned operation returns.
    #[cfg(feature = "test-ring3-abi")]
    Abi,
    /// Executes a privileged instruction and nothing else.
    #[cfg(feature = "test-ring3-privileged")]
    Privileged,
}

impl Payload {
    /// The bytes to copy, as the linker laid them out in this image.
    fn bytes(&self) -> &'static [u8] {
        let (start, end) = match self {
            #[cfg(feature = "test-ring3-abi")]
            Payload::Abi => (
                core::ptr::addr_of!(ring3_payload_start),
                core::ptr::addr_of!(ring3_payload_end),
            ),
            #[cfg(feature = "test-ring3-privileged")]
            Payload::Privileged => (
                core::ptr::addr_of!(ring3_privileged_start),
                core::ptr::addr_of!(ring3_privileged_end),
            ),
        };
        // SAFETY: both symbols are labels in this image's `.text`, in this
        // order, and the bytes between them are the payload the assembler
        // emitted — mapped, readable, and constant for the life of the boot.
        unsafe { core::slice::from_raw_parts(start, end as usize - start as usize) }
    }
}

/// Runs `payload` at CPL 3 and reports how it ended.
///
/// Every payload here ends in a fault, which is the only way an excursion can
/// end until a process can say it finished. That it comes back **at all**, and
/// that the boot continues afterwards, is the property ADR-0049 section 3 asks
/// for: a fault in a process is not the end of the system.
///
/// # Safety
///
/// `space` is the address space currently loaded in `CR3` and no other context
/// is running.
// SAFETY: the caller's promise that this space is the live one is what makes
// the two mappings below reachable by the payload.
pub unsafe fn run(
    space: &mut AddressSpace,
    frames: &mut Frames,
    payload: Payload,
) -> Result<Ended, PagingRefused> {
    const PRESENT_USER: u64 = 1 | (1 << 2);
    const WRITABLE: u64 = 1 << 1;
    const NO_EXECUTE: u64 = 1 << 63;

    let Some(code) = frames.allocate_frame() else {
        return Err(PagingRefused::NoFrame);
    };
    let Some(stack) = frames.allocate_frame() else {
        return Err(PagingRefused::NoFrame);
    };

    let bytes = payload.bytes();
    if bytes.len() > FRAME_SIZE as usize {
        return Err(PagingRefused::NoFrame);
    }
    // SAFETY: `code` is a cleared frame this pool just handed out and nothing
    // else references it; it is identity-mapped for the nucleus, and `bytes` is
    // a distinct range of this image's text.
    unsafe {
        core::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            core::ptr::with_exposed_provenance_mut::<u8>(code as usize),
            bytes.len(),
        )
    };

    // The code page is executable and not writable; the stack is writable and
    // not executable. A payload that could write its own text would be testing
    // a boundary this system does not offer.
    space.map_page(frames, USER_CODE, code, PRESENT_USER)?;
    space.map_page(
        frames,
        USER_STACK,
        stack,
        PRESENT_USER | WRITABLE | NO_EXECUTE,
    )?;
    // SAFETY: `map_page` wrote entries into the live tree, so the processor's
    // TLB may still hold the absent state of these two addresses from an
    // earlier walk.
    unsafe { flush(USER_CODE) };
    // SAFETY: as above.
    unsafe { flush(USER_STACK) };

    tos_serial::puts(b"TOS.TEST.RING3.ENTER\r\n");
    // SAFETY: both pages are mapped user-accessible in the live space, the
    // stack top is inside its own page and 16-byte aligned, and the GDT, TSS
    // and `syscall` MSRs were installed at nucleus entry.
    let ended = unsafe { process::run(USER_CODE, USER_STACK + FRAME_SIZE, 0) };

    // The process is over, so its memory stops being its memory. Unmapped
    // first and released second: a frame back in the pool while a mapping to it
    // survives is a frame two owners can reach.
    space.unmap_page(USER_CODE);
    space.unmap_page(USER_STACK);
    // SAFETY: both frames came from this pool, both mappings are gone, and
    // nothing else references them; the process that did no longer exists.
    unsafe {
        frames.release_frame(code);
        frames.release_frame(stack);
    }
    Ok(ended)
}

/// Drops one address's translation.
///
/// SAFETY: `address` is a canonical address of the live address space.
// SAFETY: the caller names an address of the live space; `invlpg` touches no
// memory of its own.
unsafe fn flush(address: u64) {
    // SAFETY: `invlpg` on a canonical address, per the caller's contract.
    unsafe { core::arch::asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)) };
}
