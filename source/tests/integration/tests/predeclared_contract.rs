// SPDX-License-Identifier: GPL-3.0-or-later
//! One authority decides every predeclared call (ADR-0088).
//!
//! `docs/39` §2 fixes a closed namespace of twenty-two predeclared functions.
//! Before this file, each component that met one of those calls knew a
//! different part of the contract and no component knew all of it:
//!
//! - the checker typed `to_u8`'s result and looked at no argument, so
//!   `to_u8(true)`, `to_u8()` and `to_u8(1u64, 2u64)` were all accepted;
//! - the lowerer kept a second list of the same names, for the same purpose,
//!   with its own idea of which produced what;
//! - the language-minor gate kept a third, and the one written for ADR-0081
//!   matched nothing at all for two minors without anyone noticing, because the
//!   type half of the same gate caught every module that could reach it;
//! - and the independent verifier held none — `CallTarget::Predeclared` was an
//!   empty match arm, so a forged artifact could name any operation with any
//!   operands and claim any result.
//!
//! Now there is one table. The frontend reads it for checking and for lowering;
//! the verifier reads it and answers every question from the artifact's own
//! type table; and `docs/43` §5's condition for a shared declarative table —
//! "its content digest is input to both components" — is met by each component
//! stating the digest it was built against and proving it here.
//!
//! **What the table does not do** is decide semantics that belong to a
//! dedicated operation. It says `share` takes a Shareable operand and produces
//! `Shared<T>`; ADR-0037's region traversal still decides what is shareable. It
//! says a device access takes an `MmioRegion` and a `size` offset; ADR-0081 §8's
//! own operation family still owns the access itself.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_ir::{CallTarget, Module, Op, Operand, TypeDef};
use tos_predeclared::{Contract, Form, Lookup, ACCEPTED_DIGESTS, OPERATIONS};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 64KiB, tasks: 2, \
     workers: 2, sync: 0, shared: 0B, cleanup: 16, recursion: 16, imports: 0]";

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

fn module_text(body: &str) -> String {
    format!("module app.predeclared version 1.4 profile full; {ENVELOPE} {body}")
}

/// The first diagnostic a fixture receives, as a code, or `"accepted"`.
fn checked(body: &str) -> String {
    let text = module_text(body);
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    match Checker::check(&source, &schema).first() {
        None => String::from("accepted"),
        Some(diagnostic) => String::from(diagnostic.code()),
    }
}

/// Lowers a fixture the checker accepts, without verifying it.
fn lowered(body: &str) -> Module {
    let text = module_text(body);
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
        source_set: String::from("tos-predeclared-tests"),
        path: String::from("app/predeclared.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    lower_module(&source, &schema, &context).expect("the fixture lowers")
}

fn verdict(module: &Module) -> String {
    match verify(module, &ResolutionSnapshot::default(), &Limits::default()) {
        Ok(_) => String::from("accepted"),
        Err(finding) => String::from(finding.code),
    }
}

fn verdict_of(body: &str) -> String {
    verdict(&lowered(body))
}

// ------------------------------------------------------- the shared contract

/// **The digest this component was built against**, stated here and not
/// imported from the table, because a constant read out of the thing it is
/// meant to pin would pin nothing. `docs/43` §5 permits a shared declarative
/// table only if its content digest is an input to both components; this is the
/// frontend-and-engine side of that input, and `tos-verifier` states the same
/// values independently in its own tests.
const CONTRACT_DIGESTS: [(u32, &str); 5] = [
    (
        0,
        "849bd9de6a7c0c485ffbcc972791a5768c724a4165d67087fd1961e863f7f555",
    ),
    (
        1,
        "26848cb863d7727c9b6787e141da156df418ea57af0471863a5767fe9f170bb3",
    ),
    (
        2,
        "c5e8ffbb3467768e15a235b57c56bdde1a359cb707ad5f33ee45f721b357c2a5",
    ),
    (
        3,
        "a4bad47aac6cdb361ced38352fde05acfedb47d4a6c331b8cd4762fd3daec02c",
    ),
    (
        4,
        "9d496d76b8e8b25157547fe6a5d81fb3c32c0dc7f331d3e19779ee1e2271a17c",
    ),
];

#[test]
fn the_contract_content_is_the_one_this_component_was_built_against() {
    for (minor, expected) in CONTRACT_DIGESTS {
        let contract = Contract::at_minor(minor);
        let hex = String::from_utf8(tos_predeclared::hex_digest(&contract).to_vec())
            .expect("hex is ASCII");
        assert_eq!(hex, expected, "TOS Core 1.{minor}");
    }
    assert_eq!(CONTRACT_DIGESTS, ACCEPTED_DIGESTS);
}

/// The table is exactly `docs/39` §2's `predeclared-function` inventory.
///
/// Read out of the document rather than restated, so the two cannot drift: the
/// inventory is machine-readable on purpose and
/// `scripts/check-stage2-language-contract.py` already binds it to the EBNF.
#[test]
fn the_table_is_the_documented_namespace() {
    let grammar = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md"
    ))
    .expect("docs/39 is readable");
    let line = grammar
        .lines()
        .find(|line| line.starts_with("predeclared-function: "))
        .expect("the inventory names the predeclared functions");
    let documented: Vec<&str> = line
        .trim_start_matches("predeclared-function: ")
        .split_whitespace()
        .collect();
    let table: Vec<&str> = OPERATIONS.iter().map(|operation| operation.name).collect();
    assert_eq!(table, documented);
}

