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
use tos_frames::FRAME_SIZE;

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
/// Allocate a region out of a memory authority (ADR-0075, ADR-0076 §5).
/// `rdi` = the authority, `rsi` = the bytes wanted; `rdx` returns the region's
/// capability, and its base and charged length are written to the caller's
/// argument region at `REGION_ALLOCATE_RECORD`.
const REGION_ALLOCATE: u64 = 17;
/// The consuming mutable-to-immutable transition (ADR-0075 §3). `rdi` = a
/// mutable region capability with `write`; `rdx` returns a **new** handle to
/// the same region, carrying `read | share`.
///
/// **Consuming, and the handle says so.** ADR-0075 §3 rules out a transition
/// that leaves the caller's writable authority standing, and it rules out one
/// whose rights change under a handle the caller already holds — a process
/// would have no way to tell a frozen region from a mutable one it wrote a
/// moment ago. So the entry keeps its slot and its generation moves: the old
/// handle is stale by the same rule that makes any released handle stale, and
/// the new one is what the caller is answered with.
const REGION_FREEZE: u64 = 18;
/// Create a process and charge its whole footprint to a `MemoryAuthority` the
/// caller presents (ADR-0076 §3, §4). `rdi` = process authority with `create`,
/// `rsi` = the authority with `spend`, `rdx` = the module path's length,
/// `r10` = how many capabilities the child is endowed with, `r8` = the rights
/// the child holds over itself, `r9` = the runtime arena it asks for.
///
/// **It replaces 8 and 15 together**, which is why the optional restart
/// generation moved out of a register: 8 asserted none and 15 asserted one, and
/// an operation that replaces both has to tell "absent" from "present and zero".
/// It travels in `CreateFundedRecord`, with a flag, because a register cannot
/// carry the difference.
const PROCESS_CREATE_FUNDED: u64 = 19;
/// Create a process from an immutable bundle a **shared** region carries
/// (ADR-0073, ADR-0076). `rdi` = process authority with `create`, `rsi` = the
/// memory authority with `spend`, `rdx` = the shared region holding the bundle;
/// `r10`, `r8` and `r9` as for 19, and the same `CreateFundedRecord`.
///
/// **There is no module path, no ordinal and no entry.** ADR-0076 is explicit
/// that the bundle declares its own entry, and a caller-supplied one would be a
/// second truth about which program this is.
const PROCESS_CREATE_FROM_BUNDLE: u64 = 20;

/// Make an empty launch plan (ADR-0077 §2). `rdi` = process authority with
/// `create`; `rdx` carries the builder's handle back.
///
/// **Creation authority is required for a thing that creates nothing.** A plan
/// is bounded nucleus metadata and grants no access to anything; what requiring
/// `create` buys is that a process which may not create children cannot
/// accumulate launch policy for them, and cannot occupy the plan table by
/// writing decisions nothing will ever apply.
const LAUNCH_PLAN_CREATE: u64 = 21;
/// Add one entry to a builder (ADR-0077 §3). `rdi` = **the capability being
/// delegated**, `rsi` = the builder, `rdx` = the rights asked for, `r10` = the
/// length of the binding at `LAUNCH_ENDOW_BINDING`.
///
/// **One selector for every kind of authority there is.** The capability comes
/// first because that is the authority the call is made under: a process may
/// place into a plan exactly what it holds, at rights no wider than it holds
/// them, and the operation is reached *through* the thing being delegated
/// rather than through a general "endowment" authority nobody was granted. That
/// is also why there is one number rather than one per interface — the ABI is
/// finite and the nominal type of what is delegated is the caller's, not this
/// contract's.
const LAUNCH_PLAN_ENDOW: u64 = 22;
/// Seal a builder (ADR-0077 §4). `rdi` = process authority with `create`,
/// `rsi` = the builder; `rdx` carries the sealed plan's handle back.
///
/// Consuming, in the shape `region_freeze` established: the same capability
/// slot, the generation advanced, and the same underlying object. After it the
/// entries cannot change, which is what makes a plan a decision rather than a
/// buffer — and what makes the *second* launch from it the same decision as the
/// first.
const LAUNCH_PLAN_SEAL: u64 = 23;

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
        REGION_ALLOCATE => region_allocate(caller, arguments.first(), arguments.second()),
        CAPABILITY_RELEASE => release_capability(caller, arguments.first()),

        // **Retired, and refused before their arguments are read**
        // (ADR-0076 §4). Both spent the boot's accounting anchor without any
        // caller presenting a `MemoryAuthority`, which is the ambient funding
        // that decision retires; operation 19 replaces both. `SYSTEM_ABI_V1` §7
        // keeps their numbers assigned forever rather than recycling them, and
        // a retired operation answers `E_NOT_SUPPORTED` — the same status a
        // caller of a *later* version would get, which is exactly right: from
        // this contract version on, neither exists.
        //
        // Nothing about the call is examined. A refusal that first resolved a
        // handle would be reporting on authority for an operation that is not
        // there to need it.
        PROCESS_CREATE | PROCESS_CREATE_WITH_GENERATION => Answer::status(E_NOT_SUPPORTED),
        // A process is created under the authority of a process, never under an
        // authority meaning "processes" — `CAPABILITY_V1` §3 admits an object
        // and rules out a class — and it is paid for out of a `MemoryAuthority`
        // the caller presents beside it.
        PROCESS_CREATE_FUNDED => create_funded(caller, frame),
        // The same creation, over a program this nucleus does not read.
        PROCESS_CREATE_FROM_BUNDLE => create_from_bundle(caller, frame),

        // Launch policy, as an object with an owner (ADR-0077). Three
        // operations and one lifecycle: made empty, written entry by entry
        // through the authority each entry delegates, and sealed once.
        LAUNCH_PLAN_CREATE => launch_plan_create(caller, arguments.first()),
        LAUNCH_PLAN_ENDOW => launch_plan_endow(caller, frame),
        LAUNCH_PLAN_SEAL => launch_plan_seal(caller, frame),

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

/// What a funded creation was asked to make, once every argument has been read
/// and judged and before anything has been built.
///
/// It exists so that the reading and the judging happen in one place and the
/// construction in another. Operation 19 and operation 20 differ in where the
/// program comes from and in nothing else — the same funding, the same
/// endowment, the same self-rights, the same optional restart generation — and
/// two copies of that argument handling would be two places for it to drift.
struct FundedCreation {
    funding: crate::process::Funding,
    restart_generation: Option<u64>,
    self_rights: u32,
    /// The sealed plan the child's endowment comes from.
    ///
    /// **Not consumed by the creation that reads it.** A plan is launch policy,
    /// and a restart is that policy applied again: a creation that took the plan
    /// would make the second launch a second decision, written by whatever the
    /// supervisor could still reach at the time. The plan survives, its entries
    /// are unchanged, and every reference it holds is still held.
    plan: crate::plan::PlanId,
    child_rights: u32,
}

/// Reads the optional restart generation operations 19 and 20 carry
/// (ADR-0067, `CreateFundedRecord`).
///
/// **Absence has one representation, and it is checked rather than assumed.**
/// Operation 19 replaces both legacy creation shapes at once, so it has to keep
/// "no restart generation" and "a restart generation that happens to be zero"
/// apart; a sentinel would collapse them. The flag says which, and when it is
/// clear the generation field must be zero — a caller that leaves rubbish there
/// is refused rather than having the rubbish ignored, because two byte patterns
/// meaning the same thing is a contract with an undocumented second spelling.
///
/// Unknown flag bits are refused for the same reason `SYSTEM_ABI_V1` §7 makes
/// operation numbers permanent: a nucleus that ignored them would have already
/// accepted, with a different meaning, every record a later version will write.
fn restart_generation_of(region: u64) -> Result<Option<u64>, Answer> {
    // SAFETY: the record is at a fixed offset in this process's own argument
    // region, whose address the nucleus chose, and it is a plain-data struct of
    // two integers.
    let record = unsafe {
        core::ptr::with_exposed_provenance::<tos_launch::CreateFundedRecord>(
            (region + tos_launch::CREATE_FUNDED_RECORD) as usize,
        )
        .read()
    };
    if record.flags & !tos_launch::HAS_RESTART_GENERATION != 0 {
        return Err(Answer::status(E_BAD_ARGUMENT));
    }
    if record.flags & tos_launch::HAS_RESTART_GENERATION == 0 {
        if record.restart_generation != 0 {
            return Err(Answer::status(E_BAD_ARGUMENT));
        }
        return Ok(None);
    }
    Ok(Some(record.restart_generation))
}

