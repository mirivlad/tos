// SPDX-License-Identifier: GPL-3.0-or-later
//! Processes: how they are built, how they take turns, and how they end.
//!
//! ADR-0049 section 3 draws the first line this module implements: **a fault in
//! the nucleus is still the end of the boot; a fault in a process is not.**
//! Before it, every exception was fatal, because there was exactly one thing
//! running and it was the nucleus. Now there can be something else running, and
//! the difference between the two is a single bit of the interrupted frame —
//! the privilege level the fault was taken at.
//!
//! ADR-0049 section 4 draws the second: **round-robin over runnable contexts in
//! one priority band, with a fixed quantum.** There are no priorities, no
//! deadlines, no fair-share accounting and no second processor. A process that
//! never calls anything still loses the processor, because losing it is not
//! something the process participates in.
//!
//! **Two directions, one mechanism each, and they are not the same mechanism.**
//!
//! - *A process takes its turn.* The timer stub saves all fifteen registers and
//!   the five words the processor pushed, in [`TrapFrame`] order, and hands the
//!   handler the frame's address. A switch is then two copies and a `CR3` load:
//!   the interrupted frame goes into the running slot, the next runnable slot's
//!   frame comes into it, and the stub's `iretq` returns into someone else
//!   without knowing it changed its mind. Entering a process for the first time
//!   is the same operation over a frame the launcher wrote rather than one the
//!   processor pushed ([`process_start`]).
//! - *A process ends.* Nothing returns from a process in the ordinary sense, so
//!   the scheduler records where to continue **before** it stops being what
//!   runs, and the death of a process is a jump back into that context. That
//!   one mechanism serves a fault-kill and a process reporting its own
//!   completion (ADR-0054): the same door, opened from two sides.
//!
//! **The table is the only record of who exists.** A slot holds where the
//! process continues, which address space it continues in, what the launcher
//! gave it that must come back, and what the machine's time was spent on it.
//! Nothing about a process is remembered anywhere else, and the frames it holds
//! are read back out of its own page tables when it dies rather than tallied
//! here twice.

use core::ptr::addr_of_mut;

use tos_frames::{Frames, FRAME_SIZE};
use tos_launch::{
    ImageHeader, Launch, LaunchCapability, LaunchUnit, ReportHeader, IMAGE_MAGIC, LAUNCH_VERSION,
};
use tos_runtime::region::Span;

use crate::apic::TrapFrame;
use crate::exception::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::paging::{self, load_root, AddressSpace, PagingRefused};

core::arch::global_asm!(include_str!("process.S"));

/// The callee-saved registers, the stack pointer and the return address — what
/// it takes to continue a call that was interrupted by leaving ring 0.
#[repr(C)]
pub struct Context {
    saved: [u64; 8],
}

impl Context {
    const EMPTY: Context = Context { saved: [0; 8] };
}

extern "C" {
    fn process_start(frame: *const TrapFrame) -> !;
    fn process_capture(context: *mut Context) -> u64;
    fn process_resume(context: *mut Context, value: u64) -> !;
}

/// How many processes may exist at once.
///
/// A fixed nucleus bound over statically reserved slots, not a number from any
/// input: the table costs what it costs whether or not it is full, and a
/// nucleus that grew one per request would allocate in the one place ADR-0049
/// section 5 says it must not. Stage 3 needs two to be true; four is the
/// smallest bound that makes "two" an ordinary case rather than the maximum.
pub const MAX_PROCESSES: usize = 4;

/// `RFLAGS` a process is entered with: reserved bit 1, and `IF`.
///
/// A process runs interruptible, which is what makes it preemptible. Until the
/// timer existed this was `0x2` — correct then, and wrong the moment ADR-0049's
/// timer was enabled.
const USER_RFLAGS: u64 = 0x202;

/// What one process is, as the nucleus holds it.
struct Slot {
    /// Whether this slot names a process, and whether that process is runnable.
    state: State,
    /// The address space it runs in, which is what `CR3` gets.
    root: u64,
    /// That space, when the nucleus built it and therefore has to take it
    /// apart. The test-only excursion runs in the nucleus's own space and owns
    /// no space of its own, which is exactly the difference this records.
    space: Option<AddressSpace>,
    /// Where it continues: everything `iretq` reads, and every register it had.
    frame: TrapFrame,
    /// Its report region, in *physical* addresses — see [`drain_report`].
    report_phys: u64,
    report_length: u64,
    /// Its message slot, in *physical* addresses, for the same reason: the
    /// nucleus reads and writes it through its own identity map, never through
    /// a mapping the process could change.
    arguments_phys: u64,
    /// What the launcher gave it that has to come back when it ends.
    reclaim: Option<Reclaim>,
    /// Timer interrupts taken while **this** process was on the processor.
    ticks: u64,
    /// How many times it was given the processor. Round-robin over two
    /// processes makes this the number of turns each took.
    quanta: u64,
    /// The tick it first ran at and the tick it last ran at. Two processes
    /// whose intervals overlap ran interleaved; two that ran one after the
    /// other could not produce overlapping intervals, which is what makes this
    /// pair evidence rather than decoration.
    first_tick: u64,
    last_tick: u64,
    /// Which of this context's calls is still waiting for an answer.
    ///
    /// Moved by anything that ends a call — the reply, a cancellation, the
    /// context ending — so that the reply capability handed out for it stops
    /// resolving. Single use is then a property of the counter rather than a
    /// flag somebody has to remember to clear.
    reply_generation: u32,
    /// What it is waiting for, while it is blocked.
    ///
    /// `SYSTEM_ABI_V1` §6: blocking is always on a handle the process holds, so
    /// this names the object rather than a condition. There is no
    /// wait-for-anything, because waiting for anything is waiting on authority
    /// nobody granted.
    waiting: Waiting,
    /// Which operation it is blocked *in*, while it is blocked.
    ///
    /// Not derivable from `waiting`: since ADR-0063 two operations wait for a
    /// message on an endpoint — `endpoint_receive` and the receive half of
    /// `endpoint_reply_receive` — so a record that inferred the operation from
    /// what is being waited for would name an operation the process never
    /// called. The audit record says which one, so it is carried.
    blocked_in: u32,
    /// How it ended, once it has.
    ended: Ended,
    /// Whether the next time this context runs, that crossing is an operation's
    /// way back.
    ///
    /// A call that blocked does not return through the edge: it is set down and
    /// picked up later. **And there are two doors it can be picked up by** — the
    /// scheduler entering it, or a timer tick switching to it — which is what
    /// makes this a flag rather than a count taken at one place. Which door is
    /// used depends only on whether whoever woke it went on to block or ran into
    /// a tick, so an instrument that watched one door measured a number that
    /// moved with the interleaving.
    ///
    /// The tick itself is scheduler preemption and `IPC_V1` §8 excludes it. The
    /// operation coming back is not preemption; it merely used that door.
    resumes_an_operation: bool,
    /// Which occupant of this slot this is.
    ///
    /// A slot is reused, and a capability naming a process must not survive the
    /// process to name its successor — that is the same staleness a handle's
    /// generation prevents, one level down, and `CAPABILITY_V1` §3 states it as
    /// a rule: a capability's lifetime is bounded by its object. So the object
    /// carries a generation of its own, and an authority over a process that
    /// has ended stops resolving rather than quietly transferring to whoever
    /// occupies the slot next.
    generation: u32,
    /// Which process this is, for the life of the boot (ADR-0067).
    ///
    /// Not the slot index and not a handle: slots are reused and handles are
    /// indices in one table, while `PROCESS_IDENTITY_V1` §4 requires an
    /// identity that never comes back — "an instance id that came back would
    /// make two different executions indistinguishable in the log".
    instance: u64,
    /// The instance that created it, where one did. The boot process has none.
    parent: u64,
    /// What that creator asserted about the restart lineage, where it asserted
    /// anything. `process_create` (8) asserts nothing and this stays absent:
    /// a zero would be a claim nobody made (ADR-0067 §8).
    restart_generation: u64,
    has_restart_generation: bool,
    /// The ending, kept in the slot until the parent collects it (ADR-0067 §4).
    ///
    /// The storage for the notice is the storage for the process, reserved when
    /// the process was created, which is what makes a notice impossible to lose
    /// without a queue, an allocation or a bound anybody has to choose.
    notice_pending: bool,
    ending_order: u64,
    ended_tick: u64,
}

/// Which process the next `create` names, and which ending the next `retire`
/// numbers. Both are boot-monotonic and neither is ever reused.
static mut NEXT_INSTANCE: u64 = 1;
static mut NEXT_ENDING: u64 = 1;

impl Slot {
    const FREE: Slot = Slot {
        state: State::Free,
        root: 0,
        space: None,
        frame: TrapFrame::ZERO,
        report_phys: 0,
        report_length: 0,
        arguments_phys: 0,
        reclaim: None,
        ticks: 0,
        quanta: 0,
        first_tick: 0,
        last_tick: 0,
        reply_generation: 1,
        waiting: Waiting::Nothing,
        blocked_in: 0,
        resumes_an_operation: false,
        ended: Ended::Fault(0),
        // One, not zero: an object named with a generation nobody wrote is an
        // object nobody was given.
        generation: 1,
        instance: 0,
        parent: 0,
        restart_generation: 0,
        has_restart_generation: false,
        notice_pending: false,
        ending_order: 0,
        ended_tick: 0,
    };
}

/// What a slot is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    /// Nothing has been built here.
    Free,
    /// A process that can be given the processor.
    Runnable,
    /// A process waiting for something a peer must do, and therefore not a
    /// candidate for the processor until that peer does it (ADR-0059).
    Blocked,
    /// A process that has been ended and whose memory has not gone back yet.
    ///
    /// Only a process that was **not** on the processor is ever left here: one
    /// ended while running is retired by the scheduler the moment it resumes,
    /// whereas one ended by a peer is ended inside that peer's system call,
    /// where the nucleus is running on someone else's behalf and the dead
    /// process's address space is not the live one. Its memory goes back at the
    /// next turn of the scheduler's loop, which is the next moment the nucleus
    /// is in its own address space with nothing running.
    Ending,
    /// A process that has ended and whose memory has gone back.
    Over,
}

