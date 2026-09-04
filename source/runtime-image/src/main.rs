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

#[cfg(all(
    feature = "test-measurement-ipc",
    any(
        feature = "test-measurement-call",
        feature = "test-more-exchanges",
        feature = "test-reply-receive-refusals"
    )
))]
compile_error!("the IPC numerator has one exact measurement workload");

extern crate alloc;

use core::panic::PanicInfo;

use tos_launch::{Launch, LaunchCapability, LaunchUnit, ReportHeader, LAUNCH_VERSION};
use tos_pipeline::{
    interfaces, prepare_from_source, render, run_prepared, CapabilityRequest, Handle, IntKind,
    Observe, PipelineStage, Preparation, Reach, ResidencyLimits, SetError, SetRequest, System,
    Trace, Trap, Unit, Value,
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
/// Retired, and named only so that a process can ask for them and be refused
/// (`SYSTEM_ABI_V1` §7, ADR-0076 §4). Neither is called except by the evidence
/// that they answer `E_NOT_SUPPORTED`.
#[cfg(any(feature = "test-funding-lifecycle", feature = "test-build-topology"))]
#[cfg(feature = "test-funding-lifecycle")]
const PROCESS_CREATE: u64 = 8;
const PROCESS_WAIT_CHILD: u64 = 14;
#[cfg(not(feature = "test-lifecycle"))]
#[cfg(any(feature = "test-funding-lifecycle", feature = "test-lifecycle"))]
const PROCESS_CREATE_WITH_GENERATION: u64 = 15;
/// The one creation from source this ABI version has: two capabilities, an
/// explicit runtime grant and a sealed launch plan (ADR-0076 §3, ADR-0077 §5).
const PROCESS_CREATE_FUNDED: u64 = 19;
/// The same creation, over a program the nucleus does not read (ADR-0073).
#[cfg(any(feature = "test-bundle-launch", feature = "test-build-topology"))]
const PROCESS_CREATE_FROM_BUNDLE: u64 = 20;
/// Launch policy as an object (ADR-0077): made, written entry by entry through
/// the authority each entry delegates, and sealed once.
const LAUNCH_PLAN_CREATE: u64 = 21;
const LAUNCH_PLAN_ENDOW: u64 = 22;
const LAUNCH_PLAN_SEAL: u64 = 23;
/// The three hardware mechanism primitives (`SYSTEM_ABI_V1` §2.1, ADR-0079).
const PCI_FUNCTION_CLAIM: u64 = 24;
const PCI_CONFIG_READ: u64 = 25;
const PCI_CONFIG_WRITE: u64 = 26;
const PCI_BAR_MAP: u64 = 27;
const PROCESS_TERMINATE: u64 = 9;
const CONTEXT_YIELD: u64 = 10;
const TIME_MONOTONIC: u64 = 11;
const PROCESS_EXIT: u64 = 12;
const ENDPOINT_REPLY_RECEIVE: u64 = 13;
const CAPABILITY_ATTENUATE_SCOPED: u64 = 16;
const REGION_ALLOCATE: u64 = 17;
#[cfg(any(
    feature = "test-memory-authority",
    feature = "test-region-transport",
    feature = "test-bundle-launch",
    feature = "test-build-topology"
))]
const REGION_SHARE: u64 = 7;
#[cfg(any(
    feature = "test-memory-authority",
    feature = "test-region-transport",
    feature = "test-bundle-launch",
    feature = "test-build-topology"
))]
const REGION_FREEZE: u64 = 18;

/// Statuses, as `SYSTEM_ABI_V1` §4 assigns them. Named here because this image
/// checks them: a refusal it could not name it could not report.
const OK: i64 = 0;
const E_NO_CAPABILITY: i64 = -1;
const E_BAD_ARGUMENT: i64 = -3;
const E_CANCELLED: i64 = -5;
#[cfg(feature = "test-region-transport")]
const E_WOULD_BLOCK: i64 = -4;

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
    unsafe { call_transferring(operation, first, second, 0, 0) }
}

/// Makes one system call with four arguments.
///
/// Reached only by the evidence workloads that name a raw operation directly.
/// The typed bridge does not use it: an operation there is performed from the
/// register table `PERFORMED` declares for it, which writes all six.
///
/// SAFETY: `operation` is assigned and every argument is legal for it.
#[allow(dead_code)]
// SAFETY: the caller names an assigned operation.
unsafe fn call4(operation: u64, first: u64, second: u64, third: u64, fourth: u64) -> (i64, u64) {
    // SAFETY: per this function's contract; the fifth argument register is the
    // region count of this ABI version, and a call that carries no region says
    // so with a zero rather than leaving the register as it found it.
    unsafe { call5(operation, first, second, third, fourth, 0) }
}

/// Makes one system call with all five assigned argument registers.
///
/// `rdi`, `rsi`, `rdx`, `r10`, `r8`, in the order `SYSTEM_ABI_V1` §3 fixes.
/// `r9` is the sixth and no operation of this version uses it, so nothing here
/// writes it.
///
/// **Every register an operation reads is written, including the zeros.** A
/// caller that left one as it found it would be asking the nucleus to read a
/// register nobody wrote, which is exactly what the contract's versioning rule
/// exists to prevent — arriving from the caller's side rather than the
/// nucleus's.
///
/// SAFETY: `operation` is an assigned operation number and every argument is
/// legal for it.
// SAFETY: the caller names an assigned operation; the instruction itself
// touches no memory of this image.
unsafe fn call5(
    operation: u64,
    first: u64,
    second: u64,
    third: u64,
    fourth: u64,
    fifth: u64,
) -> (i64, u64) {
    // SAFETY: per this function's contract; the sixth register is written with
    // a zero rather than left as it was found, for the reason the fifth is.
    unsafe { call6(operation, first, second, third, fourth, fifth, 0) }
}

/// Makes one system call with all six assigned argument registers.
///
/// `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, in the order `SYSTEM_ABI_V1` §3
/// fixes. The sixth is the runtime grant of operations 19 and 20 and nothing
/// else in this version.
///
/// SAFETY: `operation` is an assigned operation number and every argument is
/// legal for it.
// SAFETY: the caller names an assigned operation; the instruction itself
// touches no memory of this image.
#[allow(clippy::too_many_arguments)]
unsafe fn call6(
    operation: u64,
    first: u64,
    second: u64,
    third: u64,
    fourth: u64,
    fifth: u64,
    sixth: u64,
) -> (i64, u64) {
    let status: i64;
    let value: u64;
    // SAFETY: `rdx` is the third argument on the way in and the value's
    // register on the way out, which is why it is an `inlateout` and not two
    // operands. `syscall` clobbers `rcx` and `r11`; both are declared, and
    // every other register is preserved by the contract this is called against.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") operation => status,
            in("rdi") first,
            in("rsi") second,
            inlateout("rdx") third => value,
            in("r10") fourth,
            in("r8") fifth,
            in("r9") sixth,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status, value)
}

/// Makes one system call that carries `transferred` capabilities and `regions`
/// regions.
///
/// The handles themselves are in the argument region, at the two offsets
/// `IPC_V1` fixes; the registers say how many of each to read (ADR-0058). A
/// count is a value, so it travels in a register; the handles are lists, so
/// they do not.
///
/// The flag register is written as zero, which is what makes these the blocking
/// forms `SYSTEM_ABI_V1` §3 makes the default.
///
/// SAFETY: `operation` is an assigned operation number, and the argument region
/// holds `transferred` handles and `regions` region records the caller means to
/// send.
// SAFETY: the caller names an assigned operation and has written the handles it
// is counting.
unsafe fn call_transferring(
    operation: u64,
    first: u64,
    second: u64,
    transferred: u64,
    regions: u64,
) -> (i64, u64) {
    // SAFETY: per this function's contract.
    unsafe { call5(operation, first, second, 0, transferred, regions) }
}

/// Makes one funded creation (`SYSTEM_ABI_V1` §5, operation 19).
///
/// **Everything the operation reads is written here, including the zeros.** The
/// module name is already at `CREATE_MODULE` when this is called; what this puts
/// in place is the optional restart generation, in the record the contract fixes
/// for it. A caller that left that record as it found it would be asking the
/// nucleus to read whatever the last operation left in the argument region —
/// which is the mistake the canonical encoding exists to make detectable.
///
/// `generation` is `None` for a creation whose caller asserts no restart
/// lineage, which is what the retired operation 8 meant, and `Some(n)` for what
/// 15 meant. They are different records rather than different numbers, so a
/// child of the first has no generation at all rather than a zero nobody
/// claimed.
///
/// SAFETY: `process` names a process capability this process holds with
/// `create`, `memory` a memory authority with `spend`, and the argument region
/// holds the module name and the endowment entries the counts declare.
#[allow(clippy::too_many_arguments)]
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-funding-lifecycle",
    feature = "test-lifecycle"
))]
// SAFETY: the caller's promise about the two handles and the argument region is
// what makes this an ordinary call rather than a guess.
unsafe fn create_funded(
    arguments: u64,
    process: u64,
    memory: u64,
    plan: u64,
    name_length: u64,
    own_rights: u64,
    grant: u64,
    generation: Option<u64>,
) -> (i64, u64) {
    let record = tos_launch::CreateFundedRecord {
        restart_generation: generation.unwrap_or(0),
        flags: if generation.is_some() {
            tos_launch::HAS_RESTART_GENERATION
        } else {
            0
        },
    };
    // SAFETY: the record is at a fixed offset in this process's own argument
    // region, which the launcher mapped writable.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::CreateFundedRecord>(
            (arguments + tos_launch::CREATE_FUNDED_RECORD) as usize,
        )
        .write(record)
    };
    let status: i64;
    let value: u64;
    // SAFETY: operation 19 takes the process authority in `rdi`, the memory
    // authority in `rsi`, the sealed launch plan in `rdx`, the module name's
    // length in `r10`, the child's rights over itself in `r8` and the runtime
    // grant it asks for in `r9`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") PROCESS_CREATE_FUNDED => status,
            in("rdi") process,
            in("rsi") memory,
            inlateout("rdx") plan => value,
            in("r10") name_length,
            in("r8") own_rights,
            in("r9") grant,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status, value)
}

/// Makes an empty launch plan (21).
///
/// SAFETY: `process` names a process capability this process holds with
/// `create`.
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-funding-lifecycle",
    feature = "test-lifecycle",
    feature = "test-bundle-launch"
))]
// SAFETY: the caller's promise about the handle is what makes this an ordinary
// call.
unsafe fn launch_plan_create(process: u64) -> (i64, u64) {
    // SAFETY: operation 21 takes the process authority in `rdi` and nothing
    // else, and answers with the builder's handle in `rdx`.
    unsafe { call(LAUNCH_PLAN_CREATE, process, 0) }
}

/// Adds one entry to a builder (22): a capability this process holds, the
/// rights the child is to have over it, and the name the child knows it by.
///
/// SAFETY: `held` names a capability this process holds and `plan` a launch
/// plan builder it holds.
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-funding-lifecycle",
    feature = "test-lifecycle",
    feature = "test-bundle-launch"
))]
// SAFETY: as above; the binding is written into this process's own argument
// region before the call.
unsafe fn launch_plan_endow(arguments: u64, held: u64, plan: u64, rights: u32, name: &[u8]) -> i64 {
    let mut binding = [0u8; tos_launch::MAX_BINDING as usize];
    binding[..name.len()].copy_from_slice(name);
    // SAFETY: the slot is at a fixed offset in this process's own argument
    // region, which the launcher mapped writable.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<[u8; tos_launch::MAX_BINDING as usize]>(
            (arguments + tos_launch::LAUNCH_ENDOW_BINDING) as usize,
        )
        .write(binding)
    };
    let status: i64;
    // SAFETY: operation 22 takes the capability being delegated in `rdi`, the
    // builder in `rsi`, the binding's length in `rdx` and the rights in `r10`.
    //
    // **`rdx` is an `inlateout` and not an `in`**, because every operation
    // answers in `rax` and `rdx` (`SYSTEM_ABI_V1` §3) — including the ones whose
    // value is zero. Declared as an input alone, the compiler is entitled to
    // believe the length is still there afterwards and to reuse it; the second
    // entry of a plan then gets whatever the nucleus returned, which is a
    // binding of length zero. Two entries is the smallest case that shows it,
    // and it is the case a build worker's endowment actually is.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") LAUNCH_PLAN_ENDOW => status,
            in("rdi") held,
            in("rsi") plan,
            inlateout("rdx") name.len() as u64 => _,
            in("r10") u64::from(rights),
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    status
}

/// Seals a builder (23), consuming it and answering with the sealed plan.
///
/// SAFETY: `process` names a process capability with `create` and `plan` a
/// builder this process holds.
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-funding-lifecycle",
    feature = "test-lifecycle",
    feature = "test-bundle-launch"
))]
// SAFETY: the caller's promise about the two handles.
unsafe fn launch_plan_seal(process: u64, plan: u64) -> (i64, u64) {
    // SAFETY: operation 23 takes the process authority in `rdi` and the builder
    // in `rsi`, and answers with the sealed plan's handle in `rdx`.
    unsafe { call(LAUNCH_PLAN_SEAL, process, plan) }
}

/// A sealed plan carrying these entries, built the only way one can be.
///
/// Three calls and no shortcut: even an endowment of nothing is a plan that was
/// made and sealed, because a creation takes a decision and "nothing" is one.
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-funding-lifecycle",
    feature = "test-lifecycle",
    feature = "test-bundle-launch"
))]
fn sealed_plan(arguments: u64, process: u64, entries: &[(u64, u32, &[u8])]) -> (i64, u64) {
    // SAFETY: `process` names this process's own authority over itself, with
    // `create`.
    let (created, builder) = unsafe { launch_plan_create(process) };
    if created != OK {
        return (created, 0);
    }
    for (held, rights, name) in entries {
        // SAFETY: each handle names a capability this process holds, and
        // `builder` the plan just made.
        let placed = unsafe { launch_plan_endow(arguments, *held, builder, *rights, name) };
        if placed != OK {
            return (placed, 0);
        }
    }
    // SAFETY: as above.
    unsafe { launch_plan_seal(process, builder) }
}

/// The arena a **build worker** is given.
///
/// ADR-0069 §2a admits a funded, special-purpose process with a different fixed
/// policy grant, and the build workspace measurements put a worker's need far
/// above the ordinary runtime figure. It is still a fixed policy value: not a
/// share of what remains, not `min(available, …)`, and not derived from how much
/// another allocation happened to leave.
#[cfg(feature = "test-build-topology")]
const WORKER_GRANT: u64 = 96 * 1024 * 1024;

/// The runtime arena an ordinary process is given, as the reference platform
/// fixes it (ADR-0069). Named by the creator rather than chosen by the nucleus:
/// operation 19 has no default and will not pick one.
#[cfg(any(
    feature = "test-funding-lifecycle",
    feature = "test-bundle-launch",
    feature = "test-build-topology"
))]
const RUNTIME_GRANT: u64 = 54 * 1024 * 1024;

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
/// What this process holds resident while it runs (ADR-0071 §7).
///
/// **Declared against the grant this process was given**, and not inherited
/// from a host facade. `tos_pipeline::HOST_RESIDENCY` is a host's declaration —
/// `64 MiB`, above `RuntimeMemoryGrantV1` — and a byte bound larger than the
/// arena is a bound that can never bind: a run that reached it would have failed
/// to allocate long before, which is an allocation failure where an eviction
/// belonged.
///
/// The numbers come from `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`, measured
/// against this exact grant:
///
/// - `modules` is the docs/44 §2 closure ceiling, because a run may reach every
///   module of its closure and the byte bound is what actually binds;
/// - `bytes` admits **two** ceiling-sized modules of decoded state — measured at
///   `32.03 MiB` resident — which that evidence calls the smallest bound that
///   does not thrash, since the working set of a call is caller plus callee. A
///   bound of four measured `56.42 MiB` and does not fit the grant at all. The
///   remaining `22 MiB` of `54 MiB` holds the trusted records, the membership,
///   the frames, the values and what the program allocates.
const RESIDENCY: ResidencyLimits = ResidencyLimits {
    modules: 256,
    bytes: 32 * 1024 * 1024,
};

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