/// The two capabilities a funded creation requires, in the order §5's row
/// assigns them, and the arguments that follow.
///
/// **The first failed capability decides the answer, and the second is not
/// looked at.** `SYSTEM_ABI_V1` §3 assigns the positions and §4 fixes the
/// refusal order within one handle; between two handles the order is the row's
/// own. A caller holding no authority to create must not learn, from the status
/// it gets back, anything about the memory authority it also named — the answer
/// is a fact about the call, and the call failed at its first argument.
fn funded_arguments(
    caller: usize,
    process_handle: u64,
    authority_handle: u64,
    plan_handle: u64,
    grant: u64,
    self_rights: u64,
) -> Result<FundedCreation, Answer> {
    let child_rights = match capability::resolve(caller, process_handle, tos_launch::RIGHT_CREATE) {
        Err(refused) => return Err(refused.into()),
        Ok(Object::Process { .. }) => capability::rights_of(caller, process_handle),
        Ok(_) => return Err(Answer::status(E_NO_CAPABILITY)),
    };
    let authority = match capability::resolve(caller, authority_handle, tos_launch::RIGHT_SPEND) {
        Err(refused) => return Err(refused.into()),
        Ok(Object::MemoryAuthority { index, generation }) => {
            crate::region::AuthorityId { index, generation }
        }
        // `spend` over something that is not an authority is a right nobody
        // granted over an object this cannot be funded from.
        Ok(_) => return Err(Answer::status(E_NO_CAPABILITY)),
    };
    // The endowment, as a **sealed** decision. A builder is refused: it is a
    // decision still being written, and creating from one would create from
    // whatever happened to have been added by the time the call was made.
    let plan = match capability::resolve(caller, plan_handle, 0) {
        Err(refused) => return Err(refused.into()),
        Ok(Object::LaunchPlan { index, generation }) => crate::plan::PlanId { index, generation },
        Ok(_) => return Err(Answer::status(E_NO_CAPABILITY)),
    };
    let region = crate::process::arguments_region();
    if region == 0 {
        return Err(Answer::status(E_BAD_ARGUMENT));
    }
    let restart_generation = restart_generation_of(region)?;
    Ok(FundedCreation {
        funding: crate::process::Funding { authority, grant },
        restart_generation,
        // What the child may do to *itself*, bounded by the authority the parent
        // used. It cannot be a plan entry, because those name capabilities the
        // parent holds and this one names a process that does not exist until
        // the instant it is granted.
        self_rights: self_rights as u32 & child_rights,
        plan,
        child_rights,
    })
}

/// Assembles the endowment a funded creation was asked for: the child's rights
/// over itself, then every entry of the sealed plan.
///
/// **Nothing here is read out of the caller's argument region**, which is the
/// whole change ADR-0077 made to creation. The endowment used to be a table a
/// caller wrote immediately before the call, valid for that call only, held by
/// nobody in between and read at the instant of creation. It is now an object
/// with an owner, a lifetime and a reference of its own on everything it names,
/// decided at whatever earlier moment its author chose — which is what makes a
/// restart able to be the *same* decision rather than a second one.
fn funded_endowment(
    asked: &FundedCreation,
    into: &mut [capability::Endowment],
) -> Result<usize, Answer> {
    let mut count = 0;
    if asked.self_rights != 0 {
        // The name the child bound its own process authority to, which the
        // parent wrote beside the rights: the rights are a value and travel in
        // a register, the name is not and does not (ADR-0058, ADR-0061).
        let Some(binding) = self_binding() else {
            return Err(Answer::status(E_BAD_ARGUMENT));
        };
        into[0] = capability::Endowment::Own {
            binding,
            rights: asked.self_rights,
        };
        count = 1;
    }
    let Ok(entries) = crate::plan::entries(asked.plan) else {
        // The handle resolved a moment ago, so this is not a caller's mistake.
        return Err(Answer::status(E_BAD_ARGUMENT));
    };
    if count + entries.len() > into.len() {
        return Err(Answer::status(E_LIMIT));
    }
    for entry in entries {
        // The rights are re-intersected with what the plan recorded and nothing
        // else: the plan already narrowed them against what its author held when
        // the entry was written, and the plan is the holder of that reference
        // now. Asking the author again would make a decision depend on what its
        // author still happens to hold, which is exactly what sealing removed.
        into[count] = capability::Endowment::Existing {
            binding: entry.binding,
            object: entry.object,
            rights: entry.rights,
            scope: entry.scope,
        };
        count += 1;
    }
    Ok(count)
}

/// `launch_plan_create` (21).
fn launch_plan_create(caller: usize, handle: u64) -> Answer {
    match capability::resolve(caller, handle, tos_launch::RIGHT_CREATE) {
        Err(refused) => return refused.into(),
        Ok(Object::Process { .. }) => {}
        Ok(_) => return Answer::status(E_NO_CAPABILITY),
    }
    // The slot the plan will be named by, found before the plan is made. A plan
    // nobody can name is a plan nobody can seal, create from or release, and
    // the references it would go on to hold would be held by nothing.
    if !capability::has_room(caller) {
        return Answer::status(E_LIMIT);
    }
    let Ok(plan) = crate::plan::create() else {
        return Answer::status(E_LIMIT);
    };
    let object = Object::LaunchPlanBuilder {
        index: plan.index,
        generation: plan.generation,
    };
    // **No rights.** Holding a plan capability *is* the authority over it, and
    // every operation that touches one is decided by the object's kind — which
    // is its state — and by the creation authority that operation separately
    // requires. A rights field here would be a second place the same decision
    // is made, and the two would eventually disagree.
    match capability::grant(caller, object, 0, 0) {
        Ok(handle) => Answer::value(handle),
        Err(_) => {
            crate::plan::destroy(plan);
            Answer::status(E_LIMIT)
        }
    }
}

/// `launch_plan_endow` (22).
///
/// **The capability being delegated comes first**, and that is what makes one
/// selector serve every kind of authority: the operation is reached through the
/// thing being delegated, so the authority to place an endpoint in a plan is
/// holding that endpoint, and there is no general "may endow" right anybody was
/// granted. The rights are intersected with what the caller holds, so asking
/// for more produces less rather than more.
fn launch_plan_endow(caller: usize, frame: &TrapFrame) -> Answer {
    // Resolved with no required right: what is being delegated may be anything
    // the caller holds, and *which* rights travel is the third argument.
    let object = match capability::resolve(caller, frame.rdi, 0) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    // A region does not arrive by being copied — the capability is half of what
    // a holder needs and the other half is a mapping in an address space that
    // does not exist yet — and a plan is what this is. A reply names one call of
    // one caller and is single-use; no accepted contract makes one a startup
    // endowment, so it is refused rather than quietly admitted here.
    if object.is_region() || object.plan().is_some() || matches!(object, Object::Reply { .. }) {
        return Answer::status(E_NO_CAPABILITY);
    }
    let plan = match capability::resolve(caller, frame.rsi, 0) {
        Ok(Object::LaunchPlanBuilder { index, generation }) => {
            crate::plan::PlanId { index, generation }
        }
        // A sealed plan is a decision that has been made.
        Ok(_) => return Answer::status(E_NO_CAPABILITY),
        Err(refused) => return refused.into(),
    };
    let region = crate::process::arguments_region();
    if region == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    // The length is the caller's, so it is bounded before it is used: a number
    // a caller chose must not size a read (`SYSTEM_ABI_V1` §3).
    let length = frame.rdx.min(tos_launch::MAX_BINDING) as usize;
    // SAFETY: the slot is at a fixed offset in this process's own argument
    // region, whose address the nucleus chose, and the length is bounded by a
    // constant of the contract immediately above.
    let named = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(
                (region + tos_launch::LAUNCH_ENDOW_BINDING) as usize,
            ),
            length,
        )
    };
    let Some(binding) = capability::Binding::new(named) else {
        return Answer::status(E_BAD_ARGUMENT);
    };
    let entry = crate::plan::Entry {
        binding,
        object,
        rights: frame.r10 as u32 & capability::rights_of(caller, frame.rdi),
        scope: 0,
    };
    match crate::plan::endow(plan, entry) {
        Ok(()) => Answer::status(OK),
        Err(crate::plan::Refusal::Full) => Answer::status(E_LIMIT),
        Err(_) => Answer::status(E_BAD_ARGUMENT),
    }
}

/// `launch_plan_seal` (23).
///
/// Consuming, in the shape `region_freeze` established: the same capability
/// slot, the generation advanced, the same object underneath, and no second
/// name in between. The caller's old handle stops resolving, which is what says
/// the builder is gone rather than merely finished with.
fn launch_plan_seal(caller: usize, frame: &TrapFrame) -> Answer {
    match capability::resolve(caller, frame.rdi, tos_launch::RIGHT_CREATE) {
        Err(refused) => return refused.into(),
        Ok(Object::Process { .. }) => {}
        Ok(_) => return Answer::status(E_NO_CAPABILITY),
    }
    let plan = match capability::resolve(caller, frame.rsi, 0) {
        Ok(Object::LaunchPlanBuilder { index, generation }) => {
            crate::plan::PlanId { index, generation }
        }
        Ok(_) => return Answer::status(E_NO_CAPABILITY),
        Err(refused) => return refused.into(),
    };
    // Everything fallible first. After this there is one write to the plan and
    // one to the capability slot, and neither can refuse.
    let Ok(sealed) = crate::plan::seal(plan) else {
        return Answer::status(E_BAD_ARGUMENT);
    };
    let object = Object::LaunchPlan {
        index: sealed.index,
        generation: sealed.generation,
    };
    match capability::replace_in_place(caller, frame.rsi, object, 0, 0) {
        Ok(handle) => Answer::value(handle),
        Err(_) => {
            // The entry stopped naming what was resolved two statements ago,
            // between two statements of a single-context nucleus.
            crate::memory::note_divergence(b"plan-seal-replace");
            Answer::status(E_LIMIT)
        }
    }
}

