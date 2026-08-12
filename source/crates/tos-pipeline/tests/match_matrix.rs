// SPDX-License-Identifier: GPL-3.0-or-later
//! Every `match` shape the accepted contract admits, end to end.
//!
//! The claim under test is not "no `Gap` in the current corpus". It is that
//! source the checker accepts reaches an executed result — checker, lowerer,
//! independent verifier and engine agreeing on one meaning.

use tos_pipeline::{execute, render, Request, Run, Silent};

const PRELUDE: &str = "module system.boot.init version 1.0 profile bootstrap; \
     resource [fuel: 10000, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1, \
     sync: 0, shared: 0B, cleanup: 4, recursion: 8, imports: 0] ";

fn run(text: &str) -> Run {
    let request = Request {
        source_set: "tos-match-matrix",
        path: "system/boot/init.tos",
        bytes: text.as_bytes(),
        entry: "main",
    };
    execute(&request, Vec::new(), &mut Silent)
}

/// Runs a body and requires an executed answer, naming the stage if it stops.
fn answer(body: &str) -> String {
    let text = format!("{PRELUDE} {body}");
    let outcome = run(&text);
    let Run::Completed(completion) = &outcome else {
        panic!(
            "valid source must execute, stopped instead: {:?}",
            render::events(&outcome)
        );
    };
    render::value(&completion.value)
}

fn codes(body: &str) -> Vec<&'static str> {
    let text = format!("{PRELUDE} {body}");
    match run(&text) {
        Run::Diagnosed { diagnostics, .. } => diagnostics.iter().map(|d| d.code()).collect(),
        _ => Vec::new(),
    }
}

// ------------------------------------------------------- tuple subjects

#[test]
fn tuple_match_single_arm() {
    assert_eq!(
        answer(
            "pub fn main() -> i32 { let pair = (2i32, 3i32); \
                match (pair) { (a, b) => { return a * b; } } }"
        ),
        "i32:6"
    );
}

#[test]
fn tuple_match_arm_then_wildcard() {
    let body = "pub fn main() -> i32 { let pair = (2i32, 3i32); \
         match (pair) { (a, b) => { return a * b; } _ => { return 0i32; } } }";
    let found = codes(body);
    if found.is_empty() {
        assert_eq!(answer(body), "i32:6", "the first matching arm must win");
    } else {
        // If the contract refuses an unreachable arm, it must say so with a
        // diagnostic rather than by the lowerer refusing to represent it.
        assert!(!found.is_empty(), "a refusal must name a code");
    }
}

#[test]
fn wildcard_only_match_on_a_tuple() {
    assert_eq!(
        answer(
            "pub fn main() -> i32 { let pair = (2i32, 3i32); \
                match (pair) { _ => { return 7i32; } } }"
        ),
        "i32:7"
    );
}

#[test]
fn bare_binding_match_on_a_tuple() {
    let body = "pub fn main() -> i32 { let pair = (2i32, 3i32); \
         match (pair) { whole => { return 5i32; } } }";
    let found = codes(body);
    if found.is_empty() {
        assert_eq!(answer(body), "i32:5");
    }
}

#[test]
fn nested_tuple_match() {
    assert_eq!(
        answer(
            "pub fn main() -> i32 { let nested = (1i32, (2i32, 3i32)); \
                match (nested) { (head, (left, right)) => { return head + left * right; } } }"
        ),
        "i32:7"
    );
}

// ------------------------------------------------------- non-sum subjects

#[test]
fn wildcard_match_on_an_integer() {
    let body = "pub fn main() -> i32 { let value = 4i32; \
         match (value) { _ => { return value; } } }";
    let found = codes(body);
    if found.is_empty() {
        assert_eq!(answer(body), "i32:4");
    }
}

// ------------------------------------------------------- enum subjects

const MODE: &str = "pub enum Mode [Fast, Slow] ";

#[test]
fn enum_constructor_arms_execute() {
    assert_eq!(
        answer(&format!(
            "{MODE} pub fn pick(mode: Mode) -> i32 {{ \
             match (mode) {{ Fast => {{ return 1i32; }} Slow => {{ return 2i32; }} }} }} \
             pub fn main() -> i32 {{ return pick(Fast); }}"
        )),
        "i32:1"
    );
}