/// The target half of ADR-0073: a process handed one immutable artifact, which
/// it verifies for itself before it runs a single instruction of it.
///
/// **Nothing about the bundle's origin is evidence.** That a build wrote it,
/// that it arrived read-only, that a nucleus mapped it, that a supervisor
/// vouched for it — none of that admits an instruction. What admits one is this
/// process parsing the artifact with a total parser, rebuilding the declared
/// closure from what the bundle itself says, and holding every image to that
/// declaration through its own verifier. No build receipt crosses, no host
/// verdict crosses, and no nucleus verdict crosses, because none was made.
///
/// **The entry is the bundle's.** Its declared entry position is the program;
/// this process supplies only the exported function name every runtime process
/// runs, which is a property of the runtime rather than of the artifact. A
/// caller-chosen entry would be a second truth about which program this is.
///
/// A malformed header, offset, closure declaration, image or entry relation
/// fails here — before the first source instruction — and the process ends
/// saying so. That is the correct outcome of a hostile bundle and not a defect:
/// the creation succeeded, and the admission did not.
///
/// # Safety
///
/// As [`runtime_entry`], for the record shape the version selected.
// SAFETY: the caller has read the discriminator and this is the shape it named.
unsafe fn bundle_entry(launch: &tos_launch::BundleLaunch) -> ! {
    let report = Report {
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
    // SAFETY: as in `runtime_entry`; the grant is this process's alone.
    if unsafe { HEAP.adopt(&grant) }.is_err() {
        report.line("TOS.RUN.UNSTARTABLE reason=heap-rejected-grant");
        exit(EXIT_UNSTARTABLE);
    }
    let stack_region =
        tos_runtime::region::Span::new(launch.stack_base, launch.stack_base + launch.stack_length);
    // SAFETY: as in `runtime_entry`.
    let painted = unsafe { stack::paint(stack_region) };

    if launch.bundle_handle == 0 || launch.bundle_length == 0 {
        report.line("TOS.RUN.UNSTARTABLE reason=no-bundle");
        exit(EXIT_UNSTARTABLE);
    }
    let source_set = core::str::from_utf8(&launch.source_set)
        .unwrap_or("")
        .trim_end_matches('\0');
    report.line(&alloc::format!(
        "TOS.RUN.BUNDLE.BEGIN handle=0x{:x} base=0x{:x} length={} grant_length={} set={source_set}",
        launch.bundle_handle,
        launch.bundle_base,
        launch.bundle_length,
        launch.grant_length,
    ));

    // The artifact, as bytes and nothing more. The **region's** length, because
    // that is what the nucleus mapped; how many of those bytes are a bundle is
    // the bundle's own claim, and the parser is what holds it to it.
    // SAFETY: the launcher mapped this range read-only and not executable in
    // this address space and reported its base and length here.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(launch.bundle_base as usize),
            launch.bundle_length as usize,
        )
    };
    // The **region's** bytes, of which the artifact is a prefix: a bundle
    // arrives in whole frames because that is what memory is handed out in, and
    // it declares its own total. A bundle claiming more than the region holds is
    // refused by comparison rather than trusted.
    let parsed = match tos_pipeline::bundle::Bundle::parse_prefix(bytes) {
        Ok(parsed) => parsed,
        Err(refused) => {
            report.line(&alloc::format!(
                "TOS.RUN.BUNDLE.REFUSED stage=parse reason={}",
                refused.symbol()
            ));
            exit(EXIT_REFUSED);
        }
    };
    report.line(&alloc::format!(
        "TOS.RUN.BUNDLE.PARSED bytes={} modules={} entry_position={} entry_path={}",
        parsed.bytes().len(),
        parsed.modules(),
        parsed.entry_position(),
        parsed.entry_path(),
    ));

    let began = monotonic();
    let mut trace = ReportTrace { report };
    let prepared = tos_pipeline::admit_bundle(&parsed, ENTRY, &mut trace, RESIDENCY);
    // SAFETY: the launcher states the record holds `capability_count` entries
    // at `capabilities`, mapped readable in this address space.
    let held = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<LaunchCapability>(launch.capabilities as usize),
            launch.capability_count as usize,
        )
    };
    let mut endowment = Endowment {
        mappings: [DeviceMapping::EMPTY; MAX_DEVICE_MAPPINGS],
        held,
        arguments: launch.arguments_base,
        report: Report {
            base: launch.report_base,
            capacity: launch.report_length,
        },
    };
    let run = match prepared {
        Preparation::Ready(mut prepared) => {
            run_prepared(&mut prepared, alloc::vec::Vec::new(), &mut endowment)
        }
        Preparation::Refused(run) => run,
    };
    let ended = monotonic();
    let report = trace.report;
    for line in render::events(&run) {
        report.line(&line);
    }
    let (committed, peak) = HEAP.usage();
    let (blocks, free) = HEAP.block_census();
    report.line(&alloc::format!(
        "TOS.RUN.MEMORY granted={} peak={peak} committed={committed} blocks={blocks} free={free}",
        launch.grant_length,
    ));
    if let Some(floor) = painted {
        // SAFETY: `stack_region` and `floor` came from the matching `paint`
        // above, on the stack this frame is still running on.
        let used = unsafe { stack::peak(stack_region, floor) };
        report.line(&alloc::format!(
            "TOS.RUN.STACK used={used} capacity={}",
            launch.stack_length
        ));
    }
    if let (Some(began), Some(ended)) = (began, ended) {
        report.line(&alloc::format!("TOS.RUN.TICKS begin={began} end={ended}"));
    }
    exit(match run {
        tos_pipeline::Run::Completed(_) => EXIT_COMPLETED,
        _ => EXIT_REFUSED,
    });
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
    // **The discriminator first, and nothing else until it is read.** Both
    // record shapes begin with their version, and which one this is decides
    // what every byte after it means. A record read as the wrong shape is not a
    // record with wrong values in it — it is a set of pointers into whatever
    // happened to be laid out there.
    // SAFETY: the caller's contract makes the first word of the record readable
    // and aligned; the version is that word in both shapes.
    let version = unsafe { core::ptr::with_exposed_provenance::<u32>(launch as usize).read() };
    if version == tos_launch::BUNDLE_LAUNCH_VERSION {
        // SAFETY: the version says this is a `BundleLaunch`, and the launcher's
        // contract covers the whole of the record it wrote.
        unsafe { bundle_entry(&*core::ptr::with_exposed_provenance(launch as usize)) };
    }
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
    let mut endowment = Endowment {
        mappings: [DeviceMapping::EMPTY; MAX_DEVICE_MAPPINGS],
        held,
        arguments: launch.arguments_base,
        report,
    };
    let run = match prepare_from_source(&request, &mut trace, RESIDENCY) {
        Ok(Preparation::Ready(mut prepared)) => {
            trace.entering(PipelineStage::Execute);
            run_prepared(&mut prepared, alloc::vec::Vec::new(), &mut endowment)
        }
        Ok(Preparation::Refused(run)) => run,
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

    // ADR-0066's observed side. Under the measurement constant this process
    // answers an external instrument and does nothing else with the time it is
    // being measured over.
    measure_channel(launch, &mut report);

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

// Every operation of `SYSTEM_INTERFACE_V1` §4, and how each one crosses.
//
// The schema's last three columns, in the one party that has to act on them.
// The frontend deliberately does not carry these numbers — a frontend that knew
// the system ABI would be a second place it is declared, and `docs/42` §5 keeps
// the two separately versioned. A gate holds this table against §4.
//
// **This is where the ABI stops.** Above it a module names an operation and
// receives a value; below it there are registers, an argument region and fixed
// offsets. Nothing of the second kind is visible in TOS Core, which is what
// §5's "an operation returns the value it produced" costs: one table saying,
// per operation, which register each argument goes in and where each result
// comes from.

/// One of the six argument registers `SYSTEM_ABI_V1` §3 assigns.
#[derive(Clone, Copy)]
enum Reg {
    Rdi,
    Rsi,
    Rdx,
    R10,
    R8,
    R9,
}

/// How one declared value of an operation reaches the nucleus (§4.1).
#[derive(Clone, Copy)]
enum Slot {
    /// A `u64`, in the register the operation assigns it.
    Number(Reg),
    /// A capability **value** — one an operation produced, rather than one an
    /// `import capability` was answered with — in the register the operation
    /// assigns it.
    ///
    /// The engine carried it without reading it, exactly as it carries the
    /// operation's own capability, and this is the only place it becomes a
    /// number (`docs/42` §2).
    Held(Reg),
    /// A constant **this row** puts in a register, which no module supplies.
    ///
    /// It exists for a distinction that must not be a value: two schema rows
    /// over one ABI selector differing in what they produce (ADR-0081 §5). A
    /// writable device window is asked for by calling the other operation, not
    /// by passing a number — so the number is the row's, and a module has no
    /// way to write it.
    Fixed(Reg, u64),
    /// A `string`: its bytes at a fixed offset of the argument region, its
    /// length in a register.
    ///
    /// The bound is the schema's, not this host's: `SYSTEM_ABI_V1` §3 bounds
    /// every read by a constant of the contract rather than by a number a
    /// caller chose, so a value past its declared maximum is refused **before
    /// the call is made** with the status §4.1 assigns.
    Text {
        length: Reg,
        at: u64,
        maximum: usize,
    },
}

/// What an operation produces, in the shape the schema declares (§5).
#[derive(Clone, Copy)]
enum Produced {
    /// `i64`: the status, unchanged and unwrapped.
    Status,
    /// `Result<C, i64>` for a nominal capability `C`: the handle `rdx` carries
    /// on success, the status otherwise.
    Authority,
    /// `Result<system.process.CreatedProcess, i64>`: the child's capability
    /// from `rdx`, and its instance identity from the argument region. Two
    /// facts, because neither is derivable from the other — a handle is an
    /// index in one table, and an instance identity is not authority.
    CreatedProcess,
    /// `Result<MmioRegion, i64>` and its writable form: the capability `rdx`
    /// carries, and — for this host alone — the window the nucleus wrote to the
    /// argument region (ADR-0081 §13).
    ///
    /// **The only result that teaches this bridge an address.** A module
    /// receives a capability and nothing else; what is recorded here is how to
    /// turn a later `MmioRead` on that capability into a load, which is the
    /// same act the bridge already performs on a region grant.
    Mapping {
        /// Whether the window is writable, which decides which of the two
        /// results this row produces and which rights the mapping is recorded
        /// with.
        writable: bool,
    },
    /// `Result<u64, i64>`: the number `rdx` carries on success, the status
    /// otherwise.
    ///
    /// The first result of any accepted schema whose success value is an
    /// ordinary integer rather than authority or a record — a configuration
    /// read produces a number the device reported, and that is all it produces.
    /// **A number is not authority**: nothing downstream accepts one where a
    /// capability belongs, which is what keeps a BAR value data (ADR-0079 §10).
    Number,
    /// `Result<system.process.ChildEnding, i64>`: the record operation 14 wrote
    /// at `WAIT_CHILD_RECORD`, as the value it describes.
    ///
    /// **This is where a flag beside a value becomes an `Option`.** The ABI
    /// carries `has_ended_by` next to `ended_by` because a register-and-offset
    /// contract has no other way to say "absent"; the schema says it in the
    /// type, and the translation happens here, once, rather than in every
    /// supervisor that reads one.
    ChildEnding,
}

struct Performed {
    interface: &'static str,
    name: &'static str,
    operation: u64,
    /// Where each capability supplied from an `import capability` goes, in the
    /// order §4 declares them.
    capabilities: &'static [Reg],
    /// Where each declared value goes, in the order §4 declares them.
    values: &'static [Slot],
    result: Produced,
}

const PERFORMED: &[Performed] = &[
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endpoint_send",
        operation: ENDPOINT_SEND,
        capabilities: &[Reg::Rdi],
        // §5 rows 1, 3 and 4: the length goes where a one-capability
        // operation's first value goes, which is `rsi`.
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Status,
    },
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endpoint_send_text",
        operation: ENDPOINT_SEND,
        capabilities: &[Reg::Rdi],
        // The payload where `IPC_V1` §3 puts one, and its length where §5 row 1
        // puts that. The bound is the schema's 256, which is §3's inline bound:
        // a longer message is refused before the call is made, not truncated.
        values: &[Slot::Text {
            length: Reg::Rsi,
            at: tos_launch::MESSAGE_PAYLOAD,
            maximum: MAX_INLINE_BYTES as usize,
        }],
        result: Produced::Status,
    },
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endpoint_receive",
        operation: ENDPOINT_RECEIVE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Status,
    },
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endpoint_call",
        operation: ENDPOINT_CALL,
        capabilities: &[Reg::Rdi],
        // §5 rows 1, 3 and 4: the length goes where a one-capability
        // operation's first value goes, which is `rsi`.
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Status,
    },
    // One selector for every kind of authority there is (ADR-0077 §3). The
    // capability being delegated is the one the call is reached through, so
    // this row is declared once per interface in §4 and performed once here.
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endow_for_launch",
        operation: LAUNCH_PLAN_ENDOW,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Held(Reg::Rsi),
            Slot::Number(Reg::R10),
            Slot::Text {
                length: Reg::Rdx,
                at: tos_launch::LAUNCH_ENDOW_BINDING,
                maximum: tos_launch::MAX_BINDING as usize,
            },
        ],
        result: Produced::Status,
    },
    // The same operation, declared by every interface whose capabilities may be
    // a startup endowment. One ABI selector, three rows: what differs is the
    // nominal type of the capability the call is reached through, which is the
    // caller's and not this contract's.
    Performed {
        interface: "system.memory.Authority",
        name: "endow_for_launch",
        operation: LAUNCH_PLAN_ENDOW,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Held(Reg::Rsi),
            Slot::Number(Reg::R10),
            Slot::Text {
                length: Reg::Rdx,
                at: tos_launch::LAUNCH_ENDOW_BINDING,
                maximum: tos_launch::MAX_BINDING as usize,
            },
        ],
        result: Produced::Status,
    },
    Performed {
        interface: "system.process.Control",
        name: "endow_for_launch",
        operation: LAUNCH_PLAN_ENDOW,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Held(Reg::Rsi),
            Slot::Number(Reg::R10),
            Slot::Text {
                length: Reg::Rdx,
                at: tos_launch::LAUNCH_ENDOW_BINDING,
                maximum: tos_launch::MAX_BINDING as usize,
            },
        ],
        result: Produced::Status,
    },
    Performed {
        interface: "system.ipc.Reply",
        name: "endpoint_reply",
        operation: ENDPOINT_REPLY,
        capabilities: &[Reg::Rdi],
        // §5 rows 1, 3 and 4: the length goes where a one-capability
        // operation's first value goes, which is `rsi`.
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Status,
    },
    // The second capability is an argument like the first and is read like the
    // first: the engine carried both without looking at either (ADR-0063).
    Performed {
        interface: "system.ipc.Reply",
        name: "endpoint_reply_receive",
        operation: ENDPOINT_REPLY_RECEIVE,
        capabilities: &[Reg::Rdi, Reg::Rsi],
        values: &[Slot::Number(Reg::Rdx)],
        result: Produced::Status,
    },
    // Refinement and release act on **the capability given to them**, which
    // since ADR-0078 may be one an operation produced: a child's authority, or a
    // scoped budget. Nothing about the ABI row changed — `rdi` is the capability
    // and `rsi` is the rights mask — only where the capability may come from.
    Performed {
        interface: "system.memory.Authority",
        name: "capability_attenuate_scoped",
        operation: CAPABILITY_ATTENUATE_SCOPED,
        capabilities: &[Reg::Rdi],
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Authority,
    },
    Performed {
        interface: "system.memory.Authority",
        name: "capability_release",
        operation: CAPABILITY_RELEASE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Status,
    },
    Performed {
        interface: "system.process.Control",
        name: "capability_attenuate",
        operation: CAPABILITY_ATTENUATE,
        capabilities: &[Reg::Rdi],
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Authority,
    },
    Performed {
        interface: "system.process.Control",
        name: "capability_release",
        operation: CAPABILITY_RELEASE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Status,
    },
    Performed {
        interface: "system.process.Control",
        name: "process_terminate",
        operation: PROCESS_TERMINATE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Status,
    },
    Performed {
        interface: "system.process.Control",
        name: "launch_plan_create",
        operation: LAUNCH_PLAN_CREATE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Authority,
    },
    Performed {
        interface: "system.process.Control",
        name: "launch_plan_seal",
        operation: LAUNCH_PLAN_SEAL,
        capabilities: &[Reg::Rdi],
        values: &[Slot::Held(Reg::Rsi)],
        result: Produced::Authority,
    },
    Performed {
        interface: "system.process.Control",
        name: "process_create_funded",
        operation: PROCESS_CREATE_FUNDED,
        capabilities: &[Reg::Rdi, Reg::Rsi],
        values: &[
            Slot::Held(Reg::Rdx),
            Slot::Text {
                length: Reg::R10,
                at: tos_launch::CREATE_MODULE,
                maximum: tos_launch::MAX_MODULE_PATH as usize,
            },
            Slot::Number(Reg::R9),
            Slot::Number(Reg::R8),
        ],
        result: Produced::CreatedProcess,
    },
    Performed {
        interface: "system.process.Control",
        name: "process_wait_child",
        operation: PROCESS_WAIT_CHILD,
        capabilities: &[Reg::Rdi],
        // §5 row 14 puts this operation's flags in `rsi`.
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::ChildEnding,
    },
    // ---- PLATFORM_INTERFACE_V1 (ADR-0079) ----
    //
    // The same mechanism as every row above it: an interface, an operation
    // number, where each capability and value goes. That a device is at the
    // other end changes nothing here, which is the point — this bridge does not
    // know what PCI is, and the row for a configuration read is the shape of the
    // row for a send.
    Performed {
        interface: "platform.pci.Bus",
        name: "pci_function_claim",
        operation: PCI_FUNCTION_CLAIM,
        capabilities: &[Reg::Rdi],
        // §5 row 24: bus, device and function, in the three registers after the
        // capability. Three values rather than one packed word — a packed BDF
        // would have unused bits and therefore a canonical form to argue about,
        // and each of the three has its own architectural range to be refused
        // against.
        values: &[
            Slot::Number(Reg::Rsi),
            Slot::Number(Reg::Rdx),
            Slot::Number(Reg::R10),
        ],
        result: Produced::Authority,
    },
    Performed {
        interface: "platform.pci.Bus",
        name: "endow_for_launch",
        operation: LAUNCH_PLAN_ENDOW,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Held(Reg::Rsi),
            Slot::Number(Reg::R10),
            Slot::Text {
                length: Reg::Rdx,
                at: tos_launch::LAUNCH_ENDOW_BINDING,
                maximum: tos_launch::MAX_BINDING as usize,
            },
        ],
        result: Produced::Status,
    },
    Performed {
        interface: "platform.pci.Bus",
        name: "capability_attenuate",
        operation: CAPABILITY_ATTENUATE,
        capabilities: &[Reg::Rdi],
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Authority,
    },
    Performed {
        interface: "platform.pci.Bus",
        name: "capability_release",
        operation: CAPABILITY_RELEASE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Status,
    },
    // **No row here carries a bus, a device or a function.** The capability
    // decides which function; an offset and a width are all a caller says.
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "pci_config_read",
        operation: PCI_CONFIG_READ,
        capabilities: &[Reg::Rdi],
        values: &[Slot::Number(Reg::Rsi), Slot::Number(Reg::Rdx)],
        result: Produced::Number,
    },
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "pci_config_write",
        operation: PCI_CONFIG_WRITE,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Number(Reg::Rsi),
            Slot::Number(Reg::Rdx),
            Slot::Number(Reg::R10),
        ],
        result: Produced::Status,
    },
    // §5 row 27: the BAR index, then the page-aligned offset and length. The
    // form is not a value the module passes — it is which of the two rows this
    // is, so a writable window cannot be asked for by arithmetic.
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "pci_bar_map_read",
        operation: PCI_BAR_MAP,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Number(Reg::Rsi),
            Slot::Number(Reg::Rdx),
            Slot::Number(Reg::R10),
            Slot::Fixed(Reg::R8, 0),
        ],
        result: Produced::Mapping { writable: false },
    },
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "pci_bar_map_write",
        operation: PCI_BAR_MAP,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Number(Reg::Rsi),
            Slot::Number(Reg::Rdx),
            Slot::Number(Reg::R10),
            Slot::Fixed(Reg::R8, 1),
        ],
        result: Produced::Mapping { writable: true },
    },
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "endow_for_launch",
        operation: LAUNCH_PLAN_ENDOW,
        capabilities: &[Reg::Rdi],
        values: &[
            Slot::Held(Reg::Rsi),
            Slot::Number(Reg::R10),
            Slot::Text {
                length: Reg::Rdx,
                at: tos_launch::LAUNCH_ENDOW_BINDING,
                maximum: tos_launch::MAX_BINDING as usize,
            },
        ],
        result: Produced::Status,
    },
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "capability_attenuate",
        operation: CAPABILITY_ATTENUATE,
        capabilities: &[Reg::Rdi],
        values: &[Slot::Number(Reg::Rsi)],
        result: Produced::Authority,
    },
    Performed {
        interface: "platform.pci.FunctionConfig",
        name: "capability_release",
        operation: CAPABILITY_RELEASE,
        capabilities: &[Reg::Rdi],
        values: &[],
        result: Produced::Status,
    },
];

/// One optional `u64` of the wait record, as the `Option` the schema declares.
///
/// `None` is variant 0 and `Some` is variant 1 — the language's own
/// representation. The flag decides, and the value beside it is **not** read
/// when the flag is clear: a zero a caller never asserted is exactly what
/// ADR-0067 keeps out, and reading it here would put it back.
fn optional(present: u64, value: u64) -> Value {
    match present {
        0 => Value::Variant {
            index: 0,
            payload: alloc::vec![],
        },
        _ => Value::Variant {
            index: 1,
            payload: alloc::vec![Value::Int(IntKind::U64, u128::from(value) as i128)],
        },
    }
}

/// The wait record as a `system.process.ChildEnding`, field by field in the
/// order the schema declares them.
///
/// A record's fields are matched to their names by position, so this order is
/// the contract rather than a convenience, and a gate holds it against §4.2.
fn ending_value(record: tos_launch::WaitChildRecord) -> Value {
    let number = |value: u64| Value::Int(IntKind::U64, u128::from(value) as i128);
    Value::Aggregate(alloc::vec![
        number(record.child_instance),
        number(record.parent_instance),
        number(record.ending_kind),
        optional(record.has_self_reported_status, record.self_reported_status),
        optional(record.has_ended_by, record.ended_by),
        optional(record.has_restart_generation, record.restart_generation),
        number(record.ending_order),
        number(record.ended_tick),
    ])
}

/// What this process holds, as the thing a run reaches through.
///
/// It is the whole of ADR-0061's host side: it answers the module's capability
/// requests from the launch record by the name each was bound to, and it
/// performs the operations of `SYSTEM_INTERFACE_V1` by making the
/// `SYSTEM_ABI_V1` call §8 assigns. It decides nothing — the launcher decided,
/// before this process ran, and this reports what that decision was.
struct Endowment<'a> {
    held: &'a [LaunchCapability],
    /// Where this process's argument region begins, because a `string` value
    /// parameter travels in it (`SYSTEM_INTERFACE_V1` §4.1).
    ///
    /// The nucleus chose the address and the launch record reported it. Nothing
    /// above this struct knows there is a region at all: a module names a value,
    /// and this is the thing that puts its bytes where the ABI already reads
    /// them.
    arguments: u64,
    /// Its own copy, not a borrow: see [`Report`]. The trace holds one too, and
    /// both write to the one region the launcher named.
    report: Report,
    /// Where each device mapping this process holds actually is.
    ///
    /// **The only place in ring 3 that knows a device address**, and it is
    /// below the language: a module names a capability and an offset, the
    /// engine carries them without reading either, and this is where they
    /// become a window. `docs/42` §2's rule that a process never observes a
    /// region's address is about the *program*; something has to perform the
    /// access, and it is this.
    mappings: [DeviceMapping; MAX_DEVICE_MAPPINGS],
}

/// How many device mappings one process may hold at once.
const MAX_DEVICE_MAPPINGS: usize = 4;

/// One mapped device window, as the host needs it (ADR-0081 §7).
#[derive(Clone, Copy)]
struct DeviceMapping {
    /// The capability the module names it by, or zero for an empty slot. A
    /// handle of all zeros names nothing in any table, so zero is safe as the
    /// empty marker rather than needing a flag beside it.
    handle: u64,
    /// Where the nucleus mapped it in this address space.
    base: u64,
    /// How many bytes it covers. Every access is checked against this.
    length: u64,
    /// Whether the mapping is writable. The page table enforces this too — a
    /// read-only grant has no `WRITABLE` bit — and this is the check that
    /// refuses *before* the processor faults.
    writable: bool,
}

impl DeviceMapping {
    const EMPTY: Self = Self {
        handle: 0,
        base: 0,
        length: 0,
        writable: false,
    };
}

impl Endowment<'_> {
    /// Records a window this process was just granted.
    ///
    /// Refuses rather than overwriting when the table is full: a bridge that
    /// silently dropped one would leave a capability the module holds and this
    /// host cannot serve, which is a worse failure than the refusal.
    fn remember(&mut self, handle: u64, record: tos_launch::MmioMapRecord, writable: bool) -> bool {
        let Some(slot) = self.mappings.iter_mut().find(|slot| slot.handle == 0) else {
            return false;
        };
        *slot = DeviceMapping {
            handle,
            base: record.base,
            length: record.length,
            writable,
        };
        true
    }

    /// The mapping a capability names, if this process holds it.
    fn mapping(&self, handle: Handle) -> Option<DeviceMapping> {
        let named = handle.get();
        if named == 0 {
            return None;
        }
        self.mappings
            .iter()
            .find(|mapping| mapping.handle == named)
            .copied()
    }
}

