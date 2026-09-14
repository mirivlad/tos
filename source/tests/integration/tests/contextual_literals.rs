// SPDX-License-Identifier: GPL-3.0-or-later
//! An unsuffixed integer literal takes the type its position requires.
//!
//! `docs/40` section 3, and the registry's own wording for
//! `E1210_INTEGER_TYPE_MISMATCH`: "an unsuffixed literal takes the required
//! type instead". That is one rule, and until this file it was implemented
//! nowhere — the lowerer interned every unsuffixed integer as `i32` and the
//! contextual target was simply lost.
//!
//! **Why it had to be fixed rather than tolerated.** The call-operand check
//! added in `1cf4c74` compares an operand's type against the declared
//! parameter, as `docs/43` section 4 requires. With the literal pinned to `i32`,
//! `take(4)` against `fn take(v: i64)` was refused — valid source, rejected.
//! The alternative was to let the verifier accept an `i32` constant wherever an
//! integer was wanted, which would have made forged IR valid again: after the
//! old lowering, source `4` and source `4i32` were the same artifact, and
//! nothing downstream could tell a contextual literal from a wrong explicit
//! one. The distinction had to be restored where it is made.
//!
//! So the resolved type is represented by the constant itself. There is no flag
//! saying "this was unsuffixed": once the position has spoken, the canonical
//! derived IR holds `Constant::Int(I64, 4)` or `Constant::Size(4)`, and that is
//! the fact the verifier reads.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::Unreachable;
use tos_ir::{Constant, IntKind, Module};
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
    format!("module app.literals version 1.4 profile full; {ENVELOPE} {body}")
}

fn module_of(body: &str) -> Module {
    let text = text_of(body);
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
        source_set: String::from("tos-contextual-literal-tests"),
        path: String::from("app/literals.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    lower_module(&source, &schema, &context).expect("the fixture lowers")
}

/// Lowers, verifies and runs, reporting whichever stage refused.
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
        source_set: String::from("tos-contextual-literal-tests"),
        path: String::from("app/literals.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = match lower_module(&source, &schema, &context) {
        Ok(module) => module,
        Err(gap) => return format!("gap {}", gap.construct),
    };
    if let Err(finding) = verify(&module, &ResolutionSnapshot::default(), &Limits::default()) {
        return format!("verify {}", finding.code);
    }
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
    {
        Ok(outcome) => format!("{:?}", outcome.value),
        Err(trap) => format!("trap {}", trap.code),
    }
}

/// Every integer and size constant a lowered module interned.
fn constants_of(body: &str) -> Vec<Constant> {
    module_of(body)
        .constants
        .iter()
        .filter(|constant| matches!(constant, Constant::Int(_, _) | Constant::Size(_)))
        .cloned()
        .collect()
}

// ------------------------------------------- the regression `1cf4c74` created

/// An unsuffixed literal passed where `i64` is required.
///
/// **This is the case that was refused.** It is valid source, and before the
/// contextual resolution below it reached the verifier as an `i32` constant in
/// an `i64` parameter position and was rejected `V2010_TYPE`.
#[test]
fn an_unsuffixed_literal_reaches_an_i64_parameter() {
    assert_eq!(
        outcome(
            "fn take(v: i64) -> i64 { return v; } \
             pub fn main() -> i64 { return take(4); }"
        ),
        "Int(I64, 4)"
    );
}

/// The same against `u64`, where the signedness would also have been wrong.
#[test]
fn an_unsuffixed_literal_reaches_a_u64_parameter() {
    assert_eq!(
        outcome(
            "fn take(v: u64) -> u64 { return v; } \
             pub fn main() -> u64 { return take(4); }"
        ),
        "Int(U64, 4)"
    );
}

/// And against `size`, which is not an integer type at all.
#[test]
fn an_unsuffixed_literal_reaches_a_size_parameter() {
    assert_eq!(
        outcome(
            "fn take(v: size) -> bool { return v == 4B; } \
             pub fn main() -> bool { return take(4); }"
        ),
        "Bool(true)"
    );
}

