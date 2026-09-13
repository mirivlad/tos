// SPDX-License-Identifier: GPL-3.0-or-later
//! A call supplies an exact ordered operand list, and the verifier proves it.
//!
//! docs/43 section 4 states both halves:
//!
//! > A call names a declared imported or local function signature and supplies
//! > an exact ordered operand list; it cannot resolve a host symbol dynamically.
//!
//! and, of the verifier:
//!
//! > The verifier does not trust those claims. In particular, the verifier
//! > rechecks all table bounds/schema identity, nominal type references,
//! > control-flow targets, **operand types, call/effect signatures**, …
//!
//! **Until this file, `Op::CallValue` established only that its callee operand
//! had a function type.** Not how many operands it supplied, not what their
//! types were, not what result it declared. A value of a parameter's declared
//! type could therefore be absent, doubled or of another type entirely, and
//! every instruction after the call read the result slot under a static
//! assumption the run does not meet — without a `borrow mut` anywhere in sight.
//! Local calls were checked no further than that their function index was in
//! range.
//!
//! Two kinds of case, and they prove different things:
//!
//! - **from source**, so the obligation holds for what this frontend emits;
//! - **from forged IR**, because a verifier that only meets its own frontend's
//!   output proves nothing about a frontend somebody else wrote. Each forgery
//!   damages one thing in a module that verified a moment earlier, so what the
//!   refusal is about is the damage and not the fixture.
//!
//! **What is checked here is the call**, not every way a value can acquire a
//! type it does not have. A `let` whose annotation the initializer does not
//! satisfy still lowers to a move that relabels the place it names, and the
//! verifier does not yet refuse that — see the slice's report.
//!
//! **Parameter modes are not compared, and that is deliberate.** A function
//! type carries no `PassMode`, so `Owned`, `SharedBorrow` and `MutableBorrow`
//! are indistinguishable in a callable's type; that erasure is a recorded
//! contract question and is not decided here by implication.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_ir::{Module, Op, Operand, TypeDef};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 64KiB, tasks: 2, \
     workers: 2, sync: 0, shared: 0B, cleanup: 16, recursion: 16, imports: 0]";

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

