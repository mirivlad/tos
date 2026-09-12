// SPDX-License-Identifier: GPL-3.0-or-later
//! A mutable borrow preserves the place it borrowed.
//!
//! docs/40 section 2 makes the three parameter modes three different things:
//!
//! > Function parameters without `borrow` consume an owned argument unless its
//! > type is `Copy`. `borrow parameter: T` creates an immutable temporary
//! > borrow; `borrow mut parameter: T` creates an exclusive mutable temporary
//! > borrow.
//!
//! So `Copy` decides whether an **owned** parameter copies. It does not decide
//! what a mutable borrow is, and a mutable borrow of a `Copy` value is still a
//! borrow — the callee's writes are writes to the caller's place.
//!
//! **Why this file exists.** Until it did, the only end-to-end proof of a
//! write-back was `residency_execution.rs`'s
//! `a_mutable_borrow_writes_back_into_a_caller_that_was_evicted`, and it passes
//! the argument as a bare name — `mid.touch(cell)` — which lowers to the
//! caller's own value slot. The write-back landed there and the test was right.
//! Written the other way, `mid.touch(borrow mut cell)`, the argument lowers to
//! an `Op::Borrow` result: a *copy* of the place, in a fresh slot. The
//! write-back landed in that copy, nothing reached the caller, and no test in
//! the repository said a word — because every other borrow test proved either a
//! diagnostic or a handle-backed object, whose handle still reaches the same
//! host object however many times it is copied.
//!
//! Every case here therefore writes the borrow at the call site, which is the
//! form that was broken, and every one of them fails on the implementation this
//! file was added to repair.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::{Unreachable, Value};
use tos_ir::{IntKind, Module};
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

