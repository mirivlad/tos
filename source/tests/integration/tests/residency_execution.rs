// SPDX-License-Identifier: GPL-3.0-or-later
//! Execution under a resident set too small to hold the program.
//!
//! ADR-0071 section 6 says a continuation names identities and never addresses,
//! so the module a frame is suspended in may be evicted and reloaded while its
//! callee runs. Every case here makes that happen rather than asserting it: the
//! bound is **one resident module** and the closure has three, so a call across
//! a module boundary always evicts the caller, and returning into it always
//! reloads it.
//!
//! What is being checked is that a program cannot tell. The values, the
//! write-backs, the cleanup order and the trap sites are the same as they are
//! when nothing is evicted, and the residency traffic is checked too — otherwise
//! a test could pass because no eviction happened at all.

use tos_core::{
    lower_module_in_set, Checker, LoweredInterface, ModuleContext, Parser, ResolvedImport,
    SourceReader,
};
use tos_engine::{run_closure, Closure, Refusal, Trap, Unreachable, Value};
use tos_image::encode;
use tos_ir::{IntKind, Module};
use tos_residency::{
    launch, ClosureModuleId, ClosureSource, Failure, ImageSnapshot, Launched, ModuleProvider,
    Residency, ResidencyLimits,
};
use tos_verifier::{Limits, ResolutionSnapshot};

/// One module resident. Every cross-module call evicts the caller.
const ONE: ResidencyLimits = ResidencyLimits {
    modules: 1,
    bytes: 64 * 1024 * 1024,
};

/// Two resident, so a caller survives its callee.
const TWO: ResidencyLimits = ResidencyLimits {
    modules: 2,
    bytes: 64 * 1024 * 1024,
};

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 64KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 16, imports: 8]";

/// Lowers one module of a set against the modules it imports.
fn lowered(
    name: &str,
    path: &str,
    profile: &str,
    body: &str,
    imports: &[(&str, &LoweredInterface)],
) -> Module {
    let text = format!("module {name} version 1.0 profile {profile}; {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == tos_core::Severity::Error),
        "{name} checks clean: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let context = ModuleContext {
        source_set: String::from("tos-residency-execution"),
        path: String::from(path),
        content_id: content_id(source.bytes()),
        dependency_digest: content_id(b""),
        capability_interface_digest: content_id(b""),
    };
    let resolved: Vec<ResolvedImport<'_>> = imports
        .iter()
        .map(|(name, interface)| ResolvedImport { name, interface })
        .collect();
    lower_module_in_set(&source, &schema, &context, &resolved).expect("the fixture lowers")
}

/// The closure's images, in position order, with a way to lie about one of them.
struct Store {
    images: Vec<ImageSnapshot>,
    /// A position whose bytes the provider substitutes at run time, after the
    /// launch has already verified the real ones.
    substitute: Option<(usize, ImageSnapshot)>,
}

impl Store {
    fn of(modules: &[&Module]) -> Store {
        Store {
            images: modules
                .iter()
                .map(|module| {
                    let (bytes, _) = encode(module);
                    ImageSnapshot::from(bytes.into_boxed_slice())
                })
                .collect(),
            substitute: None,
        }
    }
}

impl ClosureSource for Store {
    fn count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, position: usize) -> Option<ImageSnapshot> {
        self.images.get(position).cloned()
    }
}

impl ModuleProvider for Store {
    fn image(&self, id: ClosureModuleId) -> Option<ImageSnapshot> {
        if let Some((at, instead)) = &self.substitute {
            if *at == id.position() {
                return Some(instead.clone());
            }
        }
        self.images.get(id.position()).cloned()
    }
}

/// A three-module closure: `leaf`, `mid` which calls it, and `init` which calls
/// `mid`. At a bound of one, a call from `init` to `mid` to `leaf` evicts twice
/// on the way down and reloads twice on the way back.
struct Chain {
    store: Store,
    launched: Launched,
}

