// SPDX-License-Identifier: GPL-3.0-or-later
//! The TOS Core runtime, as the thing a process is (ADR-0048).
//!
//! Until Stage 3 the reference path ran inside the nucleus, at CPL 0, on the
//! nucleus's stack. This image is the same path — the same `tos-pipeline`, the
//! same stages, the same verifier, the same engine — moved to where ADR-0048
//! says it belongs: its own address space, its own grant, CPL 3, and one
//! instance per process. Nothing about the pipeline changed to get here, which
//! is the point: if the language had to be adjusted to survive the boundary,
//! the boundary would be wrong.
//!
//! **What this image can and cannot do.** It has memory, because the nucleus
//! granted it; it has source bytes, because the launcher mapped them; and it
//! has the system-call edge. It has no serial port, no memory map, no firmware
//! table and no capsule — a process cannot reach an I/O port at all, and
//! `SYSTEM_ABI_V1` gives it no operation that would hand it one. What it has to
//! say, it writes into the report region the launch record names, and the
//! nucleus relays.

#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;

use tos_launch::{Launch, LaunchCapability, LaunchUnit, ReportHeader, LAUNCH_VERSION};
use tos_pipeline::{execute_set, render, PipelineStage, SetError, SetRequest, Trace, Unit};
use tos_runtime::{stack, GlobalHeap};

/// The heap of this runtime instance: the grant, and nothing else.
///
/// ADR-0041's property survives the move to ring 3 unchanged — a runtime with
/// no grant has no memory — and here it is enforced by two things at once: this
/// allocator refuses until adoption, and the address space contains no other
/// writable mapping to allocate out of.
#[global_allocator]
static HEAP: GlobalHeap = GlobalHeap::new();

/// The entry function the boot module must export.
const ENTRY: &str = "main";

/// Statuses this image reports through `process_exit`. They are its own claim
/// about itself (`PROCESS_IDENTITY_V1` §2, ADR-0054), never the audit record.
const EXIT_COMPLETED: u64 = 0;
const EXIT_REFUSED: u64 = 1;
const EXIT_UNSTARTABLE: u64 = 2;

/// Operations, as `SYSTEM_ABI_V1` §5 assigns them.
const ENDPOINT_SEND: u64 = 1;
const ENDPOINT_RECEIVE: u64 = 2;
const CAPABILITY_ATTENUATE: u64 = 5;
const CAPABILITY_RELEASE: u64 = 6;
const PROCESS_CREATE: u64 = 8;
const PROCESS_TERMINATE: u64 = 9;
const CONTEXT_YIELD: u64 = 10;
const TIME_MONOTONIC: u64 = 11;
const PROCESS_EXIT: u64 = 12;

/// Statuses, as `SYSTEM_ABI_V1` §4 assigns them. Named here because this image
/// checks them: a refusal it could not name it could not report.
const OK: i64 = 0;
const E_NO_CAPABILITY: i64 = -1;
const E_CANCELLED: i64 = -5;

/// The one call flag this ABI version has: ask, do not wait (ADR-0059).
const NON_BLOCKING: u64 = 1;

/// The inline payload bound `IPC_V1` §3 declares (ADR-0057).
const MAX_INLINE_BYTES: u64 = 256;

/// Reads the monotonic tick, or nothing when the nucleus does not offer one.
///
/// The tick is the nucleus's; this process can only ask for it, and asking
/// twice is the only way anything in this system can observe that time passed
/// while it ran.
fn monotonic() -> Option<u64> {
    let value: u64;
    let status: i64;
    // SAFETY: `time_monotonic` is self-only, takes no argument and returns its
    // value in `rdx`; `rcx` and `r11` are clobbered by the instruction and are
    // declared as such.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") TIME_MONOTONIC => status,
            out("rdx") value,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status == 0).then_some(value)
}