/// The whole production path over one module, at the Full profile.
fn module_of(body: &str) -> Module {
    let text = format!("module app.borrowing version 1.4 profile full; {ENVELOPE} {body}");
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
        source_set: String::from("tos-mutable-borrow-tests"),
        path: String::from("app/borrowing.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("the fixture lowers");
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the lowered IR verifies");
    module
}

/// Runs `main` and returns what it produced.
fn value(body: &str) -> Value {
    let module = module_of(body);
    let mut prepared = Prepared::launch(
        &[&module],
        &ResolutionSnapshot::default(),
        "main",
        RESIDENCY,
    )
    .expect("the fixture launches");
    prepared
        .run(Vec::new(), &mut Unreachable)
        .expect("the entry exists")
        .expect("the program does not trap")
        .value
}

fn i64_of(body: &str) -> i128 {
    match value(body) {
        Value::Int(IntKind::I64, value) => value,
        other => panic!("the fixture returns an i64: {other:?}"),
    }
}

/// The checker's verdict on a fixture, as diagnostic codes.
fn codes(body: &str) -> Vec<String> {
    let text = format!("module app.borrowing version 1.4 profile full; {ENVELOPE} {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let Some(schema) = Parser::parse_schema(&source).into_accepted() else {
        return vec![String::from("parse: rejected")];
    };
    Checker::check(&source, &schema)
        .iter()
        .map(|d| d.code().to_string())
        .collect()
}

// --------------------------------------------------------------- 1. a primitive

/// A mutable borrow of a scalar the caller owns.
///
/// The smallest case there is, and the one that shows `Copy` is not what
/// decides: `i64` is `Copy`, the callee's parameter holds a copy of the eight
/// bytes, and the caller still observes the write because the borrow named a
/// place and the place is what the write goes back to.
#[test]
fn a_mutable_borrow_of_a_primitive_reaches_the_caller() {
    assert_eq!(
        i64_of(
            "fn bump(borrow mut cell: i64) -> unit { cell = cell + 7i64; } \
             pub fn main() -> i64 { let mut cell = 5i64; bump(borrow mut cell); return cell; }"
        ),
        12
    );
}

/// The callee's view is the caller's value, not a zero or a default.
#[test]
fn a_mutable_borrow_carries_the_current_value_in() {
    assert_eq!(
        i64_of(
            "fn double(borrow mut cell: i64) -> unit { cell = cell * 2i64; } \
             pub fn main() -> i64 { let mut cell = 21i64; double(borrow mut cell); return cell; }"
        ),
        42
    );
}

// ------------------------------------------------ 2. a Copy aggregate / array

/// Passing an affine record without `borrow` moves it, which is what makes the
/// case above a borrow rather than a copy that happened to be written back.
#[test]
fn an_affine_record_passed_without_borrow_is_moved() {
    assert_eq!(
        codes(
            "pub record Pair [a: i64, b: i64] \
             fn set(borrow mut pair: Pair) -> unit { pair.a = 7i64; } \
             pub fn main() -> i64 { let mut pair = Pair(a: 0i64, b: 0i64); \
             set(pair); return pair.a; }"
        ),
        vec![String::from("E1301_USE_AFTER_MOVE")]
    );
}

/// A mutable borrow of an array, with the callee writing one element.
///
/// This is the shape Stage 4D-3 wanted and could not have: a pool of state that
/// a helper mutates on the caller's behalf.
#[test]
fn a_mutable_borrow_of_an_array_reaches_the_caller() {
    assert_eq!(
        i64_of(
            "fn place(borrow mut pool: array<i64, 4>, at: size, value: i64) -> unit { \
                 pool[at] = value; } \
             pub fn main() -> i64 { let mut pool: array<i64, 4> = [0i64, 0i64, 0i64, 0i64]; \
             place(borrow mut pool, 2B, 9i64); return pool[2B]; }"
        ),
        9
    );
}

/// Two calls through the same borrow, so the second sees what the first left.
///
/// A write-back that replaced the caller's array with a stale copy would lose
/// the first element; one that reached the place keeps both.
#[test]
fn successive_mutable_borrows_of_an_array_accumulate() {
    assert_eq!(
        i64_of(
            "fn place(borrow mut pool: array<i64, 4>, at: size, value: i64) -> unit { \
                 pool[at] = value; } \
             pub fn main() -> i64 { let mut pool: array<i64, 4> = [0i64, 0i64, 0i64, 0i64]; \
             place(borrow mut pool, 0B, 3i64); place(borrow mut pool, 1B, 4i64); \
             return pool[0B] * 10i64 + pool[1B]; }"
        ),
        34
    );
}

/// A record the callee mutates by field.
///
/// **An affine value, not a `Copy` one.** A nominal record is affine here — the
/// same fixture with a bare argument is `E1301_USE_AFTER_MOVE`, because passing
/// it without `borrow` moves it. So this is the case that shows the defect was
/// never about `Copy`: a mutable borrow of a value that cannot be copied at all
/// lost its callee's writes for exactly the same reason a scalar did.
#[test]
fn a_mutable_borrow_of_a_record_reaches_the_caller() {
    assert_eq!(
        i64_of(
            "pub record Pair [a: i64, b: i64] \
             fn set(borrow mut pair: Pair) -> unit { pair.a = 7i64; } \
             pub fn main() -> i64 { let mut pair = Pair(a: 0i64, b: 0i64); \
             set(borrow mut pair); return pair.a; }"
        ),
        7
    );
}

// ------------------------------------------------------------ 3. nested places

/// A mutable borrow of a **field**, and only that field moves.
///
/// The write-back has to reach `pair.a` rather than replacing `pair`, so the
/// sibling is the assertion that matters: a write-back that wrote the callee's
/// `i64` over the whole record would not type-check as a record at all, and one
/// that wrote the whole record would take `b` with it.
#[test]
fn a_mutable_borrow_of_a_field_changes_only_that_field() {
    assert_eq!(
        i64_of(
            "pub record Pair [a: i64, b: i64] \
             fn set(borrow mut cell: i64) -> unit { cell = 7i64; } \
             pub fn main() -> i64 { let mut pair = Pair(a: 0i64, b: 5i64); \
             set(borrow mut pair.a); return pair.a * 10i64 + pair.b; }"
        ),
        75
    );
}

/// A mutable borrow of an **element**, and only that element moves.
#[test]
fn a_mutable_borrow_of_an_element_changes_only_that_element() {
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64) -> unit { cell = 7i64; } \
             pub fn main() -> i64 { let mut pool: array<i64, 4> = [1i64, 2i64, 3i64, 4i64]; \
             set(borrow mut pool[1B]); \
             return pool[0B] * 1000i64 + pool[1B] * 100i64 + pool[2B] * 10i64 + pool[3B]; }"
        ),
        1734
    );
}

