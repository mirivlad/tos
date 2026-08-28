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
//! 5. **An executed closure.** What one run needs when the whole closure is
//!    read, checked, resolved, lowered, verified and executed together — with
//!    the entry calling across the boundary, so the dependencies are not merely
//!    present but reached. This is the number a launcher sizes a grant from,
//!    and it is not the single-module bound: nothing about one module measured
//!    alone says what several cost at once.
//!
//! The arena is a static region, which is what a nucleus grant is: a base and a
//! length the runtime is given rather than finds. It is far larger than any
//! production grant on purpose — a measurement that ran out of room would report
//! the rig's limit instead of the workload's need.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};

use tos_core::{
    lower_module_in_set, LoweringInterface, ModuleContext, ModuleEntry, ModuleSummary, Parser,
    ResolvedImport, Schema, SourceReader, SourceUnit, VerificationSurface,
};
use tos_pipeline::{
    execute, execute_set, PipelineStage, Request, Run, SetRequest, Silent, SourceProvider, Unit,
    Unreachable,
};
use tos_runtime::{GlobalHeap, RuntimeMemoryGrant, GRANT_VERSION};

/// The region the measurement runs in. A nucleus grant is the same shape.
///
/// It is taken from the host allocator rather than declared as a static array:
/// a static this large puts other statics further than 2 GiB from the code that
/// references them, which the small code model cannot address. Where the region
/// comes from is not part of what is being measured — a base and a length
/// arrive, which is exactly the shape of a grant.
#[cfg(not(feature = "grant"))]
const ARENA_BYTES: usize = 3072 * 1024 * 1024;

/// `RuntimeMemoryGrantV1` exactly (ADR-0069 §3), so a run that does not fit
/// fails to allocate rather than reporting a number.
#[cfg(feature = "grant")]
const ARENA_BYTES: usize = 54 * 1024 * 1024;

/// The capsule's own bytes, outside the measured arena.
///
/// In the freestanding reference path a source unit is a window into the
/// capsule: the loader places the capsule in physical RAM and reserves its
/// range, and `nucleus/src/process.rs` maps those same frames into the process
/// at `SOURCE` read-only, computing each unit's address as its offset from the
/// capsule base. Nothing is copied and nothing comes out of the grant. So the
/// fixture's source text is allocated straight from the system allocator,
/// bypassing the measured arena, because that is where it physically is.
///
/// It is **not free**: it is physical memory, and the whole-machine ledger
/// carries it as a platform line.
fn capsule_bytes(text: &str) -> &'static [u8] {
    outside_the_arena(text.as_bytes())
}

/// The same, for bytes that are not text: a whole capsule, for instance.
fn outside_the_arena(text: &[u8]) -> &'static [u8] {
    let layout = Layout::from_size_align(text.len().max(1), 1).expect("a valid layout");
    // SAFETY: a non-zero-sized layout, allocated from the system allocator and
    // never freed — the fixture's capsule lives for the whole measurement, as a
    // real capsule lives for the whole boot.
    let pointer = unsafe { std::alloc::System.alloc(layout) };
    assert!(!pointer.is_null(), "the capsule fixture did not allocate");
    // SAFETY: `pointer` addresses `text.len()` writable bytes just obtained.
    unsafe {
        core::ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
        core::slice::from_raw_parts(pointer, text.len())
    }
}

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
    // The executed-closure bound runs on its own, in its own process. The
    // arena's frontier never falls, so a measurement that followed the
    // 256 KiB-module one would report that measurement's high-water mark and
    // call it the closure's — and a measurement that preceded it would leave
    // its freed blocks under the published single-module bound. Separate
    // processes are the only way each number is its own.
    // The published ceiling, in its own process for the same reason every other
    // bound gets one: a frontier that never falls would otherwise carry another
    // measurement's high-water mark into this one.
    if std::env::args().any(|argument| argument == "--ir") {
        println!("TOS implementation-arena bound: lowered IR breakdown");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);
        ir_breakdown(modules, SOURCE_CEILING);
        return;
    }
    if std::env::args().any(|argument| argument == "--production") {
        println!("TOS production path, measured by phase");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);
        let shape = std::env::args()
            .skip_while(|argument| argument != "--shape")
            .nth(1)
            .unwrap_or_else(|| String::from("chain"));
        let shape = match shape.as_str() {
            "wide" => Shape::WideFanIn,
            "balanced" => Shape::Balanced,
            _ => Shape::Chain,
        };
        production_path(shape, modules, SOURCE_CEILING);
        return;
    }
    if std::env::args().any(|argument| argument == "--build") {
        println!("TOS build workspace, measured apart from what it hands over");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);
        let unit_bytes = std::env::args()
            .skip_while(|argument| argument != "--unit-bytes")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(SOURCE_CEILING);
        let shape = std::env::args()
            .skip_while(|argument| argument != "--shape")
            .nth(1)
            .unwrap_or_else(|| String::from("chain"));
        let shape = match shape.as_str() {
            "wide" => Shape::WideFanIn,
            "balanced" => Shape::Balanced,
            _ => Shape::Chain,
        };
        build_workspace(shape, modules, unit_bytes);
        return;
    }
    if std::env::args().any(|argument| argument == "--capsule") {
        println!("TOS build workspace, over a capsule-backed source set");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let unit_bytes = std::env::args()
            .skip_while(|argument| argument != "--unit-bytes")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(SOURCE_CEILING);
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok());
        capsule_source(unit_bytes, modules);
        return;
    }
    if std::env::args().any(|argument| argument == "--lowering") {
        println!("TOS implementation-arena bound: phased lowering to images");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);
        let shape = std::env::args()
            .skip_while(|argument| argument != "--shape")
            .nth(1)
            .unwrap_or_else(|| String::from("chain"));
        let shape = match shape.as_str() {
            "wide" => Shape::WideFanIn,
            "balanced" => Shape::Balanced,
            _ => Shape::Chain,
        };
        phased_lowering(shape, modules, SOURCE_CEILING);
        return;
    }
    if std::env::args().any(|argument| argument == "--phases") {
        println!("TOS implementation-arena bound: phase breakdown");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(8);
        phase_breakdown(modules, SOURCE_CEILING);
        return;
    }
    if std::env::args().any(|argument| argument == "--ceiling") {
        println!("TOS implementation-arena bound: execute_set at the published ceiling");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        let modules = std::env::args()
            .skip_while(|argument| argument != "--modules")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(CLOSURE_CEILING);
        let unit_bytes = std::env::args()
            .skip_while(|argument| argument != "--unit-bytes")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(SOURCE_CEILING);
        let (total, corpus, above) = the_published_ceiling(modules, unit_bytes);
        println!();
        println!("== the bound ==");
        println!(
            "execute_set over {modules} x {unit_bytes} B: {:>12} B ({:.2} MiB) above the corpus",
            above,
            mib(above)
        );
        println!(
            "  corpus {:>12} B ({:.2} MiB); total frontier {:>12} B ({:.2} MiB)",
            corpus,
            mib(corpus),
            total,
            mib(total)
        );
        return;
    }
    if std::env::args().any(|argument| argument == "--closure") {
        println!("TOS implementation-arena bound: one executed closure");
        println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
        println!();
        let sizes: &[usize] = if full { &[2, 4, 8, 16, 32] } else { &[2, 4, 8] };
        let measured = an_executed_closure(sizes);
        println!();
        println!("== the bound ==");
        for (count, peak) in &measured {
            println!(
                "one executed closure of {count:>3} modules  {:>12} bytes  ({:.2} MiB)",
                peak,
                mib(*peak)
            );
        }
        return;
    }
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
/// One run over a whole closure: every module read, checked, resolved,
/// lowered, verified and executed together, with the entry calling into each
/// dependency so none of them is merely present.
///
/// Measured at several closure sizes rather than one, because the question a
/// launcher asks is not "what does a closure cost" but "what does it cost as it
/// grows". A single number would be a data point wearing a bound's clothes.
fn an_executed_closure(sizes: &[usize]) -> Vec<(usize, usize)> {
    println!();
    println!("== one executed closure ==");
    let mut measured = Vec::new();
    for &count in sizes {
        let dependencies: Vec<String> = (1..count).map(dependency_module).collect();
        let entry = entry_calling(count - 1);
        let mut units = vec![Unit {
            path: "set/entry.tos",
            bytes: entry.as_bytes(),
        }];
        let paths: Vec<String> = (1..count).map(module_path).collect();
        for (index, text) in dependencies.iter().enumerate() {
            units.push(Unit {
                path: &paths[index],
                bytes: text.as_bytes(),
            });
        }
        let before = arena();
        let run = execute_set(
            &SetRequest {
                source_set: "tos-arena-bound",
                units: &units,
                entry_path: "set/entry.tos",
                entry: "main",
            },
            Vec::new(),
            &mut Silent,
            &mut Unreachable,
        )
        .expect("the set names an entry it contains");
        let after = arena();
        let Run::Completed(completion) = &run else {
            panic!("the closure must complete: {:?}", run.failed_at());
        };
        // The answer is the sum of what every dependency returned, so a run
        // that skipped one could not produce it.
        let expected = (1..count as i128).sum::<i128>();
        let tos_engine::Value::Int(_, number) = completion.value else {
            panic!("the entry returns an integer");
        };
        assert_eq!(number, expected, "every dependency must have been reached");
        println!(
            "{count:>3} modules: peak extent {} bytes ({:.2} MiB); committed {} -> {}; blocks {} ({} free)",
            after.frontier,
            mib(after.frontier),
            before.committed,
            after.committed,
            after.blocks,
            after.free
        );
        measured.push((count, after.frontier));
    }
    measured
}