/// What a blocked context is waiting for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Waiting {
    /// Nothing: the context is not blocked.
    Nothing,
    /// A message on this endpoint.
    Message(u32),
    /// Room in this endpoint's queue.
    Room(u32),
    /// The answer to a call this context made (`IPC_V1` §4). It names no
    /// endpoint because it is not waiting on the endpoint any more — it is
    /// waiting on the one capability it handed out, and whoever holds that
    /// knows exactly which context to answer.
    Reply,
    /// An ending among the direct children of this process instance
    /// (ADR-0067 §6). It names the *relation*, not one child: the waiter asked
    /// about a set that a capability scopes, and any member of that set answers
    /// it. That is still waiting on a handle the process holds, which is what
    /// `SYSTEM_ABI_V1` §6 requires — the alternative it forbids is waiting on
    /// authority nobody granted.
    ChildOf(u64),
}

impl Waiting {
    /// The object it is waiting on.
    fn endpoint(&self) -> u32 {
        match self {
            Waiting::Nothing | Waiting::Reply | Waiting::ChildOf(_) => 0,
            Waiting::Message(endpoint) | Waiting::Room(endpoint) => *endpoint,
        }
    }
}

/// What the launcher gave a process, and what has to come back when it ends.
///
/// Lengths rather than addresses, because every address is a fixed one of this
/// module: what varies between processes is how much of each region there is.
#[derive(Clone, Copy)]
struct Reclaim {
    /// Writable image bytes — data and `.bss` — from [`IMAGE`] plus this.
    data_at: u64,
    data_length: u64,
    record_length: u64,
    grant: Span,
}

/// Every process this nucleus has.
static mut TABLE: [Slot; MAX_PROCESSES] = [Slot::FREE; MAX_PROCESSES];
/// Which slot is on the processor, meaningful while [`RUNNING`] is true.
static mut CURRENT: usize = 0;
/// Where the scheduler continues when the process running now ends.
static mut RETURN: Context = Context::EMPTY;
/// Whether anything is running at CPL 3 at this instant.
///
/// The nucleus is single-context and runs with interrupts masked, so this is
/// read and written from one place in one order. It is the fault handler's only
/// question: a fault at CPL 3 with nothing running would mean the processor
/// reported a privilege level no one in this system had, and that is not a
/// process to kill — it is a nucleus that has lost track of itself.
static mut RUNNING: bool = false;

/// The process table.
///
/// # Safety
///
/// The nucleus is single-context: it runs with interrupts masked except while a
/// process is on the processor, and a process is not the nucleus. So there is
/// never a second borrow of this table in existence, and the one exception is
/// deliberate — the timer handler interrupts nucleus code that is not inside
/// this function, because the only nucleus code that runs with interrupts
/// enabled is the scheduler's own loop between `capture` and `process_start`.
// SAFETY: the caller is nucleus code, which is the only writer, and the
// single-context argument above is why no second borrow can exist.
unsafe fn table() -> &'static mut [Slot; MAX_PROCESSES] {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *addr_of_mut!(TABLE) }
}

/// Why a process ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ended {
    /// It took a fault at CPL 3. The vector is the architecture's.
    Fault(u64),
    /// It said so itself (`process_exit`, ADR-0054). The status is the
    /// process's own claim about its work, never the nucleus's assertion about
    /// it — what the nucleus asserts is that the process exited, and when.
    Exited(u64),
    /// The system could not continue and it was one of the contexts that could
    /// not be continued (ADR-0059). Not a fault, not its own claim, and not
    /// another process's decision — the fourth way, and the only one that is a
    /// statement about the system rather than about the process.
    Deadlocked,
    /// Somebody holding authority over it ended it (`process_terminate`). The
    /// slot names who, because an ending with no author is an ending nobody can
    /// be held to: the three ways a process can end are the architecture's, its
    /// own claim, and another party's decision, and they are never merged.
    Terminated(usize),
}

/// Where a process's parts live in its own address space.
///
/// Fixed addresses rather than a layout the launcher invents per process: the
/// runtime image is linked at [`IMAGE`] and enters at its first byte, so that
/// address is part of the launch boundary and moving it is a versioned change.
pub const IMAGE: u64 = 0x1000_0000;
pub const RECORD: u64 = 0x2000_0000;
pub const SOURCE: u64 = 0x2100_0000;
pub const GRANT: u64 = 0x3000_0000;
pub const STACK: u64 = 0x5000_0000;
pub const REPORT: u64 = 0x6000_0000;
pub const ARGUMENTS: u64 = 0x7000_0000;

/// The most a launch record may occupy. A fixed nucleus bound, not a number
/// from the capsule: what it limits is how much of the nucleus's memory one
/// process's description may cost.
const MAX_RECORD_BYTES: u64 = 256 * 1024;

/// Frames of stack a process is given, and frames its report region holds.
const STACK_FRAMES: u64 = 512;
const REPORT_FRAMES: u64 = 16;
/// The message slot is one frame, of which `IPC_V1` uses 256 bytes. A frame
/// because a mapping is made of frames, not because the payload needs one.
const ARGUMENT_FRAMES: u64 = 1;

/// Where the top of a process's stack is.
const STACK_TOP: u64 = STACK + STACK_FRAMES * FRAME_SIZE;

/// Page-table flags, from the process's side of the boundary.
const PRESENT_USER: u64 = 1 | (1 << 2);
const WRITABLE: u64 = 1 << 1;
const NO_EXECUTE: u64 = 1 << 63;

/// Why a process could not be started. Never a statement about the program.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unlaunchable {
    /// The handoff record declared no runtime image.
    NoRuntimeImage,
    /// The pool could not back the process's memory.
    OutOfFrames,
    /// A mapping could not be made.
    Paging(PagingRefused),
    /// More source units than the launch record can carry.
    TooManyUnits,
    /// The image's first bytes are not a runtime image header of a version this
    /// nucleus knows, or the sections it declares do not fit the bytes it came
    /// with.
    NotARuntimeImage,
    /// Every slot of the process table is taken.
    TooManyProcesses,
    /// The entry index names no unit of this boot's source set.
    NoSuchModule,
}

impl From<PagingRefused> for Unlaunchable {
    fn from(refused: PagingRefused) -> Self {
        Unlaunchable::Paging(refused)
    }
}

/// Empties the running process's report region onto the serial log.
///
/// Called whenever the process enters the nucleus, which is the only time the
/// region is stable and the nucleus is running. That is what keeps the Stage 2
/// property true across the boundary: a line written before a call is on the
/// log before the call returns, so a stage that never returns is still named by
/// the last event.
pub fn drain_report() {
    // SAFETY: single-context nucleus with interrupts masked, and the current
    // slot is the process whose report this is.
    let (base, length) = unsafe {
        let slot = &table()[CURRENT];
        (slot.report_phys, slot.report_length)
    };
    if base == 0 {
        return;
    }
    // SAFETY: `base` is a frame-aligned physical range the launcher allocated
    // for this process and mapped into the nucleus's identity map; the header
    // is written by the process and read here, and only `drained` is written
    // back, which the process never reads.
    let header =
        unsafe { &mut *core::ptr::with_exposed_provenance_mut::<ReportHeader>(base as usize) };
    let written = header
        .written
        .min(length - size_of::<ReportHeader>() as u64);
    let mut at = header.drained;
    let text = base + size_of::<ReportHeader>() as u64;
    while at < written {
        // SAFETY: `at` is inside the region by the bound above.
        let byte = unsafe { core::ptr::with_exposed_provenance::<u8>((text + at) as usize).read() };
        at += 1;
        if byte == b'\n' {
            tos_serial::puts(b"\r\n");
        } else {
            tos_serial::putc(byte);
        }
    }
    header.drained = at;
}

/// Charges one timer tick to the running process and gives the processor to the
/// next runnable one.
///
/// Called only from the timer handler, and only when the interrupt was taken at
/// CPL 3. The switch itself is the whole of ADR-0049 section 4 as this stage
/// needs it: the frame the stub built goes into the running slot, the next
/// slot's frame comes into it, and `CR3` follows. The stub then pops registers
/// and executes `iretq` exactly as it would have — it does not know that what it
/// is reading is now somebody else.
///
/// **Round-robin means the *next* one, not the best one.** The search starts
/// after the running slot and wraps, so with `n` runnable processes each gets
/// one turn in `n`, and with one runnable process the switch is skipped rather
/// than performed onto itself.
///
/// # Safety
///
/// `frame` is the frame the timer stub built on the nucleus's stack, which
/// `iretq` will read after this returns, and the interrupted `CS` names CPL 3 —
/// so there is a current process, and it is the one this frame describes.
// SAFETY: the caller's promise that this is the stub's own frame taken at CPL 3
// is what makes the current slot the frame's owner.
pub unsafe fn preempt(frame: &mut TrapFrame, tick: u64) {
    // SAFETY: single-context nucleus; the handler cannot be re-entered.
    let table = unsafe { table() };
    // SAFETY: as above.
    let current = unsafe { CURRENT };
    let running = &mut table[current];
    if running.state != State::Runnable {
        // A process was interrupted at CPL 3 that the table says is not
        // runnable. Nothing here can be repaired by guessing, and returning
        // without a switch at least resumes what was actually interrupted.
        return;
    }
    running.ticks += 1;
    if running.first_tick == 0 {
        running.first_tick = tick;
    }
    running.last_tick = tick;

    // The next runnable slot, wrapping — searched here rather than in a helper
    // because a helper would have to name the table a second time, and a second
    // `&mut` to it while this one is alive is exactly the thing the
    // single-context argument does not excuse.
    let Some(next) = (1..=MAX_PROCESSES)
        .map(|step| (current + step) % MAX_PROCESSES)
        .find(|index| table[*index].state == State::Runnable)
    else {
        return;
    };
    if next == current {
        return;
    }
    // The second door. A context woken while somebody else was still running is
    // reached by a tick rather than by the scheduler, and the operation it was
    // waiting inside comes back here — so the count is taken here too, or it
    // would move with the interleaving instead of with the work.
    if table[next].resumes_an_operation {
        table[next].resumes_an_operation = false;
        crate::syscall::count_operation_return();
    }
    table[current].frame = *frame;
    *frame = table[next].frame;
    table[next].quanta += 1;
    let root = table[next].root;
    // SAFETY: written here and read by the paths that end a process, all of
    // which run with interrupts masked and none of which is re-entrant.
    unsafe { CURRENT = next };
    // SAFETY: every space in this table maps this nucleus at the addresses it
    // is running at — the stack this handler is on, its text, and the local
    // APIC it has already acknowledged — because each was built by
    // `paging::build` over the same validated map.
    unsafe { load_root(root) };
}

