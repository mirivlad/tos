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
use tos_pipeline::{
    execute_set, interfaces, render, CapabilityRequest, Handle, IntKind, PipelineStage, Reach,
    SetError, SetRequest, System, Trace, Trap, Unit, Value,
};
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
const ENDPOINT_CALL: u64 = 3;
const ENDPOINT_REPLY: u64 = 4;
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
    // SAFETY: per this function's contract; a call that transfers nothing says
    // so with a count of zero rather than leaving the register as it found it.
    unsafe { call_transferring(operation, first, second, 0) }
}

/// Makes one system call with four arguments.
///
/// SAFETY: `operation` is assigned and every argument is legal for it.
// SAFETY: the caller names an assigned operation.
unsafe fn call4(operation: u64, first: u64, second: u64, third: u64, fourth: u64) -> (i64, u64) {
    let status: i64;
    let value: u64;
    // SAFETY: the six argument registers are `rdi, rsi, rdx, r10, r8, r9`, and
    // `rdx` is the third on the way in and the value's on the way out.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") operation => status,
            in("rdi") first,
            in("rsi") second,
            inlateout("rdx") third => value,
            in("r10") fourth,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status, value)
}

/// Makes one system call that carries `transferred` capabilities.
///
/// The handles themselves are in the argument region, at the offset `IPC_V1`
/// fixes; the register says how many of them to read (ADR-0058). A count is a
/// value, so it travels in a register; the handles are a list, so they do not.
///
/// SAFETY: `operation` is an assigned operation number, and the argument region
/// holds `transferred` handles the caller means to send.
// SAFETY: the caller names an assigned operation and has written the handles it
// is counting.
unsafe fn call_transferring(
    operation: u64,
    first: u64,
    second: u64,
    transferred: u64,
) -> (i64, u64) {
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
            in("r10") transferred,
            out("rdx") value,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status, value)
}

/// Writes a handle into the argument region's transfer table.
///
/// SAFETY: `region` is this process's argument region and `index` is inside the
/// contract's maximum.
// SAFETY: the caller names its own region and an index the contract admits.
unsafe fn set_transferred(region: u64, index: usize, handle: u64) {
    // SAFETY: per the caller's contract; the offset is the one `IPC_V1` fixes.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u64>(
            (region + tos_launch::MESSAGE_CAPABILITIES) as usize,
        )
        .add(index)
        .write(handle)
    };
}