/// Makes one system call.
///
/// The first argument is the capability the operation requires, where it
/// requires one (ADR-0056); the second is a value. No pointer crosses this edge:
/// §3 admits values and handles only, which is why a message's payload does not
/// travel here at all — it sits in the slot the launch record names.
///
/// SAFETY: `operation` is an assigned operation number and both arguments are
/// legal for it.
// SAFETY: the caller names an assigned operation; the instruction itself
// touches no memory of this image.
unsafe fn call(operation: u64, first: u64, second: u64) -> (i64, u64) {
    let status: i64;
    let value: u64;
    // The six argument registers are `rdi, rsi, rdx, r10, r8, r9` in that order
    // (§3), so the second argument is `rsi`. `rdx` is the third argument's
    // register on the way in and the *value*'s on the way out, which is why it
    // is an output here and not where `second` goes — a mistake this image made
    // once, and the nucleus read whatever `rsi` happened to hold as a length.
    // SAFETY: `syscall` clobbers `rcx` and `r11` and returns a status in `rax`
    // and a value in `rdx`; all four are declared, and every other register is
    // preserved by the contract this call is made against.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") operation => status,
            in("rdi") first,
            in("rsi") second,
            out("rdx") value,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status, value)
}

/// Ends this process, and does not return.
fn exit(status: u64) -> ! {
    loop {
        // SAFETY: `process_exit` is self-only and takes a status value.
        unsafe { call(PROCESS_EXIT, status, 0) };
        // A nucleus that answered `process_exit` and returned is one this image
        // cannot reason about, so it stops asking for anything else.
        core::hint::spin_loop();
    }
}

/// Where this runtime writes what it has to say.
///
/// The nucleus drains it whenever the process enters the edge, so a line is on
/// the log before the call that follows it returns. That is what keeps the
/// Stage 2 property true across the boundary: a stage that never returns is
/// still named by the last event.
struct Report {
    base: u64,
    capacity: u64,
}

impl Report {
    fn line(&mut self, text: &str) {
        // SAFETY: `report_base` names a writable mapping of `report_length`
        // bytes in this address space, made by the launcher, and this image is
        // its only writer.
        let header = unsafe {
            &mut *core::ptr::with_exposed_provenance_mut::<ReportHeader>(self.base as usize)
        };
        let start = size_of::<ReportHeader>() as u64 + header.written;
        let bytes = text.as_bytes();
        if start + bytes.len() as u64 + 1 > self.capacity {
            // A full region drops the line rather than wrapping over one the
            // nucleus has not read: a log that overwrites itself reports a run
            // that did not happen.
            return;
        }
        for (offset, byte) in bytes.iter().enumerate() {
            // SAFETY: the bound above proves this write is inside the region.
            unsafe {
                core::ptr::with_exposed_provenance_mut::<u8>((self.base + start) as usize)
                    .add(offset)
                    .write(*byte)
            };
        }
        // SAFETY: as above, one byte past the text and still inside the region.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>((self.base + start) as usize)
                .add(bytes.len())
                .write(b'\n')
        };
        header.written += bytes.len() as u64 + 1;
        // Give up the rest of the quantum. With one runnable context that
        // returns immediately, and it is the moment the nucleus is running and
        // the region is stable, so it is also when the line reaches the log.
        // SAFETY: `context_yield` is self-only and takes no argument.
        unsafe { call(CONTEXT_YIELD, 0, 0) };
    }
}

/// Announces each stage as it is entered, before it runs.
struct ReportTrace<'a> {
    report: &'a mut Report,
}

impl Trace for ReportTrace<'_> {
    fn entering(&mut self, stage: PipelineStage) {
        self.report
            .line(&alloc::format!("TOS.RUN.STAGE name={}", stage.symbol()));
    }
}