/// User/kernel boundary crossings **out of** the nucleus, through the one door
/// a context is entered by.
///
/// The other direction of `syscall::entries`. Preemption is not here and not
/// there: a tick returns through the timer stub's own `iretq`, which is what
/// `IPC_V1` §8 means by "excluding scheduler preemption" — so the two counters
/// together are exactly the crossings that contract bounds.
static mut ENTRIES: u64 = 0;

/// That count.
pub fn entries() -> u64 {
    // SAFETY: single-context nucleus; the only writer is the scheduler.
    unsafe { ENTRIES }
}

/// The lowest runnable slot, or nothing when the table holds no process that
/// can be given the processor.
fn first_runnable() -> Option<usize> {
    // SAFETY: single-context nucleus; nothing else touches the table.
    let table = unsafe { table() };
    (0..MAX_PROCESSES).find(|index| table[*index].state == State::Runnable)
}

/// Ends the running process because it said so (`process_exit`, ADR-0054).
///
/// Returns `false` when nothing is running at CPL 3, which is a call that
/// cannot have come from a process and therefore is not one.
pub fn exited(status: u64) -> bool {
    // SAFETY: single-context nucleus with interrupts masked.
    if !unsafe { RUNNING } {
        return false;
    }
    // SAFETY: as above; `RUNNING` being true is what says the current slot is a
    // process, and this is the path that ends it.
    unsafe {
        let table = table();
        table[CURRENT].ended = Ended::Exited(status);
        table[CURRENT].state = State::Ending;
        process_resume(addr_of_mut!(RETURN), 1)
    }
}

/// Reports a fault taken at CPL 3 and ends the process that took it.
///
/// Returns `false` when there was no process — in which case the caller has a
/// fault at CPL 3 in a system that is not running one, and the honest response
/// is the fatal path, not a guess about whose fault it was.
pub fn fault(vector: u64, error: u64, rip: u64, cr2: Option<u64>) -> bool {
    // SAFETY: single-context nucleus with interrupts masked.
    if !unsafe { RUNNING } {
        return false;
    }
    tos_serial::puts(b"TOS.RUN.PROCESS_FAULT process=");
    // SAFETY: as above.
    tos_serial::put_u32_decimal(unsafe { CURRENT } as u32);
    tos_serial::puts(b" vector=");
    tos_serial::put_u32_decimal(vector as u32);
    tos_serial::puts(b" error=0x");
    tos_serial::put_hex64(error);
    tos_serial::puts(b" rip=0x");
    tos_serial::put_hex64(rip);
    tos_serial::puts(b" cr2=");
    match cr2 {
        Some(address) => {
            tos_serial::puts(b"0x");
            tos_serial::put_hex64(address);
        }
        None => tos_serial::puts(b"none"),
    }
    tos_serial::puts(b" cpl=3\r\n");
    // SAFETY: `RETURN` was recorded by the scheduler before this process was
    // entered, and `RUNNING` being true is what says so. Nothing has run on
    // that stack since: the process ran on its own, and this handler on the
    // TSS's.
    unsafe {
        let table = table();
        table[CURRENT].ended = Ended::Fault(vector);
        table[CURRENT].state = State::Ending;
        process_resume(addr_of_mut!(RETURN), 1)
    }
}

/// Sets the running context down, waiting for `waiting`, and does not return.
///
/// The frame is the one the system-call stub built, so what is stored is a
/// context indistinguishable from a preempted one — which is the whole point:
/// the scheduler enters it later by the same instruction it enters any other,
/// and the answer to the call that blocked is written into the frame by
/// whoever satisfies the wait.
///
/// # Safety
///
/// `frame` is the frame the stub built for the running process's call, `waiting`
/// is what it waits for, `operation` is the `SYSTEM_ABI_V1` §5 number of the
/// call it is waiting inside, and the caller has resolved the handle the wait is
/// on.
// SAFETY: the caller's promise that this is the running context's own frame is
// what makes storing it storing this process.
pub unsafe fn block(frame: &TrapFrame, waiting: Waiting, operation: u32) -> ! {
    // SAFETY: single-context nucleus with interrupts masked.
    unsafe {
        let table = table();
        table[CURRENT].frame = *frame;
        table[CURRENT].waiting = waiting;
        table[CURRENT].blocked_in = operation;
        table[CURRENT].state = State::Blocked;
        // The scheduler continues where it recorded it would, exactly as it does
        // when a process ends — the difference is in what this slot now says
        // about itself, not in how control gets back.
        process_resume(addr_of_mut!(RETURN), 1)
    }
}

/// The context blocked on `waiting`, if there is one.
pub fn blocked_on(waiting: Waiting) -> Option<usize> {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    (0..MAX_PROCESSES)
        .find(|index| table[*index].state == State::Blocked && table[*index].waiting == waiting)
}

/// The physical address of a context's argument region.
///
/// Asked about a *blocked* context by whoever is satisfying its wait: the
/// message it was waiting for goes where its own call would have put it.
pub fn arguments_of(index: usize) -> u64 {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    if index >= MAX_PROCESSES {
        return 0;
    }
    table[index].arguments_phys
}

/// Which answer this context is still waiting for, or nothing when it is not
/// waiting for one.
///
/// Asked by the capability table: a reply capability names a call, and this is
/// what says whether that call is still the one outstanding.
pub fn reply_token(index: usize) -> Option<u32> {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    if index >= MAX_PROCESSES || table[index].state != State::Blocked {
        return None;
    }
    (table[index].waiting == Waiting::Reply).then_some(table[index].reply_generation)
}

/// The token a call about to block will be answered by.
pub fn next_reply_token(index: usize) -> u32 {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    if index >= MAX_PROCESSES {
        return 0;
    }
    table[index].reply_generation
}

/// The second argument of the call a context is suspended in.
///
/// A blocked sender's payload length was an argument of its `endpoint_send`,
/// and the frame it was suspended in is where that argument still is. Reading
/// it back is not remembering something twice: the frame *is* the record of
/// what the call was.
pub fn suspended_argument(index: usize) -> u64 {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    if index >= MAX_PROCESSES {
        return 0;
    }
    table[index].frame.rsi
}

/// Answers a blocked context's call and makes it runnable again.
///
/// # Safety
///
/// `index` is blocked, and `answer` is the answer to the call it blocked in.
// SAFETY: the caller's promise that this context is blocked in that call is what
// makes writing its frame writing a suspended call's result.
pub unsafe fn wake(index: usize, answer: crate::syscall::Answer) {
    // SAFETY: single-context nucleus; nothing else writes this slot.
    let table = unsafe { table() };
    if index >= MAX_PROCESSES || table[index].state != State::Blocked {
        return;
    }
    answer.into_frame(&mut table[index].frame);
    if table[index].waiting == Waiting::Reply {
        // However this wait ended — answered, cancelled, or the caller taken
        // away — the call it was waiting for is over, so the capability handed
        // out to answer it stops naming anything. One place, so that no path
        // that ends a call can forget.
        table[index].reply_generation = table[index].reply_generation.wrapping_add(1);
    }
    // Whatever it was waiting for, it was waiting *inside* an operation, and
    // whichever door it next runs through is that operation's way out.
    table[index].resumes_an_operation = true;
    table[index].waiting = Waiting::Nothing;
    table[index].state = State::Runnable;
}

/// Whether any context is waiting for something.
fn any_blocked() -> bool {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    (0..MAX_PROCESSES).any(|index| table[index].state == State::Blocked)
}

/// Cancels every block, because nothing can end them (ADR-0059).
///
/// Called at the instant the scheduler finds nothing runnable and something
/// blocked. In Stage 3 that is decidable rather than a guess: one interrupt is
/// routed, it is the timer, and it wakes nobody — so no wait in that state can
/// ever be satisfied. `E_CANCELLED` is accurate and not an approximation: the
/// operation *was* cancelled, and the canceller is the nucleus.
///
/// **Stage 4 must revisit this.** The rule is "nothing runnable *and nothing
/// routed can change that*", and the second half stops being free the day a
/// device interrupt can wake a driver.
fn cancel_every_block() {
    // What is blocked, read out before anything is done about it. `wake` names
    // the table itself, so a borrow held across it would be two live references
    // to one static — the same mistake this module has already made once, and
    // the reason nothing here iterates the table while calling into it.
    let mut waits = [(Waiting::Nothing, 0u32); MAX_PROCESSES];
    {
        // SAFETY: single-context nucleus; nothing is running.
        let table = unsafe { table() };
        for (index, slot) in table.iter().enumerate() {
            if slot.state == State::Blocked {
                waits[index] = (slot.waiting, slot.blocked_in);
            }
        }
    }
    for (index, (waiting, operation)) in waits.iter().enumerate() {
        if *waiting == Waiting::Nothing {
            continue;
        }
        tos_serial::puts(b"TOS.RUN.BLOCK_CANCELLED process=");
        tos_serial::put_u32_decimal(index as u32);
        tos_serial::puts(b" operation=");
        tos_serial::put_u32_decimal(*operation);
        tos_serial::puts(b" endpoint=");
        tos_serial::put_u32_decimal(waiting.endpoint());
        tos_serial::puts(b" reason=no-runnable-context asserted_by=nucleus\r\n");
        // SAFETY: the context is blocked, and this is the answer to the call it
        // blocked in.
        unsafe { wake(index, crate::syscall::Answer::cancelled()) };
    }
}