/// Hands the creator authority over what it made, and tells it which child that
/// is.
///
/// The rights are exactly the ones the authority the creator used carried. More
/// would be authority nobody granted it; less would be the nucleus deciding how
/// a supervisor supervises.
///
/// **The instance identity is written unconditionally**, because operation 19
/// replaces operation 15 as well as operation 8 and a supervisor cannot restart
/// what it cannot name. `rdx` carries the handle, and a handle is not an
/// identity: it is an index in one table and means nothing in another
/// (ADR-0067 §7).
fn funded_result(caller: usize, child: usize, rights: u32) -> Answer {
    let Some(generation) = crate::process::generation(child) else {
        // SAFETY: the child was created by this call and has never run.
        unsafe { crate::process::terminate(caller, child) };
        crate::memory::note_divergence(b"funded-child-generation");
        return Answer::status(E_LIMIT);
    };
    let object = Object::Process {
        slot: child as u32,
        generation,
    };
    let Ok(handle) = capability::grant(caller, object, rights, 0) else {
        // The child exists and the caller cannot name it. Ending it is the only
        // honest response: a process nobody holds authority over is a process
        // nobody can stop.
        // SAFETY: as above.
        unsafe { crate::process::terminate(caller, child) };
        return Answer::status(E_LIMIT);
    };
    if !crate::process::write_created_instance(caller, child) {
        // The caller cannot be told which child it made, so it does not get one.
        // SAFETY: as above.
        unsafe { crate::process::terminate(caller, child) };
        return Answer::status(E_BAD_ARGUMENT);
    }
    Answer::value(handle)
}

/// What a creation that could not happen answers.
///
/// A size no `RuntimeMemoryGrant` could ever serve and a module this boot does
/// not have are malformed calls; everything else is a bound that would have been
/// exceeded (ADR-0076 §7).
fn unlaunchable(refused: crate::process::Unlaunchable) -> Answer {
    match refused {
        crate::process::Unlaunchable::NoSuchModule | crate::process::Unlaunchable::BadGrant => {
            Answer::status(E_BAD_ARGUMENT)
        }
        _ => Answer::status(E_LIMIT),
    }
}

/// `process_create_from_bundle` (20).
///
/// **Three capabilities, and the third must be the shared form.** An immutable
/// *affine* region is not sufficient and is refused: a target gets a mapping of
/// its own and its creator keeps one, which is two holders — exactly what the
/// affine form exists to rule out. `share` is the operation that makes a region
/// able to be in two places, and this is the operation that puts it there.
///
/// **The bundle is opaque, and that is the trust boundary rather than an
/// omission.** Ring 0 checks capability and lifecycle facts — the object is
/// live, it is shared, it has a length, the target can be given its own name and
/// window — and reads not one byte of the artifact. It does not parse the
/// format, inspect the entry, verify an image or trust a receipt. A corrupt
/// bundle therefore produces a process that is *successfully created* and then
/// refuses itself before its first instruction; ADR-0073 owns that decision, and
/// turning a target's verdict into this call's status would move it into the
/// nucleus.
fn create_from_bundle(caller: usize, frame: &mut TrapFrame) -> Answer {
    // `r10` is the sealed plan for this operation, because `rdx` carries the
    // bundle: the row's own order, which §5 fixes and this reads.
    let asked = match funded_arguments(caller, frame.rdi, frame.rsi, frame.r10, frame.r9, frame.r8)
    {
        Ok(asked) => asked,
        Err(answer) => return answer,
    };
    // The bundle, after the three the row assigns first. Refusal order is the
    // row's: a caller that may not create, may not spend, or named no sealed
    // plan is answered from those and learns nothing about the artifact.
    let object = match capability::resolve(caller, frame.rdx, tos_launch::RIGHT_READ) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    let Object::SharedRegion { index, generation } = object else {
        return Answer::status(E_NO_CAPABILITY);
    };
    let region = crate::region::RegionId { index, generation };
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    if tree.mode(region) != Ok(crate::region::Mode::Shared) {
        return Answer::status(E_NO_CAPABILITY);
    }
    // A region with nothing in it is not an artifact, and a length that is not
    // whole frames is a region this nucleus did not make.
    match tree.length(region) {
        Ok(length) if length > 0 && (length as u64).is_multiple_of(FRAME_SIZE) => {}
        _ => return Answer::status(E_BAD_ARGUMENT),
    }
    let mut endowment = [capability::Endowment::Own {
        binding: capability::Binding::NONE,
        rights: 0,
    }; tos_launch::MAX_ENDOWMENT as usize + 1];
    let count = match funded_endowment(&asked, &mut endowment) {
        Ok(count) => count,
        Err(answer) => return answer,
    };
    let parent = crate::process::instance(caller);
    // SAFETY: the template was established at boot from validated inputs, and
    // no process is running: this call is the nucleus.
    match unsafe {
        crate::process::create_funded(
            crate::process::Program::Bundle(region),
            asked.funding,
            &endowment[..count],
            parent,
            asked.restart_generation,
        )
    } {
        Ok(child) => funded_result(caller, child, asked.child_rights),
        Err(refused) => unlaunchable(refused),
    }
}

/// `process_create_funded` (19).
///
/// **Two capabilities and no ambient anything.** The creator presents authority
/// over a process and a `MemoryAuthority` with `spend`, and the whole of the
/// child's user-memory footprint — writable data, the rounded runtime arena, the
/// stack, the report region, the argument region and the launch record — is
/// charged to that authority before a frame moves. There is no root to fall back
/// to: the creation core does not know what one is.
///
/// **The authority is not consumed and nothing is inherited.** A creation places
/// a charge against an accounting node; it does not take the capability, and the
/// child receives no name for it. A parent that wants its child to be able to
/// spend from the same node says so, by naming that capability in
/// `CREATE_ENDOWMENT` like any other — which under ADR-0076 §2b gives the child
/// another name for one budget rather than a second reservation.
fn create_funded(caller: usize, frame: &mut TrapFrame) -> Answer {
    let asked = match funded_arguments(caller, frame.rdi, frame.rsi, frame.rdx, frame.r9, frame.r8)
    {
        Ok(asked) => asked,
        Err(answer) => return answer,
    };
    // The module, named by path, whose length is in `r10` — the register the
    // endowment count vacated when the endowment became an object. An ordinal
    // would have fitted a register, which is its only advantage: it names a
    // position in a list nobody published.
    let Some(entry) = module_of(frame.r10) else {
        return Answer::status(E_BAD_ARGUMENT);
    };
    let mut endowment = [capability::Endowment::Own {
        binding: capability::Binding::NONE,
        rights: 0,
    }; tos_launch::MAX_ENDOWMENT as usize + 1];
    let count = match funded_endowment(&asked, &mut endowment) {
        Ok(count) => count,
        Err(answer) => return answer,
    };
    let parent = crate::process::instance(caller);
    // SAFETY: the template was established at boot from validated inputs, and
    // no process is running: this call is the nucleus.
    match unsafe {
        crate::process::create_funded(
            crate::process::Program::Source(entry),
            asked.funding,
            &endowment[..count],
            parent,
            asked.restart_generation,
        )
    } {
        Ok(child) => funded_result(caller, child, asked.child_rights),
        Err(refused) => unlaunchable(refused),
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
        REGION_SHARE => region_share(caller, arguments.first()),
        REGION_FREEZE => region_freeze(caller, arguments.first()),

        _ => Answer::status(E_NOT_SUPPORTED),
    }
}

/// Why a send did not happen.
///
/// Two shapes rather than one status, because the caller does something
/// genuinely different with each. A full queue is the only refusal a blocking
/// sender waits out (`IPC_V1` §7); everything else is an answer to give back,
/// already in the form the caller will return.
enum SendRefused {
    /// The queue is full, and nothing has been consumed.
    QueueFull,
    Refused(Answer),
}

/// One region a send is about to carry, as the sender's side of it resolves.
#[derive(Clone, Copy)]
struct Outbound {
    /// What the queue will hold: the affine form or the shared one.
    object: Object,
    /// The sender's handle, which a linear transfer consumes and a shared one
    /// leaves alone.
    handle: u64,
    /// Whether this transfer is the consuming kind.
    linear: bool,
}

impl Outbound {
    const NONE: Outbound = Outbound {
        object: Object::None,
        handle: 0,
        linear: false,
    };
}

