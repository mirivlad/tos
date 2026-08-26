// SPDX-License-Identifier: GPL-3.0-or-later
//! What bounded verified-module residency costs, measured (ADR-0071 evidence).
//!
//! **Measurement only.** No production engine is switched onto anything here,
//! and ADR-0070 §7's implementation gate stands: production integration waits on
//! an image format covering 100 % of `tos-ir/v1` and closing docs/43 §1 in full.
//! The images this harness uses are `TOSIMGx0`, experimental version `0`, whose
//! payload coverage is partial by declaration. That is a bounded concession:
//! what is being measured is residency behaviour — launch shape, working-set
//! peaks, eviction, reload, refusal — and none of that depends on which semantic
//! variants the payload encoder happens to implement.
//!
//! Modes, each in its own process because the arena's frontier never falls:
//!
//! - `--launch --modules N`   sequential launch peak over a closure of N
//! - `--sizes --modules N`    record and manifest size, extrapolated to 256
//! - `--residency --modules N --bound-count C`  steady state under a bound
//! - `--adversarial`          two modules alternating with room for one
//! - `--eviction`             a suspended caller evicted, reloaded, returned into
//! - `--negatives`            missing, stale, substituted, truncated, forged

mod closure;
mod engine;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tos_core::{lower_module_in_set, ModuleContext, Parser, ResolvedImport, SourceReader};
use tos_image_prototype::image;
use tos_ir::Module;
use tos_runtime::{GlobalHeap, RuntimeMemoryGrant, GRANT_VERSION};
use tos_verifier::{Limits, ResolutionSnapshot};

use closure::{ClosureModuleId, Failure, Provider, Snapshot, VerifiedModuleRecord};
use engine::ResidentSet;

const ARENA_BYTES: usize = 2048 * 1024 * 1024;

/// The published ceiling for one normalized source unit (docs/44 §2).
const SOURCE_CEILING: usize = 256 * 1024;

/// The published ceiling for a module dependency closure (docs/44 §2).
const CLOSURE_CEILING: usize = 256;

static ADOPTED: AtomicBool = AtomicBool::new(false);

struct MeasuredHeap {
    heap: GlobalHeap,
}

impl MeasuredHeap {
    fn ensure_adopted(&self) {
        if ADOPTED.swap(true, Ordering::SeqCst) {
            return;
        }
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

fn committed() -> usize {
    HEAP.heap.usage().0
}

fn frontier() -> usize {
    HEAP.heap.usage().1
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn kib(bytes: usize) -> f64 {
    bytes as f64 / 1024.0
}

// -------------------------------------------------------------- the providers

/// The store of images. In TOS these bytes are capsule or cache storage outside
/// the process grant; here they are host allocations made before any measured
/// phase, and every ledger below subtracts them from the grant account.
///
/// A resident module shares the store's `Arc` rather than copying it, which is
/// the right model: an image is mapped, not duplicated into the arena.
struct Store {
    images: Vec<Snapshot>,
}

impl Provider for Store {
    fn image(&self, id: ClosureModuleId) -> Option<Snapshot> {
        self.images.get(id.position()).cloned()
    }
}

impl Store {
    fn bytes(&self) -> usize {
        self.images.iter().map(|image| image.len()).sum()
    }
}

/// Returns nothing for one module.
struct Missing<'a> {
    inner: &'a Store,
    absent: usize,
}

impl Provider for Missing<'_> {
    fn image(&self, id: ClosureModuleId) -> Option<Snapshot> {
        if id.position() == self.absent {
            return None;
        }
        self.inner.image(id)
    }
}

/// Returns a well-formed image that is not the one the record commits to —
/// stale, corrupted or substituted, which are the same condition to a digest.
struct Swapped<'a> {
    inner: &'a Store,
    at: usize,
    instead: Snapshot,
}

impl Provider for Swapped<'_> {
    fn image(&self, id: ClosureModuleId) -> Option<Snapshot> {
        if id.position() == self.at {
            return Some(self.instead.clone());
        }
        self.inner.image(id)
    }
}

/// Returns different bytes on every request.
///
/// The nearest thing to a mutable provider buffer that this interface can
/// express — and the point is that it is not near enough to matter. The loader
/// requests once and hashes and parses **that** snapshot, so a provider that
/// changes its mind between requests changes nothing that is in use. A provider
/// which could write the bytes it had already handed over would be a different
/// story, and `Arc<[u8]>` is why it cannot.
struct Shifting<'a> {
    inner: &'a Store,
    at: usize,
    alternatives: Vec<Snapshot>,
    counter: std::cell::Cell<usize>,
}

impl Provider for Shifting<'_> {
    fn image(&self, id: ClosureModuleId) -> Option<Snapshot> {
        if id.position() == self.at {
            let turn = self.counter.get();
            self.counter.set(turn + 1);
            return Some(self.alternatives[turn % self.alternatives.len()].clone());
        }
        self.inner.image(id)
    }
}

/// A cache that also holds a forged trusted record beside a substituted image.
///
/// ADR-0071 §10. The forgery is never consulted, and the reason is structural
/// rather than diligent: [`Provider`] has one method and it returns bytes.
/// There is no call by which a record could be obtained, so "do not believe a
/// receipt from the cache that supplied the image" is not a rule this harness
/// follows — it is a sentence it cannot say.
struct Poisoned<'a> {
    inner: Swapped<'a>,
    #[allow(dead_code)]
    forged: VerifiedModuleRecord,
}

impl Provider for Poisoned<'_> {
    fn image(&self, id: ClosureModuleId) -> Option<Snapshot> {
        self.inner.image(id)
    }
}

// ---------------------------------------------------------------- the fixture

