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

use crate::apic::TrapFrame;
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
pub const E_CANCELLED: i64 = -5;
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

/// The one call flag this contract version has.
///
/// Blocking is the default because it is what `IPC_V1` describes — §4's
/// `endpoint_call` "sends and blocks", §7's sender "blocks with a cancellation
/// path" unless it asked not to. A caller that would rather be told there is
/// nothing to do sets this and receives `E_WOULD_BLOCK` or `E_LIMIT`, which is
/// exactly what §4 of `SYSTEM_ABI_V1` assigns those two statuses to.
const NON_BLOCKING: u64 = 1;

/// The flags cleared on entry, so that the nucleus never begins executing with
/// a flag a process chose: interrupts, single-step, direction, nested task and
/// alignment check.
const FMASK: u64 = 0x0004_4700;

extern "C" {
    fn syscall_entry();
}

/// The arguments of one call, read out of the frame the stub built.
///
/// There is no separate argument structure any more, and there should not be:
/// the six argument registers are six of the fifteen the frame already holds,
/// and a second view of the same words is a second thing to keep in step.
trait Arguments {
    /// The first argument: the capability the operation requires, for every
    /// operation that requires one (ADR-0056).
    fn first(&self) -> u64;
    /// The second, which is a value in every operation that has one.
    fn second(&self) -> u64;
}

impl Arguments for TrapFrame {
    fn first(&self) -> u64 {
        self.rdi
    }

    fn second(&self) -> u64 {
        self.rsi
    }
}

/// What one operation returned: a status and a value, `rax` and `rdx`.
///
/// Written into the caller's frame rather than returned in registers, which is
/// what lets a blocked call be answered long after the call was made: the frame
/// is where the answer goes either way, and the only difference between the two
/// is who writes it and when.
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

    /// The answer a blocking operation gets when it is cancelled — by the
    /// nucleus's liveness rule, or by anything else that can cancel one.
    pub const fn cancelled() -> Answer {
        Answer::status(E_CANCELLED)
    }

    /// The answer, as the process will see it in `rax` and `rdx`.
    pub fn into_frame(self, frame: &mut TrapFrame) {
        frame.rax = self.status as u64;
        frame.rdx = self.value;
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
extern "C" fn syscall_dispatch(operation: u64, frame: &mut TrapFrame) {
    // The two selectors the stub could not know: they are declared in the GDT
    // this module's neighbour builds, and a copy of them in assembly would be a
    // copy that drifts.
    frame.cs = u64::from(crate::exception::USER_CODE_SELECTOR);
    frame.ss = u64::from(crate::exception::USER_DATA_SELECTOR);
    let answer = answer(operation, frame);
    answer.into_frame(frame);
}

/// Answers one call, or does not return because the call blocked or ended the
/// process that made it.
fn answer(operation: u64, frame: &mut TrapFrame) -> Answer {
    // The process is inside the nucleus at this instant, so its report region
    // is stable and this is when what it wrote reaches the log.
    crate::process::drain_report();
    let caller = crate::process::current();
    let arguments = &*frame;
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
                Ok(Object::Endpoint(endpoint)) => send(endpoint, frame),
                // The handle resolved to an object of another kind, which is the
                // wrong authority rather than no handle at all.
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        ENDPOINT_RECEIVE => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_RECEIVE) {
                Err(refused) => refused.into(),
                Ok(Object::Endpoint(endpoint)) => receive(endpoint, frame),
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

        // A process is created under the authority of a process, never under an
        // authority meaning "processes" — `CAPABILITY_V1` §3 admits an object
        // and rules out a class. The caller names the process the child is
        // created under, which in every case this stage can express is its own,
        // and it can only do that because its launcher gave it that authority.
        PROCESS_CREATE => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_CREATE) {
                Err(refused) => refused.into(),
                Ok(Object::Process { .. }) => {
                    // The child is endowed with nothing. That is the launcher's
                    // own rule one level down — grant nothing that was not asked
                    // for — and handing the child anything else needs a way for
                    // this call to *name* more than a register holds, which this
                    // ABI does not have yet. An empty endowment is a decision;
                    // inventing an argument for it would be a guess.
                    // SAFETY: the template was established at boot from validated
                    // inputs, and no process is running: this call is the nucleus.
                    match unsafe { crate::process::create(arguments.second() as usize, &[]) } {
                        Ok(child) => {
                            // The caller gets authority over what it made, carrying
                            // exactly the rights the authority it used carried.
                            // More would be authority nobody granted it; less would
                            // be the nucleus deciding how a supervisor supervises.
                            let over_child = crate::process::generation(child).map(|generation| {
                                Object::Process {
                                    slot: child as u32,
                                    generation,
                                }
                            });
                            match over_child.and_then(|object| {
                                capability::grant(
                                    caller,
                                    object,
                                    capability::rights_of(caller, arguments.first()),
                                    0,
                                )
                            }) {
                                Some(handle) => Answer::value(handle),
                                // The child exists and the caller cannot name it.
                                // Ending it is the only honest response: a process
                                // nobody holds authority over is a process nobody
                                // can stop.
                                None => {
                                    // SAFETY: the child was just created by this
                                    // call and has never run.
                                    unsafe { crate::process::terminate(caller, child) };
                                    Answer::status(E_LIMIT)
                                }
                            }
                        }
                        Err(crate::process::Unlaunchable::NoSuchModule) => {
                            Answer::status(E_BAD_ARGUMENT)
                        }
                        Err(_) => Answer::status(E_LIMIT),
                    }
                }
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        PROCESS_TERMINATE => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_TERMINATE) {
                Err(refused) => refused.into(),
                Ok(Object::Process { slot, .. }) => {
                    // SAFETY: the capability just resolved names this process and
                    // carries the right to end it, which is the whole of the
                    // authority this operation requires.
                    if unsafe { crate::process::terminate(caller, slot as usize) } {
                        Answer::status(OK)
                    } else {
                        // The authority is real and its object has already ended.
                        Answer::status(E_BAD_ARGUMENT)
                    }
                }
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }

        // The operations whose objects this stage does not build. They are not
        // special-cased: the handle is resolved by the same code as any other,
        // and a caller holding an endpoint capability does not hold a region or
        // a reply — so the refusal is produced rather than asserted, and it
        // would stop being a refusal the moment a caller held the right thing.
        ENDPOINT_CALL => refuse(caller, arguments.first(), tos_launch::RIGHT_CALL),
        ENDPOINT_REPLY | REGION_SHARE => refuse(caller, arguments.first(), ALL_RIGHTS),

        _ => Answer::status(E_NOT_SUPPORTED),
    }
}

