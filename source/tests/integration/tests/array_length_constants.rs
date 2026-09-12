// SPDX-License-Identifier: GPL-3.0-or-later
//! `array<T, N>` takes the `const_expression` the accepted grammar gives it.
//!
//! docs/39 writes the production out:
//!
//! ```text
//! array_type       = "array" "<" type "," const_expression ">" ;
//! const_expression = const_sum ;
//! const_sum        = const_product ( ( "+" | "-" ) const_product )* ;
//! const_product    = const_primary ( ( "*" | "/" | "%" ) const_primary )* ;
//! const_primary    = integer | size | identifier | "(" const_expression ")" ;
//! ```
//!
//! and ADR-0052 §"What V1 already decided" reads the `identifier` of that
//! production: "V1 grants no user generics, so there is nothing else a name in
//! that position could denote", which is the argument the whole ADR is built on.
//! docs/40 section 2 says the same thing from the other side — "This is what
//! lets `array<T, N>` take a named constant as its compile-time `size`".
//!
//! **Why this file exists.** The parser accepted a literal and nothing else,
//! reporting `E1104_EXPECTED_LITERAL` for a name, and no gate noticed: the
//! contract gates check that documents *say* what they should, and the language
//! corpus had no fixture that wrote a named constant as an array's length. A
//! rule stated in three accepted documents and compiled in none of them is a
//! rule that is not implemented, and this is the coverage that makes the
//! difference visible.
//!
//! The length still reaches the artifact as a resolved number: ADR-0052 makes a
//! constant *be* its value, so `array<T, COUNT>` and `array<T, 4>` of the same
//! count are one type in `tos-ir/v1`, one entry in the type table, and the same
//! bytes in the module's identity.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::{Unreachable, Value};
use tos_ir::{IntKind, Module, TypeDef};
use tos_pipeline::{Prepared, ResidencyLimits};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 64KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 16, imports: 0]";

const RESIDENCY: ResidencyLimits = ResidencyLimits {
    modules: 8,
    bytes: 64 * 1024 * 1024,
};

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

fn text_of(body: &str) -> String {
    format!("module app.sized version 1.4 profile full; {ENVELOPE} {body}")
}

