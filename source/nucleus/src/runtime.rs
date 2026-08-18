// SPDX-License-Identifier: GPL-3.0-or-later
//! Running the capsule's canonical boot module on the TOS boot path.
//!
//! This is where Stage 2 stops being a set of libraries. The nucleus has
//! already established what the capsule is and what its canonical boot text
//! says; here that text goes through the ordinary reference path — reader,
//! parser, checker, module resolution, lowering, independent verifier, bounded
//! engine — and its result is reported over serial.
//!
//! **Nothing about this path is special because it is boot.** The nucleus calls
//! the same `tos-pipeline` entry a hosted test calls, with the same stages in
//! the same order, and reads the same rendered events. A boot-only interpreter,
//! a hand-built module or a verifier bypass would each make the boot path prove
//! something about itself instead of about TOS.
//!
//! **The nucleus/runtime boundary.** Stage 2 has no address spaces — those
//! arrive with the Stage 3 substrate — so the boundary here is one of
//! *authority*, and it is enforced by what each side can name. The nucleus owns
//! memory discovery: it reads the map, subtracts what is spoken for and hands
//! over one region. The runtime cannot see `BootInfo`, the memory map or the
//! firmware, because `tos-pipeline` and its dependencies do not depend on the
//! boot ABI at all. When Stage 3 brings isolation, the same call becomes a
//! different kind of handoff without the contract changing: a grant and some
//! bytes go in, a structured result comes back.

use alloc::format;
use alloc::string::String;

use tos_boot_protocol::BootInfo;
use tos_frames::Frames;
use tos_pipeline::{execute_set, render, PipelineStage, SetError, SetRequest, Trace, Unit};
use tos_runtime::region::{GrantRefused, Span};
use tos_runtime::GlobalHeap;

use crate::boot_report::ConsoleReporter;
use crate::console::BootConsole;
use crate::stack;

/// The heap of the Stage 2 reference runtime.
///
/// It refuses every allocation until the nucleus adopts a grant, which is the
/// property ADR-0041 asks for: a runtime with no grant has no memory.
#[global_allocator]
pub static HEAP: GlobalHeap = GlobalHeap::new();

/// The entry function the boot module must export.
const BOOT_ENTRY: &str = "main";

fn line(text: &str) {
    tos_serial::puts(text.as_bytes());
    tos_serial::puts(b"\r\n");
}

/// Announces each stage as it is entered, before it runs.
///
/// Before, not after: a stage that never returns is then named by the last
/// event in the log, which is the only way a hang identifies itself from
/// outside — on the serial log and, for the same reason, on the screen.
///
/// One fact, two audiences. The serial event is emitted first and
/// unconditionally, because it is the normative one and must not depend on a
/// framebuffer being present or a renderer doing anything; the console is the
/// best-effort presentation of the same event. Neither consumer knows anything
/// about the pipeline that the pipeline did not tell it.
struct BootTrace<'a, 'fb> {
    console: Option<ConsoleReporter<'a, 'fb>>,
}

impl Trace for BootTrace<'_, '_> {
    fn entering(&mut self, stage: PipelineStage) {
        tos_serial::puts(b"TOS.RUN.STAGE name=");
        tos_serial::puts(stage.symbol().as_bytes());
        tos_serial::puts(b"\r\n");
        if let Some(reporter) = &mut self.console {
            reporter.entering(stage);
        }
    }
}

/// What the nucleus brings to a run.
///
/// The three things the runtime cannot obtain for itself and must not try to:
/// the frames the nucleus owns, the stack it is running on, and which nucleus
/// build this is. Grouped because they always travel together — a run given two
/// of them is a run that discovered the third.
pub struct Machine<'a> {
    pub frames: &'a mut Frames,
    pub stack: Option<Span>,
    pub identity: u64,
}

/// Why the runtime could not be started at all.
///
/// Distinct from a module the runtime refused: this is the implementation
/// failing to obtain what it needs, which is never a statement about the
/// program it was going to run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unstartable {
    NoGrant(GrantRefused),
    HeapRejectedGrant,
    BootPathNotText,
    /// The capsule carries no module at the canonical boot path, or one of its
    /// module paths is not text. Either way there is nothing to run, and it is
    /// a statement about the capsule rather than about a program.
    NoBootModule,
}