/// The process's entry point: the nucleus enters here at CPL 3 with the launch
/// record's address in `rdi`.
///
/// # Safety
///
/// The nucleus states that `launch` addresses a readable [`Launch`] in this
/// address space, that every address inside it is mapped as the record
/// describes, and that this is the only context of this process.
// SAFETY: the launcher's promise about the record and its targets is the whole
// contract; the version check below is what makes it a promise about this ABI.
#[no_mangle]
#[link_section = ".text.runtime_entry"]
pub unsafe extern "C" fn runtime_entry(launch: *const Launch) -> ! {
    // SAFETY: the caller's contract makes this a live, aligned record.
    let launch = unsafe { &*launch };
    if launch.version != LAUNCH_VERSION {
        // Nothing can be reported: the report region is described by a record
        // this image does not understand. The status is all there is.
        exit(EXIT_UNSTARTABLE);
    }
    let mut report = Report {
        base: launch.report_base,
        capacity: launch.report_length,
    };

    let grant = tos_runtime::RuntimeMemoryGrant {
        version: launch.grant_version,
        base: launch.grant_base as usize,
        length: launch.grant_length as usize,
        alignment: 4096,
        identity: launch.grant_identity,
    };
    // SAFETY: the launcher granted this region to this process alone and mapped
    // it writable; nothing else in this address space refers to it. Adoption is
    // here, before the first allocation, in the only context there is.
    if unsafe { HEAP.adopt(&grant) }.is_err() {
        report.line("TOS.RUN.UNSTARTABLE reason=heap-rejected-grant");
        exit(EXIT_UNSTARTABLE);
    }

    // The stack is painted before the run and measured after it, which is the
    // same measurement Stage 2 published — taken now on the stack the run
    // actually uses.
    let stack_region =
        tos_runtime::region::Span::new(launch.stack_base, launch.stack_base + launch.stack_length);
    // SAFETY: the launcher mapped this region writable for this process and
    // this frame is running inside it, so painting writes only below itself.
    let painted = unsafe { stack::paint(stack_region) };

    let mut units = alloc::vec::Vec::with_capacity(launch.unit_count as usize);
    for index in 0..launch.unit_count as usize {
        // SAFETY: the record declares `unit_count` units at `units`, and the
        // launcher's contract is that they are mapped readable.
        let unit = unsafe {
            &*core::ptr::with_exposed_provenance::<LaunchUnit>(launch.units as usize).add(index)
        };
        // SAFETY: as above, for the bytes each unit names.
        let (path, bytes) = unsafe {
            (
                core::slice::from_raw_parts(
                    core::ptr::with_exposed_provenance::<u8>(unit.path as usize),
                    unit.path_length as usize,
                ),
                core::slice::from_raw_parts(
                    core::ptr::with_exposed_provenance::<u8>(unit.bytes as usize),
                    unit.bytes_length as usize,
                ),
            )
        };
        let Ok(path) = core::str::from_utf8(path) else {
            report.line("TOS.RUN.UNSTARTABLE reason=unit-path-not-text");
            exit(EXIT_UNSTARTABLE);
        };
        units.push(Unit { path, bytes });
    }
    let Some(entry_unit) = units.get(launch.entry_index as usize) else {
        report.line("TOS.RUN.UNSTARTABLE reason=no-entry-unit");
        exit(EXIT_UNSTARTABLE);
    };
    let entry_path = entry_unit.path;
    let entry_bytes = entry_unit.bytes.len();

    let source_set = core::str::from_utf8(&launch.source_set)
        .unwrap_or("")
        .trim_end_matches('\0');

    report.line(&alloc::format!(
        "TOS.RUN.BEGIN path={entry_path} bytes={entry_bytes} entry={ENTRY} \
         grant_base=0x{:x} grant_length={} grant_version={} modules={}",
        launch.grant_base,
        launch.grant_length,
        launch.grant_version,
        units.len(),
    ));

    let began = monotonic();
    let request = SetRequest {
        source_set: &alloc::string::String::from(source_set),
        units: &units,
        entry_path,
        entry: ENTRY,
    };
    let mut trace = ReportTrace {
        report: &mut report,
    };
    let run = match execute_set(&request, alloc::vec::Vec::new(), &mut trace) {
        Ok(run) => run,
        Err(SetError::EntryModuleAbsent { .. } | SetError::NoUnits) => {
            report.line("TOS.RUN.UNSTARTABLE reason=no-boot-module");
            exit(EXIT_UNSTARTABLE);
        }
    };
    for event in render::events(&run) {
        report.line(&event);
    }

    let (committed, peak) = HEAP.usage();
    let (blocks, free) = HEAP.block_census();
    report.line(&alloc::format!(
        "TOS.RUN.MEMORY granted={} peak={peak} committed={committed} blocks={blocks} free={free}",
        launch.grant_length
    ));
    if let Some(floor) = painted {
        // SAFETY: `stack_region` and `floor` came from the matching `paint`
        // above, on the stack this frame is still running on.
        let used = unsafe { stack::peak(stack_region, floor) };
        report.line(&alloc::format!(
            "TOS.RUN.STACK used={used} capacity={}",
            stack_region.length()
        ));
    }

    // The run itself is over in less than one tick, so the tick after it is the
    // tick before it and nothing has been shown. What is worth showing is that
    // time moves *while this process runs*: the loop below waits for the tick to
    // change, which can only happen if an interrupt was taken at CPL 3 and this
    // process was resumed afterwards. It is bounded, because a process that
    // cannot be interrupted must say so rather than hang.
    if let Some(began) = began {
        let ended = monotonic().unwrap_or(began);
        // Then the same question asked without asking the nucleus anything. The
        // loop below makes no system call at all, so a tick that is larger after
        // it than before it can only have been advanced by an interrupt taken
        // while this process was running its own instructions — which is the
        // claim, and which "the tick moved between two of my calls" does not
        // make. `black_box` is what keeps the loop from being optimised into the
        // nothing it computes.
        let spin_began = monotonic().unwrap_or(0);
        let mut sum = 0u64;
        for step in 0..20_000_000u64 {
            sum = core::hint::black_box(sum.wrapping_add(step));
        }
        let spin_ended = monotonic().unwrap_or(0);
        report.line(&alloc::format!(
            "TOS.RUN.TICKS begin={began} end={ended} spin_begin={spin_began} \
             spin_end={spin_ended}"
        ));
    }

    // What this process was given, and what it does with it. Empty on a
    // canonical boot, because the launcher's constant grants nothing a module
    // did not request (ADR-0055) — and a process with no authority reports that
    // it has none rather than staying silent about it.
    authority(launch, &mut report);

    match run.failed_at() {
        None => exit(EXIT_COMPLETED),
        Some(_) => exit(EXIT_REFUSED),
    }
}

