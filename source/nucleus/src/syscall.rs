// SPDX-License-Identifier: GPL-3.0-or-later
//! The nucleus/process edge: `SYSTEM_ABI_V1`.
//!
//! Everything a process can ask the system to do, it asks here, and there is
//! exactly one mechanism so that there is exactly one path to audit. This
//! module programs that mechanism and dispatches what arrives on it.
//!
//! **Every operation but three begins with the same question.** The capability
//! an operation requires is its first argument (ADR-0056), so the dispatcher
//! resolves that handle before it knows or cares what the operation does — and
//! resolves it in one order, fixed by the contract: index bounds, then
//! generation, then type, then rights. The first failure decides the status,
//! which is why an index outside the caller's table is `E_BAD_HANDLE` and
//! everything past that point is `E_NO_CAPABILITY`. The distinction is not
//! tidiness: an audit log that cannot tell "named nothing" from "lacks
//! authority" cannot describe an attack.
//!
//! **What this nucleus answers today.** The substrate is built in the order its
//! own dependencies allow, and this module refuses accurately rather than
//! plausibly at every point where a piece is missing:
//!
//! - an unassigned operation number is `E_NOT_SUPPORTED`, and the caller stays
//!   runnable — §7 forbids killing a process for asking;
//! - the operations that need an object this stage does not build — a region, a
//!   process authority, a reply — refuse by the ordinary check, because a
//!   caller holding an endpoint capability genuinely does not hold a region
//!   one. That is not a placeholder: it is the true answer, produced by the
//!   same code that would produce a different one if the caller held more;
//! - `context_yield` succeeds, and is the point at which a caller gives up the
//!   rest of its quantum;
//! - `time_monotonic` reads the tick the timer establishes (ADR-0049). It
//!   counts interrupts, not seconds: Stage 3 claims no wall-clock time and no
//!   trusted time source, and a number presented as a duration would be a claim
//!   this nucleus cannot support.

use crate::capability::{self, Object, Refused};
use crate::exception::{KERNEL_SELECTOR_BASE, USER_SELECTOR_BASE};
use crate::ipc;
use crate::msr::{self, EFER_SCE, IA32_EFER, IA32_FMASK, IA32_LSTAR, IA32_STAR};

core::arch::global_asm!(include_str!("syscall.S"));

/// Statuses, as `SYSTEM_ABI_V1` §4 assigns them.
pub const OK: i64 = 0;
pub const E_NO_CAPABILITY: i64 = -1;
pub const E_BAD_HANDLE: i64 = -2;
pub const E_BAD_ARGUMENT: i64 = -3;
pub const E_WOULD_BLOCK: i64 = -4;
pub const E_LIMIT: i64 = -6;
pub const E_NOT_SUPPORTED: i64 = -7;

/// Operations, as `SYSTEM_ABI_V1` §5 assigns them. Zero is not an operation and
/// never will be: a register nobody wrote holds zero.
const ENDPOINT_SEND: u64 = 1;
const ENDPOINT_RECEIVE: u64 = 2;
const ENDPOINT_CALL: u64 = 3;
const ENDPOINT_REPLY: u64 = 4;
const CAPABILITY_ATTENUATE: u64 = 5;
const CAPABILITY_RELEASE: u64 = 6;
const REGION_SHARE: u64 = 7;
const PROCESS_CREATE: u64 = 8;
const PROCESS_TERMINATE: u64 = 9;
const CONTEXT_YIELD: u64 = 10;
const TIME_MONOTONIC: u64 = 11;
/// ADR-0054: self only, takes a status, does not return.
const PROCESS_EXIT: u64 = 12;

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
    values: [u64; 6],
}

impl Arguments {
    /// The first argument: the capability the operation requires, for every
    /// operation that requires one (ADR-0056).
    fn first(&self) -> u64 {
        self.values[0]
    }

    /// The second, which is a value in every operation that has one.
    fn second(&self) -> u64 {
        self.values[1]
    }
}

/// What one operation returned: a status and a value, `rax` and `rdx`.
#[repr(C)]
pub struct Answer {
    status: i64,
    value: u64,
}

impl Answer {
    const fn status(status: i64) -> Answer {
        Answer { status, value: 0 }
    }

    const fn value(value: u64) -> Answer {
        Answer { status: OK, value }
    }
}