/// Each minor admits exactly the operations its decision added.
#[test]
fn a_minor_receives_the_language_its_header_claims() {
    assert!(matches!(
        Contract::at_minor(1).lookup("mmio_read_u8"),
        Lookup::RequiresMinor(_)
    ));
    assert!(matches!(
        Contract::at_minor(2).lookup("mmio_read_u8"),
        Lookup::Available(_)
    ));
    assert!(matches!(
        Contract::at_minor(3).lookup("dma_publish"),
        Lookup::RequiresMinor(_)
    ));
    assert!(matches!(
        Contract::at_minor(4).lookup("dma_publish"),
        Lookup::Available(_)
    ));
}

// ------------------------------------------------------------- from source

#[test]
fn a_correct_conversion_verifies_with_its_result_type() {
    assert_eq!(
        verdict_of(
            "pub fn main() -> i64 { match (to_u8(300u64)) { \
             Ok(small) => { return 1i64; } Err(e) => { return 0i64; } } }"
        ),
        "accepted"
    );
}

#[test]
fn a_correct_wrapping_operation_verifies_with_its_operands_type() {
    assert_eq!(
        verdict_of("pub fn main() -> u64 { return wrapping_add(1u64, 2u64); }"),
        "accepted"
    );
}

/// **The second operand takes the first's type**, which is what the contract's
/// `SameAs` rule states and what `docs/40` §3's contextual rule then does with
/// an unsuffixed literal. Without the rule the literal took the `i32` default
/// and the verifier refused a call the checker had accepted.
#[test]
fn a_wrapping_operations_literal_takes_the_other_operands_type() {
    let module = lowered("pub fn main() -> u64 { return wrapping_add(1u64, 2); }");
    assert_eq!(verdict(&module), "accepted");
    let kinds: Vec<&tos_ir::Constant> = module.constants.iter().collect();
    assert!(
        kinds
            .iter()
            .any(|constant| matches!(constant, tos_ir::Constant::Int(tos_ir::IntKind::U64, 2))),
        "the literal is a u64: {kinds:?}"
    );
}

/// A device access offset written without a suffix is `size`, not `i32`.
#[test]
fn a_device_offset_literal_takes_the_contracts_size() {
    let module = lowered(
        "pub fn read(window: MmioRegionMut) -> u64 { \
         mmio_write_le_u32(window, 4, 7); return mmio_read_le_u64(window, 0); }",
    );
    assert_eq!(verdict(&module), "accepted");
    assert!(
        module
            .constants
            .iter()
            .any(|constant| matches!(constant, tos_ir::Constant::Size(4))),
        "the offset is a size: {:?}",
        module.constants
    );
    assert!(
        module
            .constants
            .iter()
            .any(|constant| matches!(constant, tos_ir::Constant::Int(tos_ir::IntKind::U64, 7))),
        "the written value is a u64: {:?}",
        module.constants
    );
}

