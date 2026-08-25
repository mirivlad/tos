<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0069: The Stage 3 reference process grant

- Status: **Proposed**
- Date: 2026-08-25
- Decision level: 2 — it fixes a property of the ADR-0040 reference platform:
  what backs a process's runtime arena, and how its size is decided. It changes
  no invariant, no ABI operation and no TOS Core semantics
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Evidence: `docs/evidence/STAGE2_ARENA_BOUND.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`

## The gap, stated once

`RuntimeMemoryGrantV1` (ADR-0041) gives a process a base, a length and an
alignment, and ADR-0050 makes the grant per process. Neither says what backs it
or how large it is, and the implementation answered both questions by accident:
it carved a **physically contiguous** run of `min(largest_contiguous,
MAX_GRANT)`.

Both halves of that were wrong in a way that showed.

Carving made each grant take the largest run there was and leave the next one a
smaller largest run — 15707, 8512, 4244, 2708 frames on the reference platform —
so a fourth process could not start while tens of thousands of frames were free.
And `min(largest_contiguous, …)` made the size a function of what was left: a
program that ran as the first process and failed as the fourth would be
reporting a fact about scheduling as though it were a fact about itself.

## Decision

### 1. The grant is virtually contiguous; its backing is ordinary frames

On the ADR-0040 profile a process's runtime arena is one **virtually**
contiguous region at the fixed address `GRANT`, assembled from ordinary frames
and page mappings. Physical contiguity is not required and is not promised.

Nothing accepted asks for it. ADR-0041 §1 promises "one bounded region" with a
base, a length and an alignment — and the base a process is told is `GRANT`, a
virtual address; the process never learns a physical one. ADR-0050 grants per
process from a pool and says nothing about contiguity. And `tos-frames` already
describes this model in its own words: `allocate_frame` "is what an address
space, a page table **or a per-process grant** is built from", while `carve` is
"for the few structures that must be contiguous because nothing maps them yet —
at boot, before paging, that is how the Stage 2 heap grant is made". A mapped
per-process grant is not one of those.

The backing is released the way every other process mapping is released: out of
the page tables it was mapped into, at retirement, cleared on the way
(ADR-0050 §3). The slot keeps a length, not a physical span, because the grant
is not one.

### 2. The size is a fixed property of the profile

The grant size is a constant of the reference profile. It is **not** a function
of free memory, **not** a share divided among running processes, and **not**
adaptive. A process either gets the profile's size or is refused, and the
refusal names which bound it hit.

This is the half that makes a measurement mean anything: an arena bound is a
statement about a program, and it can only be that if every process of the
profile is measured against the same arena.

### 3. `54 MiB` is a candidate, not a ratified size

The implementation carries `RUNTIME_GRANT = 54 MiB` as a **provisional
candidate** pending this ADR's approval. Section 5 states what it does and does
not cover, and section 6 what would have to change to cover more.

### 4. `MAX_GRANT` stays a ceiling

`MAX_GRANT = 96 MiB` remains what its own comment already calls it: a cap, never
a target. Nothing may grant it because it is available; at that size the second
process of the reference profile would not start.

## 5. What the candidate covers, measured

`docs/evidence/STAGE2_ARENA_BOUND.md` measures two different quantities, and
the difference decides which one a grant must cover:

- `resolution_over_summaries` reports **committed** live state — `52.01 MiB`;
- `one_module_at_the_ceiling` and `an_executed_closure` report the **frontier**,
  the arena's high-water mark.

A grant has to cover the frontier: an allocator cannot hand out an address above
the region it was given, whatever the live total is at that instant. The
frontier for **one module at the published 256 KiB source ceiling** is
`50.33 MiB`. The candidate is that, rounded up.

New measurement, through the production `execute_set` on the same instrumented
arena, with the source corpus excluded — in TOS the units are capsule bytes
outside the grant, so the fixture builds them first and reports what the run
needed *above* them:

| Closure of ceiling-sized modules | Arena above the corpus |
|---:|---:|
| 2 | 109.40 MiB |
| 4 | 160.07 MiB |
| 8 | 261.25 MiB |
| 16 | 460.00 MiB |

That is `25.03 MiB` per module above a base of `59.94 MiB`, linear across the
measured range.

