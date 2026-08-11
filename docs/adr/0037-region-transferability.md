<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0037: TOS Core V1 region and DMA-region transferability

- Status: **Proposed** — needs Project Architect approval to become Accepted
- Date: 2026-08-11
- Decision level: 2 — fixes the `Transferable`, shareable and mutable facts of
  two accepted V1 type constructors
- Project Architect approval: *(pending)*

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
Region<mut T>      a mutably granted region: readable and writable, not shareable,
                   not Transferable
DmaRegion<T>       as Region<T>, and additionally never Transferable
DmaRegion<mut T>   as Region<mut T>
```

`mut` inside the type argument is the only place V1 admits it in a type, and it
is admitted for exactly these two constructors. It is not a general mutability
qualifier and introduces no `mut T` elsewhere.

### 2. The three facts

| Type | `Copy` | shareable | mutable | `Transferable` |
|---|---|---|---|---|
| `Region<T>` | no | yes | no | yes |
| `Region<mut T>` | no | no | yes | no |
| `DmaRegion<T>` | no | yes | no | no |
| `DmaRegion<mut T>` | no | no | yes | no |

A DMA region is never `Transferable` in V1 regardless of mode: it names device-
visible memory, and moving that across a task boundary is a decision the device
and driver model has to make, not the language.

### 3. Diagnostics

No new code. Capturing a non-`Transferable` region into a task is
`E1304_INVALID_TASK_CAPTURE` with `reason=mutable region` or `reason=DMA
region`; into a closure it is `E1305_INVALID_CLOSURE_CAPTURE` with the same
reasons. Writing through a `Region<T>` is `E1201_ASSIGN_TO_IMMUTABLE`.

`V2021_REGION` gains these as verifier rules, so the IR carries the mode in its
type table and the verifier rechecks it rather than trusting the frontend.

### 4. Conformance evidence

At least: a positive sharing a `Region<T>` between two tasks; a negative
capturing a `Region<mut T>` into a task; a negative capturing a `DmaRegion<T>`
into a task; a negative writing through a `Region<T>`; and a positive writing
through a `Region<mut T>`.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended.
- **Canonical representation:** unchanged; no accepted source uses a region
  today, so nothing becomes invalid.
- **Threat-model impact:** positive: a mutable region crossing a task boundary
  is the shared-mutable case `docs/44` section 3 requires a negative for, and it
  becomes decidable.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the five conformance cases, checker unit tests per row of the
  table, and verifier negatives for `V2021_REGION`.

## Consequences

Region rules become checkable, and the shared-mutable negative the threat model
requires becomes expressible in source. The cost is one narrow syntactic
extension, `mut` inside two type arguments.

## Alternatives considered

**Keep the mode in the capability contract and out of the type.** Rejected for
V1: it makes the fact invisible to a single-module check and to the IR type
table, so neither the checker nor the verifier could enforce it without
consulting an external contract the language does not name.

**Two more constructors, `MutRegion<T>` and `MutDmaRegion<T>`.** Rejected: four
names for two concepts, and the relationship between them would be spelled
nowhere.