impl System for Endowment<'_> {
    /// One device access, of exactly the declared width (ADR-0081 §9).
    ///
    /// **Checked before the device is touched, and never partially** (§12): a
    /// mapping this process does not hold, a width the contract does not
    /// declare, an offset that is not a multiple of that width, or an access
    /// reaching past the mapping all refuse with nothing read and nothing
    /// written.
    fn observe(&mut self, access: Observe) -> Result<Value, Trap> {
        let refuse = |detail: &str| {
            Err(Trap::new(
                "RUNTIME_DEVICE_REFUSED",
                alloc::string::String::from(detail),
                0,
            ))
        };
        let Some(mapping) = self.mapping(access.region) else {
            return refuse("a device access names a mapping this process does not hold");
        };
        if access.value.is_some() && !mapping.writable {
            return refuse("a write through a read-only device mapping");
        }
        let width = u64::from(access.width);
        if !matches!(width, 1 | 2 | 4 | 8) {
            return refuse("a device access of a width this contract does not declare");
        }
        if !access.offset.is_multiple_of(width) {
            return refuse("a device access whose offset is not a multiple of its width");
        }
        // Checked, so nothing wraps into a mapping it does not name.
        let Some(end) = access.offset.checked_add(width) else {
            return refuse("a device access whose extent overflows");
        };
        if end > mapping.length {
            return refuse("a device access past the end of its mapping");
        }
        let at = mapping.base + access.offset;
        // Every device this contract admits is little-endian, and the flag says
        // so rather than being assumed — a target that is not is a different
        // value here and a different transaction.
        if !access.little_endian && width != 1 {
            return refuse("a big-endian device access, which this contract does not declare");
        }
        // SAFETY: the nucleus mapped `[base, base + length)` into this address
        // space with device attributes, and the bounds above put this access
        // inside it. Volatile is what makes one source operation exactly one
        // hardware access: it may not be elided, duplicated, widened, narrowed
        // or reordered against another volatile access.
        unsafe {
            match access.value {
                None => {
                    let value = match width {
                        1 => u64::from(
                            core::ptr::with_exposed_provenance::<u8>(at as usize).read_volatile(),
                        ),
                        2 => u64::from(
                            core::ptr::with_exposed_provenance::<u16>(at as usize).read_volatile(),
                        ),
                        4 => u64::from(
                            core::ptr::with_exposed_provenance::<u32>(at as usize).read_volatile(),
                        ),
                        _ => core::ptr::with_exposed_provenance::<u64>(at as usize).read_volatile(),
                    };
                    Ok(Value::Int(IntKind::U64, u128::from(value) as i128))
                }
                Some(value) => {
                    match width {
                        1 => core::ptr::with_exposed_provenance_mut::<u8>(at as usize)
                            .write_volatile(value as u8),
                        2 => core::ptr::with_exposed_provenance_mut::<u16>(at as usize)
                            .write_volatile(value as u16),
                        4 => core::ptr::with_exposed_provenance_mut::<u32>(at as usize)
                            .write_volatile(value as u32),
                        _ => core::ptr::with_exposed_provenance_mut::<u64>(at as usize)
                            .write_volatile(value),
                    }
                    Ok(Value::Unit)
                }
            }
        }
    }

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
                interfaces::ObjectKind::MemoryAuthority => tos_launch::OBJECT_MEMORY_AUTHORITY,
                interfaces::ObjectKind::LaunchPlanBuilder => tos_launch::OBJECT_LAUNCH_PLAN_BUILDER,
                interfaces::ObjectKind::LaunchPlan => tos_launch::OBJECT_LAUNCH_PLAN,
                interfaces::ObjectKind::PciBus => tos_launch::OBJECT_PCI_BUS,
                interfaces::ObjectKind::PciFunction => tos_launch::OBJECT_PCI_FUNCTION,
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
        let Some(performed) = PERFORMED
            .iter()
            .find(|entry| entry.interface == call.interface && entry.name == call.operation)
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
        // Every register an operation reads is written, including the zeros. A
        // caller that left one as it found it would be asking the nucleus to
        // read a register nobody wrote.
        let mut registers = [0u64; 6];
        // The one `string` an operation carried, kept for the audit record.
        let mut said: Option<&str> = None;
        let capabilities = performed.capabilities.len();
        // A `Fixed` slot is the row's own constant and is not something the
        // module passes, so it is not counted among the arguments expected.
        let supplied_values = performed
            .values
            .iter()
            .filter(|slot| !matches!(slot, Slot::Fixed(_, _)))
            .count();
        if call.arguments.len() != capabilities + supplied_values {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "an operation was reached with the wrong number of arguments",
                call.source,
            ));
        }
        // The capabilities the schema declares, in the order it declares them,
        // each from its own `import capability` binding (ADR-0056, ADR-0063).
        for (register, argument) in performed.capabilities.iter().zip(call.arguments) {
            let Value::Capability(held) = argument else {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an operation was reached without the capability it requires",
                    call.source,
                ));
            };
            registers[*register as usize] = held.get();
        }
        // A `Fixed` slot takes no argument: it is the row's own constant, so
        // the argument cursor does not advance for it.
        let mut supplied = call.arguments[capabilities..].iter();
        for slot in performed.values.iter() {
            if let Slot::Fixed(register, value) = slot {
                registers[*register as usize] = *value;
                continue;
            }
            let Some(argument) = supplied.next() else {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an operation was reached with fewer values than it declares",
                    call.source,
                ));
            };
            match (slot, argument) {
                (Slot::Number(register), Value::Int(_, number)) => {
                    if *number < 0 {
                        return Err(Trap::new(
                            "RUNTIME_TYPE_CONFUSION",
                            "an operation was reached with a negative value",
                            call.source,
                        ));
                    }
                    registers[*register as usize] = *number as u64;
                }
                // A `size` is the type every bounded extent in this language
                // has — an offset into a device window, a length in pages — and
                // it crosses in the register its slot names, exactly as a `u64`
                // does. It is unsigned by construction, so there is no negative
                // case to refuse.
                (Slot::Number(register), Value::Size(number)) => {
                    let Ok(number) = u64::try_from(*number) else {
                        return Err(Trap::new(
                            "RUNTIME_TYPE_CONFUSION",
                            "an operation was reached with a size larger than the edge carries",
                            call.source,
                        ));
                    };
                    registers[*register as usize] = number;
                }
                // A capability the module *holds as a value*, because an
                // operation produced it: a launch plan, or a child. It is
                // carried exactly as an import-supplied one is — the engine
                // never read it, and this is where it becomes a number.
                (Slot::Held(register), Value::Capability(held)) => {
                    registers[*register as usize] = held.get();
                }
                (
                    Slot::Text {
                        length,
                        at,
                        maximum,
                    },
                    Value::Text(text),
                ) => {
                    // Past the declared maximum is refused before the call is
                    // made, with the status §4.1 assigns — the same one an
                    // inline payload past its own bound receives, because both
                    // are constants the caller knew before it called.
                    if text.len() > *maximum {
                        return Ok(Value::Int(IntKind::I64, i128::from(E_BAD_ARGUMENT)));
                    }
                    // SAFETY: the argument region is this process's own writable
                    // mapping, the offset is a constant of the contract, and the
                    // length is bounded by the schema's maximum immediately
                    // above.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            text.as_ptr(),
                            core::ptr::with_exposed_provenance_mut::<u8>(
                                (self.arguments + at) as usize,
                            ),
                            text.len(),
                        )
                    };
                    registers[*length as usize] = text.len() as u64;
                    said = Some(text.as_str());
                }
                _ => {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "an operation was reached with a value of the wrong kind",
                        call.source,
                    ));
                }
            }
        }
        // SAFETY: `operation` is one of the assigned numbers in the table above,
        // and every register it reads has been written from the arguments the
        // schema declares for it.
        let (status, value) = unsafe {
            call6(
                performed.operation,
                registers[0],
                registers[1],
                registers[2],
                registers[3],
                registers[4],
                registers[5],
            )
        };
        // What a module asked the system for and what the system answered, on
        // the audit record. The module sees the result; a reader of the boot log
        // sees which operation, under which request, produced it.
        //
        // **A `string` argument is rendered with it**, because it is the only
        // thing a TOS Core module can put into the world in its own words. The
        // module composed it; this writes it down. Nothing here interprets it —
        // a journal record is text the supervisor decided on, and the edge is
        // the thing that can make text visible, not the thing that decides what
        // it says.
        match said {
            Some(text) => self.report.line(&alloc::format!(
                "TOS.RUN.INTERFACE operation={} status={status} said={text}",
                call.operation
            )),
            None => self.report.line(&alloc::format!(
                "TOS.RUN.INTERFACE operation={} status={status}",
                call.operation
            )),
        }
        if let Produced::Status = performed.result {
            return Ok(Value::Int(IntKind::I64, status.into()));
        }
        // `Result` is variant 0 for `Ok` and 1 for `Err`, which is the
        // language's representation and not this host's invention.
        if status != OK {
            return Ok(Value::Variant {
                index: 1,
                payload: alloc::vec![Value::Int(IntKind::I64, status.into())],
            });
        }
        let produced = match performed.result {
            Produced::Status => unreachable!("answered above"),
            Produced::Authority => Value::Capability(Handle::new(value)),
            Produced::Number => Value::Int(IntKind::U64, u128::from(value) as i128),
            Produced::Mapping { writable } => {
                // SAFETY: the nucleus wrote the record at the fixed offset of
                // this process's own argument region, and only on success —
                // which is the branch this is.
                let record = unsafe {
                    core::ptr::with_exposed_provenance::<tos_launch::MmioMapRecord>(
                        (self.arguments + tos_launch::MMIO_MAP_RECORD) as usize,
                    )
                    .read_unaligned()
                };
                if !self.remember(value, record, writable) {
                    return Err(Trap::new(
                        "RUNTIME_DEVICE_REFUSED",
                        alloc::string::String::from(
                            "more device windows than this process may hold",
                        ),
                        0,
                    ));
                }
                Value::Capability(Handle::new(value))
            }
            Produced::CreatedProcess => {
                // SAFETY: the nucleus wrote the instance id at the fixed offset
                // of this process's own argument region, and only on success —
                // which is the branch this is.
                let instance =
                    unsafe { word_at((self.arguments + tos_launch::CREATE_INSTANCE_ID) as usize) };
                Value::Aggregate(alloc::vec![
                    Value::Capability(Handle::new(value)),
                    Value::Int(IntKind::U64, u128::from(instance) as i128),
                ])
            }
            Produced::ChildEnding => {
                // SAFETY: as above, for the record operation 14 writes at its
                // own fixed offset of the same region.
                let record = unsafe {
                    core::ptr::with_exposed_provenance::<tos_launch::WaitChildRecord>(
                        (self.arguments + tos_launch::WAIT_CHILD_RECORD) as usize,
                    )
                    .read_unaligned()
                };
                ending_value(record)
            }
        };
        Ok(Value::Variant {
            index: 0,
            payload: alloc::vec![produced],
        })
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

/// What a memory authority is, asked from ring 3 (`SYSTEM_ABI_V1` §5, 16).
///
/// The claims, in the order they are made:
///
/// - reserving out of an authority yields a **different** capability, naming a
///   child rather than the parent again;
/// - a child can be reserved out of, so the tree is a tree;
/// - a size larger than the budget is `E_LIMIT`, and zero is `E_BAD_ARGUMENT` —
///   an unaffordable request and an impossible one are not the same answer
///   (ADR-0076 §7);
/// - an alias made by generic attenuation (5) spends the **same** budget: after
///   the alias reserves everything a probe amount leaves, the original cannot
///   reserve it again. Two names, one remainder;
/// - and releasing a child returns what it held, so the parent can reserve that
///   amount again afterwards.
/// What operation 17 wrote about the region it just made.
fn region_record(launch: &Launch) -> tos_launch::RegionAllocateRecord {
    // SAFETY: the launcher mapped this process's argument region readable at
    // the address the record names, and the nucleus wrote the record there.
    unsafe {
        core::ptr::with_exposed_provenance::<tos_launch::RegionAllocateRecord>(
            (launch.arguments_base + tos_launch::REGION_ALLOCATE_RECORD) as usize,
        )
        .read()
    }
}

fn memory_authority(launch: &Launch, report: &mut Report, first: &LaunchCapability) {
    const MIB: u64 = 1024 * 1024;
    let parent = first.handle;

    // SAFETY: every call below names a capability this process holds, and each
    // does nothing when it refuses.
    let (child_status, child) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, parent, MIB) };
    let distinct = u64::from(child != parent && child_status == OK);
    // SAFETY: as above.
    let (grandchild_status, grandchild) =
        unsafe { call(CAPABILITY_ATTENUATE_SCOPED, child, MIB / 2) };
    // More than a one-megabyte child can hold, and more than any budget could:
    // the first is a limit, the second could not be served by any machine.
    // SAFETY: as above.
    let (over, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, child, 4 * MIB) };
    // SAFETY: as above.
    let (zero, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, child, 0) };
    // SAFETY: as above; a handle naming nothing is refused rather than acted on.
    let (bad_handle, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, 0xffff, MIB) };

    // Two names for one budget: the alias reserves, and what it reserved is
    // gone from what the original can reserve.
    // SAFETY: as above.
    let (alias_status, alias) = unsafe { call(CAPABILITY_ATTENUATE, child, first.rights as u64) };
    // SAFETY: as above.
    let (through_alias, spent) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, alias, MIB / 2) };
    // SAFETY: as above.
    let (after_alias, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, child, MIB / 2) };

    // And what a child held comes back when its last name goes.
    // SAFETY: as above.
    let (grandchild_released, _) = unsafe { call(CAPABILITY_RELEASE, grandchild, 0) };
    // SAFETY: as above.
    let (reclaimed, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, child, MIB / 2) };

    // A real region: allocated, written at both ends, released, and allocated
    // again into the same slot to prove the lane is reusable and the old handle
    // is not.
    const ODD: u64 = 3 * 4096 + 17;
    // SAFETY: as above; the nucleus writes the record into this process's own
    // argument region and returns the handle.
    let (region_status, region) = unsafe { call(REGION_ALLOCATE, parent, ODD) };
    let record = region_record(launch);
    let rounded = u64::from(record.length == 4 * 4096);
    let in_lane = u64::from(record.base >= 1 << 39);
    let mut wrote = 0;
    let mut zeroed = 0;
    if region_status == OK && record.length > 0 {
        // SAFETY: the nucleus mapped this range writable and not executable in
        // this address space, and reported its base and length here.
        unsafe {
            let first = core::ptr::with_exposed_provenance_mut::<u64>(record.base as usize);
            let last = core::ptr::with_exposed_provenance_mut::<u64>(
                (record.base + record.length - 8) as usize,
            );
            // A fresh region is memory nobody has used: the pool clears what it
            // hands out, and this is what says operation 17 did not go round it.
            zeroed = u64::from(first.read_volatile() == 0 && last.read_volatile() == 0);
            first.write_volatile(0x5445_5354_5f31);
            last.write_volatile(0x5445_5354_5f32);
            wrote = u64::from(
                first.read_volatile() == 0x5445_5354_5f31
                    && last.read_volatile() == 0x5445_5354_5f32,
            );
        }
    }
    // SAFETY: as above.
    let (released, _) = unsafe { call(CAPABILITY_RELEASE, region, 0) };
    // SAFETY: as above.
    let (again, second) = unsafe { call(REGION_ALLOCATE, parent, ODD) };
    let same_lane = u64::from(region_record(launch).base == record.base);
    // SAFETY: as above; the first handle named a region that no longer exists.
    let (stale, _) = unsafe { call(CAPABILITY_RELEASE, region, 0) };
    // SAFETY: as above.
    let (freed, _) = unsafe { call(CAPABILITY_RELEASE, second, 0) };

    // Eight more, each released at once. In an ordinary build every one
    // succeeds; in the fault-injection build the nucleus fails each at a named
    // point and reports what the machine looked like on both sides of it.
    let mut probes = 0;
    for _ in 0..8 {
        // SAFETY: as above.
        let (status, handle) = unsafe { call(REGION_ALLOCATE, parent, 2 * 4096) };
        if status == OK {
            probes += 1;
            // SAFETY: as above.
            unsafe { call(CAPABILITY_RELEASE, handle, 0) };
        }
    }
    report.line(&alloc::format!("TOS.RUN.REGION.PROBES completed={probes}"));

    report.line(&alloc::format!(
        "TOS.RUN.REGION allocate={region_status} rounded={rounded} in_lane={in_lane} \
zeroed={zeroed} wrote={wrote} released={released} again={again} same_lane={same_lane} \
stale={stale} freed={freed}"
    ));

    #[cfg(feature = "test-memory-authority")]
    {
        region_states(launch, report, parent);
        region_table_full(report, parent);
    }

    report.line(&alloc::format!(
        "TOS.RUN.AUTHORITY child={child_status} distinct={distinct} grandchild={grandchild_status} over={over} zero={zero} bad_handle={bad_handle} alias={alias_status} through_alias={through_alias} after_alias={after_alias} released={grandchild_released} reclaimed={reclaimed}"
    ));
    let _ = spent;
}

/// Reads one 64-bit word out of a region this process has mapped.
///
/// One function rather than a `read_volatile` at each site, because each of
/// those sites is a *claim* — the bytes are still there, the window is still
/// there, the mode did not change — and the claim is about the mapping rather
/// than about the read. Stating it once is stating it where it is true.
///
/// # Safety
///
/// `at` is inside a region the nucleus mapped into this address space and
/// reported the base and length of, and that mapping is still there.
// SAFETY: the caller's promise that the window is the nucleus's own and still
// stands is what makes this a read of mapped memory.
unsafe fn word_at(at: usize) -> u64 {
    // SAFETY: per this function's contract.
    unsafe { core::ptr::with_exposed_provenance::<u64>(at).read_volatile() }
}

/// Makes one creation from a bundle (`SYSTEM_ABI_V1` §5, operation 20).
///
/// Four capabilities and no module name: the bundle declares its own entry, so
/// there is nothing for a caller to name and nowhere for it to disagree.
///
/// SAFETY: `process` names a process capability this process holds with
/// `create`, `memory` a memory authority with `spend`, `bundle` a **shared**
/// region capability with `read`, and `plan` a sealed launch plan.
// SAFETY: the caller's promise about the four handles is what makes this an
// ordinary call.
#[cfg(any(feature = "test-bundle-launch", feature = "test-build-topology"))]
#[allow(clippy::too_many_arguments)]
unsafe fn create_from_bundle(
    arguments: u64,
    process: u64,
    memory: u64,
    bundle: u64,
    plan: u64,
    own_rights: u64,
    generation: Option<u64>,
) -> (i64, u64) {
    let record = tos_launch::CreateFundedRecord {
        restart_generation: generation.unwrap_or(0),
        flags: if generation.is_some() {
            tos_launch::HAS_RESTART_GENERATION
        } else {
            0
        },
    };
    // SAFETY: the record is at a fixed offset in this process's own argument
    // region, which the launcher mapped writable.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::CreateFundedRecord>(
            (arguments + tos_launch::CREATE_FUNDED_RECORD) as usize,
        )
        .write(record)
    };
    let status: i64;
    let value: u64;
    // SAFETY: operation 20 takes the process authority in `rdi`, the memory
    // authority in `rsi`, the shared region in `rdx`, the sealed launch plan in
    // `r10`, the child's rights over itself in `r8` and the runtime grant in
    // `r9`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") PROCESS_CREATE_FROM_BUNDLE => status,
            in("rdi") process,
            in("rsi") memory,
            inlateout("rdx") bundle => value,
            in("r10") plan,
            in("r8") own_rights,
            in("r9") RUNTIME_GRANT,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        )
    };
    (status, value)
}

/// How large a region the evidence bundle is written into.
///
/// A number this process states rather than one it discovers: the artifact is
/// the boot's own single-module closure, and a backing that grew to fit whatever
/// was produced would be a build with no bound.
#[cfg(any(feature = "test-bundle-launch", feature = "test-build-topology"))]
const BUNDLE_REGION_BYTES: u64 = 256 * 1024;

/// Writes a bundle over this boot's own source set into a region, and returns
/// the shared capability for it.
///
/// **This is not the canonical build worker.** That is ADR-0074 §4a's T1
/// topology, which `build_worker` and `build_supervisor` perform and
/// `build-topology.sh` proves. What this is, is the smallest thing that produces
/// a *real* `TOSBUNDLE/v1` over a real closure so that operation 20 can be asked
/// a real question on its own.
///
/// The region goes through the whole state machine on the way out: allocated
/// mutable, written, frozen, shared. That is the lifecycle ADR-0075 §4 names,
/// performed rather than described.
#[cfg(any(feature = "test-bundle-launch", feature = "test-build-topology"))]
fn bundle_region(
    launch: &Launch,
    report: &mut Report,
    memory: u64,
    units: &[Unit<'_>],
    source_set: &str,
    entry_path: &str,
    corrupt: bool,
) -> (i64, u64, u64) {
    // SAFETY: `region_allocate` names an authority this process holds.
    let (allocated, region) = unsafe { call(REGION_ALLOCATE, memory, BUNDLE_REGION_BYTES) };
    let record = region_record(launch);
    if allocated != OK || record.length == 0 {
        return (allocated, 0, 0);
    }
    // SAFETY: the nucleus mapped this range writable and not executable in this
    // address space, and reported its base and length here. Nothing else in
    // this process refers to it.
    let backing_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::with_exposed_provenance_mut::<u8>(record.base as usize),
            record.length as usize,
        )
    };
    let provider = tos_pipeline::SliceSourceProvider::new(units);
    let mut backing = tos_pipeline::bundle::SliceBacking::new(backing_bytes);
    let mut trace = ReportTrace { report: *report };
    let written = match tos_pipeline::build_into_bundle(
        &provider,
        source_set,
        entry_path,
        &mut backing,
        &mut trace,
    ) {
        Ok(tos_pipeline::BuildIntoBundle::Written { bytes, modules }) => {
            report.line(&alloc::format!(
                "TOS.RUN.BUNDLE.WRITTEN bytes={bytes} modules={modules} corrupt={}",
                u64::from(corrupt)
            ));
            bytes
        }
        Ok(tos_pipeline::BuildIntoBundle::OutOfRoom(full)) => {
            report.line(&alloc::format!(
                "TOS.RUN.BUNDLE.UNWRITTEN reason=out-of-room needed={} capacity={}",
                full.needed,
                full.capacity
            ));
            return (allocated, 0, 0);
        }
        _ => {
            report.line("TOS.RUN.BUNDLE.UNWRITTEN reason=refused");
            return (allocated, 0, 0);
        }
    };
    let _ = written;
    if corrupt {
        // One byte of the magic, so that what arrives is a **legal shared
        // region** carrying bytes that are not a bundle. The region is real, the
        // mapping is real, the capability is real; only the artifact is not.
        // SAFETY: the region is this process's own and still writable.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(record.base as usize).write_volatile(b'X')
        };
    }
    // SAFETY: the handle names the region just written.
    let (frozen_status, frozen) = unsafe { call(REGION_FREEZE, region, 0) };
    // SAFETY: as above; `share` consumes the affine form.
    let (shared_status, shared) = unsafe { call(REGION_SHARE, frozen, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.BUNDLE.SHARED allocate={allocated} freeze={frozen_status} \
share={shared_status} base=0x{:x} length={}",
        record.base,
        record.length
    ));
    (shared_status, shared, record.base)
}

/// Whether this process is ADR-0074's build worker, and if so, its whole life.
#[cfg(not(feature = "test-build-topology"))]
fn build_role(_launch: &Launch, _report: &mut Report, _held: &[LaunchCapability]) -> bool {
    false
}

#[cfg(feature = "test-build-topology")]
fn build_role(launch: &Launch, report: &mut Report, held: &[LaunchCapability]) -> bool {
    // A worker holds an authority to spend and the right to send, **and no
    // authority over any process**. A supervisor holds the opposite half of the
    // same channel under a different name and creation authority besides, so
    // neither can be mistaken for the other by what it was given.
    if held
        .iter()
        .any(|capability| capability.object == tos_launch::OBJECT_PROCESS)
    {
        return false;
    }
    let named_as = |name: &str| held.iter().find(|capability| named(capability) == name);
    let (Some(memory), Some(outbox)) = (named_as("memory"), named_as("outbox")) else {
        return false;
    };
    let Some((units, entry)) = launched_set(launch) else {
        report.line("TOS.RUN.TOPOLOGY.UNBUILT reason=no-entry-unit");
        return true;
    };
    let entry_path = units[entry].path;
    let source_set = launched_source_set(launch);
    build_worker(
        launch,
        report,
        memory.handle,
        outbox.handle,
        &units[..],
        &source_set[..],
        entry_path,
    );
    true
}