/// Reads one back.
///
/// SAFETY: as [`set_transferred`].
// SAFETY: as above.
unsafe fn transferred(region: u64, index: usize) -> u64 {
    // SAFETY: per the caller's contract.
    unsafe {
        core::ptr::with_exposed_provenance::<u64>(
            (region + tos_launch::MESSAGE_CAPABILITIES) as usize,
        )
        .add(index)
        .read()
    }
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
/// Two fields and no state: the region is the state, and this only says where
/// it is. That is what lets the trace and the endowment each hold one — a
/// process has one report and more than one thing with something to say, and
/// threading a single `&mut` through both would be a borrow the region's own
/// header already arbitrates.
#[derive(Clone, Copy)]
struct Report {
    base: u64,
    capacity: u64,
}

impl Report {
    fn line(&self, text: &str) {
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
struct ReportTrace {
    /// By value, for the reason [`Report`] gives: the region is the state, and
    /// two writers of one region do not need one borrow between them.
    report: Report,
}

impl Trace for ReportTrace {
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
    let mut trace = ReportTrace { report };
    // What the run reaches: this process's own endowment, answering the
    // module's requests by the name each was bound to (ADR-0061) and
    // performing the operations `SYSTEM_INTERFACE_V1` §8 assigns to
    // `SYSTEM_ABI_V1`. Nothing here decides anything — the launcher decided,
    // before this process ran.
    //
    // SAFETY: the launcher states the record holds `capability_count` entries
    // at `capabilities`, mapped readable in this address space. The slice is
    // empty when there are none, which is a process that was endowed nothing.
    let held = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<LaunchCapability>(launch.capabilities as usize),
            launch.capability_count as usize,
        )
    };
    let mut endowment = Endowment { held, report };
    let run = match execute_set(&request, alloc::vec::Vec::new(), &mut trace, &mut endowment) {
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

    hold_direction_flag(&mut report);

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

/// Holds the direction flag set across many timer ticks, and says which ticks.
///
/// A process is entitled to set `DF`, and every Rust program on this system
/// already does: `memmove` sets it to copy overlapping bytes backwards and
/// clears it a dozen instructions later. An interrupt gate clears `IF` and `TF`
/// and leaves `DF` alone, so a tick landing inside that window enters the
/// nucleus with `DF` set — and the nucleus's handlers are Rust, compiled on the
/// System V AMD64 promise that `DF` is clear. It failed exactly there: a frame
/// copy in the scheduler ran `rep movsq` backwards over its own return address,
/// once in about thirty boots.
///
/// This is that window, widened from a dozen instructions to hundreds of
/// millions so that the question is asked on every boot instead of on one in
/// thirty. The loop is written as one assembly block rather than around Rust,
/// because the point is to hold a flag that Rust is entitled to assume is
/// clear: a compiler putting anything of its own between the `std` and the `cld`
/// would be corrupting this process rather than testing the nucleus.
///
/// The two ticks bracket the window and no system call is made inside it, so a
/// tick that moved is a tick taken by the nucleus while this process held `DF`.
/// That is the same argument the scheduler gate makes about interleaving, and it
/// needs nothing from the nucleus but the count it already keeps.
#[cfg(feature = "test-direction-flag")]
fn hold_direction_flag(report: &mut Report) {
    let began = monotonic().unwrap_or(0);
    // SAFETY: the block touches one scratch register and the direction flag, and
    // leaves the flag clear. It makes no memory reference, so nothing it does
    // depends on the flag it is holding; between `std` and `cld` there is no
    // instruction whose meaning the flag changes.
    unsafe {
        core::arch::asm!(
            "std",
            "2:",
            "dec {counter}",
            "jnz 2b",
            "cld",
            counter = inout(reg) 400_000_000u64 => _,
            options(nostack),
        );
    }
    let ended = monotonic().unwrap_or(0);
    report.line(&alloc::format!(
        "TOS.RUN.DIRECTION_FLAG held_begin={began} held_end={ended}"
    ));
}

#[cfg(not(feature = "test-direction-flag"))]
fn hold_direction_flag(_report: &mut Report) {}

/// Every operation of `SYSTEM_INTERFACE_V1` §4, and the `SYSTEM_ABI_V1` call
/// that performs it.
///
/// The schema's last column, in the one party that has to act on it. The
/// frontend deliberately does not carry these numbers — a frontend that knew the
/// system ABI would be a second place it is declared, and `docs/42` §5 keeps the
/// two separately versioned. A gate holds this table against §4.
///
/// `length` says whether the operation's declared parameter after the capability
/// is a payload length. It is not a guess about arity: §4 declares which
/// operations take one, and an operation whose declaration says otherwise is a
/// disagreement the gate catches rather than a call this table improvises.
const PERFORMED: &[(&str, &str, u64, bool)] = &[
    ("system.ipc.Endpoint", "endpoint_send", ENDPOINT_SEND, true),
    (
        "system.ipc.Endpoint",
        "endpoint_receive",
        ENDPOINT_RECEIVE,
        false,
    ),
    ("system.ipc.Endpoint", "endpoint_call", ENDPOINT_CALL, true),
    ("system.ipc.Reply", "endpoint_reply", ENDPOINT_REPLY, true),
    (
        "system.process.Control",
        "process_terminate",
        PROCESS_TERMINATE,
        false,
    ),
];

/// What this process holds, as the thing a run reaches through.
///
/// It is the whole of ADR-0061's host side: it answers the module's capability
/// requests from the launch record by the name each was bound to, and it
/// performs the operations of `SYSTEM_INTERFACE_V1` by making the
/// `SYSTEM_ABI_V1` call §8 assigns. It decides nothing — the launcher decided,
/// before this process ran, and this reports what that decision was.
struct Endowment<'a> {
    held: &'a [LaunchCapability],
    /// Its own copy, not a borrow: see [`Report`]. The trace holds one too, and
    /// both write to the one region the launcher named.
    report: Report,
}

impl System for Endowment<'_> {
    fn granted(&mut self, request: CapabilityRequest<'_>) -> Option<Handle> {
        // By the binding, which is the identity of the request (ADR-0061). Not
        // by position: this process's record and its module's import list are
        // two different orders, and matching them by index would be matching on
        // something a source edit changes.
        let answer = self
            .held
            .iter()
            .find(|capability| named(capability) == request.binding);
        // And the kind has to be the kind the interface declares
        // (`SYSTEM_INTERFACE_V1` §4). A grant of the wrong kind would be refused
        // by the nucleus at the first call anyway; refusing it here is what
        // makes that a *startup* failure with a name, which is what
        // `PROCESS_IDENTITY_V1` §7.3 asks for.
        let wanted =
            interfaces::interface(request.interface).map(|interface| match interface.object {
                interfaces::ObjectKind::Endpoint => tos_launch::OBJECT_ENDPOINT,
                interfaces::ObjectKind::Region => tos_launch::OBJECT_REGION,
                interfaces::ObjectKind::Process => tos_launch::OBJECT_PROCESS,
                interfaces::ObjectKind::InterfacePublication => tos_launch::OBJECT_INTERFACE,
                interfaces::ObjectKind::Reply => tos_launch::OBJECT_REPLY,
            })?;
        let capability = answer?;
        self.report.line(&alloc::format!(
            "TOS.RUN.REQUEST binding={} interface={} object={} wanted={}",
            request.binding,
            request.interface,
            capability.object,
            wanted
        ));
        (capability.object == wanted).then(|| Handle::new(capability.handle))
    }

    fn reach(&mut self, call: Reach<'_>) -> Result<Value, Trap> {
        let Some((_, _, operation, takes_length)) =
            PERFORMED.iter().find(|(interface, name, _, _)| {
                *interface == call.interface && *name == call.operation
            })
        else {
            // An accepted schema declared it and this host cannot perform it.
            // That is a disagreement between two documents, not a program error,
            // and it ends the run rather than returning a status the module
            // would read as an answer.
            return Err(Trap::new(
                "RUNTIME_OPERATION_NOT_IMPLEMENTED",
                "this system performs no such operation of that interface",
                call.source,
            ));
        };
        let Some(Value::Capability(held)) = call.arguments.first() else {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "an operation reached without a capability first",
                call.source,
            ));
        };
        let length = match (takes_length, call.arguments.get(1)) {
            (true, Some(Value::Int(_, bytes))) if *bytes >= 0 => *bytes as u64,
            (true, _) => {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an operation that takes a length was reached without one",
                    call.source,
                ))
            }
            (false, _) => 0,
        };
        // SAFETY: `operation` is one of the assigned numbers in the table above,
        // the first argument is the handle the launcher granted this process, and
        // the second is a length this call's own declaration says it takes.
        let (status, _) = unsafe { self::call(*operation, held.get(), length) };
        // What a module asked the system for and what the system answered, on
        // the audit record. The module sees only the status; a reader of the
        // boot log sees which operation, under which request, produced it.
        self.report.line(&alloc::format!(
            "TOS.RUN.INTERFACE operation={} status={status}",
            call.operation
        ));
        Ok(Value::Int(IntKind::I64, status.into()))
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
/// The request a grant answers, as text, or `<none>` when it answers nothing.
///
/// A launcher may grant something no module asked for — that is a policy the
/// module simply never reads — and a name that is not text names no declaration
/// this frontend could have produced, so both say so rather than being shown as
/// an empty string that could be mistaken for a name.
fn named(capability: &LaunchCapability) -> &str {
    let length = (capability.binding_length as usize).min(capability.binding.len());
    match core::str::from_utf8(&capability.binding[..length]) {
        Ok("") => "<none>",
        Ok(name) => name,
        Err(_) => "<not-text>",
    }
}

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
    // `binding` is appended after the four fixed fields, per Boot ABI v1's
    // extension rule. It is the request this grant answers (ADR-0061), and it is
    // the only field of the four that a *module* can act on: a process reading
    // its own record learns which of its `import capability` declarations was
    // satisfied, which is what makes a denial a difference between two sets
    // rather than a smaller number (`PROCESS_IDENTITY_V1` §7.3).
    report.line(&alloc::format!(
        "TOS.RUN.CAPABILITY held={} handle=0x{:x} object={} rights={} binding={}",
        held.len(),
        first.handle,
        first.object,
        first.rights,
        named(first)
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

    // A process holding **both** halves of an endpoint is the deputy: strong
    // enough to send on its own account, and about to be asked by somebody
    // weaker to act. That pairing is what `CAPABILITY_V1` §7.6 is about, so it
    // is answered before the ordinary halves are.
    if first.object == tos_launch::OBJECT_ENDPOINT
        && first.rights & tos_launch::RIGHT_SEND != 0
        && first.rights & tos_launch::RIGHT_RECEIVE != 0
    {
        deputy(launch, report, first.handle);
        return;
    }
    if first.object == tos_launch::OBJECT_ENDPOINT {
        if first.rights & tos_launch::RIGHT_CALL != 0 {
            client(launch, report, first.handle);
        }
        if first.rights & tos_launch::RIGHT_SEND != 0 {
            send_half(launch, report, first.handle);
        }
        if first.rights & tos_launch::RIGHT_RECEIVE != 0 {
            receive_half(launch, report, first.handle);
        }
        if first.rights & tos_launch::RIGHT_SEND == 0 && first.rights & tos_launch::RIGHT_CALL == 0
        {
            // Only a receiver reaches here without having done a half of its
            // own, so this is the request/reply server: what it received may
            // have carried the right to answer.
            server(launch, report, first.handle);
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
        supervise(launch, report, first.handle);
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

/// Asks a question and waits for the answer (`IPC_V1` §4).
///
/// The wait is the call's own: `endpoint_call` does not return until somebody
/// answers it or the wait is cancelled. Nothing here polls, and nothing here
/// holds a capability to answer with — the right to reply is made by the
/// nucleus for this one call and given to whoever receives the request.
fn client(launch: &Launch, report: &mut Report, handle: u64) {
    let question = b"what-is-the-answer";
    for (offset, byte) in question.iter().enumerate() {
        // SAFETY: `arguments_base` names a writable mapping of
        // `arguments_length` bytes made by the launcher, and this is far inside
        // it.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(launch.arguments_base as usize)
                .add(offset)
                .write(*byte)
        };
    }
    // SAFETY: `endpoint_call` names its endpoint and the request's length; the
    // request is in the region the record names, and no pointer crosses.
    let (status, length) = unsafe { call(ENDPOINT_CALL, handle, question.len() as u64) };
    if status != OK {
        report.line(&alloc::format!(
            "TOS.RUN.IPC.CALLED status={status} bytes=0"
        ));
        return;
    }
    // SAFETY: the nucleus states it wrote `length` bytes of the answer into the
    // region the record names, bounded by the contract's inline maximum.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(launch.arguments_base as usize),
            length as usize,
        )
    };
    let text = core::str::from_utf8(bytes).unwrap_or("<not text>");
    report.line(&alloc::format!(
        "TOS.RUN.IPC.CALLED status={status} bytes={length} answer={text}"
    ));

    // And again, this time handing over a capability this process actually
    // holds. Whoever answers may act **with that** — and what it can then do is
    // bounded by what this process held, not by what the answerer holds.
    // SAFETY: the argument region is this process's own, and index 0 is inside
    // the contract's maximum.
    unsafe { set_transferred(launch.arguments_base, 0, handle) };
    // SAFETY: as above, with one handle written for the count declared.
    let (with_capability, _) =
        unsafe { call_transferring(ENDPOINT_CALL, handle, question.len() as u64, 1) };
    report.line(&alloc::format!(
        "TOS.RUN.DEPUTY.ASKED named_by_value={status} with_capability={with_capability}"
    ));
}

/// Answers a question somebody asked, with the right that came with it.
///
/// The reply capability arrives in the last slot of the transfer table, always,
/// so a receiver knows where to look without being told how many capabilities
/// the caller chose to send. It is spent by answering: the second attempt below
/// is refused, which is what single-use means and not a claim about it.
fn server(launch: &Launch, report: &mut Report, handle: u64) {
    // The first question is already in the argument region: `receive_half` took
    // it. The second has not been asked yet, so it is waited for — a client that
    // asks twice must be answered twice, or its second call waits forever and
    // the liveness rule ends the boot.
    server_once(launch, report);
    // SAFETY: `endpoint_receive` names its endpoint and its flags; this waits.
    let (status, _) = unsafe { call(ENDPOINT_RECEIVE, handle, 0) };
    if status == OK {
        server_once(launch, report);
    }
}

/// One answered question.
fn server_once(launch: &Launch, report: &mut Report) {
    // SAFETY: the argument region is this process's own.
    let reply = unsafe {
        transferred(
            launch.arguments_base,
            tos_launch::MAX_TRANSFERRED_CAPABILITIES as usize - 1,
        )
    };
    if reply == 0 {
        report.line("TOS.RUN.IPC.REPLIED status=0 handle=0x0 again=0 answer=<none>");
        return;
    }
    let answer = b"i32:240";
    for (offset, byte) in answer.iter().enumerate() {
        // SAFETY: as in `client`.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(launch.arguments_base as usize)
                .add(offset)
                .write(*byte)
        };
    }
    // SAFETY: `endpoint_reply` names the reply capability and the answer's
    // length.
    let (status, _) = unsafe { call(ENDPOINT_REPLY, reply, answer.len() as u64) };
    // The same capability, a second time. Replying spent it, so what this gets
    // is a refusal — and a reply that could be sent twice would be an unbounded
    // channel back into a process that asked one question.
    // SAFETY: as above.
    let (again, _) = unsafe { call(ENDPOINT_REPLY, reply, answer.len() as u64) };
    report.line(&alloc::format!(
        "TOS.RUN.IPC.REPLIED status={status} handle=0x{reply:x} again={again}"
    ));
}

