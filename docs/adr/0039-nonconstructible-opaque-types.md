<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0039: `E1213_NONCONSTRUCTIBLE_TYPE` for opaque non-capability handles

- Status: Accepted (Project Architect-approved), revision 4
- Date: 2026-08-11; revision 4 on 2026-08-21
- Decision level: 2 — allocates a diagnostic code conformance evidence will
  depend on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11; revision 4
  approved 2026-08-21 together with ADR-0064 option B
- Supersedes: revision 1, whose type set wrongly included `TaskResult<T>` and
  omitted `Shared<T>`; revision 2, which promised the code for constructor and
  aggregate forms without saying which of them V1 can express; and revision 3,
  which excluded the constructor form on a factual premise the working frontend
  disproved

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

- an `as` conversion whose target type is one of the nonconstructible types
  (`operation=as`);
- an `as` conversion whose operand type is one of them (`operation=as`);
- **a nonconstructible type applied to arguments** — `Event()`, `Task(1i32)`,
  `Mutex(1i32)`, `MutexGuard(0i32)` — which `docs/39` §5's single
  Call/Construct form makes *the* constructor form (`operation=construct`).

That is the whole list.

### The boundary, and it is normative

**The position decides, never the spelling.**

| form | code | `operation` |
|---|---|---|
| `Event()`, `Task(1i32)`, `MutexGuard(0i32)` — the type applied to arguments | `E1213_NONCONSTRUCTIBLE_TYPE` | `construct` |
| `Event` — the same name written alone in value position | `E1202_UNKNOWN_VALUE_NAME` | *(none)* |
| `value as Task<i32>`, `lock as u64` | `E1213_NONCONSTRUCTIBLE_TYPE` | `as` |
| `system.time.Clock()` — a capability | `E1502_FORGED_CAPABILITY` | `construct` |

A name written alone constructs nothing, and a diagnostic saying it did would be
false about the source in front of it. It resolves to no value, which is exactly
what `E1202` says; a frontend may say in its *message* that the name is a type
and where a value of it comes from, because the code is the contract and the
message is not.

An implementation must not reach this code by asking whether a name is
nonconstructible and reporting a construction wherever it appears. That rule
keyed on the spelling is what revision 3 was reacting to, and it turns every
mention of a predeclared type into a fabrication attempt.

The aggregate forms revision 2 also promised do **not** return with this
revision. What returns is the constructor form and nothing else, because that is
the form `docs/39` §5 gives V1 and the one the frontend can identify by position.

### Why revision 3 excluded the constructor form, and why that is reversed

Revision 3 held that "promising `E1213` for those forms would mean widening the
grammar to let them through to the type stage purely so a diagnostic could fire".
**That premise is false, and the working frontend is the disproof.** The
constructor form is already grammatical — `docs/39` §5 gives calls and
constructions one form, and its callee is an ordinary name — so the finding is
produced during name resolution, before any type exists, with no grammar change
whatever. Revision 3's own rejected alternative ("widen the grammar so `Event()`
reaches the type stage") is one way to reach the diagnostic and not the way it is
reached.

What remains true from revision 3 is the half this revision keeps: the bare name.
Its reasoning — that such a name resolves to nothing in value position — is
accurate about `Event` and inaccurate about `Event()`, and separating the two is
the whole of the change.

The semantic reason for covering the constructor form is `docs/40` §3's rule
itself: opaque runtime handles may not be made out of data. The capability half
of that rule already covers construction (`E1502_FORGED_CAPABILITY` with
`operation=construct`), and a system in which `system.time.Clock()` is a forgery
while `Mutex(1i32)` is a misspelling answers one rule two ways.

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

The three guard types of ADR-0036 are in this set: that ADR is accepted, and its
§1 — "writing one as a constructor is the nonconstructible-type error of
ADR-0039" — is a sentence this revision makes true again. Under revision 3 it
pointed at a code that had stopped covering the form, which is the drift ADR-0064
recorded.

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

**The boundary needs a vector on each side, and one alone is worse than none.**
`E1213` for the constructor form is R070 (`reject/forged-guard.tos`), which is
also ADR-0036 §7's required negative; `E1202` for the bare name is R081
(`reject/predeclared-type-in-value-position.tos`). Revision 3 declined to write a
vector for this form on the reasoning that its answer was settled, and the effect
was that the sentence carrying the answer became prose no gate reads — which is
how the answer could be changed in code with nothing turning red. Whichever
answer is accepted, both sides are recorded in the corpus from now on.

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
- **Tests:** the conformance cases above, both sides of the boundary; checker
  unit tests for both `as` directions, for the constructor form with
  `operation=construct`, for the bare name being `E1202` and carrying no
  `operation` at all, for `TaskResult<T>` staying outside the set, and for the
  precedence against `E1212` and `E1502`; and the mechanical gate.

## Consequences

The `as` rule of `docs/40` section 3 becomes completely implementable, and the
last silent acceptance in the type slice closes.

Revision 4 adds the consequence that the rule's two halves now answer alike: an
opaque runtime handle cannot be made out of data by conversion or by
construction, and a capability and a lock are refused in the same shape with the
same `operation` field.

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
Rejected, and still rejected in revision 4: it would change what V1 source *is*
to improve a diagnostic on a form that is already rejected. What revision 4
establishes is that this was never the choice — the form is grammatical already
and the finding is produced during name resolution, so the diagnostic and the
grammar are independent.

**Report the construction wherever the name appears.** Rejected: it is what the
implementation did between `b16cc6c` and ADR-0064, and it made `Event` alone —
where nothing is applied to anything — carry `operation=construct`. A diagnostic
must be true of the source in front of it.