/// ADR-0074's T1 topology, from the side that builds.
///
/// A **transient build worker**: it is created by a resident supervisor, builds
/// a real `TOSBUNDLE/v1` over this boot's own canonical source into a region it
/// allocated out of the authority it was endowed with, freezes it, shares it,
/// hands it to the supervisor over the endpoint it was given, and exits.
///
/// Everything it needs it was given, and nothing more: an authority to spend and
/// one endpoint to send on. It cannot create a process, cannot terminate one,
/// and never learns what the bundle it built is for.
#[cfg(feature = "test-build-topology")]
fn build_worker(
    launch: &Launch,
    report: &mut Report,
    memory: u64,
    outbox: u64,
    units: &[Unit<'_>],
    source_set: &str,
    entry_path: &str,
) {
    let (shared, bundle, _) =
        bundle_region(launch, report, memory, units, source_set, entry_path, false);
    if shared != OK {
        report.line("TOS.RUN.TOPOLOGY.UNBUILT reason=no-shared-region");
        return;
    }
    // The handoff. A shared region travels in its own area of the message with
    // its own bound (`IPC_V1` §3, §5): the sender writes only the handle,
    // because a base is an address in *this* address space and means nothing in
    // the supervisor's.
    // SAFETY: the argument region is this process's own and index 0 is inside
    // the contract's maximum.
    unsafe { set_region_handle(launch.arguments_base, 0, bundle) };
    // SAFETY: `endpoint_send` names an endpoint this process holds with `send`,
    // and declares the one region just written.
    let (handed, _) = unsafe { call_transferring(ENDPOINT_SEND, outbox, 0, 0, 1) };
    report.line(&alloc::format!(
        "TOS.RUN.TOPOLOGY.HANDED status={handed} asserted_by=worker"
    ));
}

/// ADR-0074's T1 topology, from the side that decides.
///
/// The order is the whole claim, and each step is refused until the one before
/// it happened:
///
///   1. the supervisor creates a **transient** worker and funds it from the
///      authority it holds. The worker's grant is its own role's policy figure
///      (ADR-0069 §2a), not the ordinary runtime grant;
///   2. the worker builds and hands over one shared region;
///   3. the supervisor **collects the worker's ending** before it does anything
///      with the artifact. That is what makes the worker transient rather than
///      merely finished: its memory is back, its slot is back, and the
///      supervisor knows so from `process_wait_child` rather than from a delay;
///   4. and only then is a target created from the bundle.
///
/// What the boot's own account then shows is the point of the topology: how
/// much of the machine is left for bundle backing once a supervisor and a
/// worker are both resident.
#[cfg(feature = "test-build-topology")]
fn build_supervisor(launch: &Launch, report: &mut Report, handle: u64, memory: u64, inbox: u64) {
    // The worker's endowment: an authority to spend, and the endpoint to hand
    // the artifact back on. Not `create`, not `terminate` — a build worker that
    // could start processes would be a supervisor.
    let (sealed, plan) = sealed_plan(
        launch.arguments_base,
        handle,
        &[
            (memory, tos_launch::RIGHT_SPEND, b"memory"),
            (inbox, tos_launch::RIGHT_SEND, b"outbox"),
        ],
    );
    if sealed != OK {
        report.line(&alloc::format!(
            "TOS.RUN.TOPOLOGY.UNSTARTABLE reason=no-plan status={sealed}"
        ));
        return;
    }
    let module = b"system/boot/init.tos";
    write_module_name(launch, module);
    // SAFETY: `handle` names this process's own authority with `create`,
    // `memory` an authority with `spend`, and `plan` the plan just sealed.
    let (started, worker_handle) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            memory,
            plan,
            module.len() as u64,
            0,
            WORKER_GRANT,
            None,
        )
    };
    let worker_instance = created_instance(launch);
    report.line(&alloc::format!(
        "TOS.RUN.TOPOLOGY.WORKER status={started} instance={worker_instance} grant={WORKER_GRANT}"
    ));
    if started != OK {
        return;
    }
    // The supervisor holds authority over the worker and lets it go at once:
    // this worker ends on its own, and a supervisor that kept a handle per
    // build would fill its table. What it supervises by is the ending.
    // SAFETY: the handle names the worker this process just created.
    unsafe { call(CAPABILITY_RELEASE, worker_handle, 0) };

    // The artifact, received. A region arrives with a capability of the
    // receiver's own and a window the **nucleus** chose in this address space.
    let mut base = 0;
    let mut bundle = 0;
    let mut received = 0;
    let mut attempts = 0;
    while bundle == 0 && attempts < 8 {
        // SAFETY: `endpoint_receive` names an endpoint this process holds with
        // `receive`; the nucleus writes the region record into this process's
        // own argument region.
        let (status, _) = unsafe { call(ENDPOINT_RECEIVE, inbox, 0) };
        received = status;
        if status == OK {
            // SAFETY: as above, at the offset `IPC_V1` fixes.
            let handed = unsafe { region_handed_over(launch.arguments_base, 0) };
            bundle = handed.handle;
            base = handed.base;
        }
        attempts += 1;
    }

    // **The worker ends before the target begins.** Collected rather than
    // waited out: `process_wait_child` is how a supervisor learns a child is
    // over, and everything below happens after it answered.
    let collected = wait_child(launch, handle, false);
    report.line(&alloc::format!(
        "TOS.RUN.TOPOLOGY.RECEIVED status={received} bundle=0x{bundle:x} base=0x{base:x} \
collected={} ended={} kind={}",
        collected.0,
        collected.1.child_instance,
        collected.1.ending_kind
    ));
    if bundle == 0 {
        report.line("TOS.RUN.TOPOLOGY.UNSTARTABLE reason=no-bundle");
        return;
    }

    // Only now. The worker's frames are back in the pool and its slot is free,
    // which is what the account below is measured against.
    // SAFETY: as above, with the bundle the worker handed over.
    let (target, _) = unsafe {
        create_from_bundle(
            launch.arguments_base,
            handle,
            memory,
            bundle,
            plan,
            0,
            Some(1),
        )
    };
    settle();
    settle();
    let target_ended = wait_child(launch, handle, true).0;
    // SAFETY: the region is still mapped read-only in this address space: the
    // supervisor kept its own window through the target's whole life.
    let kept = unsafe { word_at(base as usize) };
    report.line(&alloc::format!(
        "TOS.RUN.TOPOLOGY.TARGET status={target} collected={target_ended} kept=0x{kept:x}"
    ));
}

/// ADR-0073's handoff, from the side that produces the artifact.
///
/// One supervisor, one bundle, and the whole of what operation 20 is for:
///
///   - the artifact is built into a region, frozen and shared, and the
///     supervisor keeps it;
///   - a target is created from it. The supervisor's handle and window are
///     untouched — the target gets its **own** capability and its own read-only
///     mapping of the same backing, and nothing is copied;
///   - the target ends, and the **same** capability creates another one. No
///     rebuild, no refreeze, no second artifact: a restart is one bundle used
///     twice;
///   - a target created from a corrupt bundle is created *successfully* and then
///     refuses itself, because the nucleus never read the bytes and the target
///     is the only thing that has any business deciding;
///   - and the negatives: an affine region is not a shared one, and neither is a
///     handle nobody holds.
/// The canonical source set this process was launched over, read back out of
/// its own launch record.
///
/// **A build's input is what the launcher gave it.** There is nowhere else it
/// could come from and nothing here to choose: the units are the capsule's, the
/// entry is the one the record names, and the source set identity travels with
/// them (`PROCESS_IDENTITY_V1`). A worker that assembled its own input would be
/// a worker building something the capsule does not contain.
#[cfg(any(feature = "test-bundle-launch", feature = "test-build-topology"))]
fn launched_set<'a>(launch: &'a Launch) -> Option<(alloc::vec::Vec<Unit<'a>>, usize)> {
    let mut units = alloc::vec::Vec::with_capacity(launch.unit_count as usize);
    for index in 0..launch.unit_count as usize {
        // SAFETY: the record declares `unit_count` units at `units`, mapped
        // readable, exactly as `runtime_entry` reads them.
        let unit = unsafe {
            &*core::ptr::with_exposed_provenance::<LaunchUnit>(launch.units as usize).add(index)
        };
        // SAFETY: as above, for the path and the bytes each unit names.
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
        let path = core::str::from_utf8(path).ok()?;
        units.push(Unit { path, bytes });
    }
    let entry = launch.entry_index as usize;
    (entry < units.len()).then_some((units, entry))
}

/// The declared source-set identity of this process's own launch.
#[cfg(any(feature = "test-bundle-launch", feature = "test-build-topology"))]
fn launched_source_set(launch: &Launch) -> alloc::string::String {
    alloc::string::String::from(
        core::str::from_utf8(&launch.source_set)
            .unwrap_or("")
            .trim_end_matches('\0'),
    )
}

#[cfg(feature = "test-bundle-launch")]
fn bundle_supervisor(launch: &Launch, report: &mut Report, handle: u64, memory: u64) {
    let Some((units, entry)) = launched_set(launch) else {
        report.line("TOS.RUN.BUNDLE.UNSTARTABLE reason=no-entry-unit");
        return;
    };
    let entry_path = units[entry].path;
    let source_set = launched_source_set(launch);
    let units = &units[..];
    let source_set = &source_set[..];

    // --- one plan, for every target this supervisor will ever make ------------
    // Made once, sealed once, and reused: a restart is the same policy applied
    // to a new process instance, and a plan a creation consumed would make the
    // second launch a second decision. It carries a name for the same memory
    // authority this supervisor funds out of, which is what a target needs to
    // allocate anything of its own — an alias for one budget, never a second
    // reservation (ADR-0076 §2b).
    let (sealed, plan) = sealed_plan(
        launch.arguments_base,
        handle,
        &[(memory, tos_launch::RIGHT_SPEND, b"memory")],
    );
    if sealed != OK {
        report.line(&alloc::format!(
            "TOS.RUN.BUNDLE.UNSTARTABLE reason=no-plan status={sealed}"
        ));
        return;
    }

    // --- an affine region is refused ------------------------------------------
    // Before anything is shared, so what is being asked is exactly "is the
    // shared form required?" and not "was this region ready?".
    // SAFETY: every call below names a capability this process holds.
    let (_, affine) = unsafe { call(REGION_ALLOCATE, memory, 4096) };
    // SAFETY: as above.
    let (_, frozen_affine) = unsafe { call(REGION_FREEZE, affine, 0) };
    // SAFETY: as above; an immutable **affine** region is not the shared form.
    let (not_shared, _) = unsafe {
        create_from_bundle(
            launch.arguments_base,
            handle,
            memory,
            frozen_affine,
            plan,
            0,
            None,
        )
    };
    // SAFETY: as above; a handle nobody holds names nothing.
    let (unheld, _) = unsafe {
        create_from_bundle(
            launch.arguments_base,
            handle,
            memory,
            0xdead_beef,
            plan,
            0,
            None,
        )
    };
    // SAFETY: as above; a plan that has not been sealed is a decision still
    // being written, and nothing may be created from one.
    let (unsealed_plan, builder) = unsafe { launch_plan_create(handle) };
    // SAFETY: as above.
    let (unsealed, _) = unsafe {
        create_from_bundle(
            launch.arguments_base,
            handle,
            memory,
            0xdead_beef,
            builder,
            0,
            None,
        )
    };
    // SAFETY: as above.
    unsafe { call(CAPABILITY_RELEASE, builder, 0) };
    // SAFETY: as above.
    unsafe { call(CAPABILITY_RELEASE, frozen_affine, 0) };

    // --- the artifact ---------------------------------------------------------
    let (shared_status, bundle, base) =
        bundle_region(launch, report, memory, units, source_set, entry_path, false);
    if shared_status != OK {
        report.line("TOS.RUN.BUNDLE.UNSTARTABLE reason=no-shared-region");
        return;
    }

    // --- the first target -----------------------------------------------------
    // SAFETY: as above, with the shared bundle.
    let (first, first_child) = unsafe {
        create_from_bundle(
            launch.arguments_base,
            handle,
            memory,
            bundle,
            plan,
            0,
            Some(1),
        )
    };
    let first_instance = created_instance(launch);
    // The supervisor kept everything: the same handle still resolves, and the
    // same window still reads.
    // SAFETY: the region is still mapped read-only in this address space.
    let kept = unsafe { word_at(base as usize) };
    settle();
    settle();
    let collected_first = wait_child(launch, handle, true).0;

    // --- and another, from the same capability and the same backing -----------
    // No rebuild, no refreeze, no copy: a restart is one bundle used twice.
    // SAFETY: as above.
    let (second, second_child) = unsafe {
        create_from_bundle(
            launch.arguments_base,
            handle,
            memory,
            bundle,
            plan,
            0,
            Some(2),
        )
    };
    let second_instance = created_instance(launch);
    settle();
    settle();
    let collected_second = wait_child(launch, handle, true).0;
    let distinct_targets = u64::from(
        first == OK
            && second == OK
            && first_child != second_child
            && first_instance != second_instance,
    );

    report.line(&alloc::format!(
        "TOS.RUN.BUNDLE.TARGETS not_shared={not_shared} unheld={unheld} \
unsealed_plan={unsealed_plan} unsealed={unsealed} first={first} \
second={second} distinct={distinct_targets} kept=0x{kept:x} collected={collected_first}/\
{collected_second}"
    ));

    // --- a corrupt artifact ---------------------------------------------------
    // The region is legal in every way a nucleus can check. What is wrong is the
    // one thing it never looks at.
    // SAFETY: as above.
    unsafe { call(CAPABILITY_RELEASE, bundle, 0) };
    let (hostile_status, hostile, _) =
        bundle_region(launch, report, memory, units, source_set, entry_path, true);
    let hostile_created = if hostile_status == OK {
        // SAFETY: as above.
        let (created, _) = unsafe {
            create_from_bundle(
                launch.arguments_base,
                handle,
                memory,
                hostile,
                plan,
                0,
                Some(3),
            )
        };
        settle();
        settle();
        created
    } else {
        hostile_status
    };
    report.line(&alloc::format!(
        "TOS.RUN.BUNDLE.HOSTILE shared={hostile_status} created={hostile_created}"
    ));
}

/// The two consuming transitions, asked from CPL 3 (operations 18 and 7).
///
/// The claims, in the order they are made:
///
/// - a **mutable** region cannot be shared: `share` presupposes immutability
///   rather than producing it, and the write and share rights never coexist;
/// - the freeze returns a **different** handle. An operation that changed the
///   rights under the number the caller already held would leave a process
///   unable to tell a frozen region from one it wrote a moment ago;
/// - the presented handle is then stale, refused by generation;
/// - the transition has no inverse and cannot be repeated: the new handle
///   carries no write right, so 18 refuses it;
/// - the **bytes are still there, at the same address**. Nothing moved: same
///   backing, same base, one bit of each page-table leaf cleared;
/// - `share` then consumes the immutable form the same way, and what it returns
///   carries `read` and not `share` — there is nothing left for a second share
///   to consume;
/// - several names for one shared region in one process are still **one
///   window**: releasing one of them leaves the memory readable, and only the
///   last one takes the mapping with it.
#[cfg(feature = "test-memory-authority")]
fn region_states(launch: &Launch, report: &mut Report, parent: u64) {
    const PATTERN: u64 = 0x4652_4f5a_454e_5f31;
    // SAFETY: every call below names a capability this process holds, and each
    // does nothing when it refuses.
    let (allocated, mutable) = unsafe { call(REGION_ALLOCATE, parent, 2 * 4096) };
    let record = region_record(launch);
    if allocated != OK || record.length == 0 {
        report.line("TOS.RUN.REGION.STATE unstartable=1");
        return;
    }
    let at = record.base as usize;
    // SAFETY: the nucleus mapped this range writable and not executable in this
    // address space, and reported its base and length here.
    unsafe { core::ptr::with_exposed_provenance_mut::<u64>(at).write_volatile(PATTERN) };

    // SAFETY: as above; a region that is still mutable holds no share right.
    let (share_mutable, _) = unsafe { call(REGION_SHARE, mutable, 0) };
    // SAFETY: as above.
    let (freeze_status, frozen) = unsafe { call(REGION_FREEZE, mutable, 0) };
    let rehandled = u64::from(frozen != mutable && freeze_status == OK);
    // SAFETY: as above; the presented handle named a generation that has moved.
    let (stale_mutable, _) = unsafe { call(CAPABILITY_RELEASE, mutable, 0) };
    // SAFETY: as above; the frozen form carries no write right.
    let (refreeze, _) = unsafe { call(REGION_FREEZE, frozen, 0) };
    // SAFETY: read-only now, at the address it was always at.
    let kept = u64::from(unsafe { word_at(at) } == PATTERN);

    // SAFETY: as above.
    let (share_status, shared) = unsafe { call(REGION_SHARE, frozen, 0) };
    let reshaped = u64::from(shared != frozen && share_status == OK);
    // SAFETY: as above; `share` consumed the affine form.
    let (stale_frozen, _) = unsafe { call(CAPABILITY_RELEASE, frozen, 0) };
    // SAFETY: as above; a shared region carries `read` and nothing else.
    let (reshare, _) = unsafe { call(REGION_SHARE, shared, 0) };
    // SAFETY: as above.
    let (freeze_shared, _) = unsafe { call(REGION_FREEZE, shared, 0) };
    // SAFETY: the window did not move: `share` changes who may name the region
    // and not where it is.
    let after_share = u64::from(unsafe { word_at(at) } == PATTERN);

    // A second name in the same process, made by ordinary attenuation — which
    // an affine region refuses and a shared one does not.
    // SAFETY: as above.
    let (alias_status, alias) =
        unsafe { call(CAPABILITY_ATTENUATE, shared, tos_launch::RIGHT_READ as u64) };
    // SAFETY: as above.
    let (dropped_alias, _) = unsafe { call(CAPABILITY_RELEASE, alias, 0) };
    // Two names, one window: dropping one of them leaves the memory readable.
    // SAFETY: the other name still holds the mapping open.
    let survived = u64::from(unsafe { word_at(at) } == PATTERN);
    // SAFETY: as above; the last name takes the window with it, so nothing
    // reads this address afterwards.
    let (last_name, _) = unsafe { call(CAPABILITY_RELEASE, shared, 0) };

    report.line(&alloc::format!(
        "TOS.RUN.REGION.STATE share_mutable={share_mutable} freeze={freeze_status} \
rehandled={rehandled} stale_mutable={stale_mutable} refreeze={refreeze} kept={kept} \
share={share_status} reshaped={reshaped} stale_frozen={stale_frozen} reshare={reshare} \
freeze_shared={freeze_shared} after_share={after_share} alias={alias_status} \
dropped_alias={dropped_alias} survived={survived} last_name={last_name}"
    ));
}

/// A full capability table refuses operation 17 before anything moves.
///
/// **A region with no handle naming it is a region nobody can use or return**,
/// so the slot is found before the authority is charged and the backing laid
/// down. The way to ask whether that is true is to fill the table — with
/// ordinary aliases of an authority this process already holds — and then ask
/// for a region.
///
/// What proves nothing moved is not this line alone: the boot's own account
/// closes at the end, the pool returns to the root's frame count and the
/// reserve to its baseline. A charge taken here and not given back, or a lane
/// built and left, would show up there.
#[cfg(feature = "test-memory-authority")]
fn region_table_full(report: &mut Report, parent: u64) {
    let mut aliases = [0u64; 32];
    let mut held = 0;
    while held < aliases.len() {
        // SAFETY: `capability_attenuate` names a capability this process holds
        // and does nothing when it refuses.
        let (status, alias) =
            unsafe { call(CAPABILITY_ATTENUATE, parent, tos_launch::RIGHT_SPEND as u64) };
        if status != OK {
            break;
        }
        aliases[held] = alias;
        held += 1;
    }
    // SAFETY: as above; the table is full, so there is no slot for the handle a
    // region would have to be named by.
    let (full, _) = unsafe { call(REGION_ALLOCATE, parent, 4096) };
    // One slot back, and the same request succeeds — which is what says the
    // refusal was about the table and not about the authority.
    let mut freed = E_NO_CAPABILITY;
    if held > 0 {
        held -= 1;
        // SAFETY: as above.
        freed = unsafe { call(CAPABILITY_RELEASE, aliases[held], 0) }.0;
    }
    // SAFETY: as above.
    let (after, region) = unsafe { call(REGION_ALLOCATE, parent, 4096) };
    if after == OK {
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, region, 0) };
    }
    for alias in aliases[..held].iter() {
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, *alias, 0) };
    }
    report.line(&alloc::format!(
        "TOS.RUN.REGION.TABLE aliases={held} full={full} freed={freed} after={after}"
    ));
}

/// Writes one region record into the argument region's transfer area.
///
/// A sender fills in the handle and nothing else: the base and the length it
/// might write are its own address and mean nothing in another address space,
/// so the nucleus ignores them.
///
/// SAFETY: `area` is this process's argument region and `index` is inside the
/// contract's maximum.
#[cfg(any(feature = "test-region-transport", feature = "test-build-topology"))]
// SAFETY: the caller names its own region and an index the contract admits.
unsafe fn set_region_handle(area: u64, index: usize, handle: u64) {
    // SAFETY: per the caller's contract; the offset is the one `IPC_V1` fixes.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::MessageRegion>(
            (area + tos_launch::MESSAGE_REGIONS) as usize,
        )
        .add(index)
        .write(tos_launch::MessageRegion {
            handle,
            base: 0,
            length: 0,
        })
    };
}

/// Reads one back: the receiver's own handle, the address the nucleus chose in
/// this address space, and the charged and mapped length.
///
/// # Safety
///
/// As [`set_region_handle`].
#[cfg(any(feature = "test-region-transport", feature = "test-build-topology"))]
// SAFETY: as above.
unsafe fn region_handed_over(area: u64, index: usize) -> tos_launch::MessageRegion {
    // SAFETY: per the caller's contract.
    unsafe {
        core::ptr::with_exposed_provenance::<tos_launch::MessageRegion>(
            (area + tos_launch::MESSAGE_REGIONS) as usize,
        )
        .add(index)
        .read()
    }
}

/// The capability the launcher bound to `name`, if this process was given one.
#[cfg(any(feature = "test-region-transport", feature = "test-region-faults"))]
fn bound<'a>(held: &'a [LaunchCapability], name: &str) -> Option<&'a LaunchCapability> {
    held.iter().find(|capability| named(capability) == name)
}

/// The marks each region carries, so that what arrives can be shown to be what
/// was written rather than merely the right size.
#[cfg(feature = "test-region-transport")]
const SHARED_MARK: u64 = 0x5348_4152_4544_5f31;
#[cfg(feature = "test-region-transport")]
const SENT_MARK: u64 = 0x4d4f_5645_445f_5f31;
#[cfg(feature = "test-region-transport")]
const TAIL_MARK: u64 = 0x4d4f_5645_445f_5f32;

/// How many times a half gives up its quantum waiting for the other before
/// deciding this is not the boot it was built for.
///
/// A bound rather than a spin: a gate that hangs reports nothing, and a half
/// that gave up says so on the log, where the other half's silence is visible
/// beside it.
#[cfg(feature = "test-region-transport")]
const PATIENCE: u32 = 4096;

/// The region transport round, whichever half this process is (`IPC_V1` §5, §6).
///
/// Two processes over one endpoint, and which half this one is follows from
/// what its launcher gave it — the worker was endowed a memory authority it can
/// make regions out of, the peer was endowed the right to receive. Neither can
/// do the other's half, and neither was given anything it could have obtained
/// on its own.
///
/// Answers whether this process was one of the two.
#[cfg(not(feature = "test-region-transport"))]
fn region_transport(_launch: &Launch, _report: &mut Report, _held: &[LaunchCapability]) -> bool {
    false
}

#[cfg(feature = "test-region-transport")]
fn region_transport(launch: &Launch, report: &mut Report, held: &[LaunchCapability]) -> bool {
    match (bound(held, "memory"), bound(held, "endpoint")) {
        (Some(memory), Some(peer)) => {
            let sink = bound(held, "sink").map_or(0, |sink| sink.handle);
            region_worker(launch, report, memory.handle, peer.handle, sink);
            true
        }
        (None, Some(peer)) if peer.rights & tos_launch::RIGHT_RECEIVE != 0 => {
            region_peer(launch, report, peer.handle);
            true
        }
        _ => false,
    }
}