/// Watches the production pipeline and records the arena at every stage
/// boundary, so a phase's peak is the phase's and not a later one's.
struct PhaseTrace {
    marks: Vec<(PipelineStage, Arena)>,
}

impl tos_pipeline::Trace for PhaseTrace {
    fn entering(&mut self, stage: PipelineStage) {
        self.marks.push((stage, arena()));
    }
}

/// The whole production path, through the production API, measured by phase.
///
/// Not a harness reproducing the architecture: this calls `execute_set`, which
/// is the same entry the freestanding runtime image calls on the boot path.
///
/// The corpus is outside the arena on purpose — see `capsule_bytes`. What the
/// arena measures is what comes out of the process's grant.
fn production_path(shape: Shape, count: usize, unit_bytes: usize) {
    println!();
    println!(
        "== production path, {} shape, {count} modules of {unit_bytes} bytes ==",
        shape.named()
    );
    let (texts, paths) = shaped_units(shape, count, unit_bytes);
    let capsule: Vec<&'static [u8]> = texts.iter().map(|text| capsule_bytes(text)).collect();
    let capsule_bytes_total: usize = capsule.iter().map(|bytes| bytes.len()).sum();
    // The paths stay in the arena: they are tiny, and in the real path they are
    // in the launch record rather than the capsule.
    let units: Vec<Unit<'_>> = paths
        .iter()
        .zip(capsule.iter())
        .map(|(path, bytes)| Unit {
            path: path.as_str(),
            bytes,
        })
        .collect();
    let entry_path = paths.last().expect("a fixture has an entry").as_str();

    let before = arena();
    let mut trace = PhaseTrace { marks: Vec::new() };
    let run = execute_set(
        &SetRequest {
            source_set: "tos-arena-bound",
            units: &units,
            entry_path,
            entry: "main",
        },
        Vec::new(),
        &mut trace,
        &mut Unreachable,
    );
    let after = arena();

    println!(
        "  capsule source backing (outside the grant) {:>12} B ({:.2} MiB)",
        capsule_bytes_total,
        mib(capsule_bytes_total)
    );
    println!(
        "  arena before the run                       {:>12} B committed, frontier {} B",
        before.committed, before.frontier
    );
    println!("  phase boundaries, arena as each was entered:");
    let mut previous: Option<(PipelineStage, Arena)> = None;
    for (stage, at) in &trace.marks {
        if let Some((last, before)) = previous {
            println!(
                "    {:<9} committed {:>12} B ({:>7.2} MiB)  frontier {:>12} B ({:>7.2} MiB)  \
                 rose {:>12} B",
                last.symbol(),
                at.committed,
                mib(at.committed),
                at.frontier,
                mib(at.frontier),
                at.frontier.saturating_sub(before.frontier)
            );
        }
        previous = Some((*stage, *at));
    }
    if let Some((last, at)) = previous {
        println!(
            "    {:<9} committed {:>12} B ({:>7.2} MiB)  frontier {:>12} B ({:>7.2} MiB)  entered",
            last.symbol(),
            at.committed,
            mib(at.committed),
            at.frontier,
            mib(at.frontier)
        );
    }
    println!(
        "    {:<9} committed {:>12} B ({:>7.2} MiB)  frontier {:>12} B ({:>7.2} MiB)  finished",
        "done",
        after.committed,
        mib(after.committed),
        after.frontier,
        mib(after.frontier)
    );
    println!(
        "  whole-run peak inside the grant            {:>12} B ({:.2} MiB)",
        after.frontier,
        mib(after.frontier)
    );
    println!(
        "  margin inside RUNTIME_GRANT = 54 MiB       {:>12} B ({:.2} MiB)",
        (54 * 1024 * 1024usize).saturating_sub(after.frontier),
        mib((54 * 1024 * 1024usize).saturating_sub(after.frontier))
    );
    match &run {
        Ok(Run::Completed(completion)) => println!(
            "  outcome: completed {:?}, fuel {} of {}",
            completion.value, completion.accounting.fuel_used, completion.accounting.fuel_limit
        ),
        Ok(other) => println!(
            "  outcome: DID NOT COMPLETE, failed at {:?}",
            other.failed_at()
        ),
        Err(error) => println!("  outcome: set refused, {}", error.symbol()),
    }
}

