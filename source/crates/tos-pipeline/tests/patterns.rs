// SPDX-License-Identifier: GPL-3.0-or-later
//! Destructuring patterns, from source to an executed result.
//!
//! The grammar of docs/39 admits a pattern wherever `let`, `for` and `match`
//! take one, so a program the checker accepts must reach `tos-ir/v1` — the
//! accepted corpus not happening to contain a construct is not a reason for the
//! lowerer to refuse it.
//!
//! These run the whole reference path rather than the checker alone, because
//! the claim being made is that the construct *executes*, and a checker test
//! cannot say that.

use tos_pipeline::{execute, render, Request, Run, Silent};

const PRELUDE: &str = "module system.boot.init version 1.0 profile bootstrap; \
     resource [fuel: 10000, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1, \
     sync: 0, shared: 0B, cleanup: 4, recursion: 8, imports: 0] ";

fn run(text: &str) -> Run {
    let request = Request {
        source_set: "tos-pattern-tests",
        path: "system/boot/init.tos",
        bytes: text.as_bytes(),
        entry: "main",
    };
    execute(&request, Vec::new(), &mut Silent)
}

fn value_of(text: &str) -> String {
    let outcome = run(text);
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run: {:?}", render::events(&outcome));
    };
    render::value(&completion.value)
}

#[test]
fn a_tuple_destructuring_let_reaches_an_executed_result() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (2i32, 3i32); \
         let (left, right) = pair; return left * right; }}"
    );
    assert_eq!(value_of(&text), "i32:6");
}

#[test]
fn a_nested_tuple_destructuring_let_reaches_an_executed_result() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let nested = (1i32, (2i32, 3i32)); \
         let (head, (left, right)) = nested; return head + left * right; }}"
    );
    assert_eq!(value_of(&text), "i32:7");
}

#[test]
fn a_wildcard_inside_a_tuple_pattern_binds_nothing_and_takes_nothing() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (4i32, 9i32); \
         let (_, right) = pair; return right; }}"
    );
    assert_eq!(value_of(&text), "i32:9");
}

#[test]
fn destructuring_an_affine_component_moves_it_rather_than_copying_it() {
    // The checker owns this rule; the point here is that lowering expresses it
    // rather than quietly turning a move into a copy.
    let text = format!(
        "{PRELUDE} pub record Message [payload: bytes] \
         pub fn take(message: Message) -> unit {{ }} \
         pub fn main() -> unit {{ let pair = (Message(payload: b\"hi\"), 1i32); \
         let (message, count) = pair; take(message); take(message); }}"
    );
    let outcome = run(&text);
    let Run::Diagnosed { diagnostics, .. } = &outcome else {
        panic!("a second use of a moved component must be refused");
    };
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code() == "E1301_USE_AFTER_MOVE"),
        "{:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
}

#[test]
fn a_copy_component_stays_usable_after_destructuring() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (5i32, 6i32); \
         let (left, right) = pair; return left + left + right; }}"
    );
    assert_eq!(value_of(&text), "i32:16");
}

#[test]
fn the_verifier_accepts_destructured_ir_on_its_own_terms() {
    // Destructuring lowers to the same Move-through-a-place the named case
    // emits, so the independent verifier sees no new construct and its
    // ownership and type rules apply unchanged.
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (7i32, 8i32); \
         let (left, right) = pair; return right - left; }}"
    );
    let outcome = run(&text);
    let Run::Completed(completion) = &outcome else {
        panic!("expected a completed run: {:?}", render::events(&outcome));
    };
    assert!(completion.receipt.module_digest.starts_with("sha256:"));
    assert_eq!(render::value(&completion.value), "i32:1");
}

/// A tuple pattern in `match` executes.
///
/// Valid V1 source must reach an executed result, so this requires one rather
/// than accepting "the checker refused it" as an answer. The wider matrix of
/// `match` shapes is in `match_matrix.rs`.
#[test]
fn a_tuple_match_pattern_executes() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (1i32, 2i32); \
         match (pair) {{ (a, b) => {{ return a + b; }} }} }}"
    );
    assert_eq!(value_of(&text), "i32:3");
}

/// A `for` over a sequence executes.
#[test]
fn a_for_over_a_sequence_executes() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let values: array<i32, 2> = [1i32, 2i32]; \
         let mut total = 0i32; for value in (values) {{ total = total + value; }} return total; }}"
    );
    assert_eq!(value_of(&text), "i32:3");
}
