// SPDX-License-Identifier: GPL-3.0-or-later
//! Measures the implementation arena the Stage 2 pipeline actually needs.
//!
//! ADR-0041 accepts two disciplines for allocation failure, and the one this
//! implementation relies on is "a proved upper memory bound and an arena at
//! least that large". A bound has to be measured to be proved, so this runs the
//! whole production path — source reader, parser, checker, lowerer, verifier,
//! engine — with `tos_runtime`'s bounded heap installed as the global
//! allocator, and reports `peak_extent`: the arena size that run needed.
//!
//! Running the pipeline *through* the heap is also the strongest test the heap
//! has. A workload that allocates and frees hundreds of thousands of times in
//! irregular sizes exercises splitting, coalescing and reuse in ways a unit
//! test does not, and any corruption shows up as a wrong answer rather than as
//! a passing assertion.
//!
//! The arena is a static region, which is what a nucleus grant will be: a base
//! and a length that the runtime is given rather than finds.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::{run, Value};
use tos_runtime::{GlobalHeap, RuntimeMemoryGrant, GRANT_VERSION};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// The region the measurement runs in. A nucleus grant is the same shape.
const ARENA_BYTES: usize = 512 * 1024 * 1024;
static mut ARENA: [u8; ARENA_BYTES] = [0; ARENA_BYTES];

static ADOPTED: AtomicBool = AtomicBool::new(false);

/// The bounded heap, adopting its region on first use.
///
/// Adoption cannot wait for `main`: the Rust runtime allocates before it. So
/// the first allocation adopts, which is the same thing a freestanding runtime
/// does when the nucleus hands it a grant before it starts work.
struct MeasuredHeap {
    heap: GlobalHeap,
}

impl MeasuredHeap {
    fn ensure_adopted(&self) {
        if ADOPTED.swap(true, Ordering::SeqCst) {
            return;
        }
        // The static is untouched before this point — `ADOPTED` makes adoption
        // happen exactly once — and it lives for the whole program, which is
        // the promise a grant makes.
        let base = core::ptr::addr_of_mut!(ARENA) as usize;
        let aligned = base.div_ceil(64) * 64;
        let grant = RuntimeMemoryGrant {
            version: GRANT_VERSION,
            base: aligned,
            length: ARENA_BYTES - (aligned - base),
            alignment: 64,
            identity: 0,
        };
        // SAFETY: the region is owned by this program alone for its lifetime.
        unsafe { self.heap.adopt(&grant) }.expect("the static region is a well-formed grant");
    }
}

// SAFETY: the heap upholds the `GlobalAlloc` contract; this only adds a
// one-time adoption in front of it.
unsafe impl GlobalAlloc for MeasuredHeap {
    // SAFETY: the `GlobalAlloc` contract; this only puts a one-time adoption in front of the heap, which upholds it.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.ensure_adopted();
        // SAFETY: the heap has adopted its region.
        unsafe { self.heap.alloc(layout) }
    }

    // SAFETY: the `GlobalAlloc` contract; `pointer` was returned by `alloc` on this allocator.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: `pointer` came from `alloc` on this allocator.
        unsafe { self.heap.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static HEAP: MeasuredHeap = MeasuredHeap {
    heap: GlobalHeap::new(),
};

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

fn main() {
    println!("TOS Stage 2 implementation-arena bound");
    println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
    println!();

    // The largest input the published limits admit: docs/44 caps a normalized
    // source unit at 256 KiB, so this is the worst case for one module.
    let module_text = canonical_module(256 * 1024);
    println!("fixture: {} bytes of canonical source", module_text.len());

    let before = HEAP.heap.usage();
    let answer = whole_pipeline(&module_text);
    let (committed, extent) = HEAP.heap.usage();

    println!("pipeline result: {answer:?}");
    println!("committed after the run: {committed} bytes");
    println!("peak extent (the arena this run needed): {extent} bytes");
    println!(
        "  = {:.2} MiB, against a {} MiB region",
        extent as f64 / (1024.0 * 1024.0),
        ARENA_BYTES / (1024 * 1024)
    );
    let (blocks, free) = HEAP.heap.block_census();
    println!("blocks after the run: {blocks} total, {free} free");
    println!("committed before the run: {} bytes", before.0);

    // The measurement is only worth anything if the pipeline actually ran, so
    // the answer is checked rather than discarded.
    assert_eq!(answer, Value::Int(tos_ir::IntKind::I32, 3));
    println!();
    println!("The whole production path ran on this heap and produced the right");
    println!("answer, so the bound above is a bound on a run that really happened.");
}

/// Source, checked, lowered, verified, executed — no stage skipped.
fn whole_pipeline(text: &str) -> Value {
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        diagnostics.is_empty(),
        "the fixture must be checked source: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let context = ModuleContext {
        source_set: String::from("tos-arena-bound"),
        path: String::from("app/bound.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("the fixture lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("lowered IR verifies");
    run(
        &module,
        &receipt,
        "total0",
        vec![Value::Aggregate(vec![
            Value::Int(tos_ir::IntKind::I32, 1),
            Value::Int(tos_ir::IntKind::I32, 2),
        ])],
    )
    .expect("the entry exists")
    .expect("the program does not trap")
    .value
}

/// A canonical module filling the published source-unit ceiling.
fn canonical_module(bytes: usize) -> String {
    let mut text = String::from(
        "module app.bound version 1.0 profile bootstrap; \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] ",
    );
    let mut index = 0usize;
    loop {
        let chunk = format!(
            "pub record Point{index} [x: i32, y: i32] \
             pub fn total{index}(point: Point{index}) -> i32 {{ return point.x + point.y; }} "
        );
        if text.len() + chunk.len() > bytes {
            break;
        }
        text.push_str(&chunk);
        index += 1;
    }
    text
}