/// Exercises the authority this process was endowed with, and reports what the
/// system answered.
///
/// Everything here is a *question asked of the nucleus*, never an assertion by
/// this image: the process cannot see the capability table, so all it can say is
/// which handle it named and what came back. The interesting answers are the
/// refusals — a process that guesses learns nothing, a process that names a
/// released handle is told so, and a process holding one half of an endpoint
/// cannot perform the other half.
fn authority(launch: &Launch, report: &mut Report) {
    if launch.capability_count == 0 {
        report.line("TOS.RUN.CAPABILITY held=0 endowment=empty");
        return;
    }
    // SAFETY: the launcher states the record holds `capability_count` entries at
    // `capabilities`, mapped readable in this address space.
    let held = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<LaunchCapability>(launch.capabilities as usize),
            launch.capability_count as usize,
        )
    };
    let first = &held[0];
    report.line(&alloc::format!(
        "TOS.RUN.CAPABILITY held={} handle=0x{:x} object={} rights={}",
        held.len(),
        first.handle,
        first.object,
        first.rights
    ));

    // What guessing is worth. An index beyond the table is `E_BAD_HANDLE`; an
    // index inside it that the process was not granted refuses too, because a
    // handle is an index *and* a generation and the generation is not guessable
    // from the index (`CAPABILITY_V1` §7.2).
    // SAFETY: `capability_attenuate` names the capability it attenuates and
    // does nothing when it refuses.
    let (out_of_range, _) = unsafe { call(CAPABILITY_ATTENUATE, 0xffff, first.rights as u64) };
    let mut in_range_refused = 0;
    let mut guessed = 0;
    for index in 0..16u64 {
        // SAFETY: as above.
        let (status, _) = unsafe { call(CAPABILITY_ATTENUATE, index, first.rights as u64) };
        if status == OK {
            guessed += 1;
        } else if status == E_NO_CAPABILITY {
            in_range_refused += 1;
        }
    }
    report.line(&alloc::format!(
        "TOS.RUN.CAPABILITY.PROBE out_of_range={out_of_range} in_range_refused={in_range_refused} guessed={guessed}"
    ));

    if first.object == tos_launch::OBJECT_ENDPOINT {
        if first.rights & tos_launch::RIGHT_SEND != 0 {
            send_half(launch, report, first.handle);
        }
        if first.rights & tos_launch::RIGHT_RECEIVE != 0 {
            receive_half(launch, report, first.handle);
        }
        // An operation whose object this handle is not. The index is right, the
        // generation is right, and the answer is still a refusal — which is
        // `SYSTEM_ABI_V1` §8.1's harder half: a handle of a *different type*
        // supplied at the same index.
        // SAFETY: `process_create` names the process a child is created under
        // and an entry index; this handle names neither.
        let (wrong_type, _) = unsafe { call(PROCESS_CREATE, first.handle, 0) };
        report.line(&alloc::format!(
            "TOS.RUN.CAPABILITY.TYPE operation=8 status={wrong_type}"
        ));
    }
    if first.object == tos_launch::OBJECT_PROCESS {
        supervise(report, first.handle, launch.entry_index as u64);
    }

    // Asking for more than was held yields less, not more (`CAPABILITY_V1`
    // §7.4). This asks for *every* right there is; what comes back can only be
    // what this process already had, and the proof is that the resulting handle
    // still cannot perform the half this process was never given.
    // SAFETY: `capability_attenuate` names the capability it attenuates and a
    // rights mask.
    let (widening, widened) =
        unsafe { call(CAPABILITY_ATTENUATE, first.handle, u64::from(u32::MAX)) };
    let half = if first.rights & tos_launch::RIGHT_SEND != 0 {
        ENDPOINT_RECEIVE
    } else {
        ENDPOINT_SEND
    };
    // SAFETY: an assigned endpoint operation, named by the handle attenuation
    // just returned.
    let (widened_half, _) = unsafe { call(half, widened, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.CAPABILITY.ATTENUATED status={widening} asked=all widened_half={widened_half}"
    ));

    // A released handle is stale, and the nucleus says so rather than addressing
    // whatever occupies the slot next (`CAPABILITY_V1` §7.3).
    // SAFETY: `capability_release` names the capability it consumes.
    let (released, _) = unsafe { call(CAPABILITY_RELEASE, first.handle, 0) };
    // SAFETY: the same handle, now naming nothing.
    let (after, _) = unsafe { call(CAPABILITY_RELEASE, first.handle, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.CAPABILITY.RELEASED status={released} reuse={after}"
    ));
}

