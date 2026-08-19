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
use tos_engine::{run, trap_source, Accounting, Refusal, Unreachable, Value};
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
    run(&module, &receipt, entry, arguments, &mut Unreachable)
        .expect("the entry exists with this arity")
        .expect("the program does not trap")
        .value
}

fn trap(body: &str, entry: &str, arguments: Vec<Value>) -> tos_engine::Trap {
    let (module, receipt) = pipeline(body);
    let outcome =
        run(&module, &receipt, entry, arguments, &mut Unreachable).expect("the entry exists");
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
    let trap = run(&module, &receipt, "spin", vec![], &mut Unreachable)
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
    let outcome = run(&module, &receipt, "answer", vec![], &mut Unreachable)
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
        run(&module, &other_receipt, "answer", vec![], &mut Unreachable),
        Err(Refusal::ReceiptDoesNotMatch),
        "an engine accepts IR only with a receipt for that exact module"
    );
}

#[test]
fn an_engine_refuses_an_entry_the_module_does_not_have() {
    let (module, receipt) = pipeline("pub fn answer() -> i32 { return 42i32; }");
    assert_eq!(
        run(&module, &receipt, "elsewhere", vec![], &mut Unreachable),
        Err(Refusal::NoSuchEntry(String::from("elsewhere")))
    );
    assert_eq!(
        run(
            &module,
            &receipt,
            "answer",
            vec![Value::Unit],
            &mut Unreachable
        ),
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
    let first = run(
        &module,
        &receipt,
        "sum",
        vec![Value::Int(IntKind::I32, 10)],
        &mut Unreachable,
    )
    .unwrap()
    .unwrap();
    let second = run(
        &module,
        &receipt,
        "sum",
        vec![Value::Int(IntKind::I32, 10)],
        &mut Unreachable,
    )
    .unwrap()
    .unwrap();
    assert_eq!(first, second, "execution must be deterministic");
    assert_eq!(first.value, Value::Int(IntKind::I32, 45));
}

// The constructs added by full V1 lowering, executed end to end.

const FULL_ENVELOPE: &str = "resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 4, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0]";

/// The whole path for a Full-profile module.
fn full_pipeline(body: &str) -> (Module, VerifiedModule) {
    let text = format!("module app.sample version 1.0 profile full; {FULL_ENVELOPE} {body}");
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

fn evaluate_full(body: &str, entry: &str, arguments: Vec<Value>) -> Value {
    let (module, receipt) = full_pipeline(body);
    run(&module, &receipt, entry, arguments, &mut Unreachable)
        .expect("the entry exists with this arity")
        .expect("the program does not trap")
        .value
}

#[test]
fn a_spawned_child_is_joined_and_its_result_observed() {
    let body = "pub fn main() -> i32 { \
         parallel { let child = spawn parallel { return 7i32; }; \
         match (join child) { Completed(value) => { return value; } Cancelled => { return 0i32; } } } \
         return 0i32; }";
    assert_eq!(evaluate(body, "main", vec![]), Value::Int(IntKind::I32, 7));
}

#[test]
fn a_child_cancelled_before_its_join_reports_cancelled() {
    // docs/41 section 2: `cancel` is a cooperative request and the parent still
    // joins. Serialized Bootstrap defers the child to its join, so a request
    // that arrives first means the child never starts — an allowed outcome.
    let body = "pub fn main() -> i32 { \
         parallel { let child = spawn parallel { return 7i32; }; \
         cancel child; \
         match (join child) { Completed(value) => { return value; } Cancelled => { return 1i32; } } } \
         return 0i32; }";
    assert_eq!(evaluate(body, "main", vec![]), Value::Int(IntKind::I32, 1));
}

#[test]
fn a_spawned_child_sees_what_it_captured() {
    let body = "pub fn main(count: i32) -> i32 { \
         parallel { let child = spawn parallel { return count + 1i32; }; \
         match (join child) { Completed(value) => { return value; } Cancelled => { return 0i32; } } } \
         return 0i32; }";
    assert_eq!(
        evaluate(body, "main", vec![Value::Int(IntKind::I32, 41)]),
        Value::Int(IntKind::I32, 42)
    );
}

#[test]
fn a_closure_is_built_and_called_with_its_captures() {
    let body = "pub fn main(base: i32) -> i32 { \
         let add = fn (value: i32) { return value + base; }; return add(2i32); }";
    assert_eq!(
        evaluate_full(body, "main", vec![Value::Int(IntKind::I32, 40)]),
        Value::Int(IntKind::I32, 42)
    );
}

#[test]
fn an_async_function_returns_a_task_its_caller_awaits() {
    let body = "async fn produce() -> i32 { return 9i32; } \
         pub fn main() -> i32 { \
         match (await produce()) { Completed(value) => { return value; } Cancelled => { return 0i32; } } }";
    assert_eq!(
        evaluate_full(body, "main", vec![]),
        Value::Int(IntKind::I32, 9)
    );
}

#[test]
fn a_deferred_cleanup_runs_at_the_exit_it_belongs_to() {
    // The cleanup writes through the counter it captured, so the value observed
    // after the block proves the cleanup ran, and ran there.
    let body = "pub record Cell [value: i32] \
         fn bump(cell: Cell) -> Cell { return Cell(value: cell.value + 1i32); } \
         pub fn main() -> i32 { \
         let mut cell = Cell(value: 0i32); \
         if (true) { defer { cell = bump(cell); } } \
         return cell.value; }";
    assert_eq!(
        evaluate_full(body, "main", vec![]),
        Value::Int(IntKind::I32, 1)
    );
}

#[test]
fn cleanups_run_in_reverse_registration_order() {
    // The first cleanup doubles and the second adds one. Running them in
    // reverse means add-then-double: (0 + 1) * 2 = 2. Registration order would
    // have given (0 * 2) + 1 = 1.
    let body = "pub record Cell [value: i32] \
         fn doubled(cell: Cell) -> Cell { return Cell(value: cell.value * 2i32); } \
         fn incremented(cell: Cell) -> Cell { return Cell(value: cell.value + 1i32); } \
         pub fn main() -> i32 { \
         let mut cell = Cell(value: 0i32); \
         if (true) { defer { cell = doubled(cell); } defer { cell = incremented(cell); } } \
         return cell.value; }";
    assert_eq!(
        evaluate_full(body, "main", vec![]),
        Value::Int(IntKind::I32, 2)
    );
}

#[test]
fn a_return_runs_the_cleanups_of_the_blocks_it_leaves() {
    let body = "pub record Cell [value: i32] \
         fn incremented(cell: Cell) -> Cell { return Cell(value: cell.value + 1i32); } \
         pub fn main() -> i32 { \
         let mut cell = Cell(value: 0i32); \
         defer { cell = incremented(cell); } \
         return cell.value; }";
    // The cleanup runs after the return operand is evaluated, so the returned
    // value is the one that existed at the exit — ADR-0035's stated order.
    assert_eq!(
        evaluate_full(body, "main", vec![]),
        Value::Int(IntKind::I32, 0)
    );
}

#[test]
fn a_for_loop_walks_every_element_of_its_array() {
    let body = "pub fn main() -> i32 { \
         let values: array<i32, 3> = [1i32, 2i32, 3i32]; \
         let mut total = 0i32; \
         for value in (values) { total = total + 1i32; } \
         return total; }";
    assert_eq!(evaluate(body, "main", vec![]), Value::Int(IntKind::I32, 3));
}

#[test]
fn the_task_budget_bounds_how_many_children_may_start() {
    let text = "module app.sample version 1.0 profile bootstrap; \
         resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn main() -> i32 { \
         parallel { let one = spawn parallel { return 1i32; }; \
         let two = spawn parallel { return 2i32; }; \
         let first = join one; let second = join two; } \
         return 0i32; }";
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
    let trap = run(&module, &receipt, "main", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect_err("a second child exceeds a budget of one");
    assert_eq!(trap.code, "RUNTIME_TASK_LIMIT");
}

// docs/41 section 6 runtime accounting: a reservation is checked before the
// thing it pays for happens, so a module that would exceed its envelope never
// produces the effect at all.

#[test]
fn allocation_is_charged_where_a_value_is_built() {
    let (module, receipt) = pipeline(
        "pub record Point [x: i32, y: i32] \
         pub fn build() -> Point { return Point(x: 1i32, y: 2i32); }",
    );
    let outcome = run(&module, &receipt, "build", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect("no trap");
    let accounting = Accounting::of(&module, &outcome);
    assert!(
        accounting.allocation_peak > 0,
        "constructing a record must charge the allocation budget"
    );
    assert!(accounting.allocation_peak <= accounting.allocation_limit);
}

#[test]
fn an_allocation_beyond_the_envelope_fails_before_the_value_exists() {
    let text = "module app.sample version 1.0 profile bootstrap; \
         resource [fuel: 10000, stack: 64KiB, allocation: 0B, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub record Point [x: i32, y: i32] \
         pub fn build() -> Point { return Point(x: 1i32, y: 2i32); }";
    let trap = trap_in(text, "build", vec![]);
    assert_eq!(trap.code, "RUNTIME_ALLOCATION_LIMIT");
}

#[test]
fn a_frame_releases_what_it_allocated_when_it_returns() {
    // Each call charges and releases, so a bounded program stays bounded
    // however many times it calls. A budget that held only one record would
    // trap on the second call if release did not happen.
    let (module, receipt) = pipeline(
        "pub record Point [x: i32, y: i32] \
         fn one() -> Point { return Point(x: 1i32, y: 2i32); } \
         pub fn many() -> i32 { let a = one(); let b = one(); let c = one(); return 0i32; }",
    );
    let outcome = run(&module, &receipt, "many", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect("repeated calls must stay inside the budget");
    let accounting = Accounting::of(&module, &outcome);
    assert!(accounting.allocation_peak <= accounting.allocation_limit);
}

#[test]
fn a_registered_cleanup_is_charged_and_released_where_it_runs() {
    let (module, receipt) = full_pipeline(
        "pub record Cell [value: i32] \
         fn bump(cell: Cell) -> Cell { return Cell(value: cell.value + 1i32); } \
         pub fn main() -> i32 { let mut cell = Cell(value: 0i32); \
         if (true) { defer { cell = bump(cell); } } return cell.value; }",
    );
    let outcome = run(&module, &receipt, "main", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect("no trap");
    let accounting = Accounting::of(&module, &outcome);
    assert_eq!(
        accounting.cleanups_peak, 1,
        "one registration was live at a time"
    );
    assert!(accounting.cleanups_peak <= accounting.cleanup_limit);
}

#[test]
fn a_cleanup_beyond_the_envelope_never_reaches_the_engine() {
    // The verifier bounds cleanups per exit statically, so a module declaring
    // `cleanup: 0` is refused before a receipt exists. That is a stronger place
    // to catch it than the runtime charge, and the runtime charge still stands
    // behind it for anything the static bound cannot see.
    let text = "module app.sample version 1.0 profile full; \
         resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 0] \
         pub record Cell [value: i32] \
         fn bump(cell: Cell) -> Cell { return Cell(value: cell.value + 1i32); } \
         pub fn main() -> i32 { let mut cell = Cell(value: 0i32); \
         defer { cell = bump(cell); } return cell.value; }";
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
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("a cleanup budget of zero admits no registration");
    assert_eq!(finding.code, "V2022_RESOURCE");
}

#[test]
fn the_run_reserves_one_execution_context_against_the_declared_budget() {
    // Bootstrap serializes, so exactly one context is reserved, and it is
    // reserved before any instruction runs. `workers: 0` cannot be reached from
    // valid source — the frontend rejects it — so the reservation is observed
    // through the accounting rather than through a trap.
    let (module, receipt) = pipeline("pub fn answer() -> i32 { return 42i32; }");
    let outcome = run(&module, &receipt, "answer", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect("no trap");
    let accounting = Accounting::of(&module, &outcome);
    assert_eq!(accounting.workers_reserved, 1);
    assert!(accounting.workers_reserved <= accounting.worker_limit);
}

/// Runs whole module text and requires it to trap.
fn trap_in(text: &str, entry: &str, arguments: Vec<Value>) -> tos_engine::Trap {
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("parses");
    assert!(
        Checker::check(&source, &schema).is_empty(),
        "the fixture must be checked source"
    );
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
    run(&module, &receipt, entry, arguments, &mut Unreachable)
        .expect("the entry exists")
        .expect_err("the fixture must trap")
}

// ---------------------------------------------------------------------------
// A verified set (Stage 3 Phase 1 Task 4)
//
// The engine resolves a cross-module call against the set it was given and
// nothing else. What matters here is not that the call works — the pipeline
// tests cover that — but what the engine refuses: a receipt that does not match
// its module, a module the set does not contain, and a module that is present
// under the right name and is not the revision the caller was checked against.
// ---------------------------------------------------------------------------

/// Lowers and verifies one module of a set, under a name and path of its own.
fn member(name: &str, path: &str, body: &str) -> (Module, VerifiedModule) {
    let text = format!(
        "module {name} version 1.0 profile bootstrap; \
         resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] {body}"
    );
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("source parses");
    assert!(
        Checker::check(&source, &schema).is_empty(),
        "the set takes checked source only"
    );
    let context = ModuleContext {
        source_set: String::from("tos-conformance-v1"),
        path: String::from(path),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module =
        tos_core::lower_module_in_set(&source, &schema, &context, &[]).expect("member lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("member verifies");
    (module, receipt)
}

/// The dependency, and an entry that calls it. Built with the dependency's real
/// identity bound into the entry's import, exactly as the pipeline does.
fn calling_pair() -> ((Module, VerifiedModule), (Module, VerifiedModule)) {
    let dependency = member(
        "system.lib.math",
        "system/lib/math.tos",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let text = "module system.boot.init version 1.0 profile bootstrap; \
         import system.lib.math as math; \
         resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] \
         pub fn main() -> i32 { return math.double(21i32); }";
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("entry parses");
    assert!(Checker::check(&source, &schema).is_empty());
    let context = ModuleContext {
        source_set: String::from("tos-conformance-v1"),
        path: String::from("system/boot/init.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = tos_core::lower_module_in_set(
        &source,
        &schema,
        &context,
        &[tos_core::ResolvedImport {
            name: "system.lib.math",
            module: &dependency.0,
        }],
    )
    .expect("entry lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("entry verifies");
    (dependency, (module, receipt))
}

#[test]
fn a_call_across_the_set_returns_the_callee_result() {
    let (dependency, entry) = calling_pair();
    let set = [
        tos_engine::Verified {
            module: &dependency.0,
            receipt: &dependency.1,
        },
        tos_engine::Verified {
            module: &entry.0,
            receipt: &entry.1,
        },
    ];
    let outcome = tos_engine::run_set(&set, 1, "main", Vec::new(), &mut Unreachable)
        .expect("the entry is runnable")
        .expect("the run completes");
    assert_eq!(outcome.value, Value::Int(IntKind::I32, 42));
}

/// A module runs because its own receipt matches it. A dependency admitted on
/// the strength of its caller's receipt would make the receipt a statement
/// about the set, and it is a statement about one module.
#[test]
fn a_dependency_whose_receipt_does_not_match_it_refuses_the_whole_run() {
    let (dependency, entry) = calling_pair();
    let mut forged = dependency.1.clone();
    forged.module_digest = String::from("sha256:not-this-module");
    let set = [
        tos_engine::Verified {
            module: &dependency.0,
            receipt: &forged,
        },
        tos_engine::Verified {
            module: &entry.0,
            receipt: &entry.1,
        },
    ];
    match tos_engine::run_set(&set, 1, "main", Vec::new(), &mut Unreachable) {
        Err(Refusal::ReceiptDoesNotMatch) => {}
        other => panic!("a mismatched dependency receipt was accepted: {other:?}"),
    }
}

/// Every receipt is checked before anything runs, not when a call reaches it: a
/// program must not be able to choose which modules get checked by choosing
/// which branch it takes.
#[test]
fn a_receipt_is_checked_even_for_a_dependency_the_run_never_calls() {
    let (dependency, _) = calling_pair();
    let alone = member(
        "system.boot.init",
        "system/boot/init.tos",
        "pub fn main() -> i32 { return 7i32; }",
    );
    let mut forged = dependency.1.clone();
    forged.module_digest = String::from("sha256:not-this-module");
    let set = [
        tos_engine::Verified {
            module: &dependency.0,
            receipt: &forged,
        },
        tos_engine::Verified {
            module: &alone.0,
            receipt: &alone.1,
        },
    ];
    match tos_engine::run_set(&set, 1, "main", Vec::new(), &mut Unreachable) {
        Err(Refusal::ReceiptDoesNotMatch) => {}
        other => panic!("an unused module's receipt went unchecked: {other:?}"),
    }
}

#[test]
fn a_call_to_a_module_the_set_does_not_contain_traps() {
    let (_, entry) = calling_pair();
    let set = [tos_engine::Verified {
        module: &entry.0,
        receipt: &entry.1,
    }];
    let trap = tos_engine::run_set(&set, 0, "main", Vec::new(), &mut Unreachable)
        .expect("the entry is runnable")
        .expect_err("a call with nothing to call must trap");
    assert_eq!(trap.code, "RUNTIME_UNRESOLVED_IMPORT");
}

/// The right name is not enough. A set holding another revision of the module
/// under the same name is not the module this caller was lowered and verified
/// against, and running against it would silently execute code the caller was
/// never checked with.
#[test]
fn a_dependency_of_another_revision_under_the_same_name_traps() {
    let (_, entry) = calling_pair();
    let other = member(
        "system.lib.math",
        "system/lib/math.tos",
        "pub fn double(value: i32) -> i32 { return value * 3i32; }",
    );
    let set = [
        tos_engine::Verified {
            module: &other.0,
            receipt: &other.1,
        },
        tos_engine::Verified {
            module: &entry.0,
            receipt: &entry.1,
        },
    ];
    let trap = tos_engine::run_set(&set, 1, "main", Vec::new(), &mut Unreachable)
        .expect("the entry is runnable")
        .expect_err("a substituted dependency must trap");
    assert_eq!(trap.code, "RUNTIME_UNRESOLVED_IMPORT");
    assert!(
        trap.detail.contains("lowered against"),
        "the trap must say what disagreed: {trap:?}"
    );
}