fn chain(leaf_body: &str, mid_body: &str, init_body: &str) -> Chain {
    chain_in("bootstrap", leaf_body, mid_body, init_body)
}

/// The same, at a declared conformance profile. `defer` is a Full-profile
/// construct, so the cleanup cases declare it rather than being written around.
fn chain_in(profile: &str, leaf_body: &str, mid_body: &str, init_body: &str) -> Chain {
    let leaf = lowered(
        "set.leaf",
        "set/leaf.tos",
        profile,
        &format!("{ENVELOPE} {leaf_body}"),
        &[],
    );
    let mid = lowered(
        "set.mid",
        "set/mid.tos",
        profile,
        &format!("import set.leaf as leaf; {ENVELOPE} {mid_body}"),
        &[("set.leaf", &LoweredInterface::of(&leaf))],
    );
    let init = lowered(
        "set.init",
        "set/init.tos",
        profile,
        &format!("import set.mid as mid; {ENVELOPE} {init_body}"),
        &[("set.mid", &LoweredInterface::of(&mid))],
    );
    let store = Store::of(&[&leaf, &mid, &init]);
    let launched = launch(
        &store,
        &|_| ResolutionSnapshot::default(),
        &Limits::default(),
        2,
        "main",
    )
    .expect("the closure verifies");
    Chain { store, launched }
}

impl Chain {
    /// Runs the entry under a declared residency bound.
    fn run(
        &self,
        limits: ResidencyLimits,
    ) -> (
        Result<Result<tos_engine::Outcome, Trap>, Refusal>,
        tos_residency::Traffic,
    ) {
        let mut residency = Residency::new(limits, parse_limits()).expect("admissible bounds");
        let mut closure = Closure::new(
            &mut residency,
            &self.store,
            &self.launched.records,
            &self.launched.manifest,
        );
        let outcome = run_closure(&mut closure, Vec::new(), &mut Unreachable);
        let traffic = closure.traffic();
        (outcome, traffic)
    }

    fn value(&self, limits: ResidencyLimits) -> (Value, tos_residency::Traffic) {
        let (outcome, traffic) = self.run(limits);
        let value = outcome
            .expect("the entry is runnable")
            .expect("the run completes")
            .value;
        (value, traffic)
    }

    fn trap(&self, limits: ResidencyLimits) -> (Trap, tos_residency::Traffic) {
        let (outcome, traffic) = self.run(limits);
        let trap = outcome
            .expect("the entry is runnable")
            .expect_err("the run must trap");
        (trap, traffic)
    }
}

fn parse_limits() -> tos_image::ParseLimits {
    let limits = Limits::default();
    tos_image::ParseLimits {
        table_entries: limits.table_entries,
        modules: limits.modules,
        fields: limits.fields,
        parameters: limits.parameters,
        blocks_per_function: limits.blocks_per_function,
        instructions_per_block: limits.instructions_per_block,
        source_map_entries: limits.source_map_entries,
    }
}

/// Nested imported calls, two boundaries deep, at one resident module.
///
/// `init` is suspended in a call to `mid`, which is itself suspended in a call
/// to `leaf`. At a bound of one, neither suspended caller is resident while
/// `leaf` runs — both of them are frames whose module is gone — and both are
/// loaded again to be returned into.
#[test]
fn nested_imported_calls_run_with_one_module_resident() {
    let chain = chain(
        "pub fn value() -> i32 { return 7i32; }",
        "pub fn twice() -> i32 { return leaf.value() + leaf.value(); }",
        "pub fn main() -> i32 { return mid.twice() + 1i32; }",
    );
    let (one, traffic) = chain.value(ONE);
    assert_eq!(one, Value::Int(IntKind::I32, 15));
    assert!(
        traffic.evictions >= 4,
        "two boundaries, crossed and returned through twice: {traffic:?}"
    );

    let (two, roomy) = chain.value(TWO);
    assert_eq!(one, two, "the bound changes what is held, never the result");
    assert!(
        roomy.evictions < traffic.evictions,
        "more room, fewer evictions: {roomy:?}"
    );
}