/// The sending half: it makes the regions, is refused what must be refused, and
/// hands one over linearly before dying on the address it no longer owns.
///
/// The order is the evidence, and each step is what the one before it makes
/// possible:
///
/// 1. a **shared** region goes first, because it is the non-consuming case: the
///    sender keeps its handle and its window, and the queue takes a reference
///    of its own;
/// 2. a **mutable** region is offered and refused whole. `Region<mut T>` is
///    neither shareable nor transferable (ADR-0037), so the message does not
///    travel with that record dropped — it does not travel at all — and the
///    sender still holds and can still write what it offered;
/// 3. a region count past the contract's bound is `E_BAD_ARGUMENT` and consumes
///    nothing, because two is a constant of `IPC_V1` §3 the caller knew before
///    it called;
/// 4. a send onto a **full** queue refuses and takes nothing. This is the case
///    the whole transaction is shaped for: a linear region taken away and then
///    discovered to have nowhere to go is a region belonging to nobody, and no
///    rollback can be relied on to put it back — rebuilding the window needs
///    page tables and can fail on its own;
/// 5. and the same handle then sends successfully, which is what proves step 4
///    left it whole.
///
/// The last act is a read of the address the region used to be at. It faults,
/// and it is meant to: `IPC_V1` §9.6 wants the sender's loss of the mapping
/// demonstrated by a fault on its next access, and this process has said
/// everything it has to say by then.
#[cfg(feature = "test-region-transport")]
fn region_worker(launch: &Launch, report: &mut Report, memory: u64, peer: u64, sink: u64) {
    let area = launch.arguments_base;

    // --- a message of ordinary capabilities, first ---------------------------
    // **All-or-nothing is a property of every message, not only of the ones
    // carrying regions.** The receive path used to grant until something
    // refused and write a zero handle for the rest, which is a partial delivery
    // with a success status. So the first thing the peer is offered — while its
    // table is deliberately full — carries a capability and no region at all,
    // and what must happen to it is exactly what happens to a region: refused
    // whole, still queued, delivered when there is room.
    //
    // Delegation copies, so this process keeps `sink` and goes on using it
    // below.
    // SAFETY: the area is this process's own and index 0 is inside the bound.
    unsafe { set_transferred(area, 0, sink) };
    // SAFETY: `endpoint_send` names a capability this process holds and does
    // nothing when it refuses.
    let (delegated_sent, _) = unsafe { call_transferring(ENDPOINT_SEND, peer, 0, 1, 0) };

    // --- a shared region, sent without being given up ------------------------
    // SAFETY: every call below names a capability this process holds, and each
    // does nothing when it refuses.
    let (made, mutable) = unsafe { call(REGION_ALLOCATE, memory, 4096) };
    let shared_record = region_record(launch);
    if made != OK {
        report.line("TOS.RUN.REGION.WORKER unstartable=allocate");
        return;
    }
    let shared_at = shared_record.base as usize;
    // SAFETY: the nucleus mapped this range writable in this address space and
    // reported its base and length here.
    unsafe { core::ptr::with_exposed_provenance_mut::<u64>(shared_at).write_volatile(SHARED_MARK) };
    // SAFETY: as above.
    let (froze_shared, frozen_shared) = unsafe { call(REGION_FREEZE, mutable, 0) };
    // SAFETY: as above.
    let (shared_status, shared) = unsafe { call(REGION_SHARE, frozen_shared, 0) };
    // A second local name for it, which only a shared region admits.
    // SAFETY: as above.
    let (alias_status, alias) =
        unsafe { call(CAPABILITY_ATTENUATE, shared, tos_launch::RIGHT_READ as u64) };
    // SAFETY: the area is this process's own and index 0 is inside the bound.
    unsafe { set_region_handle(area, 0, shared) };
    // SAFETY: as above.
    let (shared_sent, _) = unsafe { call_transferring(ENDPOINT_SEND, peer, 0, 0, 1) };
    // Non-consuming: the sender keeps the handle and the window.
    // SAFETY: still mapped, still read-only, still this process's.
    let shared_kept = u64::from(unsafe { word_at(shared_at) } == SHARED_MARK);
    // SAFETY: as above.
    let (alias_dropped, _) = unsafe { call(CAPABILITY_RELEASE, alias, 0) };
    // One window, several names: dropping one of them changes nothing.
    // SAFETY: as above.
    let shared_after_alias = u64::from(unsafe { word_at(shared_at) } == SHARED_MARK);

    // --- a mutable region, refused whole -------------------------------------
    // SAFETY: as above.
    let (built, movable) = unsafe { call(REGION_ALLOCATE, memory, 2 * 4096) };
    let record = region_record(launch);
    if built != OK || record.length < 16 {
        report.line("TOS.RUN.REGION.WORKER unstartable=second-allocate");
        return;
    }
    let base = record.base as usize;
    let tail = (record.base + record.length - 8) as usize;
    // SAFETY: the nucleus mapped this range writable in this address space.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u64>(base).write_volatile(SENT_MARK);
        core::ptr::with_exposed_provenance_mut::<u64>(tail).write_volatile(TAIL_MARK);
    }
    // SAFETY: the area is this process's own.
    unsafe { set_region_handle(area, 0, movable) };
    // SAFETY: as above; a mutable region may not travel at all.
    let (mutable_refused, _) = unsafe { call_transferring(ENDPOINT_SEND, peer, 0, 0, 1) };
    // Refused whole means the sender still has it, and still writable.
    // SAFETY: as above.
    let still_writable = unsafe {
        let at = core::ptr::with_exposed_provenance_mut::<u64>(base);
        at.write_volatile(SENT_MARK);
        u64::from(at.read_volatile() == SENT_MARK)
    };

    // --- a count past the bound ----------------------------------------------
    // SAFETY: as above; three is past `MAX_TRANSFERRED_REGIONS`.
    let (overcount, _) = unsafe {
        call_transferring(
            ENDPOINT_SEND,
            peer,
            0,
            0,
            tos_launch::MAX_TRANSFERRED_REGIONS + 1,
        )
    };

    // --- a full queue --------------------------------------------------------
    // The sink endpoint has no receiver at all, so what is put on it stays
    // there: the only way to ask what a full queue does to a linear transfer is
    // to have a queue nothing can drain.
    // SAFETY: as above.
    let (frozen_status, frozen) = unsafe { call(REGION_FREEZE, movable, 0) };
    let mut filled = 0;
    let mut queue_full = OK;
    for _ in 0..8 {
        // SAFETY: as above; `rdx` carries a send's flags.
        let (status, _) = unsafe { call5(ENDPOINT_SEND, sink, 0, NON_BLOCKING, 0, 0) };
        if status != OK {
            queue_full = status;
            break;
        }
        filled += 1;
    }
    // SAFETY: the area is this process's own.
    unsafe { set_region_handle(area, 0, frozen) };
    // SAFETY: as above; asked not to wait, onto a queue with no room.
    let (refused_full, _) = unsafe { call5(ENDPOINT_SEND, sink, 0, NON_BLOCKING, 0, 1) };
    // The refusal took nothing: the window is still here and still readable.
    // SAFETY: as above.
    let intact = u64::from(unsafe { word_at(base) } == SENT_MARK);

    // --- and the same handle sends -------------------------------------------
    // SAFETY: the area is this process's own; the handle is the one the full
    // queue refused a moment ago.
    unsafe { set_region_handle(area, 0, frozen) };
    // SAFETY: as above.
    let (sent, _) = unsafe { call_transferring(ENDPOINT_SEND, peer, 0, 0, 1) };
    // SAFETY: as above; the send consumed the handle, so it names a generation
    // that has moved on.
    let (stale, _) = unsafe { call(CAPABILITY_RELEASE, frozen, 0) };

    report.line(&alloc::format!(
        "TOS.RUN.REGION.WORKER delegated_sent={delegated_sent} froze_shared={froze_shared} shared={shared_status} \
alias={alias_status} shared_sent={shared_sent} shared_kept={shared_kept} \
alias_dropped={alias_dropped} after_alias={shared_after_alias} \
mutable_refused={mutable_refused} still_writable={still_writable} overcount={overcount} \
frozen={frozen_status} filled={filled} queue_full={queue_full} \
refused_full={refused_full} intact={intact} sent={sent} stale={stale} \
base=0x{base:x} length={length}",
        length = record.length
    ));

    // The address is not this process's any more. `IPC_V1` §9.6 asks for the
    // sender's loss of the mapping to be demonstrated by a fault on its next
    // access, and this is that access — deliberate, last, and after everything
    // this process had to say is on the log.
    // SAFETY: nothing about this read is safe, and that is the point: the
    // nucleus took this window away with the handle, so the fault is the
    // evidence.
    let fell = unsafe { word_at(base) };
    report.line(&alloc::format!(
        "TOS.RUN.REGION.WORKER.UNREACHED read=0x{fell:x}"
    ));
}

/// The receiving half: it proves that acceptance is all-or-nothing before it
/// proves that a region arrives.
///
/// The table is filled first, deliberately, so the first thing this process
/// asks of a queued message is one it cannot be given. What must happen then is
/// `E_LIMIT` **and nothing else**: no partial delivery, no zero handle written
/// where authority should be, and the message still on the queue for the
/// attempt that follows one freed slot. That is the property `IPC_V1` §3 states
/// for every message and that a receive granting until something refused could
/// never have had.
///
/// It polls rather than blocks, and the reason is the property above: a
/// receiver blocked on a message it cannot accept cannot run to make room for
/// it. Blocking is right for a receiver waiting for *a* message and wrong for
/// one deliberately unable to take the one that is there.
#[cfg(feature = "test-region-transport")]
fn region_peer(launch: &Launch, report: &mut Report, endpoint: u64) {
    let area = launch.arguments_base;

    // Fill this process's table, so that the message waiting for it cannot be
    // accepted. Ordinary aliases of the one capability it holds: no authority
    // is invented, and each is a name for the same endpoint.
    let mut aliases = [0u64; 32];
    let mut held = 0;
    while held < aliases.len() {
        // SAFETY: `capability_attenuate` names a capability this process holds
        // and does nothing when it refuses.
        let (status, alias) = unsafe {
            call(
                CAPABILITY_ATTENUATE,
                endpoint,
                tos_launch::RIGHT_RECEIVE as u64,
            )
        };
        if status != OK {
            break;
        }
        aliases[held] = alias;
        held += 1;
    }

    // Wait for a message to be there, and be refused it. `E_WOULD_BLOCK` says
    // nothing has arrived; `E_LIMIT` says something has and this process cannot
    // take it, which is exactly the state the next step undoes.
    let mut refused = OK;
    let mut waited = 0;
    while waited < PATIENCE {
        // SAFETY: `endpoint_receive` names this process's own capability and
        // writes only its own argument region; `rsi` carries the flags.
        let (status, _) = unsafe { call(ENDPOINT_RECEIVE, endpoint, NON_BLOCKING) };
        if status == E_WOULD_BLOCK {
            // SAFETY: self-only, and gives up the rest of this quantum.
            unsafe { call(CONTEXT_YIELD, 0, 0) };
            waited += 1;
            continue;
        }
        refused = status;
        break;
    }

    // One slot back, and the same message arrives — with its capability in it.
    // This one carries no region at all, which is the point: what was refused
    // and then delivered whole is an **ordinary** delegation.
    let mut freed = E_NO_CAPABILITY;
    if held > 0 {
        held -= 1;
        // SAFETY: as above.
        freed = unsafe { call(CAPABILITY_RELEASE, aliases[held], 0) }.0;
    }
    let (delegated_status, _) = receive_when_ready(endpoint);
    // SAFETY: the area is this process's own and index 0 is inside the bound.
    let delegated = unsafe { transferred(area, 0) };

    // The second message: the shared region.
    if held > 0 {
        held -= 1;
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, aliases[held], 0) };
    }
    let (first_status, first_length) = receive_when_ready(endpoint);
    // SAFETY: as above.
    let shared = unsafe { region_handed_over(area, 0) };
    let shared_read = read_mark(&shared);

    // The third: the affine region, whose sender is gone by the time anything
    // is read from it.
    if held > 0 {
        held -= 1;
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, aliases[held], 0) };
    }
    let (second_status, _) = receive_when_ready(endpoint);
    // SAFETY: as above.
    let moved = unsafe { region_handed_over(area, 0) };
    let moved_read = read_mark(&moved);
    let moved_tail = if moved.handle != 0 && moved.length >= 16 {
        // SAFETY: the nucleus mapped this range read-only in this address space
        // and reported its base and length here.
        unsafe {
            core::ptr::with_exposed_provenance::<u64>((moved.base + moved.length - 8) as usize)
                .read_volatile()
        }
    } else {
        0
    };
    // The lanes are the nucleus's choice in *this* address space, and a
    // region's identity is not an address: two regions arriving here land in
    // two different lanes because they are two different slots.
    let distinct = u64::from(shared.base != moved.base && shared.handle != moved.handle);

    // **Waiting for the sender to be gone, by the one mechanism that can say
    // so.** Nothing lets one process observe another's death, and polling a
    // tick count would be a guess dressed as a measurement. What is exact is
    // ADR-0059's liveness rule: a blocking receive is cancelled at the instant
    // no context is runnable and nothing routed can change that — which, in a
    // boot of two processes where the other one never blocks, is the instant
    // the other one has ended. So this blocks on a message that will never come
    // and reads `E_CANCELLED` as "I am the only one left".
    //
    // Everything after it is therefore a statement about a region whose sender
    // has faulted and been reclaimed: its address space is gone, its handles
    // are gone, and the bytes are still here.
    // SAFETY: `endpoint_receive` names this process's own capability; the flags
    // are zero, which is the blocking form.
    let (alone, _) = unsafe { call(ENDPOINT_RECEIVE, endpoint, 0) };
    let shared_after = read_mark(&shared);
    let moved_after = read_mark(&moved);

    report.line(&alloc::format!(
        "TOS.RUN.REGION.PEER aliases={held} refused={refused} freed={freed} \
delegated={delegated_status} handle=0x{delegated:x} \
first={first_status} first_length={first_length} shared_read=0x{shared_read:x} \
shared_base=0x{shared_base:x} shared_length={shared_length} second={second_status} \
moved_read=0x{moved_read:x} moved_tail=0x{moved_tail:x} moved_length={moved_length} \
distinct={distinct} alone={alone} shared_after=0x{shared_after:x} \
moved_after=0x{moved_after:x} waited={waited}",
        shared_base = shared.base,
        shared_length = shared.length,
        moved_length = moved.length
    ));

    // Everything back: the aliases, then both regions. What proves the backing
    // is reclaimed exactly once is not this line — it is the boot's own account
    // closing at the end, with the pool back to the root's frame count and the
    // reserve back to its baseline.
    for alias in aliases[..held].iter() {
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, *alias, 0) };
    }
    if shared.handle != 0 {
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, shared.handle, 0) };
    }
    if moved.handle != 0 {
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, moved.handle, 0) };
    }
    if delegated != 0 {
        // SAFETY: as above.
        unsafe { call(CAPABILITY_RELEASE, delegated, 0) };
    }
}

/// Receives when there is something to receive, giving up the quantum until
/// there is.
#[cfg(feature = "test-region-transport")]
fn receive_when_ready(endpoint: u64) -> (i64, u64) {
    for _ in 0..PATIENCE {
        // SAFETY: `endpoint_receive` names this process's own capability and
        // writes only its own argument region.
        let answer = unsafe { call(ENDPOINT_RECEIVE, endpoint, NON_BLOCKING) };
        if answer.0 != E_WOULD_BLOCK {
            return answer;
        }
        // SAFETY: self-only.
        unsafe { call(CONTEXT_YIELD, 0, 0) };
    }
    (E_WOULD_BLOCK, 0)
}

/// The first word of a region this process was handed, or zero when it was
/// handed none.
#[cfg(feature = "test-region-transport")]
fn read_mark(record: &tos_launch::MessageRegion) -> u64 {
    if record.handle == 0 || record.length < 8 {
        return 0;
    }
    // SAFETY: the nucleus mapped this range read-only and not executable in
    // this address space, and reported its base and length here.
    unsafe { word_at(record.base as usize) }
}

/// The two ways a region stops authorising an access, each in a process of its
/// own (ADR-0075 §5a, `SYSTEM_ABI_V1` §5).
///
/// **Dedicated processes, because both of them end in a fault.** A fault is the
/// evidence here rather than a failure, and a fault in a process that was also
/// doing something else would be a fault nobody could attribute. So each of
/// these does one thing, says what it is about to do, and dies doing it.
///
/// Which one this process is follows from the name its launcher bound its
/// authority to. Both hold a name for the same child authority — two names, one
/// budget — and neither can reach anything the other made.
///
/// Answers whether this process was one of the two.
#[cfg(not(feature = "test-region-faults"))]
fn region_faults(_launch: &Launch, _report: &mut Report, _held: &[LaunchCapability]) -> bool {
    false
}

#[cfg(feature = "test-region-faults")]
fn region_faults(launch: &Launch, report: &mut Report, held: &[LaunchCapability]) -> bool {
    if let Some(memory) = bound(held, "nx") {
        region_not_executable(launch, report, memory.handle);
        return true;
    }
    if let Some(memory) = bound(held, "stale") {
        region_after_release(launch, report, memory.handle);
        return true;
    }
    false
}

/// A region is data, and stays data (`SYSTEM_ABI_V1` §5, operation 17).
///
/// Operation 17 maps what it allocates writable and **not executable**, and
/// that pairing is the whole of it: memory a process may write is memory a
/// process may not run. A region that could be written and then entered would
/// be a process able to author its own text, which is the one thing the
/// verified-image path exists to prevent — and no amount of verification above
/// matters if the boundary below hands out a page that is both.
///
/// So this writes an instruction into a region and jumps to it. What must
/// happen is a page fault at CPL 3 with the instruction-fetch bit set, at the
/// address it jumped to, and this process ending there.
#[cfg(feature = "test-region-faults")]
fn region_not_executable(launch: &Launch, report: &mut Report, memory: u64) {
    // SAFETY: `region_allocate` names a capability this process holds and does
    // nothing when it refuses.
    let (made, region) = unsafe { call(REGION_ALLOCATE, memory, 4096) };
    let record = region_record(launch);
    if made != OK || record.length == 0 {
        report.line("TOS.RUN.REGION.NX unstartable=1");
        return;
    }
    // `ret` — one byte, valid, and enough to tell "the processor refused to
    // fetch this" from "the processor fetched rubbish and faulted on that".
    // SAFETY: the nucleus mapped this range writable in this address space and
    // reported its base and length here.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u8>(record.base as usize).write_volatile(0xc3)
    };
    let wrote = u64::from(
        // SAFETY: as above.
        unsafe { core::ptr::with_exposed_provenance::<u8>(record.base as usize).read_volatile() }
            == 0xc3,
    );
    report.line(&alloc::format!(
        "TOS.RUN.REGION.NX handle=0x{region:x} base=0x{base:x} length={length} wrote={wrote}",
        base = record.base,
        length = record.length
    ));
    // SAFETY: nothing about this is safe, and that is the evidence. The bytes
    // are there and the mapping is there; what is not there is permission to
    // fetch an instruction from it, which is a fact about the leaf rather than
    // about this image.
    let entry: extern "C" fn() =
        unsafe { core::mem::transmute::<*const (), extern "C" fn()>(record.base as *const ()) };
    entry();
    report.line("TOS.RUN.REGION.NX.UNREACHED returned=1");
}

/// A mapping is derived authority and does not outlive it (ADR-0075 §5a).
///
/// Releasing the handle to a region and leaving its window mapped would be the
/// capability model bypassed in one line: the process would go on reading
/// memory it holds no authority over at all. So `capability_release` takes the
/// window with the handle, and the only way to see that from outside the
/// nucleus is to read the address afterwards.
#[cfg(feature = "test-region-faults")]
fn region_after_release(launch: &Launch, report: &mut Report, memory: u64) {
    const MARK: u64 = 0x5354_414c_455f_5f31;
    // SAFETY: as above.
    let (made, region) = unsafe { call(REGION_ALLOCATE, memory, 4096) };
    let record = region_record(launch);
    if made != OK || record.length == 0 {
        report.line("TOS.RUN.REGION.STALE unstartable=1");
        return;
    }
    let at = record.base as usize;
    // SAFETY: the nucleus mapped this range writable in this address space.
    let wrote = unsafe {
        let cell = core::ptr::with_exposed_provenance_mut::<u64>(at);
        cell.write_volatile(MARK);
        u64::from(cell.read_volatile() == MARK)
    };
    // SAFETY: as above; this is the only handle naming the region.
    let (released, _) = unsafe { call(CAPABILITY_RELEASE, region, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.REGION.STALE base=0x{base:x} length={length} wrote={wrote} released={released}",
        base = record.base,
        length = record.length
    ));
    // SAFETY: as in the neighbour above — the read is the evidence. The handle
    // is gone, so the window is gone, so this faults.
    let read = unsafe { word_at(at) };
    report.line(&alloc::format!(
        "TOS.RUN.REGION.STALE.UNREACHED read=0x{read:x}"
    ));
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

    // Under the cost constants this process does its half of the exchange and
    // **nothing else**, so that what the nucleus counts is the exchange rather
    // than the exchange plus everything below. Returning here is what makes the
    // boot an instrument: every probe below crosses the same edge.
    if exchange_only(launch, report, first) {
        return;
    }

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

    // The region transport round, when this boot is the one built for it. Asked
    // before the ordinary halves because its two processes are told apart by
    // their whole endowment rather than by the kind of their first capability:
    // the worker holds a memory authority *and* the right to send, which no
    // other constant grants together.
    if region_transport(launch, report, held) {
        return;
    }
    // ADR-0074's build worker, told apart the same way: it holds an authority to
    // spend and the right to send, and no authority over any process. A
    // supervisor holds the opposite, and neither can be mistaken for the other.
    if build_role(launch, report, held) {
        return;
    }
    // And the two dedicated fault processes, for the same reason: each is told
    // apart by what its whole endowment is bound to rather than by the kind of
    // its first capability.
    if region_faults(launch, report, held) {
        return;
    }

    // A memory authority: the one kind whose scope is a quantity, and the one
    // whose amount moves. Everything below is done from CPL 3 through the
    // ordinary edge, because a reservation model proved only in a host test is
    // a model nothing has actually asked the nucleus for.
    if first.object == tos_launch::OBJECT_MEMORY_AUTHORITY {
        memory_authority(launch, report, first);
        return;
    }

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
        //
        // **Asked of a live operation.** This used to name `process_create`
        // (8), which is now retired and answers `E_NOT_SUPPORTED` whatever
        // handle it is given — so the probe would have passed without the type
        // ever being compared. `process_terminate` (9) names a process object,
        // this handle names an endpoint, and the refusal is the comparison.
        // SAFETY: `process_terminate` names the process it ends; this handle
        // names no process.
        let (wrong_type, _) = unsafe { call(PROCESS_TERMINATE, first.handle, 0) };
        report.line(&alloc::format!(
            "TOS.RUN.CAPABILITY.TYPE operation=9 status={wrong_type}"
        ));
    }
    if first.object == tos_launch::OBJECT_PROCESS {
        // **Creation is funded, so a creator needs two capabilities.** The
        // authority over a process says it may create; the memory authority
        // pays for what it creates. A process holding only the first can create
        // nothing, which is the whole of ADR-0076 §4 seen from ring 3.
        let memory = held
            .iter()
            .find(|capability| capability.object == tos_launch::OBJECT_MEMORY_AUTHORITY)
            .map_or(0, |capability| capability.handle);
        // ADR-0073's handoff has its own supervisor: it builds an artifact,
        // shares it, and creates targets from it. A boot built for that is not
        // also the boot that exercises ordinary creation, because the two need
        // different numbers of live processes out of the same four slots.
        #[cfg(feature = "test-bundle-launch")]
        bundle_supervisor(launch, report, first.handle, memory);
        // ADR-0074's T1: this process holds authority over a process, so it is
        // the supervisor. The worker holds no such thing and takes the other
        // branch below.
        #[cfg(feature = "test-build-topology")]
        {
            let inbox = held
                .iter()
                .find(|capability| capability.object == tos_launch::OBJECT_ENDPOINT)
                .map_or(0, |capability| capability.handle);
            build_supervisor(launch, report, first.handle, memory, inbox);
        }
        #[cfg(feature = "test-funding-lifecycle")]
        supervise(launch, report, first.handle, memory, first.rights);
        #[cfg(not(any(
            feature = "test-funding-lifecycle",
            feature = "test-lifecycle",
            feature = "test-bundle-launch"
        )))]
        let _ = (launch, memory);
        // ADR-0067's arrangement has three roles and one image. Which one this
        // process is, is the name its authority was bound to (ADR-0061): the
        // supervisor was given "control", the middle parent "parent", and the
        // delegated observer "watch". A role read from a binding is a role
        // somebody granted; a role read from a slot index would be a guess.
        #[cfg(all(feature = "test-lifecycle", not(feature = "test-bundle-launch")))]
        match named(first) {
            "parent" => lifecycle_parent(launch, report, first.handle, memory),
            "watch" => lifecycle_watcher(launch, report, first.handle),
            #[cfg(feature = "test-lifecycle-delegate")]
            _ => lifecycle_arrangement(launch, report, first.handle, memory),
            #[cfg(not(feature = "test-lifecycle-delegate"))]
            _ => lifecycle(launch, report, first.handle, memory),
        }
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

