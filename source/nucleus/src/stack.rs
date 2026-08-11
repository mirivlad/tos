// SPDX-License-Identifier: GPL-3.0-or-later
//! Measuring how much stack the reference path actually uses.
//!
//! TOS Core modules declare a `stack` budget (docs/41 section 6), and the
//! nucleus runs on a stack the loader sized without knowing what would run on
//! it. Neither number is worth anything unless someone measures the real one,
//! and a stack that overflows a boot-time region does not report an error — it
//! writes over whatever lies below it.
//!
//! So the unused part of the stack is painted before the run and read back
//! afterwards. The pattern is derived from each word's own address, which is
//! what keeps the reading honest: a program that happened to write the constant
//! would make an untouched word look touched, but it cannot write the right
//! constant for every address it never visited.
//!
//! Nothing here changes what runs. If the measurement is impossible — the
//! stack's extent is not in the memory map — it reports nothing rather than
//! guessing.

use core::arch::asm;

use tos_runtime::region::Span;

/// Bytes below the current stack pointer left untouched by painting.
///
/// The painting loop itself has a frame, and a compiler is free to use the
/// space just below the pointer it observed. This is slack, not a guard page.
const RESERVE: u64 = 512;

/// What one word of unused stack holds.
fn pattern(address: u64) -> u64 {
    // A fixed nonce mixed with the address: distinct per word, so a program
    // cannot leave an untouched-looking gap by writing one constant.
    address ^ 0x544F_535F_5354_4143
}

/// The current stack pointer.
pub fn pointer() -> u64 {
    let rsp: u64;
    // SAFETY: reading rsp into a register has no memory operand and no effect
    // beyond producing the value.
    unsafe {
        asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
    }
    rsp
}

/// The memory-map span that holds `address`, when one does.
pub fn containing(spans: impl IntoIterator<Item = Span>, address: u64) -> Option<Span> {
    spans.into_iter().find(|span| span.holds(address))
}

/// Paints the unused part of the stack below the current frame.
///
/// Returns the lowest address painted, which the later reading needs, or `None`
/// when the region does not actually hold the stack pointer.
///
/// # Safety
///
/// `region` must be the stack the caller is running on, and no live data may
/// exist below the current stack pointer — which is what "unused stack" means.
// SAFETY: only memory below the current stack pointer inside the caller's own stack region is written.
pub unsafe fn paint(region: Span) -> Option<u64> {
    let rsp = pointer();
    if !region.holds(rsp) {
        return None;
    }
    let floor = region.start.next_multiple_of(8);
    let ceiling = rsp.checked_sub(RESERVE)?;
    if ceiling <= floor {
        return None;
    }
    let mut address = floor;
    while address + 8 <= ceiling {
        // SAFETY: `address` is 8-aligned and lies inside the caller's own stack
        // region, strictly below the current stack pointer, so no live frame,
        // and no memory belonging to anyone else, is written.
        unsafe { (address as *mut u64).write(pattern(address)) };
        address += 8;
    }
    Some(floor)
}

/// The deepest the stack was carried since [`paint`], in bytes used from the
/// top of the region.
///
/// # Safety
///
/// `region` and `floor` must come from a matching [`paint`] on the same stack,
/// and the memory between them must still belong to that stack.
// SAFETY: only the painted range of the caller's own stack region is read.
pub unsafe fn peak(region: Span, floor: u64) -> u64 {
    let mut address = floor;
    let rsp = pointer();
    let ceiling = rsp.saturating_sub(RESERVE);
    while address + 8 <= ceiling {
        // SAFETY: this reads back exactly the range `paint` wrote, which is
        // inside the caller's own stack region.
        let word = unsafe { (address as *const u64).read() };
        if word != pattern(address) {
            break;
        }
        address += 8;
    }
    region.end.saturating_sub(address)
}
