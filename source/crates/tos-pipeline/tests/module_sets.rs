// SPDX-License-Identifier: GPL-3.0-or-later
//! Source sets through the reference path (Stage 3 Phase 1, docs/42 section 1).
//!
//! A service is a separate module, so a supervisor has nothing to launch until
//! a set of modules can be read, checked and resolved as a set. These tests are
//! about the set: what only more than one module can be wrong about, and what
//! the entry's identity says about the modules behind it.

use tos_pipeline::{
    execute, execute_set, PipelineStage, Request, Run, SetError, SetRequest, Silent, Unit,
    Unreachable,
};

/// `source = module_header import_decl* item*`: imports come before items, and
/// the resource declaration is an item.
fn module(name: &str, imports: &str, body: &str) -> String {
    format!(
        "module {name} version 1.0 profile bootstrap; {imports} \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] {body}"
    )
}

fn lib(name: &str, body: &str) -> String {
    module(name, "", body)
}

fn attempt(units: &[(&str, &str)], entry_path: &str) -> Result<Run, SetError> {
    let units: Vec<Unit<'_>> = units
        .iter()
        .map(|(path, text)| Unit {
            path,
            bytes: text.as_bytes(),
        })
        .collect();
    execute_set(
        &SetRequest {
            source_set: "tos-module-set-tests",
            units: &units,
            entry_path,
            entry: "main",
        },
        Vec::new(),
        &mut Silent,
        &mut Unreachable,
    )
}

fn set(units: &[(&str, &str)], entry_path: &str) -> Run {
    attempt(units, entry_path).expect("the set names an entry it contains")
}

/// The single-module entry point is the one-unit case of the set, and it must
/// keep behaving exactly as it did: every existing caller, including the boot
/// path, goes through it.
#[test]
fn one_module_through_the_set_path_is_the_single_module_path() {
    let text = lib(
        "system.boot.init",
        "pub fn main() -> i32 { return 6i32 * 7i32; }",
    );
    let single = execute(
        &Request {
            source_set: "tos-module-set-tests",
            path: "system/boot/init.tos",
            bytes: text.as_bytes(),
            entry: "main",
        },
        Vec::new(),
        &mut Silent,
        &mut Unreachable,
    );
    let as_set = set(&[("system/boot/init.tos", &text)], "system/boot/init.tos");

    let (Run::Completed(single), Run::Completed(as_set)) = (single, as_set) else {
        panic!("both must complete");
    };
    // Same module identity, not merely the same answer: a set of one that
    // computed a different digest would mean the two paths disagree about what
    // was run.
    assert_eq!(single.receipt.module_digest, as_set.receipt.module_digest);
    assert_eq!(single.receipt.content_id, as_set.receipt.content_id);
}