/// What the pipeline did with a fixture, as one string: a parse refusal, the
/// checker's codes, the lowering gap, or the value `main` returned.
fn outcome(body: &str) -> String {
    let text = text_of(body);
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let Some(schema) = Parser::parse_schema(&source).into_accepted() else {
        return String::from("parse refused");
    };
    let diagnostics = Checker::check(&source, &schema);
    if !diagnostics.is_empty() {
        return format!(
            "check {:?}",
            diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
        );
    }
    let context = ModuleContext {
        source_set: String::from("tos-array-length-tests"),
        path: String::from("app/sized.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module: Module = match lower_module(&source, &schema, &context) {
        Ok(module) => module,
        Err(gap) => return format!("gap {}", gap.construct),
    };
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the lowered IR verifies");
    let mut prepared = Prepared::launch(
        &[&module],
        &ResolutionSnapshot::default(),
        "main",
        RESIDENCY,
    )
    .expect("the fixture launches");
    match prepared
        .run(Vec::new(), &mut Unreachable)
        .expect("the entry exists")
        .expect("the program does not trap")
        .value
    {
        Value::Int(IntKind::I64, value) => format!("value {value}"),
        other => format!("value {other:?}"),
    }
}

/// The array lengths a lowered module's type table holds, smallest first.
fn array_lengths(body: &str) -> Vec<u64> {
    let text = text_of(body);
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    assert!(Checker::check(&source, &schema).is_empty());
    let context = ModuleContext {
        source_set: String::from("tos-array-length-tests"),
        path: String::from("app/sized.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("the fixture lowers");
    let mut lengths: Vec<u64> = module
        .types
        .iter()
        .filter_map(|ty| match ty {
            TypeDef::Array(_, count) => Some(*count),
            _ => None,
        })
        .collect();
    lengths.sort_unstable();
    lengths
}

const USES_FOUR: &str = "pub fn main() -> i64 { \
     let pool: array<i64, COUNT> = [1i64, 2i64, 3i64, 4i64]; return pool[3B]; }";

// ------------------------------------------------------------------- positive

/// A named `size` constant, which is the form docs/40 section 2 names.
#[test]
fn a_named_size_constant_is_an_array_length() {
    assert_eq!(
        outcome(&format!("const COUNT: size = 4B; {USES_FOUR}")),
        "value 4"
    );
}

/// A named constant written without a unit.
#[test]
fn a_named_constant_without_a_unit_is_an_array_length() {
    assert_eq!(
        outcome(&format!("const COUNT: size = 4; {USES_FOUR}")),
        "value 4"
    );
}

/// A constant that names another constant, which `const_primary`'s `identifier`
/// admits at any depth.
#[test]
fn a_constant_may_name_another_constant() {
    assert_eq!(
        outcome(&format!(
            "const BASE: size = 2B; const COUNT: size = BASE + BASE; {USES_FOUR}"
        )),
        "value 4"
    );
}

/// Arithmetic in the length position — `const_sum` and `const_product`.
#[test]
fn the_length_position_takes_arithmetic() {
    assert_eq!(
        outcome(
            "const BASE: size = 2B; \
             pub fn main() -> i64 { let pool: array<i64, BASE * 2> = [1i64, 2i64, 3i64, 4i64]; \
             return pool[3B]; }"
        ),
        "value 4"
    );
}

/// Parentheses, which `const_primary`'s fourth alternative admits.
#[test]
fn the_length_position_takes_parentheses() {
    assert_eq!(
        outcome(
            "const ONE: size = 1B; \
             pub fn main() -> i64 { let pool: array<i64, (ONE + 1B) * 2> = \
             [1i64, 2i64, 3i64, 4i64]; return pool[3B]; }"
        ),
        "value 4"
    );
}

/// Every operator the grammar gives `const_product`.
#[test]
fn the_length_position_takes_every_const_operator() {
    for (expression, length) in [
        ("2B + 2B", 4),
        ("6B - 2B", 4),
        ("2B * 2B", 4),
        ("8B / 2B", 4),
        ("9B % 5B", 4),
    ] {
        assert_eq!(
            outcome(&format!(
                "pub fn main() -> i64 {{ let pool: array<i64, {expression}> = \
                 [1i64, 2i64, 3i64, 4i64]; return pool[3B]; }}"
            )),
            format!("value {length}"),
            "{expression}"
        );
    }
}

/// A size constant with a unit larger than a byte still denotes its count.
#[test]
fn a_unit_bearing_size_constant_denotes_its_count() {
    let lengths = array_lengths(
        "const PAGE: size = 1KiB; \
         pub fn main() -> i64 { let pool: array<i64, PAGE> = [0i64]; return pool[0B]; }",
    );
    assert!(lengths.contains(&1024), "{lengths:?}");
}

// ----------------------------------------------------- the identity obligation

/// **The length reaches the artifact as a number, not as a name.**
///
/// A constant *is* its value (ADR-0052), so the two modules below intern one
/// and the same `TypeDef::Array(element, 4)`. If the length were carried
/// symbolically, renaming the constant would change the type table and with it
/// the module digest — and verification and provenance would both depend on a
/// spelling.
#[test]
fn a_named_length_and_a_written_one_lower_to_the_same_type() {
    let named = array_lengths(&format!("const COUNT: size = 4B; {USES_FOUR}"));
    let written = array_lengths(
        "pub fn main() -> i64 { let pool: array<i64, 4> = [1i64, 2i64, 3i64, 4i64]; \
         return pool[3B]; }",
    );
    assert_eq!(named, written);
    assert_eq!(named, vec![4]);
}

/// And renaming the constant changes nothing in the artifact.
#[test]
fn renaming_the_constant_does_not_change_the_lowered_length() {
    let first = array_lengths(&format!("const COUNT: size = 4B; {USES_FOUR}"));
    let second = array_lengths(
        "const WINDOW: size = 4B; \
         pub fn main() -> i64 { let pool: array<i64, WINDOW> = [1i64, 2i64, 3i64, 4i64]; \
         return pool[3B]; }",
    );
    assert_eq!(first, second);
}

// ------------------------------------------------------------------- negative

/// A name that is not a module constant.
///
/// V1 grants no user generics, so there is nothing else this could be, and the
/// answer is a refusal rather than a guess.
#[test]
fn a_length_naming_nothing_is_refused() {
    assert_eq!(
        outcome(
            "pub fn main() -> i64 { let pool: array<i64, MISSING> = [1i64]; return pool[0B]; }"
        ),
        "gap array length naming no module constant"
    );
}

/// A name that is a binding rather than a constant is not a length either: the
/// length is a **compile-time** value and a binding has none.
#[test]
fn a_length_naming_a_local_binding_is_refused() {
    assert_eq!(
        outcome(
            "pub fn main() -> i64 { let count: size = 4B; \
             let pool: array<i64, count> = [1i64]; return pool[0B]; }"
        ),
        "gap array length naming no module constant"
    );
}

/// A literal that is not a count. `const_primary` admits `integer` and `size`,
/// and a string is neither — so it is refused where it stands, with the code
/// this position has always reported.
#[test]
fn a_length_that_is_a_string_literal_is_refused() {
    assert_eq!(
        outcome(
            "pub fn main() -> i64 { let pool: array<i64, \"four\"> = [1i64]; return pool[0B]; }"
        ),
        "parse refused"
    );
}

/// A boolean is not a count either.
#[test]
fn a_length_that_is_a_boolean_is_refused() {
    assert_eq!(
        outcome("pub fn main() -> i64 { let pool: array<i64, true> = [1i64]; return pool[0B]; }"),
        "parse refused"
    );
}

/// A constant whose initializer is not a count.
#[test]
fn a_length_naming_a_constant_that_is_not_a_count_is_refused() {
    assert_eq!(
        outcome(
            "const NAME: string = \"four\"; \
             pub fn main() -> i64 { let pool: array<i64, NAME> = [1i64]; return pool[0B]; }"
        ),
        "gap array length that is not a count"
    );
}

/// Division by zero in a length. The arithmetic is checked like every other
/// arithmetic the language performs (docs/40 section 3).
#[test]
fn a_length_divided_by_zero_is_refused() {
    assert_eq!(
        outcome(
            "const ZERO: size = 0B; \
             pub fn main() -> i64 { let pool: array<i64, 4B / ZERO> = [1i64]; return pool[0B]; }"
        ),
        "gap array length that does not evaluate"
    );
}

/// A length that would go below zero.
#[test]
fn a_length_below_zero_is_refused() {
    assert_eq!(
        outcome(
            "pub fn main() -> i64 { let pool: array<i64, 1B - 4B> = [1i64]; return pool[0B]; }"
        ),
        "gap array length that does not evaluate"
    );
}

/// A length that would overflow the count.
#[test]
fn a_length_that_overflows_is_refused() {
    assert_eq!(
        outcome(
            "const HUGE: size = 18446744073709551615; \
             pub fn main() -> i64 { let pool: array<i64, HUGE * 2> = [1i64]; return pool[0B]; }"
        ),
        "gap array length that does not evaluate"
    );
}

/// A constant that names itself has no value to substitute.
#[test]
fn a_constant_cycle_in_a_length_is_refused() {
    assert_eq!(
        outcome(
            "const A: size = B; const B: size = A; \
             pub fn main() -> i64 { let pool: array<i64, A> = [1i64]; return pool[0B]; }"
        ),
        "gap constant cycle"
    );
}

/// **An imported constant cannot be written here, and the grammar is why.**
///
/// `const_primary = integer | size | identifier | "(" const_expression ")"` has
/// no dotted form, so `other.COUNT` is not a `const_expression` at all and the
/// parse ends where the `.` is. docs/40 section 2 describes a constant's
/// *initializer* as admitting "a `const_expression` whose `identifier` names
/// another constant, of this module or an imported one", which is a wider claim
/// than the production it names — and reconciling the two is a normative
/// question rather than a parser's to settle. This records what the accepted
/// grammar admits today; it is not an argument that the wider reading is wrong.
#[test]
fn a_dotted_name_is_not_a_const_expression() {
    assert_eq!(
        outcome(
            "pub fn main() -> i64 { let pool: array<i64, other.COUNT> = [1i64]; \
             return pool[0B]; }"
        ),
        "parse refused"
    );
}