/// Ends every blocked context because cancelling them changed nothing.
///
/// The livelock terminator: the rule above has now fired twice with no message
/// delivered in between, so the contexts are not waiting for something that
/// merely has not happened yet — they are waiting for each other.
fn end_every_block() {
    // SAFETY: single-context nucleus; nothing is running.
    let table = unsafe { table() };
    for slot in table.iter_mut() {
        if slot.state == State::Blocked {
            slot.ended = Ended::Deadlocked;
            slot.state = State::Ending;
        }
    }
}

/// Ends the process in `target` on `by`'s authority.
///
/// The caller has already been checked to hold authority over `target`; this is
/// only the ending. Two cases, and they are genuinely different: a process that
/// is not on the processor is marked and its memory goes back at the scheduler's
/// next turn, while a process ending *itself* cannot be marked and left — the
/// nucleus is running on its stack and there is nothing to return to — so it
/// takes the same door a fault takes.
///
/// Returns `false` when the target names no live process, which is a caller
/// holding authority over something that has already ended.
///
/// # Safety
///
/// `by` and `target` are process slots, and `by` holds a capability naming
/// `target` with the right to terminate it.
// SAFETY: the caller's promise that the authority was checked is what makes this
// an exercise of authority rather than a nucleus killing at its own discretion.
pub unsafe fn terminate(by: usize, target: usize) -> bool {
    // SAFETY: single-context nucleus with interrupts masked.
    let table = unsafe { table() };
    // A blocked context is as much a live process as a runnable one, and ending
    // it is the cancellation path `SYSTEM_ABI_V1` §6 requires of anything that
    // blocks: an unkillable process is an authority the system cannot revoke.
    if target >= MAX_PROCESSES || !matches!(table[target].state, State::Runnable | State::Blocked) {
        return false;
    }
    table[target].ended = Ended::Terminated(by);
    // SAFETY: as above.
    if target == unsafe { CURRENT } {
        // The caller is ending itself. Everything below the resume belongs to a
        // stack that is about to stop being anybody's.
        table[target].state = State::Ending;
        // SAFETY: `RETURN` was recorded by the scheduler before this process
        // was entered, and the caller is running, which is what says so.
        unsafe { process_resume(addr_of_mut!(RETURN), 1) }
    }
    table[target].state = State::Ending;
    true
}

/// Gives the processor to every runnable process, in turn, until none is left.
///
/// This is the scheduler's loop, and it lives at CPL 0: a process is entered by
/// `iretq` from its saved frame, and the only way back here is the end of that
/// process. Between the two, the timer may have moved the processor to a
/// different process any number of times — so the slot that comes back is read
/// from [`CURRENT`] rather than assumed to be the one that was entered.
///
/// A process that ends has its memory taken back **in the nucleus's own address
/// space**: the pool writes to a frame when it clears it, and doing that through
/// a dead process's mappings would mean trusting tables the process could have
/// been running in when it died.
///
/// # Safety
///
/// `nucleus` is an address space that maps this nucleus at the addresses it is
/// running at, and every runnable slot was built by [`create`] or [`admit`].
///
/// The pool is deliberately **not** a parameter. A `&mut Frames` held here
/// would be held across `process_start`, and the first system call the entered
/// process made would take a second borrow of the same pool while this one was
/// still alive. It is taken in `retire`, where no process is running.
// SAFETY: the caller's promise about the nucleus's space is what makes the
// return path survivable; each slot's own space is the launcher's promise.
pub unsafe fn schedule(nucleus: &AddressSpace) {
    // How many times the liveness rule has fired without a message being
    // delivered in between, and what the delivery count was when it last did.
    let mut firings = 0u32;
    let mut deliveries = 0u64;
    loop {
        // Anything that ended since the last turn goes back first, here, where
        // the nucleus's own address space is the live one and nothing is
        // running.
        for index in 0..MAX_PROCESSES {
            // SAFETY: single-context nucleus; nothing else touches the table.
            if unsafe { table()[index].state } == State::Ending {
                // SAFETY: the process is over, the nucleus's space is live, and
                // nothing references what it held.
                unsafe { retire(index) };
            }
        }
        let next = match first_runnable() {
            Some(next) => next,
            // Nothing to run. Whether that is the end of the boot or a system
            // that has stopped depends on whether anybody is waiting, and in
            // Stage 3 that question has an answer rather than a guess.
            None if !any_blocked() => return,
            None => {
                let delivered = crate::ipc::deliveries();
                if firings > 0 && delivered == deliveries {
                    // The rule fired, every block was cancelled, the contexts
                    // ran again — and blocked again without a single message
                    // moving. They are not waiting for something that has not
                    // happened yet; they are waiting for each other.
                    tos_serial::puts(b"TOS.RUN.DEADLOCK asserted_by=nucleus\r\n");
                    end_every_block();
                } else {
                    // Consecutive is counted in **deliveries, not in turns**.
                    // A cancelled context becomes runnable and takes a turn
                    // immediately — that is what cancelling it is for — so a
                    // counter reset by "somebody ran" would reset every time
                    // and never reach two. What distinguishes a system that is
                    // making progress from one that is not is whether a message
                    // moved, and nothing else.
                    firings = 1;
                    deliveries = delivered;
                    cancel_every_block();
                }
                continue;
            }
        };
        // The slot is named as a raw place rather than borrowed, and that is
        // not a style choice. The frame's address is read by `process_start`
        // after any borrow would have ended, and the timer handler names this
        // same table while the process runs — so a reference held across that
        // window would be a reference the handler invalidates. A raw pointer
        // has no such claim to make.
        // SAFETY: as above; these are the only writers.
        unsafe {
            CURRENT = next;
            let slot = addr_of_mut!(TABLE).cast::<Slot>().add(next);
            (*slot).quanta += 1;
            // SAFETY: the slot's space maps this nucleus at the addresses it is
            // running at, by the contract of whoever built the slot.
            load_root((*slot).root);
            RUNNING = true;
            // SAFETY: as above; counted before the crossing rather than after,
            // because nothing after it runs until the context comes back.
            ENTRIES += 1;
            if (*slot).resumes_an_operation {
                (*slot).resumes_an_operation = false;
                crate::syscall::count_operation_return();
            }
            if process_capture(addr_of_mut!(RETURN)) == 0 {
                // SAFETY: the frame is this slot's, and the launcher mapped its
                // entry executable and its stack writable in the space just
                // loaded. `process_start` masks interrupts before it points RSP
                // at the frame, so nothing is pushed over the table.
                process_start(&raw const (*slot).frame)
            }
        };
        // Something resumed the captured context. Until blocking existed only
        // the end of a process did that, and the loop could retire what came
        // back; now a context may also have been *set down*, and the difference
        // is a state the slot already carries. So nothing is concluded here —
        // the loop's own top retires whatever ended, and a slot that blocked is
        // simply not runnable this turn.
        // SAFETY: single-context nucleus; nothing else writes this.
        unsafe { RUNNING = false };
        // SAFETY: the caller states this space maps the running nucleus, and it
        // is the space this call arrived in.
        unsafe { nucleus.activate() };
    }
}