/// What a build workspace costs, apart from what it hands over (ADR-0073 §7).
///
/// **Two accounts, measured where they part.** `build_from_provider` is the
/// build side and ends when it returns; `admit` is the target side and is the
/// first thing that verifies anything. The frontier at the boundary is what the
/// build needed; `image_bytes` is the part of it that must survive the
/// workspace, because it is what the admission is handed.
///
/// The two are reported separately because they answer different questions. How
/// large a build workspace must be depends on whether the images accumulate
/// inside it or are written somewhere that outlives it — and that is exactly
/// what has not been decided. `frontier - images` is the transient part: what
/// the workspace holds that nothing downstream ever sees.
///
/// One count per process. The arena's frontier never falls, so a second
/// measurement in the same process would report the first one's high-water mark.
fn build_workspace(shape: Shape, count: usize, unit_bytes: usize) {
    println!();
    println!(
        "== build workspace, {} shape, {count} modules of {unit_bytes} bytes ==",
        shape.named()
    );
    let (texts, paths) = shaped_units(shape, count, unit_bytes);
    // Outside the arena, where a capsule's source physically is.
    let capsule: Vec<&'static [u8]> = texts.iter().map(|text| capsule_bytes(text)).collect();
    let source_backing: usize = capsule.iter().map(|bytes| bytes.len()).sum();
    // **The fixture's own copy goes here, before anything is measured.** The
    // generator builds its text through the global allocator, which is the
    // arena; leaving it there would put the whole corpus inside an account that
    // is supposed to be reading source from outside one, and every number below
    // would carry it. The bytes the build actually reads are the capsule copies.
    drop(texts);
    let units: Vec<Unit<'_>> = paths
        .iter()
        .zip(capsule.iter())
        .map(|(path, bytes)| Unit {
            path: path.as_str(),
            bytes,
        })
        .collect();
    let entry_path = paths.last().expect("a fixture has an entry").as_str();
    let provider = tos_pipeline::SliceSourceProvider::new(&units);
    measure_build(&provider, entry_path, source_backing);
}

/// Builds through one provider, reports both accounts, and runs the entry.
///
/// Everything measurable happens between the two `arena()` calls, and what is
/// between them is the production pair: `build_from_provider`, then `admit`.
/// `source_backing` is what the caller's source costs outside both accounts,
/// reported so a reader can see it is not in either.
fn measure_build(provider: &dyn tos_pipeline::SourceProvider, entry_path: &str, backing: usize) {
    let before = arena();
    let built =
        tos_pipeline::build_from_provider(provider, "tos-arena-bound", entry_path, &mut Silent)
            .expect("the fixture names an entry it contains");
    let boundary = arena();
    let tos_pipeline::Build::Ready(built) = built else {
        panic!("the fixture builds");
    };
    let images = built.image_bytes();
    let modules = built.modules();

    println!(
        "  source backing, outside both accounts   {:>12} B ({:>7.2} MiB)",
        backing,
        mib(backing)
    );
    println!(
        "  arena before the build                  {:>12} B committed, frontier {} B",
        before.committed, before.frontier
    );
    println!(
        "  build frontier, workspace and products  {:>12} B ({:>7.2} MiB)",
        boundary.frontier,
        mib(boundary.frontier)
    );
    println!(
        "  committed at the boundary               {:>12} B ({:>7.2} MiB)",
        boundary.committed,
        mib(boundary.committed)
    );
    println!(
        "  images handed to the admission          {:>12} B ({:>7.2} MiB) over {modules} modules",
        images,
        mib(images)
    );
    // **What is live at the boundary is the product, not the workspace.** The
    // summaries, the plan, the surfaces and the lowering views are locals of the
    // build and are gone the instant it returns, so what `committed` still shows
    // is what a `BuiltClosure` holds: the images, and the declaration the
    // verifier will be held to. The workspace's own composition is not visible
    // from here — `--lowering` walks the same phases and reads the arena between
    // them, and that is where it is attributed.
    println!(
        "  declaration handed with them            {:>12} B ({:>7.2} MiB)",
        boundary.committed.saturating_sub(images),
        mib(boundary.committed.saturating_sub(images))
    );
    println!(
        "  what survives the workspace, in total   {:>12} B ({:>7.2} MiB)",
        boundary.committed,
        mib(boundary.committed)
    );
    println!(
        "  frontier above what survives            {:>12} B ({:>7.2} MiB)",
        boundary.frontier.saturating_sub(boundary.committed),
        mib(boundary.frontier.saturating_sub(boundary.committed))
    );
    println!(
        "  per module: image {:>9.2} KiB   declaration {:>9.2} KiB",
        images as f64 / modules as f64 / 1024.0,
        boundary.committed.saturating_sub(images) as f64 / modules as f64 / 1024.0
    );

    // The target side, from here on. It is measured in the same process because
    // that is the only way to see whether it needs anything *above* the build's
    // high-water mark — but its own bound is not this number: what a process
    // grant must hold is measured under the grant itself, in
    // `tests/residency --launch`.
    let admitted = tos_pipeline::admit(*built, "main", &mut Silent, tos_pipeline::HOST_RESIDENCY);
    let after_admission = arena();
    let tos_pipeline::Preparation::Ready(mut prepared) = admitted else {
        panic!("the built closure is admitted");
    };
    let run = tos_pipeline::run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
    let after_run = arena();
    println!(
        "  admission, above the build's frontier   {:>12} B ({:>7.2} MiB)",
        after_admission.frontier.saturating_sub(boundary.frontier),
        mib(after_admission.frontier.saturating_sub(boundary.frontier))
    );
    println!(
        "  run, above the admission's frontier     {:>12} B ({:>7.2} MiB)",
        after_run.frontier.saturating_sub(after_admission.frontier),
        mib(after_run.frontier.saturating_sub(after_admission.frontier))
    );
    println!(
        "  whole-process frontier                  {:>12} B ({:>7.2} MiB)",
        after_run.frontier,
        mib(after_run.frontier)
    );
    // What the run did is reported exactly, because the two ways it can end
    // short are not the same news. A trap is the fixture meeting a limit it
    // declared — the chain fixture declares `recursion: 8`, so a closure deeper
    // than that traps by its own envelope and the build is not what failed. A
    // refusal before the first instruction is.
    match &run {
        Run::Completed(completion) => println!(
            "  outcome: completed {:?}, fuel {} of {}",
            completion.value, completion.accounting.fuel_used, completion.accounting.fuel_limit
        ),
        Run::Trapped { code, detail, .. } => println!(
            "  outcome: verified and executed, then trapped {code} ({detail}) — \
             the fixture's own declared bound"
        ),
        other => println!("  outcome: DID NOT RUN: {other:?}"),
    }
}