/// A `borrow mut` argument written by a callee, with the caller evicted.
///
/// The write-back is a plan of index pairs computed before the call, carried by
/// the continuation, and applied to the caller's own values when it comes back.
/// None of that reaches into the caller's module, which is why it survives the
/// caller being released in between.
#[test]
fn a_mutable_borrow_writes_back_into_a_caller_that_was_evicted() {
    let chain = chain(
        "pub fn bump(borrow mut cell: i32) -> i32 { cell = cell + 10i32; return 1i32; }",
        "pub fn touch(borrow mut cell: i32) -> i32 { return leaf.bump(cell); }",
        "pub fn main() -> i32 { let mut cell = 5i32; let flag = mid.touch(cell); \
         return cell + flag; }",
    );
    let (value, traffic) = chain.value(ONE);
    assert_eq!(
        value,
        Value::Int(IntKind::I32, 16),
        "the callee's write reached the reloaded caller's slot"
    );
    assert!(traffic.evictions >= 2, "{traffic:?}");
    assert_eq!(chain.value(TWO).0, value);
}

/// A cleanup chain that crosses an eviction between its bodies.
///
/// ADR-0035 makes what one cleanup leaves visible to the next. Each body is a
/// call, so at a bound of one the scope's module is evicted and reloaded between
/// them — and the order and the visibility are unchanged.
#[test]
fn a_cleanup_chain_survives_eviction_between_its_bodies() {
    let chain = chain_in(
        "full",
        "pub fn add(value: i32, step: i32) -> i32 { return value + step; }",
        "pub fn step(value: i32, by: i32) -> i32 { return leaf.add(value, by); }",
        "pub fn main() -> i32 { let mut total = 1i32; \
         if (true) { defer { total = mid.step(total, 2i32); } \
         defer { total = mid.step(total, 30i32); } } return total; }",
    );
    let (value, traffic) = chain.value(ONE);
    // Reverse registration order: the second registered runs first.
    assert_eq!(value, Value::Int(IntKind::I32, 33));
    assert!(traffic.evictions >= 2, "{traffic:?}");
    assert_eq!(
        chain.value(TWO).0,
        value,
        "cleanup order is the language's, not the resident set's"
    );
}

/// A cleanup that traps, across an eviction.
#[test]
fn a_cleanup_that_traps_reports_the_same_trap_under_eviction() {
    let chain = chain_in(
        "full",
        "pub fn divide(value: i32, by: i32) -> i32 { return value / by; }",
        "pub fn step(value: i32, by: i32) -> i32 { return leaf.divide(value, by); }",
        "pub fn main() -> i32 { let mut total = 1i32; \
         if (true) { defer { total = mid.step(total, 0i32); } } return total; }",
    );
    let (one, traffic) = chain.trap(ONE);
    assert_eq!(one.code, "RUNTIME_DIVISION_BY_ZERO");
    assert!(traffic.evictions >= 1, "{traffic:?}");

    let (two, _) = chain.trap(TWO);
    assert_eq!(one.code, two.code);
    assert_eq!(
        one.site, two.site,
        "the trap names the same span whatever was resident"
    );
}

/// A trap inside an imported callee names its own module's span.
///
/// The unwind asks for each frame's module by the identity the frame holds, and
/// the module it needs was evicted two boundaries ago. Reaching it again is the
/// same operation as running an instruction in it.
#[test]
fn an_imported_trap_names_its_source_after_the_module_was_evicted() {
    let chain = chain(
        "pub fn burst() -> i32 { return 1i32 / 0i32; }",
        "pub fn reach() -> i32 { return leaf.burst(); }",
        "pub fn main() -> i32 { return mid.reach(); }",
    );
    let (one, traffic) = chain.trap(ONE);
    assert_eq!(one.code, "RUNTIME_DIVISION_BY_ZERO");
    let site = one
        .site
        .as_deref()
        .expect("a trap names where it came from");
    assert_eq!(
        site.path, "set/leaf.tos",
        "the span is the callee's own, not the caller's"
    );
    assert!(traffic.evictions >= 2, "{traffic:?}");

    let (two, _) = chain.trap(TWO);
    assert_eq!(one.site, two.site);
}