/// The element a **computed** index named when the borrow was taken.
///
/// A borrow fixes one location, and the index that chose it is read where the
/// borrow is written rather than where the call returns. Changing the index
/// afterwards therefore moves nothing: the borrow was never about the variable.
#[test]
fn a_mutable_borrow_of_a_computed_element_names_the_element_it_was_taken_of() {
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64) -> unit { cell = 7i64; } \
             pub fn main() -> i64 { let mut pool: array<i64, 4> = [1i64, 2i64, 3i64, 4i64]; \
             let mut at: size = 2B; set(borrow mut pool[at]); at = 0B; \
             return pool[0B] * 1000i64 + pool[1B] * 100i64 + pool[2B] * 10i64 + pool[3B]; }"
        ),
        1274
    );
}

// ------------------------------------------------------- 4. a shared borrow

/// A shared borrow cannot write, and the checker is what says so.
#[test]
fn a_shared_borrow_cannot_be_assigned_through() {
    assert_eq!(
        codes(
            "fn set(borrow cell: i64) -> unit { cell = 7i64; } \
             pub fn main() -> i64 { let mut cell = 1i64; set(borrow cell); return cell; }"
        ),
        vec![String::from("E1201_ASSIGN_TO_IMMUTABLE")]
    );
}

/// And a shared borrow that only reads leaves the caller's value alone.
///
/// The engine records nothing for a shared borrow, so there is nothing for a
/// return to write back — which is the difference between the two modes made
/// observable rather than asserted.
#[test]
fn a_shared_borrow_leaves_the_caller_alone() {
    assert_eq!(
        i64_of(
            "fn peek(borrow cell: i64) -> i64 { return cell + 1i64; } \
             pub fn main() -> i64 { let cell = 5i64; let seen = peek(borrow cell); \
             return seen * 10i64 + cell; }"
        ),
        65
    );
}

// -------------------------------------------- 5. conflict diagnostics unchanged

/// Two shared borrows of one place are compatible.
#[test]
fn repeated_shared_borrows_are_still_compatible() {
    assert!(codes(
        "fn peek(borrow cell: i64) -> i64 { return cell; } \
         pub fn main() -> i64 { let cell = 1i64; \
         return peek(borrow cell) + peek(borrow cell); }"
    )
    .is_empty());
}

/// A mutable borrow is still exclusive against an owner read.
#[test]
fn a_mutable_borrow_is_still_exclusive() {
    let reported = codes(
        "pub record Counter [value: i64] \
         fn write(borrow mut counter: Counter) -> i64 { counter.value = 1i64; return 0i64; } \
         pub fn main() -> i64 { let mut counter = Counter(value: 0i64); \
         let held = borrow mut counter; return write(held) + counter.value; }",
    );
    assert!(
        reported
            .iter()
            .any(|code| code == "E1302_CONFLICTING_BORROW"),
        "{reported:?}"
    );
}

// ------------------------------------------------------------ 6. local calls

/// A borrow passed through one call into the next.
///
/// The inner callee borrows the outer callee's parameter, which is itself a
/// borrow of `main`'s binding. Each frame's write-back reaches its own caller's
/// place, and the value arrives where it started.
#[test]
fn a_mutable_borrow_composes_through_nested_local_calls() {
    assert_eq!(
        i64_of(
            "fn inner(borrow mut cell: i64) -> unit { cell = cell + 1i64; } \
             fn outer(borrow mut cell: i64) -> unit { inner(borrow mut cell); \
                 inner(borrow mut cell); } \
             pub fn main() -> i64 { let mut cell = 0i64; outer(borrow mut cell); return cell; }"
        ),
        2
    );
}

/// Two mutable borrows of two different places in one call.
#[test]
fn two_mutable_borrows_in_one_call_reach_two_places() {
    assert_eq!(
        i64_of(
            "fn swapish(borrow mut left: i64, borrow mut right: i64) -> unit { \
                 left = 3i64; right = 4i64; } \
             pub fn main() -> i64 { let mut a = 0i64; let mut b = 0i64; \
             swapish(borrow mut a, borrow mut b); return a * 10i64 + b; }"
        ),
        34
    );
}

/// A closure's declared `borrow mut` parameter is a mutable borrow too.
///
/// docs/39 gives a closure an ordinary parameter list — `closure_parameters =
/// parameter ( "," parameter )* ","?` and `parameter = borrow_mode? identifier
/// ":" type` — so `borrow mut` means there exactly what it means on a named
/// function, and lowering it as an owned parameter was the frontend deciding it
/// meant something else.
#[test]
fn a_closure_parameter_declared_borrow_mut_is_one() {
    assert_eq!(
        i64_of(
            "pub fn main() -> i64 { let mut cell = 5i64; \
             let bump = fn (borrow mut slot: i64) { slot = slot + 7i64; }; \
             bump(borrow mut cell); return cell; }"
        ),
        12
    );
}