/// Runs the capsule's canonical boot module and reports what happened.
///
/// `Ok(Ok(()))` means the module ran to completion. `Ok(Err(stage))` names the
/// stage that ended it — a refusal or a trap — which is what the caller turns
/// into `RESULT_BOOT_MODULE_FAILED`. `Err` is the runtime failing to start at
/// all, which is never a statement about the module. Every case is reported in
/// full over serial first, because a boot that stops here has to say what
/// stopped it.
pub fn execute_boot_text(
    bi: &BootInfo,
    machine: Machine<'_>,
    boot_path: &[u8],
    modules: &[(&[u8], &[u8])],
    source_kind: &[u8],
    mut console: Option<&mut BootConsole<'_>>,
) -> Result<Result<(), &'static str>, Unstartable> {
    let Ok(path) = core::str::from_utf8(boot_path) else {
        return Err(Unstartable::BootPathNotText);
    };
    let path = path.trim_start_matches('/');
    let Machine {
        frames,
        stack: running_on,
        identity,
    } = machine;

    // Taking the region from the pool, adopting it and painting the stack is
    // real work that can fail, so it is announced before it is attempted. A
    // caller that sees `Unstartable` marks this row failed; nothing else opens
    // it.
    if let Some(console) = console.as_deref_mut() {
        console.begin(b"Preparing runtime memory", None);
    }

    // The grant is carved from the frames the nucleus owns (ADR-0050 section
    // 1), not from the largest hole in the map. It is still a V1 grant with V1's
    // property — a runtime with no grant has no memory — and the pool keeps what
    // it did not hand over, which is what a system that will create processes
    // needs and what a single derivation could never leave behind.
    let grant = frames.grant(identity).map_err(Unstartable::NoGrant)?;

    // SAFETY: `grant` names frames the pool admitted from usable map entries
    // after subtracting the nucleus image, the capsule, the handoff record, the
    // converted map, the framebuffer and this stack, and the pool has handed
    // them to no one else. The region outlives the heap: the nucleus halts
    // without releasing it. Adoption happens here, before the first allocation,
    // from the single context that runs the runtime.
    unsafe { HEAP.adopt(&grant) }.map_err(|_| Unstartable::HeapRejectedGrant)?;

    // Painting the unused stack must follow adoption only because nothing
    // before it may allocate; it must precede the run, which is what it
    // measures.
    let painted = running_on.and_then(|region| {
        // SAFETY: `region` is the map entry holding this frame's own stack
        // pointer, so it is the stack being run on, and painting writes only
        // below the current frame.
        unsafe { stack::paint(region) }
    });

    // Every module the capsule carries, not only the canonical boot text: a
    // service is a separate module, and a boot module that imports one has
    // nothing to import unless the set arrives whole. Paths are module-root
    // relative, which is what docs/42 section 1 derives a module name from.
    //
    // Built here and not earlier: this is the first allocation of the run, and
    // ADR-0041's property is that a runtime with no grant has no memory. A
    // vector assembled before adoption is a panic, which is the allocator
    // saying exactly that.
    let mut units = alloc::vec::Vec::with_capacity(modules.len());
    for (name, content) in modules {
        let Ok(name) = core::str::from_utf8(name) else {
            return Err(Unstartable::BootPathNotText);
        };
        units.push(Unit {
            path: name.trim_start_matches('/'),
            bytes: content,
        });
    }

    let boot_bytes = units
        .iter()
        .find(|unit| unit.path == path)
        .map(|unit| unit.bytes.len())
        .unwrap_or(0);
    // `modules` is appended after the fields the accepted contract requires,
    // under its own extension rule: the set is now part of what a run is, and a
    // log that showed only the entry would understate what was executed.
    line(&format!(
        "TOS.RUN.BEGIN path={path} bytes={boot_bytes} entry={BOOT_ENTRY} \
         nucleus=0x{identity:016x} grant_base=0x{:x} grant_length={} grant_version={} \
         modules={}",
        grant.base,
        grant.length,
        grant.version,
        units.len(),
    ));
    if let Some(console) = console.as_deref_mut() {
        console.succeed();
    }

    let source_set = source_set_identity(source_kind, &bi.capsule_source_identity);
    let request = SetRequest {
        source_set: &source_set,
        units: &units,
        entry_path: path,
        entry: BOOT_ENTRY,
    };
    let mut trace = BootTrace {
        console: console.map(|console| ConsoleReporter::new(console, path.as_bytes())),
    };
    let run = match execute_set(&request, alloc::vec::Vec::new(), &mut trace) {
        Ok(run) => run,
        // The request does not describe something runnable. No stage ran, so
        // this is not a refusal of anyone's source.
        Err(SetError::EntryModuleAbsent { .. } | SetError::NoUnits) => {
            return Err(Unstartable::NoBootModule)
        }
    };
    // Serial first, and in full: the machine-readable log is the record of what
    // happened, and the screen is a reading of it.
    for event in render::events(&run) {
        line(&event);
    }
    if let Some(reporter) = &mut trace.console {
        reporter.finished(&run);
    }

    let (committed, peak) = HEAP.usage();
    let (blocks, free) = HEAP.block_census();
    line(&format!(
        "TOS.RUN.MEMORY granted={} peak={peak} committed={committed} blocks={blocks} free={free}",
        grant.length
    ));
    if let (Some(region), Some(floor)) = (running_on, painted) {
        // SAFETY: `region` and `floor` came from the matching `paint` above, on
        // the stack this frame is still running on.
        let used = unsafe { stack::peak(region, floor) };
        line(&format!(
            "TOS.RUN.STACK used={used} capacity={}",
            region.length()
        ));
    }
    Ok(match run.failed_at() {
        None => Ok(()),
        Some(stage) => Err(stage.symbol()),
    })
}

/// The declared source-set identity of the capsule's source tree.
fn source_set_identity(kind: &[u8], value: &[u8; 32]) -> String {
    let mut hex = [0u8; 64];
    tos_hash::hex(value, &mut hex);
    let kind = core::str::from_utf8(kind).unwrap_or("unknown");
    // A detached capsule's identity is a whole-tree digest; a git one is an
    // object id. Both are named by their kind so neither is read as the other.
    let digest = core::str::from_utf8(&hex).unwrap_or("");
    if kind == "git" {
        format!("git:{digest}")
    } else {
        format!("{kind}:{digest}")
    }
}