/// On a boot that is not being measured, nothing.
#[cfg(any(
    not(feature = "test-measurement-port"),
    feature = "test-measurement-ipc"
))]
fn measure_channel(_launch: &Launch, _report: &mut Report) {}

/// COM1, and the four registers this protocol touches.
#[cfg(feature = "test-measurement-port")]
mod wire {
    pub const DATA: u16 = 0x3f8;
    pub const LINE_STATUS: u16 = 0x3fd;
    pub const DATA_READY: u8 = 0x01;
    pub const TRANSMITTER_EMPTY: u8 = 0x20;
}

/// The instrument's half of the protocol, as this side sees it.
///
/// **Every protocol byte has its high bit set.** The same wire carries the boot
/// log, which is ASCII, so a marker inside 0x00..0x7f could be a letter of a log
/// line and an observer could not tell. It also proves the line is transparent
/// to eight bits: if it were not, `READY` would not arrive.
///
/// A request carries a four-bit sequence number and a one-bit work selector in
/// its low five bits, and **both** markers echo the complete tag. The echo is
/// what makes a sample causal rather than coincidental: a pair that does not
/// name the exact request it followed invalidates the series.
///
/// The measured interval is between the two markers this side emits, and the
/// request that starts a sample is deliberately outside it. Milestone 1 measured
/// why: a byte travelling *into* the machine costs 30 µs median and 94 µs p99,
/// because QEMU delivers it from its main loop while this process hammers the
/// line-status register from the vCPU thread. A byte travelling *out* leaves
/// synchronously with the instruction that wrote it. An interval built from two
/// outward markers therefore carries neither the inward path nor its jitter.
#[cfg(feature = "test-measurement-port")]
mod protocol {
    /// Host to guest: begin sample `n`.
    pub const GO: u8 = 0xc0;
    /// Guest to host: the interval opens.
    pub const OPEN: u8 = 0x80;
    /// Guest to host: the interval closes.
    pub const CLOSE: u8 = 0xa0;
    /// In a paired calibration request, execute the denominator call.
    pub const WORK: u8 = 0x10;
    /// Sequence identity within either request class.
    pub const SEQUENCE: u8 = 0x0f;
    /// The complete request identity echoed by both markers.
    pub const TAG: u8 = WORK | SEQUENCE;
    /// Host to guest: no more samples.
    pub const STOP: u8 = 0xe0;
    /// Guest to host, once, before the first request may be sent.
    ///
    /// Without it the observer would be talking to the firmware: OVMF drives
    /// this same UART during boot and consumes what arrives on it, so a request
    /// sent before this process is listening is not late — it is gone.
    pub const READY: u8 = 0xff;

    pub fn is_go(byte: u8) -> bool {
        byte & 0xe0 == GO
    }
}

/// Reads one byte of COM1's receive register, waiting for it.
///
/// # Safety
///
/// The TSS I/O bitmap of the measurement nucleus permits CPL 3 exactly these
/// ports; every other port still faults.
#[cfg(feature = "test-measurement-port")]
// SAFETY: the caller runs only under the measurement feature whose nucleus
// permits exactly COM1 through the TSS I/O bitmap.
unsafe fn wire_read() -> u8 {
    loop {
        let status: u8;
        // SAFETY: per this function's contract.
        unsafe {
            core::arch::asm!("in al, dx", out("al") status, in("dx") wire::LINE_STATUS,
                             options(nomem, nostack, preserves_flags));
        }
        if status & wire::DATA_READY != 0 {
            let byte: u8;
            // SAFETY: as above.
            unsafe {
                core::arch::asm!("in al, dx", out("al") byte, in("dx") wire::DATA,
                                 options(nomem, nostack, preserves_flags));
            }
            return byte;
        }
        core::hint::spin_loop();
    }
}

/// Empties the receive register of anything that arrived before now.
///
/// # Safety
///
/// As `wire_read`.
#[cfg(feature = "test-measurement-port")]
// SAFETY: as `wire_read`; every access is confined to COM1.
unsafe fn wire_drain() {
    loop {
        let status: u8;
        // SAFETY: per this function's contract.
        unsafe {
            core::arch::asm!("in al, dx", out("al") status, in("dx") wire::LINE_STATUS,
                             options(nomem, nostack, preserves_flags));
        }
        if status & wire::DATA_READY == 0 {
            return;
        }
        let _discarded: u8;
        // SAFETY: as above.
        unsafe {
            core::arch::asm!("in al, dx", out("al") _discarded, in("dx") wire::DATA,
                             options(nomem, nostack, preserves_flags));
        }
    }
}

/// Puts one byte on the wire.
///
/// The transmitter-empty poll comes **first**, so that the write itself is the
/// last instruction before the byte leaves and nothing this side does after it
/// lands inside the interval the observer measures.
///
/// # Safety
///
/// As `wire_read`.
#[cfg(feature = "test-measurement-port")]
// SAFETY: as `wire_read`; every access is confined to COM1.
unsafe fn wire_write(byte: u8) {
    loop {
        let status: u8;
        // SAFETY: per this function's contract.
        unsafe {
            core::arch::asm!("in al, dx", out("al") status, in("dx") wire::LINE_STATUS,
                             options(nomem, nostack, preserves_flags));
        }
        if status & wire::TRANSMITTER_EMPTY != 0 {
            break;
        }
        core::hint::spin_loop();
    }
    // SAFETY: as above.
    unsafe {
        core::arch::asm!("out dx, al", in("dx") wire::DATA, in("al") byte,
                         options(nomem, nostack, preserves_flags));
    }
}

/// Answers the instrument until it says stop.
///
/// What sits between the `GO` and the `DONE` is the whole of what is being
/// measured, and under this constant alone it is **nothing**: the floor of the
/// channel is the first thing that has to be known, because every later reading
/// contains it and none of them may have it subtracted (ADR-0066).
///
/// This process makes no system call inside the loop, which is the reason the
/// marker is a port write rather than a report line: a line would put the
/// nucleus inside the measurement.
#[cfg(all(
    feature = "test-measurement-port",
    not(feature = "test-measurement-ipc")
))]
fn measure_channel(launch: &Launch, report: &mut Report) {
    let mut work = Work::prepare(launch, report);
    let mut answered = 0u32;
    // Anything already in the receive register belongs to whoever was using
    // this line before this process existed.
    // SAFETY: the measurement nucleus permits CPL 3 these ports.
    unsafe { wire_drain() };
    // SAFETY: as above.
    unsafe { wire_write(protocol::READY) };
    loop {
        // SAFETY: as above.
        let request = unsafe { wire_read() };
        if request == protocol::STOP {
            break;
        }
        if !protocol::is_go(request) {
            continue;
        }
        let tag = request & protocol::TAG;
        // The interval opens here and closes below, and between them is the
        // whole of what is being measured. Nothing else may go between: no
        // report line, no system call, no second thought.
        work.mark(tag);
        work.perform(tag & protocol::WORK != 0);
        answered += 1;
    }
    report.line(&alloc::format!(
        "TOS.RUN.MEASURE.ANSWERED samples={answered}{}",
        work.summary()
    ));
}

/// What the instrument measures.
///
/// Nothing, in the channel-validation build: the floor of the channel is what
/// that boot is for, and a floor with work in it is not a floor.
#[cfg(all(
    feature = "test-measurement-port",
    not(feature = "test-measurement-call"),
    not(feature = "test-measurement-ipc")
))]
struct Work(u8);

#[cfg(all(
    feature = "test-measurement-port",
    not(feature = "test-measurement-call"),
    not(feature = "test-measurement-ipc")
))]
impl Work {
    fn prepare(_launch: &Launch, _report: &mut Report) -> Work {
        Work(0)
    }

    /// Which sample the next marks belong to.
    fn mark(&mut self, tag: u8) {
        self.0 = tag;
    }

    /// The two marks with nothing between them: the floor of the channel, and
    /// the same two writes the engine makes when there is a call between them.
    fn perform(&mut self, _measure_call: bool) {
        // SAFETY: the measurement nucleus permits CPL 3 these ports.
        unsafe { wire_write(protocol::OPEN | (self.0 & protocol::TAG)) };
        // SAFETY: as above.
        unsafe { wire_write(protocol::CLOSE | (self.0 & protocol::TAG)) };
    }

    fn summary(&self) -> alloc::string::String {
        alloc::string::String::new()
    }
}

/// One TOS Core call inside a run that has already started — the denominator
/// `IPC_V1` §8 defines, measured by the instrument that measures the numerator.
///
/// **Where the marks are.** Not here: they are taken inside the engine,
/// immediately around the execution of one `Op::Call`, through the two methods
/// ADR-0066 adds to `System` under `measurement-marks`. Everything a run does
/// once — the receipt and digest check, the entry lookup, the arity check, the
/// capability grants, the engine's construction and its resource state, the
/// worker reservation — happens before the first mark and is not in any sample.
///
/// The module's `bench(value)` therefore exists to make the measured call an
/// ordinary one: its single statement is `measured(value)`, and that call is the
/// `Op::Call` the marks bracket.
#[cfg(feature = "test-measurement-call")]
struct Work {
    /// The benchmark's verified closure: images, records and membership, with a
    /// bounded resident set beside them. Prepared once, outside every sample.
    ///
    /// `'static` because `Prepared::launch` encodes each module into owned image
    /// bytes and returns a closure that borrows nothing — which is what lets the
    /// lowered module be dropped immediately below, and what keeps the resident
    /// set the only thing alive across a sample.
    prepared: tos_pipeline::Prepared<'static>,
    argument: tos_engine::Value,
    system: Marked,
    performed: u32,
    refused: u32,
}

/// The system the measured run reaches: nothing, and two marks.
///
/// It grants no capability and performs no operation — the benchmark asks for
/// neither — so the only thing it does is put a byte on the wire at the two
/// instants the engine hands it.
#[cfg(feature = "test-measurement-call")]
struct Marked {
    sequence: u8,
}

#[cfg(feature = "test-measurement-call")]
impl tos_pipeline::System for Marked {
    fn granted(&mut self, _request: tos_pipeline::CapabilityRequest<'_>) -> Option<Handle> {
        None
    }

    fn reach(&mut self, call: tos_pipeline::Reach<'_>) -> Result<tos_engine::Value, Trap> {
        Err(Trap::new(
            "RUNTIME_OPERATION_NOT_IMPLEMENTED",
            "the benchmark reaches nothing",
            call.source,
        ))
    }

    fn mark_before_call(&mut self) {
        // SAFETY: the measurement nucleus permits CPL 3 these ports.
        unsafe { wire_write(protocol::OPEN | (self.sequence & protocol::TAG)) };
    }

    fn mark_after_call(&mut self) {
        // SAFETY: as above.
        unsafe { wire_write(protocol::CLOSE | (self.sequence & protocol::TAG)) };
    }
}

/// Where the benchmark's canonical text lives in the capsule.
#[cfg(feature = "test-measurement-call")]
const BENCH_PATH: &str = "system/bench/call.tos";
/// The entry whose one statement is the call being measured, and the eight
/// fields that make the 64 bytes.
#[cfg(feature = "test-measurement-call")]
const BENCH_ENTRY: &str = "bench";
#[cfg(feature = "test-measurement-call")]
const BENCH_FIELDS: usize = 8;
/// One module resident, which is the whole of this closure.
#[cfg(feature = "test-measurement-call")]
const BENCH_RESIDENCY: tos_pipeline::ResidencyLimits = tos_pipeline::ResidencyLimits {
    modules: 1,
    bytes: 32 * 1024 * 1024,
};

#[cfg(feature = "test-measurement-call")]
impl Work {
    fn prepare(launch: &Launch, report: &mut Report) -> Work {
        let bytes = unit_bytes(launch, BENCH_PATH).unwrap_or_else(|| {
            report.line("TOS.RUN.MEASURE.UNSTARTABLE reason=no-benchmark-module");
            exit(EXIT_UNSTARTABLE)
        });
        let source = tos_core::SourceReader::read(bytes).unwrap_or_else(|_| {
            report.line("TOS.RUN.MEASURE.UNSTARTABLE reason=benchmark-not-transport-valid");
            exit(EXIT_UNSTARTABLE)
        });
        let Some(schema) = tos_core::Parser::parse_schema(&source).into_accepted() else {
            report.line("TOS.RUN.MEASURE.UNSTARTABLE reason=benchmark-not-parsed");
            exit(EXIT_UNSTARTABLE)
        };
        if tos_core::Checker::check(&source, &schema)
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error)
        {
            report.line("TOS.RUN.MEASURE.UNSTARTABLE reason=benchmark-not-checked");
            exit(EXIT_UNSTARTABLE)
        }
        let context = tos_core::ModuleContext {
            source_set: alloc::string::String::from("measurement"),
            path: alloc::string::String::from(BENCH_PATH),
            content_id: tos_pipeline::content_id(bytes),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        };
        let Ok(module) = tos_core::lower_module(&source, &schema, &context) else {
            report.line("TOS.RUN.MEASURE.UNSTARTABLE reason=benchmark-not-lowered");
            exit(EXIT_UNSTARTABLE)
        };
        // Encode, verify the image, keep the record and the membership, release
        // the module. The preparation is here, outside every sample, so that
        // what the marks bracket is one `Op::Call` and nothing around it.
        let Ok(prepared) = tos_pipeline::Prepared::launch(
            &[&module],
            &tos_verifier::ResolutionSnapshot::default(),
            BENCH_ENTRY,
            BENCH_RESIDENCY,
        ) else {
            report.line("TOS.RUN.MEASURE.UNSTARTABLE reason=benchmark-not-verified");
            exit(EXIT_UNSTARTABLE)
        };
        drop(module);
        // Sixty-four bytes as the record declares them: eight `i64` in order.
        let argument = tos_engine::Value::Aggregate(
            (0..BENCH_FIELDS)
                .map(|field| tos_engine::Value::Int(tos_pipeline::IntKind::I64, field as i128))
                .collect(),
        );
        Work {
            prepared,
            argument,
            system: Marked { sequence: 0 },
            performed: 0,
            refused: 0,
        }
    }

    /// Which sample the engine's marks belong to.
    fn mark(&mut self, tag: u8) {
        self.system.sequence = tag;
    }

    /// Starts a run and lets the engine mark the call inside it.
    ///
    /// The run itself is *not* the sample: the marks are taken by the engine,
    /// around the one `Op::Call` this module makes, and everything before the
    /// first mark is startup this measurement deliberately excludes.
    fn perform(&mut self, measure_call: bool) {
        if !measure_call {
            // The adjacent floor uses the exact same prepared process, UART,
            // observer and trace window. Only the immutable work selector
            // differs, and no work lies between these marks.
            self.system.mark_before_call();
            self.system.mark_after_call();
            return;
        }
        match self
            .prepared
            .run(alloc::vec![self.argument.clone()], &mut self.system)
        {
            Ok(Ok(_)) => self.performed += 1,
            _ => self.refused += 1,
        }
    }

    /// What the calls did, so that a run whose engine refused every one of them
    /// cannot be read as a run that measured them.
    fn summary(&self) -> alloc::string::String {
        alloc::format!(" calls={} refused={}", self.performed, self.refused)
    }
}

/// The bytes of one unit of the launch record, by canonical path.
#[cfg(feature = "test-measurement-call")]
fn unit_bytes<'a>(launch: &'a Launch, wanted: &str) -> Option<&'a [u8]> {
    for index in 0..launch.unit_count as usize {
        // SAFETY: the launcher states the record holds `unit_count` units at
        // `units`, mapped readable, each naming bytes it also mapped.
        let unit = unsafe {
            &*core::ptr::with_exposed_provenance::<LaunchUnit>(launch.units as usize).add(index)
        };
        // SAFETY: as above.
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
        if path == wanted.as_bytes() {
            return Some(bytes);
        }
    }
    None
}

/// On a boot that is not measuring an exchange, nothing (and it says so).
#[cfg(not(any(
    feature = "test-exchange-cost",
    feature = "test-reply-receive-refusals"
)))]
fn exchange_only(_launch: &Launch, _report: &mut Report, _first: &LaunchCapability) -> bool {
    false
}

/// The half of one exchange this process was given the authority for.
///
/// Under the cost constants this is the **whole** of what the process does with
/// the system, which is what `IPC_V1` §9.7 needs and what the boot below the
/// counters could not otherwise be: an exchange measured inside a boot that also
/// polls, probes and delegates is a subtraction, and a subtraction is an
/// estimate wearing a counter's clothes.
#[cfg(any(
    feature = "test-exchange-cost",
    feature = "test-reply-receive-refusals"
))]
fn exchange_only(launch: &Launch, report: &mut Report, first: &LaunchCapability) -> bool {
    if first.object != tos_launch::OBJECT_ENDPOINT {
        report.line("TOS.RUN.EXCHANGE.UNSTARTABLE reason=not-an-endpoint");
        return true;
    }
    if first.rights & tos_launch::RIGHT_CALL != 0 {
        asking(launch, report, first.handle);
    } else if first.rights & tos_launch::RIGHT_RECEIVE != 0 {
        answering(launch, report, first.handle);
    } else {
        report.line("TOS.RUN.EXCHANGE.UNSTARTABLE reason=neither-half");
    }
    true
}

/// What one exchange carries, in both directions.
#[cfg(all(
    any(
        feature = "test-exchange-cost",
        feature = "test-reply-receive-refusals"
    ),
    not(feature = "test-measurement-ipc")
))]
const QUESTION: &[u8] = b"what-does-an-exchange-cost";
#[cfg(all(
    any(
        feature = "test-exchange-cost",
        feature = "test-reply-receive-refusals"
    ),
    not(feature = "test-measurement-ipc")
))]
const ANSWER: &[u8] = b"four-crossings";
#[cfg(feature = "test-measurement-ipc")]
const QUESTION: &[u8] = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
#[cfg(feature = "test-measurement-ipc")]
const ANSWER: &[u8] = b"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// How many questions the client asks.
///
/// Two boots differ in nothing but this number, and that is what makes the
/// measurement a **slope**. Entering the server's loop and leaving it cost one
/// crossing each and belong to no exchange; a difference between two boots
/// cancels them without anybody having to decide which two they were.
#[cfg(all(
    feature = "test-exchange-cost",
    not(feature = "test-more-exchanges"),
    not(feature = "test-measurement-ipc")
))]
const EXCHANGES: usize = 1;
#[cfg(all(
    feature = "test-exchange-cost",
    feature = "test-more-exchanges",
    not(feature = "test-measurement-ipc")
))]
const EXCHANGES: usize = 3;
/// The measured server answers one priming exchange and then one per block of
/// the latency series: three warm-ups and the retained samples (ADR-0068
/// section 5). It is written as that sum rather than as the total, because a
/// bare number here was sized for a 21-sample series and stopped the server
/// mid-run when the series grew — the client's next call then found nobody
/// runnable and the liveness rule ended the boot, which is the gate failing
/// closed rather than reporting a short series.
#[cfg(feature = "test-measurement-ipc")]
const LATENCY_WARMUPS: usize = 3;
#[cfg(feature = "test-measurement-ipc")]
const LATENCY_SAMPLES: usize = 300;
#[cfg(feature = "test-measurement-ipc")]
const EXCHANGES: usize = 1 + LATENCY_WARMUPS + LATENCY_SAMPLES;
/// The refusal boot asks twice: the first question is what the refusals are
/// tried against, and the second is what the server is waiting for while it
/// tries them.
#[cfg(feature = "test-reply-receive-refusals")]
const EXCHANGES: usize = 2;

/// Puts the question in the argument region.
#[cfg(any(
    feature = "test-exchange-cost",
    feature = "test-reply-receive-refusals"
))]
fn put(region: u64, bytes: &[u8]) {
    for (offset, byte) in bytes.iter().enumerate() {
        // SAFETY: `arguments_base` names a writable mapping of
        // `arguments_length` bytes made by the launcher, and both constants
        // above are far inside it.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(region as usize)
                .add(offset)
                .write(*byte)
        };
    }
}

/// Whether the region holds the answer the server sends.
#[cfg(any(
    feature = "test-exchange-cost",
    feature = "test-reply-receive-refusals"
))]
fn is_answer(region: u64, length: u64) -> bool {
    if length as usize != ANSWER.len() {
        return false;
    }
    // SAFETY: the nucleus states it wrote `length` bytes into the region the
    // launch record names, and `length` is inside the contract's inline bound.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(region as usize),
            length as usize,
        )
    };
    bytes == ANSWER
}

/// Whether the server received the complete declared request payload.
#[cfg(feature = "test-measurement-ipc")]
fn is_question(region: u64, length: u64) -> bool {
    if length as usize != QUESTION.len() {
        return false;
    }
    // SAFETY: the nucleus returned `length` only after writing that many bytes
    // into this process's argument region; the IPC contract bounds it to the
    // inline message capacity.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(region as usize),
            length as usize,
        )
    };
    bytes == QUESTION
}

/// The client: [`EXCHANGES`] questions, one operation each, and nothing else.
///
/// It counts its own operations, so the nucleus's count of what crossed the edge
/// can be held against the two processes' count of what they asked for. A
/// counter nobody can check from the other side is a number the nucleus tells
/// about itself.
#[cfg(any(
    all(feature = "test-exchange-cost", not(feature = "test-measurement-ipc")),
    feature = "test-reply-receive-refusals"
))]
fn asking(launch: &Launch, report: &mut Report, handle: u64) {
    let mut answered = 0;
    let mut operations = 0;
    for exchange in 0..EXCHANGES {
        put(launch.arguments_base, QUESTION);
        // The second question carries this process's own endpoint capability —
        // authority with `call` and not `receive` — so that the server can be
        // asked to wait on something it demonstrably holds and that
        // demonstrably lacks the right the operation declares (ADR-0063).
        let delegating = cfg!(feature = "test-reply-receive-refusals") && exchange == 1;
        if delegating {
            // SAFETY: the argument region is this process's own, and index 0 is
            // inside the contract's maximum.
            unsafe { set_transferred(launch.arguments_base, 0, handle) };
        }
        let carried = u64::from(delegating);
        // SAFETY: `endpoint_call` names its endpoint and the request's length;
        // the request is in the region the launch record names, and no pointer
        // crosses.
        let (status, length) =
            unsafe { call_transferring(ENDPOINT_CALL, handle, QUESTION.len() as u64, carried, 0) };
        operations += 1;
        if status == OK && is_answer(launch.arguments_base, length) {
            answered += 1;
        }
    }
    report.line(&alloc::format!(
        "TOS.RUN.EXCHANGE.ASKED asked={EXCHANGES} answered={answered} operations={operations}"
    ));
}

