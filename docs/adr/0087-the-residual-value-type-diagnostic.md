<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0087: the residual value-type diagnostic

- Status: **Accepted (Project Architect-approved, 2026-09-14)**
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-14, granted before
  implementation
- Date: 2026-09-14
- Decision level: **2**. It allocates one diagnostic code and changes no source
  syntax, no semantic rule and no artifact schema
- Related: `docs/40` §2 and §3 (typed bindings, exact types, contextual integer
  literals), `docs/44` §7 (the diagnostic registry), ADR-0052 (constants are
  compile-time values), ADR-0032 (diagnostic regions and recovery)

## 1. The gap, stated exactly

`docs/40` §2 makes a binding's annotation part of its type:

> A `let` binding has the declared type when one is written and the
> initializer's type otherwise.

and §3 makes the language's types **exact** — there is no subtyping and no
implicit conversion between them. Together those say that

```tos
let flag: bool = 1i64;
```

is not a program. Nothing in the accepted registry says so.

The registry owns four type mismatches and each is about a *particular*
position:

| Code | Position it owns |
|---|---|
| `E1210_INTEGER_TYPE_MISMATCH` | a value of one integer type where a different exact integer type is required |
| `E1211_INDEX_TYPE_MISMATCH` | an index that is not `size` |
| `E1215_ARGUMENT_TYPE_MISMATCH` | an argument of a resolved call or predeclared operation |
| `E1222_RETURN_TYPE_MISMATCH` | a `return` against the declared result |

A `bool` initializer under an `i64` annotation is none of them. Neither is a
`string` assigned to a `bytes` binding, nor an aggregate built at one type and
bound at another. The frontend's typing slice says so in its own text — it
reports "only the integer case the contract states" — and the consequence is
that every other exact-type disagreement is accepted, lowered, and discovered by
the engine if it is discovered at all.

**This is a hole in the registry, not in the language.** The rule the program
above breaks is already accepted. What is missing is the code that names the
breach.

## 2. Decision

Allocate **`E1216_VALUE_TYPE_MISMATCH`**:

> an expression's value does not have the exact type its statically typed
> context requires, and no more specific accepted diagnostic owns that mismatch.

Required fields: `context`, `expected`, `actual`. A more specific field —
`binding`, `field`, `element` — may accompany them where it helps a reader find
the position, and never in place of the three.

`context` names the *kind* of typed position, so that one code stays readable
across the positions it has to cover: `binding`, `assignment`, `array element`,
`record field`, `tuple element`, `enum payload`, `branch`.

### 2a. Precedence

`E1216` is **residual**. A mismatch is reported under it only when no other
accepted diagnostic owns the rule actually violated:

```text
E1210_INTEGER_TYPE_MISMATCH     two exact numeric types disagree
E1211_INDEX_TYPE_MISMATCH       an index is not `size`
E1215_ARGUMENT_TYPE_MISMATCH    a call or predeclared argument, absent a
                                more specific numeric code
E1222_RETURN_TYPE_MISMATCH      a `return` against the declared result
E1204_TYPE_ARGUMENT_ARITY       a type constructor's argument count
E1206_MISSING_RECORD_FIELD      a constructor's field set
E12xx ownership / capability    where that rule is the one broken
E1216_VALUE_TYPE_MISMATCH       everything else, and only then
```

Two consequences worth stating, because both are easy to get wrong:

- an `i32` initializer under an `i64` annotation is **`E1210`**, not `E1216` —
  the numeric code owns it and is more specific;
- an **unsuffixed** integer literal under any exact numeric annotation is **not
  a mismatch at all**. `docs/40` §3 makes it take the required type, so
  `let x: size = 4;` is a `size` and `let x: i64 = 4;` is an `i64`. A residual
  code that reported those would be contradicting the contextual-typing rule
  rather than completing the registry.

## 3. What this is not

**Not a new language rule.** Every program `E1216` rejects was already outside
`docs/40` §2 and §3; the frontend accepted it because no code existed to report
it. A conforming implementation that already rejected these programs — under
whatever code — was not wrong about the language.

**Not a new source form**, so `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR` does not
apply and **no TOS Core minor is required**. `docs/44` §7 fixes that code to a
module using "a source form added in a later minor"; nothing here adds one.
Canonical module headers keep the versions they declare, and no fixture's
`version` line changes on account of this decision.

**Not a change to contextual typing.** §2a's second consequence is the existing
rule restated, not narrowed.

**Not an arity diagnostic.** Wrong call arity remains unnamed in the registry
and is deliberately left so: it is a different question about a different
position, and inventing a second code inside a decision about this one would be
the drift this ADR exists to stop.

## 4. Conformance

`docs/language/conformance/v1/reject/` gains vectors for the residual cases —
a `bool` initializer under a numeric annotation, a `bytes` value under a `text`
annotation, an aggregate bound at the wrong element type — and
`docs/language/conformance/v1/accept/` gains the contextual-literal cases that
must **not** be reported: an unsuffixed literal under `i64`, under `u64` and
under `size`.

The precedence rule is itself conformance-visible: a vector whose mismatch is
numeric must report `E1210` and not `E1216`, which is what stops the residual
code from absorbing the specific ones over time.

## 5. Architecture impact statement

- **Change level**: 2 — one registry entry, no syntax, no semantics, no schema.
- **Invariants affected**: none. `docs/02`'s type-soundness expectation is
  served rather than altered.
- **Canonical representation**: unchanged. No source form is added, removed or
  respelled.
- **Trusted base**: unchanged. The diagnostic is a frontend refusal; the
  independent verifier's obligations are untouched by this decision.
- **Source-to-runtime**: unchanged for every program that was already valid.
  Programs this rejects never had a defined lowering.
- **Recovery and rollback**: none required. A frontend that reports the code is
  compatible with artifacts built before it existed.
- **Stage gate**: none. This is contract completeness rather than stage work.
- **Threat model**: narrows an accept path — a static mismatch reaching the
  engine — and opens none.
- **Performance**: no measured path is touched.
- **Compatibility profile**: unchanged; both profiles.
- **Dependencies, licence, patent**: none.
- **Tests**: the vectors of §4, plus frontend tests for each `context` value.

## 6. What this ADR does not decide

The general typed-value invariant — that every lowered operand, value and place
carries one exact type, with no fallback able to make the runtime value graph
disagree with the verified type graph — is the slice this diagnostic was needed
for, and it is decided by its own work rather than here. `E1216` names a breach;
it does not say how a frontend comes to know the types it compares.

Wrong call arity, imported-call signature transport, predeclared signature
transport and callable `PassMode` erasure all remain open and are all
deliberately untouched.