/// A deferred cleanup still sees and leaves the scope's bindings.
///
/// ADR-0035 makes a cleanup act on the scope it runs in, and its captures are
/// the bindings themselves rather than borrows of them — so this path was
/// always right, and the repair must not have moved it.
#[test]
fn a_cleanup_still_acts_on_the_scope_it_runs_in() {
    assert_eq!(
        i64_of(
            "pub fn main() -> i64 { let mut total = 1i64; \
             if (true) { defer { total = total + 2i64; } defer { total = total * 30i64; } } \
             return total; }"
        ),
        32
    );
}

/// A cleanup that reaches the scope's binding **through a call**.
///
/// The cleanup body's own capture is a mutable-borrow parameter; taking a
/// borrow of that parameter and passing it on is the composition that was lost
/// before, and it is the one a driver writes.
#[test]
fn a_cleanup_can_pass_the_scope_binding_on_by_mutable_borrow() {
    assert_eq!(
        i64_of(
            "fn bump(borrow mut cell: i64) -> unit { cell = cell + 7i64; } \
             pub fn main() -> i64 { let mut total = 1i64; \
             if (true) { defer { bump(borrow mut total); } } return total; }"
        ),
        8
    );
}

// ------------------------------------ the regression the repair is really about

/// A mutable borrow is not a copy written back into the borrow expression.
///
/// **This is the defect, stated as a program.** `Op::Borrow` produces a value,
/// and a value here is a copy of what was read. If the write-back aims at the
/// slot that copy lives in, every assertion above returns the value the caller
/// started with and the callee's work is discarded in silence — no trap, no
/// diagnostic, no difference the program can see except the wrong answer.
///
/// The fixture makes the two outcomes maximally far apart: the callee's write
/// is the only thing that can produce a non-zero answer, and the caller reads
/// the place after the call rather than the borrow expression. A run that
/// scores 0 here is an engine writing into a temporary.
#[test]
fn a_mutable_borrow_does_not_write_back_into_the_borrow_expression() {
    let body = "fn fill(borrow mut pool: array<i64, 3>) -> unit { \
             pool[0B] = 1i64; pool[1B] = 2i64; pool[2B] = 3i64; } \
         pub fn main() -> i64 { let mut pool: array<i64, 3> = [0i64, 0i64, 0i64]; \
         fill(borrow mut pool); \
         return pool[0B] * 100i64 + pool[1B] * 10i64 + pool[2B]; }";
    let scored = i64_of(body);
    assert_ne!(
        scored, 0,
        "the callee's writes reached a temporary rather than the borrowed place"
    );
    assert_eq!(scored, 123);
}

/// And the borrow's own value is a copy, which is what makes the above the only
/// way to observe the write.
///
/// The callee writes `9` into the borrowed scalar and the caller reads the place
/// afterwards. Nothing in the source names the borrow after the call, because
/// V1 borrows do not escape (docs/40 section 2) — the place is the only witness
/// there is.
#[test]
fn the_place_is_the_only_witness_a_mutable_borrow_leaves() {
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64) -> unit { cell = 9i64; } \
             pub fn main() -> i64 { let mut cell = 0i64; set(borrow mut cell); \
             let after = cell; return after; }"
        ),
        9
    );
}
/// **The precondition the write-back rests on**: no two live borrows of
/// overlapping places reach one call.
///
/// Copying a value in and copying it out is observationally a reference only
/// while nothing else names the same location for the duration of the call. If
/// two mutable borrows of one place could be passed together, the second
/// write-back would overwrite the first and the result would depend on
/// parameter order rather than on the program. The checker is what forbids it,
/// and these are the three shapes it has to catch: the same place twice, a
/// place and a part of it, and a shared borrow beside a mutable one.
#[test]
fn overlapping_borrows_never_reach_one_call() {
    for body in [
        "fn two(borrow mut a: i64, borrow mut b: i64) -> unit { a = 1i64; b = 2i64; } \
         pub fn main() -> i64 { let mut cell = 0i64; \
         two(borrow mut cell, borrow mut cell); return cell; }",
        "fn two(borrow mut pool: array<i64, 2>, borrow mut cell: i64) -> unit { \
             pool[0B] = 1i64; cell = 2i64; } \
         pub fn main() -> i64 { let mut pool: array<i64, 2> = [0i64, 0i64]; \
         two(borrow mut pool, borrow mut pool[0B]); return pool[0B]; }",
        "fn two(borrow a: i64, borrow mut b: i64) -> unit { b = a + 1i64; } \
         pub fn main() -> i64 { let mut cell = 0i64; \
         two(borrow cell, borrow mut cell); return cell; }",
    ] {
        assert_eq!(
            codes(body),
            vec![String::from("E1302_CONFLICTING_BORROW")],
            "{body}"
        );
    }
}