/// The measured client: prime the steady-state server, then execute exactly one
/// real 64-byte request/reply inside each externally requested interval.
#[cfg(feature = "test-measurement-ipc")]
fn asking(launch: &Launch, report: &mut Report, handle: u64) {
    put(launch.arguments_base, QUESTION);
    // The unmeasured prime proves the server has reached its atomic
    // reply-and-receive loop before READY is emitted. Without it the first
    // sample could include server-loop startup rather than a steady-state
    // exchange.
    // SAFETY: this process holds the endpoint call capability, the request is
    // in its bounded argument region, and the zero transfer count reads no
    // capability slots.
    let (prime_status, prime_length) =
        unsafe { call_transferring(ENDPOINT_CALL, handle, QUESTION.len() as u64, 0, 0) };
    if prime_status != OK || !is_answer(launch.arguments_base, prime_length) {
        report.line("TOS.RUN.MEASURE.IPC.UNSTARTABLE reason=prime-refused");
        return;
    }

    let mut answered = 0u32;
    let mut refused = 0u32;
    // Anything received before READY belongs to firmware or an earlier user of
    // the line, never to this protocol.
    // SAFETY: the measurement nucleus grants CPL 3 exactly COM1's ports.
    unsafe { wire_drain() };
    // SAFETY: as above.
    unsafe { wire_write(protocol::READY) };
    loop {
        // SAFETY: as above.
        let request = unsafe { wire_read() };
        if request == protocol::STOP {
            break;
        }
        if !protocol::is_go(request) {
            continue;
        }
        let tag = request & protocol::TAG;
        put(launch.arguments_base, QUESTION);
        // SAFETY: as above. The request itself and payload preparation are
        // outside the interval; the complete endpoint_call is inside it.
        unsafe { wire_write(protocol::OPEN | tag) };
        // SAFETY: the same endpoint, bounded argument region and zero transfer
        // count as the validated prime are used for every measured call.
        let (status, length) =
            unsafe { call_transferring(ENDPOINT_CALL, handle, QUESTION.len() as u64, 0, 0) };
        // SAFETY: as above. No work lies between the completed call and CLOSE.
        unsafe { wire_write(protocol::CLOSE | tag) };
        if status == OK && is_answer(launch.arguments_base, length) {
            answered += 1;
        } else {
            refused += 1;
        }
    }
    report.line(&alloc::format!(
        "TOS.RUN.MEASURE.IPC samples={} answered={answered} refused={refused} request_bytes={} reply_bytes={} primed=1",
        answered + refused,
        QUESTION.len(),
        ANSWER.len()
    ));
}

/// The server: one wait to begin with, and then one operation per exchange.
///
/// The loop ends the only way it can end — the wait nobody will satisfy is
/// cancelled by ADR-0059's liveness rule once the client has gone — and what it
/// returns is reported, because `E_CANCELLED` arriving *after* an answer was
/// delivered is ADR-0063's rule that a cancellation cannot un-answer.
#[cfg(any(
    all(feature = "test-exchange-cost", not(feature = "test-measurement-ipc")),
    feature = "test-reply-receive-refusals"
))]
fn answering(launch: &Launch, report: &mut Report, endpoint: u64) {
    // SAFETY: `endpoint_receive` names its endpoint and its flags; this waits.
    let (mut status, _) = unsafe { call(ENDPOINT_RECEIVE, endpoint, 0) };
    let mut operations = 1;
    let mut answered = 0;
    let mut spent = 0;
    while status == OK {
        // The right to answer arrives in the last slot of the transfer table,
        // always (`IPC_V1` §4).
        // SAFETY: the argument region is this process's own.
        let reply = unsafe {
            transferred(
                launch.arguments_base,
                tos_launch::MAX_TRANSFERRED_CAPABILITIES as usize - 1,
            )
        };
        refusals(launch, report, reply, endpoint, answered, spent);
        put(launch.arguments_base, ANSWER);
        let (next, _) = answer_and_wait(reply, endpoint);
        operations += operations_per_answer();
        answered += 1;
        // What this iteration spent, for the next one to try again with.
        spent = reply;
        status = next;
    }
    report.line(&alloc::format!(
        "TOS.RUN.EXCHANGE.SERVED answered={answered} last={status} operations={operations}"
    ));
}

/// The measured server is already waiting before every retained client marker.
#[cfg(feature = "test-measurement-ipc")]
fn answering(launch: &Launch, report: &mut Report, endpoint: u64) {
    // SAFETY: the endpoint is this process's receive endowment.
    let (mut status, mut length) = unsafe { call(ENDPOINT_RECEIVE, endpoint, 0) };
    let mut answered = 0usize;
    let mut refused = 0usize;
    let mut handled = 0usize;
    while status == OK && handled < EXCHANGES {
        let valid_request = is_question(launch.arguments_base, length);
        // SAFETY: the received call places its single-use reply in the fixed
        // final transfer slot.
        let reply = unsafe {
            transferred(
                launch.arguments_base,
                tos_launch::MAX_TRANSFERRED_CAPABILITIES as usize - 1,
            )
        };
        put(launch.arguments_base, ANSWER);
        handled += 1;
        let (next_status, next_length) = answer_and_wait(reply, endpoint);
        // `reply_receive` answers before it waits. On the final exchange the
        // client receives the answer, emits CLOSE, consumes STOP and exits;
        // only then may liveness return E_CANCELLED to this now-unsatisfiable
        // wait. The delivered answer does not depend on that next wait's
        // status.
        if valid_request {
            answered += 1;
        } else {
            refused += 1;
        }
        status = next_status;
        length = next_length;
    }
    report.line(&alloc::format!(
        "TOS.RUN.MEASURE.IPC.SERVER served={answered} refused={refused} payload_bytes={} last={status}",
        QUESTION.len(),
    ));
}

/// Answers the call and waits for the next message, as **one** operation.
#[cfg(all(
    feature = "test-reply-receive",
    any(
        feature = "test-exchange-cost",
        feature = "test-reply-receive-refusals"
    )
))]
fn answer_and_wait(reply: u64, endpoint: u64) -> (i64, u64) {
    // SAFETY: `SYSTEM_ABI_V1` §5 row 13 — the reply in `rdi`, the endpoint in
    // `rsi`, the answer's length in `rdx`, flags in `r10`. Both handles were
    // granted to this process: the endpoint by its launcher, the reply by the
    // call it is about to answer.
    unsafe {
        call4(
            ENDPOINT_REPLY_RECEIVE,
            reply,
            endpoint,
            ANSWER.len() as u64,
            0,
        )
    }
}

/// The same thing as **two**: the shape this ABI had before operation 13.
#[cfg(all(
    not(feature = "test-reply-receive"),
    any(
        feature = "test-exchange-cost",
        feature = "test-reply-receive-refusals"
    )
))]
fn answer_and_wait(reply: u64, endpoint: u64) -> (i64, u64) {
    // SAFETY: `endpoint_reply` names the reply capability and the answer's
    // length.
    let (status, value) = unsafe { call(ENDPOINT_REPLY, reply, ANSWER.len() as u64) };
    if status != OK {
        return (status, value);
    }
    // SAFETY: `endpoint_receive` names its endpoint and its flags; this waits.
    unsafe { call(ENDPOINT_RECEIVE, endpoint, 0) }
}

/// How many operations one answer costs the server: the whole difference.
#[cfg(all(
    feature = "test-reply-receive",
    not(feature = "test-measurement-ipc"),
    any(
        feature = "test-exchange-cost",
        feature = "test-reply-receive-refusals"
    )
))]
fn operations_per_answer() -> u64 {
    1
}
#[cfg(all(
    not(feature = "test-reply-receive"),
    not(feature = "test-measurement-ipc"),
    any(
        feature = "test-exchange-cost",
        feature = "test-reply-receive-refusals"
    )
))]
fn operations_per_answer() -> u64 {
    2
}

/// Nothing, on the boots that are counting rather than probing.
#[cfg(all(feature = "test-exchange-cost", not(feature = "test-measurement-ipc")))]
fn refusals(
    _launch: &Launch,
    _report: &mut Report,
    _reply: u64,
    _endpoint: u64,
    _answered: u64,
    _spent: u64,
) {
}

/// Every way `endpoint_reply_receive` must refuse, asked of a live reply.
///
/// The point each one turns on is that **nothing is delivered**: a refusal that
/// spent the reply, or answered the caller, or entered a wait, would be a
/// half-performed operation, which is the state ADR-0063 says this operation
/// exists to make impossible. That is not asserted here — it is proved by what
/// happens next, because the answer that follows uses the same reply capability
/// and succeeds.
#[cfg(feature = "test-reply-receive-refusals")]
fn refusals(
    launch: &Launch,
    report: &mut Report,
    reply: u64,
    endpoint: u64,
    answered: u64,
    spent: u64,
) {
    let length = ANSWER.len() as u64;
    if answered == 0 {
        // The two capabilities, the other way round. Neither position accepts
        // the other's object, and the caller holds both — which is the case a
        // check on "did you pass two handles" would miss.
        // SAFETY: an assigned operation, named with handles this process holds.
        let (swapped, _) = unsafe { call4(ENDPOINT_REPLY_RECEIVE, endpoint, reply, length, 0) };
        // A reply handle that names nothing, with a good endpoint.
        // SAFETY: as above, with an index outside this process's table.
        let (no_reply, _) = unsafe { call4(ENDPOINT_REPLY_RECEIVE, ABSENT, endpoint, length, 0) };
        // A good reply with an endpoint handle that names nothing. This is the
        // one that decides whether the operation is atomic: the reply is
        // resolvable and the operation must still deliver nothing.
        // SAFETY: as above.
        let (no_endpoint, _) = unsafe { call4(ENDPOINT_REPLY_RECEIVE, reply, ABSENT, length, 0) };
        // Holding `receive` on an endpoint and a reply for a call on it is not
        // holding `send` (`CAPABILITY_V1` §7.4): the pair grants nothing the two
        // did not grant separately.
        // SAFETY: `endpoint_send` names its endpoint and a payload length.
        let (sending, _) = unsafe { call(ENDPOINT_SEND, endpoint, length) };
        report.line(&alloc::format!(
            "TOS.RUN.EXCHANGE.REFUSED swapped={swapped} no_reply={no_reply} \
             no_endpoint={no_endpoint} sending={sending}"
        ));
        return;
    }
    // The second question carried the client's own endpoint capability: an
    // endpoint this process now holds, with `call` and without `receive`. So
    // this names a real object of the right kind under a right the operation
    // declares and this handle does not carry.
    // SAFETY: the argument region is this process's own; slot 0 is where the
    // client wrote what it transferred.
    let carried = unsafe { transferred(launch.arguments_base, 0) };
    // SAFETY: an assigned operation, named with handles this process holds.
    let (no_right, _) = unsafe { call4(ENDPOINT_REPLY_RECEIVE, reply, carried, length, 0) };
    // And the reply the *first* exchange already spent. Single use is a property
    // of the capability, so this is refused — and refused **before** any wait is
    // entered, which the line after it appearing at all is what proves.
    // SAFETY: as above, with a handle whose object is gone.
    let (again, _) = unsafe { call4(ENDPOINT_REPLY_RECEIVE, spent, endpoint, length, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.EXCHANGE.REFUSED carried=0x{carried:x} no_right={no_right} spent=0x{spent:x} \
         again={again}"
    ));
}

/// A handle index outside any process's table (`CAPABILITY_V1` §7.2).
#[cfg(feature = "test-reply-receive-refusals")]
const ABSENT: u64 = 0xdead_beef;

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
        unsafe { call_transferring(ENDPOINT_CALL, handle, question.len() as u64, 1, 0) };
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

/// The middle of the arrangement: a process with children of its own.
///
/// It creates one child, lets it end, and then keeps its ending uncollected on
/// purpose. When this process is itself ended, that uncollected notice has no
/// receiver left — ADR-0067 §10 — and the nucleus says so on the log rather
/// than letting a slot become eternal.
#[cfg(feature = "test-lifecycle")]
fn lifecycle_parent(
    launch: &Launch,
    report: &mut Report,
    handle: u64,
    #[cfg_attr(not(feature = "test-lifecycle-collector"), allow(unused_variables))] memory: u64,
) {
    // Two scenarios need a middle parent and they need opposite things of it.
    //
    // The cancellation one needs it childless: an ending delivered to the
    // waiting observer would answer the wait rather than test what happens when
    // the relation itself ends. The collection one needs exactly one child,
    // ending while the observer is already blocked — which is the case a
    // delivery that looked only at the parent would have got wrong.
    #[cfg(feature = "test-lifecycle-collector")]
    {
        let module = b"system/boot/init.tos";
        write_module_name(launch, module);
        // Late enough that the observer has reached its wait; the child then
        // ends on its own while that wait is blocked.
        settle();
        settle();
        let (created, _) =
            create_child_endowed(launch, handle, memory, module.len() as u64, &[], 0, None);
        report.line(&alloc::format!("TOS.RUN.LIFECYCLE.PARENT child={created}"));
    }
    #[cfg(not(feature = "test-lifecycle-collector"))]
    {
        let _ = (launch, handle);
        report.line("TOS.RUN.LIFECYCLE.PARENT childless=1");
    }
    loop {
        settle();
    }
}

/// The delegated observer: it watches a relation that is not its own.
///
/// `RIGHT_WAIT_CHILD` was delegated to it over another process's object, so the
/// set it waits on is that process's children. When that process ends, the set
/// can gain no further member, and ADR-0067 §9a says what the wait gets.
#[cfg(feature = "test-lifecycle")]
fn lifecycle_watcher(launch: &Launch, report: &mut Report, handle: u64) {
    report.line("TOS.RUN.LIFECYCLE.WATCHER waiting=1");
    let waited = wait_child(launch, handle, false);
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.WATCHER status={} child={}",
        waited.0,
        waited.1.child_instance
    ));
}

/// Puts a module name where a funded creation reads one.
#[cfg(any(feature = "test-lifecycle-delegate", feature = "test-build-topology"))]
fn write_module_name(launch: &Launch, module: &[u8]) {
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
}

/// ADR-0067 §9a and §10: an observer of somebody else's children, and what
/// becomes of an ending nobody is left to collect.
///
/// Three processes and one image. This one creates a middle parent, gives a
/// second child the right to watch *that* parent's children, and then ends the
/// parent. Two things must follow, and neither is arranged by this process:
///
///   - the watcher's blocked wait is answered `E_CANCELLED`, because the
///     relation it subscribed to can gain no further member;
///   - the ending the parent had not collected is released rather than holding
///     a slot forever, and the nucleus says so on the log.
///
/// A separate boot from the collection scenario because it needs three live
/// processes at once, and each memory grant takes the largest contiguous run
/// there is.
#[cfg(feature = "test-lifecycle-delegate")]
fn lifecycle_arrangement(launch: &Launch, report: &mut Report, handle: u64, memory: u64) {
    let module = b"system/boot/init.tos";
    write_module_name(launch, module);
    let name_length = module.len() as u64;

    // The middle parent, told by its binding which role it is, and given the
    // three rights a parent needs over itself.
    write_self_binding(launch, b"parent");
    // It is given one capability of its own: a name for the **same** memory
    // authority this process is funding out of. Nothing is inherited — a child
    // funded from an authority receives no name for it unless its creator says
    // so — and this creator says so, because a parent that cannot fund cannot
    // create.
    let (parent_status, parent_handle) = create_child_endowed(
        launch,
        handle,
        memory,
        name_length,
        &[(memory, tos_launch::RIGHT_SPEND, b"memory")],
        u64::from(
            tos_launch::RIGHT_CREATE | tos_launch::RIGHT_TERMINATE | tos_launch::RIGHT_WAIT_CHILD,
        ),
        Some(LIFECYCLE_FIRST_GENERATION),
    );
    let parent_instance = created_instance(launch);
    settle();

    // The watcher gets one capability: the right to observe the parent's
    // children, and nothing else — not create, not terminate. Attenuation is
    // what makes that a subset rather than a promise.
    // SAFETY: `capability_attenuate` names the capability it attenuates and a
    // rights mask.
    let (attenuated, watch_handle) = unsafe {
        call(
            CAPABILITY_ATTENUATE,
            parent_handle,
            u64::from(tos_launch::RIGHT_WAIT_CHILD),
        )
    };
    // No rights over itself: this child is an observer, and an observer that
    // could end things would be something else.
    let (watcher_status, _) = create_child_endowed(
        launch,
        handle,
        memory,
        name_length,
        &[(watch_handle, tos_launch::RIGHT_WAIT_CHILD, b"watch")],
        0,
        Some(LIFECYCLE_FIRST_GENERATION),
    );
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.ARRANGED parent={parent_status}/{parent_instance} \
attenuate={attenuated} watcher={watcher_status}"
    ));
    // Long enough for the watcher to start, reach its wait and block in it.
    // Three processes share the turns here, and a child of this image spends
    // most of its first fifty ticks getting through reader, checker, verifier
    // and engine before it makes its first call.
    settle();
    settle();
    settle();

    // In the collection scenario the parent stays alive: what is tested there
    // is a delivery to a blocked collector that is not the parent, and ending
    // the parent would cancel that wait instead of answering it.
    #[cfg(feature = "test-lifecycle-collector")]
    {
        settle();
        settle();
        settle();
        report.line("TOS.RUN.LIFECYCLE.ARRANGED collector=1");
        // The parent loops forever by design, so somebody has to end it or the
        // machine never halts. It is ended after the collection it was staged
        // for, which is why this is cleanup rather than part of the test.
        // SAFETY: the handle names the middle parent this process created.
        unsafe { call(PROCESS_TERMINATE, parent_handle, 0) };
        wait_child(launch, handle, false);
        return;
    }
    // And now the parent ends. Everything after this is the nucleus's doing.
    // SAFETY: the handle names the middle parent this process created.
    #[allow(unreachable_code)]
    let (ended, _) = unsafe { call(PROCESS_TERMINATE, parent_handle, 0) };
    // Blocking, because ending a process is not retiring it: the scheduler's
    // loop does that, and blocking is how this process reaches it.
    // What comes back is the parent's own ending — the earliest by ending
    // order, since the watcher is cancelled *by* that retirement and so ends
    // after it.
    let collected = wait_child(launch, handle, false);
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.ORPHANED ended={ended} collected={} child={} kind={}",
        collected.0,
        collected.1.child_instance,
        collected.1.ending_kind
    ));
    // And then this process ends without collecting the watcher's ending.
    // Nobody is left who could: §10 releases that notice rather than holding a
    // slot for a reader that has itself ended, and the nucleus says so.
    settle();
}

/// Writes the name a child's authority over itself is bound to (ADR-0061).
#[cfg(feature = "test-lifecycle-delegate")]
fn write_self_binding(launch: &Launch, name: &[u8]) {
    let mut binding = [0u8; tos_launch::MAX_BINDING as usize];
    binding[..name.len()].copy_from_slice(name);
    // SAFETY: the slot is at a fixed offset in this process's own argument
    // region, which the launcher mapped writable.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<[u8; tos_launch::MAX_BINDING as usize]>(
            (launch.arguments_base + tos_launch::CREATE_SELF_BINDING) as usize,
        )
        .write(binding)
    };
}

/// ADR-0067: a supervisor that outlives its children and collects their endings.
///
/// One boot, five phases, each a conformance test of the decision. The table
/// holds four processes and this supervisor is one of them, so three children
/// exist at a time — which is what makes the exhaustion phase a real bound
/// rather than a simulated one.
#[cfg(all(feature = "test-lifecycle", not(feature = "test-lifecycle-delegate")))]
fn lifecycle(launch: &Launch, report: &mut Report, handle: u64, memory: u64) {
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
    let name_length = module.len() as u64;

    // Phase 1 — two children that ended before a single wait.
    //
    // Both are created with an asserted generation, both are ended by this
    // process, and only then is anything collected. Neither record may be lost
    // and their order is the order they ended in, not the order of the slots
    // they happened to occupy.
    let (first_status, first_handle) = create_child(
        launch,
        handle,
        memory,
        name_length,
        LIFECYCLE_FIRST_GENERATION,
    );
    let first_instance = created_instance(launch);
    let (second_status, second_handle) = create_child(
        launch,
        handle,
        memory,
        name_length,
        LIFECYCLE_SECOND_GENERATION,
    );
    let second_instance = created_instance(launch);
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.CREATED first={first_status}/{first_instance} \
second={second_status}/{second_instance}"
    ));
    // SAFETY: each handle names the child this process just created.
    let ended = unsafe {
        (
            call(PROCESS_TERMINATE, first_handle, 0).0,
            call(PROCESS_TERMINATE, second_handle, 0).0,
        )
    };
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.ENDED first={} second={}",
        ended.0,
        ended.1
    ));
    let first_record = wait_child(launch, handle, false);
    let second_record = wait_child(launch, handle, false);
    // A third, with nothing left to collect. Non-blocking, because a blocking
    // one here would be answered by the liveness rule and end the boot — which
    // phase 5 does deliberately, at the end.
    let empty = wait_child(launch, handle, true);
    report_record(report, "first", &first_record);
    report_record(report, "second", &second_record);
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.EMPTY status={}",
        empty.0
    ));

    // Phase 2 — a stale capability does not come back to life.
    //
    // The first child's slot is free now: its record was collected. Whoever
    // takes that slot is a different process, and the handle this supervisor
    // still holds over the ended child must refuse rather than name the new
    // occupant (ADR-0067 §7).
    let (reused_status, reused_handle) = create_child(
        launch,
        handle,
        memory,
        name_length,
        LIFECYCLE_THIRD_GENERATION,
    );
    let reused_instance = created_instance(launch);
    // SAFETY: the handle named a process that has ended; what it names now is
    // the question this asks.
    let (stale, _) = unsafe { call(PROCESS_TERMINATE, first_handle, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.STALE created={reused_status}/{reused_instance} status={stale}"
    ));
    // That child goes back before the next phase, which is about slots: leaving
    // it alive would measure the memory bound instead.
    // The wait is blocking on purpose: a process this one ended is retired by
    // the scheduler's own loop, and blocking is how a supervisor reaches it.
    // SAFETY: the handle names the child created just above.
    unsafe { call(PROCESS_TERMINATE, reused_handle, 0) };
    wait_child(launch, handle, false);
    drain(launch, handle);

    // Phase 3 — a capability without the right cannot observe.
    //
    // Attenuation is subtractive, so this asks for everything but `wait_child`
    // and gets a handle that can still create and terminate.
    // SAFETY: `capability_attenuate` names the capability and a rights mask.
    let (attenuated_status, blind) = unsafe {
        call(
            CAPABILITY_ATTENUATE,
            handle,
            u64::from(tos_launch::RIGHT_CREATE | tos_launch::RIGHT_TERMINATE),
        )
    };
    let blind_wait = if attenuated_status == OK {
        wait_child(launch, blind, true).0
    } else {
        attenuated_status
    };
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.UNRIGHTED attenuate={attenuated_status} wait={blind_wait}"
    ));

    // Phase 4 — an uncollected ending holds its slot, and `E_LIMIT` says so.
    //
    // Three children, one at a time, each left to end itself and each left
    // uncollected. Their memory went back at their retirement; their records
    // did not, and a record is what a slot is holding. With this supervisor in
    // the fourth slot there is nowhere to put a fifth process, and the nucleus
    // says which bound it hit rather than leaving `E_LIMIT` to be guessed at.
    let mut filled = 0;
    for _ in 0..3 {
        let (status, _) = create_child(
            launch,
            handle,
            memory,
            name_length,
            LIFECYCLE_FIRST_GENERATION,
        );
        if status != OK {
            break;
        }
        filled += 1;
        settle();
    }
    let (full, _) = create_child(
        launch,
        handle,
        memory,
        name_length,
        LIFECYCLE_FIRST_GENERATION,
    );
    // One collection frees one slot, and exactly one: the other two records
    // still hold theirs.
    let collected = wait_child(launch, handle, true);
    let (after_one, _) = create_child(
        launch,
        handle,
        memory,
        name_length,
        LIFECYCLE_FIRST_GENERATION,
    );
    let (full_again, _) = create_child(
        launch,
        handle,
        memory,
        name_length,
        LIFECYCLE_FIRST_GENERATION,
    );
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.EXHAUSTED filled={filled} full={full} collected={} \
kind={} after_one={after_one} full_again={full_again}",
        collected.0,
        collected.1.ending_kind
    ));
    settle();
    drain(launch, handle);

    // Phase 4 — a child whose creator asserts **no** restart generation, and
    // which ends itself.
    //
    // One child answers two questions. Its `CreateFundedRecord` carries the
    // flag clear, which is what the retired operation 8 meant and what
    // operation 19 has to keep expressible: the record must report absence
    // rather than a zero its caller never asserted. And it is left to reach its
    // own `process_exit` instead of being ended, so the same record carries the
    // other ending kind and the self-reported status that goes with it — the
    // one a terminated child cannot have.
    //
    // Four children is what a boot affords: each grant takes the largest
    // contiguous run there is, so the runs get smaller and the fourth is the
    // last that fits. The nucleus says which bound it hit, in its own log.
    let (legacy_status, _) =
        create_child_endowed(launch, handle, memory, name_length, &[], 0, None);
    settle();
    let legacy = wait_child(launch, handle, true);
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.LEGACY created={legacy_status} status={} kind={} \
status_present={} generation_present={} generation={}",
        legacy.0,
        legacy.1.ending_kind,
        legacy.1.has_self_reported_status,
        legacy.1.has_restart_generation,
        legacy.1.restart_generation
    ));

    // Phase 5 — a wait nothing can end is ended by the nucleus.
    //
    // Everything pending is drained first, so this blocks with nothing to
    // collect and nothing else runnable. ADR-0059's liveness rule answers it,
    // and `E_CANCELLED` is exact rather than approximate.
    drain(launch, handle);
    let cancelled = wait_child(launch, handle, false);
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.CANCELLED status={}",
        cancelled.0
    ));
}