/// Acts for somebody weaker, and does not lend them its own strength.
///
/// This is `CAPABILITY_V1` §7.6, the test docs/37 says fails quietly in systems
/// that pass the other five. The deputy here **can** send on this endpoint —
/// that is its own authority and it is real. The question is what happens when a
/// client that cannot send asks it to.
///
/// Two requests, and the difference between them is the whole answer.
///
/// - The first names its object **by value**: a number in the payload, and no
///   capability. There is nothing to act on. The number would name something in
///   *this* process's table, and using it would be the deputy acting on its own
///   authority at a stranger's direction — which is the confused deputy exactly.
///   So it refuses, and the refusal names the request rather than guessing.
/// - The second carries a capability the client actually holds. The deputy acts
///   **with that**, and what it can then do is bounded by what the client held —
///   `call`, not `send` — even though this process holds `send`. One line later
///   it does the same operation on its own account and succeeds, so the refusal
///   above cannot be read as the deputy being weak.
fn deputy(launch: &Launch, report: &mut Report, own: u64) {
    for request in 0..2u32 {
        // SAFETY: `endpoint_receive` names its endpoint and its flags; this
        // waits.
        let (status, length) = unsafe { call(ENDPOINT_RECEIVE, own, 0) };
        if status != OK {
            report.line(&alloc::format!("TOS.RUN.DEPUTY.WAIT status={status}"));
            return;
        }
        // SAFETY: the argument region is this process's own; the reply
        // capability is in the last slot, and whatever the client sent is
        // before it.
        let (reply, offered) = unsafe {
            (
                transferred(
                    launch.arguments_base,
                    tos_launch::MAX_TRANSFERRED_CAPABILITIES as usize - 1,
                ),
                transferred(launch.arguments_base, 0),
            )
        };
        if offered == 0 {
            report.line(&alloc::format!(
                "TOS.RUN.DEPUTY.REFUSED request={request} reason=named-by-value bytes={length}"
            ));
        } else {
            // SAFETY: `endpoint_send` names its endpoint and a length.
            let (for_client, _) = unsafe { call(ENDPOINT_SEND, offered, 0) };
            // SAFETY: as above, on this process's own capability.
            let (on_own_account, _) = unsafe { call(ENDPOINT_SEND, own, 0) };
            report.line(&alloc::format!(
                "TOS.RUN.DEPUTY.ACTED request={request} for_client={for_client} on_own_account={on_own_account}"
            ));
        }
        if reply != 0 {
            // SAFETY: `endpoint_reply` names the reply capability and a length.
            unsafe { call(ENDPOINT_REPLY, reply, 0) };
        }
    }
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
fn supervise(launch: &Launch, report: &mut Report, handle: u64) {
    // The module by **name**, written where `SYSTEM_ABI_V1` §5 says a name goes.
    // An ordinal would have fitted a register and named a position in a list
    // nobody published; two boots whose capsules differ would give the same
    // ordinal to different modules.
    let module = b"system/boot/init.tos";
    for (offset, byte) in module.iter().enumerate() {
        // SAFETY: `arguments_base` names a writable mapping the launcher made,
        // and the module offset plus a short name is far inside it.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(
                (launch.arguments_base + tos_launch::CREATE_MODULE) as usize,
            )
            .add(offset)
            .write(*byte)
        };
    }

    // First, an endowment naming a capability this process does not hold. The
    // whole creation must fail: a child half-endowed would be a child holding
    // authority nobody decided to give it.
    // SAFETY: the endowment table is at a fixed offset in this process's own
    // argument region.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::CreateEndowment>(
            (launch.arguments_base + tos_launch::CREATE_ENDOWMENT) as usize,
        )
        .write(tos_launch::CreateEndowment {
            handle: 0xdead_beef,
            rights: u32::MAX,
            // The name this grant would have answered, had the handle named
            // anything (ADR-0061). It does not, so the creation is refused
            // before the name matters — which is the order that makes a
            // half-endowed child impossible rather than merely unlikely.
            binding_length: 0,
            binding: [0; tos_launch::MAX_BINDING as usize],
        })
    };
    // SAFETY: `process_create` names the process a child is created under, the
    // module name's length, the endowment count and the rights the child is to
    // hold over itself.
    let (forged, _) = unsafe { call4(PROCESS_CREATE, handle, module.len() as u64, 1, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.REFUSED reason=endowment-not-held status={forged}"
    ));

    // Then a child that may end itself and nothing more. This process holds
    // `create` and `terminate`; the child is given only the second, which is
    // attenuation at the moment of creation rather than after it.
    // SAFETY: as above, with no endowment entries.
    let (created, child) = unsafe {
        call4(
            PROCESS_CREATE,
            handle,
            module.len() as u64,
            0,
            u64::from(tos_launch::RIGHT_TERMINATE),
        )
    };
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
        // process that never existed on the processor: it is one that did, and
        // was ended by authority anyway.
        // SAFETY: `process_terminate` names the process it ends.
        let (ended, _) = unsafe { call(PROCESS_TERMINATE, child, 0) };
        // The same handle, over something that has now ended. A capability's
        // lifetime is bounded by its object (`CAPABILITY_V1` §3), so what it
        // named is gone and the answer is a refusal rather than authority over
        // whoever occupies that slot next.
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

    // A module this boot's source set does not have. Refused rather than
    // matched to something near it: a process launched over a different module
    // than the one asked for is a process nobody asked for.
    let absent = b"system/boot/nowhere.tos";
    for (offset, byte) in absent.iter().enumerate() {
        // SAFETY: as above.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(
                (launch.arguments_base + tos_launch::CREATE_MODULE) as usize,
            )
            .add(offset)
            .write(*byte)
        };
    }
    // SAFETY: as above, with a name the set does not hold.
    let (no_module, _) = unsafe { call4(PROCESS_CREATE, handle, absent.len() as u64, 0, 0) };
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
    // The second of the three §3 bounds, asked the same way. It answers the same
    // status as the first because both are constants of the contract that the
    // caller knew before it called — a full queue is the runtime condition, and
    // `E_LIMIT` is its answer alone.
    // SAFETY: as above, with a transfer count past the contract's maximum and no
    // handle written: the count is refused before anything is read.
    let (overcount, _) = unsafe {
        call_transferring(
            ENDPOINT_SEND,
            handle,
            0,
            tos_launch::MAX_TRANSFERRED_CAPABILITIES + 1,
        )
    };
    // And a transfer of something this process does not hold. `IPC_V1` §9.3
    // wants a send that fails **after** the capability check to transfer
    // nothing; this fails *in* it, which is the earlier of the two and the one
    // that decides whether a handle a caller does not hold can travel.
    // SAFETY: the argument region is this process's own and index 0 is inside
    // the contract's maximum; the handle written is deliberately not one this
    // process holds.
    unsafe { set_transferred(launch.arguments_base, 0, 0xdead_beef) };
    // SAFETY: as above, declaring the one handle just written.
    let (unheld, _) = unsafe { call_transferring(ENDPOINT_SEND, handle, 0, 1) };

    // No space in the payload: it is reported back as the value of a `text=`
    // field, and a value with a space in it would be two fields to a reader
    // that splits on them.
    let payload = b"authority-crossed-a-boundary";
    for (offset, byte) in payload.iter().enumerate() {
        // SAFETY: `arguments_base` names a writable mapping of `arguments_length`
        // bytes made by the launcher, and this payload is far inside it.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(launch.arguments_base as usize)
                .add(offset)
                .write(*byte)
        };
    }
    // The message carries this process's own endpoint capability. The receiver
    // holds `receive` on that endpoint and not `send`, so what arrives with the
    // message is an ability it demonstrably did not have — which is what makes a
    // delegation observable rather than asserted.
    // SAFETY: the argument region is this process's own, and index 0 is inside
    // the contract's maximum.
    unsafe { set_transferred(launch.arguments_base, 0, handle) };
    // SAFETY: as above; the length is the payload's and is inside the bound, and
    // one handle has been written for the count declared.
    let (sent, _) = unsafe { call_transferring(ENDPOINT_SEND, handle, payload.len() as u64, 1) };
    // Holding `send` is not holding `receive` (`IPC_V1` §2): the same handle,
    // the other half, refused by the rights mask rather than by anything this
    // process agreed to.
    // SAFETY: `endpoint_receive` names its endpoint and takes no value.
    let (other_half, _) = unsafe { call(ENDPOINT_RECEIVE, handle, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.IPC.SENT bytes={} status={sent} oversize={oversize} other_half={other_half} \
         overcount={overcount} unheld={unheld}",
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
    // record names, which is a mapping of `arguments_length` bytes, and `length`
    // is bounded by the contract's inline maximum.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(launch.arguments_base as usize),
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

    // And what came with the message. This is a handle in *this* process's
    // table, made when the message arrived; nothing about the sender's name for
    // it is visible here. Using it does the very thing the line above was
    // refused, on the same endpoint — so the difference between the two statuses
    // is the delegation, and nothing else.
    // SAFETY: the argument region is this process's own.
    let delegated = unsafe { transferred(launch.arguments_base, 0) };
    // SAFETY: `endpoint_send` names its endpoint and a length.
    let (with_delegated, _) = unsafe { call(ENDPOINT_SEND, delegated, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.IPC.DELEGATED handle=0x{delegated:x} send={with_delegated}"
    ));
}

/// A process that panics ends, and says so with the only channel it has left.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(EXIT_UNSTARTABLE)
}