/// A ceiling-sized dependency exporting one function that returns its index.
fn dependency(index: usize, bytes: usize) -> String {
    let mut text = format!(
        "module set.m{index} version 1.0 profile bootstrap; \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] "
    );
    let mut filler = 0usize;
    loop {
        let chunk = format!(
            "pub record Filler{index}_{filler} [x: i32, y: i32] \
             pub fn fill{index}_{filler}(point: Filler{index}_{filler}) -> i32 \
             {{ return point.x + point.y; }} "
        );
        if text.len() + chunk.len() + 128 > bytes {
            break;
        }
        text.push_str(&chunk);
        filler += 1;
    }
    text.push_str(&format!(
        "pub fn value{index}() -> i32 {{ return {index}i32; }} "
    ));
    text
}

/// An entry that calls the dependencies in exactly the order given.
///
/// The order is the workload. A sweep visits each once; an alternating pattern
/// is the adversarial case §7's bound of one has to survive.
fn entry_calling(pattern: &[usize], imports: usize, bytes: usize) -> String {
    let mut text = String::from("module set.entry version 1.0 profile bootstrap; ");
    for index in 1..=imports {
        text.push_str(&format!("import set.m{index} as m{index}; "));
    }
    text.push_str(
        "resource [fuel: 100000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 256] ",
    );
    let mut filler = 0usize;
    while text.len() + 256 + pattern.len() * 24 < bytes {
        text.push_str(&format!(
            "pub fn entry_fill{filler}(x: i32) -> i32 {{ return x + {filler}i32; }} "
        ));
        filler += 1;
    }
    text.push_str("pub fn main() -> i32 { return ");
    for (at, index) in pattern.iter().enumerate() {
        if at > 0 {
            text.push_str(" + ");
        }
        text.push_str(&format!("m{index}.value{index}()"));
    }
    text.push_str("; }");
    text
}

struct Prepared {
    store: Store,
    /// The declared resolution, **sliced per module**: what each module's own
    /// verification needs, and no more.
    ///
    /// One closure-wide snapshot was the second owner the attribution found. The
    /// verifier consults only the modules a module imports, so the closure-wide
    /// form was holding the other 255 for nothing — and the shape a launch
    /// receives its resolution in is an input-format question, not a verifier
    /// change.
    slices: Vec<ResolutionSnapshot>,
    entry: usize,
    expected: i128,
}

impl Prepared {
    fn resolver(&self) -> impl Fn(usize) -> ResolutionSnapshot + '_ {
        |position: usize| self.slices[position].clone()
    }
}

/// Everything before launch: source, lowering, encoding. Not measured — it is
/// what a build produces, not what an execution costs.
fn prepare(dependencies: usize, pattern: &[usize], unit_bytes: usize) -> Prepared {
    let texts: Vec<String> = (1..=dependencies)
        .map(|index| dependency(index, unit_bytes))
        .collect();
    let entry_text = entry_calling(pattern, dependencies, unit_bytes);

    let mut sources = Vec::with_capacity(dependencies + 1);
    for text in &texts {
        sources.push(SourceReader::read(text.as_bytes()).expect("the fixture is transport-valid"));
    }
    sources
        .push(SourceReader::read(entry_text.as_bytes()).expect("the fixture is transport-valid"));

    let paths: Vec<String> = (1..=dependencies)
        .map(|index| format!("set/m{index}.tos"))
        .collect();

    let mut lowered: Vec<Module> = Vec::with_capacity(sources.len());
    for (at, source) in sources.iter().enumerate() {
        let path = if at < dependencies {
            paths[at].clone()
        } else {
            "set/entry.tos".to_string()
        };
        let context = ModuleContext {
            source_set: "tos-residency-prototype".to_string(),
            path,
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        };
        let imports: Vec<ResolvedImport<'_>> = lowered
            .iter()
            .map(|module| ResolvedImport {
                name: module.header.module_name.as_str(),
                module,
            })
            .collect();
        let schema = Parser::parse_schema(source)
            .into_accepted()
            .expect("the fixture parses");
        lowered.push(
            lower_module_in_set(source, &schema, &context, &imports).expect("the fixture lowers"),
        );
    }

    // The declared resolution: an input to launch, never something launch
    // discovers (docs/43 §5). Sliced per module, because that is what each
    // module's verification actually consults.
    let mut slices: Vec<ResolutionSnapshot> = Vec::with_capacity(lowered.len());
    for module in &lowered {
        let mut slice = ResolutionSnapshot::default();
        let wanted: Vec<&str> = core::iter::once(module.header.module_name.as_str())
            .chain(
                module
                    .imports
                    .iter()
                    .map(|import| import.module_name.as_str()),
            )
            .collect();
        for other in &lowered {
            if !wanted.contains(&other.header.module_name.as_str()) {
                continue;
            }
            slice.modules.insert(
                other.header.module_name.clone(),
                other.header.content_id.clone(),
            );
            slice.exports.insert(
                other.header.module_name.clone(),
                other
                    .exports
                    .iter()
                    .map(|export| export.name.clone())
                    .collect(),
            );
        }
        slices.push(slice);
    }

    let images: Vec<Snapshot> = lowered
        .iter()
        .map(|module| {
            let (bytes, _) = image::encode(module).expect("the fixture is inside the coverage");
            Snapshot::from(bytes.into_boxed_slice())
        })
        .collect();

    let expected = pattern.iter().map(|index| *index as i128).sum();
    drop(lowered);

    Prepared {
        store: Store { images },
        slices,
        entry: dependencies,
        expected,
    }
}