/// Gives the machine the turns a child needs to run, end and be retired.
///
/// Two operations, and each does a different half. `context_yield` hands the
/// processor to the next runnable context — since ADR-0067's implementation it
/// really does, rather than answering `OK` and keeping it — which is what gets
/// a freshly created child onto the processor promptly. What turns an *ended*
/// child into a record is the scheduler's own loop, which a yield also reaches.
///
/// The waiting is still measured by the only clock a process has
/// (`time_monotonic`, operation 11), which counts timer interrupts: ticks
/// rather than iterations, because an iteration count is a guess about a
/// machine's speed and a tick is not.
#[cfg(any(
    feature = "test-lifecycle",
    feature = "test-funding-lifecycle",
    feature = "test-bundle-launch",
    feature = "test-build-topology"
))]
fn settle() {
    let start = monotonic().unwrap_or(0);
    loop {
        // SAFETY: `context_yield` is self-only and takes nothing.
        unsafe { call(CONTEXT_YIELD, 0, 0) };
        match monotonic() {
            Some(now) if now >= start + LIFECYCLE_SETTLE_TICKS => return,
            // No clock is not a reason to spin forever.
            None => return,
            Some(_) => {}
        }
    }
}

/// How many timer interrupts a child is given to run, end and be retired.
///
/// Measured rather than guessed: a child of this image takes about sixty ticks
/// to reach its own exit, so this is that with room. Too small a number does
/// not make the test flaky in the dangerous direction — it makes the next
/// creation fail for want of memory the previous child still holds, loudly.
#[cfg(any(
    feature = "test-lifecycle",
    feature = "test-funding-lifecycle",
    feature = "test-bundle-launch",
    feature = "test-build-topology"
))]
const LIFECYCLE_SETTLE_TICKS: u64 = 150;

/// Collects every ending already recorded, without blocking for one that is not.
#[cfg(all(feature = "test-lifecycle", not(feature = "test-lifecycle-delegate")))]
fn drain(launch: &Launch, handle: u64) {
    loop {
        settle();
        if wait_child(launch, handle, true).0 != OK {
            return;
        }
    }
}

/// Generations this supervisor asserts. Distinct so a record cannot be read as
/// having come from another creation, and none of them zero — a legacy child's
/// absent generation must not be confusable with an asserted one.
#[cfg(feature = "test-lifecycle")]
const LIFECYCLE_FIRST_GENERATION: u64 = 7;
#[cfg(all(feature = "test-lifecycle", not(feature = "test-lifecycle-delegate")))]
const LIFECYCLE_SECOND_GENERATION: u64 = 9;
#[cfg(all(feature = "test-lifecycle", not(feature = "test-lifecycle-delegate")))]
const LIFECYCLE_THIRD_GENERATION: u64 = 11;

/// A child, funded from an authority this process holds, with the generation
/// its creator asserts for it (operation 19).
///
/// The three shapes the retired operations needed are one call now: whether a
/// restart lineage is asserted is a **record** rather than a second operation
/// number, and the endowment count and the child's rights over itself were
/// always arguments.
/// The arena this supervisor gives the children it creates.
///
/// **Named by the creator, which is what operation 19 made possible.** These
/// children start, run the boot module and end; the measured peak of that run is
/// under 128 KiB, and `RUNTIME_GRANT`'s 54 MiB is the reference *runtime*
/// policy rather than a floor anything needs. Sixteen megabytes is two orders of
/// magnitude of headroom over what they use and it keeps this arrangement about
/// the lifecycle rather than about arena size — the reference platform funds
/// exactly four 54 MiB processes and not one frame more, so a boot that needs
/// four of them is a boot one byte of code growth can break.
///
/// It is a policy figure and not a share of what is free: no `min(requested,
/// available)`, no remainder, no percentage. What it will not pay for is
/// `E_LIMIT`, which is what the exhaustion scenario below asks for on purpose.
#[cfg(feature = "test-lifecycle")]
const LIFECYCLE_GRANT: u64 = 16 * 1024 * 1024;

/// A child created from a plan carrying these entries.
///
/// The plan is made, written and sealed for this creation and released after
/// it. That is the *worst* case for a supervisor and the right one for a test:
/// a plan that survives a creation is the interesting property, and reusing one
/// here would prove it by accident rather than where it is asserted.
#[cfg(feature = "test-lifecycle")]
fn create_child_endowed(
    launch: &Launch,
    handle: u64,
    memory: u64,
    name_length: u64,
    entries: &[(u64, u32, &[u8])],
    own_rights: u64,
    generation: Option<u64>,
) -> (i64, u64) {
    let (sealed, plan) = sealed_plan(launch.arguments_base, handle, entries);
    if sealed != OK {
        return (sealed, 0);
    }
    // SAFETY: `handle` names a process capability this process holds with
    // `create`, `memory` a memory authority with `spend`, `plan` the plan just
    // sealed, and the module name is already in the argument region.
    let created = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            memory,
            plan,
            name_length,
            own_rights,
            LIFECYCLE_GRANT,
            generation,
        )
    };
    // SAFETY: the plan is this process's own and the creation is over.
    unsafe { call(CAPABILITY_RELEASE, plan, 0) };
    created
}

/// The same, with no endowment and no rights over itself.
#[cfg(all(feature = "test-lifecycle", not(feature = "test-lifecycle-delegate")))]
fn create_child(
    launch: &Launch,
    handle: u64,
    memory: u64,
    name_length: u64,
    generation: u64,
) -> (i64, u64) {
    create_child_endowed(
        launch,
        handle,
        memory,
        name_length,
        &[],
        0,
        Some(generation),
    )
}

/// The instance id operation 15 left in this process's argument region.
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-lifecycle",
    feature = "test-bundle-launch"
))]
fn created_instance(launch: &Launch) -> u64 {
    // SAFETY: the region is the launcher's own mapping and the offset is the
    // one the contract fixes.
    unsafe {
        core::ptr::with_exposed_provenance::<u64>(
            (launch.arguments_base + tos_launch::CREATE_INSTANCE_ID) as usize,
        )
        .read_unaligned()
    }
}

/// `process_wait_child`, and the record it left behind.
#[cfg(any(
    feature = "test-build-topology",
    feature = "test-lifecycle",
    feature = "test-bundle-launch"
))]
fn wait_child(
    launch: &Launch,
    handle: u64,
    non_blocking: bool,
) -> (i64, tos_launch::WaitChildRecord) {
    let flags = u64::from(non_blocking);
    // SAFETY: operation 14 takes the authority in `rdi` and its flags in `rsi`.
    let (status, _) = unsafe { call(PROCESS_WAIT_CHILD, handle, flags) };
    if status != OK {
        return (status, tos_launch::WaitChildRecord::default());
    }
    // SAFETY: the nucleus wrote the record at the fixed offset of this
    // process's own region, and only on success.
    let record = unsafe {
        core::ptr::with_exposed_provenance::<tos_launch::WaitChildRecord>(
            (launch.arguments_base + tos_launch::WAIT_CHILD_RECORD) as usize,
        )
        .read_unaligned()
    };
    (status, record)
}

/// One collected ending, as the log carries it.
#[cfg(all(feature = "test-lifecycle", not(feature = "test-lifecycle-delegate")))]
fn report_record(report: &mut Report, which: &str, collected: &(i64, tos_launch::WaitChildRecord)) {
    let (status, record) = collected;
    report.line(&alloc::format!(
        "TOS.RUN.LIFECYCLE.RECORD which={which} status={status} child={} parent={} kind={} \
status_present={} ended_by={}/{} generation={}/{} order={}",
        record.child_instance,
        record.parent_instance,
        record.ending_kind,
        record.has_self_reported_status,
        record.ended_by,
        record.has_ended_by,
        record.restart_generation,
        record.has_restart_generation,
        record.ending_order
    ));
}

/// Creates a process and ends it, on authority this process was given.
///
/// The lifecycle build runs its own supervisor instead of this one, which is
/// why this is compiled out there rather than left as an unused function: a
/// warning suppressed is a warning that stops being read.
#[cfg(not(feature = "test-lifecycle"))]
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
#[cfg(feature = "test-funding-lifecycle")]
fn supervise(launch: &Launch, report: &mut Report, handle: u64, memory: u64, rights: u32) {
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

    // First, a plan entry naming a capability this process does not hold. It is
    // refused **when it is written**, not when a child is created from it: a
    // plan is the decision, and a decision that could record authority its
    // author never held would be one whose sealing proved nothing.
    // SAFETY: `handle` names this process's authority over itself with
    // `create`, and the handle being delegated is deliberately not one it
    // holds.
    let (_, forging) = unsafe { launch_plan_create(handle) };
    // SAFETY: as above.
    let forged =
        unsafe { launch_plan_endow(launch.arguments_base, 0xdead_beef, forging, u32::MAX, b"x") };
    // SAFETY: as above; a builder nothing was written into is still released
    // like anything else.
    unsafe { call(CAPABILITY_RELEASE, forging, 0) };
    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.REFUSED reason=endowment-not-held status={forged}"
    ));

    // Then a child that may end itself and nothing more. This process holds
    // `create` and `terminate`; the child is given only the second, which is
    // attenuation at the moment of creation rather than after it.
    let (_, empty) = sealed_plan(launch.arguments_base, handle, &[]);
    // SAFETY: as above, with an endowment of nothing.
    let (created, child) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            memory,
            empty,
            module.len() as u64,
            u64::from(tos_launch::RIGHT_TERMINATE),
            RUNTIME_GRANT,
            None,
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
    let (no_module, _) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            memory,
            empty,
            absent.len() as u64,
            0,
            RUNTIME_GRANT,
            None,
        )
    };
    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.REFUSED reason=no-such-module status={no_module}"
    ));

    // **The two retired operations, asked for and refused** (ADR-0076 §4,
    // `SYSTEM_ABI_V1` §7). Both funded a process out of the boot's accounting
    // anchor with nobody presenting a `MemoryAuthority`; both now answer
    // `E_NOT_SUPPORTED` forever, and their numbers are never reused. Asked with
    // the same authority that just created a process, so the refusal is about
    // the operation and not about what this process holds.
    // SAFETY: an unassigned-to-this-version operation number; nothing is read.
    let (legacy_create, _) = unsafe { call4(PROCESS_CREATE, handle, module.len() as u64, 0, 0) };
    // SAFETY: as above.
    let (legacy_generation, _) = unsafe {
        call4(
            PROCESS_CREATE_WITH_GENERATION,
            handle,
            module.len() as u64,
            0,
            0,
        )
    };
    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.RETIRED create={legacy_create} with_generation={legacy_generation}"
    ));

    // The module name again: the probe above deliberately left a name the set
    // does not carry where the next call would read one.
    for (offset, byte) in module.iter().enumerate() {
        // SAFETY: as above.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u8>(
                (launch.arguments_base + tos_launch::CREATE_MODULE) as usize,
            )
            .add(offset)
            .write(*byte)
        };
    }

    // **A size no authority could ever serve, told apart from one this
    // authority cannot** (ADR-0076 §7). A caller answered `E_LIMIT` for the
    // first would retry it forever, and one answered `E_BAD_ARGUMENT` for the
    // second would give up on something that will work when memory frees.
    // SAFETY: as above, with a grant far past the accepted `MAX_GRANT`.
    let (impossible, _) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            memory,
            empty,
            module.len() as u64,
            0,
            u64::MAX / 2,
            None,
        )
    };
    // An ordinary arena, funded from an authority that cannot pay for it: a
    // megabyte reserved out of what this process holds. The size is legal and
    // the node is real; what is missing is the memory. **This is also what says
    // the charge goes to the authority that was presented** — the same request
    // through the parent authority succeeded a moment ago.
    // SAFETY: `capability_attenuate_scoped` names an authority this process
    // holds and the bytes to reserve out of it.
    let (reserved, small) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, memory, 1024 * 1024) };
    // SAFETY: as above, funded from the small child.
    let (unaffordable, _) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            small,
            empty,
            module.len() as u64,
            0,
            RUNTIME_GRANT,
            None,
        )
    };
    let _ = reserved;
    // A restart record that is not canonical: absence spelled with a generation
    // left in the field. One byte pattern per meaning, or none.
    // SAFETY: the record is at a fixed offset in this process's own argument
    // region.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<tos_launch::CreateFundedRecord>(
            (launch.arguments_base + tos_launch::CREATE_FUNDED_RECORD) as usize,
        )
        .write(tos_launch::CreateFundedRecord {
            restart_generation: 7,
            flags: 0,
        })
    };
    // SAFETY: as above; the record is deliberately malformed and `create_funded`
    // is bypassed so that it stays that way. `rdx` is the sealed plan and `r10`
    // the module name's length, which is the row's own order.
    let (malformed, _) = unsafe {
        call4(
            PROCESS_CREATE_FUNDED,
            handle,
            memory,
            empty,
            module.len() as u64,
        )
    };
    // Composed so that no single answer produces it: a system that refused all
    // three the same way gives 0, and only the declared trio — a domain
    // refusal, a bound refusal, a domain refusal — gives this.
    let distinguished = u64::from(
        impossible == E_BAD_ARGUMENT && malformed == E_BAD_ARGUMENT && unaffordable != impossible,
    );
    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.FUNDING reserved={reserved} impossible={impossible} \
unaffordable={unaffordable} malformed={malformed} distinguished={distinguished}"
    ));

    // **The rest needs both halves of what a supervisor is.** These probes
    // create children and end them again, so a grant carrying `create` and not
    // `terminate` — which is a real and deliberate endowment this image is also
    // launched under — would leave processes running that nothing could stop.
    // A boot with no memory authority cannot create at all, and says so by not
    // trying.
    #[cfg(feature = "test-funding-lifecycle")]
    if memory != 0 && rights & tos_launch::RIGHT_TERMINATE != 0 {
        funding_lifecycle(launch, report, handle, memory, module);
    }
    #[cfg(not(feature = "test-funding-lifecycle"))]
    let _ = rights;
}

/// What a funded creation does to the authority that paid for it, over the whole
/// life of the child (ADR-0076 §3).
///
/// **A creation is an allocation held by the accounting, not by the capability.**
/// Three claims, and each is a way that could go wrong:
///
///   A. the creator **keeps** its funding authority. A creation places a charge;
///      it does not consume the handle, and it grants the child no name for it —
///      nothing is inherited.
///   B. the charge outlives the capability. A creator that releases its last
///      name for the funding node while a child it paid for is still running has
///      not freed anything; when that child ends, the bytes travel back up the
///      lineage that funded it and the node settles.
///   C. a child *may* be given a name for the same node, by its creator naming
///      that capability in the endowment like any other — two names for one
///      budget (ADR-0076 §2b), never a second reservation.
///
/// The authority is sized to hold exactly one child, so "the charge came back"
/// is observable as "the same request works again" rather than as a number this
/// process would have to be told.
#[cfg(feature = "test-funding-lifecycle")]
fn funding_lifecycle(
    launch: &Launch,
    report: &mut Report,
    handle: u64,
    memory: u64,
    module: &[u8],
) {
    let name_length = module.len() as u64;
    // Room for one child and not two: `RUNTIME_GRANT` plus the rest of a
    // process's footprint, with a little over. Sized so that "the charge came
    // back" is observable as "the same request works again" rather than as a
    // number this process would have to be told.
    const ONE_CHILD: u64 = 60 * 1024 * 1024;
    // SAFETY: `capability_attenuate_scoped` names an authority this process
    // holds and the bytes to reserve out of it.
    let (reserved, funding) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, memory, ONE_CHILD) };

    // C — the child is given a name for the **same** node, because its creator
    // placed that capability in a plan like any other. Two names, one budget;
    // not a second reservation, and not something the child inherited.
    //
    // One plan for all four creations below. Its entry holds a reference of its
    // own on the funding node, which is what lets the creator release its
    // *last* handle later on while the decision goes on naming what it named.
    let (sealed, plan) = sealed_plan(
        launch.arguments_base,
        handle,
        &[(funding, tos_launch::RIGHT_SPEND, b"memory")],
    );
    // SAFETY: both handles are this process's, and the plan entry names the
    // capability just placed in it.
    let (first, first_child) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            funding,
            plan,
            name_length,
            0,
            RUNTIME_GRANT,
            None,
        )
    };

    // A — the creator keeps what it paid with. The capability is untouched by
    // the creation: still resolvable, still spendable.
    // SAFETY: as above.
    let (still_held, alias) = unsafe { call(CAPABILITY_ATTENUATE, funding, u64::from(u32::MAX)) };
    // Released again at once, so that the release below really is the creator's
    // *last* name for the node.
    // SAFETY: as above.
    unsafe { call(CAPABILITY_RELEASE, alias, 0) };
    // And a second child does not fit: the first one's bytes are spent, not
    // merely promised.
    // SAFETY: as above.
    let (second, _) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            funding,
            plan,
            name_length,
            0,
            RUNTIME_GRANT,
            None,
        )
    };
    // The child ends, and the scheduler retires it. Only then are the bytes
    // back — physical first, accounting second.
    // SAFETY: `process_terminate` names the child this process created.
    unsafe { call(PROCESS_TERMINATE, first_child, 0) };
    settle();
    // SAFETY: as above.
    let (again, again_child) = unsafe {
        create_funded(
            launch.arguments_base,
            handle,
            funding,
            plan,
            name_length,
            0,
            RUNTIME_GRANT,
            None,
        )
    };

    // B — the creator lets go of its own handle for the funding node while the
    // child it paid for is still running. Nothing comes back yet: a live
    // child's memory is not free budget, and the accounting node outlives the
    // capability that named it precisely so that the bytes have somewhere to
    // return to.
    // SAFETY: as above.
    let (released, _) = unsafe { call(CAPABILITY_RELEASE, funding, 0) };
    // SAFETY: as above; the handle named a node this process no longer names.
    let (stale, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, funding, 4096) };
    // SAFETY: as above.
    unsafe { call(PROCESS_TERMINATE, again_child, 0) };
    settle();

    // **And the node is still there, because the creator's handle was not the
    // last name.** The plan took a reference of its own when the entry was
    // written (ADR-0077 §3), so a supervisor that released its handle has
    // *handed* the authority to the plan rather than dropped it.
    //
    // Making that observable takes one step, because "did 60 MiB come back?"
    // is not a question a parent with room to spare can answer: it would say
    // yes either way. So the parent is first drained to less than one
    // reservation's worth, in reservations of exactly that size, until it
    // refuses. After that the parent has room for nothing, and the only thing
    // that can make room is the funding node returning — which happens when its
    // **last** name goes, and the plan is holding it.
    let mut drained = 0;
    for _ in 0..8 {
        // SAFETY: `capability_attenuate_scoped` names an authority this process
        // holds and the bytes to reserve out of it.
        let (status, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, memory, ONE_CHILD) };
        if status != OK {
            break;
        }
        drained += 1;
    }
    // Nothing left: the parent cannot reserve while the plan holds the node.
    // SAFETY: as above.
    let (held_by_plan, _) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, memory, ONE_CHILD) };
    // SAFETY: as above; the plan is this process's own, and releasing it is the
    // loss of the last name for the node its one entry describes.
    unsafe { call(CAPABILITY_RELEASE, plan, 0) };

    // And now it is back — past the node no capability and no plan names, up
    // the lineage that funded it, so the parent authority can reserve that
    // amount once more where a moment ago it could not. That is the whole of
    // "process funding is an allocation held by the accounting rather than by
    // the continued existence of a handle", with the plan counted as one of the
    // things that can hold it.
    // SAFETY: as above.
    let (returned, back) = unsafe { call(CAPABILITY_ATTENUATE_SCOPED, memory, ONE_CHILD) };
    // SAFETY: as above; nothing else needs it, and a reservation nobody spends
    // should not outlive the evidence it was made for.
    unsafe { call(CAPABILITY_RELEASE, back, 0) };
    let _ = drained;

    report.line(&alloc::format!(
        "TOS.RUN.PROCESS.LIFECYCLE sealed={sealed} reserved={reserved} first={first} \
still_held={still_held} second={second} again={again} released={released} stale={stale} \
held_by_plan={held_by_plan} returned={returned}"
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
            0,
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
    let (unheld, _) = unsafe { call_transferring(ENDPOINT_SEND, handle, 0, 1, 0) };

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
    let (sent, _) = unsafe { call_transferring(ENDPOINT_SEND, handle, payload.len() as u64, 1, 0) };
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