/// Sends the caller's message, waiting for room when there is none.
///
/// Two things happen after a message is queued, and the second is what makes
/// blocking worth having: if somebody is waiting for a message on this
/// endpoint, it is handed to them here and their call is answered — they do not
/// wake up to ask again. That is two copies of the payload, sender to queue and
/// queue to receiver, which is what docs/35 budgets for an inline message.
fn send(endpoint: u32, frame: &mut TrapFrame) -> Answer {
    let from = crate::process::message_slot();
    if from == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    let length = frame.rsi;
    // SAFETY: `from` is the physical address of this process's argument region,
    // mapped by the launcher and read here through the nucleus's own identity
    // map.
    match unsafe { ipc::send(endpoint, from, length) } {
        Ok(()) => {
            deliver_to_waiter(endpoint);
            Answer::status(OK)
        }
        Err(ipc::Refused::BadArgument) => Answer::status(E_BAD_ARGUMENT),
        Err(ipc::Refused::Limit) if frame.rdx & NON_BLOCKING != 0 => Answer::status(E_LIMIT),
        Err(ipc::Refused::Limit) => {
            // `IPC_V1` §7: the system never grows a queue to accept a message,
            // so the sender waits for room rather than the queue making some.
            // SAFETY: this is the running context's own frame, and the handle
            // the wait is on was resolved above.
            unsafe { crate::process::block(frame, crate::process::Waiting::Room(endpoint)) }
        }
        Err(ipc::Refused::WouldBlock) => Answer::status(E_WOULD_BLOCK),
    }
}

/// Takes a message, waiting for one when there is none.
///
/// Taking a message frees a place in the queue, so a sender that was waiting
/// for room is served here for the same reason a receiver is served by a send:
/// the operation that satisfies a wait is the one that performs it.
fn receive(endpoint: u32, frame: &mut TrapFrame) -> Answer {
    let into = crate::process::message_slot();
    if into == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    // SAFETY: `into` is this process's own argument region, as above.
    match unsafe { ipc::receive(endpoint, into) } {
        Ok(length) => {
            accept_from_waiter(endpoint);
            Answer::value(length)
        }
        Err(ipc::Refused::WouldBlock) if frame.rsi & NON_BLOCKING != 0 => {
            Answer::status(E_WOULD_BLOCK)
        }
        Err(ipc::Refused::WouldBlock) => {
            // SAFETY: as in `send`.
            unsafe { crate::process::block(frame, crate::process::Waiting::Message(endpoint)) }
        }
        Err(ipc::Refused::BadArgument) => Answer::status(E_BAD_ARGUMENT),
        Err(ipc::Refused::Limit) => Answer::status(E_LIMIT),
    }
}

/// Hands the message just queued to a context waiting for one, if there is one.
fn deliver_to_waiter(endpoint: u32) {
    let Some(waiter) = crate::process::blocked_on(crate::process::Waiting::Message(endpoint))
    else {
        return;
    };
    let into = crate::process::arguments_of(waiter);
    if into == 0 {
        return;
    }
    // SAFETY: `into` is the waiting context's own argument region, which its
    // own call would have written; the nucleus reaches it through its identity
    // map exactly as it would have then.
    if let Ok(length) = unsafe { ipc::receive(endpoint, into) } {
        // SAFETY: the context is blocked in the receive this answers.
        unsafe { crate::process::wake(waiter, Answer::value(length)) };
    }
}

/// Queues the message of a context that was waiting for room, if there is one.
fn accept_from_waiter(endpoint: u32) {
    let Some(waiter) = crate::process::blocked_on(crate::process::Waiting::Room(endpoint)) else {
        return;
    };
    let from = crate::process::arguments_of(waiter);
    if from == 0 {
        return;
    }
    let length = crate::process::suspended_argument(waiter);
    // SAFETY: as above, for the read side.
    if unsafe { ipc::send(endpoint, from, length) }.is_ok() {
        // SAFETY: the context is blocked in the send this answers.
        unsafe { crate::process::wake(waiter, Answer::status(OK)) };
        deliver_to_waiter(endpoint);
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