/// Writes a prepared closure to a directory, so that launch can be measured in
/// a process the frontend never ran in.
///
/// This is not a format proposal. The images are `TOSIMGx0`; the sidecar is
/// three kinds of line and exists because the arena's frontier never falls, so
/// a launch measured after a build phase would inherit that phase's high-water
/// mark and report it as its own. The resolution it carries is **launch input**
/// by design (docs/43 §5) — a real launch receives it from the resolution that
/// produced the closure, exactly as this one does.
fn prepare_mode(modules: usize, directory: &str) {
    let (dependencies, pattern) = sweep(modules);
    let prepared = prepare(dependencies, &pattern, SOURCE_CEILING);
    std::fs::create_dir_all(directory).expect("the directory is writable");
    for (at, bytes) in prepared.store.images.iter().enumerate() {
        std::fs::write(format!("{directory}/image.{at:03}"), &bytes[..])
            .expect("the image is writable");
    }
    let mut sidecar = String::new();
    sidecar.push_str(&format!("count {}\n", prepared.store.images.len()));
    sidecar.push_str(&format!("entry {}\n", prepared.entry));
    sidecar.push_str(&format!("expected {}\n", prepared.expected));
    std::fs::write(format!("{directory}/closure.txt"), sidecar).expect("the sidecar is writable");
    // One resolution slice per module, so a launch reads one module's import
    // surface rather than the closure's.
    for (at, slice) in prepared.slices.iter().enumerate() {
        let mut text = String::new();
        for (name, content_id) in &slice.modules {
            text.push_str(&format!("module {name} {content_id}\n"));
        }
        for (name, exports) in &slice.exports {
            for export in exports {
                text.push_str(&format!("export {name} {export}\n"));
            }
        }
        std::fs::write(format!("{directory}/resolution.{at:03}"), text)
            .expect("the slice is writable");
    }
    println!("== prepared ==");
    println!(
        "{} modules, {} B of images, written to {directory}",
        prepared.store.images.len(),
        prepared.store.bytes()
    );
    println!("run --launch --dir {directory} in a fresh process to measure the launch");
}

/// Reads back what `--prepare` wrote.
fn read_prepared(directory: &str) -> (Store, usize) {
    let sidecar =
        std::fs::read_to_string(format!("{directory}/closure.txt")).expect("run --prepare first");
    let mut count = 0usize;
    let mut entry = 0usize;
    for line in sidecar.lines() {
        let mut parts = line.split(' ');
        match (parts.next(), parts.next()) {
            (Some("count"), Some(value)) => count = value.parse().expect("a count"),
            (Some("entry"), Some(value)) => entry = value.parse().expect("an entry index"),
            _ => {}
        }
    }
    let images: Vec<Snapshot> = (0..count)
        .map(|at| {
            let bytes = std::fs::read(format!("{directory}/image.{at:03}")).expect("an image");
            Snapshot::from(bytes.into_boxed_slice())
        })
        .collect();
    (Store { images }, entry)
}

/// Reads one module's resolution slice, and nothing else.
///
/// Held only while that module is verified. What this bounds is the term the
/// attribution found accumulating across the closure; what it does **not**
/// bound is a single module that imports everything, whose slice is the whole
/// closure's export surface. That residue is stated in the evidence rather than
/// hidden by a fixture that never builds one.
fn resolution_slice(directory: &str, position: usize) -> ResolutionSnapshot {
    let text = std::fs::read_to_string(format!("{directory}/resolution.{position:03}"))
        .expect("a resolution slice");
    let mut slice = ResolutionSnapshot::default();
    for line in text.lines() {
        let mut parts = line.split(' ');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("module"), Some(name), Some(content_id)) => {
                slice
                    .modules
                    .insert(name.to_string(), content_id.to_string());
            }
            (Some("export"), Some(name), Some(export)) => {
                slice
                    .exports
                    .entry(name.to_string())
                    .or_default()
                    .insert(export.to_string());
            }
            _ => {}
        }
    }
    slice
}