/// A provider that answers a reload with different bytes.
///
/// The launch verified the real image and the record commits to its digest.
/// What comes back at reload time is hashed before it is parsed, so a
/// substitution is refused by the digest and never reaches the parser — let
/// alone the engine.
#[test]
fn a_substituted_image_refuses_the_reload_rather_than_running_it() {
    let mut chain = chain(
        "pub fn value() -> i32 { return 7i32; }",
        "pub fn twice() -> i32 { return leaf.value() + leaf.value(); }",
        "pub fn main() -> i32 { return mid.twice(); }",
    );
    // `leaf` is position 0. Hand back `mid`'s image for it instead: well formed,
    // verified, and not this module.
    let instead = chain.store.images[1].clone();
    chain.store.substitute = Some((0, instead));

    let (trap, _) = chain.trap(ONE);
    assert_eq!(trap.code, "RUNTIME_MODULE_UNAVAILABLE");
    assert!(
        trap.detail.contains("not the one this launch verified"),
        "the refusal names the check that caught it: {trap:?}"
    );
}

/// The same, for bytes that are merely damaged.
#[test]
fn a_truncated_image_refuses_the_reload() {
    let mut chain = chain(
        "pub fn value() -> i32 { return 7i32; }",
        "pub fn twice() -> i32 { return leaf.value() + leaf.value(); }",
        "pub fn main() -> i32 { return mid.twice(); }",
    );
    let whole = chain.store.images[0].to_vec();
    let cut = whole[..whole.len() / 2].to_vec();
    chain.store.substitute = Some((0, ImageSnapshot::from(cut.into_boxed_slice())));

    let (trap, _) = chain.trap(ONE);
    assert_eq!(trap.code, "RUNTIME_MODULE_UNAVAILABLE");
}

/// A provider that has nothing when a reload asks.
#[test]
fn a_provider_with_nothing_refuses_the_reload() {
    let chain = chain(
        "pub fn value() -> i32 { return 7i32; }",
        "pub fn twice() -> i32 { return leaf.value() + leaf.value(); }",
        "pub fn main() -> i32 { return mid.twice(); }",
    );
    struct Absent;
    impl ModuleProvider for Absent {
        fn image(&self, _: ClosureModuleId) -> Option<ImageSnapshot> {
            None
        }
    }
    let mut residency = Residency::new(ONE, parse_limits()).expect("admissible bounds");
    let mut closure = Closure::new(
        &mut residency,
        &Absent,
        &chain.launched.records,
        &chain.launched.manifest,
    );
    match run_closure(&mut closure, Vec::new(), &mut Unreachable) {
        Err(Refusal::EntryNotResident(Failure::Missing(2))) => {}
        other => panic!("a run started without its entry module: {other:?}"),
    }
}

/// A bound too small for a single module refuses the run rather than thrashing.
#[test]
fn a_byte_bound_below_one_module_refuses_the_run() {
    let chain = chain(
        "pub fn value() -> i32 { return 7i32; }",
        "pub fn twice() -> i32 { return leaf.value() + leaf.value(); }",
        "pub fn main() -> i32 { return mid.twice(); }",
    );
    let (outcome, _) = chain.run(ResidencyLimits {
        modules: 4,
        bytes: 64,
    });
    match outcome {
        Err(Refusal::EntryNotResident(Failure::OverResidencyBound { module: 2, .. })) => {}
        other => panic!("an unsatisfiable bound was not refused: {other:?}"),
    }
}
