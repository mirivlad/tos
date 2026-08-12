<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0047: Which `match` arm runs when several could

- Status: **Accepted** (Project Architect-approved)
- Date: 2026-08-12
- Decision level: 2 — the observable meaning of an accepted V1 statement
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-12

## The gap

docs/40 section 4 fixes evaluation order in detail — calls left to right,
operands left before right, aggregate fields in lexical source order, "match
subject evaluates before patterns". It fixes exhaustiveness: a `match` on an
enum, `Option` or `Result` must cover every case, and "an `_` arm is
exhaustive".

It does not say **which arm runs when more than one matches**.

That is not a hypothetical. Nothing in the accepted contract forbids an
unreachable arm, so this is valid V1 source:

```tos
match (mode) {
    _    => { return 1i32; }
    Fast => { return 2i32; }
}
```

Both arms match a `Fast`. The contract admits the program and does not say
whether it returns 1 or 2.

## How it was found

The Stage 2 closure audit measured it rather than reasoned about it. The
implementation returned **2**: the lowerer built a variant-to-target map from
every arm and used the wildcard only as a default for variants no arm named, so
a variant arm written *after* a catch-all displaced it. Whatever the answer
should be, an implementation should not arrive at one by accident.

## Options

1. **First matching arm in source order.** The earlier arm wins; later arms that
   could also match are unreachable. This is what every language with pattern
   matching that TOS Core resembles does, and it is the only reading consistent
   with docs/40's insistence on lexical order everywhere else.
2. **Most specific arm wins.** A variant arm beats a catch-all wherever it
   appears. Defensible in isolation and hostile in practice: reordering arms
   would stop changing behaviour, and a reader could not tell which arm runs
   without a specificity calculation the source does not show.
3. **Reject unreachable arms with a diagnostic.** Makes the question moot by
   forbidding the program. It is a real option, but it is a new rule and a new
   code, and it should not be adopted merely because it is convenient for a
   lowerer.

## Decision

**Option 1.** TOS Core V1 `match` has this normative semantics:

1. The subject is evaluated **exactly once**, before any arm is selected.
2. Arms are considered in **strict lexical source order**.
3. The **first** arm whose pattern matches the subject is the arm that runs.
4. Once an arm is selected, later arms take no part in selection and their
   bodies do not execute.
5. **Exactly one** arm body executes.
6. The existing exhaustiveness rules are unchanged.
7. A wildcard and a bare binding are catch-alls under the existing rules.
8. An irrefutable tuple pattern likewise makes every later arm unreachable.
9. **Unreachable arms are permitted in V1.** No compile-time diagnostic for one
   is introduced now.
10. Forbidding unreachable arms later would be a separate versioned language
    decision, and it would not change the first-match rule this ADR fixes.

## Why option 1 (as recommended before the decision)

It is the reading the rest of docs/40 already implies, it makes an arm's
position meaningful in the way a reader expects, and it needs no new diagnostic.
Option 3 could be adopted **in addition** later without conflicting with it.

It is the reading the rest of docs/40 already implies, it makes an arm's
position meaningful in the way a reader expects, and it needs no new diagnostic.
Option 3 could be adopted **in addition** later without conflicting with it.

## What the implementation does

Exactly this, and it already did before the decision — the lowerer takes arms in
source order and stops at the first irrefutable one, so a catch-all before a
variant arm wins and the later arm is unreachable. `tos-ir/v1` is unchanged:
first-match needs no new IR construct, only an ordered `MatchEnum` map whose
default is the first irrefutable arm.

The tests are now written as the decision requires: a program this ADR makes
valid must reach `Run::Completed` with its exact V1 result, and a diagnostic is
no longer an acceptable alternative outcome for one
(`crates/tos-pipeline/tests/match_matrix.rs`).

## Consequences

docs/40 section 4 gains the rule beside the existing evaluation-order sentences,
and the implementation is already correct against it. The class of accepted V1
programs whose result the contract did not determine is now empty.

Permitting unreachable arms is a deliberate choice, not an omission: a rule that
forbade them would be a new diagnostic and a new conformance obligation, and
point 10 keeps that a separate decision rather than something this one implies.