/// What a Capsule v1 can carry, and what building from it costs (ADR-0073, B).
///
/// **The provider is the real one.** `CapsuleSourceProvider` reads the capsule's
/// own path table and hands out windows into its mapped payload, so what is
/// measured is a build over a boot's actual source backend rather than over
/// units a caller already held.
///
/// How many modules fit is **derived, not assumed**: the fixture is built at a
/// count estimated from the ceiling and then reduced until the builder produces
/// a capsule that is within `MAX_CAPSULE_BYTES` and parses. A capsule that was
/// merely close to the ceiling would not answer the question.
///
/// This establishes the provider and the algorithm. It does not establish that
/// a build worker can hand its output to another process, and it is not a claim
/// about source larger than a capsule (ADR-0073's claim C).
fn capsule_source(unit_bytes: usize, requested: Option<usize>) {
    println!();
    println!("== capsule-backed build, units of {unit_bytes} bytes ==");
    // Per file the capsule spends a path entry, a file entry and a name; the
    // estimate only has to be close, because it is corrected by building.
    let per_file = unit_bytes + 128;
    let mut count = requested.unwrap_or(tos_capsule::MAX_CAPSULE_BYTES / per_file);
    let (bytes, carried) = loop {
        assert!(
            count >= 2,
            "a chain fixture needs an entry and a dependency"
        );
        let (texts, paths) = capsule_chain(count, unit_bytes);
        let carried: usize = texts.iter().map(|text| text.len()).sum();
        let mut builder = tos_capsule::build::Builder::new();
        for (path, text) in paths.iter().zip(texts.iter()) {
            builder.add(tos_capsule::build::FileSpec::new(path, text.as_bytes()));
        }
        // Not source, and the provider will not offer it: a capsule carries more
        // than modules, and a set that included the version marker would ask the
        // frontend to parse a file that never claimed to be one.
        builder.add(tos_capsule::build::FileSpec::new(
            "/system/version",
            b"0.2.1\n",
        ));
        builder.set_licence_notice(b"NOTICES\n".to_vec());
        match builder.build() {
            Ok(bytes) if bytes.len() <= tos_capsule::MAX_CAPSULE_BYTES => break (bytes, carried),
            _ => count -= 1,
        }
    };

    // Outside the arena, where a mapped capsule physically is. The builder's own
    // copy goes with the fixture's, before anything is measured.
    let capsule_length = bytes.len();
    let mapped = outside_the_arena(&bytes);
    drop(bytes);
    let capsule = tos_capsule::parse(mapped).expect("the fixture capsule parses");
    let provider = tos_capsule_source::CapsuleSourceProvider::over(capsule);
    let offered = provider.catalog().len();

    println!(
        "  capsule                                 {:>12} B of {} B ceiling, {} B spare",
        capsule_length,
        tos_capsule::MAX_CAPSULE_BYTES,
        tos_capsule::MAX_CAPSULE_BYTES - capsule_length
    );
    println!(
        "  source carried                          {:>12} B ({:>7.2} MiB) in {count} units",
        carried,
        mib(carried)
    );
    println!("  offered as a set by the provider        {offered:>12} units");
    measure_build(&provider, "system/boot/init.tos", capsule_length);
}

/// A chain of source units with capsule paths, the entry at the boot path.
///
/// A capsule's boot file is `/system/boot/init.tos` by contract, so the entry is
/// stored there and the dependencies under `/system/lib/`. The declared
/// recursion covers the chain's own depth: a fixture that trapped on its own
/// envelope halfway down would leave the run unproven, and what is being
/// measured is what a capsule can carry rather than how deep a call may go.
fn capsule_chain(count: usize, unit_bytes: usize) -> (Vec<String>, Vec<String>) {
    let envelope = "resource [fuel: 100000000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
         workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 1000, imports: 8] ";
    let mut texts = Vec::with_capacity(count);
    let mut paths = Vec::with_capacity(count);
    for index in 0..count - 1 {
        let mut text = if index == 0 {
            format!(
                "module system.lib.m0 version 1.0 profile bootstrap; {envelope} \
                 pub fn value0() -> i32 {{ return 1i32; }} "
            )
        } else {
            format!(
                "module system.lib.m{index} version 1.0 profile bootstrap; \
                 import system.lib.m{prev} as prev; {envelope} \
                 pub fn value{index}() -> i32 {{ return prev.value{prev}(); }} ",
                prev = index - 1
            )
        };
        fill_to(&mut text, index, unit_bytes);
        texts.push(text);
        paths.push(format!("/system/lib/m{index}.tos"));
    }
    let last = count - 2;
    let mut entry = format!(
        "module system.boot.init version 1.0 profile bootstrap; \
         import system.lib.m{last} as prev; {envelope} \
         pub fn main() -> i32 {{ return prev.value{last}(); }} "
    );
    fill_to(&mut entry, count, unit_bytes);
    texts.push(entry);
    paths.push(String::from("/system/boot/init.tos"));
    (texts, paths)
}

/// Which graph a lowering measurement is taken over.
///
/// One number for "256 modules" is not a bound, because what is live during
/// lowering is a property of the **shape**, not the count. A chain holds one
/// dependency view at a time; a wide fan-in holds its whole fan; a balanced DAG
/// sits between them. All three are measured.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    /// One direct import per module, 256 deep.
    Chain,
    /// One entry importing as many dependencies as a conforming source unit can
    /// name, each of them interface-heavy.
    WideFanIn,
    /// A binary DAG: each module imports the two below it.
    Balanced,
}

impl Shape {
    fn named(self) -> &'static str {
        match self {
            Shape::Chain => "A chain",
            Shape::WideFanIn => "B wide fan-in",
            Shape::Balanced => "C balanced DAG",
        }
    }
}