#[test]
fn a_conversion_with_no_argument_is_an_arity_mismatch() {
    assert_eq!(
        checked("pub fn main() -> i64 { to_u8(); return 0i64; }"),
        "E1217_CALL_ARITY_MISMATCH"
    );
}

#[test]
fn a_conversion_with_two_arguments_is_an_arity_mismatch() {
    assert_eq!(
        checked("pub fn main() -> i64 { to_u8(1u64, 2u64); return 0i64; }"),
        "E1217_CALL_ARITY_MISMATCH"
    );
}

#[test]
fn a_wrapping_operation_with_one_argument_is_an_arity_mismatch() {
    assert_eq!(
        checked("pub fn main() -> i64 { wrapping_add(1u64); return 0i64; }"),
        "E1217_CALL_ARITY_MISMATCH"
    );
}

#[test]
fn a_conversion_of_something_that_is_not_an_integer_is_refused() {
    assert_eq!(
        checked("pub fn main() -> i64 { to_u8(true); return 0i64; }"),
        "E1215_ARGUMENT_TYPE_MISMATCH"
    );
}

/// Two exact integer types disagreeing is `E1210`, which is more specific than
/// the residual argument code and keeps it.
#[test]
fn a_wrapping_operation_over_two_integer_types_is_the_numeric_code() {
    assert_eq!(
        checked("pub fn main() -> i64 { wrapping_add(1u64, 2i32); return 0i64; }"),
        "E1210_INTEGER_TYPE_MISMATCH"
    );
}

/// A device offset that is not `size` is the index code, exactly as an array
/// index is: it is the same kind of position.
#[test]
fn a_device_offset_of_the_wrong_type_is_the_index_code() {
    assert_eq!(
        checked(
            "pub fn read(window: MmioRegionMut) -> u64 { \
             return mmio_read_le_u64(window, 1i64); }"
        ),
        "E1211_INDEX_TYPE_MISMATCH"
    );
}

/// A device write through a read-only mapping is refused in source.
#[test]
fn a_device_write_through_a_read_only_mapping_is_refused() {
    assert_eq!(
        checked(
            "pub fn write(window: MmioRegion) -> unit { \
             mmio_write_u8(window, 0B, 1u64); }"
        ),
        "E1215_ARGUMENT_TYPE_MISMATCH"
    );
}

/// **A predeclared name is an ordinary identifier** (`docs/39` §2). A module
/// that declares `fn share` has declared a function, and the call resolves to
/// it — which is how the lowerer has always resolved it, and how the checker
/// now does too.
#[test]
fn a_declared_function_of_a_predeclared_name_is_the_one_called() {
    assert_eq!(
        verdict_of(
            "fn share(value: i64) -> i64 { return value; } \
             pub fn main() -> i64 { return share(7i64); }"
        ),
        "accepted"
    );
}

/// And its arity is the declared function's, not the operation's.
#[test]
fn a_declared_function_of_a_predeclared_name_keeps_its_own_arity() {
    assert_eq!(
        checked(
            "fn share(a: i64, b: i64) -> i64 { return a; } \
             pub fn main() -> i64 { return share(7i64); }"
        ),
        "E1217_CALL_ARITY_MISMATCH"
    );
}

// --------------------------------------------------- the language-minor gate

fn checked_at_minor(minor: u32, body: &str) -> String {
    let text = format!("module app.predeclared version 1.{minor} profile full; {ENVELOPE} {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    match Checker::check(&source, &schema).first() {
        None => String::from("accepted"),
        Some(diagnostic) => String::from(diagnostic.code()),
    }
}

#[test]
fn a_dma_ordering_point_below_its_minor_is_refused() {
    assert_eq!(
        checked_at_minor(
            3,
            "pub fn order(area: DmaRegion<mut u64>) -> unit { dma_publish(area); }"
        ),
        "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR"
    );
    assert_eq!(
        checked_at_minor(
            4,
            "pub fn order(area: DmaRegion<mut u64>) -> unit { dma_publish(area); }"
        ),
        "accepted"
    );
}