// ------------------------------------- the borrow table's bound and lifetime

/// One borrow site executed two thousand times keeps one entry.
///
/// The frame's record of live mutable borrows is keyed by the slot the borrow
/// instruction defines, and a function's slots are fixed when it is lowered —
/// so a borrow inside a loop rewrites one entry however many times it runs.
/// This is that property as a program: the loop is long enough that an entry
/// per iteration would be visible as cost rather than as a wrong answer.
#[test]
fn a_borrow_in_a_loop_does_not_grow_the_frames_record() {
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64, value: i64) -> unit { cell = value; } \
             pub fn main() -> i64 { let mut acc = 0i64; let mut i: i64 = 0i64; \
             while (i < 2000i64) { set(borrow mut acc, i); i = i + 1i64; } return acc; }"
        ),
        1999
    );
}

/// Each iteration's borrow is about that iteration's element.
///
/// **The sharpest test of stale identity there is.** The borrow is re-taken
/// every pass with a different computed index, so an implementation that kept
/// the first iteration's place would write every value into element 0 and leave
/// the rest zero. Four distinct values in four distinct elements is the whole
/// assertion.
#[test]
fn a_borrow_retaken_in_a_loop_names_the_current_element() {
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64, value: i64) -> unit { cell = value; } \
             pub fn main() -> i64 { let mut pool: array<i64, 4> = [0i64, 0i64, 0i64, 0i64]; \
             let mut i: size = 0B; let mut v: i64 = 1i64; \
             while (i < 4B) { set(borrow mut pool[i], v); i = i + 1B; v = v + 1i64; } \
             return pool[0B] * 1000i64 + pool[1B] * 100i64 + pool[2B] * 10i64 + pool[3B]; }"
        ),
        1234
    );
}

/// An ended borrow leaves nothing a later call can pick up.
///
/// The second call borrows a different binding, and a third form — a bare
/// argument, whose slot was never a borrow — follows one that was. Neither can
/// reach the first borrow's place: an entry is readable only by passing that
/// very borrow value as an operand, and a V1 borrow does not outlive the
/// statement that took it (docs/40 section 2).
#[test]
fn an_ended_borrow_leaves_no_place_a_later_call_can_reach() {
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64, value: i64) -> unit { cell = value; } \
             pub fn main() -> i64 { let mut a = 0i64; let mut b = 0i64; \
             set(borrow mut a, 5i64); set(borrow mut b, 9i64); return a * 10i64 + b; }"
        ),
        59
    );
    assert_eq!(
        i64_of(
            "fn set(borrow mut cell: i64, value: i64) -> unit { cell = value; } \
             pub fn main() -> i64 { let mut a = 0i64; let mut b = 0i64; \
             set(borrow mut a, 5i64); set(b, 9i64); return a * 10i64 + b; }"
        ),
        59
    );
}

// ------------------------------- a callable value, and what its type does not say

