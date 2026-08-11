<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0039: `E1213_NONCONSTRUCTIBLE_TYPE` for opaque non-capability handles

- Status: Accepted (Project Architect-approved), revision 3
- Date: 2026-08-11
- Decision level: 2 — allocates a diagnostic code conformance evidence will
  depend on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11
- Supersedes: revision 1, whose type set wrongly included `TaskResult<T>` and
  omitted `Shared<T>`; and revision 2, which promised the code for constructor
  and aggregate forms that V1 source cannot express in the first place

## Context

`docs/40` section 3 says an attempt to use `as` with a capability, region, DMA
region, task, synchronization object, function, closure or pointer-like host
value "is not a generic conversion error: it is `E1502_FORGED_CAPABILITY` for a
capability and **the corresponding nonconstructible-type error** for the other
opaque types".

No accepted document names that error. So the implementation reports nothing for
seven of the eight cases: casting a task, a region, a mutex, a closure or a
function is silently accepted by the type slice, because `E1212` is explicitly
excluded and nothing else applies. That is the gap recorded in `PROGRESS.md` as
an unresolved contract boundary, and it is the last one blocking a complete
`as`-conversion rule.

## Decision

### 1. `E1213_NONCONSTRUCTIBLE_TYPE`

Stage `type`. An operation attempts to bring into existence a value of a type
that V1 makes nonconstructible from source. The operations are:

- an `as` conversion whose target type is one of the nonconstructible types;
- an `as` conversion whose operand type is one of them.

That is the whole list, and it is short for a reason. A predeclared type is not
an expression primary or callee in V1, so `Event()`, `Task(1i32)` and
`Mutex(1i32)` are not fabrication attempts this code has to catch — they are
names that resolve to nothing in value position, and the frontend already
reports each as `E1202_UNKNOWN_VALUE_NAME`. Verified against the reference
frontend, not assumed.

Promising `E1213` for those forms would mean widening the grammar to let them
through to the type stage purely so a diagnostic could fire, which is a worse
outcome than the rejection they already get. The grammar is not widened, and any
future V1 operation that can genuinely express such a fabrication comes under
this code when it exists.

The nonconstructible types are: `Task<T>`, `Shared<T>`, `Region<T>`,
`DmaRegion<T>`, `Mutex<T>`, `RwLock<T>`, `Channel<T>`, `Event`, `Semaphore`,
`Barrier`, `Latch`, the three atomic types, `slice<T>`, and any function or
closure type.

`TaskResult<T>` is **not** among them. `docs/39` section 2 gives `Completed` and
`Cancelled` as predeclared constructors in expression position, so a
`TaskResult<T>` is an ordinary affine result value that source is meant to
build. What may not be fabricated is the `Task<T>` a join consumes, not the
value the join produces.

`Shared<T>` **is** among them. `docs/40` makes it the handle a typed `share`
contract yields; a cast or constructor producing one would manufacture sharing
that no operation granted.

The three guard types of ADR-0036 join this set when that ADR is accepted. They
are named here rather than assumed, because until it is accepted they do not
exist and this list would be citing types the contract does not have.

The diagnostic carries the type as spelled and which operation attempted it.

### 2. Precedence

1. a capability is `E1502_FORGED_CAPABILITY` — it is more specific and names
   authority, which is the thing that matters most;
2. any other nonconstructible type is `E1213_NONCONSTRUCTIBLE_TYPE`;
3. only a conversion between ordinary value types reaches
   `E1212_INVALID_AS_CONVERSION`.

One attempt produces one diagnostic. `E1212` is never reported for a type this
code covers, which is what `docs/40` section 3 means by "not a generic
conversion error".

### 3. What it does not cover

A nonconstructible value obtained the way the language provides — a task from
`spawn`, a guard from a lock, a region from a grant — is ordinary and correct.
This code is about constructing one out of data, never about holding one.

### 4. Conformance evidence

At least: a negative casting an integer to `Task<i32>`; a negative casting a
`Mutex<i32>` to an integer; a negative casting an integer to `Shared<i32>`; a
positive building a `TaskResult<T>` with `Completed` and `Cancelled`, proving
the code does not fire on a value source is meant to build; and a positive
obtaining a task from `spawn` and using it, proving it does not fire on the
legitimate path either.

A vector for `Event()` is deliberately absent: R-vectors record the code a form
actually produces, and that form produces `E1202_UNKNOWN_VALUE_NAME`.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-09 is served —
  the code becomes part of the versioned diagnostic boundary; I-15 is served by
  replacing "the corresponding nonconstructible-type error" with a name.
- **Canonical representation:** unchanged. No accepted source becomes invalid:
  every case this rejects was already an error under `docs/40` section 3, with
  no code to report it.
- **Threat-model impact:** positive. Fabricating a task, a lock or a region out
  of integer data is the same class of forgery as fabricating a capability, and
  it was silently accepted.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the five conformance cases, checker unit tests for both `as`
  directions, for every type in the set, for `TaskResult<T>` staying outside it,
  for a predeclared type in value position still being `E1202`, and for the
  precedence against `E1212` and `E1502`, and the mechanical gate.

## Consequences

The `as` rule of `docs/40` section 3 becomes completely implementable, and the
last silent acceptance in the type slice closes.

The cost is one more code fixed for TOS Core 1.0.

## Alternatives considered

**Reuse `E1212_INVALID_AS_CONVERSION`.** Rejected: `docs/40` section 3 says in
so many words that this is not a generic conversion error, and conformance
tooling could not tell a narrowing mistake from a forgery attempt.

**Reuse `E1502_FORGED_CAPABILITY` for everything opaque.** Rejected: a task is
not authority, and widening a capability code to cover non-authority values
would make every audit of that code less meaningful.

**Leave the `as` cases unreported.** Rejected: it leaves a stated rule
unenforced and a forgery path open.

**Widen the grammar so `Event()` reaches the type stage and gets `E1213`.**
Rejected: it would change what V1 source *is* to improve a diagnostic on a form
that is already rejected, and a grammar that admits nonsense so a later stage can
name it is worse than one that does not admit it.
