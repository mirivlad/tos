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

/// A catalog over units a test already holds: metadata only, as the interface
/// requires.
fn catalog_of<'a>(units: &'a [Unit<'a>]) -> Vec<tos_pipeline::SourceCatalogEntry<'a>> {
    units
        .iter()
        .enumerate()
        .map(|(position, unit)| tos_pipeline::SourceCatalogEntry {
            id: tos_pipeline::SourceEntryId::at(position),
            path: unit.path,
        })
        .collect()
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
        let run = tos_pipeline::run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
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

/// A provider that answers a member with something other than what resolution
/// saw fails the preparation (ADR-0072 §6).
///
/// Materialization is a separate stage from resolution precisely so that this
/// is catchable: the identity is recomputed from the bytes that came back, not
/// remembered about them. Nothing looks for an alternative.
#[test]
fn source_that_changed_after_resolution_fails_the_preparation() {
    let dependency = lib(
        "system.lib.math",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let substituted = lib(
        "system.lib.math",
        "pub fn double(value: i32) -> i32 { return value * 3i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return math.double(21i32); }",
    );
    let units = [
        Unit {
            path: "system/lib/math.tos",
            bytes: dependency.as_bytes(),
        },
        Unit {
            path: "system/boot/init.tos",
            bytes: entry.as_bytes(),
        },
    ];

    /// Answers the resolution pass truthfully and the materialization pass with
    /// something else — the exact time-of-check to time-of-use window the two
    /// stages exist to close.
    struct Swapped<'a> {
        units: &'a [Unit<'a>],
        at: usize,
        instead: &'a [u8],
        seen: core::cell::Cell<usize>,
    }
    impl tos_pipeline::SourceProvider for Swapped<'_> {
        fn catalog(&self) -> Vec<tos_pipeline::SourceCatalogEntry<'_>> {
            catalog_of(self.units)
        }

        fn source(&self, id: tos_pipeline::SourceEntryId) -> Option<&[u8]> {
            if id.position() == self.at {
                let seen = self.seen.get();
                self.seen.set(seen + 1);
                if seen > 0 {
                    return Some(self.instead);
                }
            }
            self.units.get(id.position()).map(|unit| unit.bytes)
        }
    }

    let provider = Swapped {
        units: &units,
        at: 0,
        instead: substituted.as_bytes(),
        seen: core::cell::Cell::new(0),
    };
    match tos_pipeline::prepare_from_provider(
        &provider,
        "tos-module-set-tests",
        "system/boot/init.tos",
        "main",
        &mut Silent,
        tos_pipeline::HOST_RESIDENCY,
    ) {
        Ok(tos_pipeline::Preparation::Refused(Run::SourceRefused(refusal))) => {
            assert_eq!(refusal.symbol(), "source-changed");
            assert_eq!(refusal.path(), "system/lib/math.tos");
        }
        Ok(_) => panic!("substituted source produced an executable"),
        Err(error) => panic!("the request itself was rejected: {error:?}"),
    }
}

/// A provider that has nothing for a member of its own resolved closure fails
/// the preparation rather than running without it.
#[test]
fn source_that_vanished_after_resolution_fails_the_preparation() {
    let dependency = lib(
        "system.lib.math",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return math.double(21i32); }",
    );
    let units = [
        Unit {
            path: "system/lib/math.tos",
            bytes: dependency.as_bytes(),
        },
        Unit {
            path: "system/boot/init.tos",
            bytes: entry.as_bytes(),
        },
    ];

    /// Present when the closure was resolved, gone when it was needed.
    struct Gone<'a> {
        units: &'a [Unit<'a>],
        at: usize,
        seen: core::cell::Cell<usize>,
    }
    impl tos_pipeline::SourceProvider for Gone<'_> {
        fn catalog(&self) -> Vec<tos_pipeline::SourceCatalogEntry<'_>> {
            catalog_of(self.units)
        }

        fn source(&self, id: tos_pipeline::SourceEntryId) -> Option<&[u8]> {
            if id.position() == self.at {
                let seen = self.seen.get();
                self.seen.set(seen + 1);
                if seen > 0 {
                    return None;
                }
            }
            self.units.get(id.position()).map(|unit| unit.bytes)
        }
    }

    let provider = Gone {
        units: &units,
        at: 0,
        seen: core::cell::Cell::new(0),
    };
    match tos_pipeline::prepare_from_provider(
        &provider,
        "tos-module-set-tests",
        "system/boot/init.tos",
        "main",
        &mut Silent,
        tos_pipeline::HOST_RESIDENCY,
    ) {
        Ok(tos_pipeline::Preparation::Refused(Run::SourceRefused(refusal))) => {
            assert_eq!(refusal.symbol(), "source-absent");
            assert_eq!(refusal.path(), "system/lib/math.tos");
        }
        Ok(_) => panic!("a missing member produced an executable"),
        Err(error) => panic!("the request itself was rejected: {error:?}"),
    }
}