/// The units of one shape, dependencies first and the entry last.
///
/// The entry of a wide fan-in is **derived from source byte accounting**, not
/// guessed: import lines and call sites are appended until the next one would
/// cross the source ceiling, and the fan is whatever fitted. A fixture that
/// assumed 255 imports fit in 256 KiB would be measuring a module that cannot
/// exist.
fn shaped_units(shape: Shape, count: usize, unit_bytes: usize) -> (Vec<String>, Vec<String>) {
    let mut texts: Vec<String> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    match shape {
        Shape::Chain => {
            texts.push(canonical_module_calling(0, unit_bytes));
            paths.push(module_path(0));
            for index in 1..count {
                let mut text = format!(
                    "module set.m{index} version 1.0 profile bootstrap; \
                     import set.m{} as prev; \
                     resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
                     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 8] \
                     pub record Point{index} [x: i32, y: i32] \
                     pub fn value{index}() -> i32 {{ return prev.value{}(); }} \
                     pub fn total{index}(point: Point{index}) -> i32 {{ \
                     return point.x + point.y; }} ",
                    index - 1,
                    index - 1
                );
                if index == count - 1 {
                    text.push_str(&format!(
                        "pub fn main() -> i32 {{ return prev.value{}(); }} ",
                        index - 1
                    ));
                }
                fill_to(&mut text, index, unit_bytes);
                texts.push(text);
                paths.push(module_path(index));
            }
        }
        Shape::WideFanIn => {
            for index in 0..count.saturating_sub(1) {
                texts.push(canonical_module_calling(index, unit_bytes));
                paths.push(module_path(index));
            }
            // Derived, not assumed: append one import and one use at a time
            // until the next pair would cross the ceiling.
            let head = "module set.entry version 1.0 profile bootstrap; ";
            let envelope = "resource [fuel: 10000000, stack: 64KiB, allocation: 64KiB, \
                 tasks: 1, workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, \
                 imports: 255] ";
            let mut imports = String::new();
            let mut body = String::from("pub fn main() -> i32 { let mut total = 0i32; ");
            let mut fan = 0usize;
            for index in 0..count.saturating_sub(1) {
                let line = format!("import set.m{index} as m{index}; ");
                let use_site = format!("total = total + m{index}.value{index}(); ");
                let projected = head.len()
                    + imports.len()
                    + line.len()
                    + envelope.len()
                    + body.len()
                    + use_site.len()
                    + "return total; }".len();
                if projected > unit_bytes {
                    break;
                }
                imports.push_str(&line);
                body.push_str(&use_site);
                fan += 1;
            }
            body.push_str("return total; }");
            let entry = format!("{head}{imports}{envelope}{body}");
            assert!(entry.len() <= unit_bytes, "the entry is derived to fit");
            println!(
                "  entry fan-in derived from source accounting: {fan} of {} dependencies, \
                 entry {} B of {unit_bytes} B",
                count.saturating_sub(1),
                entry.len()
            );
            texts.push(entry);
            paths.push(String::from("set/entry.tos"));
        }
        Shape::Balanced => {
            for index in 0..count {
                if index < 2 {
                    texts.push(canonical_module_calling(index, unit_bytes));
                } else {
                    let (left, right) = (index - 1, index - 2);
                    let mut text = format!(
                        "module set.m{index} version 1.0 profile bootstrap; \
                         import set.m{left} as l; import set.m{right} as r; \
                         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
                         workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, \
                         imports: 8] \
                         pub record Point{index} [x: i32, y: i32] \
                         pub fn value{index}() -> i32 {{ \
                         return l.value{left}() + r.value{right}(); }} \
                         pub fn total{index}(point: Point{index}) -> i32 {{ \
                         return point.x + point.y; }} "
                    );
                    if index == count - 1 {
                        text.push_str(&format!(
                            "pub fn main() -> i32 {{ return l.value{left}() + r.value{right}(); }} "
                        ));
                    }
                    fill_to(&mut text, index, unit_bytes);
                    texts.push(text);
                }
                paths.push(module_path(index));
            }
        }
    }
    (texts, paths)
}

/// Pads a module towards the source ceiling with ordinary declarations.
fn fill_to(text: &mut String, index: usize, bytes: usize) {
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
}

/// The production lowering path, measured term by term.
///
/// ADR-0040 bounds the whole machine, not the execution phase, so the shape that
/// matters is not what survives to the first instruction but what is alive at
/// the worst moment of lowering. Every term below is the production path's, and
/// the lowering views obey the production path's deterministic liveness: a view
/// is dropped the instant its last consumer in the lowering order is done.
fn phased_lowering(shape: Shape, count: usize, unit_bytes: usize) {
    println!();
    println!(
        "== phased lowering, {} shape, {count} modules of {unit_bytes} bytes ==",
        shape.named()
    );
    let (texts, paths) = shaped_units(shape, count, unit_bytes);
    let units: Vec<Unit<'_>> = texts
        .iter()
        .zip(paths.iter())
        .map(|(text, path)| Unit {
            path: path.as_str(),
            bytes: text.as_bytes(),
        })
        .collect();

    // The fixture's own source text, which the caller owns in production too:
    // `Unit.bytes` is borrowed, so it is not the pipeline's storage. Measured
    // separately so the pipeline's terms are the pipeline's.
    let fixture_bytes: usize = texts.iter().map(|text| text.capacity()).sum();
    let before_summaries = arena();

    // Read, parse, check, summarize — one module at a time, nothing kept but the
    // owned summary.
    let mut summaries: Vec<ModuleSummary> = Vec::with_capacity(units.len());
    for unit in &units {
        let source = SourceReader::read(unit.bytes).expect("the fixture is valid");
        let parsed = Parser::parse_schema(&source);
        let schema = parsed.into_accepted().expect("the fixture parses");
        summaries.push(ModuleEntry::new(unit.path, &source, &schema).summarize());
    }
    let names: Vec<String> = summaries.iter().map(|s| s.name.clone()).collect();
    let after_summaries = arena();
    // Measured, not estimated: what the summary pass left behind is what the
    // arena is holding that it was not holding before. An estimate over the
    // fields this harness happens to know about would miss the ones it does not.
    let plan_bytes = after_summaries
        .committed
        .saturating_sub(before_summaries.committed);

    // The lowering order: the entry last.
    let entry_index = units.len() - 1;
    let order: Vec<usize> = closure_order(&summaries, entry_index, &names);
    let last_consumer = last_consumers(&summaries, &order, &names);

    let mut interfaces: Vec<Option<LoweringInterface>> = (0..units.len()).map(|_| None).collect();
    let mut surfaces: Vec<VerificationSurface> = Vec::with_capacity(units.len());
    let mut image_bytes = 0usize;
    let mut worst_scratch = 0usize;
    let mut worst_frontier = 0usize;
    let mut max_live_count = 0usize;
    let mut max_live_bytes = 0usize;
    let mut max_surface_bytes = 0usize;
    for (position, &index) in order.iter().enumerate() {
        let context = ModuleContext {
            source_set: "tos-arena-bound".to_string(),
            path: summaries[index].path.clone(),
            content_id: tos_pipeline::content_id(units[index].bytes),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        };
        let imports: Vec<ResolvedImport<'_>> = interfaces
            .iter()
            .enumerate()
            .filter_map(|(at, held)| {
                held.as_ref().map(|interface| ResolvedImport {
                    name: names[at].as_str(),
                    interface,
                })
            })
            .collect();
        let before = arena();
        let source = SourceReader::read(units[index].bytes).expect("the fixture is valid");
        let reparsed = Parser::parse_schema(&source);
        let schema = reparsed.into_accepted().expect("the fixture parses");
        let module =
            lower_module_in_set(&source, &schema, &context, &imports).expect("the fixture lowers");
        let peak = arena();
        worst_scratch = worst_scratch.max(peak.committed.saturating_sub(before.committed));
        worst_frontier = worst_frontier.max(peak.frontier);
        drop(imports);
        drop(schema);
        drop(source);

        interfaces[index] = Some(LoweringInterface::of(&module));
        surfaces.push(VerificationSurface::of(&module));
        let (image, _) = tos_image::encode(&module);
        image_bytes += image.len();
        drop(image);
        drop(module);

        for (at, held) in interfaces.iter_mut().enumerate() {
            if held.is_some() && last_consumer[at] == Some(position) {
                *held = None;
            }
        }
        let live: Vec<&LoweringInterface> = interfaces.iter().flatten().collect();
        max_live_count = max_live_count.max(live.len());
        max_live_bytes = max_live_bytes.max(
            live.iter()
                .map(|interface| interface.retained_bytes())
                .sum::<usize>(),
        );
        max_surface_bytes = max_surface_bytes.max(
            surfaces
                .iter()
                .map(|surface| surface.retained_bytes())
                .sum::<usize>(),
        );
    }
    let settled = arena();
    let surface_bytes: usize = surfaces
        .iter()
        .map(|surface| surface.retained_bytes())
        .sum();

    println!(
        "  fixture source text (caller-owned)      {:>12} B ({:.2} MiB)",
        fixture_bytes,
        mib(fixture_bytes)
    );
    println!(
        "  current SourceUnit and tree, worst turn {:>12} B ({:.2} MiB)",
        worst_scratch,
        mib(worst_scratch)
    );
    println!(
        "  closure plan (summaries)                {:>12} B ({:.2} MiB)",
        plan_bytes,
        mib(plan_bytes)
    );
    println!("  live lowering interfaces, maximum       {:>12} B ({:.2} MiB) over {max_live_count} modules", max_live_bytes, mib(max_live_bytes));
    println!(
        "  live verifier surfaces, maximum         {:>12} B ({:.2} MiB) over {} modules",
        max_surface_bytes,
        mib(max_surface_bytes),
        surfaces.len()
    );
    println!(
        "  accumulated image bytes                 {:>12} B ({:.2} MiB)",
        image_bytes,
        mib(image_bytes)
    );
    println!(
        "  verifier surfaces, final                {:>12} B ({:.2} MiB)",
        surface_bytes,
        mib(surface_bytes)
    );
    println!(
        "  arena after summaries                   {:>12} B ({:.2} MiB) committed",
        after_summaries.committed,
        mib(after_summaries.committed)
    );
    println!(
        "  arena settled after lowering            {:>12} B ({:.2} MiB) committed",
        settled.committed,
        mib(settled.committed)
    );
    println!(
        "  process frontier                        {:>12} B ({:.2} MiB)",
        worst_frontier.max(settled.frontier),
        mib(worst_frontier.max(settled.frontier))
    );
}

