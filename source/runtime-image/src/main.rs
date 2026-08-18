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

use tos_launch::{Launch, LaunchUnit, ReportHeader, LAUNCH_VERSION};
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
const CONTEXT_YIELD: u64 = 10;
const TIME_MONOTONIC: u64 = 11;
const PROCESS_EXIT: u64 = 12;

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
/// SAFETY: `operation` is an assigned operation number and `argument` is legal
/// for it. No pointer crosses this edge: §3 admits values and handles only.
// SAFETY: the caller names an assigned operation; the instruction itself
// touches no memory of this image.
unsafe fn call(operation: u64, argument: u64) -> i64 {
    let status: i64;
    // SAFETY: `syscall` clobbers `rcx` and `r11` and returns a status in `rax`;
    // both are declared, and every other register is preserved by the contract
    // this call is made against.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") operation => status,
            in("rdi") argument,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    status
}

/// Ends this process, and does not return.
fn exit(status: u64) -> ! {
    loop {
        // SAFETY: `process_exit` is self-only and takes a status value.
        unsafe { call(PROCESS_EXIT, status) };
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
        unsafe { call(CONTEXT_YIELD, 0) };
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
        let mut ended = began;
        let mut attempts = 0u64;
        while ended == began && attempts < 200_000 {
            ended = monotonic().unwrap_or(began);
            attempts += 1;
        }
        report.line(&alloc::format!(
            "TOS.RUN.TICKS begin={began} end={ended} waits={attempts}"
        ));
    }

    match run.failed_at() {
        None => exit(EXIT_COMPLETED),
        Some(_) => exit(EXIT_REFUSED),
    }
}

/// A process that panics ends, and says so with the only channel it has left.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(EXIT_UNSTARTABLE)
}