/// Reports how one process ended, gives its memory back, and frees its slot.
///
/// # Safety
///
/// The process is over, the nucleus's own address space is the live one, and
/// nothing else references anything the process held.
// SAFETY: the caller's promise that the process is over and its space is not
// live is what makes releasing its frames a release of unreferenced memory.
unsafe fn retire(index: usize) {
    // What it wrote and had not yet said, before its region stops being one.
    // First, and through the table rather than beside it: `drain_report` names
    // the table itself, and holding a borrow of a slot across that call would
    // be two live references to one static.
    drain_report();
    // SAFETY: single-context nucleus; the process is over.
    let slot = unsafe { &mut table()[index] };
    slot.state = State::Over;
    // The process is over, so every capability naming it stops naming anything —
    // authority over it, and the right to answer whatever it was still asking.
    slot.generation = slot.generation.wrapping_add(1);
    slot.reply_generation = slot.reply_generation.wrapping_add(1);
    slot.report_phys = 0;
    slot.report_length = 0;
    slot.arguments_phys = 0;
    // Its authority ends with it, and the generations advance so that nothing
    // written down about the old occupant addresses the next one.
    crate::capability::clear(index);

    match slot.ended {
        // What the nucleus asserts is that the process exited; the status is
        // the process's own claim, and the event says which is which.
        Ended::Exited(status) => {
            tos_serial::puts(b"TOS.RUN.PROCESS_EXIT process=");
            tos_serial::put_u32_decimal(index as u32);
            tos_serial::puts(b" asserted_by=nucleus self_reported_status=");
            tos_serial::put_u32_decimal(status as u32);
            // How much of the machine's time went to this process, counted by
            // the nucleus. A process cannot observe how long it was *off* the
            // processor, so none of this is a number it could have reported.
            tos_serial::puts(b" ticks=");
            tos_serial::put_u32_decimal(slot.ticks as u32);
            tos_serial::puts(b" quanta=");
            tos_serial::put_u32_decimal(slot.quanta as u32);
            tos_serial::puts(b" first_tick=");
            tos_serial::put_u32_decimal(slot.first_tick as u32);
            tos_serial::puts(b" last_tick=");
            tos_serial::put_u32_decimal(slot.last_tick as u32);
            tos_serial::puts(b"\r\n");
        }
        // The fault was reported where it happened, with everything only the
        // handler knew. Saying it again here would be two events for one death.
        Ended::Fault(_) => {}
        // The system could not continue, and this is one of the contexts it
        // could not continue. Named separately from a fault because nothing
        // went wrong inside this process: what failed is the arrangement.
        Ended::Deadlocked => {
            tos_serial::puts(b"TOS.RUN.PROCESS_DEADLOCKED process=");
            tos_serial::put_u32_decimal(index as u32);
            tos_serial::puts(b" operation=");
            tos_serial::put_u32_decimal(slot.blocked_in);
            tos_serial::puts(b" endpoint=");
            tos_serial::put_u32_decimal(slot.waiting.endpoint());
            tos_serial::puts(b" asserted_by=nucleus\r\n");
        }
        // A decision by another party, and the record says whose. The nucleus
        // asserts the whole of this one: nothing here is a process's claim.
        Ended::Terminated(by) => {
            tos_serial::puts(b"TOS.RUN.PROCESS_TERMINATED process=");
            tos_serial::put_u32_decimal(index as u32);
            tos_serial::puts(b" by=");
            tos_serial::put_u32_decimal(by as u32);
            tos_serial::puts(b" ticks=");
            tos_serial::put_u32_decimal(slot.ticks as u32);
            tos_serial::puts(b" quanta=");
            tos_serial::put_u32_decimal(slot.quanta as u32);
            tos_serial::puts(b" asserted_by=nucleus\r\n");
        }
    }

    // ADR-0067: the ending becomes a record before the memory goes back, and
    // the record lives in this slot. Everything the table needs is read out
    // here, because the work below names the table again and a borrow held
    // across that would be two live references to one static.
    let (instance, parent, ended, generation, has_generation) = (
        slot.instance,
        slot.parent,
        slot.ended,
        slot.restart_generation,
        slot.has_restart_generation,
    );
    // SAFETY: single-context nucleus; the handler cannot be re-entered.
    let order = unsafe {
        let order = NEXT_ENDING;
        NEXT_ENDING = NEXT_ENDING.wrapping_add(1);
        order
    };
    slot.ending_order = order;
    slot.ended_tick = crate::apic::ticks();
    // A process that ends stops being a receiver: its own children's pending
    // notices have nobody entitled to them, and anything blocked waiting on its
    // children is waiting on a relation that can gain no further member.
    release_notices_of(instance);
    cancel_child_waiters(instance);
    // The notice is kept only while somebody could still collect it. When the
    // parent is already over, the audit event on the log is the whole of the
    // record and the slot goes straight back (ADR-0067 §10).
    let keep = parent != 0 && live_instance(parent).is_some();
    {
        // SAFETY: single-context nucleus; the process is over.
        let slot = unsafe { &mut table()[index] };
        slot.notice_pending = keep;
    }
    if keep {
        deliver_child_notice(parent);
    }
    let _ = (ended, generation, has_generation);

    let (Some(space), Some(reclaim)) = (slot.space.as_mut(), slot.reclaim) else {
        // A process the nucleus did not build a space for holds nothing of the
        // pool's through that space, and its builder takes back what it lent.
        return;
    };
    // What the process held goes back, cleared on the way (ADR-0050 section 3),
    // and what it holds is read out of its own page tables rather than
    // remembered here: one record of what a process had, and it is the one the
    // processor used.
    //
    // Three ranges are deliberately **not** returned. The image's text and the
    // capsule's source are not the pool's — the loader reserved them — and
    // releasing memory that was never allocated would hand the same frames out
    // twice. The page tables of the dead space are the pool's and are not
    // returned yet: freeing an interior table means proving nothing else under
    // it is mapped, and the nucleus's own mappings live in that same tree.
    // About fifty frames per process, named here rather than left to be found.
    // SAFETY: no process is running — this is the scheduler between two of
    // them — so nothing else holds the pool.
    let frames = unsafe { crate::memory::frames() };
    let held = frames.in_use();
    // SAFETY: every frame below was handed out by this pool for this process,
    // its address space is no longer the live one, and the process that could
    // reach it does not exist.
    unsafe {
        release_mapped(space, frames, reclaim.data_at, reclaim.data_length);
        release_mapped(space, frames, RECORD, reclaim.record_length);
        release_mapped(space, frames, STACK, STACK_FRAMES * FRAME_SIZE);
        release_mapped(space, frames, REPORT, REPORT_FRAMES * FRAME_SIZE);
        release_mapped(space, frames, ARGUMENTS, ARGUMENT_FRAMES * FRAME_SIZE);
        frames.release(reclaim.grant);
    }
    // Measured, not asserted: the pool says how many frames came back and how
    // many it holds now. A reclamation nobody counts is a claim, and this is
    // the number a second process would be built out of.
    tos_serial::puts(b"TOS.RUN.PROCESS_RECLAIMED process=");
    tos_serial::put_u32_decimal(index as u32);
    tos_serial::puts(b" frames=");
    tos_serial::put_u32_decimal((held - frames.in_use()) as u32);
    tos_serial::puts(b" available=");
    tos_serial::put_u32_decimal(frames.available() as u32);
    tos_serial::puts(b"\r\n");
}

/// Which slot a live instance is in, if it is still live (ADR-0067).
///
/// "Live" is anything that has not been retired: a process being ended is still
/// entitled to its children's notices until its own ending is recorded.
fn live_instance(instance: u64) -> Option<usize> {
    if instance == 0 {
        return None;
    }
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    (0..MAX_PROCESSES).find(|index| {
        table[*index].instance == instance
            && matches!(
                table[*index].state,
                State::Runnable | State::Blocked | State::Ending
            )
    })
}

/// Frees the tombstones held for a parent that is itself over (ADR-0067 §10).
///
/// The audit records were emitted at each child's ending and are on the log;
/// what is released is the programmatic notice, which now has no receiver that
/// could ever read it. Holding one would make a slot eternal for the sake of a
/// reader that has ended.
fn release_notices_of(parent_instance: u64) {
    if parent_instance == 0 {
        return;
    }
    // SAFETY: single-context nucleus; nothing is running.
    let table = unsafe { table() };
    for slot in table.iter_mut() {
        if slot.state == State::Over && slot.notice_pending && slot.parent == parent_instance {
            // Said out loud. The audit event for this child's ending is already
            // on the log; what is dropped here is the programmatic notice, and
            // a record that vanished without a line would leave an operator to
            // infer it from a slot count.
            tos_serial::puts(b"TOS.RUN.NOTICE_RELEASED child=");
            tos_serial::put_u32_decimal(slot.instance as u32);
            tos_serial::puts(b" parent=");
            tos_serial::put_u32_decimal(parent_instance as u32);
            tos_serial::puts(b" reason=parent-ended asserted_by=nucleus\r\n");
            slot.notice_pending = false;
            slot.state = State::Free;
        }
    }
}

/// Cancels a wait on the child relation of a process that has ended.
///
/// ADR-0067 §9a: the waiter subscribed to "endings of the direct children of
/// this process", and after that process is over the set can gain no further
/// member — §10 releases what was pending and later children hold nothing. A
/// wait left blocking would be one nothing could ever satisfy, which ADR-0059
/// forbids, and an `OK` with an empty record would be a result that looks like
/// a measurement.
fn cancel_child_waiters(instance: u64) {
    let mut blocked = [false; MAX_PROCESSES];
    {
        // SAFETY: single-context nucleus.
        let table = unsafe { table() };
        for index in 0..MAX_PROCESSES {
            blocked[index] = table[index].state == State::Blocked
                && table[index].waiting == Waiting::ChildOf(instance);
        }
    }
    for (index, waiting) in blocked.iter().enumerate() {
        if !waiting {
            continue;
        }
        tos_serial::puts(b"TOS.RUN.WAIT_CANCELLED process=");
        tos_serial::put_u32_decimal(index as u32);
        tos_serial::puts(b" reason=parent-ended asserted_by=nucleus\r\n");
        // SAFETY: the context is blocked, and this is the answer to the call it
        // blocked in.
        unsafe { wake(index, crate::syscall::Answer::cancelled()) };
    }
}

/// Hands a freshly recorded ending to a parent already blocked for one.
///
/// The wait is satisfied by the operation that produced what it waited for, the
/// same rule `IPC_V1` §7 states for messages: a woken context does not wake up
/// to ask again.
fn deliver_child_notice(parent_instance: u64) {
    let Some(parent) = live_instance(parent_instance) else {
        return;
    };
    let blocked = {
        // SAFETY: single-context nucleus.
        let table = unsafe { table() };
        table[parent].state == State::Blocked
            && table[parent].waiting == Waiting::ChildOf(parent_instance)
    };
    if !blocked {
        return;
    }
    let Some((child, record)) = take_child_notice(parent_instance) else {
        return;
    };
    let answer = match write_child_record(parent, &record) {
        // The record is in the waiter's own region and the identity is its
        // result, exactly as a call that had not blocked would have left them.
        Ok(()) => crate::syscall::Answer::value(record.child_instance),
        // The waiter cannot be told, so the notice is not consumed: it goes
        // back to pending rather than being lost between two frames.
        Err(answer) => {
            restore_child_notice(child);
            answer
        }
    };
    // SAFETY: the context is blocked, and this is the answer to the call it
    // blocked in.
    unsafe { wake(parent, answer) };
}