fn argument(name: &str, fallback: usize) -> usize {
    std::env::args()
        .skip_while(|value| value != name)
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn directory() -> String {
    std::env::args()
        .skip_while(|value| value != "--dir")
        .nth(1)
        .unwrap_or_else(|| "target/residency".to_string())
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mode = arguments.get(1).map(String::as_str).unwrap_or("--launch");
    println!("TOS bounded verified-module residency — prototype, measurement only");
    println!("images: TOSIMGx0 v0 (experimental, partial coverage; ADR-0070 §7 gate stands)");
    println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
    println!();
    match mode {
        "--prepare" => prepare_mode(argument("--modules", 8), &directory()),
        "--launch" => launch_mode(&directory()),
        "--attribute" => attribute_mode(&directory()),
        "--manifest-bound" => manifest_bound_mode(),
        "--sizes" => sizes_mode(argument("--modules", 8)),
        "--residency" => residency_mode(argument("--modules", 8), argument("--bound-count", 2)),
        "--adversarial" => adversarial_mode(argument("--repeats", 8)),
        "--eviction" => eviction_mode(),
        "--negatives" => negatives_mode(),
        other => {
            eprintln!("unknown mode: {other}");
            std::process::exit(2);
        }
    }
}

/// A closure of `modules` total, entry included, each dependency called once.
fn sweep(modules: usize) -> (usize, Vec<usize>) {
    let dependencies = modules.saturating_sub(1).max(1);
    (dependencies, (1..=dependencies).collect())
}

fn launched(prepared: &Prepared, limits: &Limits) -> (closure::Launched, usize, usize) {
    let before = frontier();
    let started = Instant::now();
    let result = closure::launch(
        &prepared.store.images,
        &prepared.resolver(),
        limits,
        prepared.entry,
        "main",
        committed,
        frontier,
    )
    .expect("the fixture's closure verifies");
    let elapsed = started.elapsed().as_micros() as usize;
    (result, before, elapsed)
}

/// **1. Launch peak.** The claim is that sequential verification makes the peak
/// one module's working set rather than the closure's.
fn launch_mode(directory: &str) {
    let (store, entry) = read_prepared(directory);
    let modules = store.images.len();
    let limits = Limits::default();
    let store_bytes = store.bytes();
    let baseline = frontier();
    println!("== launch, closure of {modules} modules ==");
    println!("read from {directory} in a process the frontend never ran in, so the");
    println!("frontier below is this launch's own high-water mark and no earlier phase's");
    println!(
        "images {} B ({:.2} MiB) in the store, outside the grant account",
        store_bytes,
        mib(store_bytes)
    );
    println!(
        "arena before launch: committed {} B, frontier {} B ({:.2} MiB)",
        committed(),
        baseline,
        mib(baseline)
    );

    let started = Instant::now();
    let result = closure::launch(
        &store.images,
        &|position| resolution_slice(directory, position),
        &limits,
        entry,
        "main",
        committed,
        frontier,
    )
    .expect("the fixture's closure verifies");
    let elapsed = started.elapsed().as_micros() as usize;

    println!();
    println!(
        "launch peak frontier            {:>12} B ({:>7.2} MiB)",
        result.peak,
        mib(result.peak)
    );
    println!(
        "  of which the store            {:>12} B ({:>7.2} MiB)  [machine ledger]",
        store_bytes,
        mib(store_bytes)
    );
    println!(
        "  above the store               {:>12} B ({:>7.2} MiB)  [grant ledger]",
        result.peak.saturating_sub(store_bytes),
        mib(result.peak.saturating_sub(store_bytes))
    );
    println!(
        "largest single module in flight {:>12} B ({:>7.2} MiB)",
        result.largest_module,
        mib(result.largest_module)
    );
    println!(
        "records retained {} x {} B = {} B ({:.2} KiB)",
        result.records.len(),
        core::mem::size_of::<VerifiedModuleRecord>(),
        result.records.len() * core::mem::size_of::<VerifiedModuleRecord>(),
        kib(result.records.len() * core::mem::size_of::<VerifiedModuleRecord>())
    );
    println!(
        "manifest {} links, {} B ({:.2} KiB)",
        result.manifest.links(),
        result.manifest.heap_bytes(),
        kib(result.manifest.heap_bytes())
    );
    println!(
        "launch-time scaffolding released: {} B ({:.2} KiB)",
        result.scaffolding_released,
        kib(result.scaffolding_released)
    );
    println!("launch time {:.2} ms", elapsed as f64 / 1000.0);
    println!(
        "arena after launch: committed {} B ({:.2} MiB)",
        committed(),
        mib(committed())
    );
    println!();
    println!(
        "MODULES {} PEAK {} STORE {} ABOVE {} LARGEST {} RECORDS {} MANIFEST {} US {}",
        modules,
        result.peak,
        store_bytes,
        result.peak.saturating_sub(store_bytes),
        result.largest_module,
        result.records.len() * core::mem::size_of::<VerifiedModuleRecord>(),
        result.manifest.heap_bytes(),
        elapsed
    );
}

/// Where launch's frontier actually goes, by owner.
///
/// ADR-0069 sizes a grant from the **frontier**, so a flat live working set is
/// not by itself a flat launch bound. This walks the same launch and reads the
/// arena at every phase boundary, because "which owner" is invisible from a
/// peak.
fn attribute_mode(directory: &str) {
    let base_committed = committed();
    let (store, entry) = read_prepared(directory);
    let modules = store.images.len();
    let limits = Limits::default();
    let store_bytes = store.bytes();
    let snapshot_bytes = committed()
        .saturating_sub(base_committed)
        .saturating_sub(store_bytes);

    println!("== launch frontier attribution, closure of {modules} modules ==");
    println!(
        "images {} B ({:.2} MiB); declared resolution ~{} B ({:.2} MiB)",
        store_bytes,
        mib(store_bytes),
        snapshot_bytes,
        mib(snapshot_bytes)
    );
    println!("  the resolution snapshot is launch **input**, required by the verifier for");
    println!("  every module (docs/43 §5). It is closure-scaled and it is not this harness's");
    println!("  to change: `tos_verifier::verify` takes it by reference.");
    println!();

    let result = closure::launch(
        &store.images,
        &|position| resolution_slice(directory, position),
        &limits,
        entry,
        "main",
        committed,
        frontier,
    )
    .expect("the fixture's closure verifies");

    let base = result.marks[0];
    println!(
        "{:<22} {:>4}  {:>13}  {:>13}  {:>13}  {:>13}",
        "phase", "mod", "committed", "d committed", "frontier", "d frontier"
    );
    let mut previous = base;
    for mark in &result.marks {
        println!(
            "{:<22} {:>4}  {:>13}  {:>+13}  {:>13}  {:>+13}",
            mark.label,
            mark.module
                .map(|at| at.to_string())
                .unwrap_or_else(|| "-".to_string()),
            mark.committed,
            mark.committed as i64 - previous.committed as i64,
            mark.frontier,
            mark.frontier as i64 - previous.frontier as i64
        );
        previous = *mark;
    }

    let releases: Vec<&closure::Mark> = result
        .marks
        .iter()
        .filter(|mark| mark.label == "module released")
        .collect();
    let first_release = releases.first().expect("at least one module");
    let last_release = releases.last().expect("at least one module");
    let temporary_growth = last_release.committed as i64 - first_release.committed as i64;
    let frontier_growth = last_release.frontier as i64 - first_release.frontier as i64;

    let decoded: i64 = result
        .marks
        .windows(2)
        .filter(|pair| pair[1].label == "decoded Module")
        .map(|pair| pair[1].frontier as i64 - pair[0].frontier as i64)
        .max()
        .unwrap_or(0);
    let workspace: i64 = result
        .marks
        .windows(2)
        .filter(|pair| pair[1].label == "verifier workspace")
        .map(|pair| pair[1].frontier as i64 - pair[0].frontier as i64)
        .max()
        .unwrap_or(0);
    let exports_total: i64 = result
        .marks
        .windows(2)
        .filter(|pair| pair[1].label == "export table")
        .map(|pair| pair[1].committed as i64 - pair[0].committed as i64)
        .sum();
    let pending_total: i64 = result
        .marks
        .windows(2)
        .filter(|pair| pair[1].label == "pending links")
        .map(|pair| pair[1].committed as i64 - pair[0].committed as i64)
        .sum();
    let records_total = modules * core::mem::size_of::<VerifiedModuleRecord>();

    println!();
    println!("== owners ==");
    println!(
        "  one-module scratch, worst decode        {:>12} B ({:>7.2} MiB)",
        decoded,
        decoded as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  one-module scratch, verifier workspace  {:>12} B ({:>7.2} MiB)",
        workspace,
        workspace as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  closure-wide: export lookup tables      {:>12} B ({:>7.2} MiB)",
        exports_total,
        exports_total as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  closure-wide: pending links             {:>12} B ({:>7.2} KiB)",
        pending_total,
        pending_total as f64 / 1024.0
    );
    println!(
        "  closure-wide: records                   {:>12} B ({:>7.2} KiB)",
        records_total,
        kib(records_total)
    );
    println!(
        "  closure-wide: declared resolution       {:>12} B ({:>7.2} MiB)  [verifier input]",
        snapshot_bytes,
        mib(snapshot_bytes)
    );
    println!();
    println!(
        "  live temporary state accumulated across the closure: {temporary_growth:+} B ({:.2} MiB)",
        temporary_growth as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  frontier carried over the same span:                 {frontier_growth:+} B ({:.2} MiB)",
        frontier_growth as f64 / (1024.0 * 1024.0)
    );
    println!();
    println!(
        "ATTRIB MODULES {modules} STORE {store_bytes} SNAPSHOT {snapshot_bytes} DECODE {decoded} WORKSPACE {workspace} EXPORTS {exports_total} PENDING {pending_total} RECORDS {records_total} TEMP_GROWTH {temporary_growth} FRONTIER_GROWTH {frontier_growth} PEAK {}",
        result.peak
    );
}

/// The real upper bound on a `VerifiedClosureManifest`, from the accepted V1
/// ceilings rather than from a fixture.
///
/// The fixture's one-link-per-dependency shape is a property of the fixture. A
/// conforming closure may pack cross-module call sites as densely as its source
/// allows, and the manifest holds one link per site — so the bound is set by how
/// many call sites fit in a conforming source unit, times the closure ceiling.
///
/// docs/44 §2 bounds "IR tables/blocks/instructions" by the **declared module
/// resource envelope**, whose fields are `u128` and self-declared, so the IR
/// side supplies no finite bound of its own. What does bound it is the source
/// unit: every call site must be written down.
fn manifest_bound_mode() {
    println!("== the manifest's upper bound, derived ==");
    println!();
    println!("accepted V1 ceilings in play (docs/44 §2):");
    println!(
        "  normalized source unit      {} B (256 KiB)",
        tos_core::MAX_SOURCE_BYTES
    );
    println!("  module dependency closure   {CLOSURE_CEILING} modules");
    println!("  IR tables/blocks/instructions   bounded by the declared resource envelope");
    println!("                                  — u128 fields, self-declared, no finite bound");
    println!();

    // The densest packing of cross-module call sites the reference frontend
    // accepts inside one conforming source unit. Measured, not assumed: the
    // shortest text for one call decides the number, and guessing it would be
    // guessing the answer.
    let callee = "module set.a version 1.0 profile bootstrap; \
         resource [fuel: 100, stack: 1KiB, allocation: 1KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 1, recursion: 1, imports: 0] \
         pub fn v() -> i32 { return 1i32; } "
        .to_string();
    let head = "module set.d version 1.0 profile bootstrap; import set.a as a; \
         resource [fuel: 100000000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 1, recursion: 1, imports: 1] \
         pub fn f() -> i32 { return a.v()";
    let _ = head;
    // Call sites are spread across functions rather than packed into one
    // expression. That began as a way around a stack overflow — a single
    // 32 738-term sum, inside every published limit, used to abort the
    // frontend — and the defect is fixed: `crate::walk` in `tos-core` made
    // every walk over an operator run iterative. It stays because it is also
    // the densest packing measured: chunked functions reach 8.4 bytes per call
    // site, and one enormous expression wastes more on its tail than it saves.
    let chunk = argument("--chunk", 512);
    let mut dense = String::from(
        "module set.d version 1.0 profile bootstrap; import set.a as a; \
         resource [fuel: 100000000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 1, recursion: 1, imports: 1] ",
    );
    let mut sites = 0usize;
    let mut function = 0usize;
    loop {
        let mut body = format!("pub fn f{function}() -> i32 {{ return a.v()");
        for _ in 1..chunk {
            body.push_str(" + a.v()");
        }
        body.push_str("; } ");
        if dense.len() + body.len() > tos_core::MAX_SOURCE_BYTES {
            break;
        }
        dense.push_str(&body);
        sites += chunk;
        function += 1;
    }
    println!(
        "densest conforming caller: {} B of source, {} written call sites",
        dense.len(),
        sites
    );

    let callee_source = SourceReader::read(callee.as_bytes()).expect("valid");
    let dense_source = SourceReader::read(dense.as_bytes()).expect("valid");
    let callee_schema = Parser::parse_schema(&callee_source)
        .into_accepted()
        .expect("the callee parses");
    let callee_module = lower_module_in_set(
        &callee_source,
        &callee_schema,
        &ModuleContext {
            source_set: "bound".to_string(),
            path: "set/a.tos".to_string(),
            content_id: tos_pipeline::content_id(callee_source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
        &[],
    )
    .expect("the callee lowers");
    let dense_schema = match Parser::parse_schema(&dense_source).into_accepted() {
        Some(schema) => schema,
        None => {
            println!();
            println!("the frontend refused the densest caller; the bound below is the");
            println!("source-arithmetic one and is not confirmed by lowering");
            report_manifest_bound(sites);
            return;
        }
    };
    let lowered = lower_module_in_set(
        &dense_source,
        &dense_schema,
        &ModuleContext {
            source_set: "bound".to_string(),
            path: "set/d.tos".to_string(),
            content_id: tos_pipeline::content_id(dense_source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
        &[ResolvedImport {
            name: "set.a",
            module: &callee_module,
        }],
    );
    match lowered {
        Ok(module) => {
            let imported: usize = module
                .functions
                .iter()
                .flat_map(|function| function.blocks.iter())
                .flat_map(|block| block.instructions.iter())
                .filter(|instruction| {
                    matches!(
                        &instruction.op,
                        tos_ir::Op::Call {
                            target: tos_ir::CallTarget::Imported { .. },
                            ..
                        }
                    )
                })
                .count();
            let instructions: usize = module
                .functions
                .iter()
                .flat_map(|function| function.blocks.iter())
                .map(|block| block.instructions.len())
                .sum();
            println!(
                "lowered: {imported} cross-module call sites, {instructions} instructions, \
                 {} source-map entries",
                module.source_map.len()
            );
            let published = Limits::default();
            println!(
                "reference profile: instructions/block {}, blocks/function {}, source-map entries {}",
                published.instructions_per_block,
                published.blocks_per_function,
                published.source_map_entries
            );
            report_manifest_bound(imported);
        }
        Err(_) => {
            println!();
            println!("the densest caller did not lower under the reference limits; the");
            println!("source-arithmetic bound below stands as the ceiling either way");
            report_manifest_bound(sites);
        }
    }
}

fn report_manifest_bound(links_per_module: usize) {
    let link = core::mem::size_of::<closure::Link>();
    let total = links_per_module * CLOSURE_CEILING;
    let bytes = total * link;
    println!();
    println!("== the bound ==");
    println!("  cross-module call sites in one conforming module   {links_per_module}");
    println!("  x {CLOSURE_CEILING} modules                                        {total}");
    println!(
        "  x size_of::<Link>() = {link} B                          {bytes} B ({:.0} MiB)",
        mib(bytes)
    );
    println!();
    println!("  ADR-0040 whole-machine budget                      268435456 B (256 MiB)");
    if bytes > 256 * 1024 * 1024 {
        println!();
        println!("  THE BOUND EXCEEDS THE WHOLE MACHINE. A conforming closure may require a");
        println!("  manifest larger than the reference platform's entire memory, so ADR-0071 §2");
        println!("  cannot be accepted as written without a decision about which contract gives.");
        println!("  No lower cap is chosen here: docs/44 §2 permits one only in a declared");
        println!("  conformance profile, and choosing a conformance profile by its memory bill");
        println!("  is a Level-2 decision, not a measurement.");
    } else {
        println!();
        println!("  The bound fits the budget and can be recorded as ADR-0071 §2's structural");
        println!("  limit.");
    }
    println!();
    println!("MANIFEST_BOUND LINKS_PER_MODULE {links_per_module} TOTAL {total} LINK {link} BYTES {bytes}");
}

/// **2. Record and manifest size**, reported apart from each other and
/// extrapolated to the declared 256-module ceiling.
fn sizes_mode(modules: usize) {
    let (dependencies, pattern) = sweep(modules);
    let prepared = prepare(dependencies, &pattern, SOURCE_CEILING);
    let limits = Limits::default();
    let (result, _, _) = launched(&prepared, &limits);

    let record = core::mem::size_of::<VerifiedModuleRecord>();
    let links = result.manifest.links();
    let manifest = result.manifest.heap_bytes();
    let per_module_links = links as f64 / (dependencies + 1) as f64;

    println!("== what survives a module, and what survives a closure ==");
    println!();
    println!("VerifiedModuleRecord: {record} B, fixed size, no heap");
    println!("  it holds seven 32-byte digests, two bounded names, a profile and");
    println!("  ten envelope limits — every field an identity or a bound, none of");
    println!("  them a list. The size is a constant of the design, not of the module.");
    println!();
    println!(
        "measured closure of {} modules: {} links, manifest {} B",
        dependencies + 1,
        links,
        manifest
    );
    println!("  links are integers only: (caller, function, block, instruction) ->");
    println!("  (callee ClosureModuleId, function index). No names, so nothing has to");
    println!("  stay alive to resolve one.");
    println!();
    let projected_records = CLOSURE_CEILING * record;
    // The fixture's entry calls every dependency once, so cross-module call
    // sites scale with the closure. Stated as an extrapolation, which is what
    // it is.
    let projected_links = (per_module_links * CLOSURE_CEILING as f64) as usize;
    let projected_manifest = core::mem::size_of::<closure::VerifiedClosureManifest>()
        + projected_links * core::mem::size_of::<closure::Link>();
    println!("at the declared {CLOSURE_CEILING}-module ceiling, extrapolated:");
    println!(
        "  records  {CLOSURE_CEILING} x {record} B = {} B ({:.1} KiB)",
        projected_records,
        kib(projected_records)
    );
    println!(
        "  manifest ~{} links = {} B ({:.1} KiB)",
        projected_links,
        projected_manifest,
        kib(projected_manifest)
    );
    println!(
        "  together {} B ({:.2} MiB) against a live closure of {:.1} GiB",
        projected_records + projected_manifest,
        mib(projected_records + projected_manifest),
        CLOSURE_CEILING as f64 * 12.52 / 1024.0
    );
    println!();
    println!(
        "RECORD {record} LINKS {links} MANIFEST {manifest} PROJ_RECORDS {projected_records} PROJ_MANIFEST {projected_manifest}"
    );
}

fn report_run(
    label: &str,
    prepared: &Prepared,
    bound_count: usize,
    bound_bytes: usize,
) -> engine::Traffic {
    let limits = Limits::default();
    let (result, _, _) = launched(prepared, &limits);
    let store_bytes = prepared.store.bytes();

    let mut set = ResidentSet::new(bound_count, bound_bytes, committed);
    let started = Instant::now();
    let answer = engine::run(
        &result.manifest,
        &result.records,
        &prepared.store,
        &limits,
        &mut set,
    )
    .expect("the workload runs");
    let elapsed = started.elapsed();
    assert_eq!(
        answer, prepared.expected,
        "the workload must produce its answer, or the residency figures describe a run that did not happen"
    );

    let ledger = set.ledger();
    let traffic = set.traffic;
    println!("== {label} ==");
    println!(
        "bounds: {bound_count} resident module(s), {} B ({:.2} MiB) of module-derived state",
        bound_bytes,
        mib(bound_bytes)
    );
    println!("answer {answer} (expected {})", prepared.expected);
    println!();
    println!("steady-state ledger, ADR-0071 §7's three components:");
    println!(
        "  image bytes                {:>12} B ({:>7.2} MiB)",
        ledger.image_bytes,
        mib(ledger.image_bytes)
    );
    println!(
        "  decoded / view / index     {:>12} B ({:>7.2} MiB)",
        ledger.decoded_bytes,
        mib(ledger.decoded_bytes)
    );
    println!(
        "  bookkeeping                {:>12} B ({:>7.2} KiB)",
        ledger.bookkeeping_bytes,
        kib(ledger.bookkeeping_bytes)
    );
    println!(
        "  total                      {:>12} B ({:>7.2} MiB)   peak {} B ({:.2} MiB)",
        ledger.total(),
        mib(ledger.total()),
        traffic.peak_ledger,
        mib(traffic.peak_ledger)
    );
    println!();
    println!("ADR-0071 §8, the two ledgers:");
    println!(
        "  process grant     {:>12} B ({:>7.2} MiB)  decoded state, records, manifest, frames, bookkeeping",
        committed().saturating_sub(store_bytes),
        mib(committed().saturating_sub(store_bytes))
    );
    println!(
        "  machine residency {:>12} B ({:>7.2} MiB)  the store; {} B of it resident and shared, not copied",
        store_bytes,
        mib(store_bytes),
        ledger.image_bytes
    );
    println!();
    println!(
        "traffic: {} loads, {} evictions ({} of a suspended module), {} reloads of one",
        traffic.loads,
        traffic.evictions,
        traffic.evictions_while_suspended,
        traffic.reloads_of_suspended
    );
    println!(
        "         {} calls, {} returns, {} instructions, {} hashes over {} B ({:.2} MiB)",
        traffic.calls,
        traffic.returns,
        traffic.instructions,
        traffic.hashes,
        traffic.bytes_hashed,
        mib(traffic.bytes_hashed)
    );
    println!("         {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    traffic
}

/// **3. Steady-state residency** at a bound.
fn residency_mode(modules: usize, bound_count: usize) {
    let (dependencies, pattern) = sweep(modules);
    let prepared = prepare(dependencies, &pattern, SOURCE_CEILING);
    let bound_bytes = argument("--bound-bytes", 64 * 1024 * 1024);
    let traffic = report_run(
        &format!(
            "residency, {} modules at bound {bound_count}",
            dependencies + 1
        ),
        &prepared,
        bound_count,
        bound_bytes,
    );
    println!();
    println!(
        "MODULES {} BOUND {} LOADS {} EVICTIONS {} PEAK_LEDGER {}",
        dependencies + 1,
        bound_count,
        traffic.loads,
        traffic.evictions,
        traffic.peak_ledger
    );
}

/// **4. The adversarial case.** Two modules called alternately with room for
/// one, so every crossing is a miss. The worst case measured rather than
/// avoided.
fn adversarial_mode(repeats: usize) {
    let mut pattern = Vec::with_capacity(repeats * 2);
    for _ in 0..repeats {
        pattern.push(1);
        pattern.push(2);
    }
    let prepared = prepare(2, &pattern, SOURCE_CEILING);
    let traffic = report_run(
        &format!("adversarial A<->B, {repeats} alternations, bound = 1"),
        &prepared,
        1,
        64 * 1024 * 1024,
    );
    println!();
    println!("the entry is a module too, so at a bound of one every call evicts the caller");
    println!(
        "and every return reloads it: {} calls produced {} loads",
        traffic.calls, traffic.loads
    );
    assert!(
        traffic.evictions_while_suspended > 0,
        "at a bound of one the caller must have been evicted while suspended"
    );
    println!();
    println!(
        "ADVERSARIAL REPEATS {repeats} LOADS {} EVICTIONS {} SUSPENDED_EVICTIONS {} RELOADS {} HASHED {}",
        traffic.loads,
        traffic.evictions,
        traffic.evictions_while_suspended,
        traffic.reloads_of_suspended,
        traffic.bytes_hashed
    );
}

/// **5. Eviction under suspension.** A caller suspended inside a call, its own
/// module evicted, reloaded, and the call returned into.
fn eviction_mode() {
    let prepared = prepare(1, &[1], SOURCE_CEILING);
    println!("== a suspended caller, evicted and returned into ==");
    println!("closure: entry + one dependency; bound = 1 resident module");
    println!("so entering the dependency must evict the entry, which is suspended in the call");
    println!();
    let traffic = report_run("eviction under suspension", &prepared, 1, 64 * 1024 * 1024);
    println!();
    assert!(
        traffic.evictions_while_suspended >= 1,
        "the caller's module must have been evicted while it was suspended"
    );
    assert!(
        traffic.reloads_of_suspended >= 1,
        "the caller's module must have been reloaded to return into"
    );
    println!("PASS: the caller was evicted while suspended, reloaded, and returned into.");
    println!("Nothing in a frame pointed into the image — the continuation names a");
    println!("ClosureModuleId, a function index, a block index and an instruction index,");
    println!("and its own values. That is what made the eviction survivable.");
    println!();
    println!(
        "EVICTION SUSPENDED {} RELOADS {}",
        traffic.evictions_while_suspended, traffic.reloads_of_suspended
    );
}

/// **6 and 7. Negatives.** Every one of them fails the execution, naming the
/// identity and the check.
fn negatives_mode() {
    let prepared = prepare(2, &[1, 2, 1, 2], SOURCE_CEILING);
    let limits = Limits::default();
    let (result, _, _) = launched(&prepared, &limits);
    let records = result.records.clone();
    let manifest = &result.manifest;

    let mut failures = 0usize;
    let mut cases = 0usize;

    let mut check = |what: &str, outcome: Result<i128, Failure>, expected: fn(&Failure) -> bool| {
        cases += 1;
        match outcome {
            Ok(answer) => {
                failures += 1;
                println!("  {what:<38} ACCEPTED, answered {answer} — must not have");
            }
            Err(failure) => {
                let right = expected(&failure);
                if !right {
                    failures += 1;
                }
                println!(
                    "  {what:<38} refused: {failure:?}{}",
                    if right { "" } else { "  — WRONG REASON" }
                );
            }
        }
    };

    let run_with = |provider: &dyn Provider| {
        let mut set = ResidentSet::new(1, 64 * 1024 * 1024, committed);
        engine::run(manifest, &records, provider, &limits, &mut set)
    };

    println!("== a provider that fails ==");

    check(
        "missing: no image for a module",
        run_with(&Missing {
            inner: &prepared.store,
            absent: 0,
        }),
        |failure| matches!(failure, Failure::Missing(0)),
    );

    // Stale: the same module, re-encoded after a byte was changed and resealed.
    // A perfectly well-formed image that is not the one the record commits to.
    let mut stale = prepared.store.images[0].to_vec();
    let at = image::FRAME_HEADER + stale.len() / 2;
    stale[at] ^= 0x01;
    image::reseal(&mut stale);
    check(
        "stale: a different, valid image",
        run_with(&Swapped {
            inner: &prepared.store,
            at: 0,
            instead: Snapshot::from(stale.into_boxed_slice()),
        }),
        |failure| matches!(failure, Failure::ArtifactDigest { module: 0 }),
    );

    check(
        "substituted: another module's image",
        run_with(&Swapped {
            inner: &prepared.store,
            at: 0,
            instead: prepared.store.images[1].clone(),
        }),
        |failure| matches!(failure, Failure::ArtifactDigest { module: 0 }),
    );

    let truncated = prepared.store.images[0][..prepared.store.images[0].len() / 2].to_vec();
    check(
        "truncated image",
        run_with(&Swapped {
            inner: &prepared.store,
            at: 0,
            instead: Snapshot::from(truncated.into_boxed_slice()),
        }),
        |failure| matches!(failure, Failure::ArtifactDigest { module: 0 }),
    );

    // A provider that answers differently every time. The loader requests once
    // and hashes and parses that one snapshot, so there is no window between
    // the check and the use — the first wrong answer is simply refused.
    check(
        "shifting: different bytes per request",
        run_with(&Shifting {
            inner: &prepared.store,
            at: 0,
            alternatives: vec![
                prepared.store.images[1].clone(),
                prepared.store.images[0].clone(),
            ],
            counter: std::cell::Cell::new(0),
        }),
        |failure| matches!(failure, Failure::ArtifactDigest { module: 0 }),
    );

    println!();
    println!("== a cache that lies about having been verified (ADR-0071 §10) ==");
    let mut forged = records[0];
    forged.artifact_digest = tos_hash::sha256(&prepared.store.images[1]);
    check(
        "forged record beside a substituted image",
        run_with(&Poisoned {
            inner: Swapped {
                inner: &prepared.store,
                at: 0,
                instead: prepared.store.images[1].clone(),
            },
            forged,
        }),
        |failure| matches!(failure, Failure::ArtifactDigest { module: 0 }),
    );
    println!("  the forged record was never consulted, and could not have been:");
    println!("  `Provider` has one method and it returns bytes. There is no call by");
    println!("  which a record could be obtained, so the rule is not one this harness");
    println!("  follows — it is a sentence it cannot say.");

    println!();
    println!("== widening (ADR-0071 §3) ==");
    println!("  a request for a module outside the closure has no test here because it");
    println!("  has no representation: `ClosureModuleId` is minted only by the trusted");
    println!("  manifest, its field is private, and the type exposes no constructor.");
    println!(
        "  The manifest of this closure mints {} identities and no more.",
        manifest.modules()
    );
    assert!(
        manifest.module(manifest.modules()).is_none(),
        "the manifest must not mint an identity past the closure"
    );
    println!("  minting one past the closure returns None.");

    println!();
    if failures == 0 {
        println!(
            "PASS: {cases} negative cases, each refusing with the identity and the check named"
        );
    } else {
        println!("FAIL: {failures} of {cases} negative cases did not behave");
        std::process::exit(1);
    }
}