/// The lowering order for a fixture: dependencies before the modules that
/// import them, entry last.
fn closure_order(summaries: &[ModuleSummary], entry: usize, names: &[String]) -> Vec<usize> {
    let mut order = Vec::with_capacity(summaries.len());
    let mut settled = alloc_set(summaries.len());
    fn walk(
        index: usize,
        summaries: &[ModuleSummary],
        names: &[String],
        settled: &mut Vec<bool>,
        order: &mut Vec<usize>,
    ) {
        if settled[index] {
            return;
        }
        settled[index] = true;
        for import in &summaries[index].imports {
            if let Some(at) = names.iter().position(|name| *name == import.target) {
                walk(at, summaries, names, settled, order);
            }
        }
        order.push(index);
    }
    walk(entry, summaries, names, &mut settled, &mut order);
    order
}

fn alloc_set(len: usize) -> Vec<bool> {
    (0..len).map(|_| false).collect()
}

/// The last position of the lowering order that reads each module's view.
fn last_consumers(
    summaries: &[ModuleSummary],
    order: &[usize],
    names: &[String],
) -> Vec<Option<usize>> {
    let mut last: Vec<Option<usize>> = (0..summaries.len()).map(|_| None).collect();
    for (position, &index) in order.iter().enumerate() {
        for import in &summaries[index].imports {
            if let Some(at) = names.iter().position(|name| *name == import.target) {
                last[at] = Some(position);
            }
        }
    }
    last
}

