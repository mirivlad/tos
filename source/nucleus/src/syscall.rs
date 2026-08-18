// SPDX-License-Identifier: GPL-3.0-or-later
//! The nucleus/process edge: `SYSTEM_ABI_V1`.
//!
//! Everything a process can ask the system to do, it asks here, and there is
//! exactly one mechanism so that there is exactly one path to audit. This
//! module programs that mechanism and dispatches what arrives on it.
//!
//! **What this nucleus answers today.** The substrate is being built in the
//! order its own dependencies allow, and this module refuses accurately rather
//! than plausibly at every point where a piece is missing:
//!
//! - an unassigned operation number is `E_NOT_SUPPORTED`, and the caller stays
//!   runnable — §7 forbids killing a process for asking;
//! - every capability-bearing operation is `E_NO_CAPABILITY`, which is not a
//!   placeholder but the true answer: there is no capability table yet, so no
//!   caller holds the handle the operation requires, and §8.1 asks for exactly
//!   this refusal when the handle is absent;
//! - `context_yield` succeeds. With one runnable context, giving up the rest of
//!   the quantum returns to the same context, and saying `OK` describes what
//!   happened;
//! - `time_monotonic` is the one operation of V1 this nucleus does not yet
//!   implement. The tick it reads is the timer's (ADR-0049), the timer is not
//!   up, and a fabricated number would be worse than a refusal. It is named
//!   here so the gap is a statement rather than a silence.

use crate::exception::{KERNEL_SELECTOR_BASE, USER_SELECTOR_BASE};
use crate::msr::{self, EFER_SCE, IA32_EFER, IA32_FMASK, IA32_LSTAR, IA32_STAR};

core::arch::global_asm!(include_str!("syscall.S"));

/// Statuses, as `SYSTEM_ABI_V1` §4 assigns them.
pub const OK: i64 = 0;
pub const E_NO_CAPABILITY: i64 = -1;
pub const E_NOT_SUPPORTED: i64 = -7;

/// Operations, as `SYSTEM_ABI_V1` §5 assigns them. Zero is not an operation and
/// never will be: a register nobody wrote holds zero.
const CONTEXT_YIELD: u64 = 10;
const TIME_MONOTONIC: u64 = 11;
/// The operations that name a capability — every assigned number that is not
/// one of the two self-only ones.
const CAPABILITY_OPERATIONS: core::ops::RangeInclusive<u64> = 1..=9;

/// The flags cleared on entry, so that the nucleus never begins executing with
/// a flag a process chose: interrupts, single-step, direction, nested task and
/// alignment check.
const FMASK: u64 = 0x0004_4700;

extern "C" {
    fn syscall_entry();
}

/// The arguments of one call, in the order §3 gives them.
#[repr(C)]
pub struct Arguments {
    _values: [u64; 6],
}

/// What one operation returned: a status and a value, `rax` and `rdx`.
#[repr(C)]
pub struct Answer {
    status: i64,
    value: u64,
}

/// Installs the edge.
///
/// # Safety
///
/// Called once, before any process exists, with the nucleus-owned GDT already
/// loaded: the selectors written into `IA32_STAR` name descriptors of *that*
/// table, and `sysret` computes two more from them by arithmetic.
// SAFETY: the caller's promise that the nucleus GDT is loaded is what makes the
// selector arithmetic below name real descriptors.
pub unsafe fn install() {
    let star = (u64::from(USER_SELECTOR_BASE) << 48) | (u64::from(KERNEL_SELECTOR_BASE) << 32);
    // SAFETY: these four are architected MSRs of every x86_64 processor, and
    // the values are the entry point in this image plus selectors of the GDT
    // the caller states is loaded.
    unsafe {
        msr::write(IA32_STAR, star);
        msr::write(IA32_LSTAR, syscall_entry as *const () as u64);
        msr::write(IA32_FMASK, FMASK);
        msr::write(IA32_EFER, msr::read(IA32_EFER) | EFER_SCE);
    }
}

/// Answers one call. Called only by the entry stub.
///
/// The answer is by operation number and nothing else: no argument is
/// dereferenced, because §3 says arguments are values and handles, never
/// pointers the nucleus follows.
#[no_mangle]
extern "C" fn syscall_dispatch(operation: u64, _arguments: &Arguments) -> Answer {
    match operation {
        CONTEXT_YIELD => Answer {
            status: OK,
            value: 0,
        },
        // Every operation that names a capability, answered by the capability
        // the caller does not hold. There is no table to hold one in yet.
        operation if CAPABILITY_OPERATIONS.contains(&operation) => Answer {
            status: E_NO_CAPABILITY,
            value: 0,
        },
        // Assigned, and not implemented here — see the module header. The
        // status is the same one an unassigned number gets, which is the one
        // thing about this arm that is not exact, and is why it is written down
        // rather than left to be discovered.
        TIME_MONOTONIC => Answer {
            status: E_NOT_SUPPORTED,
            value: 0,
        },
        _ => Answer {
            status: E_NOT_SUPPORTED,
            value: 0,
        },
    }
}
