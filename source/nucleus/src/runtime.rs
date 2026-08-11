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

use tos_boot_protocol::{BootInfo, MemoryRange, MEM_USABLE};
use tos_pipeline::{execute, render, PipelineStage, Request, Trace};
use tos_runtime::region::{self, Span};
use tos_runtime::GlobalHeap;

use crate::stack;

/// The heap of the Stage 2 reference runtime.
///
/// It refuses every allocation until the nucleus adopts a grant, which is the
/// property ADR-0041 asks for: a runtime with no grant has no memory.
#[global_allocator]
pub static HEAP: GlobalHeap = GlobalHeap::new();

extern "C" {
    static __tos_image_start: u8;
    static __tos_image_load_end: u8;
    static __tos_image_end: u8;
}

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
/// outside.
struct SerialTrace;

impl Trace for SerialTrace {
    fn entering(&mut self, stage: PipelineStage) {
        tos_serial::puts(b"TOS.RUN.STAGE name=");
        tos_serial::puts(stage.symbol().as_bytes());
        tos_serial::puts(b"\r\n");
    }
}

/// The span the nucleus occupies in memory, `.bss` included.
fn image() -> Span {
    // Only the addresses of these symbols are taken; no byte of the image is
    // read through them, which is why no unsafe block is needed here.
    Span::new(
        core::ptr::addr_of!(__tos_image_start) as u64,
        core::ptr::addr_of!(__tos_image_end) as u64,
    )
}

/// A digest of the loaded nucleus image, truncated to name this build.
///
/// `--oformat=binary` makes `[__tos_image_start, __tos_image_load_end)` exactly
/// the bytes of `nucleus.bin`, so this identity can be recomputed from the
/// artifact rather than taken on the running image's word.
fn build_identity() -> u64 {
    // SAFETY: the linker script places both symbols in the loaded image, in
    // this order, and the range between them is mapped and readable.
    let bytes = unsafe {
        let start = core::ptr::addr_of!(__tos_image_start);
        let end = core::ptr::addr_of!(__tos_image_load_end);
        core::slice::from_raw_parts(start, end as usize - start as usize)
    };
    let digest = tos_hash::sha256(bytes);
    u64::from_le_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

/// Every span the runtime must not be granted.
///
/// The loader reserves what it handed over, but it allocates the nucleus image
/// by file length while `.bss` is `NOLOAD` — so the memory the nucleus's own
/// statics occupy is still reported usable. The rest are subtracted although
/// the map already marks them reserved: memory the runtime could write over is
/// not a risk worth resting on one component's bookkeeping.
fn occupied(bi: &BootInfo, bi_address: u64, stack: Option<Span>) -> ([Span; 6], usize) {
    let mut spans = [Span::new(0, 0); 6];
    let mut count = 0;
    let mut push = |span: Option<Span>| {
        if let Some(span) = span {
            if span.length() > 0 && count < spans.len() {
                spans[count] = span;
                count += 1;
            }
        }
    };
    push(Some(image()));
    push(Span::sized(bi.capsule_phys, bi.capsule_length));
    push(Span::sized(
        bi_address,
        tos_boot_protocol::STRUCT_SIZE as u64,
    ));
    push(Span::sized(bi.memory_map_phys, bi.memory_map_length));
    push(Span::sized(
        bi.framebuffer_phys,
        u64::from(bi.framebuffer_pitch) * u64::from(bi.framebuffer_height),
    ));
    push(stack);
    (spans, count)
}

/// Free spans, as the region chooser wants them.
fn free_spans(descs: &[MemoryRange]) -> impl Iterator<Item = Span> + '_ {
    descs
        .iter()
        .filter(|descriptor| descriptor.ty == MEM_USABLE)
        .filter_map(|descriptor| Span::sized(descriptor.phys_start, descriptor.phys_length))
}

/// Every span in the map, for locating the stack the nucleus is running on.
fn all_spans(descs: &[MemoryRange]) -> impl Iterator<Item = Span> + '_ {
    descs
        .iter()
        .filter_map(|descriptor| Span::sized(descriptor.phys_start, descriptor.phys_length))
}

/// Why the runtime could not be started at all.
///
/// Distinct from a module the runtime refused: this is the implementation
/// failing to obtain what it needs, which is never a statement about the
/// program it was going to run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unstartable {
    NoGrant(region::GrantRefused),
    HeapRejectedGrant,
    BootPathNotText,
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
    bi_address: u64,
    descs: &[MemoryRange],
    boot_path: &[u8],
    boot_content: &[u8],
    source_kind: &[u8],
) -> Result<Result<(), &'static str>, Unstartable> {
    let Ok(path) = core::str::from_utf8(boot_path) else {
        return Err(Unstartable::BootPathNotText);
    };
    let path = path.trim_start_matches('/');

    let identity = build_identity();
    let running_on = stack::containing(all_spans(descs), stack::pointer());
    let (spans, count) = occupied(bi, bi_address, running_on);
    let grant = region::derive(free_spans(descs), &spans[..count], identity)
        .map_err(Unstartable::NoGrant)?;

    // SAFETY: `grant` names a region the memory map reports usable and that the
    // spans above proved disjoint from the nucleus image, the capsule, the
    // handoff record, the converted map, the framebuffer and this stack. Stage
    // 1 has no other memory consumer, and the region outlives the heap: the
    // nucleus halts without releasing it. Adoption happens here, before the
    // first allocation, from the single context that runs the runtime.
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

    line(&format!(
        "TOS.RUN.BEGIN path={path} bytes={} entry={BOOT_ENTRY} nucleus=0x{identity:016x} \
         grant_base=0x{:x} grant_length={} grant_version={}",
        boot_content.len(),
        grant.base,
        grant.length,
        grant.version,
    ));

    let source_set = source_set_identity(source_kind, &bi.capsule_source_identity);
    let request = Request {
        source_set: &source_set,
        path,
        bytes: boot_content,
        entry: BOOT_ENTRY,
    };
    let run = execute(&request, alloc::vec::Vec::new(), &mut SerialTrace);
    for event in render::events(&run) {
        line(&event);
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