**So the candidate covers one ceiling-sized module and no closure of them.** Two
already need twice it. This is stated rather than smoothed over: the size was
not chosen to fit a workload, and the workload does not fit it.

## 6. The promise the implementation currently makes, and cannot keep

docs/44 §2 requires an implementation to publish exact numeric limits and allows
a **lower** cap "if reported in the implementation's declared conformance
profile". The reference implementation declares none: `tos_verifier::limits`
`Limits::default()` is the accepted V1 ceiling, with `modules: 256`, and
`tos_core::MAX_SOURCE_BYTES` is the 256 KiB source unit. The promise is
therefore the ceiling itself.

Extrapolating the measured slope, honouring it needs about **6.3 GiB** of arena
for one process — roughly twenty-five times the whole reference platform. The
gap is not between the grant and the promise; it is between the promise and the
machine.

**This ADR does not choose a smaller cap.** Doing so would be picking a number
to fit a grant, which is the failure mode section 2 exists to prevent, and it is
a decision about the conformance profile rather than about memory. What this ADR
records is that one is needed: either the implementation declares a conformance
profile with a closure cap it can honour on the reference platform, or the
reference platform stops being where the full ceiling is claimed. Both are Level
2 decisions and neither is taken here.

## 7. Four processes, and what binds next

Measured on the reference platform: the pool holds `58 839` frames — about
`229.8 MiB` — after the nucleus takes its own. A process costs `14 356` frames,
`56.08 MiB`: the `54 MiB` grant plus about `2.08 MiB` of stack, report,
arguments, launch record and page tables.

Four of them is `224.3 MiB`, inside the pool with about `5.5 MiB` to spare, and
that is not arithmetic on paper — the ADR-0067 lifecycle gate now reaches
`TOS.RUN.PROCESS_REFUSED reason=no-slot uncollected=3`, which is only reachable
when the fourth slot is occupied rather than unaffordable.

**Memory is what binds next, through the process table.** At a grant much above
`55.3 MiB` the fourth process stops fitting, and the refusal changes from
`reason=no-slot` to `reason=no-grant`. So the two declared numbers —
`MAX_PROCESSES = 4` and the grant size — are one decision with two names, and
raising either lowers the other.

## What this ADR does not decide

It does not set the conformance profile's closure cap (§6), does not change
`MAX_PROCESSES`, does not change `MIN_GRANT` or `MAX_GRANT`, and does not
introduce any adaptive sizing. A policy of the form "give a process what is
left" is refused by §2 and is not an alternative this ADR keeps open.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended.
- **Canonical representation:** unchanged.
- **Trusted-base impact:** the nucleus maps the grant frame by frame with the
  path it already uses for stack, report and argument regions, and releases it
  through the same page-table walk. One allocator path disappears from the
  per-process case rather than one being added.
- **Source-to-runtime impact:** unchanged. A process is told the same base,
  length and alignment it was told before.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** none claimed. This fixes a platform property that
  Stage 3 evidence is taken under.
- **Threat-model impact:** neutral to positive. Frames released from a grant go
  back through the release path and are cleared there (ADR-0050 §3), where a
  carve was returned as one span; and a fixed size removes a channel by which
  one process's memory appetite changed what a later process was given.
- **Performance contract:** mapping a grant frame by frame costs one page-table
  walk per frame at creation instead of one range mapping. `process_create` is
  not on a measured path and carries no latency budget; if it ever acquires one,
  this is the cost to measure rather than assume.
- **Compatibility profile:** ADR-0040's machine profile is unchanged and gains
  the grant size as a stated property.
- **Dependencies, licence, patents:** none.
- **Tests:** the lifecycle gate's slot-exhaustion phase is the four-process
  evidence; the arena-bound harness carries the ceiling measurement of §5; and
  every process of a boot reclaiming the same frame count is what shows the
  grant no longer depends on creation order.

## Alternatives considered

**Keep the carve.** Rejected by measurement: it is what made the fourth slot
unusable, and it bought a contiguity nothing asked for.

**Size the grant from what is left.** Rejected in §2: it makes a program's
success depend on how many processes preceded it.

**Raise the grant until the declared ceiling fits.** Impossible on this
platform by a factor of twenty-five (§6), and it would take the process table
down to one.

**Lower the declared closure cap here.** Refused as out of scope in §6: choosing
a cap so that it fits a grant is choosing a conformance profile by its memory
bill.
