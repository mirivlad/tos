// SPDX-License-Identifier: GPL-3.0-or-later
//! Module-level constants (ADR-0052, docs/40 section 2).
//!
//! A constant is a compile-time value: its initializer is a constant
//! expression, and a use is the value itself. These are composition tests —
//! that the decision holds through check, lowering, verification and execution
//! — rather than tests of any one stage.

use tos_pipeline::{execute, render, PipelineStage, Request, Run, Severity, Silent};

const PRELUDE: &str = "module system.boot.init version 1.0 profile bootstrap; \
     resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
     sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] ";

fn request<'a>(text: &'a str) -> Request<'a> {
    Request {
        source_set: "tos-constant-tests",
        path: "system/boot/init.tos",
        bytes: text.as_bytes(),
        entry: "main",
    }
}

fn run(body: &str) -> Run {
    let text = format!("{PRELUDE} {body}");
    execute(&request(&text), Vec::new(), &mut Silent)
}

fn errors(run: &Run) -> Vec<(&'static str, String)> {
    let Run::Diagnosed { diagnostics, .. } = run else {
        panic!("expected diagnostics, got {run:?}");
    };
    diagnostics
        .iter()
        .filter(|entry| entry.severity() == Severity::Error)
        .map(|entry| {
            let reason = entry
                .fields()
                .iter()
                .find(|field| field.key() == "reason")
                .map(|field| field.value().to_string())
                .unwrap_or_default();
            (entry.code(), reason)
        })
        .collect()
}

#[test]
fn a_scalar_constant_is_its_value_where_it_is_used() {
    let outcome = run("pub const LIMIT: i32 = 7i32; pub fn main() -> i32 { return LIMIT; }");
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run, got {outcome:?}");
    };
    assert_eq!(render::value(&completion.value), "i32:7");
}

/// The pattern the decision exists for: a constant computed from another one.
/// Forbidding this would have made a worse language to save an implementation
/// from constant folding.
#[test]
fn a_constant_may_be_computed_from_another_constant() {
    let outcome = run(
        "pub const PAGE: i32 = 4096i32; pub const WINDOW: i32 = PAGE * 4i32; \
         pub fn main() -> i32 { return WINDOW; }",
    );
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run, got {outcome:?}");
    };
    assert_eq!(render::value(&completion.value), "i32:16384");
}

#[test]
fn an_aggregate_constant_is_constructed_where_it_is_read() {
    let outcome = run("pub record Limits [pub depth: i32, pub width: i32] \
         pub const LIMITS: Limits = Limits(depth: 8i32, width: 4i32); \
         pub fn main() -> i32 { return LIMITS.depth; }");
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run, got {outcome:?}");
    };
    assert_eq!(render::value(&completion.value), "i32:8");
}

/// An initializer may not execute anything. This is the whole of option A's
/// cost, and it is a diagnostic in source terms rather than a lowering gap.
#[test]
fn an_initializer_that_calls_is_refused_at_the_checker() {
    let outcome = run("pub fn four() -> i32 { return 4i32; } \
         pub const LIMIT: i32 = four(); \
         pub fn main() -> i32 { return LIMIT; }");
    assert_eq!(outcome.failed_at(), Some(PipelineStage::Check));
    assert!(
        errors(&outcome).contains(&("E1224_NONCONSTANT_INITIALIZER", String::from("call"))),
        "{:?}",
        errors(&outcome)
    );
}

#[test]
fn a_self_referential_constant_is_refused_rather_than_recursed() {
    let outcome = run("pub const A: i32 = B; pub const B: i32 = A; \
         pub fn main() -> i32 { return A; }");
    // Whatever refuses it, the boot never runs and the stack never unwinds.
    assert_ne!(outcome.failed_at(), None, "a cycle must not execute");
    assert!(!outcome.is_completed());
}

/// A constant that nothing reads still costs nothing and blocks nothing.
#[test]
fn an_unread_constant_does_not_change_the_module() {
    let outcome = run("pub const LIMIT: i32 = 7i32; pub fn main() -> i32 { return 1i32; }");
    assert!(outcome.is_completed(), "{outcome:?}");
}