/// Which process occupies a slot, by the identity of ADR-0067 §7.
///
/// Zero for a slot holding nobody. Public because the edge needs to turn the
/// object a capability named into the identity a record carries: a slot index
/// is not an identity, and the edge must not publish one as if it were.
pub fn instance(index: usize) -> u64 {
    instance_of(index)
}

/// Leaves the child's instance id in the creator's argument region.
///
/// `process_create_with_generation` (15) reports identity here rather than in
/// `rdx`, which already carries the child's capability handle. False when the
/// creator has no region to be told in — refused rather than half-answered.
pub fn write_created_instance(creator: usize, child: usize) -> bool {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    let base = table[creator].arguments_phys;
    if base == 0 || child >= MAX_PROCESSES {
        return false;
    }
    let instance = table[child].instance;
    let at = base + tos_launch::CREATE_INSTANCE_ID;
    // SAFETY: the region is one frame the nucleus mapped for this process and
    // reaches through its own identity map; the offset is inside that frame.
    unsafe { core::ptr::with_exposed_provenance_mut::<u64>(at as usize).write_unaligned(instance) };
    true
}

/// The earliest pending ending among a process object's direct children.
///
/// "Earliest" is by the ending order of ADR-0067 §1 and not by slot position:
/// which of two children died first must not depend on where the table put
/// them, and two observers must agree.
fn take_child_notice(parent_instance: u64) -> Option<(usize, tos_launch::WaitChildRecord)> {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    let mut best: Option<usize> = None;
    for index in 0..MAX_PROCESSES {
        if table[index].state != State::Over
            || !table[index].notice_pending
            || table[index].parent != parent_instance
        {
            continue;
        }
        if best.is_none_or(|found| table[index].ending_order < table[found].ending_order) {
            best = Some(index);
        }
    }
    let index = best?;
    let slot = &mut table[index];
    let (kind, status, has_status, ended_by, has_ended_by) = match slot.ended {
        // The status is the child's own claim (ADR-0054) and travels labelled.
        Ended::Exited(status) => (tos_launch::ENDING_EXITED, status, 1, 0, 0),
        Ended::Fault(_) => (tos_launch::ENDING_FAULTED, 0, 0, 0, 0),
        Ended::Deadlocked => (tos_launch::ENDING_DEADLOCKED, 0, 0, 0, 0),
        // Who ended it, by identity rather than by slot: the slot may already
        // hold somebody else by the time this record is read.
        Ended::Terminated(by) => (tos_launch::ENDING_TERMINATED, 0, 0, instance_of(by), 1),
    };
    let record = tos_launch::WaitChildRecord {
        child_instance: slot.instance,
        parent_instance,
        ending_kind: kind,
        self_reported_status: status,
        has_self_reported_status: has_status,
        ended_by,
        has_ended_by,
        restart_generation: slot.restart_generation,
        has_restart_generation: u64::from(slot.has_restart_generation),
        ending_order: slot.ending_order,
        ended_tick: slot.ended_tick,
    };
    // Collected: the notice is gone and the slot is reusable from this instant
    // and not before. The slot's *generation* was advanced when the process
    // ended, not here, so no capability naming the child comes back to life
    // when the slot is taken by somebody else.
    slot.notice_pending = false;
    slot.state = State::Free;
    Some((index, record))
}

/// Puts a notice back, for the one path that cannot deliver it.
fn restore_child_notice(index: usize) {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    table[index].notice_pending = true;
    table[index].state = State::Over;
}

/// Which instance occupies a slot, or zero for a slot that holds nobody.
fn instance_of(index: usize) -> u64 {
    if index >= MAX_PROCESSES {
        return 0;
    }
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    table[index].instance
}

/// Writes a lifecycle record into a process's own argument region.
///
/// Into the region the nucleus mapped for that process at a fixed offset, never
/// through an address the process supplied: `SYSTEM_ABI_V1` §3's rule that the
/// nucleus walks no pointer a caller chose holds for results as well as for
/// arguments.
fn write_child_record(
    index: usize,
    record: &tos_launch::WaitChildRecord,
) -> Result<(), crate::syscall::Answer> {
    // SAFETY: single-context nucleus.
    let table = unsafe { table() };
    let base = table[index].arguments_phys;
    if base == 0 {
        // A process with no argument region cannot be handed a record. It is
        // refused rather than partially answered.
        return Err(crate::syscall::Answer::status(
            crate::syscall::E_BAD_ARGUMENT,
        ));
    }
    let at = base + tos_launch::WAIT_CHILD_RECORD;
    // SAFETY: the region is one frame the nucleus mapped for this process and
    // reaches through its own identity map, and the record is far inside it:
    // the offset plus the record's size is below the frame's end.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::WaitChildRecord>(at as usize)
            .write_unaligned(*record)
    };
    Ok(())
}

/// `process_wait_child` (ADR-0067): the earliest pending ending, or a block.
///
/// # Safety
///
/// `caller` is the running process and `target` is the instance its capability
/// named.
// SAFETY: the caller's promise that this is the running context is what makes
// setting its frame down and picking it up later a suspended call rather than
// a lost one.
pub unsafe fn wait_child(
    caller: usize,
    frame: &TrapFrame,
    target: u64,
    non_blocking: bool,
) -> crate::syscall::Answer {
    if let Some((child, record)) = take_child_notice(target) {
        return match write_child_record(caller, &record) {
            Ok(()) => crate::syscall::Answer::value(record.child_instance),
            Err(answer) => {
                restore_child_notice(child);
                answer
            }
        };
    }
    if non_blocking {
        return crate::syscall::Answer::status(crate::syscall::E_WOULD_BLOCK);
    }
    let _ = caller;
    // Does not return: the call is set down and picked up when a child ends, is
    // cancelled with the relation (§9a), or is cancelled by the liveness rule.
    // SAFETY: this is the running context's own frame, and the wait is on the
    // relation the resolved capability named.
    unsafe {
        block(
            frame,
            Waiting::ChildOf(target),
            crate::syscall::PROCESS_WAIT_CHILD as u32,
        )
    }
}

/// Which occupant of `index` is there now, or nothing when the slot holds no
/// live process.
///
/// Asked by the capability table: an authority over a process is an authority
/// over *that* process, and this is what says whether that process is still the
/// one in the slot.
pub fn generation(index: usize) -> Option<u32> {
    // SAFETY: single-context nucleus with interrupts masked.
    let table = unsafe { table() };
    if index >= MAX_PROCESSES || table[index].state != State::Runnable {
        return None;
    }
    Some(table[index].generation)
}

/// Which process is on the processor.
///
/// The system-call edge asks this, because a call arrives with no statement of
/// who made it: the caller is whoever the scheduler last gave the processor to,
/// and that is a fact the nucleus keeps rather than one the call carries. A
/// caller that could name itself could name someone else.
pub fn current() -> usize {
    // SAFETY: single-context nucleus with interrupts masked.
    unsafe { CURRENT }
}

/// The physical address of the running process's message slot, or zero when it
/// has none.
pub fn arguments_region() -> u64 {
    // SAFETY: single-context nucleus with interrupts masked.
    unsafe { table()[CURRENT].arguments_phys }
}

/// How the process in `index` ended.
///
/// # Safety
///
/// That slot holds a process that has ended.
// SAFETY: the caller's promise that the process is over is what makes this a
// fact rather than a slot's initial value.
pub unsafe fn ended(index: usize) -> Ended {
    // SAFETY: single-context nucleus.
    unsafe { table()[index].ended }
}

/// Puts a process into the table, ready to be given the processor.
///
/// The frame written here is the first one, and it is the same shape as every
/// frame after it: `iretq` cannot tell a process that has never run from one the
/// timer interrupted, which is what makes entering and resuming one mechanism.
///
/// # Safety
///
/// `root` names a page-table tree that maps this nucleus at the addresses it is
/// running at, maps `entry` user-executable and the page below `stack` writable,
/// and `report` is either `(0, 0)` or a physically contiguous region the nucleus
/// can read through its own identity map.
// SAFETY: the caller's promise about the tree is what makes the frame below
// describe a process that can actually be entered.
#[allow(clippy::too_many_arguments)]
unsafe fn admit(
    root: u64,
    space: Option<AddressSpace>,
    entry: u64,
    stack: u64,
    argument: u64,
    report: (u64, u64),
    message: u64,
    reclaim: Option<Reclaim>,
    parent: u64,
    restart_generation: Option<u64>,
) -> Result<usize, Unlaunchable> {
    // SAFETY: single-context nucleus; nothing else touches the table.
    let table = unsafe { table() };
    let index = match (0..MAX_PROCESSES).find(|index| table[*index].state == State::Free) {
        Some(index) => index,
        None => {
            // `SYSTEM_ABI_V1` §4 has one status for a bound that would be
            // exceeded, and a caller gets `E_LIMIT` whichever bound it was. The
            // log does not have to be that terse: which resource ran out is a
            // fact the nucleus knows and an operator needs, and after ADR-0067
            // one of the two causes is "an ending nobody has collected still
            // holds its slot", which looks like nothing at all from outside.
            let pending = table
                .iter()
                .filter(|slot| slot.state == State::Over && slot.notice_pending)
                .count();
            tos_serial::puts(b"TOS.RUN.PROCESS_REFUSED reason=no-slot uncollected=");
            tos_serial::put_u32_decimal(pending as u32);
            tos_serial::puts(b" asserted_by=nucleus\r\n");
            return Err(Unlaunchable::TooManyProcesses);
        }
    };
    let slot = &mut table[index];
    // The generation survives the reset: it belongs to the slot, not to the
    // process, and it is what keeps an authority over the last occupant from
    // naming this one.
    let generation = slot.generation;
    *slot = Slot::FREE;
    slot.generation = generation;
    // Who this is, for the life of the boot. The counter is the identity: the
    // slot index is reused and would make two executions look like one
    // (`PROCESS_IDENTITY_V1` §4), and a handle is an index in one table.
    // SAFETY: single-context nucleus; nothing else advances this.
    slot.instance = unsafe {
        let instance = NEXT_INSTANCE;
        NEXT_INSTANCE = NEXT_INSTANCE.wrapping_add(1);
        instance
    };
    slot.parent = parent;
    // Recorded, never computed: `PROCESS_IDENTITY_V1` §3 makes the supervisor
    // the asserter, and a nucleus that filled this in for a caller who asserted
    // nothing would be manufacturing a claim (ADR-0067 §8).
    slot.has_restart_generation = restart_generation.is_some();
    slot.restart_generation = restart_generation.unwrap_or(0);
    slot.state = State::Runnable;
    slot.root = root;
    slot.space = space;
    slot.report_phys = report.0;
    slot.report_length = report.1;
    slot.arguments_phys = message;
    slot.reclaim = reclaim;
    slot.frame = TrapFrame {
        rip: entry,
        cs: u64::from(USER_CODE_SELECTOR),
        rflags: USER_RFLAGS,
        rsp: stack,
        ss: u64::from(USER_DATA_SELECTOR),
        // The process's one argument, in the register the C ABI puts a first
        // argument in — because the entry point is a C-ABI function and this is
        // the only thing it is told.
        rdi: argument,
        ..TrapFrame::ZERO
    };
    Ok(index)
}