/// What a lowered module is made of: semantic payload against representation.
///
/// Diagnostic. `canonical_stream` is used here **only** as a density estimate
/// for the semantic content — docs/43 has deliberately not fixed an on-disk
/// encoding, and nothing here proposes one. The question it answers is narrow:
/// of the live bytes a `tos_ir::Module` occupies, how many are the module's
/// meaning and how many are this representation carrying it.
fn ir_breakdown(count: usize, unit_bytes: usize) {
    println!();
    println!("== lowered IR, {count} modules of {unit_bytes} bytes ==");
    let dependencies: Vec<String> = (1..count)
        .map(|index| canonical_module_calling(index, unit_bytes))
        .collect();
    let entry = entry_summing(count.max(2) - 1, unit_bytes);
    let paths: Vec<String> = (1..count).map(module_path).collect();
    let mut units = vec![Unit {
        path: "set/entry.tos",
        bytes: entry.as_bytes(),
    }];
    for (index, text) in dependencies.iter().enumerate() {
        units.push(Unit {
            path: &paths[index],
            bytes: text.as_bytes(),
        });
    }
    let mut sources = Vec::with_capacity(units.len());
    for unit in &units {
        sources.push(SourceReader::read(unit.bytes).expect("the fixture is valid"));
    }
    let summaries: Vec<ModuleSummary> = sources
        .iter()
        .zip(units.iter())
        .map(|(source, unit)| {
            let parsed = Parser::parse_schema(source);
            let schema = parsed.into_accepted().expect("the fixture parses");
            ModuleEntry::new(unit.path, source, &schema).summarize()
        })
        .collect();
    let names: Vec<String> = summaries.iter().map(|s| s.name.clone()).collect();

    let mut lowered: Vec<(usize, tos_ir::Module)> = Vec::with_capacity(units.len());
    let order: Vec<usize> = (1..units.len()).chain(core::iter::once(0)).collect();
    let mut live_total = 0usize;
    let mut stream_total = 0usize;
    for &index in &order {
        let context = ModuleContext {
            source_set: "tos-arena-bound".to_string(),
            path: summaries[index].path.clone(),
            content_id: tos_pipeline::content_id(sources[index].bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        };
        // This harness measures the all-resident lowering slope on purpose, so
        // it keeps every module. The interfaces are derived per turn from what
        // it holds, which is what the production path derives once and keeps
        // instead of the module.
        let interfaces: Vec<(usize, LoweringInterface)> = lowered
            .iter()
            .map(|(at, module)| (*at, LoweringInterface::of(module)))
            .collect();
        let imports: Vec<ResolvedImport<'_>> = interfaces
            .iter()
            .map(|(at, interface)| ResolvedImport {
                name: names[*at].as_str(),
                interface,
            })
            .collect();
        let before = arena();
        let reparsed = Parser::parse_schema(&sources[index]);
        let schema = reparsed.into_accepted().expect("the fixture parses");
        let module = lower_module_in_set(&sources[index], &schema, &context, &imports)
            .expect("the fixture lowers");
        drop(schema);
        let after = arena();
        let live = after.committed.saturating_sub(before.committed);
        let stream = tos_ir::canonical_stream(&module).len();
        live_total += live;
        stream_total += stream;
        report_module(index, live, stream, &module);
        lowered.push((index, module));
    }
    println!();
    println!(
        "  totals: live {} B ({:.2} MiB); canonical stream {} B ({:.2} MiB); ratio {:.1}x",
        live_total,
        mib(live_total),
        stream_total,
        mib(stream_total),
        live_total as f64 / stream_total.max(1) as f64
    );
}

/// One lowered module, counted.
fn report_module(index: usize, live: usize, stream: usize, module: &tos_ir::Module) {
    let blocks: usize = module.functions.iter().map(|f| f.blocks.len()).sum();
    let instructions: usize = module
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .map(|b| b.instructions.len())
        .sum();
    let values: usize = module.functions.iter().map(|f| f.values.len()).sum();
    // The source map's own strings, and how much of them is the same text
    // repeated. Six owned strings per entry, and five of them name the module
    // rather than the operation.
    let mut map_bytes = 0usize;
    let mut unique = std::collections::BTreeSet::new();
    for entry in &module.source_map {
        for text in [
            &entry.source_set,
            &entry.path,
            &entry.content_id,
            &entry.frontend_identity,
            &entry.language_version,
            &entry.unicode_normalization_baseline,
        ] {
            map_bytes += text.len();
            unique.insert(text.clone());
        }
    }
    let unique_bytes: usize = unique.iter().map(|text| text.len()).sum();
    println!(
        "  module {index:>3}: live {:>11} B ({:>6.2} MiB)  stream {:>10} B ({:>5.2} MiB)  {:>5.1}x",
        live,
        mib(live),
        stream,
        mib(stream),
        live as f64 / stream.max(1) as f64
    );
    println!(
        "            types {:>5} imports {:>4} cap-imports {:>3} exports {:>5} constants {:>5}",
        module.types.len(),
        module.imports.len(),
        module.capability_imports.len(),
        module.exports.len(),
        module.constants.len()
    );
    println!(
        "            functions {:>5} blocks {:>6} instructions {:>7} ssa values {:>7}",
        module.functions.len(),
        blocks,
        instructions,
        values
    );
    println!(
        "            source map {:>6} entries; strings {:>10} B, of which distinct {:>8} B ({:.0}x repeated)",
        module.source_map.len(),
        map_bytes,
        unique_bytes,
        map_bytes as f64 / unique_bytes.max(1) as f64
    );
}

/// Where the arena goes, phase by phase, on the ceiling-sized closure.
///
/// Diagnostic, not conformance. It walks the same phases `execute_set` walks,
/// in the same order, on the same fixture, and reads the arena between them —
/// which `execute_set` itself cannot be asked to do without instrumenting
/// production code. What it is for is attribution: a linear cost per module is
/// a fact, and *which retained object* is linear is a different fact.
fn phase_breakdown(count: usize, unit_bytes: usize) {
    println!();
    println!("== phases, {count} modules of {unit_bytes} bytes ==");
    let dependencies: Vec<String> = (1..count)
        .map(|index| canonical_module_calling(index, unit_bytes))
        .collect();
    let entry = entry_summing(count - 1, unit_bytes);
    let paths: Vec<String> = (1..count).map(module_path).collect();
    let mut units = vec![Unit {
        path: "set/entry.tos",
        bytes: entry.as_bytes(),
    }];
    for (index, text) in dependencies.iter().enumerate() {
        units.push(Unit {
            path: &paths[index],
            bytes: text.as_bytes(),
        });
    }
    let corpus = arena();
    mark("corpus (capsule bytes in TOS, not grant)", corpus, corpus);

    // Read: normalized source units, which is what the frontend works on.
    let mut sources = Vec::with_capacity(units.len());
    for unit in &units {
        let source = SourceReader::read(unit.bytes).expect("the fixture is valid");
        sources.push(source);
    }
    let after_read = arena();
    mark("after read (SourceUnit x N)", corpus, after_read);

    // Parse: the trees. This is the object the accepted architecture says a
    // caller may drop as soon as it has a summary.
    let mut schemas = Vec::with_capacity(sources.len());
    for source in &sources {
        let parsed = Parser::parse_schema(source);
        schemas.push(parsed.into_accepted().expect("the fixture parses"));
    }
    let after_parse = arena();
    mark("after parse (Schema x N)", after_read, after_parse);

    let entries: Vec<ModuleEntry<'_>> = sources
        .iter()
        .zip(schemas.iter())
        .zip(units.iter())
        .map(|((source, schema), unit)| ModuleEntry::new(unit.path, source, schema))
        .collect();
    let after_entries = arena();
    mark("after entries", after_parse, after_entries);

    for entry in &entries {
        let diagnostics = entry.check();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.severity() == tos_core::Severity::Error),
            "the fixture checks clean"
        );
    }
    let after_check = arena();
    mark("after per-module check", after_entries, after_check);

    let summaries: Vec<ModuleSummary> = entries.iter().map(|entry| entry.summarize()).collect();
    let after_summaries = arena();
    mark("after summaries (owned)", after_check, after_summaries);

    let diagnostics = tos_core::check_module_summaries(&summaries);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "the set resolves"
    );
    let after_resolution = arena();
    mark(
        "after set resolution over summaries",
        after_summaries,
        after_resolution,
    );

    // Lowering, in dependency order: the fixture's entry is unit 0 and imports
    // every other, so the dependencies are lowered first and the entry last.
    let names: Vec<String> = entries.iter().map(|entry| entry.summarize().name).collect();
    let mut lowered: Vec<(usize, tos_ir::Module)> = Vec::with_capacity(entries.len());
    let order: Vec<usize> = (1..entries.len()).chain(core::iter::once(0)).collect();
    let mut lowering_marks = Vec::new();
    for &index in &order {
        let context = ModuleContext {
            source_set: "tos-arena-bound".to_string(),
            path: entries[index].path().to_string(),
            content_id: tos_pipeline::content_id(sources[index].bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        };
        // This harness measures the all-resident lowering slope on purpose, so
        // it keeps every module. The interfaces are derived per turn from what
        // it holds, which is what the production path derives once and keeps
        // instead of the module.
        let interfaces: Vec<(usize, LoweringInterface)> = lowered
            .iter()
            .map(|(at, module)| (*at, LoweringInterface::of(module)))
            .collect();
        let imports: Vec<ResolvedImport<'_>> = interfaces
            .iter()
            .map(|(at, interface)| ResolvedImport {
                name: names[*at].as_str(),
                interface,
            })
            .collect();
        let module = lower_module_in_set(&sources[index], &schemas[index], &context, &imports)
            .expect("the fixture lowers");
        lowered.push((index, module));
        lowering_marks.push(arena().frontier);
    }
    let after_lowering = arena();
    mark(
        "after lowering (Module x N)",
        after_resolution,
        after_lowering,
    );
    if lowering_marks.len() > 2 {
        let first = lowering_marks[0] - after_resolution.frontier;
        let last =
            lowering_marks[lowering_marks.len() - 1] - lowering_marks[lowering_marks.len() - 2];
        println!(
            "  first lowering +{} B ({:.2} MiB); last lowering +{} B ({:.2} MiB)",
            first,
            mib(first),
            last,
            mib(last)
        );
    }

    println!(
        "  by phase delta: sources {:.2} MiB, parse trees {:.2} MiB, summaries {:.2} MiB (committed), lowered IR {:.2} MiB",
        mib(after_read.frontier - corpus.frontier),
        mib(after_parse.frontier - after_read.frontier),
        mib(after_summaries.committed - after_check.committed),
        mib(after_lowering.frontier - after_resolution.frontier)
    );
    println!(
        "  per module: parse tree {:.2} MiB, summary {:.2} MiB, lowered IR {:.2} MiB",
        mib((after_parse.frontier - after_read.frontier) / count),
        mib((after_summaries.committed - after_check.committed) / count),
        mib((after_lowering.frontier - after_resolution.frontier) / count)
    );
}

