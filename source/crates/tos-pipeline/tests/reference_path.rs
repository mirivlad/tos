// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference path, exercised end to end and at each place it refuses.
//!
//! These are composition tests, not language tests: each one asks whether the
//! right stage ran and whether its refusal reached the caller intact. The
//! language rules themselves are tested where they live.

use tos_pipeline::{execute, render, PipelineStage, Request, Run, Silent, Trace};

const PRELUDE: &str = "module system.boot.init version 1.0 profile bootstrap; \
     resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
     sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] ";

const PATH: &str = "system/boot/init.tos";

fn request<'a>(text: &'a str, entry: &'a str) -> Request<'a> {
    Request {
        source_set: "tos-pipeline-tests",
        path: PATH,
        bytes: text.as_bytes(),
        entry,
    }
}

/// Records the stages the pipeline announced, in order.
#[derive(Default)]
struct Recorder {
    stages: Vec<PipelineStage>,
}

impl Trace for Recorder {
    fn entering(&mut self, stage: PipelineStage) {
        self.stages.push(stage);
    }
}

#[test]
fn canonical_source_reaches_an_executed_result_through_every_stage() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return 6i32 * 7i32; }}");
    let mut recorder = Recorder::default();
    let run = execute(&request(&text, "main"), Vec::new(), &mut recorder);

    assert_eq!(
        recorder.stages,
        vec![
            PipelineStage::Read,
            PipelineStage::Parse,
            PipelineStage::Check,
            PipelineStage::Resolve,
            PipelineStage::Lower,
            PipelineStage::Verify,
            PipelineStage::Execute,
        ],
        "every stage must be entered, in order"
    );

    let Run::Completed(completion) = run else {
        panic!("expected a completed run, got {run:?}");
    };
    assert_eq!(render::value(&completion.value), "i32:42");
    // The receipt must name this exact module, which is what makes the engine's
    // acceptance a statement about the IR it actually ran.
    assert_eq!(completion.receipt.module_name, "system.boot.init");
    assert!(completion.receipt.module_digest.starts_with("sha256:"));
    assert_eq!(
        completion.receipt.verifier_identity,
        tos_verifier::VERIFIER_IDENTITY
    );
    assert!(completion.accounting.fuel_used > 0, "a run consumes fuel");
    assert!(completion.accounting.fuel_used <= completion.accounting.fuel_limit);
}

#[test]
fn identity_is_computed_from_the_source_rather_than_declared() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return 1i32; }}");
    let other = format!("{PRELUDE} pub fn main() -> i32 {{ return 2i32; }}");
    let first = execute(&request(&text, "main"), Vec::new(), &mut Silent);
    let second = execute(&request(&other, "main"), Vec::new(), &mut Silent);

    let (Run::Completed(first), Run::Completed(second)) = (first, second) else {
        panic!("both fixtures must run");
    };
    assert_ne!(
        first.receipt.content_id, second.receipt.content_id,
        "two different sources cannot share a content identity"
    );
    assert_ne!(first.receipt.module_digest, second.receipt.module_digest);
    // Same source set, same declared imports: the digests over those lists are
    // equal because the lists are, not because they are placeholders.
    assert_eq!(
        first.receipt.dependency_digest,
        second.receipt.dependency_digest
    );
    assert!(first.receipt.dependency_digest.starts_with("sha256:"));
}

#[test]
fn the_same_source_twice_produces_the_same_identity_and_answer() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return 6i32 * 7i32; }}");
    let first = execute(&request(&text, "main"), Vec::new(), &mut Silent);
    let second = execute(&request(&text, "main"), Vec::new(), &mut Silent);
    let (Run::Completed(first), Run::Completed(second)) = (first, second) else {
        panic!("both runs must complete");
    };
    assert_eq!(first.receipt.module_digest, second.receipt.module_digest);
    assert_eq!(render::value(&first.value), render::value(&second.value));
    assert_eq!(first.accounting, second.accounting);
}

#[test]
fn bytes_that_are_not_a_transport_valid_source_unit_stop_at_the_reader() {
    let mut recorder = Recorder::default();
    let run = execute(
        &request("\u{feff}module x;", "main"),
        Vec::new(),
        &mut recorder,
    );
    assert_eq!(recorder.stages, vec![PipelineStage::Read]);
    let Run::SourceRejected { code, .. } = run else {
        panic!("expected a transport refusal, got {run:?}");
    };
    assert_eq!(code, "E1002_BOM_FORBIDDEN");
}

