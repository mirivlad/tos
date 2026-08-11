<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0037: TOS Core V1 region and DMA-region transferability

- Status: **Proposed (revision 3)** — the region model is accepted; this
  revision adds the `share` operation the model needs and stops at one boundary
  it uncovers
- Date: 2026-08-11
- Decision level: 2 — fixes the `Transferable`, shareable and mutable facts of
  two accepted V1 type constructors
- Project Architect approval: *(pending)*
- Supersedes: revision 1, whose share model let a shareable DMA region become
  `Shared<DmaRegion<T>>` and be copied into several tasks; and revision 2, which
  used a `share(region)` form that the accepted V1 word inventory and
  `predeclared-function` list do not contain

## Context

`docs/40` section 6 lists "a mutable region" among the values a task may not
capture, and `docs/40` section 5 makes regions non-`Copy`. `docs/42` section 4
makes a capability transferable only when its interface declares it so.

Nothing says how a checker decides whether a given `Region<T>` is mutable or
shareable. The type constructor alone does not say — a region granted read-only
and a region granted for writing have the same written type — so the ownership
slice classified nothing and reported nothing, which is where the implementation
correctly stopped rather than guessing.

The missing piece is that a region's rights live in its **grant**, and V1 source
has no way to write a grant down.

## Decision

### 1. A region's rights are part of its type

`Region<T>` and `DmaRegion<T>` gain a declared access mode, written as the
grant that produced them:

```text
Region<T>          an immutably granted region: readable, shareable, Transferable
Region<mut T>      a mutably granted region: readable and writable,
                   not shareable, not Transferable
DmaRegion<T>       an immutably granted device-visible region
DmaRegion<mut T>   a mutably granted device-visible region
```

`mut` inside the type argument is the only place V1 admits it in a type, and it
is admitted for exactly these two constructors. It is not a general mutability
qualifier and introduces no `mut T` elsewhere.

### 2. The four facts

| Type | `Copy` | mutable | Shareable | `Transferable` |
|---|---|---|---|---|
| `Region<T>` | no | no | yes | yes |
| `Region<mut T>` | no | yes | no | no |
| `DmaRegion<T>` | no | no | **no** | **no** |
| `DmaRegion<mut T>` | no | yes | no | no |

Both DMA variants are conservative in V1. Making `DmaRegion<T>` shareable would
let it become `Shared<DmaRegion<T>>`, and a `Shared<T>` is `Copy`, so the handle
could then be copied into several tasks — which is exactly the crossing the rule
"a DMA region never crosses a task boundary" exists to forbid. A narrower
statement that can be walked around is worse than none. Wider DMA sharing or
transfer may arrive later through a typed driver or device contract that says
what makes it safe; it is not something the language grants by default.

### 3. Sharing is an explicit typed operation, never an implicit copy

`Region<T>` is affine like every other non-`Copy` value, so its handle has one
owner. `Transferable` means that ownership may move into **exactly one** task —
not that the handle may be duplicated.

Using one region from several tasks is written `share(region)`. Revision 2 used
that form without it existing: `share` is in neither the accepted V1 word
inventory of `docs/39` section 2 nor its `predeclared-function` list. Section 4
adds it, because the model is not expressible without it.

`Shared<T>` is the `Copy` handle `docs/40` already defines, so the copies the
several tasks hold are copies of a `Shared`, produced by a typed operation that
appears in the source and in the IR. There is no path where an affine region
handle is silently duplicated because two tasks happened to name it: that would
make an ownership transfer look like a read.

### 4. `share` as a predeclared operation

`share` joins `predeclared-function` in `docs/39` section 2, alongside `to_*`
and `wrapping_*`. It is a language operation, not a library call and not ambient
runtime behaviour.

**Type rule.**

```text
share(T) -> Shared<T>    only when T is transitively immutable and Shareable
```

Transitively immutable means `T` and every type reachable from it contains no
mutable region, no mutable borrow and no guard. Shareable is the column of the
section 2 table: `Region<T>` is, `Region<mut T>` and both `DmaRegion` variants
are not.

**Ownership.** `share` consumes its argument. The affine handle is moved into
the operation and the caller holds `Shared<T>` afterwards; the original name is
moved-from, and using it is `E1301_USE_AFTER_MOVE` like any other move. A
`share` that left the original usable would hand out two roots to one region,
which is the duplication this whole section exists to prevent.

**Verifier-visible and accounted.** `share` lowers to its own IR operation, not
to an opaque helper call: `docs/43` section 3 forbids hiding shared-memory
access behind one. It carries its argument, its result type and its source span,
`V2021_REGION` rechecks the Shareable requirement independently, and the
resulting `Shared<T>` counts against the module's declared `shared` resource
limit, so sharing is bounded by the envelope like every other resource.