/// **A callable value's type carries no parameter mode, and that is the
/// contract rather than the implementation.**
///
/// docs/39 gives it as `function_type = "fn" "(" type_list? ")" "->" type` — a
/// type list and a result. `tos-ir/v1`'s `TypeDef::Function(Vec<TypeId>,
/// TypeId)` mirrors that exactly, so owned, `borrow` and `borrow mut` are
/// indistinguishable in a callable's type at every layer: source, checker and
/// verifier. These cases pin what follows, so that it is a recorded property
/// rather than something discovered later in runtime behaviour.
///
/// Two closures differing only in parameter mode are assignable to one binding,
/// and the call is decided by the body that is actually reached.
#[test]
fn a_callable_value_carries_no_parameter_mode() {
    // The body declares `borrow mut`; the annotation says only `fn (i64)`.
    assert_eq!(
        i64_of(
            "pub fn main() -> i64 { let mut p = 1i64; \
             let f: fn (i64) -> unit = fn (borrow mut v: i64) { v = 7i64; }; \
             f(borrow mut p); return p; }"
        ),
        7
    );
    // The same annotation, a body that borrows nothing.
    assert_eq!(
        i64_of(
            "pub fn main() -> i64 { let mut p = 1i64; \
             let f: fn (i64) -> unit = fn (v: i64) { }; \
             f(borrow mut p); return p; }"
        ),
        1
    );
    // And one binding reaching either, chosen at run time.
    assert_eq!(
        i64_of(
            "pub fn main() -> i64 { let mut p = 1i64; let pick = true; \
             let mut f = fn (v: i64) { }; \
             if (pick) { f = fn (borrow mut v: i64) { v = 7i64; }; } \
             f(borrow mut p); return p; }"
        ),
        7
    );
}

/// A closure reached through a parameter or a return value behaves the same.
#[test]
fn a_callable_value_keeps_its_body_s_modes_wherever_it_is_carried() {
    assert_eq!(
        i64_of(
            "fn apply(f: fn (i64) -> unit, borrow mut cell: i64) -> unit { f(borrow mut cell); } \
             pub fn main() -> i64 { let mut p = 1i64; \
             apply(fn (borrow mut v: i64) { v = 7i64; }, borrow mut p); return p; }"
        ),
        7
    );
    assert_eq!(
        i64_of(
            "fn make() -> fn (i64) -> unit { return fn (borrow mut v: i64) { v = 7i64; }; } \
             pub fn main() -> i64 { let mut p = 1i64; let f = make(); f(borrow mut p); return p; }"
        ),
        7
    );
}

/// **What the erasure may not do: write a value of one type into a place of
/// another.**
///
/// A callable's type carries its parameter types, and a value call is not
/// checked against them — so a body declaring `borrow mut v: i32` is reachable
/// through an `fn (i64) -> unit` binding. Its write-back would put an `i32`
/// into an `i64` place, and the caller would go on computing with a value whose
/// type nothing declared: no trap, no diagnostic, a wrong kind of number.
///
/// That is refused at the call rather than performed. The trap is the honest
/// answer while the type system cannot state the obligation: a write-back is
/// the one thing a call does to the caller's own storage, and it may not do it
/// blind.
#[test]
fn a_value_call_may_not_write_back_through_a_mismatched_type() {
    for body in [
        "pub fn main() -> i64 { let mut p: i64 = 1i64; \
         let f: fn (i64) -> unit = fn (borrow mut v: i32) { v = 7i32; }; \
         f(borrow mut p); return p; }",
        "pub fn main() -> i64 { let mut p: i64 = 1i64; \
         let f: fn (i64) -> unit = fn (borrow mut v: bool) { v = true; }; \
         f(borrow mut p); return p; }",
        "pub fn main() -> i64 { let mut p: i64 = 1i64; \
         let f: fn (i64) -> unit = fn (borrow mut v: array<i64, 2>) { v[0B] = 7i64; }; \
         f(borrow mut p); return p; }",
        "pub fn main() -> i64 { let mut pool: array<i64, 2> = [1i64, 2i64]; \
         let f: fn (i64) -> unit = fn (borrow mut v: i32) { v = 7i32; }; \
         f(borrow mut pool[0B]); return pool[0B]; }",
    ] {
        let module = module_of(body);
        let mut prepared = Prepared::launch(
            &[&module],
            &ResolutionSnapshot::default(),
            "main",
            RESIDENCY,
        )
        .expect("the fixture launches");
        let trap = prepared
            .run(Vec::new(), &mut Unreachable)
            .expect("the entry exists")
            .expect_err("a mismatched write-back is refused");
        assert_eq!(trap.code, "RUNTIME_TYPE_CONFUSION", "{body}");
    }
}

/// And a value call that agrees still writes back, so the refusal above is
/// about disagreement rather than about value calls.
#[test]
fn a_value_call_that_agrees_still_writes_back() {
    assert_eq!(
        i64_of(
            "pub fn main() -> i64 { let mut p: i64 = 1i64; \
             let f: fn (i64) -> unit = fn (borrow mut v: i64) { v = 7i64; }; \
             f(borrow mut p); return p; }"
        ),
        7
    );
}