// ---------------------------------------------------------------- forged IR

const CONVERSION: &str =
    "pub fn main() -> i64 { match (to_u8(300u64)) { Ok(small) => { return 1i64; } \
     Err(e) => { return 0i64; } } }";

const WRAPPING: &str = "pub fn main() -> u64 { return wrapping_add(1u64, 2u64); }";

/// The first predeclared `Call`, as a position to damage.
fn first_predeclared(module: &Module) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if let Op::Call {
                    target: CallTarget::Predeclared(_),
                    ..
                } = &instruction.op
                {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("the fixture contains a predeclared call");
}

/// Interns a type that is not already in the table and returns its id.
fn foreign_type(module: &mut Module) -> usize {
    let wanted = TypeDef::Bool;
    match module.types.iter().position(|ty| *ty == wanted) {
        Some(found) => found,
        None => {
            module.types.push(wanted);
            module.types.len() - 1
        }
    }
}

#[test]
fn a_forged_call_to_a_name_no_operation_has_is_refused() {
    let mut module = lowered(CONVERSION);
    assert_eq!(verdict(&module), "accepted");
    let (f, b, i) = first_predeclared(&module);
    if let Op::Call { target, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        *target = CallTarget::Predeclared(String::from("checked_add"));
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

/// **An operation with its own IR form may not be written as a call.** This is
/// the forgery `docs/43` §3 is about: an opaque call in place of the operation
/// a reader of the artifact is entitled to see.
#[test]
fn a_forged_call_naming_an_operation_with_its_own_form_is_refused() {
    let mut module = lowered(CONVERSION);
    let (f, b, i) = first_predeclared(&module);
    if let Op::Call { target, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        *target = CallTarget::Predeclared(String::from("dma_publish"));
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

#[test]
fn a_forged_predeclared_call_with_a_dropped_operand_is_refused() {
    let mut module = lowered(WRAPPING);
    assert_eq!(verdict(&module), "accepted");
    let (f, b, i) = first_predeclared(&module);
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands.pop();
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

#[test]
fn a_forged_predeclared_call_with_an_extra_operand_is_refused() {
    let mut module = lowered(WRAPPING);
    let (f, b, i) = first_predeclared(&module);
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        let last = operands[operands.len() - 1].clone();
        operands.push(last);
    }
    assert_eq!(verdict(&module), "V2011_CFG");
}

/// An operand of a type the rule does not admit — here a `bool` where the
/// conversion requires an exact integer or `size`.
#[test]
fn a_forged_predeclared_call_with_a_wrong_operand_type_is_refused() {
    let mut module = lowered(CONVERSION);
    let foreign = foreign_type(&mut module);
    let (f, b, i) = first_predeclared(&module);
    module.functions[f].values.push(foreign);
    let slot = module.functions[f].values.len() - 1;
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands[0] = Operand::Value(slot);
    }
    assert_eq!(verdict(&module), "V2010_TYPE");
}

/// **"The same exact integer type" is two requirements.** A `wrapping_add`
/// whose operands are both integers but of different types is refused by the
/// relational rule, which nothing but a derived comparison could catch.
#[test]
fn a_forged_wrapping_operation_over_two_integer_types_is_refused() {
    let mut module = lowered(WRAPPING);
    let other = match module
        .types
        .iter()
        .position(|ty| *ty == TypeDef::Int(tos_ir::IntKind::I64))
    {
        Some(found) => found,
        None => {
            module.types.push(TypeDef::Int(tos_ir::IntKind::I64));
            module.types.len() - 1
        }
    };
    let (f, b, i) = first_predeclared(&module);
    module.functions[f].values.push(other);
    let slot = module.functions[f].values.len() - 1;
    if let Op::Call { operands, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        operands[1] = Operand::Value(slot);
    }
    assert_eq!(verdict(&module), "V2010_TYPE");
}

/// A call claiming a result the operation does not produce. The instruction's
/// type and its result slot move together, so the module stays self-consistent
/// everywhere except the one place this is about.
#[test]
fn a_forged_predeclared_call_with_a_wrong_result_type_is_refused() {
    let mut module = lowered(CONVERSION);
    let foreign = foreign_type(&mut module);
    let (f, b, i) = first_predeclared(&module);
    let instruction = &mut module.functions[f].blocks[b].instructions[i];
    instruction.ty = foreign;
    let result = instruction.result.expect("the call defines a value");
    module.functions[f].values[result] = foreign;
    assert_eq!(verdict(&module), "V2010_TYPE");
}

/// **The declared minor is a verifier obligation too.** The artifact below is
/// correct in every other way; what is wrong with it is that its header claims
/// a language the operation it performs does not belong to.
///
/// Reached by moving a 1.4 artifact's header back, because no frontend will
/// produce one: `E1608` refuses the source. That is the point — a hand-written
/// artifact never met the frontend that would have refused it.
///
/// **This exercises the obligation at the operation family that can express
/// it, and the predeclared-*call* branch's own minor check is not reachable
/// today.** Every operation the contract gives `Form::Call` — the eight
/// conversions and the three wrapping contracts — exists at TOS Core 1.0, so no
/// artifact can name one above its minor. The check is written because a later
/// minor that adds a call-form operation would otherwise add it with no gate,
/// which is exactly how ADR-0086's source gate came to exist and match nothing.
#[test]
fn a_forged_artifact_using_an_operation_above_its_minor_is_refused() {
    let mut module =
        lowered("pub fn order(area: DmaRegion<mut u64>) -> unit { dma_publish(area); }");
    assert_eq!(verdict(&module), "accepted");
    module.header.language_version = String::from("1.3");
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// Every operation the table names, with its `Form`, is what the frontend
/// actually emits for it.
///
/// A table that said `share` was a `Call` while the lowerer emitted `Op::Share`
/// would be an authority nobody followed; this is the check that they agree,
/// over the operations a fixture can reach.
#[test]
fn the_lowered_form_is_the_form_the_contract_states() {
    let cases = [
        ("to_u8", CONVERSION),
        ("wrapping_add", WRAPPING),
        (
            "share",
            "pub fn take(area: Region<u64>) -> Shared<Region<u64>> { return share(area); }",
        ),
        (
            "mmio_read_le_u64",
            "pub fn read(window: MmioRegionMut) -> u64 { return mmio_read_le_u64(window, 0B); }",
        ),
        (
            "mmio_write_u8",
            "pub fn write(window: MmioRegionMut) -> unit { mmio_write_u8(window, 0B, 1u64); }",
        ),
        (
            "dma_publish",
            "pub fn order(area: DmaRegion<mut u64>) -> unit { dma_publish(area); }",
        ),
    ];
    for (name, body) in cases {
        let operation = tos_predeclared::operation(name).expect("the table names it");
        let module = lowered(body);
        let mut seen = false;
        for function in &module.functions {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    let matches = match (&instruction.op, operation.form) {
                        (
                            Op::Call {
                                target: CallTarget::Predeclared(called),
                                ..
                            },
                            Form::Call,
                        ) => called == name,
                        (Op::Share { .. }, Form::Share) => true,
                        (
                            Op::MmioRead {
                                width,
                                little_endian,
                                ..
                            },
                            Form::Mmio {
                                width: w,
                                writes: false,
                                little_endian: le,
                            },
                        ) => *width == w && *little_endian == le,
                        (
                            Op::MmioWrite {
                                width,
                                little_endian,
                                ..
                            },
                            Form::Mmio {
                                width: w,
                                writes: true,
                                little_endian: le,
                            },
                        ) => *width == w && *little_endian == le,
                        (Op::DmaSync { direction, .. }, Form::DmaSync(wanted)) => {
                            *direction == wanted
                        }
                        _ => false,
                    };
                    seen = seen || matches;
                }
            }
        }
        assert!(seen, "{name} did not lower to the form the contract states");
    }
}