### 5. One boundary this uncovers — an Architect decision

A `share` whose argument is not Shareable — `share(dma)`, `share(mutable)` —
has **no diagnostic code to report it**. The registry has no general
argument-type-mismatch code at all: `E1210` is integer agreement, `E1211` is
indexing, `E1212` is `as`, `E1222` is a return. Nothing covers "this call's
argument does not satisfy the operation's declared requirement".

Rather than borrow one of those for a condition it does not describe, this ADR
stops here and proposes the narrow options:

1. **`E1214_INVALID_SHARE`** — one code for exactly this operation, with a
   `reason` field (`not_shareable`, `mutable`, `dma`). Smallest possible
   addition; adds a code that only ever fires for one operation.
2. **`E1215_ARGUMENT_TYPE_MISMATCH`** — a general code for an argument that does
   not satisfy a declared parameter or predeclared-operation requirement. Wider,
   and it fills a gap the registry has independently of `share`: today an
   ordinary call with a wrongly typed argument has no code either.
3. **Extend `E1210`'s condition** to argument agreement generally. Rejected here
   as a suggestion — it is named to be dismissed: `E1210` is about integer
   types, and widening an accepted stable condition to cover unrelated cases is
   what the registry discipline exists to prevent.

Option 2 is the recommendation: the gap it closes is real and larger than
`share`, and one general code is easier for conformance tooling to reason about
than a family of operation-specific ones. But allocating a code is a versioned
language decision, so it is the Project Architect's, and this ADR is not
Accepted until it is made.

### 6. Diagnostics

No new code. Capturing a non-`Transferable` region into a task is
`E1304_INVALID_TASK_CAPTURE` with `reason=mutable region` or `reason=DMA
region`; into a closure it is `E1305_INVALID_CLOSURE_CAPTURE` with the same
reasons. Writing through a `Region<T>` is `E1201_ASSIGN_TO_IMMUTABLE`.

`V2021_REGION` gains these as verifier rules, so the IR carries the mode in its
type table and the verifier rechecks it rather than trusting the frontend.

### 7. Conformance evidence

At least: a positive moving a `Region<T>` into one task; a positive sharing one
through `share(region)` and using the `Shared<Region<T>>` from two tasks; a
negative capturing a `Region<T>` handle into two tasks without `share`; a
negative capturing a `Region<mut T>` into a task; a negative capturing a
`DmaRegion<T>` into a task; a negative applying `share` to a `DmaRegion<T>` and to a
`Region<mut T>` under whichever code section 5 settles on; a negative using a
region after `share` consumed it (`E1301`); a negative writing through a
`Region<T>`; and a positive writing through a
`Region<mut T>`. Each capture and share negative has a forged-IR counterpart for
`V2021_REGION`, so the checker and the verifier prove the same rule
independently.

## Architecture impact statement

- **Change level:** 2 — the type facts, plus one predeclared operation and, once
  section 5 is settled, one diagnostic code. **Invariants affected:** none
  amended.
- **Canonical representation:** unchanged; no accepted source uses a region or
  `share` today, so nothing becomes invalid.
- **Threat-model impact:** positive on both counts. A mutable region crossing a
  task boundary is the shared-mutable case `docs/44` section 3 requires a
  negative for, and it becomes decidable. Keeping both DMA variants
  non-shareable closes the route by which a device-visible region could have
  reached several tasks through a `Copy` handle.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the eight conformance cases of section 5, checker unit tests per
  row of the table and for `share`, and verifier negatives for `V2021_REGION`.

## Consequences

Region rules become checkable, the shared-mutable negative the threat model
requires becomes expressible in source, and sharing is something a reader can
see rather than something that happens because two tasks named the same handle.

The cost is one narrow syntactic extension — `mut` inside two type arguments —
and a deliberately conservative DMA model that a later typed device contract
will have to widen explicitly.

## Alternatives considered

**Keep the mode in the capability contract and out of the type.** Rejected for
V1: it makes the fact invisible to a single-module check and to the IR type
table, so neither the checker nor the verifier could enforce it without
consulting an external contract the language does not name.

**Two more constructors, `MutRegion<T>` and `MutDmaRegion<T>`.** Rejected: four
names for two concepts, and the relationship between them would be spelled
nowhere.

**Let `Transferable` also mean shareable, so several tasks may hold a region.**
Rejected: it makes a duplication of an affine handle invisible, and the number
of holders would depend on how many tasks named it rather than on an operation
in the source.

**Make `DmaRegion<T>` shareable, since it is immutable.** Rejected: a
`Shared<DmaRegion<T>>` is `Copy`, so shareability is transitively a way across
the task boundary the DMA rule forbids. V1 stays conservative and a typed device
contract widens it later if it can say why that is safe.