/// One whole send, from resolving what the sender named to the message being
/// queued — or nothing at all.
///
/// **Everything fallible happens before anything is consumed, and the order is
/// the argument.** `IPC_V1` §9.3 requires a failed send to transfer nothing,
/// and once a region can travel that is no longer a statement about capability
/// handles: a linear transfer takes the sender's window away, and a window
/// rebuilt after the fact needs page tables from the reserve and can fail on its
/// own. There is no rollback that is guaranteed to work, so there is no
/// rollback: the queue's room, the receiver's bound, the region's state, the
/// sender's ownership and the sender's lane are all established first, and
/// below the line nothing may refuse.
///
/// It is a function rather than the body of `send` because two callers need it:
/// the operation, and the resumption of a sender that blocked for room. A
/// blocked sender consumed nothing when it blocked, so what runs when room
/// appears is this same transaction again rather than a second, thinner one that
/// forgets what the first was carrying.
fn send_transaction(
    sender: usize,
    endpoint: u32,
    length: u64,
    capabilities: u64,
    regions: u64,
    reply: Option<(Object, u32, u64)>,
) -> Result<(), SendRefused> {
    let refuse = |status: i64| Err(SendRefused::Refused(Answer::status(status)));
    let from = crate::process::arguments_of(sender);
    if from == 0 {
        return refuse(E_BAD_ARGUMENT);
    }
    let mut granted = [(Object::None, 0u32, 0u64); ipc::MAX_TRANSFERRED as usize];
    // A count past the contract's maximum is a **malformed call**, not a
    // resource condition, and it answers `E_BAD_ARGUMENT` for the same reason
    // an oversize payload does: the three bounds of `IPC_V1` §3 are constants
    // the caller knew before it called. `E_LIMIT` is what a *full queue*
    // answers (§9.2), and a caller that could not tell "retry later" from "this
    // call can never work" would be told nothing useful by either — which is
    // exactly the merge `SYSTEM_ABI_V1` §4 forbids for the other pair.
    //
    // A call reserves the last capability slot for the answer, so it carries
    // one fewer of its own than a send does.
    let reserved = u64::from(reply.is_some());
    if length > ipc::MAX_INLINE_BYTES
        || capabilities + reserved > ipc::MAX_TRANSFERRED
        || regions > ipc::MAX_TRANSFERRED_REGIONS
    {
        return refuse(E_BAD_ARGUMENT);
    }
    let count = capabilities as usize;
    if let Err(answer) = resolve_transfers(sender, from, &mut granted[..count]) {
        return Err(SendRefused::Refused(answer));
    }
    if let Some(reply) = reply {
        // The right to answer this call, made now and belonging to nobody yet.
        // It goes in the **last** slot, always, so that a receiver knows where
        // to look without being told how many capabilities the caller chose to
        // send.
        granted[ipc::MAX_TRANSFERRED as usize - 1] = reply;
    }
    let mut outbound = [Outbound::NONE; ipc::MAX_TRANSFERRED_REGIONS as usize];
    let region_count = regions as usize;
    if let Err(answer) = resolve_regions(sender, from, &mut outbound[..region_count]) {
        return Err(SendRefused::Refused(answer));
    }
    // The room, before anything moves. This is the whole reason the transaction
    // is shaped this way.
    match ipc::has_room(endpoint) {
        Ok(()) => {}
        Err(ipc::Refused::Limit) => return Err(SendRefused::QueueFull),
        Err(_) => return refuse(E_BAD_ARGUMENT),
    }
    // The queue is about to become a holder of everything the message carries,
    // and it is a holder that outlives this call: the sender may release its
    // handles, or end, before anybody receives. Taken before anything is
    // consumed, so a refusal here still leaves the sender with everything.
    let queued = if reply.is_some() {
        granted.len()
    } else {
        count
    };
    if capability::retain_in_transit(&granted[..queued]).is_err() {
        return refuse(E_LIMIT);
    }
    for (taken, entry) in outbound[..region_count].iter().enumerate() {
        let Some(region) = entry.object.region() else {
            continue;
        };
        if capability::retain_region_in_transit(region).is_err() {
            for earlier in outbound[..taken].iter() {
                if let Some(region) = earlier.object.region() {
                    capability::release_region_from_transit(region);
                }
            }
            capability::release_from_transit(&granted[..queued]);
            return refuse(E_LIMIT);
        }
    }

    // --- committed from here ---
    // **The message's internal reference is the ownership bridge.** It is taken
    // above, before the sender loses anything, so a linearly transferred region
    // is never reachable by nobody — the sender's mapping and handle go while
    // the queue is already holding it.
    for entry in outbound[..region_count].iter() {
        if !entry.linear {
            continue;
        }
        let Some(region) = entry.object.region() else {
            continue;
        };
        // SAFETY: `unmap_region` asks for one of two things, and both hold
        // here. When the sender is the running process this is the live space
        // and the flush below follows before ring 3 is reached again; when it
        // is a blocked sender being resumed by whoever freed the room, this is
        // not the live space at all. The flush is unconditional rather than
        // conditional on which, because a translation discarded needlessly
        // costs a reload and one kept wrongly is a window that outlived its
        // capability.
        unsafe { crate::process::unmap_region(sender, region.index) };
        // SAFETY: single-context nucleus; nothing else holds the tree.
        let tree = unsafe { crate::memory::authority() };
        if tree.detach(region, sender as u32).is_err() {
            crate::memory::note_divergence(b"region-send-detach");
        }
        if capability::release(sender, entry.handle).is_err() {
            crate::memory::note_divergence(b"region-send-handle");
        }
    }
    if outbound[..region_count].iter().any(|entry| entry.linear) {
        // SAFETY: the live tree is complete and maps this nucleus, and the lane
        // edits above are finished.
        unsafe { crate::paging::AddressSpace::flush() };
    }
    let mut carried = [Object::None; ipc::MAX_TRANSFERRED_REGIONS as usize];
    for (slot, entry) in carried.iter_mut().zip(outbound.iter()) {
        *slot = entry.object;
    }
    // SAFETY: `from` is the physical address of the sending process's argument
    // region, mapped by the launcher and read here through the nucleus's own
    // identity map.
    if unsafe {
        ipc::send(
            endpoint,
            from,
            length,
            &granted[..queued],
            &carried[..region_count],
        )
    }
    .is_err()
    {
        // Ruled out above: the room was established, the bounds were checked,
        // and the endpoint resolved a moment ago in a nucleus nothing else runs
        // in. Reaching here is the queue contradicting its own preflight, and
        // the region is already the message's.
        crate::memory::note_divergence(b"send-after-preflight");
        return refuse(E_LIMIT);
    }
    Ok(())
}