/// An **explicit** `i32` against `i64` is still a mismatch, and still reported.
///
/// The contextual rule is about a literal that fixes no type of its own. A
/// suffix fixes one, and this is what distinguishes the two cases that the old
/// lowering had made identical in the artifact.
#[test]
fn an_explicit_i32_argument_is_still_refused_where_i64_is_required() {
    let reported = outcome(
        "fn take(v: i64) -> i64 { return v; } \
         pub fn main() -> i64 { return take(4i32); }",
    );
    assert_eq!(reported, "check [\"E1210_INTEGER_TYPE_MISMATCH\"]");
}

// ------------------------------------------------------ the other positions

/// An unsuffixed literal returned from an `i64` function is an `i64`.
#[test]
fn an_unsuffixed_return_takes_the_declared_result_type() {
    assert_eq!(outcome("pub fn main() -> i64 { return 4; }"), "Int(I64, 4)");
    assert_eq!(
        constants_of("pub fn main() -> i64 { return 4; }"),
        vec![Constant::Int(IntKind::I64, 4)]
    );
}

/// An unsuffixed initializer takes its binding's annotation.
///
/// `let x: size = 4;` used to store an `Int(I32, 4)` in a `size` binding, and
/// `x == 4B` was **false** — a silent wrong answer with no trap and no
/// diagnostic anywhere.
#[test]
fn an_unsuffixed_initializer_takes_its_annotation() {
    assert_eq!(
        outcome("pub fn main() -> bool { let x: size = 4; return x == 4B; }"),
        "Bool(true)"
    );
    assert_eq!(
        outcome("pub fn main() -> u64 { let x: u64 = 4; return x; }"),
        "Int(U64, 4)"
    );
}

/// **The artifact, not the answer.** A test that only ran the program could
/// pass while the constant table still said `i32`, because the engine compares
/// magnitudes. This reads the lowered module.
#[test]
fn the_resolved_type_is_in_the_constant_table() {
    assert_eq!(
        constants_of("pub fn main() -> bool { let x: size = 4; return x == 4B; }"),
        vec![Constant::Size(4)],
        "a size annotation interns a size constant, not an i32"
    );
    assert_eq!(
        constants_of(
            "fn take(v: u64) -> u64 { return v; } pub fn main() -> u64 { return take(7); }"
        ),
        vec![Constant::Int(IntKind::U64, 7)]
    );
    assert_eq!(
        constants_of("pub fn main() -> i8 { return 7; }"),
        vec![Constant::Int(IntKind::I8, 7)]
    );
}

/// A literal with no numeric expectation keeps the accepted default.
///
/// `docs/40` section 3 gives `i32`, and the contextual rule does not reach a
/// position that states no numeric type.
#[test]
fn an_unsuffixed_literal_with_no_expectation_is_i32() {
    assert_eq!(
        constants_of("pub fn main() -> i32 { let x = 4; return x; }"),
        vec![Constant::Int(IntKind::I32, 4)]
    );
    assert_eq!(
        outcome("pub fn main() -> i32 { let x = 4; return x; }"),
        "Int(I32, 4)"
    );
}

/// A suffixed literal ignores the position and keeps what it says.
#[test]
fn a_suffixed_literal_keeps_its_own_type() {
    assert_eq!(
        constants_of("pub fn main() -> i64 { return 4i64; }"),
        vec![Constant::Int(IntKind::I64, 4)]
    );
}

/// The range is checked **after** the contextual target is known, which is the
/// only order that can work: `200` is a `u8` and is not an `i8`.
#[test]
fn the_range_is_checked_against_the_contextual_type() {
    assert_eq!(
        constants_of("pub fn main() -> u8 { return 200; }"),
        vec![Constant::Int(IntKind::U8, 200)]
    );
    assert_eq!(
        outcome("pub fn main() -> i8 { return 200; }"),
        "gap integer literal out of range"
    );
    assert_eq!(
        outcome("pub fn main() -> i32 { return 4294967296i32; }"),
        "gap integer literal out of range"
    );
}
fn codes(body: &str) -> Vec<String> {
    let text = text_of(body);
    let source = SourceReader::read(text.as_bytes()).expect("ok");
    let Some(schema) = Parser::parse_schema(&source).into_accepted() else {
        return vec![String::from("parse refused")];
    };
    Checker::check(&source, &schema)
        .iter()
        .map(|d| d.code().to_string())
        .collect()
}

// ------------------------------------ the residual value-type diagnostic