/// Creates a process and ends it, on authority this process was given.
///
/// A supervisor is not a special kind of program: it is a process that was
/// endowed with authority over itself, and everything it can do to another
/// process follows from that one grant. What it cannot do is give itself more —
/// the capability it holds came from its launcher, and the capability it gets
/// over the child carries no more than the one it used.
///
/// The child is ended as soon as it exists, and the nucleus records who ended
/// it. It is not ended before it can run — see the note below — which makes the
/// evidence stronger rather than weaker: what was ended had been on the
/// processor.
fn supervise(report: &mut Report, handle: u64, entry: u64) {
    // SAFETY: `process_create` names the process the child is created under and
    // the entry module's index; no pointer crosses.
    let (created, child) = unsafe { call(PROCESS_CREATE, handle, entry) };
    if created == OK {
        // Ended immediately, and nothing is reported between the two calls: a
        // report line gives up the rest of the quantum, and every quantum given
        // up is a turn the child could take.
        //
        // It takes one anyway, and the log says so. Building a child's address
        // space is long enough that a timer interrupt is always pending by the
        // time `process_create` returns, and it is delivered at the first
        // instruction back at CPL 3 — when the child is runnable and this
        // process is not the only candidate. So what this demonstrates is not a
        // process that never existed on the processor: it is a process that
        // did, and was ended by authority anyway.
        // SAFETY: `process_terminate` names the process it ends.
        let (ended, _) = unsafe { call(PROCESS_TERMINATE, child, 0) };
        // The same handle, over something that has now ended. The handle still
        // resolves — nothing consumed it — but a capability's lifetime is
        // bounded by its object (`CAPABILITY_V1` §3), so what it names is gone
        // and the answer is a refusal rather than authority over whoever
        // occupies that slot next.
        // SAFETY: as above.
        let (again, _) = unsafe { call(PROCESS_TERMINATE, child, 0) };
        report.line(&alloc::format!(
            "TOS.RUN.PROCESS.CREATED status={created} child=0x{child:x}"
        ));
        report.line(&alloc::format!(
            "TOS.RUN.PROCESS.ENDED status={ended} again={again}"
        ));
    } else {
        report.line(&alloc::format!(
            "TOS.RUN.PROCESS.CREATED status={created} child=0x{child:x}"
        ));
    }
    // A module index this boot's source set does not have. Refused rather than
    // clamped: a process launched over a different module than the one asked
    // for is a process nobody asked for.
    // SAFETY: as above, with an entry index outside the set.
    let (no_module, _) = unsafe { call(PROCESS_CREATE, handle, u64::from(u32::MAX)) };
    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.REFUSED reason=no-such-module status={no_module}"
    ));
}

