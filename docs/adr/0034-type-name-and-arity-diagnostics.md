<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0034: TOS Core V1 type-name and type-argument-arity diagnostics

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 2 — allocates two diagnostic codes that conformance evidence
  will depend on, and removes an ambiguity about which stage checks type
  argument arity
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

The type slice of the reference frontend cannot start without two diagnostics
the contract describes but does not name.

**A type name that resolves to nothing.** `docs/40` section 1 states that V1 has
nominal types and that a type name resolves through the declared import graph,
never by host search paths. It allocates no code for the case where that
resolution fails, so a checker had no way to reject `let value: Nonexistent =
...` other than silence.

**A wrong number of type arguments.** `docs/40` section 2 fixes the arity of
every V1 constructed type — ten constructors take one type argument, `Result`
takes two, eight take none — and then says using another arity "is a parse/type
error". That phrase leaves two normative answers about which stage rejects it.
It matters: if the parser refuses to build `Option<i32, bool>`, the error is a
syntax error at an unexpected token, the arity is invisible in the diagnostic,
and the tree stops at the first mistake. If the checker rejects it, the
diagnostic can carry the constructor and both arities.

Under ADR-0032, allocating a code is a versioned language decision rather than
an implementation choice, so both had to be decided before the type slice could
be written.

## Decision

### 1. `E1203_UNKNOWN_TYPE_NAME`

Stage `type`. A type name, after ordinary module, import and type-name
resolution, resolves to none of:

- a primitive type;
- a fixed or predeclared TOS Core type;
- a local nominal type;
- a reachable imported type.

For a qualified name, the module or import part must resolve first. If the
import or module itself does not exist, the applicable `E16xx` code governs; if
the module or import exists but does not declare that type name, the result is
`E1203_UNKNOWN_TYPE_NAME`.

The diagnostic carries at least the unresolved type name as spelled.

### 2. `E1204_TYPE_ARGUMENT_ARITY`

Stage `type`. A name resolves to a known parameterized V1 type constructor but
is applied to the wrong number of type arguments.

The number of type arguments is a static type property, not a parser decision.
The parser MUST be able to build a syntactically valid constructed-type node for
a known V1 type constructor written with `<...>`, and the checker compares the
actual count against the fixed V1 arity:

```tos
Option<i32>              // accepted
Option<i32, bool>        // E1204, expected 1, actual 2

Result<i32, Error>       // accepted
Result<i32>              // E1204, expected 2, actual 1
```

The diagnostic carries at least:

```text
constructor
expected_arity
actual_arity
```

This admits no user generics, and it does not make an arbitrary `Foo<T>` valid
V1 type syntax. It applies only to the fixed set of parameterized constructors
already defined by TOS Core V1.

`array<T, N>` is deliberately excluded. Its second argument is a compile-time
`size` constant rather than a type argument, and its existing grammar and type
contract stay separate. No general kind or generic mechanism is introduced for
`E1204`.

### 3. Precedence

1. an unresolved constructor or type name is `E1203_UNKNOWN_TYPE_NAME`;
2. a name that resolves to a known parameterized constructor applied with the
   wrong number of arguments is `E1204_TYPE_ARGUMENT_ARITY`;
3. only after the arity is correct are the argument types themselves and the
   remaining type rules checked.

One mistake must not cascade into further diagnostics derived from a constructed
type that does not exist.

### 4. Removing the ambiguity in docs/40

The phrase "using another arity is a parse/type error" is replaced. After this
ADR there is one normative answer: arity is checked at the type stage and
reported as `E1204_TYPE_ARGUMENT_ARITY`. `docs/39` records the matching grammar
boundary — the parser builds the constructed-type node and does not decide
arity.

### 5. Conformance evidence

The corpus gains negative cases for at least: an unknown local type; an unknown
qualified type where the import and module resolve; `Option` with the wrong
arity; `Result` with the wrong arity; and a case proving the precedence of an
unresolved name over an arity finding.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-09 is served — the two codes become
  part of the versioned diagnostic boundary; I-15 is served by replacing
  "parse/type error" with one stated stage.
- **Canonical representation after the change:** unchanged. No accepted source
  becomes invalid: every arity these codes reject was already an error under
  docs/40 section 2, only with an unstated stage.
- **Trusted-base impact:** none.
- **Source-to-runtime impact:** none directly. A rejected type expression now
  names its constructor and arities, so evidence can cite an exact condition.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** none. Moving arity to the type stage keeps the parser
  total and bounded; the checker reads only declared types.
- **Performance contract:** none applicable.
- **Compatibility profile:** TOS Core 1.0. Both codes and the arity stage are
  fixed for V1 and change only through a versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** the five conformance cases in section 5,
  checker unit tests for both codes and their precedence, and the mechanical
  language-contract gate binding the codes to the registry.

## Consequences

The type slice can begin. A rejected type expression names the constructor and
both arities instead of pointing at a token, and the precedence rule keeps one
mistake from producing a cascade of derived findings.

The cost is two more codes fixed for TOS Core 1.0. That is the intended trade:
a code conformance depends on must not drift.

## Alternatives considered

**Reject wrong arity in the parser.** Rejected: it is the reading that makes the
diagnostic least useful — an unexpected-token error cannot name the constructor
or the expected count — and it would let a syntax stage encode a type property.

**Reuse `E1202_UNKNOWN_VALUE_NAME` for unknown type names.** Rejected: a value
name and a type name are different namespaces in a nominal language, and
conformance tooling could not tell which one failed.

**Generalize `array<T, N>` with the other constructors.** Rejected: its second
argument is a constant, not a type. Folding it in would require a kind system
that V1 deliberately does not have.