/// An annotation constrains its initializer (ADR-0087).
///
/// Until the code existed, `let flag: bool = 1i64;` was accepted, lowered and
/// left for the engine. The checker had both types in hand at that exact point
/// and compared neither: `let bound = declared.unwrap_or(inferred);` — the
/// annotation simply won.
#[test]
fn an_annotation_that_its_initializer_does_not_satisfy_is_reported() {
    for body in [
        "pub fn main() -> i64 { let x: bool = 1i64; return 0i64; }",
        "pub fn main() -> i64 { let x: i64 = true; return 0i64; }",
        "pub fn main() -> i64 { let x: string = b\"a\"; return 0i64; }",
        "pub fn main() -> i64 { let x: array<i64, 2> = [true, false]; return 0i64; }",
    ] {
        assert_eq!(
            codes(body),
            vec![String::from("E1216_VALUE_TYPE_MISMATCH")],
            "{body}"
        );
    }
}

/// **The residual code stays residual** (ADR-0087 §2a).
///
/// Two of the eight exact integer types disagreeing is `E1210`'s by name — "a
/// value of one integer type is assigned … where a different integer type is
/// required" — and an initializer under an annotation is that assignment. If
/// this ever reports `E1216`, the residual code has begun absorbing the
/// specific ones.
#[test]
fn a_numeric_binding_mismatch_is_reported_under_the_numeric_code() {
    assert_eq!(
        codes("pub fn main() -> i64 { let x: i64 = 1i32; return 0i64; }"),
        vec![String::from("E1210_INTEGER_TYPE_MISMATCH")]
    );
}

/// And a contextual literal is not a mismatch at all.
///
/// Reporting these would contradict docs/40 §3 rather than complete the
/// registry, which is the mistake ADR-0087 §2a exists to forbid.
#[test]
fn a_contextual_literal_under_an_annotation_is_not_reported() {
    for body in [
        "pub fn main() -> i64 { let x: size = 4; return 0i64; }",
        "pub fn main() -> i64 { let x: i64 = 4; return 0i64; }",
        "pub fn main() -> i64 { let x: u64 = 4; return 0i64; }",
        "pub fn main() -> i64 { let x: u8 = 4; return 0i64; }",
        "pub fn main() -> i64 { let x: bool = true; return 0i64; }",
    ] {
        assert!(codes(body).is_empty(), "{body}: {:?}", codes(body));
    }
}

// --------------------------------- a projected place has one exact type

/// **A region's element is the type it is, and widening is written.**
///
/// This is the defect the typed-value slice found in canonical Stage 4 source:
///
/// ```text
/// region: DmaRegion<mut u8>
/// region[at] : u8
/// let status: u64 = region[at];      // u8 bound to u64, no conversion
/// ```
///
/// docs/40 §3 makes the eight integer types exact and gives `as` as the only
/// widening between them, so that binding is `E1210`'s. It went unreported
/// because the checker never compared a `let`'s annotation with its
/// initializer, and it went unnoticed at run time because the engine compares
/// integer magnitudes rather than kinds — the program answered correctly for a
/// reason that had nothing to do with being type-correct.
///
/// Nothing here is about VirtIO: the region is a parameter, and what is under
/// test is the projection.
#[test]
fn a_u8_region_element_does_not_bind_to_a_u64_without_widening() {
    assert_eq!(
        codes(
            "pub fn read(region: DmaRegion<mut u8>, at: size) -> u64 { \
                 let status: u64 = region[at]; return status; }"
        ),
        vec![String::from("E1210_INTEGER_TYPE_MISMATCH")]
    );
}

/// And written with the conversion, it is accepted.
#[test]
fn a_u8_region_element_binds_to_a_u64_when_widened() {
    assert!(codes(
        "pub fn read(region: DmaRegion<mut u8>, at: size) -> u64 { \
             let status: u64 = region[at] as u64; return status; }"
    )
    .is_empty());
}

/// The element's own type binds without a conversion, which is what makes the
/// case above about the widening rather than about regions.
#[test]
fn a_u8_region_element_binds_to_a_u8() {
    assert!(codes(
        "pub fn read(region: DmaRegion<mut u8>, at: size) -> u8 { \
             let status: u8 = region[at]; return status; }"
    )
    .is_empty());
}
