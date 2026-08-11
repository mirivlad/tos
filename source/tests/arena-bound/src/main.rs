// SPDX-License-Identifier: GPL-3.0-or-later
//! What arena the Stage 2 reference path actually needs, measured.
//!
//! ADR-0041 accepts two disciplines for allocation failure, and the one this
//! implementation relies on is "a proved upper memory bound and an arena at
//! least that large". A bound has to be measured to be proved, so everything
//! here runs the whole production path — source reader, parser, checker, module
//! resolution, lowerer, verifier, engine — with `tos_runtime`'s bounded heap
//! installed as the global allocator.
//!
//! Running the pipeline *through* the heap is also the strongest test the heap
//! has. A workload that allocates and frees hundreds of thousands of times in
//! irregular sizes exercises splitting, coalescing and reuse in ways a unit
//! test does not, and any corruption shows up as a wrong answer rather than as
//! a passing assertion.
//!
//! Four questions are answered, and they are different questions:
//!
//! 1. **One module at the ceiling.** The worst case docs/44 admits for a single
//!    source unit.
//! 2. **Repeated execution.** Whether running the same thing again needs more
//!    than running it once. Equal live bytes do not answer this: an arena can
//!    hold the same total in twice as many pieces, and a reference runtime that
//!    degrades over repetitions is not a recovery oracle.
//! 3. **A source set, one module at a time.** Whether the executable path
//!    accumulates across modules or reuses what the previous module returned.
//! 4. **Set-wide resolution.** What it costs to have every module of a closure
//!    resolvable at once, which is the one part that cannot be phased away
//!    while resolution reads parse trees.
//!
//! The arena is a static region, which is what a nucleus grant is: a base and a
//! length the runtime is given rather than finds. It is far larger than any
//! production grant on purpose — a measurement that ran out of room would report
//! the rig's limit instead of the workload's need.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};

use tos_core::{ModuleEntry, ModuleSummary, Parser, Schema, SourceReader, SourceUnit};
use tos_pipeline::{execute, Request, Run, Silent};
use tos_runtime::{GlobalHeap, RuntimeMemoryGrant, GRANT_VERSION};

/// The region the measurement runs in. A nucleus grant is the same shape.
///
/// It is taken from the host allocator rather than declared as a static array:
/// a static this large puts other statics further than 2 GiB from the code that
/// references them, which the small code model cannot address. Where the region
/// comes from is not part of what is being measured — a base and a length
/// arrive, which is exactly the shape of a grant.
const ARENA_BYTES: usize = 3072 * 1024 * 1024;

static ADOPTED: AtomicBool = AtomicBool::new(false);

/// The published ceiling for one normalized source unit (docs/44 section 2).
const SOURCE_CEILING: usize = 256 * 1024;
/// The published ceiling for a module dependency closure (docs/44 section 2).
const CLOSURE_CEILING: usize = 256;

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
        // `ADOPTED` makes this happen exactly once, before the heap serves
        // anything. The region is never released, which is the promise a grant
        // makes: it outlives the runtime that was given it.
        let layout = Layout::from_size_align(ARENA_BYTES, 4096).expect("a valid region layout");
        // SAFETY: `System` is the host allocator and is unaffected by the
        // global allocator installed below it; the layout is non-zero-sized.
        let base = unsafe { std::alloc::System.alloc(layout) };
        assert!(
            !base.is_null(),
            "the measurement needs a {ARENA_BYTES}-byte region"
        );
        let grant = RuntimeMemoryGrant {
            version: GRANT_VERSION,
            base: base as usize,
            length: ARENA_BYTES,
            alignment: 4096,
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

/// Everything observable about the arena at one instant.
///
/// `frontier` is the bound; the rest is the layout. Two instants with equal
/// `committed` and different `blocks` are not the same arena, which is exactly
/// the difference accumulating fragmentation would show up as.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Arena {
    committed: usize,
    frontier: usize,
    blocks: usize,
    free: usize,
}

