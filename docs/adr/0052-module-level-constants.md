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

The `identifier` admitted in a `const_expression` can only be a named constant:
V1 grants no user generics, so there is nothing else a name in that position
could denote. So V1 already requires **some** module-level constant to be
evaluable at compile time — otherwise `array<T, CAPACITY>` cannot be written,
and the grammar admits it.

**"Some" is the exact strength of this argument, and it is worth not
overstating.** It does not follow that *every* `const` must be compile-time. A
language may hold one form that is compile-time and another that is a runtime
object — C++ separates `const` from `constexpr`, Rust separates `const` from
`static` — and it may equally hold a single runtime form with a narrower
syntactic rule for the array-size position alone. Both are coherent designs, and
option B below is stated in that stronger form rather than the strawman the first
draft attacked.

What the argument does settle is that a design in which `CAPACITY` is a runtime
object *everywhere* is not available: the array-size position needs a
compile-time value, and something has to supply it.

Given a compile-time form, question 2 answers itself: a value computed at
compile time has no evaluation moment to place, no trap to order and no
accounting to charge.

It also dissolves question 3. A compile-time constant does not need to exist in
the IR at all — like a type, it is consumed during lowering. A `pub const`
imported by another module is resolved by the same source-set step that binds an
imported function's signature: the importer substitutes the value, and the
exporter's content id is already inside the importer's `dependency_digest`, so
changing the constant changes the importing module's digest and invalidates its
cache. docs/42's promise is kept, and `tos-ir/v1` is untouched.

## What neither option is

A reader comparing the two naturally asks whether the runtime option lets a
constant be initialized from something supplied at launch. It does not, and the
question is worth answering in the ADR because the answer is a property of the
language rather than of either option.

A module has no parameters. Nothing is passed to it. Under B an initializer runs
before any function runs, so the only things it can read are literals, other
constants and its own imports — the same inputs A has, with the addition that it
may *compute* over them by calling functions. B buys computation, not
configuration.

Values that genuinely come from outside arrive by a different route, and V1
already fixes it: a launcher grants authority as a capability, and docs/42 §2
states in terms that a capability "cannot be a `const`, record field, serialized
value, numeric conversion, equality key, or deserialized replacement". A module
that must behave differently per deployment reads that difference through a
capability operation or, from Stage 5, through `/config` — never through a
constant, under either option here.

The distinction matters for a second reason. Under A a launcher can read a
module's constants **before** starting it, from source or from the lowered
module, which is the same property ADR-0051 relies on when it has the launcher
decide a capability grant from the verified module image. Under B a constant's
value does not exist until the module has run, so it cannot participate in any
decision made before launch.

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

A carries one further clause, which exists because of what it forecloses: **the
initializer form is not widened later.** If a runtime-initialized object is ever
wanted, it arrives as its own item form with its own keyword, its own
initialization contract and its own accounting story — never by relaxing what
may initialize a `const`. Widening it later would silently change *when* existing
source evaluates, and source that changes meaning without changing text is the
one outcome a versioned language contract exists to prevent.

*Cost:* a constant cannot be the result of a call, and an aggregate constant is
constructed at each use rather than once. Neither is free, and both are stated
below rather than buried.

**B — a constant is a runtime object, with a compile-time rule for array sizes.**
`const` denotes a value initialized once before any function runs, admitting an
arbitrary expression; the array-size position separately requires its identifier
to name a constant whose initializer is a literal constant expression. This is
coherent, and it is roughly where C++ stood before `constexpr`.

*Cost, in two parts.* The first is that `const` means two different things
depending on where it is read, and the reader must know the position to know
which. `array<u8, CAPACITY>` compiles or does not depending on how `CAPACITY`
was written, and the diagnostic for the bad case has to explain a distinction the
declaration itself does not show.

The second is architectural: a module-initialization phase V1 does not have,
executing source **outside any function's declared resource envelope and outside
any verifier receipt bound to a call** — the two properties Stage 2 closed on.
It would also need a defined initialization order across a dependency closure,
and initialization order between compilation units is a famous defect source in
every language that has it.

B is a live option; it is not the cheap one and it is not obviously the nicer
language, but it is the one that eventually gives a service a real shared,
once-computed table. A chooses to make that a separate, explicit, later decision
instead of the default meaning of the word `const`.

**C — remove `const` from the V1 surface.** Not a live option, and it is listed
only so its absence is not mistaken for an oversight: it would change the
accepted grammar, contradict docs/42 twice, and delete the only form that can
supply the `identifier` the array-size grammar already admits. An option list
padded with a non-option to look balanced is its own kind of dishonesty.

## Recommendation

**A** — as one of two live options, not as the only survivor.

The choice between A and B is not about what is cheap to build; both are
implementable. It is about which of two things the word `const` should mean, and
about when TOS is willing to execute source outside its own accounting.

A says a constant is a value the compiler knows, and takes as its cost that a
constant cannot be computed by running something. B says a constant is an object
the system creates, and takes as its cost a phase in which source runs with no
declared resource envelope and no verifier receipt — plus a `const` whose
meaning depends on where it is read, since the array-size position still needs a
compile-time value.

The recommendation is A because those two costs are not comparable in this
system. "You cannot call a function to build a constant" is a restriction a
programmer meets at the declaration, reads in one diagnostic, and works around
in one line. "Some source executes before anything is accounted for or verified"
is an exception to the two properties Stage 2 was closed on, and exceptions to
those do not stay small.

If the Project Architect wants the runtime object, B is the honest way to get
it, and it should be taken deliberately with its initialization contract written
first — not arrived at by letting `const` quietly widen.

## What each option costs a working system

Stated concretely, because "compile-time versus runtime" understates how
differently the two behave in this system.

| | A | B |
|---|---|---|
| When the value exists | at lowering | after the module's initializer runs |
| Readable before launch | yes — from source and from the module image | no |
| In the module digest | yes: changing it changes the digest and invalidates the cache | no: only the code that computes it is |
| Startup | unchanged | gains an initialization phase that can fail |
| Failure modes at start | `CapabilityDenied` | `CapabilityDenied`, plus a trapping initializer, plus an unsatisfiable initialization order |
| Resource accounting | none to charge | initializer work is charged to nothing V1 defines |
| Process memory | none; values are substituted | constants occupy the process's grant for its lifetime |
| Ordering across a closure | none | a defined order, and a diagnostic for cycles |
| What a programmer gives up | a constant computed by a call | nothing, at the cost of everything in this column |

The rows about the module digest and about readability before launch are the
ones specific to TOS rather than to language design in general, and they point
the same way. A constant that is part of the module digest makes cache
invalidation exact — change the number, get a different module identity. A
constant a launcher can read before starting a process is a constant that can
take part in a launch decision, which is how ADR-0051 has manifests work.

Under A, a table that genuinely needs computing is computed by a function and
passed, or — from Stage 3 — computed once by a service and served over IPC. The
expressiveness gap is real and it is bounded.

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

A second question followed — whether one option was now left. It was not, and
the revision that produced the answer had overstated its own argument. The
array-size grammar proves that *some* constant must be compile-time, not that
every one must be; a language may carry two forms, or one runtime form with a
narrower rule for array sizes. B is therefore restated in its strongest form
rather than as the strawman the previous draft attacked, C is marked as the
non-option it is instead of padding the list, and A gains the clause that its
initializer is never widened later — which is the property the question
surfaced, and the one that keeps existing source from changing meaning without
changing text.