impl From<Refused> for Answer {
    fn from(refused: Refused) -> Answer {
        Answer::status(match refused {
            Refused::BadHandle => E_BAD_HANDLE,
            Refused::NoCapability => E_NO_CAPABILITY,
        })
    }
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
/// No argument is dereferenced: §3 says arguments are values and handles, never
/// pointers the nucleus follows. The one thing an operation can name that is
/// bigger than a register — a message's payload — is not named by the call at
/// all; it sits in the slot the launcher mapped, at an address the nucleus
/// knows and the caller did not choose.
#[no_mangle]
extern "C" fn syscall_dispatch(operation: u64, arguments: &Arguments) -> Answer {
    // The process is inside the nucleus at this instant, so its report region
    // is stable and this is when what it wrote reaches the log.
    crate::process::drain_report();
    let caller = crate::process::current();
    match operation {
        // The one operation that does not answer: the process is over, and the
        // nucleus continues where it recorded it would (ADR-0054).
        PROCESS_EXIT => {
            if crate::process::exited(arguments.first()) {
                unreachable!("a process that exited does not receive an answer")
            }
            // Nothing is running at CPL 3, so this call did not come from a
            // process and there is nothing to end.
            Answer::status(E_NO_CAPABILITY)
        }
        CONTEXT_YIELD => Answer::status(OK),
        // The monotonic tick, which counts timer interrupts and nothing else:
        // Stage 3 claims no wall-clock time and no trusted time source, so this
        // is a number that only ever goes up, not a duration.
        TIME_MONOTONIC => Answer::value(crate::apic::ticks()),

        ENDPOINT_SEND => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_SEND) {
                Err(refused) => refused.into(),
                Ok(Object::Endpoint(endpoint)) => {
                    let slot = crate::process::message_slot();
                    if slot == 0 {
                        return Answer::status(E_BAD_ARGUMENT);
                    }
                    // SAFETY: `slot` is the physical address of this process's
                    // message region, mapped by the launcher and read here through
                    // the nucleus's own identity map.
                    match unsafe { ipc::send(endpoint, slot, arguments.second()) } {
                        Ok(()) => Answer::status(OK),
                        Err(ipc::Refused::BadArgument) => Answer::status(E_BAD_ARGUMENT),
                        Err(ipc::Refused::Limit) => Answer::status(E_LIMIT),
                        Err(ipc::Refused::WouldBlock) => Answer::status(E_WOULD_BLOCK),
                    }
                }
                // The handle resolved to an object of another kind, which is the
                // wrong authority rather than no handle at all.
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        ENDPOINT_RECEIVE => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_RECEIVE) {
                Err(refused) => refused.into(),
                Ok(Object::Endpoint(endpoint)) => {
                    let slot = crate::process::message_slot();
                    if slot == 0 {
                        return Answer::status(E_BAD_ARGUMENT);
                    }
                    // SAFETY: as above, for the write side.
                    match unsafe { ipc::receive(endpoint, slot) } {
                        Ok(length) => Answer::value(length),
                        Err(ipc::Refused::WouldBlock) => Answer::status(E_WOULD_BLOCK),
                        Err(ipc::Refused::BadArgument) => Answer::status(E_BAD_ARGUMENT),
                        Err(ipc::Refused::Limit) => Answer::status(E_LIMIT),
                    }
                }
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        CAPABILITY_ATTENUATE => {
            // Attenuation names no right of its own: what it needs is the
            // capability itself, and what it produces is bounded by that
            // capability's rights whatever the caller asked for.
            match capability::attenuate(caller, arguments.first(), arguments.second() as u32, 0) {
                Ok(handle) => Answer::value(handle),
                Err(refused) => refused.into(),
            }
        }
        CAPABILITY_RELEASE => match capability::release(caller, arguments.first()) {
            Ok(()) => Answer::status(OK),
            Err(refused) => refused.into(),
        },

        // The operations whose objects this stage does not build. They are not
        // special-cased: the handle is resolved by the same code as any other,
        // and a caller holding an endpoint capability does not hold a region,
        // a reply or a process authority — so the refusal is produced rather
        // than asserted, and it would stop being a refusal the moment a caller
        // held the right thing.
        ENDPOINT_CALL => refuse(caller, arguments.first(), tos_launch::RIGHT_CALL),
        ENDPOINT_REPLY | REGION_SHARE | PROCESS_CREATE | PROCESS_TERMINATE => {
            refuse(caller, arguments.first(), ALL_RIGHTS)
        }

        _ => Answer::status(E_NOT_SUPPORTED),
    }
}

/// Every right of every object kind: what no capability this stage issues can
/// satisfy, so that an operation whose object does not exist yet refuses
/// through the ordinary check rather than beside it.
const ALL_RIGHTS: u32 = u32::MAX;

/// Resolves a handle for an operation this stage does not implement, so that
/// the caller learns the true reason its call could not proceed.
fn refuse(caller: usize, handle: u64, rights: u32) -> Answer {
    match capability::resolve(caller, handle, rights) {
        Err(refused) => refused.into(),
        // Unreachable while no capability carries these rights, and correct
        // rather than convenient if one ever does: an operation that resolved
        // its capability and then did nothing would be a success that was not
        // one.
        Ok(_) => Answer::status(E_NOT_SUPPORTED),
    }
}