fn arena() -> Arena {
    let (committed, frontier) = HEAP.heap.usage();
    let (blocks, free) = HEAP.heap.block_census();
    Arena {
        committed,
        frontier,
        blocks,
        free,
    }
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn main() {
    let full = std::env::args().any(|argument| argument == "--full");
    println!("TOS Stage 2 implementation-arena bound");
    println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
    println!("mode: {}", if full { "full" } else { "fast" });
    println!();

    let ceiling = one_module_at_the_ceiling();
    repeated_execution();
    let phased = a_source_set_one_module_at_a_time(if full { CLOSURE_CEILING } else { 16 });
    let resolution = set_wide_resolution(full);
    let at_ceiling = resolution_at_the_source_ceiling();
    let summarized = resolution_over_summaries(full);

    println!();
    println!("== the bound ==");
    println!(
        "one module at the {SOURCE_CEILING}-byte ceiling      {:>12} bytes  ({:.2} MiB)",
        ceiling,
        mib(ceiling)
    );
    println!(
        "after a source set processed module by module  {:>12} bytes  ({:.2} MiB)",
        phased,
        mib(phased)
    );
    println!(
        "set-wide resolution, {CLOSURE_CEILING} modules alive        {:>12} bytes  ({:.2} MiB){}",
        resolution.0,
        mib(resolution.0),
        if resolution.1 { "" } else { "  [fitted]" }
    );
    println!(
        "the same, every module at the source ceiling  {:>12} bytes  ({:.2} MiB)  [fitted]",
        at_ceiling,
        mib(at_ceiling)
    );
    println!(
        "resolution over summaries, {CLOSURE_CEILING} ceiling modules {:>12} bytes  ({:.2} MiB)",
        summarized,
        mib(summarized)
    );
}

/// What set-wide resolution costs when it reads summaries instead of trees.
///
/// The loader shape this measures is the one a bounded implementation uses:
/// parse one module, summarize it, drop the tree, keep the summary. Only one
/// parse tree is ever live, so the closure's cost is its *interfaces* rather
/// than its bodies — and the modules here are at the published source ceiling,
/// which is where the tree-based architecture needed gigabytes.
fn resolution_over_summaries(full: bool) -> usize {
    println!();
    println!("== set-wide resolution over derived summaries ==");
    let counts: &[usize] = if full {
        &[1, 8, 32, CLOSURE_CEILING]
    } else {
        &[1, 8, 32]
    };
    let mut last = 0usize;
    for &count in counts {
        let before = arena().committed;
        let mut summaries: Vec<ModuleSummary> = Vec::with_capacity(count);
        for index in 0..count {
            // One tree at a time. Everything but the summary is dropped before
            // the next module is read.
            let text = canonical_module(index, SOURCE_CEILING);
            let source = SourceReader::read(text.as_bytes()).expect("transport-valid");
            let schema = Parser::parse_schema(&source)
                .into_accepted()
                .expect("the fixture parses");
            let path = module_path(index);
            summaries.push(ModuleEntry::new(&path, &source, &schema).summarize());
        }
        let held = arena().committed - before;
        let diagnostics = tos_core::check_module_summaries(&summaries);
        assert!(
            diagnostics.is_empty(),
            "the generated set must resolve: {:?}",
            diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
        );
        let peak = arena().committed - before;
        println!(
            "  {count:>4} modules of {SOURCE_CEILING} bytes: {held:>10} bytes of summaries, \
             {peak} bytes live while resolving ({:.2} MiB)",
            mib(peak)
        );
        last = peak;
        drop(summaries);
    }
    last
}

/// The marginal resolution cost when every module is at the source ceiling.
///
/// The worst case the two published ceilings admit together is 256 modules of
/// 256 KiB each. Measuring it outright needs more memory than the machine this
/// runs on has, which is itself the finding: the slope is measured over the
/// counts that do fit, and the ceiling figure is stated as fitted from it
/// rather than presented as a measurement that was never taken.
fn resolution_at_the_source_ceiling() -> usize {
    println!();
    println!("== set-wide resolution with every module at the source ceiling ==");
    let mut points: Vec<(usize, usize)> = Vec::new();
    for &count in &[1usize, 2, 4] {
        let cost = resolution_cost(count, SOURCE_CEILING);
        points.push((count, cost));
        println!(
            "  {count:>4} modules of {SOURCE_CEILING} bytes: {cost:>12} bytes live ({:.2} MiB)",
            mib(cost)
        );
    }
    let marginal = (points[2].1 - points[1].1) as f64 / (points[2].0 - points[1].0) as f64;
    let projected = points[2].1 + (marginal * (CLOSURE_CEILING - points[2].0) as f64) as usize;
    println!("  marginal cost per ceiling-sized module (measured slope): {marginal:.0} bytes");
    println!(
        "  at {CLOSURE_CEILING} modules: {projected} bytes ({:.2} MiB), fitted from the measured slope",
        mib(projected)
    );
    projected
}

/// The worst case docs/44 admits for a single source unit.
fn one_module_at_the_ceiling() -> usize {
    println!("== one module at the published source ceiling ==");
    let text = canonical_module(0, SOURCE_CEILING);
    println!("fixture: {} bytes of canonical source", text.len());
    let before = arena();
    let value = whole_pipeline(&text, 0);
    let after = arena();
    // The measurement is only worth anything if the pipeline actually ran.
    assert_eq!(value, 3, "the fixture must produce its answer");
    println!(
        "peak extent {} bytes ({:.2} MiB); committed {} -> {}; blocks {} ({} free)",
        after.frontier,
        mib(after.frontier),
        before.committed,
        after.committed,
        after.blocks,
        after.free
    );
    after.frontier
}

/// Whether running the same thing again needs more than running it once.
///
/// The invariant is layout, not volume: from the second round on, the arena
/// must return to the *same* committed bytes **and** the same block census,
/// and the frontier must stop moving. Accumulating fragmentation breaks the
/// census long before it breaks the total, which is why the total alone is not
/// the test.
fn repeated_execution() {
    println!();
    println!("== repeated whole-pipeline execution ==");
    let text = canonical_module(0, 16 * 1024);
    let rounds = 64;
    let mut settled: Option<Arena> = None;
    for round in 0..rounds {
        assert_eq!(whole_pipeline(&text, 0), 3);
        let now = arena();
        match settled {
            None => {
                if round >= 1 {
                    settled = Some(now);
                }
            }
            Some(first) => assert_eq!(
                now, first,
                "round {round} left the arena in a different state than round 1"
            ),
        }
    }
    let state = settled.expect("at least two rounds");
    println!(
        "{rounds} rounds; from round 1 on: committed {} bytes, {} blocks ({} free), \
         frontier {} bytes ({:.2} MiB)",
        state.committed,
        state.blocks,
        state.free,
        state.frontier,
        mib(state.frontier)
    );
    println!(
        "the layout after every later round is identical to the first, not merely equal in total"
    );
}

/// A source set processed the way a loader processes one: module by module,
/// releasing each module's state before the next begins.
///
/// Returns the frontier after the whole set. The claim being tested is that it
/// does not grow with the number of modules — first-fit hands the next module
/// the memory the previous one returned, and the frontier is a high-water mark
/// that only a *deeper* run can move.
fn a_source_set_one_module_at_a_time(count: usize) -> usize {
    println!();
    println!("== a source set, one module at a time ({count} modules) ==");
    let bytes = 32 * 1024;
    // What survives each module: enough to name it and its verified identity.
    let mut retained: Vec<(String, String)> = Vec::new();
    let mut after_first = 0usize;
    for index in 0..count {
        let text = canonical_module(index, bytes);
        let request = Request {
            source_set: "tos-arena-bound",
            path: &module_path(index),
            bytes: text.as_bytes(),
            entry: &format!("total{index}"),
        };
        let run = execute(&request, arguments(), &mut Silent);
        let Run::Completed(completion) = run else {
            panic!("module {index} must complete");
        };
        retained.push((
            completion.receipt.module_name.clone(),
            completion.receipt.module_digest.clone(),
        ));
        if index == 0 {
            after_first = arena().frontier;
        }
    }
    let after = arena();
    assert_eq!(retained.len(), count);
    println!(
        "each module {bytes} bytes; frontier after module 1: {} bytes; after module {count}: {} bytes",
        after_first, after.frontier
    );
    println!(
        "growth across {} further modules: {} bytes ({:.1} bytes per module)",
        count - 1,
        after.frontier - after_first,
        (after.frontier - after_first) as f64 / (count - 1) as f64
    );
    after.frontier
}

/// What it costs to have a whole closure resolvable at once.
///
/// docs/42 resolution is the one part of the path that cannot be phased away
/// module by module: `check_module_set` compares every module's declared name,
/// imports and type table against every other's, and it reads them from parse
/// trees. So every module of the closure is live at the same time, and this
/// term *is* linear in the closure size.
///
/// Returns the figure for the published closure ceiling and whether it was
/// measured there or fitted from measured points.
fn set_wide_resolution(full: bool) -> (usize, bool) {
    println!();
    println!("== set-wide resolution ==");
    let bytes = 8 * 1024;
    let counts: &[usize] = if full {
        &[1, 8, 32, 128, CLOSURE_CEILING]
    } else {
        &[1, 8, 32]
    };
    let mut points: Vec<(usize, usize)> = Vec::new();
    for &count in counts {
        let cost = resolution_cost(count, bytes);
        points.push((count, cost));
        println!(
            "  {count:>4} modules of {bytes} bytes: {cost:>12} bytes live ({:.2} MiB)",
            mib(cost)
        );
    }
    // A marginal cost per module, measured between the two largest points
    // rather than assumed. It is the slope of a line through real data, not a
    // single measurement multiplied by a count.
    let (small, small_cost) = points[points.len() - 2];
    let (large, large_cost) = points[points.len() - 1];
    let marginal = (large_cost - small_cost) as f64 / (large - small) as f64;
    println!("  marginal cost per module (measured slope): {marginal:.0} bytes");
    if large == CLOSURE_CEILING {
        println!("  at the published {CLOSURE_CEILING}-module ceiling: measured");
        (large_cost, true)
    } else {
        let projected = large_cost + (marginal * (CLOSURE_CEILING - large) as f64) as usize;
        println!(
            "  at the published {CLOSURE_CEILING}-module ceiling: {projected} bytes, fitted from \
             the measured slope (run with --full to measure it)"
        );
        (projected, false)
    }
}

/// Live bytes with `count` modules parsed and resolvable at the same time.
fn resolution_cost(count: usize, bytes: usize) -> usize {
    let before = arena().committed;
    let texts: Vec<String> = (0..count)
        .map(|index| canonical_module(index, bytes))
        .collect();
    let sources: Vec<SourceUnit> = texts
        .iter()
        .map(|text| SourceReader::read(text.as_bytes()).expect("transport-valid"))
        .collect();
    let schemas: Vec<Schema> = sources
        .iter()
        .map(|source| {
            Parser::parse_schema(source)
                .into_accepted()
                .expect("the fixture parses")
        })
        .collect();
    let paths: Vec<String> = (0..count).map(module_path).collect();
    let entries: Vec<ModuleEntry> = (0..count)
        .map(|index| ModuleEntry::new(&paths[index], &sources[index], &schemas[index]))
        .collect();
    let diagnostics = tos_core::check_module_set(&entries);
    assert!(
        diagnostics.is_empty(),
        "the generated set must resolve: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let peak = arena().committed;
    drop(entries);
    drop(schemas);
    drop(sources);
    drop(texts);
    peak - before
}

/// Source, checked, resolved, lowered, verified, executed — no stage skipped.
fn whole_pipeline(text: &str, index: usize) -> i128 {
    let request = Request {
        source_set: "tos-arena-bound",
        path: &module_path(index),
        bytes: text.as_bytes(),
        entry: &format!("total{index}"),
    };
    let run = execute(&request, arguments(), &mut Silent);
    let Run::Completed(completion) = run else {
        panic!("the fixture must complete: {:?}", run.failed_at());
    };
    let tos_engine::Value::Int(_, number) = completion.value else {
        panic!("the fixture returns an integer");
    };
    number
}

fn arguments() -> Vec<tos_engine::Value> {
    vec![tos_engine::Value::Aggregate(vec![
        tos_engine::Value::Int(tos_ir::IntKind::I32, 1),
        tos_engine::Value::Int(tos_ir::IntKind::I32, 2),
    ])]
}

fn module_path(index: usize) -> String {
    format!("set/m{index}.tos")
}

/// A canonical module of about `bytes` bytes, named for its position in a set.
///
/// Every module exports `total<index>` taking a two-field record, so a driver
/// can run any of them without knowing which one it has.
fn canonical_module(index: usize, bytes: usize) -> String {
    let mut text = format!(
        "module set.m{index} version 1.0 profile bootstrap; \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub record Point{index} [x: i32, y: i32] \
         pub fn total{index}(point: Point{index}) -> i32 {{ return point.x + point.y; }} "
    );
    let mut filler = 0usize;
    loop {
        let chunk = format!(
            "pub record Filler{index}_{filler} [x: i32, y: i32] \
             pub fn fill{index}_{filler}(point: Filler{index}_{filler}) -> i32 \
             {{ return point.x + point.y; }} "
        );
        if text.len() + chunk.len() > bytes {
            break;
        }
        text.push_str(&chunk);
        filler += 1;
    }
    text
}