/// Puts a process into the table that runs in an address space it does not own.
///
/// Test-only, and the difference from [`create`] is exactly one thing: this
/// process borrows the nucleus's address space instead of having one built for
/// it, so nothing of the pool's comes back through its page tables when it ends
/// and its builder takes back what it lent. Everything else is shared — the
/// table, the round-robin, the frame, and the two ways a process can end — which
/// is what makes the evidence it produces evidence about the real mechanism.
///
/// # Safety
///
/// Both addresses are mapped user-accessible in the tree at `root`, which also
/// maps this nucleus at the addresses it is running at, and the GDT, TSS and
/// `syscall` MSRs are installed.
#[cfg(any(
    feature = "test-ring3-abi",
    feature = "test-ring3-privileged",
    feature = "test-ring3-nucleus"
))]
// SAFETY: the caller's promise that the two mappings and the edge exist is what
// makes the process reachable; the scheduler's capture makes its end recoverable.
pub unsafe fn admit_borrowed(
    root: u64,
    entry: u64,
    stack: u64,
    argument: u64,
) -> Result<usize, Unlaunchable> {
    // SAFETY: per this function's contract; the slot carries no space and no
    // reclamation because the process owns neither.
    // The test excursion is nobody's child and nobody asserted a lineage for it.
    unsafe { admit(root, None, entry, stack, argument, (0, 0), 0, None, 0, None) }
}

/// Builds a process and puts it in the table, without entering it.
///
/// Everything the process is made of comes from the pool the nucleus owns and
/// is mapped into an address space the nucleus builds. The process discovers
/// nothing: it is entered at a fixed address with one pointer, and every other
/// address it will ever use is in the record that pointer names.
///
/// **Building is separate from entering, and that is the whole of what makes
/// more than one process possible.** Everything below happens in the *nucleus's*
/// address space, into a space that is not yet live; the process becomes real
/// when the scheduler loads its root, which may be after another process has
/// been built the same way.
///
/// # Safety
///
/// `image` names the verified runtime image bytes and `capsule` the capsule
/// bytes, both physically contiguous and identity-mapped for the nucleus;
/// `descs` is the validated memory map; and the nucleus's own address space is
/// the live one.
// SAFETY: the caller's promise that the image and capsule ranges are what they
// say makes the mappings below name the bytes the identity record claims.
pub unsafe fn create(
    entry_index: usize,
    endowment: &[crate::capability::Endowment],
    parent: u64,
    restart_generation: Option<u64>,
) -> Result<usize, Unlaunchable> {
    let template = crate::launch::template().ok_or(Unlaunchable::NoRuntimeImage)?;
    if !template.holds(entry_index) {
        // A module index this boot does not have. Refused rather than clamped:
        // a process launched over a different module than the one asked for is
        // a process nobody asked for.
        return Err(Unlaunchable::NoSuchModule);
    }
    let (bi, descs) = (template.bi, template.descs);
    let (image, capsule) = (template.image, template.capsule);
    let units = template.units();
    let identity = template.identity;
    let source_set = template.source_set();
    if image.length() == 0 {
        return Err(Unlaunchable::NoRuntimeImage);
    }
    // Taken here and dropped when this returns: building a process is the whole
    // of what the pool is needed for, and no process runs while it happens.
    // SAFETY: nucleus code, and nothing else holds the pool for the duration of
    // this call.
    let frames = unsafe { crate::memory::frames() };
    // The nucleus's own mappings are in every address space, supervisor-only:
    // a syscall and a fault both change privilege without changing CR3, so the
    // nucleus has to be reachable from where the process runs.
    let mut space = paging::build(bi, descs, frames)?;

    // The runtime image, split the way its own header says it is split. A
    // process gets its text read-only and executable and its data writable and
    // not executable, for the same reason the nucleus does: a mapping that is
    // both is a defect. The header is read from the image rather than assumed,
    // and every boundary in it is checked against the bytes that arrived.
    // SAFETY: `image` is the range the loader reserved and the nucleus digested,
    // identity-mapped and at least one frame long.
    let header =
        unsafe { core::ptr::with_exposed_provenance::<ImageHeader>(image.start as usize).read() };
    if header.magic != IMAGE_MAGIC
        || header.entry >= header.text
        || header.text > header.file
        || header.file > header.memory
        || header.text % FRAME_SIZE != 0
        || header.memory % FRAME_SIZE != 0
        || header.text > image.length()
    {
        return Err(Unlaunchable::NotARuntimeImage);
    }
    map_range(
        &mut space,
        frames,
        IMAGE,
        Span::new(image.start, image.start + header.text),
        PRESENT_USER,
    )?;
    // Data and `.bss` are fresh frames, not the loader's copy: two processes
    // will share one image and must not share one writable page, and the file
    // carries no `.bss` at all. What the file does carry is copied in; the rest
    // is what a frame from the pool already is, which is zero.
    let mut offset = header.text;
    while offset < header.memory {
        let frame = frames.allocate_frame().ok_or(Unlaunchable::OutOfFrames)?;
        let carried = image.length().saturating_sub(offset).min(FRAME_SIZE);
        if carried > 0 {
            // SAFETY: `offset + carried` is inside the image range the caller
            // named, and `frame` is a cleared frame this pool just handed out
            // that nothing else references.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    core::ptr::with_exposed_provenance::<u8>((image.start + offset) as usize),
                    core::ptr::with_exposed_provenance_mut::<u8>(frame as usize),
                    carried as usize,
                )
            };
        }
        space.map_page(
            frames,
            IMAGE + offset,
            frame,
            PRESENT_USER | WRITABLE | NO_EXECUTE,
        )?;
        offset += FRAME_SIZE;
    }
    // The source set: read-only, not executable. A process that could write the
    // text it was verified against could execute something else.
    //
    // Mapped from the frame the capsule *starts in*, not from its first byte: a
    // page table addresses frames, so a range beginning mid-frame would be
    // mapped with its low bits silently dropped and every unit inside it would
    // be offset by however far into the frame the capsule began. The skew is
    // added back when each unit's address is computed, below.
    let capsule_frame = capsule.start & !(FRAME_SIZE - 1);
    let skew = capsule.start - capsule_frame;
    map_range(
        &mut space,
        frames,
        SOURCE,
        Span::new(capsule_frame, capsule.end),
        PRESENT_USER | NO_EXECUTE,
    )?;

    // The grant. One region, contiguous, mapped writable and not executable —
    // ADR-0041's property, one address space further out.
    let grant = frames.grant(identity).map_err(|refused| {
        // The other bound, named for the same reason as the slot one: an
        // operator reading `E_LIMIT` cannot tell a full process table from a
        // pool with no contiguous run left, and the two are repaired by
        // different actions — collect the endings, or ask for less memory.
        tos_serial::puts(b"TOS.RUN.PROCESS_REFUSED reason=no-grant available=");
        tos_serial::put_u32_decimal(frames.available() as u32);
        tos_serial::puts(b" asserted_by=nucleus\r\n");
        let _ = refused;
        Unlaunchable::OutOfFrames
    })?;
    let grant_span = Span::new(grant.base as u64, (grant.base + grant.length) as u64);
    map_range(
        &mut space,
        frames,
        GRANT,
        grant_span,
        PRESENT_USER | WRITABLE | NO_EXECUTE,
    )?;

    map_fresh(&mut space, frames, STACK, STACK_FRAMES)?;
    let report = map_fresh(&mut space, frames, REPORT, REPORT_FRAMES)?;
    let message = map_fresh(&mut space, frames, ARGUMENTS, ARGUMENT_FRAMES)?;
    let table_bytes = units.len() * size_of::<LaunchUnit>();
    let paths_bytes: usize = units.iter().map(|(path, _)| relative(path).len()).sum();
    // Room for the endowment's description, sized by what the launcher decided
    // to give rather than by what a process may hold: a record that reserved
    // sixteen entries for an endowment of one would be describing capabilities
    // nobody granted.
    let endowment_bytes = endowment.len() * size_of::<LaunchCapability>();
    let record_bytes = (size_of::<Launch>() + table_bytes + paths_bytes + endowment_bytes) as u64;
    if record_bytes > MAX_RECORD_BYTES {
        return Err(Unlaunchable::TooManyUnits);
    }
    // Sized by the set it carries, not by a frame: a capsule may hold a
    // thousand source files, and a record that fitted only what one frame holds
    // would refuse to launch a machine whose capsule is merely large. Carved
    // contiguously because the nucleus writes it through its own identity map,
    // where one frame at a time and one struct across two frames are different
    // things.
    let record_span = frames
        .carve(record_bytes, FRAME_SIZE)
        .ok_or(Unlaunchable::OutOfFrames)?;
    let record = record_span.start;
    // A carve is not cleared (ADR-0050 section 3 clears on release), and this
    // one becomes a process's memory, so it is cleared here.
    // SAFETY: the run was just carved from the pool, is identity-mapped for the
    // nucleus, and nothing else references it.
    unsafe {
        core::ptr::write_bytes(
            core::ptr::with_exposed_provenance_mut::<u8>(record as usize),
            0,
            record_span.length() as usize,
        )
    };
    let mut mapped = 0;
    while mapped < record_span.length() {
        space.map_page(
            frames,
            RECORD + mapped,
            record + mapped,
            PRESENT_USER | NO_EXECUTE,
        )?;
        mapped += FRAME_SIZE;
    }

    // The record itself, written through the nucleus's identity map and read by
    // the process through its own. Everything it carries lives in one frame:
    // the record, then the unit table, then the paths the units name. The one
    // arithmetic that matters is the last: the tail begins after *the units
    // there are*, not after the most that would fit, and the whole of it is
    // checked against the frame before a single byte is written. Sized against
    // capacity instead, as the first version of this was, the tail begins 16
    // bytes from the end of the frame and the first path walks straight out of
    // it — which it did, into a page table, and the fault it produced named a
    // missing mapping rather than the write that removed it.
    let unit_table = RECORD + size_of::<Launch>() as u64;
    let tail = size_of::<Launch>() as u64 + table_bytes as u64;
    // The endowment's description goes after the paths, so that the two
    // variable-length parts are laid out in the order they were sized in.
    let endowment_at = tail + paths_bytes as u64;
    let mut launch = Launch {
        version: LAUNCH_VERSION,
        unit_count: units.len() as u32,
        entry_index: entry_index as u32,
        grant_version: grant.version,
        grant_base: GRANT,
        grant_length: grant.length as u64,
        grant_identity: grant.identity,
        units: unit_table,
        report_base: REPORT,
        report_length: REPORT_FRAMES * FRAME_SIZE,
        stack_base: STACK,
        stack_length: STACK_FRAMES * FRAME_SIZE,
        arguments_base: ARGUMENTS,
        arguments_length: ARGUMENT_FRAMES * FRAME_SIZE,
        capabilities: RECORD + endowment_at,
        // Patched once the process has a slot to hold the capabilities in: a
        // count written before the grants exist would describe authority the
        // nucleus had not issued.
        capability_count: 0,
        reserved: 0,
        source_set: [0; 96],
    };
    let named = source_set.len().min(launch.source_set.len());
    launch.source_set[..named].copy_from_slice(&source_set[..named]);
    // SAFETY: `record` is a cleared frame this pool just handed out; nothing
    // else references it, and it is identity-mapped for the nucleus.
    unsafe { core::ptr::with_exposed_provenance_mut::<Launch>(record as usize).write(launch) };

    for (index, (path, bytes)) in units.iter().enumerate() {
        // Every unit is inside the capsule, so its address in the process is
        // its offset from the capsule's base. Nothing is copied: the process
        // reads the same bytes whose digest the boot already accounted for.
        let unit = LaunchUnit {
            path: RECORD + tail + path_offset(units, index),
            path_length: relative(path).len() as u64,
            bytes: SOURCE + skew + (bytes.as_ptr() as u64 - capsule.start),
            bytes_length: bytes.len() as u64,
        };
        // SAFETY: the bound checked above covers the whole unit table, so this
        // entry is inside the record's frame, which the nucleus owns and has
        // cleared.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<LaunchUnit>(
                (record + size_of::<Launch>() as u64) as usize,
            )
            .add(index)
            .write(unit)
        };
        // The paths are the capsule's names, which live in the capsule's name
        // arena rather than beside their content, so they are copied into the
        // record's own tail — module-root relative, which is what docs/42
        // section 1 derives a module name from, so the capsule's leading slash
        // is not part of the name.
        let at = record + tail + path_offset(units, index);
        for (offset, byte) in relative(path).iter().enumerate() {
            // SAFETY: the bound checked above covers the record, the whole unit
            // table and every path, so this byte is inside the record's frame.
            unsafe {
                core::ptr::with_exposed_provenance_mut::<u8>(at as usize)
                    .add(offset)
                    .write(*byte)
            };
        }
    }

    // The process is complete and nothing has entered it. The scheduler will,
    // when it is this one's turn — which may be after another process has been
    // built exactly the same way out of the same pool.
    // SAFETY: `space` maps this nucleus at the addresses it is running at, the
    // image executable at `IMAGE`, the stack writable below its top and the
    // record readable at `RECORD`; the edge was installed at nucleus entry.
    let index = unsafe {
        admit(
            space.root(),
            Some(space),
            IMAGE + header.entry,
            STACK_TOP,
            RECORD,
            (report, REPORT_FRAMES * FRAME_SIZE),
            message,
            Some(Reclaim {
                data_at: IMAGE + header.text,
                data_length: header.memory - header.text,
                record_length: record_span.length(),
                grant: grant_span,
            }),
            parent,
            restart_generation,
        )
    }?;

    // The endowment, written after the process has a table to hold it in and
    // before the process is entered — which is the whole of ADR-0055: a process
    // holds what whoever launched it decided, and it holds it before it runs its
    // first instruction. Nothing a process does can add to this.
    // SAFETY: the record was carved with room for exactly this many entries,
    // cleared, and is identity-mapped for the nucleus; nothing else references
    // it, and the process it belongs to has not been entered.
    let described = unsafe {
        let out = core::slice::from_raw_parts_mut(
            core::ptr::with_exposed_provenance_mut::<LaunchCapability>(
                (record + endowment_at) as usize,
            ),
            endowment.len(),
        );
        crate::capability::endow(index, endowment, out)
    };
    // SAFETY: `record` addresses the `Launch` written above, in the nucleus's
    // own identity map, and this field is the only one changed.
    unsafe {
        (&raw mut (*core::ptr::with_exposed_provenance_mut::<Launch>(record as usize))
            .capability_count)
            .write(described)
    };
    // The launcher's decision, on the record, named as a decision. A grant
    // nobody can attribute is ambient authority with a handle in front of it
    // (`CAPABILITY_V1` §3), and a count nobody asserted is a default — which is
    // what nobody decided.
    tos_serial::puts(b"TOS.RUN.PROCESS_ENDOWED process=");
    tos_serial::put_u32_decimal(index as u32);
    tos_serial::puts(b" capabilities=");
    tos_serial::put_u32_decimal(described);
    tos_serial::puts(b" policy=launcher-constant asserted_by=launcher\r\n");
    Ok(index)
}