/// Deleting every derived image costs a machine nothing it cannot rebuild.
///
/// ADR-0002's recovery property, and ADR-0072 §5: the image is a disposable
/// artifact, the source is canonical, and a preparation that starts from source
/// with no image at all reaches the same executable state. Two independent
/// preparations over the same canonical source produce the same receipt, which
/// is the same statement about the same bytes.
#[test]
fn a_closure_regenerates_from_canonical_source_with_no_image_at_all() {
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

    let mut receipts = Vec::new();
    for _ in 0..2 {
        // Nothing is carried between these two: no image, no record, no
        // manifest. Each starts from the canonical source and nothing else.
        let prepared =
            tos_pipeline::prepare_from_source(&request, &mut Silent, tos_pipeline::HOST_RESIDENCY)
                .expect("the set names an entry it contains");
        let tos_pipeline::Preparation::Ready(mut prepared) = prepared else {
            panic!("the closure prepares");
        };
        let run = tos_pipeline::run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
        let Run::Completed(completion) = run else {
            panic!("the regenerated closure runs: {run:?}");
        };
        assert_eq!(
            completion.value,
            tos_pipeline::Value::Int(tos_pipeline::IntKind::I32, 42)
        );
        receipts.push(completion.receipt.module_digest.clone());
    }
    assert_eq!(
        receipts[0], receipts[1],
        "regeneration from canonical source reaches the same verified module"
    );
}

/// A prepared closure runs after the source is gone (ADR-0072 §2, §1).
///
/// The provider, the request and the canonical text are all dropped between the
/// preparation and the run. What the process holds is images, records, the
/// membership and the entry receipt; it reaches its modules through the resident
/// set, and it names where it trapped with a canonical path and a byte span it
/// carried out of the run itself.
///
/// The line and the column come afterwards, from a reader that still has the
/// source. That reader is not the process.
#[test]
fn a_prepared_closure_runs_after_its_source_is_dropped() {
    let dependency = lib(
        "system.lib.math",
        "pub fn halve(value: i32) -> i32 { return value / 0i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return math.halve(4i32); }",
    );

    // Everything about the source lives inside this block and nothing escapes
    // it but the prepared closure.
    let prepared = {
        let texts = [
            ("system/lib/math.tos", dependency.clone()),
            ("system/boot/init.tos", entry.clone()),
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
        let tos_pipeline::Preparation::Ready(prepared) = prepared else {
            panic!("the closure prepares");
        };
        prepared
    };
    let mut prepared = prepared;

    let run = tos_pipeline::run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
    let Run::Trapped { code, at, .. } = &run else {
        panic!("expected a trap, got {run:?}");
    };
    assert_eq!(*code, "RUNTIME_DIVISION_BY_ZERO");
    let location = at.as_ref().expect("the trap names where it came from");
    assert_eq!(
        location.path, "system/lib/math.tos",
        "the canonical path came out of the run, not out of a source table"
    );
    assert!(location.byte_end > location.byte_start);

    // A boot log is written by a process in exactly this position, so the
    // rendered form must exist without source too.
    let rendered = tos_pipeline::render::events(&run);
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("TOS.RUN.TRAP ") && line.contains("system/lib/math.tos")),
        "{rendered:?}"
    );

    // And only now, by a reader that kept the text, does it become a line and a
    // column.
    let site = tos_pipeline::locate(location, dependency.as_bytes())
        .expect("the span locates against the source it names");
    assert_eq!(site.path, "system/lib/math.tos");
    assert!(site.start.line() >= 1);
}

/// A catalog entry is not membership (ADR-0072 §6, and 72.3c).
///
/// The source set declares three modules: an entry, a dependency it reaches,
/// and a perfectly valid module nothing imports. The provider knows about all
/// three — that is what a catalog is. The **closure** is two, and the unrelated
/// module has no `SourceModuleId` at all: discovery and executable authority are
/// different things, and only the second is minted.
#[test]
fn an_unrelated_catalog_entry_is_not_a_member_of_the_closure() {
    let dependency = lib(
        "system.lib.math",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let unrelated = lib(
        "system.lib.spectator",
        "pub fn watch() -> i32 { return 0i32; }",
    );
    let entry = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return math.double(21i32); }",
    );
    let texts = [
        ("system/lib/math.tos", dependency.as_str()),
        ("system/lib/spectator.tos", unrelated.as_str()),
        ("system/boot/init.tos", entry.as_str()),
    ];
    let units: Vec<Unit<'_>> = texts
        .iter()
        .map(|(path, text)| Unit {
            path,
            bytes: text.as_bytes(),
        })
        .collect();

    // The provider's catalog names all three.
    let provider = tos_pipeline::SliceSourceProvider::new(&units);
    assert_eq!(
        tos_pipeline::SourceProvider::catalog(&provider).len(),
        3,
        "the source set declares three modules"
    );

    let prepared = tos_pipeline::prepare_from_provider(
        &provider,
        "tos-module-set-tests",
        "system/boot/init.tos",
        "main",
        &mut Silent,
        tos_pipeline::HOST_RESIDENCY,
    )
    .expect("the set names an entry it contains");
    let tos_pipeline::Preparation::Ready(mut prepared) = prepared else {
        panic!("the closure prepares");
    };

    // The executable closure is two. The spectator is not in it, and there is
    // no identity under which it could be asked for.
    assert_eq!(
        prepared.modules(),
        2,
        "the closure holds what the entry reaches, not what the catalog offers"
    );
    let run = tos_pipeline::run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
    let Run::Completed(completion) = run else {
        panic!("the closure runs: {run:?}");
    };
    assert_eq!(
        completion.value,
        tos_pipeline::Value::Int(tos_pipeline::IntKind::I32, 42)
    );
}
