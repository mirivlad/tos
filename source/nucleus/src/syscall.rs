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
/// Answer a call and wait for the next message, in one crossing pair
/// (ADR-0063). The only operation of this ABI version requiring two
/// capabilities, and `SYSTEM_ABI_V1` §3 assigns their positions in §5 order:
/// the reply it consumes in `rdi`, the endpoint it then waits on in `rsi`.
const ENDPOINT_REPLY_RECEIVE: u64 = 13;
/// The endings of the direct children of the process object this names
/// (ADR-0067). `rdi` = a process capability with `wait_child`, `rsi` = flags.
pub const PROCESS_WAIT_CHILD: u64 = 14;
/// `process_create` with the restart generation its caller asserts (ADR-0067).
/// Operation 8 is unchanged and asserts none.
const PROCESS_CREATE_WITH_GENERATION: u64 = 15;
/// Reserve part of a memory authority as a child of it (ADR-0076 §9).
/// `rdi` = the authority, `rsi` = the bytes to reserve; the result is a handle
/// to the child, in the value register operation 5 already returns one in.
///
/// **A different operation from 5, because it does a different thing.** Five
/// refines rights and hands back another name for the same budget; this makes a
/// new accounting node and takes the parent's remainder down by what the child
/// may spend. One changes what everybody else can spend and the other does not,
/// so they are not two spellings of one call.
const CAPABILITY_ATTENUATE_SCOPED: u64 = 16;

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
    pub(crate) const fn status(status: i64) -> Answer {
        Answer { status, value: 0 }
    }

    pub(crate) const fn value(value: u64) -> Answer {
        Answer { status: OK, value }
    }

    /// The answer a blocking operation gets when it is cancelled — by the
    /// nucleus's liveness rule, or by anything else that can cancel one.
    pub const fn cancelled() -> Answer {
        Answer::status(E_CANCELLED)
    }

    /// The status alone, for a caller that has to decide whether to go on.
    ///
    /// One place needs this — the two-capability operation, whose second half
    /// runs only if the first succeeded — and it is a read rather than a second
    /// copy of the field.
    const fn status_of(&self) -> i64 {
        self.status
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
/// User/kernel boundary crossings **into** the nucleus through the one edge.
///
/// `IPC_V1` §8 bounds a request/reply at four crossings "excluding scheduler
/// preemption", and §9.7 asks for the number to be counted rather than
/// estimated. This counts one direction; `process::entries` counts the other,
/// and preemption goes through the timer stub, which is neither.
/// Split by whether the operation is one `IPC_V1` §8 bounds, because the bound
/// is "per request/reply" and a boot's total says nothing about that: a
/// `time_monotonic` in a spin loop crosses the same edge and belongs to no
/// exchange.
static mut IPC_ENTRIES: u64 = 0;
static mut OTHER_ENTRIES: u64 = 0;
/// Calls that came back through the edge. A call that blocked or ended its
/// process never reaches this, which is why it is counted separately from the
/// entry rather than assumed to follow it.
static mut RETURNS: u64 = 0;
/// Exchanges begun. One `endpoint_call` is one request/reply, which is the unit
/// `IPC_V1` §8 states its crossing bound in.
static mut EXCHANGES: u64 = 0;
/// The *outward* crossings of the operations an exchange is made of: one per
/// operation that came back, by whichever of the three doors it used — the edge,
/// the scheduler, or a tick switching to the context it was set down in.
static mut IPC_RETURNS: u64 = 0;

/// One operation's way out, from a door that is not the edge.
///
/// Called by the scheduler and by the preemption path, which are the two ways a
/// call that blocked comes back. It lives here rather than there because the
/// count belongs beside the one for the edge: three doors, one number.
pub fn count_operation_return() {
    // SAFETY: single-context nucleus; the dispatcher, the scheduler and the
    // timer handler never run at once.
    unsafe { IPC_RETURNS += 1 };
}

/// That count.
pub fn ipc_returns() -> u64 {
    // SAFETY: as above.
    unsafe { IPC_RETURNS }
}

/// That count.
pub fn exchanges() -> u64 {
    // SAFETY: single-context nucleus; the only writer is the dispatcher.
    unsafe { EXCHANGES }
}

/// Those three: IPC calls in, other calls in, calls returned.
pub fn crossings() -> (u64, u64, u64) {
    // SAFETY: single-context nucleus; the only writer is the dispatcher, which
    // runs with interrupts masked.
    unsafe { (IPC_ENTRIES, OTHER_ENTRIES, RETURNS) }
}

/// Whether an operation is one an exchange is made of.
///
/// `endpoint_reply_receive` is one of them and is the reason this predicate is
/// load-bearing rather than descriptive: it performs the two halves an exchange
/// spends on a server, so an instrument that did not count it would report the
/// operation that meets `IPC_V1` §8's bound as belonging to no exchange, and
/// the boot that met the bound would measure as costing less than nothing.
fn is_ipc(operation: u64) -> bool {
    matches!(
        operation,
        ENDPOINT_SEND | ENDPOINT_RECEIVE | ENDPOINT_CALL | ENDPOINT_REPLY | ENDPOINT_REPLY_RECEIVE
    )
}

#[no_mangle]
extern "C" fn syscall_dispatch(operation: u64, frame: &mut TrapFrame) {
    // SAFETY: as above. Counted first, so that a call which blocks and never
    // reaches the bottom of this function is still counted as having crossed.
    unsafe {
        if is_ipc(operation) {
            IPC_ENTRIES += 1;
            if operation == ENDPOINT_CALL {
                EXCHANGES += 1;
            }
        } else {
            OTHER_ENTRIES += 1;
        }
    };
    // The two selectors the stub could not know: they are declared in the GDT
    // this module's neighbour builds, and a copy of them in assembly would be a
    // copy that drifts.
    frame.cs = u64::from(crate::exception::USER_CODE_SELECTOR);
    frame.ss = u64::from(crate::exception::USER_DATA_SELECTOR);
    let answer = answer(operation, frame);
    answer.into_frame(frame);
    // Reached only by a call that is about to return through the edge: one that
    // blocked or ended its process left `answer` by another door.
    // SAFETY: single-context nucleus with interrupts masked.
    unsafe {
        RETURNS += 1;
        if is_ipc(operation) {
            IPC_RETURNS += 1;
        }
    };
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
        // "Gives up the rest of the quantum" (§5). It does not return here:
        // the call is set down with its answer already written and picked up
        // when the scheduler comes back round.
        // SAFETY: this is the running context's own frame.
        CONTEXT_YIELD => unsafe { crate::process::yield_now(frame) },
        // The monotonic tick, which counts timer interrupts and nothing else:
        // Stage 3 claims no wall-clock time and no trusted time source, so this
        // is a number that only ever goes up, not a duration.
        TIME_MONOTONIC => Answer::value(crate::apic::ticks()),

        ENDPOINT_SEND => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_SEND) {
                Err(refused) => refused.into(),
                Ok(Object::Endpoint(endpoint)) => send(caller, endpoint, frame),
                // The handle resolved to an object of another kind, which is the
                // wrong authority rather than no handle at all.
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        ENDPOINT_RECEIVE => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_RECEIVE) {
                Err(refused) => refused.into(),
                // §5 row 2 puts this operation's flags in `rsi`.
                Ok(Object::Endpoint(endpoint)) => {
                    receive(caller, endpoint, frame.rsi, ENDPOINT_RECEIVE as u32, frame)
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
        CAPABILITY_ATTENUATE_SCOPED => {
            attenuate_scoped(caller, arguments.first(), arguments.second())
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
        PROCESS_CREATE => create_process(caller, frame, None),
        // ADR-0067: the same creation, with the restart generation its caller
        // asserts. A separate number rather than an argument on operation 8,
        // because an old caller leaves `r8` uninitialised and `rdx` already
        // carries the child's capability handle — extending 8 in place would
        // break the minor-version rule §7 states.
        PROCESS_CREATE_WITH_GENERATION => create_process(caller, frame, Some(frame.r8)),

        // The endings of a process object's direct children (ADR-0067). The
        // authority is over a *process*, and what it scopes is that process's
        // child relation: this is not a wait-for-anything, it is a wait on the
        // set one handle names.
        PROCESS_WAIT_CHILD => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_WAIT_CHILD) {
                Err(refused) => refused.into(),
                Ok(Object::Process { slot, .. }) => {
                    let target = crate::process::instance(slot as usize);
                    if target == 0 {
                        // The capability resolved, so the object is live; a
                        // process with no identity is a defect, not an answer.
                        return Answer::status(E_BAD_ARGUMENT);
                    }
                    // SAFETY: the caller is the running process and this is its
                    // own frame; `target` is the instance the capability named.
                    unsafe {
                        crate::process::wait_child(
                            caller,
                            frame,
                            target,
                            frame.rsi & NON_BLOCKING != 0,
                        )
                    }
                }
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        // Everything from `process_terminate` on. The split is a consequence of
        // this function having grown one operation at a time; it changes no
        // dispatch order, because a match on distinct constants has none.
        _ => answer_rest(operation, frame, caller),
    }
}

/// `process_create` (8) and `process_create_with_generation` (15).
///
/// One body, because the two differ in exactly one thing: whether a supervisor
/// asserted a restart generation. Operation 8 passes `None` and the child then
/// has no generation at all — not zero, which would be a claim nobody made.
fn create_process(caller: usize, frame: &mut TrapFrame, restart_generation: Option<u64>) -> Answer {
    let arguments = &*frame;
    {
        {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_CREATE) {
                Err(refused) => refused.into(),
                Ok(Object::Process { .. }) => {
                    let Some(entry) = module_of(frame) else {
                        return Answer::status(E_BAD_ARGUMENT);
                    };
                    let mut endowment = [capability::Endowment::Own {
                        binding: capability::Binding::NONE,
                        rights: 0,
                    };
                        tos_launch::MAX_ENDOWMENT as usize + 1];
                    let held = capability::rights_of(caller, frame.rdi);
                    // What the child may do to *itself*, decided by its parent
                    // and bounded by the authority the parent used. It cannot be
                    // one of the entries below, because those name capabilities
                    // the parent holds and this one names a process that does not
                    // exist until the instant it is granted — the same reason
                    // only a launcher could issue the first one.
                    let mut count = 0;
                    if frame.r10 != 0 {
                        // The name the child bound its own process authority
                        // to, which the parent wrote beside the rights: the
                        // rights are a value and travel in a register, the name
                        // is not and does not (ADR-0058, ADR-0061).
                        let Some(binding) = self_binding() else {
                            return Answer::status(E_BAD_ARGUMENT);
                        };
                        endowment[0] = capability::Endowment::Own {
                            binding,
                            rights: frame.r10 as u32 & held,
                        };
                        count = 1;
                    }
                    match child_endowment(caller, frame, &mut endowment[count..]) {
                        Ok(given) => count += given,
                        Err(answer) => return answer,
                    }
                    let parent = crate::process::instance(caller);
                    // SAFETY: the template was established at boot from validated
                    // inputs, and no process is running: this call is the nucleus.
                    match unsafe {
                        crate::process::create(
                            entry,
                            &endowment[..count],
                            parent,
                            restart_generation,
                        )
                    } {
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
                                .ok()
                            }) {
                                Some(handle) => {
                                    // The identity, where the caller asked for
                                    // the form that reports one. `rdx` carries
                                    // the handle either way, and a handle is not
                                    // an identity (ADR-0067 §7).
                                    if restart_generation.is_some()
                                        && !crate::process::write_created_instance(caller, child)
                                    {
                                        // The caller cannot be told which child
                                        // it made, so it does not get one.
                                        // SAFETY: the child was just created and
                                        // has never run.
                                        unsafe { crate::process::terminate(caller, child) };
                                        return Answer::status(E_BAD_ARGUMENT);
                                    }
                                    Answer::value(handle)
                                }
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
    }
}

/// The rest of `answer`, from `process_terminate` on.
fn answer_rest(operation: u64, frame: &mut TrapFrame, caller: usize) -> Answer {
    let arguments = &*frame;
    match operation {
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
        ENDPOINT_CALL => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_CALL) {
                Err(refused) => refused.into(),
                Ok(Object::Endpoint(endpoint)) => call(caller, endpoint, frame),
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        ENDPOINT_REPLY => {
            match capability::resolve(caller, arguments.first(), tos_launch::RIGHT_REPLY) {
                Err(refused) => refused.into(),
                // §5 row 4: the answer's length in `rsi`, where a one-capability
                // operation's first value goes.
                Ok(Object::Reply { caller: asked, .. }) => {
                    reply(caller, asked as usize, arguments.first(), frame.rsi)
                }
                Ok(_) => Answer::status(E_NO_CAPABILITY),
            }
        }
        // Two capabilities, resolved **both before either is used**. A
        // half-performed operation would leave a caller answered and this
        // process not waiting, which is the state this operation exists to make
        // impossible (ADR-0063), so nothing is delivered until both are known
        // good. `IPC_V1` §9.3's rule for a failed send is the same rule.
        ENDPOINT_REPLY_RECEIVE => {
            let reply_handle = arguments.first();
            let endpoint_handle = arguments.second();
            let asked = match capability::resolve(caller, reply_handle, tos_launch::RIGHT_REPLY) {
                Err(refused) => return refused.into(),
                Ok(Object::Reply { caller: asked, .. }) => asked as usize,
                Ok(_) => return Answer::status(E_NO_CAPABILITY),
            };
            let endpoint =
                match capability::resolve(caller, endpoint_handle, tos_launch::RIGHT_RECEIVE) {
                    Err(refused) => return refused.into(),
                    Ok(Object::Endpoint(endpoint)) => endpoint,
                    Ok(_) => return Answer::status(E_NO_CAPABILITY),
                };
            reply_receive(caller, asked, reply_handle, endpoint, frame)
        }
        REGION_SHARE => refuse(caller, arguments.first(), ALL_RIGHTS),

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
fn send(caller: usize, endpoint: u32, frame: &mut TrapFrame) -> Answer {
    let from = crate::process::arguments_region();
    if from == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    let length = frame.rsi;
    // The authority travelling with the message, resolved **before** anything is
    // queued. What goes into the queue is the object, not this caller's name for
    // it: a handle means nothing in another table, and this one may be released
    // or its owner may end before the message is delivered.
    let mut granted = [(Object::None, 0u32, 0u64); ipc::MAX_TRANSFERRED as usize];
    let count = frame.r10 as usize;
    // A count past the contract's maximum is a **malformed call**, not a
    // resource condition, and it answers `E_BAD_ARGUMENT` for the same reason
    // an oversize payload does: the three bounds of `IPC_V1` §3 are constants
    // the caller knew before it called. `E_LIMIT` is what a *full queue*
    // answers (§9.2), and a caller that could not tell "retry later" from "this
    // call can never work" would be told nothing useful by either — which is
    // exactly the merge `SYSTEM_ABI_V1` §4 forbids for the other pair.
    if count > granted.len() {
        return Answer::status(E_BAD_ARGUMENT);
    }
    match resolve_transfers(caller, from, &mut granted[..count]) {
        Ok(()) => {}
        Err(answer) => return answer,
    }
    // The queue is about to become a holder of everything the message carries,
    // and it is a holder that outlives this call: taken before the message is
    // queued, so no path exists where the message is committed and its objects
    // are not counted.
    if capability::retain_in_transit(&granted[..count]).is_err() {
        return Answer::status(E_LIMIT);
    }
    // SAFETY: `from` is the physical address of this process's argument region,
    // mapped by the launcher and read here through the nucleus's own identity
    // map.
    match unsafe { ipc::send(endpoint, from, length, &granted[..count]) } {
        Ok(()) => {
            deliver_to_waiter(endpoint);
            Answer::status(OK)
        }
        // A send that did not happen holds nothing: the message's names go
        // back before the refusal is answered, including on the blocking path,
        // where the caller will resolve and retain again when it is resumed.
        Err(refused) => {
            capability::release_from_transit(&granted[..count]);
            match refused {
                ipc::Refused::BadArgument => Answer::status(E_BAD_ARGUMENT),
                ipc::Refused::Limit if frame.rdx & NON_BLOCKING != 0 => Answer::status(E_LIMIT),
                ipc::Refused::Limit => {
                    // `IPC_V1` §7: the system never grows a queue to accept a
                    // message, so the sender waits for room rather than the
                    // queue making some.
                    // SAFETY: this is the running context's own frame, and the
                    // handle the wait is on was resolved above.
                    unsafe {
                        crate::process::block(
                            frame,
                            crate::process::Waiting::Room(endpoint),
                            ENDPOINT_SEND as u32,
                        )
                    }
                }
                ipc::Refused::WouldBlock => Answer::status(E_WOULD_BLOCK),
            }
        }
    }
}

/// Sends a request and waits for its answer (`IPC_V1` §4).
///
/// The receiver is handed a **reply capability** with the message: the right to
/// answer this one call, naming this caller, and nothing else. It travels in
/// the last slot of the transfer table, so a call may carry one fewer
/// capability of its own than a send — a reply is a capability like any other
/// and takes a place like any other.
///
/// **A call does not wait for room.** A full queue means the request could not
/// be made, and answering `E_LIMIT` says exactly that; blocking for room and
/// then calling would be a call assembled in two steps, with a half-made call
/// in the nucleus in between — the shape ADR-0058 refused. What a call waits
/// for is the answer.
fn call(caller: usize, endpoint: u32, frame: &mut TrapFrame) -> Answer {
    let from = crate::process::arguments_region();
    if from == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    let mut granted = [(Object::None, 0u32, 0u64); ipc::MAX_TRANSFERRED as usize];
    let count = frame.r10 as usize;
    if count + 1 > granted.len() {
        // One place is spoken for by the answer, so a call carries one fewer of
        // its own than a send does. Malformed rather than limited, as in `send`.
        return Answer::status(E_BAD_ARGUMENT);
    }
    match resolve_transfers(caller, from, &mut granted[..count]) {
        Ok(()) => {}
        Err(answer) => return answer,
    }
    // The right to answer this call, made now and belonging to nobody yet. It
    // goes in the **last** slot, always, so that a receiver knows where to look
    // without being told how many capabilities the caller chose to send.
    granted[ipc::MAX_TRANSFERRED as usize - 1] = (
        Object::Reply {
            caller: caller as u32,
            generation: crate::process::next_reply_token(caller),
        },
        tos_launch::RIGHT_REPLY,
        0,
    );
    // As in `send`: the queue becomes a holder of everything the message
    // carries — the reply capability among them — before it is queued.
    if capability::retain_in_transit(&granted).is_err() {
        return Answer::status(E_LIMIT);
    }
    // SAFETY: `from` is this process's own argument region.
    match unsafe { ipc::send(endpoint, from, frame.rsi, &granted) } {
        Ok(()) => {
            deliver_to_waiter(endpoint);
            // SAFETY: this is the running context's own frame, and the handle
            // the wait is on was resolved above.
            unsafe {
                crate::process::block(frame, crate::process::Waiting::Reply, ENDPOINT_CALL as u32)
            }
        }
        Err(refused) => {
            capability::release_from_transit(&granted);
            match refused {
                ipc::Refused::BadArgument => Answer::status(E_BAD_ARGUMENT),
                _ => Answer::status(E_LIMIT),
            }
        }
    }
}

/// Answers one call, and spends the capability that permitted it.
///
/// The answer does not go through a queue: the caller is already waiting for
/// it, so this is one copy from the replier's argument region into the caller's
/// and the caller's suspended call is answered where it stands.
///
/// `length` is the caller's, for the same reason `receive`'s flags are: the two
/// operations that answer a call do not carry the answer's length in the same
/// register — `endpoint_reply` in `rsi`, `endpoint_reply_receive` in `rdx`,
/// because `rsi` is where §5 row 13 puts its second capability.
fn reply(replier: usize, asked: usize, handle: u64, length: u64) -> Answer {
    let from = crate::process::arguments_region();
    let into = crate::process::arguments_of(asked);
    if from == 0 || into == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    // SAFETY: both are argument regions the launcher mapped, reached through
    // the nucleus's own identity map.
    let length = match unsafe { ipc::hand(from, into, length) } {
        Ok(length) => length,
        Err(_) => return Answer::status(E_BAD_ARGUMENT),
    };
    // SAFETY: the reply capability resolved, which is what says this context is
    // blocked waiting for exactly this answer.
    unsafe { crate::process::wake(asked, Answer::value(length)) };
    // Waking the caller moved the counter the reply capability names, so it has
    // already stopped resolving. Releasing the handle as well is not belt and
    // braces: it gives the slot back, and a table full of capabilities that name
    // nothing is a table that fills up.
    let _ = capability::release(replier, handle);
    Answer::status(OK)
}

/// Answers a call and waits for the next message, without returning to CPL 3.
///
/// The two halves are one operation and the order is what makes it one: the
/// answer is delivered first, because a delivery that failed must not leave a
/// wait behind, and the wait is entered second, because a wait entered first
/// would be a wait this process could be cancelled out of while still holding an
/// unspent reply.
///
/// **Cancellation cannot un-answer.** If the wait is ended by ADR-0059's
/// liveness rule after the answer was delivered, the caller keeps its answer and
/// this operation returns `E_CANCELLED` — which is what the caller already
/// observed as a completed call. Making the delivery conditional on something
/// that happens afterwards would make a message's arrival depend on the future.
fn reply_receive(
    replier: usize,
    asked: usize,
    handle: u64,
    endpoint: u32,
    frame: &mut TrapFrame,
) -> Answer {
    // §5 row 13: the answer's length in `rdx`, the flags in `r10`. Neither is
    // where the one-capability operations put them, because `rsi` is spoken for
    // by the second capability.
    let answer = reply(replier, asked, handle, frame.rdx);
    if answer.status_of() != OK {
        // Nothing was delivered, so nothing is waited on: the operation refuses
        // whole rather than performing the half that worked.
        return answer;
    }
    receive(
        replier,
        endpoint,
        frame.r10,
        ENDPOINT_REPLY_RECEIVE as u32,
        frame,
    )
}

/// Which module a `process_create` names, by path.
///
/// The path is in the argument region and its length in a register (ADR-0058),
/// because a name is not a value and does not travel in one. An ordinal would
/// have fitted a register and named a position in a list nobody published.
fn module_of(frame: &TrapFrame) -> Option<usize> {
    let region = crate::process::arguments_region();
    let length = frame.rsi;
    if region == 0 || length == 0 || length > tos_launch::MAX_MODULE_PATH {
        return None;
    }
    // SAFETY: the path is at a fixed offset in this process's own argument
    // region, whose address the nucleus chose, and its length is bounded by a
    // constant of the contract rather than by anything the caller said.
    let path = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>((region + tos_launch::CREATE_MODULE) as usize),
            length as usize,
        )
    };
    crate::launch::template()?.index_of(path)
}

/// The name a parent wrote for the authority its child holds over itself.
///
/// A fixed slot of the argument region (`CREATE_SELF_BINDING`), read at an
/// address the nucleus knew before it read anything, and refused rather than
/// truncated when it does not fit.
fn self_binding() -> Option<capability::Binding> {
    let region = crate::process::arguments_region();
    if region == 0 {
        return None;
    }
    // SAFETY: the slot is at a fixed offset in this process's own argument
    // region, whose address the nucleus chose, and its length is a constant of
    // the contract rather than anything the caller said.
    let named = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(
                (region + tos_launch::CREATE_SELF_BINDING) as usize,
            ),
            tos_launch::MAX_BINDING as usize,
        )
    };
    // The slot is fixed-width and the name inside it ends at the first zero: a
    // name is text, and text does not contain one.
    let length = named
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(named.len());
    capability::Binding::new(&named[..length])
}

/// The endowment a parent gives a child, attenuated from what the parent holds.
///
/// Every entry names a capability the parent holds and the rights it wants the
/// child to have; what the child gets is the intersection. Widening is not
/// refused so much as unexpressible — the same shape attenuation has, because it
/// is the same rule.
fn child_endowment(
    parent: usize,
    frame: &TrapFrame,
    into: &mut [capability::Endowment],
) -> Result<usize, Answer> {
    let region = crate::process::arguments_region();
    let count = frame.rdx as usize;
    if count > into.len() || count > tos_launch::MAX_ENDOWMENT as usize {
        return Err(Answer::status(E_LIMIT));
    }
    for (index, slot) in into[..count].iter_mut().enumerate() {
        // SAFETY: the table is at a fixed offset in the parent's own argument
        // region, and `index` is inside the count bounded above.
        let asked = unsafe {
            core::ptr::with_exposed_provenance::<tos_launch::CreateEndowment>(
                (region + tos_launch::CREATE_ENDOWMENT) as usize,
            )
            .add(index)
            .read()
        };
        let object = capability::resolve(parent, asked.handle, 0).map_err(Answer::from)?;
        // The length is the parent's, so it is bounded before it is used: a
        // number a caller chose must not size a read (`SYSTEM_ABI_V1` §3).
        let length = (asked.binding_length as u64).min(tos_launch::MAX_BINDING) as usize;
        let Some(binding) = capability::Binding::new(&asked.binding[..length]) else {
            return Err(Answer::status(E_BAD_ARGUMENT));
        };
        *slot = capability::Endowment::Existing {
            binding,
            object,
            rights: asked.rights & capability::rights_of(parent, asked.handle),
            scope: 0,
        };
    }
    Ok(count)
}

/// Reserves part of a memory authority as a child of it (operation 16).
///
/// The reservation moves at once and no frame moves with it (ADR-0076 §2b): the
/// parent's remainder falls by exactly what the child may spend, the child
/// starts with all of it unspent, and the pool is untouched.
///
/// **The capability slot is found before the node is made.** A child authority
/// with no handle naming it is a reservation nobody can spend or return, and it
/// would be one the caller could not even be told about. So a full table
/// refuses here, before the parent has been touched, rather than after —
/// leaving the failure path with nothing to undo but its own refusal.
fn attenuate_scoped(caller: usize, handle: u64, bytes: u64) -> Answer {
    let object = match capability::resolve(caller, handle, tos_launch::RIGHT_SPEND) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    // The right alone is not enough: `spend` over something that is not an
    // authority is a right nobody granted over an object this cannot reserve
    // out of.
    let Object::MemoryAuthority { index, generation } = object else {
        return Answer::status(E_NO_CAPABILITY);
    };
    if !capability::has_room(caller) {
        return Answer::status(E_LIMIT);
    }
    let parent = crate::region::AuthorityId { index, generation };
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    let child = match tree.attenuate(parent, bytes as usize) {
        Ok(child) => child,
        // A size no budget could serve, told apart from a budget that cannot
        // serve this one (ADR-0076 §7). `Stopped` answers the nearest accepted
        // resource refusal: the caller is refused fail-closed, the nucleus has
        // already said `TOS.NUCLEUS.INVARIANT … funding-stopped`, and this is
        // not evidence that the authority's own budget ran out.
        Err(crate::region::Refusal::Empty | crate::region::Refusal::BadArgument) => {
            return Answer::status(E_BAD_ARGUMENT);
        }
        Err(_) => return Answer::status(E_LIMIT),
    };
    let named = capability::grant(
        caller,
        Object::MemoryAuthority {
            index: child.index,
            generation: child.generation,
        },
        tos_launch::RIGHT_SPEND,
        0,
    );
    // The child came back holding the maker's name; the grant took a second.
    // The maker's goes here, so the caller's handle is the only name — and the
    // count never passed through zero on the way, so the reservation never went
    // back to the parent in between.
    let answer = match named {
        Ok(handle) => Answer::value(handle),
        // Ruled out above, and undone completely if it happens anyway: releasing
        // the only name returns the whole reservation to the parent rather than
        // leaving a node nothing can reach.
        Err(_) => Answer::status(E_LIMIT),
    };
    if tree.release_name(child).is_err() {
        crate::memory::note_divergence(b"scoped-attenuation-handover");
    }
    answer
}

/// Resolves the handles a message carries, in the caller's table.
fn resolve_transfers(
    caller: usize,
    region: u64,
    into: &mut [(Object, u32, u64)],
) -> Result<(), Answer> {
    for (index, entry) in into.iter_mut().enumerate() {
        // SAFETY: the table is at a fixed offset in this process's own argument
        // region, whose address the nucleus chose, and `index` is inside the
        // count the caller checked against the contract's maximum.
        let handle = unsafe {
            core::ptr::with_exposed_provenance::<u64>(
                (region + tos_launch::MESSAGE_CAPABILITIES) as usize,
            )
            .add(index)
            .read()
        };
        // A capability is delegated only by somebody who holds it, and holding
        // it is the only right this needs: sending a capability is not an
        // operation *on* the object it names.
        let object = capability::resolve(caller, handle, 0).map_err(Answer::from)?;
        *entry = (object, capability::rights_of(caller, handle), 0);
    }
    Ok(())
}

/// Takes a message, waiting for one when there is none.
///
/// Taking a message frees a place in the queue, so a sender that was waiting
/// for room is served here for the same reason a receiver is served by a send:
/// the operation that satisfies a wait is the one that performs it.
///
/// `flags` and `operation` are the caller's, not this function's to find. Two
/// operations wait here since ADR-0063 and they do not put their flags in the
/// same register — `endpoint_receive` in `rsi`, `endpoint_reply_receive` in
/// `r10`, where §5 row 13 puts them because `rsi` holds its second capability.
/// Reading one register for both would have read a *handle* as a flag word, and
/// a handle is odd as often as not.
fn receive(
    caller: usize,
    endpoint: u32,
    flags: u64,
    operation: u32,
    frame: &mut TrapFrame,
) -> Answer {
    let into = crate::process::arguments_region();
    if into == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    let mut granted = [(Object::None, 0u32, 0u64); ipc::MAX_TRANSFERRED as usize];
    // SAFETY: `into` is this process's own argument region, as above.
    match unsafe { ipc::receive(endpoint, into, &mut granted) } {
        Ok(length) => {
            hand_over(caller, into, &granted);
            accept_from_waiter(endpoint);
            Answer::value(length)
        }
        Err(ipc::Refused::WouldBlock) if flags & NON_BLOCKING != 0 => Answer::status(E_WOULD_BLOCK),
        Err(ipc::Refused::WouldBlock) => {
            // SAFETY: as in `send`.
            unsafe {
                crate::process::block(frame, crate::process::Waiting::Message(endpoint), operation)
            }
        }
        Err(ipc::Refused::BadArgument) => Answer::status(E_BAD_ARGUMENT),
        Err(ipc::Refused::Limit) => Answer::status(E_LIMIT),
    }
}

/// Writes the receiver's own handles for what a message carried.
///
/// The receiver gets its own names, in its own table, with their own
/// generations; nothing about the sender's indices is visible to it
/// (`CAPABILITY_V1` §4). Slots the message did not fill are zeroed, and a handle
/// of all zeros names nothing in any table — so a receiver reads the whole table
/// and needs no count beside it.
fn hand_over(receiver: usize, region: u64, granted: &[(Object, u32, u64)]) {
    for index in 0..ipc::MAX_TRANSFERRED as usize {
        let handle = match granted.get(index) {
            Some((object, rights, scope)) if *object != Object::None => {
                capability::grant(receiver, *object, *rights, *scope).unwrap_or(0)
            }
            _ => 0,
        };
        // SAFETY: the table is at a fixed offset in the receiver's own argument
        // region, whose address the nucleus chose, and `index` is inside the
        // contract's maximum.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u64>(
                (region + tos_launch::MESSAGE_CAPABILITIES) as usize,
            )
            .add(index)
            .write(handle)
        };
    }
    // The message has stopped being a holder, and it stops **after** the
    // receiver has become one: the order is what keeps a count from passing
    // through zero between the two, which for an authority would mean returning
    // a reservation to its parent and then delivering it.
    capability::release_from_transit(granted);
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
    let mut granted = [(Object::None, 0u32, 0u64); ipc::MAX_TRANSFERRED as usize];
    // SAFETY: `into` is the waiting context's own argument region, which its
    // own call would have written; the nucleus reaches it through its identity
    // map exactly as it would have then.
    if let Ok(length) = unsafe { ipc::receive(endpoint, into, &mut granted) } {
        hand_over(waiter, into, &granted);
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
    // A blocked sender's message carries no capability: what it named was
    // resolved when the call was made, and re-resolving it now would be
    // resolving it in a table that may have changed since. Carrying resolved
    // objects across a block belongs with the reply-capability work, and is
    // named as not done rather than guessed at.
    // SAFETY: as above, for the read side.
    if unsafe { ipc::send(endpoint, from, length, &[]) }.is_ok() {
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