/// Puts a message in the slot the launch record names and sends it.
///
/// Two sends, and the first is meant to fail: `IPC_V1` §9.1 requires that a
/// message past the inline bound be refused rather than truncated, and the only
/// way to know which happened is to ask for one byte too many and see the
/// refusal instead of a shortened success.
fn send_half(launch: &Launch, report: &mut Report, handle: u64) {
    // SAFETY: `endpoint_send` names its endpoint and a length; the payload is in
    // the region the record names, and this call carries no pointer.
    let (oversize, _) = unsafe { call(ENDPOINT_SEND, handle, MAX_INLINE_BYTES + 1) };

    // No space in the payload: it is reported back as the value of a `text=`
    // field, and a value with a space in it would be two fields to a reader
    // that splits on them.
    let payload = b"authority-crossed-a-boundary";
    for (offset, byte) in payload.iter().enumerate() {
        // SAFETY: `message_base` names a writable mapping of `message_length`
        // bytes made by the launcher, and this payload is far inside it.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(launch.message_base as usize)
                .add(offset)
                .write(*byte)
        };
    }
    // SAFETY: as above; the length is the payload's and is inside the bound.
    let (sent, _) = unsafe { call(ENDPOINT_SEND, handle, payload.len() as u64) };
    // Holding `send` is not holding `receive` (`IPC_V1` §2): the same handle,
    // the other half, refused by the rights mask rather than by anything this
    // process agreed to.
    // SAFETY: `endpoint_receive` names its endpoint and takes no value.
    let (other_half, _) = unsafe { call(ENDPOINT_RECEIVE, handle, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.IPC.SENT bytes={} status={sent} oversize={oversize} other_half={other_half}",
        payload.len()
    ));
}

/// Waits for a message and reports what arrived.
///
/// The wait is a *block*, not a loop: this process stops being runnable until
/// somebody sends, and the nucleus answers the call it was suspended in. What
/// it costs while it waits is nothing — the scheduler has nobody to give the
/// processor to on its behalf, which is the whole difference from asking again.
///
/// A block can be cancelled (`SYSTEM_ABI_V1` §6), and `E_CANCELLED` is not a
/// result that looks like one: it says the wait was ended by somebody else. The
/// nucleus ends every wait at the moment nothing in the system could satisfy
/// one, so a process that receives it has learned something true — that nobody
/// is going to send — and asking once more is a reasonable thing to do exactly
/// once. Asking forever would be a program refusing to be told.
fn receive_half(launch: &Launch, report: &mut Report, handle: u64) {
    // The other form, once, before waiting for real: a call that would have
    // blocked is told so instead. Both are true answers, and which one this
    // gets depends on whether anybody has sent yet — so it is reported rather
    // than assumed, and the message is taken whichever way it arrives.
    // SAFETY: `endpoint_receive` names its endpoint and its flags.
    let (polled, polled_length) = unsafe { call(ENDPOINT_RECEIVE, handle, NON_BLOCKING) };
    report.line(&alloc::format!("TOS.RUN.IPC.POLLED status={polled}"));
    let mut attempts = 0;
    let (status, length) = if polled == OK {
        (polled, polled_length)
    } else {
        loop {
            // SAFETY: as above, with no flags: this one waits.
            let answered = unsafe { call(ENDPOINT_RECEIVE, handle, 0) };
            if answered.0 == OK {
                break answered;
            }
            attempts += 1;
            report.line(&alloc::format!(
                "TOS.RUN.IPC.WAIT status={} attempt={attempts}",
                answered.0
            ));
            if answered.0 != E_CANCELLED || attempts > 1 {
                return;
            }
        }
    };
    let _ = status;
    // SAFETY: the nucleus states it wrote `length` bytes into the region the
    // record names, which is a mapping of `message_length` bytes, and `length`
    // is bounded by the contract's inline maximum.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(launch.message_base as usize),
            length as usize,
        )
    };
    let text = core::str::from_utf8(bytes).unwrap_or("<not text>");
    report.line(&alloc::format!(
        "TOS.RUN.IPC.RECEIVED bytes={length} text={text}"
    ));
    // The other half, refused for the same reason and from the other side:
    // holding `receive` is not holding `send`.
    // SAFETY: `endpoint_send` names its endpoint and a length.
    let (other_half, _) = unsafe { call(ENDPOINT_SEND, handle, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.IPC.RIGHTS other_half={other_half}"
    ));
}

/// A process that panics ends, and says so with the only channel it has left.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(EXIT_UNSTARTABLE)
}
