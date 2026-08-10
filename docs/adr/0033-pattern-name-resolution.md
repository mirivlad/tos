<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0033: TOS Core V1 pattern name resolution

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 2 — fixes pattern resolution semantics inside the accepted
  TOS Core V1 contract and adds the qualified constructor-pattern form the
  grammar was missing
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

The first checker slice reached a question the accepted contract does not
answer: when a pattern is a bare identifier, does it match a constructor or
bind a new name?

`docs/39` section 2 makes only reserved, primitive, predeclared type and
predeclared value names unshadowable, which reads as "every other identifier in
a pattern binds". The accepted corpus disagrees: `explicit-control-return.tos`
writes `match (signal) { Low => { ... } High => { ... } }` and expects `Low` and
`High` to match the variants of `Signal`.

A second gap sits next to it. `docs/40` section 2 states that enum variant names
are local to their defining module, may be used unqualified there, and that an
imported enum variant uses a qualified type or module name. The `pattern`
production in `docs/39` section 5 has no qualified form, so the language
requires a syntax its own grammar cannot express. That is a conflict between two
Tier 2 documents under `docs/38`, not an omission of detail.

Neither question can be deferred past the type slice: exhaustiveness checking
and payload typing both depend on whether an arm matches a constructor or binds
a catch-all.

## Decision

### 1. A bare pattern name resolves against the expected type

Every pattern is checked against an expected type, which the checker knows
before it resolves the pattern:

- `match (expression)` — the type of the scrutinee expression;
- `let pattern = expression` — the type of the initializer, refined by an
  explicit type annotation when one is present;
- `for pattern in (expression)` — the element type of the iterated value;
- a nested pattern — the type of the corresponding tuple element or enum
  payload position.

If the expected type is an enum and a bare identifier exactly equals the name of
one of that enum's variants, the identifier is the constructor pattern for that
variant. Otherwise a bare ordinary identifier introduces a new pattern binding.

### 2. Resolution is nominal, never lexical or lexicographic

There is no capitalization rule. `Uppercase` does not mean constructor and
`lowercase` does not mean binding; V1 has no such convention and none is
introduced.

An existing lexical or value binding of the same name does not change the
decision. Constructor resolution is determined by the expected nominal type
alone, so introducing an unrelated local named `Low` cannot silently turn a
variant pattern into a binding, and removing one cannot turn a binding into a
variant pattern.

A consequence is that two enums may declare variants with the same name without
colliding:

```tos
enum Signal [ Low, High ]
enum Power [ Low, High ]
```

`Low` inside a pattern is resolved by the type of the subject.

### 3. Payload variants use the same rule

`Name(...)` is a constructor and destructuring pattern and is resolved against
the expected enum type exactly as the bare form is. Its sub-patterns are then
checked against the payload positions of that variant.

### 4. Predeclared constructors keep their status

`Some`, `None`, `Ok`, `Err`, `Completed` and `Cancelled` remain non-shadowable
constructor names and resolve against their expected constructed types
(`Option<T>`, `Result<T,E>`, `TaskResult<T>`). They are never bindings.

### 5. Qualified constructor patterns

The `pattern` production gains a qualified constructor path, using the existing
TOS qualified-name punctuation. No `::` is introduced:

```text
pattern          = "_"
                 | pattern_path ( "(" pattern_list? ")" )?
                 | "(" pattern_list ")" ;
pattern_path     = pattern_name ( "." identifier )* ;
pattern_name     = identifier | predeclared_value ;
```

This stays deterministic. A single identifier remains exactly one syntactic
alternative — a `pattern_path` with no suffix — and whether it denotes a
constructor or a binding is decided during resolution, not during parsing. The
`.` suffix is unambiguous, because no other production may follow a pattern name
with a dot.

A `pattern_path` containing at least one `.` is **always** a constructor path
and is **never** a binding. A local variant MAY be written either in the short
form `Low`, when the expected enum type determines it, or explicitly as
`Signal.Low`. An imported variant uses the qualified form and resolves through
ordinary module and import resolution, so `other.Signal.Low` names the `Low`
variant of `Signal` in the module bound to `other`.

A qualified path that names no reachable variant is an error rather than a
binding. It cannot degrade into a catch-all.

### 6. Conformance evidence

The corpus gains positive and negative cases for at least: a local bare unit
variant; a bare binding where the expected type has no such variant; two enums
sharing a variant name disambiguated by the expected type; payload variant
destructuring; an explicitly qualified local variant; a qualified imported
variant; an unknown qualified variant; an exhaustive match over bare variants;
wildcard and binding exhaustiveness; and a case proving resolution does not
depend on capitalization.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-15 is served: the language now states
  precisely what a bare pattern name means instead of leaving two readings.
- **Canonical representation after the change:** unchanged. Existing canonical
  source stays valid; the qualified form is additive.
- **Trusted-base impact:** none.
- **Source-to-runtime impact:** none directly. Pattern resolution becomes
  reproducible from the module's own declarations plus its import closure.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** none. Resolution reads only declared types, so hostile
  source cannot make a pattern silently change meaning by introducing a name.
- **Performance contract:** none applicable.
- **Compatibility profile:** TOS Core 1.0. Making the rule type-directed fixes it
  for V1; changing it later is a versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** the ten conformance cases in section 6,
  plus checker unit tests for the resolution rule and the qualified path form.

## Consequences

Pattern resolution now requires the expected type, so it belongs to the type
slice rather than to name resolution. The checker's current name-resolution
slice is unaffected: both readings admitted the same set of resolvable names, so
no diagnostic changes.

Variant names stop being module-global for pattern purposes, which removes a
collision that would otherwise force every enum in a module to use distinct
variant names.

## Alternatives considered

**Resolve against any constructor in scope.** Rejected: it makes variant names
module-global, so adding a variant whose name matches an existing local silently
changes a binding into a match, and two enums cannot share a variant name.

**Require an explicit form for every variant pattern.** Rejected: it follows the
literal reading of docs/39 section 2 but invalidates accepted canonical source,
and the corpus is accepted evidence rather than a draft.

**Adopt a capitalization convention.** Rejected: V1 has no such convention
anywhere else, and it would make meaning depend on spelling rather than on
declarations.

## Open matter deliberately not decided

Whether `let` and `for` patterns must be irrefutable, and what a refutable
pattern in those positions reports, is a separate question. It is not settled
here and must not be inferred from an implementation.