/// One phase boundary.
fn mark(what: &str, before: Arena, now: Arena) {
    println!(
        "  {what:<44} committed {:>12} B  frontier {:>12} B (+{} B, {:.2} MiB)",
        now.committed,
        now.frontier,
        now.frontier - before.frontier,
        mib(now.frontier - before.frontier)
    );
}

/// The full promise, measured: `execute_set` over the published ceiling.
///
/// docs/44 §2 lets an implementation declare a **lower** cap in its conformance
/// profile. This one declares none: `tos_verifier::limits::Limits::default()`
/// is the accepted V1 ceiling — 256 modules in a closure — and
/// `tos_core::MAX_SOURCE_BYTES` is the 256 KiB source unit. So the promise is
/// the ceiling itself, and this is what the promise costs.
///
/// **The source corpus is not the arena's to carry.** In TOS the units are
/// bytes of the capsule, mapped outside the process grant; here they are host
/// allocations, and they are made *before* the frontier is read. What the run
/// needed above them is the difference — the frontier never falls, so the
/// corpus sits below and the delta is the pipeline's own extent. Both figures
/// are printed, because a reader must be able to see which is which.
fn the_published_ceiling(count: usize, unit_bytes: usize) -> (usize, usize, usize) {
    println!();
    println!("== execute_set at the published ceiling ==");
    println!("fixture: {count} modules of {unit_bytes} bytes each");
    // The corpus first, and its extent recorded before anything is run.
    let dependencies: Vec<String> = (1..count)
        .map(|index| canonical_module_calling(index, unit_bytes))
        .collect();
    let entry = entry_summing(count - 1, unit_bytes);
    let paths: Vec<String> = (1..count).map(module_path).collect();
    let mut units = vec![Unit {
        path: "set/entry.tos",
        bytes: entry.as_bytes(),
    }];
    for (index, text) in dependencies.iter().enumerate() {
        units.push(Unit {
            path: &paths[index],
            bytes: text.as_bytes(),
        });
    }
    let corpus = arena();
    println!(
        "corpus in place: frontier {} bytes ({:.2} MiB) — capsule bytes in TOS, not grant",
        corpus.frontier,
        mib(corpus.frontier)
    );
    let run = execute_set(
        &SetRequest {
            source_set: "tos-arena-bound",
            units: &units,
            entry_path: "set/entry.tos",
            entry: "main",
        },
        Vec::new(),
        &mut Silent,
        &mut Unreachable,
    )
    .expect("the set names an entry it contains");
    let after = arena();
    let Run::Completed(completion) = &run else {
        panic!("the closure must complete: {:?}", run.failed_at());
    };
    let expected = (1..count as i128).sum::<i128>();
    let tos_engine::Value::Int(_, number) = completion.value else {
        panic!("the entry returns an integer");
    };
    assert_eq!(number, expected, "every dependency must have been reached");
    let above = after.frontier - corpus.frontier;
    println!(
        "peak extent {} bytes ({:.2} MiB) total; {} bytes ({:.2} MiB) above the corpus",
        after.frontier,
        mib(after.frontier),
        above,
        mib(above)
    );
    println!(
        "committed {} -> {}; blocks {} ({} free)",
        corpus.committed, after.committed, after.blocks, after.free
    );
    (after.frontier, corpus.frontier, above)
}

/// A ceiling-sized dependency that also exports the function the entry calls.
fn canonical_module_calling(index: usize, bytes: usize) -> String {
    let mut text = canonical_module(index, bytes);
    text.push_str(&format!(
        "pub fn value{index}() -> i32 {{ return {index}i32; }} "
    ));
    text
}

/// A ceiling-sized entry that imports every dependency and calls each once.
fn entry_summing(count: usize, bytes: usize) -> String {
    let mut text = String::from("module set.entry version 1.0 profile bootstrap; ");
    for index in 1..=count {
        text.push_str(&format!("import set.m{index} as m{index}; "));
    }
    text.push_str(
        "resource [fuel: 10000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 256] ",
    );
    // Padding first, so the entry is a ceiling-sized unit like every other.
    let mut filler = 0usize;
    while text.len() + 4096 < bytes {
        text.push_str(&format!(
            "pub fn entry_fill{filler}(x: i32) -> i32 {{ return x + {filler}i32; }} "
        ));
        filler += 1;
    }
    text.push_str("pub fn main() -> i32 { return ");
    for index in 1..=count {
        if index > 1 {
            text.push_str(" + ");
        }
        text.push_str(&format!("m{index}.value{index}()"));
    }
    text.push_str("; }");
    text
}

/// A dependency exporting one function that returns its own index.
fn dependency_module(index: usize) -> String {
    format!(
        "module set.m{index} version 1.0 profile bootstrap; \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn value{index}() -> i32 {{ return {index}i32; }} "
    )
}

/// An entry importing every dependency and calling each one exactly once.
fn entry_calling(count: usize) -> String {
    let mut text = String::from("module set.entry version 1.0 profile bootstrap; ");
    for index in 1..=count {
        text.push_str(&format!("import set.m{index} as m{index}; "));
    }
    text.push_str(
        "resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 256] \
         pub fn main() -> i32 { return ",
    );
    for index in 1..=count {
        if index > 1 {
            text.push_str(" + ");
        }
        text.push_str(&format!("m{index}.value{index}()"));
    }
    text.push_str("; }");
    text
}

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
        let run = execute(&request, arguments(), &mut Silent, &mut Unreachable);
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
    let run = execute(&request, arguments(), &mut Silent, &mut Unreachable);
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