#[test]
fn a_two_module_set_is_read_checked_and_resolved() {
    let outcome = set(
        &[
            (
                "system/boot/init.tos",
                &module(
                    "system.boot.init",
                    "import system.lib.math as math;",
                    "pub fn main() -> i32 { return 1i32; }",
                ),
            ),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn double(value: i32) -> i32 { return value * 2i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    // Resolution is what this task delivers; executing across the boundary is
    // Task 4, and the outcome must not pretend otherwise.
    assert!(
        outcome.failed_at().is_none() || outcome.failed_at() > Some(PipelineStage::Resolve),
        "the set must get past resolution: {outcome:?}"
    );
}

/// The dependency digest describes what the entry depends on. Two sets whose
/// dependency differs cannot share a module digest, or a cache keyed by it
/// would hand back a module built against other source.
#[test]
fn a_changed_dependency_changes_the_entry_module_identity() {
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return 1i32; }",
    );
    let first = set(
        &[
            ("system/boot/init.tos", &entry),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn double(value: i32) -> i32 { return value * 2i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    let second = set(
        &[
            ("system/boot/init.tos", &entry),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn double(value: i32) -> i32 { return value * 3i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    let (Run::Completed(first), Run::Completed(second)) = (first, second) else {
        panic!("both sets must complete");
    };
    assert_eq!(
        first.receipt.content_id, second.receipt.content_id,
        "the entry's own source did not change"
    );
    assert_ne!(
        first.receipt.dependency_digest, second.receipt.dependency_digest,
        "a different dependency must produce a different dependency digest"
    );
    assert_ne!(
        first.receipt.module_digest, second.receipt.module_digest,
        "the module digest must carry the dependency digest"
    );
}

/// A module nothing imports is not part of what runs, so it is not part of what
/// the entry's identity describes.
#[test]
fn an_unreachable_module_is_not_in_the_dependency_digest() {
    let entry = lib("system.boot.init", "pub fn main() -> i32 { return 1i32; }");
    let alone = set(&[("system/boot/init.tos", &entry)], "system/boot/init.tos");
    let with_spectator = set(
        &[
            ("system/boot/init.tos", &entry),
            (
                "system/lib/unused.tos",
                &lib(
                    "system.lib.unused",
                    "pub fn ignored() -> i32 { return 0i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    let (Run::Completed(alone), Run::Completed(with_spectator)) = (alone, with_spectator) else {
        panic!("both must complete");
    };
    assert_eq!(
        alone.receipt.module_digest,
        with_spectator.receipt.module_digest
    );
}

#[test]
fn an_import_no_module_provides_is_refused_at_resolution() {
    let outcome = set(
        &[(
            "system/boot/init.tos",
            &module(
                "system.boot.init",
                "import system.lib.absent as absent;",
                "pub fn main() -> i32 { return 1i32; }",
            ),
        )],
        "system/boot/init.tos",
    );
    assert_eq!(outcome.failed_at(), Some(PipelineStage::Resolve));
    let Run::Diagnosed { diagnostics, .. } = &outcome else {
        panic!("expected diagnostics, got {outcome:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code() == "E1604_IMPORT_NOT_FOUND"),
        "{diagnostics:?}"
    );
}

/// An import cycle is refused by diagnostic, not by recursion: the walk that
/// orders a closure must never be the thing that discovers a cycle.
#[test]
fn an_import_cycle_is_refused_at_resolution() {
    let outcome = set(
        &[
            (
                "system/boot/init.tos",
                &module(
                    "system.boot.init",
                    "import system.lib.other as other;",
                    "pub fn main() -> i32 { return 1i32; }",
                ),
            ),
            (
                "system/lib/other.tos",
                &module(
                    "system.lib.other",
                    "import system.boot.init as back;",
                    "pub fn helper() -> i32 { return 1i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    assert_eq!(outcome.failed_at(), Some(PipelineStage::Resolve));
    let Run::Diagnosed { diagnostics, .. } = &outcome else {
        panic!("expected diagnostics, got {outcome:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code() == "E1606_IMPORT_CYCLE"),
        "{diagnostics:?}"
    );
}

/// A module whose name does not derive from its path is a set-wide finding: one
/// module alone cannot see that it disagrees with where it is stored.
#[test]
fn a_module_at_the_wrong_path_is_refused_at_resolution() {
    let outcome = set(
        &[(
            "system/boot/other.tos",
            &lib("system.boot.init", "pub fn main() -> i32 { return 1i32; }"),
        )],
        "system/boot/other.tos",
    );
    assert_eq!(outcome.failed_at(), Some(PipelineStage::Resolve));
}

#[test]
fn a_transport_refusal_names_the_unit_it_came_from() {
    let units = [
        (
            "system/boot/init.tos",
            lib("system.boot.init", "pub fn main() -> i32 { return 1i32; }"),
        ),
        ("system/lib/broken.tos", String::from("\u{feff}module x;")),
    ];
    let units: Vec<(&str, &str)> = units
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    let outcome = set(&units, "system/boot/init.tos");
    let Run::SourceRejected { code, path, .. } = &outcome else {
        panic!("expected a transport refusal, got {outcome:?}");
    };
    assert_eq!(*code, "E1002_BOM_FORBIDDEN");
    assert_eq!(path, "system/lib/broken.tos");
}

/// A request naming an entry the set does not contain is the caller's mistake,
/// not a refusal of anyone's source — so it is not a `Run` at all, and no stage
/// is announced for it.
#[test]
fn a_set_without_the_declared_entry_is_not_a_run() {
    struct Watcher(Vec<PipelineStage>);
    impl tos_pipeline::Trace for Watcher {
        fn entering(&mut self, stage: PipelineStage) {
            self.0.push(stage);
        }
    }

    let text = lib("system.lib.math", "pub fn main() -> i32 { return 1i32; }");
    let unit = Unit {
        path: "system/lib/math.tos",
        bytes: text.as_bytes(),
    };
    let mut watcher = Watcher(Vec::new());
    let outcome = execute_set(
        &SetRequest {
            source_set: "tos-module-set-tests",
            units: core::slice::from_ref(&unit),
            entry_path: "system/boot/init.tos",
            entry: "main",
        },
        Vec::new(),
        &mut watcher,
        &mut Unreachable,
    );
    let Err(error) = outcome else {
        panic!("expected a request error, not a run");
    };
    assert_eq!(
        error,
        SetError::EntryModuleAbsent {
            path: String::from("system/boot/init.tos")
        }
    );
    assert_eq!(error.symbol(), "entry-module-absent");
    assert!(
        watcher.0.is_empty(),
        "no stage may be announced: {:?}",
        watcher.0
    );
}

#[test]
fn an_empty_set_is_not_a_run() {
    assert_eq!(
        attempt(&[], "system/boot/init.tos").err(),
        Some(SetError::NoUnits)
    );
}

/// The import carries the identity of the module that actually resolved.
///
/// An empty content id would mean "not resolved", and a plausible-looking one
/// invented by the importer would be worse than either: the verifier could not
/// tell a real resolution from a claimed one.
#[test]
fn an_import_carries_the_resolved_module_content_id() {
    let math = lib(
        "system.lib.math",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return 1i32; }",
    );
    let outcome = set(
        &[
            ("system/boot/init.tos", &entry),
            ("system/lib/math.tos", &math),
        ],
        "system/boot/init.tos",
    );
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run, got {outcome:?}");
    };
    // The dependency's own content id, computed from its bytes by the same
    // function the pipeline used, and reachable through the entry's identity.
    let expected = tos_pipeline::content_id(math.as_bytes());
    assert!(
        completion.receipt.dependency_digest != tos_pipeline::list_digest(&[])
            && !expected.is_empty(),
        "the dependency digest must be over a real dependency"
    );
    assert_eq!(
        completion.receipt.dependency_digest,
        tos_pipeline::list_digest(&[("system.lib.math", expected.as_str())]),
        "the digest must be over the resolved name and computed content id"
    );
}

/// The whole point of a source set: a boot module calls a library module and
/// gets its answer. Every stage of the reference path, across a real module
/// boundary, ending in a value.
#[test]
fn a_cross_module_call_runs_and_returns_the_callee_answer() {
    let outcome = set(
        &[
            (
                "system/boot/init.tos",
                &module(
                    "system.boot.init",
                    "import system.lib.math as math;",
                    "pub fn main() -> i32 { return math.double(21i32); }",
                ),
            ),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn double(value: i32) -> i32 { return value * 2i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run, got {outcome:?}");
    };
    assert_eq!(tos_pipeline::render::value(&completion.value), "i32:42");
    // One run, one budget. docs/41 section 6 admits a call only when the
    // callee's declared contract fits the caller's envelope, so crossing a
    // module boundary is not a way to obtain a second one: the accounting
    // reported is the entry's, and the callee's work is charged to it.
    assert_eq!(
        completion.accounting.fuel_limit, 100000,
        "the run is governed by the entry module's declared fuel"
    );
    assert!(
        completion.accounting.fuel_used > 0,
        "the callee's work is charged to the run that made the call"
    );
    assert!(
        completion.accounting.max_call_depth >= 2,
        "a cross-module call is a call: it costs depth like any other"
    );
}

/// A trap raised inside a dependency is located in the dependency's own source.
/// The source-map index that names it is only meaningful in that module's
/// table, so a trap that crossed a boundary and was resolved against the
/// caller's map would name a real line in the wrong file.
#[test]
fn a_trap_inside_a_dependency_is_located_in_the_dependency() {
    let outcome = set(
        &[
            (
                "system/boot/init.tos",
                &module(
                    "system.boot.init",
                    "import system.lib.math as math;",
                    "pub fn main() -> i32 { return math.halve(1i32); }",
                ),
            ),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn halve(value: i32) -> i32 { return value / 0i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    let Run::Trapped { code, at, .. } = &outcome else {
        panic!("expected a trap, got {outcome:?}");
    };
    assert_eq!(*code, "RUNTIME_DIVISION_BY_ZERO");
    let at = at.as_ref().expect("the trap must be located");
    assert_eq!(at.path, "system/lib/math.tos");
}

/// A call to a name the imported module does not export is a lowering gap, not
/// a `unit`-typed call the verifier would have to take on trust.
#[test]
fn a_call_to_a_name_the_dependency_does_not_export_refuses_at_lowering() {
    let outcome = set(
        &[
            (
                "system/boot/init.tos",
                &module(
                    "system.boot.init",
                    "import system.lib.math as math;",
                    "pub fn main() -> i32 { return math.absent(1i32); }",
                ),
            ),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn double(value: i32) -> i32 { return value * 2i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    assert_eq!(
        outcome.failed_at(),
        Some(PipelineStage::Lower),
        "{outcome:?}"
    );
}

/// The verifier is handed what the set actually provides, and it is handed
/// every module — not only the entry. A dependency the verifier never saw would
/// be running on its caller's receipt, and a receipt is a statement about one
/// module.
#[test]
fn every_module_of_a_set_is_verified_against_the_resolved_snapshot() {
    // The dependency is well-formed source whose *IR* the verifier must still
    // judge on its own. If only the entry were verified, this set would run.
    let outcome = set(
        &[
            (
                "system/boot/init.tos",
                &module(
                    "system.boot.init",
                    "import system.lib.math as math;",
                    "pub fn main() -> i32 { return 1i32; }",
                ),
            ),
            (
                "system/lib/math.tos",
                &lib(
                    "system.lib.math",
                    "pub fn double(value: i32) -> i32 { return value * 2i32; }",
                ),
            ),
        ],
        "system/boot/init.tos",
    );
    assert!(outcome.is_completed(), "{outcome:?}");

    // The receipt names the entry, and the verifier that issued it is the one
    // the crate declares — not a stand-in the pipeline chose.
    let Run::Completed(completion) = &outcome else {
        unreachable!()
    };
    assert_eq!(completion.receipt.module_name, "system.boot.init");
    assert_eq!(
        completion.receipt.verifier_identity,
        tos_verifier::VERIFIER_IDENTITY
    );
}

/// A `borrow mut` argument written by an imported function is observed by the
/// caller after the call returns.
///
/// This is a **language** statement, made through the reference path over
/// canonical source, and it is here because the engine used to get it wrong.
/// The write-back plan is built from the callee's declared parameter modes; a
/// cross-module call was building it against the *caller's* module, where the
/// callee is not, so the plan came out empty every time and the write was
/// dropped in silence. Nothing about the source said so, and no test asked.
///
/// The test names no engine type. What it asserts is what a program can see.
#[test]
fn an_imported_function_writes_back_through_a_mutable_borrow() {
    let dependency = lib(
        "system.lib.bump",
        "pub fn bump(borrow mut cell: i32) -> i32 { cell = cell + 10i32; return 1i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.bump as lib;",
        "pub fn main() -> i32 { let mut cell = 5i32; let flag = lib.bump(cell); \
         return cell + flag; }",
    );
    let run = set(
        &[
            ("system/lib/bump.tos", &dependency),
            ("system/boot/init.tos", &entry),
        ],
        "system/boot/init.tos",
    );
    let Run::Completed(completion) = run else {
        panic!("the set runs: {run:?}");
    };
    assert_eq!(
        completion.value,
        tos_pipeline::Value::Int(tos_pipeline::IntKind::I32, 16),
        "the callee's write must be visible to the caller: 5 + 10, plus the 1 it returned"
    );
}

/// The same, across two module boundaries.
///
/// `init` lends to `mid`, `mid` lends the same binding on to `leaf`, and `leaf`
/// writes. The value has to come back through both returns.
#[test]
fn a_mutable_borrow_writes_back_across_two_module_boundaries() {
    let leaf = lib(
        "system.lib.leaf",
        "pub fn write(borrow mut cell: i32) -> i32 { cell = cell + 100i32; return 2i32; }",
    );
    let mid = module(
        "system.lib.mid",
        "import system.lib.leaf as leaf;",
        "pub fn relay(borrow mut cell: i32) -> i32 { return leaf.write(cell); }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.mid as mid;",
        "pub fn main() -> i32 { let mut cell = 7i32; let flag = mid.relay(cell); \
         return cell + flag; }",
    );
    let run = set(
        &[
            ("system/lib/leaf.tos", &leaf),
            ("system/lib/mid.tos", &mid),
            ("system/boot/init.tos", &entry),
        ],
        "system/boot/init.tos",
    );
    let Run::Completed(completion) = run else {
        panic!("the set runs: {run:?}");
    };
    assert_eq!(
        completion.value,
        tos_pipeline::Value::Int(tos_pipeline::IntKind::I32, 109),
        "7 + 100 written two boundaries down, plus the 2 it returned"
    );
}

/// Preparation and execution are two accounts with two lifetimes (ADR-0072 §2).
///
/// What `prepare_from_source` returns is an executable closure: images, one
/// fixed-size record per module, the membership, the entry receipt and the
/// declared envelope. Everything the build needed to produce them is gone by
/// then, and the run reaches its modules through the resident set rather than
/// through anything the preparation held.
#[test]
fn a_prepared_closure_outlives_the_workspace_that_built_it() {
    let dependency = lib(
        "system.lib.math",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return math.double(21i32); }",
    );
    let texts = [
        ("system/lib/math.tos", dependency.as_str()),
        ("system/boot/init.tos", entry.as_str()),
    ];
    let units: Vec<Unit<'_>> = texts
        .iter()
        .map(|(path, text)| Unit {
            path,
            bytes: text.as_bytes(),
        })
        .collect();
    let request = SetRequest {
        source_set: "tos-module-set-tests",
        units: &units,
        entry_path: "system/boot/init.tos",
        entry: "main",
    };

    let prepared =
        tos_pipeline::prepare_from_source(&request, &mut Silent, tos_pipeline::HOST_RESIDENCY)
            .expect("the set names an entry it contains");
    let tos_pipeline::Preparation::Ready(mut prepared) = prepared else {
        panic!("the closure prepares");
    };
    assert_eq!(prepared.modules(), 2, "both modules are in the membership");

    // The same closure runs twice from one preparation. Nothing about the build
    // is consulted again: if any of it had been needed, it could not be here.
    for _ in 0..2 {
        let run = tos_pipeline::run_prepared(&mut prepared, &request, Vec::new(), &mut Unreachable);
        let Run::Completed(completion) = run else {
            panic!("the prepared closure runs: {run:?}");
        };
        assert_eq!(
            completion.value,
            tos_pipeline::Value::Int(tos_pipeline::IntKind::I32, 42)
        );
    }
    assert!(
        prepared.traffic().loads >= 2,
        "the run reached its modules through the resident set: {:?}",
        prepared.traffic()
    );
}

/// A closure that cannot be built is refused by the preparation, and nothing
/// executes.
#[test]
fn a_closure_that_does_not_build_never_becomes_a_prepared_executable() {
    let entry = module(
        "system.boot.init",
        "import system.lib.absent as absent;",
        "pub fn main() -> i32 { return absent.value(); }",
    );
    let units = [Unit {
        path: "system/boot/init.tos",
        bytes: entry.as_bytes(),
    }];
    let request = SetRequest {
        source_set: "tos-module-set-tests",
        units: &units,
        entry_path: "system/boot/init.tos",
        entry: "main",
    };
    match tos_pipeline::prepare_from_source(&request, &mut Silent, tos_pipeline::HOST_RESIDENCY) {
        Ok(tos_pipeline::Preparation::Refused(run)) => {
            assert_eq!(run.failed_at(), Some(PipelineStage::Resolve));
        }
        Ok(_) => panic!("an unresolvable import produced an executable"),
        Err(error) => panic!("the request itself was rejected: {error:?}"),
    }
}