/// Sends the caller's message, waiting for room when there is none.
///
/// Two things happen after a message is queued, and the second is what makes
/// blocking worth having: if somebody is waiting for a message on this
/// endpoint, it is handed to them here and their call is answered — they do not
/// wake up to ask again. That is two copies of the payload, sender to queue and
/// queue to receiver, which is what docs/35 budgets for an inline message.
///
/// **A sender that waits for room has given up nothing.** `IPC_V1` §7 lets a
/// full queue block the sender, and the transaction above is arranged so that
/// blocking happens before any of the message has been taken from it: the
/// handles it named are still its own, the regions it named are still mapped in
/// it, and when room appears the whole transaction runs again.
fn send(caller: usize, endpoint: u32, frame: &mut TrapFrame) -> Answer {
    match send_transaction(caller, endpoint, frame.rsi, frame.r10, frame.r8, None) {
        Ok(()) => {
            deliver_to_waiter(endpoint);
            Answer::status(OK)
        }
        Err(SendRefused::Refused(answer)) => answer,
        Err(SendRefused::QueueFull) if frame.rdx & NON_BLOCKING != 0 => Answer::status(E_LIMIT),
        Err(SendRefused::QueueFull) => {
            // `IPC_V1` §7: the system never grows a queue to accept a message,
            // so the sender waits for room rather than the queue making some.
            // SAFETY: this is the running context's own frame, and the handle
            // the wait is on was resolved above.
            unsafe {
                crate::process::block(
                    frame,
                    crate::process::Waiting::Room(endpoint),
                    ENDPOINT_SEND as u32,
                )
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
    let reply = (
        Object::Reply {
            caller: caller as u32,
            generation: crate::process::next_reply_token(caller),
        },
        tos_launch::RIGHT_REPLY,
        0,
    );
    match send_transaction(
        caller,
        endpoint,
        frame.rsi,
        frame.r10,
        frame.r8,
        Some(reply),
    ) {
        Ok(()) => {
            deliver_to_waiter(endpoint);
            // SAFETY: this is the running context's own frame, and the handle
            // the wait is on was resolved above.
            unsafe {
                crate::process::block(frame, crate::process::Waiting::Reply, ENDPOINT_CALL as u32)
            }
        }
        Err(SendRefused::QueueFull) => Answer::status(E_LIMIT),
        Err(SendRefused::Refused(answer)) => answer,
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
fn module_of(length: u64) -> Option<usize> {
    let region = crate::process::arguments_region();
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

/// Gives up a capability, and everything that was derived from holding it.
///
/// **A mapping is derived authority, so it cannot outlive the capability it
/// was derived from** (ADR-0075 §5a). Releasing the handle to a region and
/// leaving its window mapped would be the capability model bypassed in one
/// line: the process would keep reading and writing memory it no longer holds
/// any authority over. So the window goes first, then the entry, and ring 3
/// cannot observe a state in between — the nucleus does not return until both
/// are done.
///
/// The order is deliberate. Detaching the mapping is the step that can find an
/// impossible state; if it does, the capability is *not* destroyed, because a
/// caller left holding a handle to memory it can still reach is recoverable
/// and a caller left with neither is not.
fn release_capability(caller: usize, handle: u64) -> Answer {
    let object = match capability::resolve(caller, handle, 0) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    if let Some(region) = object.region() {
        // SAFETY: single-context nucleus; nothing else holds the tree.
        let tree = unsafe { crate::memory::authority() };
        let Ok(length) = tree.length(region) else {
            crate::memory::note_divergence(b"region-release-length");
            return Answer::status(E_LIMIT);
        };
        // **What mode the window is in is the region's answer, not this
        // operation's.** A mutable region is mapped writable, an immutable or
        // shared one read-only, and an operation that assumed either would be
        // right until the first freeze and wrong afterwards. So the expected
        // mode is read out of the per-process mapping record and the lane is
        // checked against *that*.
        let Ok(mapped) = tree.mapped_by(region, caller as u32) else {
            crate::memory::note_divergence(b"region-release-mapping");
            return Answer::status(E_LIMIT);
        };
        let Some(writable) = mapped else {
            // A handle to a region this process does not map. Every path that
            // grants one builds the window first, so this is the table and the
            // region disagreeing rather than anything the caller did.
            crate::memory::note_divergence(b"region-release-unmapped");
            return Answer::status(E_LIMIT);
        };
        // **Whether the window goes with the handle depends on how many
        // handles there are.** One process may hold several shared capabilities
        // for one region and still have exactly one mapping of it; the mapping
        // belongs to the process, not to any one of its names for the region,
        // so it goes when the last of them does and not before. An affine
        // region has exactly one by construction, so this is always its last.
        let last = capability::names_held(caller, object) <= 1;
        let pages = length as u64 / FRAME_SIZE;
        // Everything that could refuse is asked here, while the window, the
        // handle and the region's own counts are all still intact. Below this
        // line nothing can refuse, so there is nothing to roll back — which
        // matters because rebuilding a released lane could itself need reserve
        // frames and fail.
        if last && !crate::process::lane_matches(caller, region.index, pages, writable) {
            crate::memory::note_divergence(b"region-release-lane");
            return Answer::status(E_LIMIT);
        }

        // --- committed from here ---
        if last {
            // SAFETY: the caller's space is the live one, and the flush follows
            // before this returns to ring 3.
            unsafe { crate::process::unmap_region(caller, region.index) };
            // SAFETY: the tree is complete and maps this nucleus.
            unsafe { crate::paging::AddressSpace::flush() };
            if tree.unmap(region, caller as u32, writable).is_err() {
                crate::memory::note_divergence(b"region-mapping-count");
            }
        }
    }
    match capability::release(caller, handle) {
        Ok(()) => {
            drain_reclaims();
            Answer::status(OK)
        }
        Err(refused) => refused.into(),
    }
}

/// The consuming mutable-to-immutable transition (operation 18, ADR-0075 §3).
///
/// **One capability slot, one new generation, and no second name in between.**
/// The caller's handle stops resolving and a new one naming the same region
/// takes its place, carrying `read | share` where it carried `read | write`.
/// Granting a fresh capability and releasing the old one would take a second
/// reference the affine region refuses and a second table slot a full table may
/// not have; rewriting the entry where it stands takes neither.
///
/// **The whole lane is judged before the first bit of it moves.** ADR-0075 §3
/// forbids a half-frozen region, so everything that could be wrong — the branch,
/// the page count, every leaf against the backing index, present, user,
/// no-execute, currently writable, nothing mapped past the length — is asked
/// while the region is still completely mutable. Below the line the window is
/// demoted in place at the same addresses over the same frames, the region's
/// state moves, and the handle is rewritten; none of those can refuse.
///
/// Nothing physical moves: no frame, no page table, no charge. A frozen region
/// is the same memory under a different rule.
fn region_freeze(caller: usize, handle: u64) -> Answer {
    let object = match capability::resolve(caller, handle, tos_launch::RIGHT_WRITE) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    // The write right over something that is not an affine region is a right
    // nobody granted over an object this cannot freeze. A shared region reaches
    // here only if it somehow carried `write`, which is exactly the state this
    // refusal exists to keep unreachable.
    let Object::Region { index, generation } = object else {
        return Answer::status(E_NO_CAPABILITY);
    };
    let region = crate::region::RegionId { index, generation };
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    if tree.mode(region) != Ok(crate::region::Mode::Mutable) {
        return Answer::status(E_NO_CAPABILITY);
    }
    if tree.holder(region) != Ok(Some(caller as u32)) || tree.capabilities(region) != Ok(1) {
        crate::memory::note_divergence(b"region-freeze-holder");
        return Answer::status(E_LIMIT);
    }
    let Ok(length) = tree.length(region) else {
        crate::memory::note_divergence(b"region-freeze-length");
        return Answer::status(E_LIMIT);
    };
    let pages = length as u64 / FRAME_SIZE;
    if tree.mapped_by(region, caller as u32) != Ok(Some(true))
        || !crate::process::lane_demotable(caller, index, pages)
    {
        crate::memory::note_divergence(b"region-freeze-lane");
        return Answer::status(E_LIMIT);
    }

    // --- committed from here ---
    // SAFETY: the lane preflighted a moment ago and nothing has run since;
    // `demote_lane` reloads `CR3` itself, so no writable translation survives.
    if !unsafe { crate::process::demote_lane(caller, index, pages) } {
        crate::memory::note_divergence(b"region-freeze-demote");
    }
    if tree.freeze(region, caller as u32).is_err() {
        crate::memory::note_divergence(b"region-freeze-state");
    }
    match capability::replace_in_place(
        caller,
        handle,
        Object::Region { index, generation },
        tos_launch::RIGHT_READ | tos_launch::RIGHT_SHARE,
        0,
    ) {
        Ok(named) => Answer::value(named),
        Err(_) => {
            // The handle resolved two statements ago in a nucleus nothing else
            // runs in. Reaching here is the table contradicting itself, and the
            // region is already immutable — so it is reported rather than
            // undone, and the caller is refused fail-closed.
            crate::memory::note_divergence(b"region-freeze-handle");
            Answer::status(E_LIMIT)
        }
    }
}

/// The consuming immutable-to-shared transition (operation 7, `IPC_V1` §5).
///
/// `share` consumes its argument, so this is the same in-place replacement the
/// freeze performs and for the same reasons: one slot, one new generation, one
/// capability reference throughout. What changes is the region's state — it
/// stops having an exclusive holder — and the rights, which become `read` and
/// nothing else.
///
/// **`RIGHT_SHARE` is what the caller presents and never what it receives.**
/// The share right is the authority to perform this transition once; carrying
/// it onto the result would make a shared region shareable again, which is a
/// transition with nothing left to consume.
///
/// The caller's mapping does not move. Same frames, same address, same
/// read-only window; what has changed is that another process may now be given
/// one of its own.
fn region_share(caller: usize, handle: u64) -> Answer {
    let object = match capability::resolve(caller, handle, tos_launch::RIGHT_SHARE) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    let Object::Region { index, generation } = object else {
        return Answer::status(E_NO_CAPABILITY);
    };
    let region = crate::region::RegionId { index, generation };
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    if tree.mode(region) != Ok(crate::region::Mode::Immutable) {
        return Answer::status(E_NO_CAPABILITY);
    }
    if tree.holder(region) != Ok(Some(caller as u32))
        || tree.capabilities(region) != Ok(1)
        || tree.mapped_by(region, caller as u32) != Ok(Some(false))
    {
        crate::memory::note_divergence(b"region-share-holder");
        return Answer::status(E_LIMIT);
    }

    // --- committed from here ---
    if tree.share(region, caller as u32).is_err() {
        crate::memory::note_divergence(b"region-share-state");
        return Answer::status(E_LIMIT);
    }
    match capability::replace_in_place(
        caller,
        handle,
        Object::SharedRegion { index, generation },
        tos_launch::RIGHT_READ,
        0,
    ) {
        Ok(named) => Answer::value(named),
        Err(_) => {
            crate::memory::note_divergence(b"region-share-handle");
            Answer::status(E_LIMIT)
        }
    }
}

/// Each way a region's construction can fail, driven once, with the machine
/// measured on both sides.
///
/// Wrapped here rather than driven from the boot because operation 17 needs a
/// caller: a process with an address space of its own and an authority it was
/// endowed. So the evidence build lets ring 3 ask, and fails the first few
/// asks at named points.
#[cfg(feature = "test-creation-rollback")]
static mut REGION_CASE: usize = 0;

#[cfg(not(feature = "test-creation-rollback"))]
fn region_allocate(caller: usize, handle: u64, bytes: u64) -> Answer {
    region_allocate_inner(caller, handle, bytes)
}

#[cfg(feature = "test-creation-rollback")]
fn region_allocate(caller: usize, handle: u64, bytes: u64) -> Answer {
    use crate::injection::Case;
    const CASES: [(&[u8], Option<Case>, Option<u64>); 6] = [
        (b"zero", None, Some(0)),
        (b"round-overflow", None, Some(u64::MAX)),
        (b"over-budget", None, Some(u64::MAX / 2)),
        (b"pool-mid-backing", Some(Case::RegionPool), None),
        (b"tables-mid-backing", Some(Case::RegionBackingTable), None),
        (b"tables-mid-mapping", Some(Case::RegionMappingTable), None),
    ];
    // SAFETY: single-context nucleus; the only writer is this function.
    let at = unsafe { REGION_CASE };
    if at >= CASES.len() {
        return region_allocate_inner(caller, handle, bytes);
    }
    // SAFETY: as above.
    unsafe { REGION_CASE = at + 1 };
    let (name, case, size) = CASES[at];
    let before = crate::memory::snapshot(caller);
    if let Some(case) = case {
        crate::injection::arm(case);
    }
    let answer = region_allocate_inner(caller, handle, size.unwrap_or(bytes));
    crate::injection::disarm();
    crate::memory::report_rollback(b"TOS.RUN.REGION_ROLLBACK", name, &before, caller);
    answer
}

/// Allocates a region out of a memory authority (operation 17).
///
/// **A transaction, and the order is the argument.** Everything knowable is
/// decided before anything moves: the authority and its right, the size and its
/// rounding, a free region slot, a free capability slot, and that the caller has
/// an address space of its own to map into. Then the authority is charged, the
/// backing is laid down a frame at a time, the caller's lane is mapped from
/// exactly those frames, and only then does a capability naming it exist.
///
/// **The region is never unreachable.** It is born holding one construction
/// reference — the nucleus's, for as long as it is building — and that goes
/// only once the caller's mapping is in place, which is itself a way of
/// reaching it. There is no instant at which the three counts are all zero and
/// something could retire it out from under the transaction.
///
/// A failure at any point puts back everything: the frames to the pool, the
/// backing lane's tables and the process lane's tables to the reserve, and the
/// charge to the authority. Ring 3 sees the whole region or nothing.
#[cfg_attr(feature = "test-creation-rollback", allow(clippy::needless_return))]
fn region_allocate_inner(caller: usize, handle: u64, bytes: u64) -> Answer {
    let object = match capability::resolve(caller, handle, tos_launch::RIGHT_SPEND) {
        Ok(object) => object,
        Err(refused) => return refused.into(),
    };
    let Object::MemoryAuthority { index, generation } = object else {
        return Answer::status(E_NO_CAPABILITY);
    };
    if bytes == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    // A borrowed context runs in the nucleus's own address space, and a user
    // window there is not a region, it is a hole in the boundary.
    if !crate::process::owns_space(caller) || !capability::has_room(caller) {
        return Answer::status(E_LIMIT);
    }
    let region_area = crate::process::arguments_region();
    if region_area == 0 {
        return Answer::status(E_BAD_ARGUMENT);
    }
    let authority = crate::region::AuthorityId { index, generation };

    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    let region = match tree.allocate_rounded(
        authority,
        bytes as usize,
        FRAME_SIZE as usize,
        caller as u32,
    ) {
        Ok(region) => region,
        Err(crate::region::Refusal::Empty | crate::region::Refusal::BadArgument) => {
            return Answer::status(E_BAD_ARGUMENT);
        }
        Err(_) => return Answer::status(E_LIMIT),
    };
    let length = tree.length(region).unwrap_or(0) as u64;
    let pages = length / FRAME_SIZE;
    let lane = crate::process::region_lane(region.index);

    // The backing, one arbitrary frame at a time. Physical contiguity is not a
    // region's business, and demanding it would refuse an authority that has
    // the memory in pieces.
    // SAFETY: single-context nucleus; nothing else holds these.
    let frames = unsafe { crate::memory::frames() };
    // SAFETY: as above.
    let tables = unsafe { crate::memory::tables() };
    // SAFETY: as above; the index exists from boot.
    let Some(backing) = (unsafe { crate::memory::backing() }) else {
        return unbuild(caller, region, 0, false, E_LIMIT);
    };
    let mut built = 0;
    while built < pages {
        #[cfg(feature = "test-creation-rollback")]
        if crate::injection::armed(crate::injection::Case::RegionPool) && built == 1 {
            return unbuild(caller, region, built, false, E_LIMIT);
        }
        #[cfg(feature = "test-creation-rollback")]
        if crate::injection::armed(crate::injection::Case::RegionBackingTable) && built == 1 {
            return unbuild(caller, region, built, false, E_LIMIT);
        }
        let Some(frame) = frames.allocate_frame() else {
            // The authority paid for these bytes and the accounting says the
            // pool has them, so it not having them is the two accounts
            // disagreeing rather than a caller asking for too much.
            crate::memory::note_divergence(b"region-backing-pool");
            return unbuild(caller, region, built, false, E_LIMIT);
        };
        if backing.construct(tables, lane, built, frame).is_err() {
            // SAFETY: the frame was handed over a moment ago and nothing
            // named it.
            unsafe { frames.release_frame(frame) };
            crate::memory::note_divergence(b"region-backing-table");
            return unbuild(caller, region, built, false, E_LIMIT);
        }
        built += 1;
    }

    // The caller's window, from exactly those frames.
    #[cfg(feature = "test-creation-rollback")]
    if crate::injection::armed(crate::injection::Case::RegionMappingTable) {
        return unbuild(caller, region, built, false, E_LIMIT);
    }
    if crate::process::map_region(caller, region.index, pages, true).is_err() {
        crate::memory::note_divergence(b"region-mapping-table");
        return unbuild(caller, region, built, false, E_LIMIT);
    }
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    if tree.map(region, caller as u32, true).is_err() {
        return unbuild(caller, region, built, true, E_LIMIT);
    }
    // A reused lane must not answer with what the last one held.
    // SAFETY: the live tree is complete and maps this nucleus; the edit above
    // is finished.
    unsafe { crate::paging::AddressSpace::flush() };

    // **The capability first, and construction lets go afterwards.** Dropping
    // the construction reference before the handoff was committed left one
    // failure — an impossible internal state — able to return a refusal while a
    // live user mapping existed that nothing could name. Now the caller has its
    // handle before the nucleus stops holding the region, and the counts never
    // pass through zero on the way: the mapping alone would have kept it alive
    // in between anyway.
    let granted = capability::grant(
        caller,
        Object::Region {
            index: region.index,
            generation: region.generation,
        },
        tos_launch::RIGHT_READ | tos_launch::RIGHT_WRITE,
        0,
    );
    let Ok(named) = granted else {
        // The preflight proved a slot was free and the region proved it had no
        // capability, so reaching here is the capability table and the region
        // disagreeing rather than a caller asking for too much. Undone whole,
        // and said so.
        crate::memory::note_divergence(b"region-handoff");
        let _ = tree.unmap(region, caller as u32, true);
        return unbuild(caller, region, built, true, E_LIMIT);
    };
    // Now the caller can name it, the nucleus need not.
    if tree.release_internal(region).is_err() {
        // The region is nameable, mapped and usable; what is lost is a
        // reference the accounting will show. Better a leak the numbers report
        // than a refusal for a region the caller already holds.
        crate::memory::note_divergence(b"region-construction-ref");
    }

    // SAFETY: the argument region is this process's own, mapped by the
    // launcher, and the nucleus writes it through its own identity map.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::RegionAllocateRecord>(
            (region_area + tos_launch::REGION_ALLOCATE_RECORD) as usize,
        )
        .write(tos_launch::RegionAllocateRecord { base: lane, length })
    };
    Answer::value(named)
}

/// Undoes a region under construction, whatever it got as far as.
///
/// Construction fills a lane from page zero, so the count is the record and no
/// list of frames is needed. The charge goes back last, after the frames, which
/// is the same order every other retirement observes.
fn unbuild(
    caller: usize,
    region: crate::region::RegionId,
    built: u64,
    mapped: bool,
    status: i64,
) -> Answer {
    let lane = crate::process::region_lane(region.index);
    // SAFETY: single-context nucleus; nothing else holds these.
    let frames = unsafe { crate::memory::frames() };
    // SAFETY: as above.
    let tables = unsafe { crate::memory::tables() };
    if mapped {
        // SAFETY: the caller's space is the live one; the flush below follows.
        unsafe { crate::process::unmap_region(caller, region.index) };
    }
    // SAFETY: as above; the index exists from boot.
    if let Some(backing) = unsafe { crate::memory::backing() } {
        // SAFETY: nothing outside this transaction ever mapped these frames.
        unsafe { backing.discard(tables, frames, lane, built) };
    }
    // **The transaction finishes its own retirement.** It has just put the
    // backing back itself, so leaving the region to the general drain would
    // have that drain look for a lane this rollback has already emptied — and
    // an index that does not describe what it should is a divergence, which is
    // exactly what a rollback must not manufacture. So the construction
    // reference goes, the receipt is taken here, and the charge is credited by
    // the code that did the physical work.
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let tree = unsafe { crate::memory::authority() };
    if tree.release_internal(region).is_err() {
        crate::memory::note_divergence(b"region-rollback-ref");
        return Answer::status(status);
    }
    match tree.take_reclaim(region) {
        Ok(ticket) => {
            if tree.finish_reclaim(ticket).is_err() {
                crate::memory::note_divergence(b"region-rollback-finish");
            }
        }
        Err(_) => crate::memory::note_divergence(b"region-rollback-receipt"),
    }
    Answer::status(status)
}

/// Retires every region nothing can reach any more.
///
/// Physical first and accounting second: the frames go back to the pool and the
/// lane's tables to the reserve, and only the receipt from that work credits
/// the authority.
pub fn drain_reclaims() {
    loop {
        // SAFETY: single-context nucleus; nothing else holds the tree.
        let tree = unsafe { crate::memory::authority() };
        let Some(region) = tree.reclaimable() else {
            return;
        };
        let Ok(ticket) = tree.take_reclaim(region) else {
            return;
        };
        let pages = tree.reclaim_bytes(&ticket) as u64 / FRAME_SIZE;
        let lane = crate::process::region_lane(region.index);
        // SAFETY: single-context nucleus; nothing else holds these.
        let frames = unsafe { crate::memory::frames() };
        // SAFETY: as above.
        let tables = unsafe { crate::memory::tables() };
        // SAFETY: as above.
        let Some(backing) = (unsafe { crate::memory::backing() }) else {
            crate::memory::note_divergence(b"region-reclaim-index");
            return;
        };
        // SAFETY: every process that mapped these frames has had its lane
        // released or its address space destroyed.
        if unsafe { backing.drain(tables, frames, lane, pages) }.is_err() {
            // The index does not describe what it should. Nothing is credited:
            // stranded memory the accounting shows is safer than free memory
            // the pool does not have.
            crate::memory::note_divergence(b"region-backing-corrupt");
            return;
        }
        // SAFETY: as above.
        let tree = unsafe { crate::memory::authority() };
        if tree.finish_reclaim(ticket).is_err() {
            crate::memory::note_divergence(b"region-reclaim-finish");
            return;
        }
    }
}

/// Resolves the generic handles a message carries, in the caller's table.
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
        // **A region does not travel here, in either of its forms.**
        // `IPC_V1` §3 gives regions a bound of their own and §5 gives them
        // rules of their own — a linear transfer for the affine forms, a
        // mapping in the receiver for both — and neither is expressible as a
        // delegated capability. Refused rather than quietly accepted into the
        // generic bound, which would let one message spend the other's.
        if object.is_region() {
            return Err(Answer::status(E_NO_CAPABILITY));
        }
        *entry = (object, capability::rights_of(caller, handle), 0);
    }
    Ok(())
}

/// Resolves the regions a message carries, and decides which of them the send
/// will consume.
///
/// **Every region is validated before any of them is taken.** One message may
/// carry two, and a transfer that consumed the first and then discovered the
/// second was unsendable would be exactly the partial send `IPC_V1` §9.3
/// forbids.
///
/// What each record must be:
///
/// - a handle this process holds, naming a region;
/// - **not** a mutable one. `Region<mut T>` is neither shareable nor
///   transferable (ADR-0037), so a message naming one is refused whole rather
///   than having that record dropped;
/// - if affine: held by this sender, mapped read-only in it, and its lane
///   exactly what the backing index says — because the commit is going to take
///   that lane apart and may not discover anything while doing it;
/// - and named **once**. A linear object cannot be consumed twice, and a
///   message naming one affine region in both records is asking for exactly
///   that. Two records naming one *shared* region are two names for something
///   that has no owner to duplicate, and are admissible.
fn resolve_regions(caller: usize, area: u64, into: &mut [Outbound]) -> Result<(), Answer> {
    for index in 0..into.len() {
        // SAFETY: the region area is at a fixed offset in this process's own
        // argument region, whose address the nucleus chose, and `index` is
        // inside the count the caller checked against the contract's maximum.
        let record = unsafe {
            core::ptr::with_exposed_provenance::<tos_launch::MessageRegion>(
                (area + tos_launch::MESSAGE_REGIONS) as usize,
            )
            .add(index)
            .read()
        };
        // The base and the length the sender may have written are its own
        // address and are meaningless anywhere else. Read the handle, ignore
        // the rest: a nucleus that validated them would be validating a number
        // it is about to overwrite.
        let object = capability::resolve(caller, record.handle, 0).map_err(Answer::from)?;
        let Some(region) = object.region() else {
            return Err(Answer::status(E_NO_CAPABILITY));
        };
        // SAFETY: single-context nucleus; nothing else holds the tree.
        let tree = unsafe { crate::memory::authority() };
        let Ok(mode) = tree.mode(region) else {
            return Err(Answer::status(E_NO_CAPABILITY));
        };
        let linear = match mode {
            crate::region::Mode::Mutable => return Err(Answer::status(E_NO_CAPABILITY)),
            crate::region::Mode::Immutable => true,
            crate::region::Mode::Shared => false,
        };
        if into[..index]
            .iter()
            .any(|earlier| earlier.linear && earlier.object == object)
        {
            return Err(Answer::status(E_BAD_ARGUMENT));
        }
        let Ok(length) = tree.length(region) else {
            return Err(Answer::status(E_NO_CAPABILITY));
        };
        let pages = length as u64 / FRAME_SIZE;
        if linear
            && (tree.holder(region) != Ok(Some(caller as u32))
                || tree.mapped_by(region, caller as u32) != Ok(Some(false))
                || !crate::process::lane_matches(caller, region.index, pages, false))
        {
            // The sender does not hold what a linear transfer would move, or
            // its window is not what the region says it is. Refused before
            // anything is taken apart.
            return Err(Answer::status(E_NO_CAPABILITY));
        }
        into[index] = Outbound {
            object,
            handle: record.handle,
            linear,
        };
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
    match ipc::peek(endpoint) {
        Ok(pending) => match accept(caller, endpoint, into, &pending) {
            Some(length) => {
                accept_from_waiter(endpoint);
                Answer::value(length)
            }
            // **The message is still queued.** The receiver could not be given
            // everything it carries — a table with no room, a lane already
            // occupied, a reserve that could not build the window — so nothing
            // was delivered and nothing was consumed. `E_LIMIT` says a declared
            // bound would have been exceeded, and a later attempt, after the
            // receiver has made room, gets the same message.
            None => Answer::status(E_LIMIT),
        },
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

/// What one region of a message costs its receiver, once the receiver has been
/// looked at.
#[derive(Clone, Copy)]
struct Inbound {
    object: Object,
    pages: u64,
    length: u64,
    /// Whether a window has to be built, or one the receiver already has is
    /// reused. One process holding two shared handles to one region still has
    /// one window.
    map: bool,
}

impl Inbound {
    const NONE: Inbound = Inbound {
        object: Object::None,
        pages: 0,
        length: 0,
        map: false,
    };
}

/// Whether a receiver can be given everything a message carries, and what each
/// region will cost it.
///
/// **The whole of the acceptance's fallibility, asked in one place.** Below
/// this the commit writes table entries and page-table leaves and may not
/// refuse, so everything that could is here: enough capability slots for the
/// capabilities *and* the regions, the one-receiver rule, every authority's
/// name count, an address space to map into, each region in the state its
/// transfer requires, a free lane for each window that has to be built, and
/// enough reserve to build them.
fn acceptable(
    receiver: usize,
    pending: &ipc::Pending,
    into: &mut [Inbound; ipc::MAX_TRANSFERRED_REGIONS as usize],
) -> bool {
    let granted = &pending.granted[..pending.granted_count];
    let regions = &pending.regions[..pending.region_count];
    // **What a message costs in table slots is one per object it carries, not
    // one per position it occupies.** A call reserves the last capability slot
    // for its answer whatever else it carries, so its transfer table has gaps;
    // counting positions would demand four slots of a receiver that is being
    // given one reply.
    let named = granted
        .iter()
        .filter(|(object, _, _)| *object != Object::None)
        .count();
    if capability::room(receiver) < named + pending.region_count {
        return false;
    }
    if !capability::can_grant_all(receiver, granted) {
        return false;
    }
    if pending.region_count > 0 && !crate::process::owns_space(receiver) {
        return false;
    }
    let mut tables_wanted = 0u64;
    for (index, object) in regions.iter().enumerate() {
        let Some(region) = object.region() else {
            return false;
        };
        // SAFETY: single-context nucleus; nothing else holds the tree.
        let tree = unsafe { crate::memory::authority() };
        let (Ok(mode), Ok(length)) = (tree.mode(region), tree.length(region)) else {
            return false;
        };
        // The queue holds the form the sender's region was in, and the region
        // must still be in it. An affine one must have arrived by a linear
        // transfer and be owned by nobody; a shared one must still be shared.
        let expected_affine = matches!(*object, Object::Region { .. });
        match mode {
            crate::region::Mode::Immutable if expected_affine => {
                if tree.holder(region) != Ok(None) || tree.capabilities(region) != Ok(0) {
                    return false;
                }
            }
            crate::region::Mode::Shared if !expected_affine => {}
            _ => return false,
        }
        if !tree.can_name(region) {
            return false;
        }
        let Ok(mapped) = tree.mapped_by(region, receiver as u32) else {
            return false;
        };
        // A window this receiver already has, or one an earlier record of this
        // same message is already going to build: either way it is built once.
        let already = into[..index]
            .iter()
            .any(|earlier| earlier.object.region().map(|r| r.index) == Some(region.index));
        let map = mapped.is_none() && !already;
        if map {
            if !crate::process::lane_free(receiver, region.index) {
                return false;
            }
            tables_wanted += crate::process::lane_table_cost(region.index, length as u64);
        } else if mapped == Some(true) {
            // A writable window onto something no longer mutable. Nothing can
            // produce that, and accepting into it would be accepting into a
            // state nothing describes.
            return false;
        }
        into[index] = Inbound {
            object: *object,
            pages: length as u64 / FRAME_SIZE,
            length: length as u64,
            map,
        };
    }
    // SAFETY: single-context nucleus; nothing else holds the reserve.
    if tables_wanted > unsafe { crate::memory::tables() }.remaining() {
        return false;
    }
    true
}

/// Accepts the message [`ipc::peek`] found, or leaves it exactly where it is.
///
/// The order is the transaction: preflight, commit every grant and every
/// mapping, copy the payload and pop, write the receiver's results, and only
/// then let the message stop being a holder. Nothing between the preflight and
/// the pop may refuse, and a preflight that says no returns before any of it.
///
/// Answers how many payload bytes were delivered, or nothing when the message
/// could not be accepted and is still queued.
fn accept(receiver: usize, endpoint: u32, into: u64, pending: &ipc::Pending) -> Option<u64> {
    let mut inbound = [Inbound::NONE; ipc::MAX_TRANSFERRED_REGIONS as usize];
    if !acceptable(receiver, pending, &mut inbound) {
        return None;
    }

    // --- committed from here ---
    let mut handles = [0u64; ipc::MAX_TRANSFERRED_REGIONS as usize];
    let mut bases = [0u64; ipc::MAX_TRANSFERRED_REGIONS as usize];
    let mut mapped_anything = false;
    for (index, entry) in inbound[..pending.region_count].iter().enumerate() {
        let Some(region) = entry.object.region() else {
            continue;
        };
        // SAFETY: single-context nucleus; nothing else holds the tree.
        let tree = unsafe { crate::memory::authority() };
        // Ownership before the window and before the handle, because both ask
        // the region who its holder is.
        if matches!(entry.object, Object::Region { .. })
            && tree.adopt(region, receiver as u32).is_err()
        {
            crate::memory::note_divergence(b"region-receive-adopt");
        }
        if entry.map {
            if crate::process::map_region(receiver, region.index, entry.pages, false).is_err() {
                crate::memory::note_divergence(b"region-receive-map");
            }
            if tree.map(region, receiver as u32, false).is_err() {
                crate::memory::note_divergence(b"region-receive-mapping");
            }
            mapped_anything = true;
        }
        let rights = match entry.object {
            // What comes out of a linear transfer is what went in: the
            // immutable affine form, `read | share`. The receiver may freeze
            // nothing — there is nothing left to freeze — and may share it,
            // which is the transition the rights name.
            Object::Region { .. } => tos_launch::RIGHT_READ | tos_launch::RIGHT_SHARE,
            // And a shared region is `read` and nothing else. Not `share`: the
            // transition that produced it consumed the only thing it could
            // consume, and a right to repeat it would name no operation.
            _ => tos_launch::RIGHT_READ,
        };
        handles[index] =
            capability::grant(receiver, entry.object, rights, 0).unwrap_or_else(|_| {
                crate::memory::note_divergence(b"region-receive-handle");
                0
            });
        bases[index] = crate::process::region_lane(region.index);
    }
    if mapped_anything {
        // SAFETY: the live tree is complete and maps this nucleus, and every
        // lane edit above is finished.
        unsafe { crate::paging::AddressSpace::flush() };
    }

    // The payload, and the message off the queue. Everything the receiver needs
    // now exists in it.
    // SAFETY: `into` is the receiving process's own argument region, mapped by
    // the launcher and written here through the nucleus's own identity map.
    let length = match unsafe { ipc::take(endpoint, into) } {
        Ok(length) => length,
        Err(_) => {
            // The message was there one statement ago in a nucleus nothing else
            // runs in.
            crate::memory::note_divergence(b"receive-after-preflight");
            0
        }
    };
    hand_over(receiver, into, &pending.granted[..pending.granted_count]);
    write_regions(into, &inbound[..pending.region_count], &handles, &bases);
    // The message has stopped being a holder, and it stops **after** the
    // receiver has become one: the order is what keeps a count from passing
    // through zero between the two, which for an authority would mean returning
    // a reservation to its parent and then delivering it, and for a region
    // would mean reclaiming backing the receiver is already mapping.
    capability::release_from_transit(&pending.granted[..pending.granted_count]);
    for entry in inbound[..pending.region_count].iter() {
        if let Some(region) = entry.object.region() {
            capability::release_region_from_transit(region);
        }
    }
    Some(length)
}

/// Writes the receiver's own handles for the capabilities a message carried.
///
/// The receiver gets its own names, in its own table, with their own
/// generations; nothing about the sender's indices is visible to it
/// (`CAPABILITY_V1` §4). Slots the message did not fill are zeroed, and a handle
/// of all zeros names nothing in any table — so a receiver reads the whole table
/// and needs no count beside it.
///
/// Infallible by the time it runs: [`acceptable`] established that every one of
/// these could be granted before anything was dequeued.
fn hand_over(receiver: usize, region: u64, granted: &[(Object, u32, u64)]) {
    for index in 0..ipc::MAX_TRANSFERRED as usize {
        let handle = match granted.get(index) {
            Some((object, rights, scope)) if *object != Object::None => {
                capability::grant(receiver, *object, *rights, *scope).unwrap_or_else(|_| {
                    crate::memory::note_divergence(b"message-grant-after-preflight");
                    0
                })
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
}

/// Writes the receiver's record of each region it was given: its own handle,
/// the address **the nucleus** chose in its address space, and the charged and
/// mapped length.
///
/// Records the message did not fill are zeroed whole, for the reason unfilled
/// capability slots are: a handle of all zeros names nothing, so a receiver
/// reads the whole area and needs no count beside it.
fn write_regions(area: u64, inbound: &[Inbound], handles: &[u64], bases: &[u64]) {
    for index in 0..ipc::MAX_TRANSFERRED_REGIONS as usize {
        let record = match inbound.get(index) {
            Some(entry) if entry.object != Object::None => tos_launch::MessageRegion {
                handle: handles[index],
                base: bases[index],
                length: entry.length,
            },
            _ => tos_launch::MessageRegion::default(),
        };
        // SAFETY: the region area is at a fixed offset in the receiver's own
        // argument region, whose address the nucleus chose, and `index` is
        // inside the contract's maximum.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<tos_launch::MessageRegion>(
                (area + tos_launch::MESSAGE_REGIONS) as usize,
            )
            .add(index)
            .write(record)
        };
    }
}

/// Hands the message just queued to a context waiting for one, if there is one.
///
/// A waiter that cannot accept it stays blocked and the message stays queued:
/// the wait is for a message it can take, and handing it half of one would be
/// worse than making it wait.
fn deliver_to_waiter(endpoint: u32) {
    let Some(waiter) = crate::process::blocked_on(crate::process::Waiting::Message(endpoint))
    else {
        return;
    };
    let into = crate::process::arguments_of(waiter);
    if into == 0 {
        return;
    }
    let Ok(pending) = ipc::peek(endpoint) else {
        return;
    };
    if let Some(length) = accept(waiter, endpoint, into, &pending) {
        // SAFETY: the context is blocked in the receive this answers.
        unsafe { crate::process::wake(waiter, Answer::value(length)) };
    }
}

/// Runs again the whole send transaction of a context that was waiting for
/// room, if there is one.
///
/// **The same transaction, not a thinner one.** A blocked sender gave up
/// nothing when it blocked — its handles are its own, its regions are still
/// mapped in it — so what runs here is `send_transaction` with the arguments
/// the frame still holds. Re-resolving in the sender's table is not resolving
/// it twice: the sender has not run since, so the table is the one the call was
/// made against, and the frame *is* the record of what the call was.
fn accept_from_waiter(endpoint: u32) {
    let Some(waiter) = crate::process::blocked_on(crate::process::Waiting::Room(endpoint)) else {
        return;
    };
    let (length, capabilities, regions) = crate::process::suspended_transfer(waiter);
    match send_transaction(waiter, endpoint, length, capabilities, regions, None) {
        Ok(()) => {
            // SAFETY: the context is blocked in the send this answers.
            unsafe { crate::process::wake(waiter, Answer::status(OK)) };
            deliver_to_waiter(endpoint);
        }
        // Still no room. It goes on waiting, having given up nothing.
        Err(SendRefused::QueueFull) => {}
        // Something else refuses it now. The sender is told rather than left
        // blocked on a wait that has been satisfied and cannot be used.
        Err(SendRefused::Refused(answer)) => {
            // SAFETY: as above.
            unsafe { crate::process::wake(waiter, answer) };
        }
    }
}