/// Lowers a fixture the checker accepts, without verifying it.
fn lowered(body: &str) -> Module {
    let text = format!("module app.calls version 1.4 profile full; {ENVELOPE} {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        diagnostics.is_empty(),
        "the fixture checks: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let context = ModuleContext {
        source_set: String::from("tos-call-boundary-tests"),
        path: String::from("app/calls.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    lower_module(&source, &schema, &context).expect("the fixture lowers")
}

/// The verifier's verdict on a module, as a code.
fn verdict(module: &Module) -> String {
    match verify(module, &ResolutionSnapshot::default(), &Limits::default()) {
        Ok(_) => String::from("accepted"),
        Err(finding) => String::from(finding.code),
    }
}

fn verdict_of(body: &str) -> String {
    verdict(&lowered(body))
}

/// The first `Call` or `CallValue` of a module, as a position to damage.
fn first_call(module: &mut Module) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if matches!(instruction.op, Op::Call { .. } | Op::CallValue { .. }) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("the fixture contains a call");
}

/// Interns a type that is not already in the table and returns its id.
fn foreign_type(module: &mut Module) -> usize {
    let wanted = TypeDef::Int(tos_ir::IntKind::I8);
    match module.types.iter().position(|ty| *ty == wanted) {
        Some(found) => found,
        None => {
            module.types.push(wanted);
            module.types.len() - 1
        }
    }
}

// --------------------------------------------------------------- from source

const LOCAL: &str = "fn add(a: i64, b: i64) -> i64 { return a + b; } \
     pub fn main() -> i64 { return add(1i64, 2i64); }";

const VALUE: &str = "pub fn main() -> i64 { let f = fn (a: i64, b: i64) { }; \
     f(1i64, 2i64); return 0i64; }";

#[test]
fn a_correct_local_call_verifies() {
    assert_eq!(verdict_of(LOCAL), "accepted");
}

#[test]
fn a_correct_value_call_verifies() {
    assert_eq!(verdict_of(VALUE), "accepted");
}

#[test]
fn a_local_call_with_too_few_operands_is_refused() {
    assert_eq!(
        verdict_of(
            "fn add(a: i64, b: i64) -> i64 { return a + b; } \
             pub fn main() -> i64 { return add(1i64); }"
        ),
        "V2011_CFG"
    );
}

#[test]
fn a_local_call_with_too_many_operands_is_refused() {
    assert_eq!(
        verdict_of(
            "fn one(a: i64) -> i64 { return a; } \
             pub fn main() -> i64 { return one(1i64, 2i64); }"
        ),
        "V2011_CFG"
    );
}

#[test]
fn a_value_call_with_too_few_operands_is_refused() {
    assert_eq!(
        verdict_of(
            "pub fn main() -> i64 { let f = fn (a: i64, b: i64) { }; f(1i64); return 0i64; }"
        ),
        "V2011_CFG"
    );
}

#[test]
fn a_value_call_with_too_many_operands_is_refused() {
    assert_eq!(
        verdict_of("pub fn main() -> i64 { let f = fn (a: i64) { }; f(1i64, 2i64); return 0i64; }"),
        "V2011_CFG"
    );
}

#[test]
fn a_value_call_with_a_wrong_operand_type_is_refused() {
    assert_eq!(
        verdict_of("pub fn main() -> i64 { let f = fn (a: i64) { }; f(true); return 0i64; }"),
        "V2010_TYPE"
    );
}

/// A value call's result is the callee type's, and is checked as one.
///
/// The fixture feeds a `unit`-returning closure's result to a parameter of
/// another type; nothing in the frontend objected, and the verifier does.
#[test]
fn a_value_call_result_is_checked_against_the_function_type() {
    assert_eq!(
        verdict_of(
            "fn wants(b: bool) -> i64 { return 0i64; } \
             pub fn main() -> i64 { let f = fn (a: i64) { }; return wants(f(1i64)); }"
        ),
        "V2010_TYPE"
    );
}

/// An `async fn` call declares the result the callee's signature has.
///
/// docs/40 section 4 gives an `async fn` declared `-> T` the result `Task<T>`,
/// and the function is lowered with exactly that signature. A call site that
/// declared the written `T` instead would name a result slot of one type and
/// receive a task handle of another — which is what it did, until the check
/// below was added and found it.
#[test]
fn an_async_call_declares_the_task_its_callee_produces() {
    assert_eq!(
        verdict_of(
            "async fn produce() -> i64 { return 9i64; } \
             pub fn main() -> i64 { match (await produce()) { \
             Completed(value) => { return value; } Cancelled => { return 0i64; } } }"
        ),
        "accepted"
    );
}

/// An array literal's element type is the element's, not the enclosing
/// function's result.
///
/// The builder could not type a constant operand and substituted the function's
/// result type, so `[0u64, 0u64]` inside a function returning `i64` produced
/// `array<i64, 2>`. The binding it initialised was declared `array<u64, 2>`,
/// and the artifact then held an aggregate and a place that disagreed.
#[test]
fn an_array_literal_of_constants_takes_its_elements_type() {
    assert_eq!(
        verdict_of(
            "pub fn main() -> i64 { let pool: array<u64, 2> = [0u64, 0u64]; \
             if (pool[0B] == 0u64) { return 0i64; } return 1i64; }"
        ),
        "accepted"
    );
}

/// A closure or spawned body returning a name it captured keeps that name's
/// type, rather than falling back to `unit`.
#[test]
fn a_captured_name_gives_a_body_its_result_type() {
    assert_eq!(
        verdict_of(
            "pub fn main() -> i64 { let count = 7i64; \
             let by_copy: fn () -> i64 = fn () { return count; }; \
             return count; }"
        ),
        "accepted"
    );
}

// ---------------------------------------------------------------- forged IR

/// Dropping an operand from a verified module's call is refused.
#[test]
fn forged_local_call_with_too_few_operands_is_refused() {
    let mut module = lowered(LOCAL);
    assert_eq!(verdict(&module), "accepted");
    let (f, b, i) = first_call(&mut module);
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands.pop();
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

/// Repeating one is refused too: an exact list is exact in both directions.
#[test]
fn forged_local_call_with_too_many_operands_is_refused() {
    let mut module = lowered(LOCAL);
    let (f, b, i) = first_call(&mut module);
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        let last = operands[operands.len() - 1].clone();
        operands.push(last);
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

/// An operand of another type is refused, with no source that could write it.
#[test]
fn forged_local_call_with_a_wrong_operand_type_is_refused() {
    let mut module = lowered(LOCAL);
    let foreign = foreign_type(&mut module);
    let (f, b, i) = first_call(&mut module);
    module.functions[f].values.push(foreign);
    let slot = module.functions[f].values.len() - 1;
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands[0] = Operand::Value(slot);
    }
    assert_eq!(verdict(&module), "V2010_TYPE");
}

#[test]
fn forged_value_call_with_too_few_operands_is_refused() {
    let mut module = lowered(VALUE);
    assert_eq!(verdict(&module), "accepted");
    let (f, b, i) = first_call(&mut module);
    if let Op::CallValue { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands.pop();
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

#[test]
fn forged_value_call_with_too_many_operands_is_refused() {
    let mut module = lowered(VALUE);
    let (f, b, i) = first_call(&mut module);
    if let Op::CallValue { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        let last = operands[operands.len() - 1].clone();
        operands.push(last);
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

#[test]
fn forged_value_call_with_a_wrong_operand_type_is_refused() {
    let mut module = lowered(VALUE);
    let foreign = foreign_type(&mut module);
    let (f, b, i) = first_call(&mut module);
    module.functions[f].values.push(foreign);
    let slot = module.functions[f].values.len() - 1;
    if let Op::CallValue { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands[0] = Operand::Value(slot);
    }
    assert_eq!(verdict(&module), "V2010_TYPE");
}

/// A value call declaring a result the function type does not name.
///
/// The instruction's type and its result slot are changed together, so the
/// module stays self-consistent everywhere except the one place this is about:
/// the call claims a result its callee does not produce.
#[test]
fn forged_value_call_with_a_wrong_result_type_is_refused() {
    let mut module = lowered(VALUE);
    let foreign = foreign_type(&mut module);
    let (f, b, i) = first_call(&mut module);
    let instruction = &mut module.functions[f].blocks[b].instructions[i];
    instruction.ty = foreign;
    let result = instruction.result.expect("the call defines a value");
    module.functions[f].values[result] = foreign;
    assert_eq!(verdict(&module), "V2010_TYPE");
}

/// And a local call claiming someone else's result.
#[test]
fn forged_local_call_with_a_wrong_result_type_is_refused() {
    let mut module = lowered(LOCAL);
    let foreign = foreign_type(&mut module);
    let (f, b, i) = first_call(&mut module);
    let instruction = &mut module.functions[f].blocks[b].instructions[i];
    instruction.ty = foreign;
    let result = instruction.result.expect("the call defines a value");
    module.functions[f].values[result] = foreign;
    assert_eq!(verdict(&module), "V2010_TYPE");
}
