<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0052: What a module-level `const` is

- Status: **Proposed**
- Date: 2026-08-12
- Decision level: 2 — fixes the observable meaning of an accepted V1 item form,
  and decides whether `tos-ir/v1` gains a representation for it
- Project Architect approval: *(pending)*

## The gap

`const` is one of the six item forms the accepted V1 grammar admits:

```text
item       = visibility? resource_decl | visibility? record_decl
           | visibility? enum_decl     | visibility? const_decl
           | visibility? function_decl | visibility? extern_decl ;
const_decl = "const" identifier ":" type "=" expression ";" ;
```

docs/42 section 1 puts constants in the cross-module surface twice: "`import
a.b as c;` imports exported types, functions, and constants under `c`", and
"items declare types, constants, resources, and functions only".

Nothing else in the accepted set says what one *means*. docs/40 defines
evaluation order, bindings, arithmetic and traps, and never mentions module-level
constants. So the accepted contract does not answer:

1. **What may initialize one.** The grammar says `expression`, not
   `const_expression` — the restricted arithmetic form V1 uses for array sizes.
   Read literally, a constant may be initialized by a call, an arithmetic
   expression that can trap on overflow or division, or an aggregate
   constructor.
2. **When it is evaluated.** Once per module, or substituted at each use? The
   two differ observably as soon as the initializer can trap: substitution traps
   at the use site, evaluate-once traps before any function runs — at a point
   V1 does not define, because V1 has no module-initialization phase.
3. **What `pub const` exports.** docs/42 says constants are importable, and
   `tos-ir/v1` has no way to carry one across a module boundary: `Import` names
   a module and a binding, `Constant` is a scalar pool entry, and there is no
   named module-constant table.

## What is measured, not assumed

Probed against the production frontend at `7b0847d`:

- `pub const MANIFEST: Manifest = Manifest(provides: "…", restartable: true);`
  parses, type-checks, lowers, verifies and executes — as long as nothing reads
  it. The declaration is accepted and then dropped.
- Reading it refuses at lowering. Before this ADR the refusal said
  `construct=unbound place`, which describes a lowering data structure rather
  than the source; it now says `construct=module-level const`.
- `tos_ir::Constant` is `Unit | Bool | Int | Size | Duration | Text | Bytes`.
  `Module.constants` is an unnamed pool of literal values used by instructions,
  not a table of declared constants.

So today an accepted declaration form is silently ignored, and any use of it is
refused with a gap. That is honest but incomplete, and it is not a state the
language should stay in.

## Why this is a decision and not a fix

Two of the three questions above are language semantics — what may initialize a
constant and when it evaluates — and their answers are observable in traps and
in evaluation order, which docs/40 fixes precisely for everything else.

The third looked worse than it is, and the first draft of this ADR said so:
making `pub const` importable, as docs/42 promises, appeared to require
`tos-ir/v1` to carry a named constant and an import of one — an extension of a
closed contract. That is true only if a constant is a runtime object. Section
"What V1 already decided" below shows it is not, and the conclusion changes.

## What V1 already decided

The accepted contract is not silent about what kind of thing a constant is. It
says so in the one place a constant is unavoidably used:

- `array_type = "array" "<" type "," const_expression ">"`, and
  `const_primary = integer | size | identifier | "(" const_expression ")"`;
- docs/40: "`array<T, N>` takes one type argument and one **compile-time** `size`
  constant".

The `identifier` admitted in a `const_expression` can only be a named constant.
So V1 already requires a module-level constant to be evaluable at compile time —
otherwise `array<T, CAPACITY>` cannot be written, and the grammar admits it.

That settles question 1 in the language's own terms rather than by an
implementer's preference, and it settles question 2 as a consequence: a value
computed at compile time has no evaluation moment to place, no trap to order and
no accounting to charge.

It also dissolves question 3. A compile-time constant does not need to exist in
the IR at all — like a type, it is consumed during lowering. A `pub const`
imported by another module is resolved by the same source-set step that binds an
imported function's signature: the importer substitutes the value, and the
exporter's content id is already inside the importer's `dependency_digest`, so
changing the constant changes the importing module's digest and invalidates its
cache. docs/42's promise is kept, and `tos-ir/v1` is untouched.

## Options

**A — a constant is a compile-time value.** The initializer is a
`const_expression` — the arithmetic form V1 already defines — extended to
literals of every scalar type, and to record, enum and array constructors whose
arguments are themselves constant. No calls, no effects, nothing that can trap
or observe anything. Lowering substitutes the value at each use, including
across a module boundary, so `pub const` is importable as docs/42 says.

Written out, that admits everything a systems module actually wants:

```tos
pub const PAGE:     size   = 4096;
pub const WINDOW:   size   = PAGE * 4;
pub const ENDPOINT: string = "net.adapter.v1";
pub const LIMITS:   Limits = Limits(depth: 8i32, width: 4i32);