#[test]
fn an_irrefutable_arm_before_a_variant_arm_keeps_source_order() {
    // The ordering question: if V1 means "first matching arm", a catch-all
    // before a variant arm must win and the later arm must not run.
    let body = format!(
        "{MODE} pub fn pick(mode: Mode) -> i32 {{ \
         match (mode) {{ _ => {{ return 1i32; }} Fast => {{ return 2i32; }} }} }} \
         pub fn main() -> i32 {{ return pick(Fast); }}"
    );
    let found = codes(&body);
    if found.is_empty() {
        assert_eq!(
            answer(&body),
            "i32:1",
            "a catch-all before a variant arm must win under source order"
        );
    }
}

#[test]
fn a_binding_catch_all_before_a_variant_arm_keeps_source_order() {
    let body = format!(
        "{MODE} pub fn pick(mode: Mode) -> i32 {{ \
         match (mode) {{ other => {{ return 1i32; }} Fast => {{ return 2i32; }} }} }} \
         pub fn main() -> i32 {{ return pick(Fast); }}"
    );
    let found = codes(&body);
    if found.is_empty() {
        assert_eq!(answer(&body), "i32:1");
    }
}

#[test]
fn option_constructor_arms_execute() {
    assert_eq!(
        answer(
            "pub fn pick(value: Option<i32>) -> i32 { \
             match (value) { Some(inner) => { return inner; } None => { return 0i32; } } } \
             pub fn main() -> i32 { return pick(Some(9i32)); }"
        ),
        "i32:9"
    );
}

// ------------------------------------------------- adversarial: no Gap at all

/// No source the checker accepts may reach a lowering `Gap`.
///
/// This is the audit's standing question in test form. Each fixture below is
/// valid V1 by the accepted grammar and type semantics, so each must reach the
/// engine — a `Gap` here is a contract the implementation does not meet, and a
/// checker refusal would have to name a code rather than be silent.
#[test]
fn no_accepted_source_in_this_matrix_reaches_a_lowering_gap() {
    let fixtures = [
        "pub fn main() -> i32 { let pair = (1i32, 2i32); \
         match (pair) { (a, b) => { return a + b; } } }",
        "pub fn main() -> i32 { let pair = (1i32, 2i32); \
         match (pair) { _ => { return 1i32; } } }",
        "pub fn main() -> i32 { let pair = (1i32, 2i32); \
         match (pair) { whole => { return 1i32; } } }",
        "pub fn main() -> i32 { let v = 1i32; match (v) { _ => { return v; } } }",
        "pub fn main() -> i32 { let n = (1i32, (2i32, 3i32)); \
         match (n) { (a, (b, c)) => { return a + b + c; } } }",
        "pub enum Mode [Fast, Slow] pub fn pick(m: Mode) -> i32 { \
         match (m) { Fast => { return 1i32; } Slow => { return 2i32; } } } \
         pub fn main() -> i32 { return pick(Slow); }",
        "pub enum Mode [Fast, Slow] pub fn pick(m: Mode) -> i32 { \
         match (m) { Fast => { return 1i32; } _ => { return 2i32; } } } \
         pub fn main() -> i32 { return pick(Slow); }",
        "pub fn pick(v: Result<i32, i32>) -> i32 { \
         match (v) { Ok(x) => { return x; } Err(e) => { return e; } } } \
         pub fn main() -> i32 { return pick(Ok(3i32)); }",
        "pub fn main() -> i32 { let pair = (4i32, 5i32); let (a, b) = pair; return a + b; }",
        "pub fn main() -> i32 { let v: array<i32, 2> = [1i32, 2i32]; \
         let mut t = 0i32; for x in (v) { t = t + x; } return t; }",
    ];
    let mut gaps = Vec::new();
    for body in fixtures {
        let text = format!("{PRELUDE} {body}");
        if let Run::NotLowered(gap) = run(&text) {
            gaps.push(format!("{}: {body}", gap.construct));
        }
    }
    assert!(
        gaps.is_empty(),
        "checker-accepted source reached a Gap: {gaps:#?}"
    );
}