/// Returns every frame a range of a process's space is mapped to.
///
/// # Safety
///
/// The range is one this pool allocated for that process, the space is no
/// longer live, and nothing else references any of it.
// SAFETY: the caller's promise that these frames are the pool's and unreferenced
// is the whole contract; the translation says which frames those are.
unsafe fn release_mapped(space: &mut AddressSpace, frames: &mut Frames, at: u64, length: u64) {
    let mut offset = 0;
    while offset < length {
        if let Some(frame) = space.translate(at + offset) {
            // Unmapped before it is released, and not only for tidiness: a
            // frame back in the pool with a mapping to it still standing is a
            // frame two owners can reach, and the next owner is a process.
            space.unmap_page(at + offset);
            // SAFETY: per the caller's contract, and the mapping is now gone.
            unsafe { frames.release_frame(frame) };
        }
        offset += FRAME_SIZE;
    }
}

/// A capsule path as a module-root-relative one: what docs/42 section 1
/// derives a module name from, so the capsule's leading slash is not part of it.
fn relative(path: &[u8]) -> &[u8] {
    match path.split_first() {
        Some((b'/', rest)) => rest,
        _ => path,
    }
}

/// Where unit `index`'s path goes in the record's tail.
///
/// **`relative`, not the capsule's own name.** What is written is the
/// module-root-relative path and what is reserved is the sum of those lengths,
/// so a stride taken from the capsule's name — one byte longer for every path
/// that starts with `/` — walks each unit one byte further than the last and
/// puts the final one past the end of what was reserved. What follows the paths
/// is the endowment's description, so the overrun is invisible until a boot has
/// **both** more than one unit and something in its endowment: with an empty
/// endowment there is nothing after the paths to overwrite, which is why the
/// two-module gate never saw it and the first supervisor did.
fn path_offset(units: &[(&[u8], &[u8])], index: usize) -> u64 {
    units[..index]
        .iter()
        .map(|(path, _)| relative(path).len() as u64)
        .sum()
}

/// Maps a physically contiguous range into a process, frame by frame.
fn map_range(
    space: &mut AddressSpace,
    frames: &mut Frames,
    at: u64,
    range: Span,
    flags: u64,
) -> Result<(), Unlaunchable> {
    let mut offset = 0;
    while offset < range.length() {
        space.map_page(frames, at + offset, range.start + offset, flags)?;
        offset += FRAME_SIZE;
    }
    Ok(())
}

/// Takes `count` fresh frames and maps them writable at `at`.
fn map_fresh(
    space: &mut AddressSpace,
    frames: &mut Frames,
    at: u64,
    count: u64,
) -> Result<u64, Unlaunchable> {
    let mut first = 0;
    for index in 0..count {
        let frame = frames.allocate_frame().ok_or(Unlaunchable::OutOfFrames)?;
        if index == 0 {
            first = frame;
        }
        space.map_page(
            frames,
            at + index * FRAME_SIZE,
            frame,
            PRESENT_USER | WRITABLE | NO_EXECUTE,
        )?;
    }
    Ok(first)
}