pub fn buffer() -> array<u8, WINDOW> { … }
```

*Cost:* a constant cannot be the result of a call, and an aggregate constant is
constructed at each use rather than once. Neither is free, and both are stated
below rather than buried.

**B — a constant is a runtime object, initialized once.** Admit an arbitrary
expression, evaluate it once before any function runs, and give `tos-ir/v1` a
named constant table plus constant imports.

*Cost:* a module-initialization phase V1 does not have. That phase would execute
source **outside any function's declared resource envelope and outside any
verifier receipt bound to a call** — the two properties Stage 2 closed on. It
would also need a defined initialization order across a dependency closure, and
initialization order between compilation units is a famous source of defects in
every language that has it. This is not merely expensive; for this language it
is worse.

**C — remove `const` from the V1 surface.** Retract the item form.

*Cost:* changes the accepted grammar, contradicts docs/42 twice, and removes a
form that costs nothing under option A. Listed for completeness and not
recommended.

## Recommendation

**A**, and the reason is what V1 already says rather than what is cheap to
build. `array<T, N>` requires a compile-time `size` constant and the grammar
lets a named constant be that `N`. A language cannot hold both "constants are
compile-time" for array sizes and "constants are runtime objects" everywhere
else without deciding which one `CAPACITY` is at the point of use. V1 already
chose; this ADR writes the choice down and follows it to its consequences.

B is rejected on architecture, not on effort. TOS accounts every execution
against a declared envelope and executes nothing the verifier has not issued a
receipt for. A module-initialization phase is, by construction, execution with
neither. Buying convenience with an exception to both is the trade this project
does not make.

The honest cost of A is stated plainly: no `const` computed by a function, and
an aggregate constant paid for at each use. The first is a real restriction that
a later language version may lift with a proper compile-time-evaluation model —
which is a much larger and better-founded decision than smuggling one in through
an initializer. The second is a code-size question, not a semantics question,
and an optimizer may share identical constructions later without changing what
the source means.

Under A the implementation work is Level 1: restrict the checker to constant
initializers with a diagnostic for anything else, and substitute at use in
lowering, including across a module boundary through the Phase 1 source-set
step. The diagnostic code is part of this decision, not an implementation
choice, and is assigned when the option is chosen.

## Boundary

Nothing is implemented under this ADR until it is accepted. The current state —
declaration accepted, use refused with a named gap — is the honest interim, and
Stage 3 Phase 1 scopes itself to function imports so that it does not depend on
the answer.

## Revision note

The first draft of this ADR recommended A partly on cost: it kept `tos-ir/v1`
closed and avoided work. That was a weak reason for a language decision, and the
Project Architect asked whether the recommendation came from ease of
implementation or from what makes a better language.

Re-examined on the language question, the recommendation stands and two of its
terms change. The initializer is V1's own `const_expression` rather than
literals only, because `const BUFFER: size = PAGE * 4;` is a legitimate and
common pattern that the array-size grammar already admits — forbidding it would
have made a worse language to save an implementation from constant folding. And
cross-module constant import is kept rather than narrowed out of docs/42,
because a compile-time constant needs no IR representation to cross a module
boundary. The cost argument turned out to be an artifact of assuming a constant
is a runtime object.
