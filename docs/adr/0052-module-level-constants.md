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

The third is worse: making `pub const` importable, as docs/42 already promises,
requires `tos-ir/v1` to carry a named constant and an import of one. **That is
an extension of a closed Stage 2 contract**, and the rule this project works
under says to stop at that boundary rather than widen it inside an
implementation.

## Options

**A — literal-initialized constants, substituted at use.** Narrow the initializer
to literals and constructors over literals: no calls, no effects, nothing that
can trap. Lowering substitutes the value at each use, which needs no IR change
for scalars and, for aggregates, emits the same construction the source would
have written inline. Evaluation-order questions disappear because the
initializer cannot observe anything. Cross-module constant import stays
unimplemented, so docs/42's sentence is narrowed to say constants are
module-local in V1 and importable from V1.1.

*Cost:* a documented narrowing of two accepted sentences, and an aggregate
constant costs its construction at every use rather than once.

**B — full constants with module initialization.** Admit an arbitrary
expression, evaluate once before any function runs, and give `tos-ir/v1` a named
constant table plus constant imports. This is what docs/42 promises, read
literally.

*Cost:* a module-initialization phase V1 does not have, with its own ordering,
trap and resource-accounting rules across a dependency closure; an extension to
a closed IR contract; and new verifier rules for both. This is a language
version's worth of work, not a gap fix.

**C — remove `const` from the V1 surface.** Retract the item form.

*Cost:* changes the accepted grammar, contradicts docs/42 twice, and removes a
form that costs nothing under option A. Listed for completeness and not
recommended.

## Recommendation

**A.** It makes an accepted form work, keeps `tos-ir/v1` closed, introduces no
evaluation phase the language does not otherwise have, and leaves B available as
a V1.1 decision once there is a real service source set asking for shared
constants across modules. The narrowing it requires is two sentences, and both
would otherwise be promises the implementation cannot keep.

Under A the implementation work is Level 1: restrict the checker to
literal-and-constructor initializers with a diagnostic for anything else, and
substitute at use in lowering. The diagnostic code is part of this decision, not
an implementation choice, and is assigned when the option is chosen.

## Boundary

Nothing is implemented under this ADR until it is accepted. The current state —
declaration accepted, use refused with a named gap — is the honest interim, and
Stage 3 Phase 1 scopes itself to function imports so that it does not depend on
the answer.
