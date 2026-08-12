<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0046: `let` and `for` patterns must be irrefutable

- Status: **Accepted** (Project Architect-approved)
- Date: 2026-08-12
- Decision level: 2 — settles a question ADR-0033 left open and adds one stable
  diagnostic to the accepted V1 surface
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-12

## Context

The accepted grammar of docs/39 admits a pattern in three places:

```text
let_stmt     = "let" "mut"? pattern ...
for_stmt     = "for" pattern "in" ...
match_branch = pattern "=>" block
```

and the accepted type semantics already fixes the expected type of a pattern in
each. ADR-0033 settled how a bare name in a pattern resolves, and left one
question open: whether a `let` or `for` pattern may be *refutable* — may fail to
match the value it is given.

The question is not theoretical. `match` has arms: a pattern that does not match
falls through to one that does, and `E1220_NONEXHAUSTIVE_MATCH` makes sure one
always does. `let` and `for` have no such structure. They bind unconditionally.

So a refutable pattern in either would need one of: a hidden runtime trap, a
hidden conditional branch, or a silently ignored mismatch. V1 has none of these
and should acquire none of them by accident.

## Decision

### 1. `match` is unchanged

Refutable patterns are admitted, as they always were. Exhaustiveness is checked
by `E1220_NONEXHAUSTIVE_MATCH`.

### 2. A `let` pattern must be irrefutable for the type of its initializer

```tos
let x = value;                       // accepted
let _ = value;                       // accepted
let (a, b) = pair;                   // accepted
let (head, (left, right)) = nested;  // accepted
```

A refutable pattern is a compile-time error:

```tos
let Some(x) = value;      // rejected: Option has None
let Ok(x) = result;       // rejected: Result has Err
let Fast = mode;          // rejected: Mode has other variants
```

### 3. A `for` pattern must be irrefutable for the element type

Each iteration binds one element. A refutable pattern there would be a filter or
a failure branch wearing a loop's clothes, and the loop's meaning stays simple
by refusing it.

### 4. Irrefutability is recursive

- `_` is irrefutable.
- A bare binding name is irrefutable.
- A tuple pattern is irrefutable exactly when **every** element is.
- A constructor pattern — an enum variant, `Some`, `Ok`, `Completed` — is
  irrefutable only when its type has **no other variant to be**, and then only
  when every sub-pattern of its payload is irrefutable.

A single-variant enum is therefore destructurable in `let`: refutability is a
fact about the type's alternatives, not about the pattern's shape.

### 5. `E1223_REFUTABLE_PATTERN`

Stage `type`. It means exactly *a pattern may fail to match where the context
binds unconditionally* — not a type mismatch, and not non-exhaustiveness.
Structured fields: `context` (`let` or `for`), `reason`, `expected`. The span is
the pattern's.

### 6. Precedence

Nothing is reported until the pattern has a settled meaning. In order:

1. an unresolved pattern name or constructor path — `E1202_UNKNOWN_VALUE_NAME`;
2. a constructor that is not a variant of the expected type, or a payload whose
   arity or types disagree — the existing type codes;
3. an undetermined initializer or element type — nothing is reported at all,
   because a guess is not a finding;
4. **only then** refutability.

Reporting that a pattern *may fail to match* a construct nobody has resolved
would be describing something that does not yet mean anything.

### 7. Ownership is unaffected

Destructuring is not a new ownership rule. A `Copy` component stays usable, an
affine component moves, and what remains of the aggregate follows the existing
partial-move rules of docs/40 section 5. The checker remains the source-level
proof; lowering expresses it.

### 8. No hidden runtime failure

Nothing in this decision introduces a trap, a branch or a silent mismatch. A
program that compiles binds exactly what it says it binds.

## Conformance evidence

Positives: tuple destructuring `let`; nested tuple destructuring; a wildcard
inside a tuple pattern; a sole-variant constructor in `let`; destructuring a
`Copy` component and an affine one; a refutable pattern in `match`.
Negatives: `Some(...)`, `Ok(...)` and a multi-variant enum constructor in `let`;
a refutable component inside an otherwise irrefutable tuple pattern.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-09 is served —
  `E1223` joins the versioned diagnostic boundary.
- **Canonical representation:** unchanged.
- **`tos-ir/v1`:** unchanged. Destructuring lowers to the `Move` through a
  `PlaceStep::Field` that the language already uses for any other aggregate
  access, so the independent verifier sees no new construct and its existing
  ownership and type rules apply without amendment.
- **Trusted-base impact:** none. **Threat-model impact:** positive — a rule that
  was undecided is now checked.
- **Compatibility profile:** TOS Core 1.0.

## Consequences

A question ADR-0033 left open is closed, and the production lowerer can stop
refusing valid V1 source. `let` and `for` keep their unconditional meaning, and
the one construct that could have quietly acquired a runtime failure does not.

## Alternatives considered

**Allow refutable `let` with a runtime trap.** Rejected: it makes a binding into
a conditional failure that the source does not show, and docs/41 traps are for
dynamic preconditions rather than for pattern shape.

**Allow refutable `let` as a silent no-op.** Rejected outright: a binding that
sometimes does not happen, with no diagnostic and no branch, is the worst of the
three options.

**Leave it undecided and keep the lowerer's `Gap`.** Rejected: the grammar and
the type semantics already admit these patterns, so the checker accepts source
the production path cannot represent. That is a contract the implementation does
not meet, which is what this ADR exists to fix.