#[test]
fn a_grammar_error_stops_at_the_parser_and_carries_its_diagnostics() {
    let mut recorder = Recorder::default();
    let run = execute(
        &request(
            "module system.boot.init version 1.0 profile bootstrap; pub fn (",
            "main",
        ),
        Vec::new(),
        &mut recorder,
    );
    assert_eq!(
        recorder.stages,
        vec![PipelineStage::Read, PipelineStage::Parse]
    );
    let Run::Diagnosed { stage, diagnostics } = run else {
        panic!("expected diagnostics, got {run:?}");
    };
    assert_eq!(stage, PipelineStage::Parse);
    assert!(!diagnostics.is_empty());
    // The rendering must carry the normative locator, not just a message.
    let line = render::diagnostic(&diagnostics[0]);
    assert!(line.contains("bytes="), "{line}");
    assert!(line.contains("stage=parse"), "{line}");
}

#[test]
fn a_checked_rule_stops_at_the_checker() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return true; }}");
    let mut recorder = Recorder::default();
    let run = execute(&request(&text, "main"), Vec::new(), &mut recorder);
    assert_eq!(
        recorder.stages,
        vec![
            PipelineStage::Read,
            PipelineStage::Parse,
            PipelineStage::Check
        ]
    );
    let Run::Diagnosed { stage, diagnostics } = run else {
        panic!("expected diagnostics, got {run:?}");
    };
    assert_eq!(stage, PipelineStage::Check);
    assert!(
        diagnostics.iter().any(|d| d.code().starts_with("E12")),
        "{diagnostics:?}"
    );
}

#[test]
fn a_module_stored_at_the_wrong_path_stops_at_resolution() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return 1i32; }}");
    let request = Request {
        source_set: "tos-pipeline-tests",
        path: "system/boot/other.tos",
        bytes: text.as_bytes(),
        entry: "main",
    };
    let run = execute(&request, Vec::new(), &mut Silent);
    let Run::Diagnosed { stage, diagnostics } = run else {
        panic!("expected a resolution refusal, got {run:?}");
    };
    assert_eq!(stage, PipelineStage::Resolve);
    assert!(!diagnostics.is_empty());
}

#[test]
fn a_missing_entry_is_the_engine_refusing_rather_than_a_trap() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return 1i32; }}");
    let run = execute(&request(&text, "absent"), Vec::new(), &mut Silent);
    let Run::Refused(refusal) = &run else {
        panic!("expected a refusal, got {run:?}");
    };
    assert!(matches!(refusal, tos_engine::Refusal::NoSuchEntry(name) if name == "absent"));
    assert_eq!(run.failed_at(), Some(PipelineStage::Execute));
}

#[test]
fn a_trap_names_the_source_it_came_from() {
    // Division by zero is a defined dynamic failure, not a checked one.
    let text = format!("{PRELUDE} pub fn main(divisor: i32) -> i32 {{ return 1i32 / divisor; }}");
    let run = execute(
        &request(&text, "main"),
        vec![tos_engine::Value::Int(tos_ir::IntKind::I32, 0)],
        &mut Silent,
    );
    let Run::Trapped { code, at, .. } = &run else {
        panic!("expected a trap, got {run:?}");
    };
    assert_eq!(*code, "RUNTIME_DIVISION_BY_ZERO");
    let site = at.as_ref().expect("a trap must name its source");
    assert_eq!(site.path, PATH);
    assert!(site.start.line() >= 1);
    // The rendered form is what a boot log carries, so it is checked too.
    let rendered = render::events(&run);
    assert!(
        rendered
            .iter()
            .any(|line| line.starts_with("TOS.RUN.TRAP ")),
        "{rendered:?}"
    );
    assert!(
        rendered.iter().any(|line| line.contains(PATH)),
        "{rendered:?}"
    );
}

#[test]
fn a_completed_run_renders_verification_accounting_and_answer_in_order() {
    let text = format!("{PRELUDE} pub fn main() -> i32 {{ return 6i32 * 7i32; }}");
    let run = execute(&request(&text, "main"), Vec::new(), &mut Silent);
    let lines = render::events(&run);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].starts_with("TOS.RUN.VERIFIED module=system.boot.init digest=sha256:"));
    assert!(lines[1].starts_with("TOS.RUN.ACCOUNTING fuel="));
    assert_eq!(lines[2], "TOS.RUN.COMPLETED value=i32:42");
    // One event per line: nothing a value or a detail carries may split a line.
    for line in &lines {
        assert!(!line.contains('\n') && !line.contains('\r'), "{line}");
    }
}
