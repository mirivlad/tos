// SPDX-License-Identifier: GPL-3.0-or-later
//! The end-to-end gate: source, checked, lowered, verified, executed.
//!
//! Every case here goes through the whole production path — SourceReader,
//! Parser, Checker, lowerer, independent verifier, reference interpreter — with
//! no shortcut at any stage. An engine accepts IR only with a receipt for that
//! exact module, so the first thing each run proves is that the receipt matches.
//!
//! The interpreter is the semantic oracle, so what it computes is checked
//! against what the language says, including the traps: checked arithmetic,
//! division by zero, an invalid shift, fuel exhaustion, and the recursion
//! bound. Nothing here relies on a host panic or a Rust integer's width.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::{run, trap_source, Accounting, Refusal, Value};
use tos_ir::{IntKind, Module};
use tos_verifier::{verify, Limits, ResolutionSnapshot, VerifiedModule};

const ENVELOPE: &str = "resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0]";

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

/// Runs the whole production path over one module body.
fn pipeline(body: &str) -> (Module, VerifiedModule) {
    let text = format!("module app.sample version 1.0 profile bootstrap; {ENVELOPE} {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("source parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        diagnostics.is_empty(),
        "the pipeline takes checked source only: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let context = ModuleContext {
        source_set: String::from("tos-execution-tests"),
        path: String::from("app/sample.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("source lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("lowered IR verifies");
    (module, receipt)
}

fn evaluate(body: &str, entry: &str, arguments: Vec<Value>) -> Value {
    let (module, receipt) = pipeline(body);
    run(&module, &receipt, entry, arguments)
        .expect("the entry exists with this arity")
        .expect("the program does not trap")
        .value
}

fn trap(body: &str, entry: &str, arguments: Vec<Value>) -> tos_engine::Trap {
    let (module, receipt) = pipeline(body);
    let outcome = run(&module, &receipt, entry, arguments).expect("the entry exists");
    let trap = outcome.expect_err("the program must trap");
    assert!(
        trap_source(&module, &trap).is_some(),
        "a runtime trap names the source it came from"
    );
    trap
}

#[test]
fn a_constant_returning_function_runs_end_to_end() {
    assert_eq!(
        evaluate("pub fn answer() -> i32 { return 42i32; }", "answer", vec![]),
        Value::Int(IntKind::I32, 42)
    );
}

#[test]
fn checked_arithmetic_evaluates_left_to_right() {
    assert_eq!(
        evaluate(
            "pub fn total() -> i32 { return 2i32 * 3i32 + 4i32; }",
            "total",
            vec![]
        ),
        Value::Int(IntKind::I32, 10)
    );
}

#[test]
fn a_conditional_takes_the_branch_its_condition_names() {
    let body =
        "pub fn pick(flag: bool) -> i32 { if (flag) { return 1i32; } else { return 2i32; } }";
    assert_eq!(
        evaluate(body, "pick", vec![Value::Bool(true)]),
        Value::Int(IntKind::I32, 1)
    );
    assert_eq!(
        evaluate(body, "pick", vec![Value::Bool(false)]),
        Value::Int(IntKind::I32, 2)
    );
}

#[test]
fn a_loop_accumulates_and_terminates() {
    let body = "pub fn sum(limit: i32) -> i32 { \
         let mut total = 0i32; let mut current = 0i32; \
         while (current < limit) { total = total + current; current = current + 1i32; } \
         return total; }";
    assert_eq!(
        evaluate(body, "sum", vec![Value::Int(IntKind::I32, 5)]),
        Value::Int(IntKind::I32, 10)
    );
}

#[test]
fn a_record_is_built_and_read_by_field() {
    let body = "pub record Point [x: i32, y: i32] \
         pub fn total() -> i32 { let point = Point(x: 3i32, y: 4i32); return point.x + point.y; }";
    assert_eq!(evaluate(body, "total", vec![]), Value::Int(IntKind::I32, 7));
}

#[test]
fn a_named_constructor_orders_its_fields_by_declaration() {
    // The source supplies `y` first; the declared order still decides the
    // aggregate's operand order.
    let body = "pub record Point [x: i32, y: i32] \
         pub fn build() -> Point { return Point(y: 9i32, x: 1i32); }";
    assert_eq!(
        evaluate(body, "build", vec![]),
        Value::Aggregate(vec![
            Value::Int(IntKind::I32, 1),
            Value::Int(IntKind::I32, 9)
        ])
    );
}

#[test]
fn a_local_call_returns_through_its_own_scope() {
    let body = "fn double(value: i32) -> i32 { return value + value; } \
         pub fn main() -> i32 { return double(21i32); }";
    assert_eq!(evaluate(body, "main", vec![]), Value::Int(IntKind::I32, 42));
}

#[test]
fn a_match_dispatches_on_the_variant_it_is_given() {
    let body = "pub enum Signal [Low, High] \
         pub fn rank(signal: Signal) -> i32 { \
         match (signal) { Low => { return 1i32; } High => { return 2i32; } } }";
    assert_eq!(
        evaluate(
            body,
            "rank",
            vec![Value::Variant {
                index: 1,
                payload: vec![]
            }]
        ),
        Value::Int(IntKind::I32, 2)
    );
}

#[test]
fn a_checked_conversion_reports_its_outcome_as_a_result() {
    let body = "pub fn narrow(value: i32) -> Result<u8, ConversionError> { return to_u8(value); }";
    assert_eq!(
        evaluate(body, "narrow", vec![Value::Int(IntKind::I32, 7)]),
        Value::Variant {
            index: 0,
            payload: vec![Value::Int(IntKind::U8, 7)]
        }
    );
    // 300 does not fit u8, and the language says so with a value rather than a
    // silent truncation.
    let narrowed = evaluate(body, "narrow", vec![Value::Int(IntKind::I32, 300)]);
    let Value::Variant { index, .. } = narrowed else {
        panic!("a checked conversion produces a Result");
    };
    assert_eq!(index, 1);
}

#[test]
fn checked_overflow_traps_at_the_declared_width() {
    // The host could hold this in an i128; the program said i32, and that is
    // what bounds it.
    let body = "pub fn overflow(value: i32) -> i32 { return value * value; }";
    let trap = trap(body, "overflow", vec![Value::Int(IntKind::I32, 100_000)]);
    assert_eq!(trap.code, "RUNTIME_ARITHMETIC_OVERFLOW");
}

#[test]
fn division_by_zero_traps() {
    let body = "pub fn divide(left: i32, right: i32) -> i32 { return left / right; }";
    let trap = trap(
        body,
        "divide",
        vec![Value::Int(IntKind::I32, 1), Value::Int(IntKind::I32, 0)],
    );
    assert_eq!(trap.code, "RUNTIME_DIVISION_BY_ZERO");
}

#[test]
fn an_out_of_range_shift_count_traps() {
    let body = "pub fn shift(value: i32, places: i32) -> i32 { return value << places; }";
    let trap = trap(
        body,
        "shift",
        vec![Value::Int(IntKind::I32, 1), Value::Int(IntKind::I32, 32)],
    );
    assert_eq!(trap.code, "RUNTIME_INVALID_SHIFT");
}

#[test]
fn an_unbounded_loop_exhausts_its_declared_fuel() {
    // The loop never ends; fuel is what makes that a bounded, defined outcome
    // rather than a hang.
    let text = "module app.sample version 1.0 profile bootstrap; \
         resource [fuel: 64, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn spin() -> unit { loop { } }";
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("parses");
    assert!(Checker::check(&source, &schema).is_empty());
    let context = ModuleContext {
        source_set: String::from("tos-execution-tests"),
        path: String::from("app/sample.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("lowers");
    let receipt =
        verify(&module, &ResolutionSnapshot::default(), &Limits::default()).expect("verifies");
    let trap = run(&module, &receipt, "spin", vec![])
        .expect("the entry exists")
        .expect_err("an unbounded loop must exhaust its budget");
    assert_eq!(trap.code, "RUNTIME_FUEL_EXHAUSTED");
}

#[test]
fn unbounded_recursion_stops_at_the_declared_depth() {
    let body = "pub fn descend(value: i32) -> i32 { return descend(value + 1i32); }";
    let trap = trap(body, "descend", vec![Value::Int(IntKind::I32, 0)]);
    assert_eq!(trap.code, "RUNTIME_RECURSION_LIMIT");
}

#[test]
fn a_run_records_what_it_consumed() {
    let (module, receipt) = pipeline("pub fn answer() -> i32 { return 42i32; }");
    let outcome = run(&module, &receipt, "answer", vec![])
        .expect("the entry exists")
        .expect("no trap");
    let accounting = Accounting::of(&module, &outcome);
    assert!(accounting.fuel_used > 0);
    assert!(accounting.fuel_used <= accounting.fuel_limit);
    assert_eq!(accounting.max_call_depth, 1);
}

#[test]
fn an_engine_refuses_a_receipt_for_another_module() {
    let (module, _) = pipeline("pub fn answer() -> i32 { return 42i32; }");
    let (_, other_receipt) = pipeline("pub fn answer() -> i32 { return 7i32; }");
    assert_eq!(
        run(&module, &other_receipt, "answer", vec![]),
        Err(Refusal::ReceiptDoesNotMatch),
        "an engine accepts IR only with a receipt for that exact module"
    );
}

#[test]
fn an_engine_refuses_an_entry_the_module_does_not_have() {
    let (module, receipt) = pipeline("pub fn answer() -> i32 { return 42i32; }");
    assert_eq!(
        run(&module, &receipt, "elsewhere", vec![]),
        Err(Refusal::NoSuchEntry(String::from("elsewhere")))
    );
    assert_eq!(
        run(&module, &receipt, "answer", vec![Value::Unit]),
        Err(Refusal::EntryArity {
            expected: 0,
            actual: 1
        })
    );
}

#[test]
fn the_same_program_produces_the_same_result_every_run() {
    let (module, receipt) = pipeline(
        "pub fn sum(limit: i32) -> i32 { let mut total = 0i32; let mut current = 0i32; \
         while (current < limit) { total = total + current; current = current + 1i32; } \
         return total; }",
    );
    let first = run(&module, &receipt, "sum", vec![Value::Int(IntKind::I32, 10)])
        .unwrap()
        .unwrap();
    let second = run(&module, &receipt, "sum", vec![Value::Int(IntKind::I32, 10)])
        .unwrap()
        .unwrap();
    assert_eq!(first, second, "execution must be deterministic");
    assert_eq!(first.value, Value::Int(IntKind::I32, 45));
}
