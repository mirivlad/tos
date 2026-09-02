<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0069: The Stage 3 reference process grant

- Status: **Accepted**
- Date: 2026-08-25
- Decision level: 2 — it fixes a property of the ADR-0040 reference platform:
  what backs a process's runtime arena, and how its size is decided. It changes
  no invariant, no ABI operation and no TOS Core semantics
- Project Architect approval: **given, 2026-08-27**
- Evidence: `docs/evidence/STAGE2_ARENA_BOUND.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`
- Note: §6 was rewritten on 2026-08-25 after the Project Architect identified
  the first reading as evidence of implementation retention rather than of a
  contract requirement

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

### 2a. Reconciliation with ADR-0076 (added 2026-09-01, not a rewrite)

ADR-0076 is a later Accepted decision and narrows what §2 and §3 say, without
retracting either. What holds after it:

- **`RUNTIME_GRANT = 54 MiB` is the grant of an ordinary Stage 3 runtime
  process**, which is what it was measured for and what the four-process budget
  was computed from;
- a **funded, special-purpose process may receive a different fixed policy
  grant** — a build worker's, for one, which the workspace measurements put far
  above `54 MiB`;
- that does not weaken §2's rule. A role's grant is still a **fixed policy
  value**: not a share of what remains, not `min(available, …)`, not adaptive,
  and not derived from how much memory another allocation happened to leave;
- **each role's grant carries its own evidence.** `54 MiB` has
  `STAGE3_PROCESS_GRANT.md`; any other role's number needs a measurement of that
  role before it is a number rather than a guess.

The text below is the original decision and is unchanged.

### 2. The size is a fixed property of the profile

The grant size is a constant of the reference profile. It is **not** a function
of free memory, **not** a share divided among running processes, and **not**
adaptive. A process either gets the profile's size or is refused, and the
refusal names which bound it hit.

This is the half that makes a measurement mean anything: an arena bound is a
statement about a program, and it can only be that if every process of the
profile is measured against the same arena.

### 3. `RUNTIME_GRANT` is `54 MiB`

`RUNTIME_GRANT = 54 MiB` is the Stage 3 reference profile's grant. **Ratified,
not provisional**: it is enforced rather than estimated — a bounded allocator
whose whole arena is exactly this size runs a launch of the exact resolved
closure at every size up to the published 256-module ceiling, and at the worst
declared resolution the V1 ceilings admit.

| Under a hard `54 MiB` arena | Grant frontier |
|---|---:|
| closure of 2 ceiling-sized modules | 19.68 MiB |
| closure of 16 | 20.10 MiB |
| closure of 64 | 21.57 MiB |
| **closure of 256 — the published ceiling** | **27.60 MiB** |
| **256, worst admissible declared resolution** | **42.42 MiB** |
| steady-state residency at a bound of two modules | 32.03 MiB |
| permanent closure state at 256 modules | 182 KiB |

No lower conformance profile was introduced to reach this, no ceiling was
changed, and nothing in the path consults free memory. The evidence is
`docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`.

Sections 5 and 6 are the history of how the number was arrived at, and are
**superseded as residency evidence** by the table above: they measured a path
that held every lowered module of a set alive at once, which ADR-0071 replaced.
They are kept because §6 is where the defect was found, and a correction is
worth more than the number it corrected.

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

**Superseded.** That was the retaining path, before ADR-0071: it held every
lowered module of a set alive at once, so a closure cost the sum of its modules.
Under sequential launch a closure of 256 ceiling-sized modules is verified inside
`27.60 MiB`, because one module is materialized at a time and nothing but a
fixed-size record survives it (§3).

## 6. What the closure measurement actually found

The first version of this section read the closure slope as the cost of TOS
Core V1's declared ceiling. That reading was wrong, and the correction matters
more than the number: **the measurement had found `execute_set` departing from
an accepted memory architecture, not a fundamental requirement.**

`docs/evidence/STAGE2_ARENA_BOUND.md` already fixes that architecture. Set-wide
resolution over 256 ceiling-sized module **summaries** was measured at
`52.01 MiB`; the executable path is to be phased module by module;
`ModuleEntry::summarize()` returns an *owned* summary precisely so the parse
tree can be dropped at once; and `check_module_summaries()` exists so that
resolution never needs a tree at all.

`execute_set` did none of that. It parsed every module and held every tree for
the whole run, then accumulated every lowered module beside them. The phase
breakdown, on ceiling-sized modules, attributes the slope:

| Retained object | Per ceiling-sized module |
|---|---:|
| normalized `SourceUnit` | 0.22 MiB |
| **parse tree (`Schema`)** | **13.99 MiB** |
| owned summary | 0.19 MiB |
| **lowered IR (`Module`)** | **15.13 MiB** |

Restoring the discipline — parse, check and summarize one module at a time,
drop each tree at the end of its turn, resolve over summaries, and re-parse a
module only to lower it — moved the measured production `execute_set`:

| Ceiling-sized modules | Retaining path | Phased path |
|---:|---:|---:|
| 2 | 109.40 MiB | **92.83 MiB** |
| 4 | 160.07 MiB | **118.16 MiB** |
| 8 | 261.25 MiB | **168.76 MiB** |
| 16 | 460.00 MiB | **268.15 MiB** |

`25.03 MiB` per module became `12.52 MiB`, which is the lowered IR and no
longer the trees.

**So `~6.3 GiB` is retained here only as an extrapolation of the retaining
implementation path, and explicitly not as the necessary cost of TOS Core V1.**
The phased path extrapolates to about `3.2 GiB` at 256 modules — still far past
this platform, and still an extrapolation of an implementation that keeps every
lowered module alive because `run_set` is handed the whole set at once.

Whether that last component was reducible was the open question when this was
written. **It was**, and ADR-0071 answered it: a closure is verified one module
at a time and nothing but a fixed-size record survives each one, so the question
of what "every lowered module at once" costs no longer arises. **No lower
conformance cap was introduced.**

## 7. How many processes, and what binds next

Measured on the reference platform at the build this ADR was written against:
the pool holds `58 839` frames — about `229.8 MiB` — after the nucleus takes its
own. A process cost `14 356` frames, `56.08 MiB`: the `54 MiB` grant plus about
`2.08 MiB` of stack, report, arguments, launch record and page tables.

Four of them was `224.3 MiB`, inside the pool with about `5.5 MiB` to spare, and
that was not arithmetic on paper — the ADR-0067 lifecycle gate reached
`TOS.RUN.PROCESS_REFUSED reason=no-slot uncollected=3`, which is only reachable
when the fourth slot is occupied rather than unaffordable.

**That measurement is evidence about one build, and it is not an invariant.**
The Project Architect fixed this on 2026-09-03, and the wording here is amended
to carry the decision:

> `MAX_PROCESSES = 4` is the bounded number of process **slots**, not a
> reservation guaranteeing that four simultaneous processes each with the
> ordinary `54 MiB` arena can always be funded. Process slots and memory
> authority are independent finite resources. A creation may therefore see a
> free slot **and** an authority that cannot pay, and `E_LIMIT` is the correct
> answer — ordinary resource behaviour, not a failure of this ADR.

The reason the distinction had to be drawn is concrete: with the four-process
sum asserted as an invariant, **every page of code growth in the runtime image
was an architecture STOP**, because the image is charged to the same pool the
processes are funded from. The per-process charge has since moved from `14 356`
frames to `14 357`, and the root from `57 424` to `57 415`; four ordinary
processes no longer fit, and nothing is wrong.

`RUNTIME_GRANT` stays at `54 MiB`, `MAX_PROCESSES` stays at 4, and the reference
machine is not enlarged. What changed is what the gate asserts. The unified
memory account gate now **reports** how many ordinary processes one root can
fund, and asserts the topologies the system is actually built to run:

- a supervisor and one target — the floor, below which no topology is left;
- a resident supervisor and a transient build worker, with the remainder
  reported as the headroom a bundle may occupy.

Both hold at the current build, with `112.11 MiB` left for bundle backing after
two ordinary processes.

**Memory is what binds next, through the process table.** At a grant of roughly
`55 MiB` or more the fourth process stops fitting, and the refusal changes from
`reason=no-slot` to `reason=no-grant`. The figure is **approximate on purpose**:
a larger grant is also more page tables, so the per-process overhead above the
grant is itself a function of the grant, and the crossing point moves with it.

`MAX_PROCESSES` and the grant size are therefore **jointly constrained by the
ADR-0040 memory budget** — neither can be raised without lowering the other on
this platform, and neither number means anything without the other beside it.
That constraint is on what can be *simultaneously funded*, which §7's amendment
above separates from what can be simultaneously *slotted*.

**A note the residency decision will need.** Process-grant memory and the
physical residency of images or caches are **counted separately** — they are
different regions with different owners and different lifetimes — but both are
spent from the same ADR-0040 whole-machine budget. Moving IR out of a process's
arena is therefore not, by itself, a saving of physical memory: it moves the
cost to another line of the same account. A residency decision that reported
only the arena would be reporting half a ledger.

## What this ADR does not decide

It does not set the conformance profile's closure cap — none is declared, and
the published 256-module ceiling stands — does not change `MAX_PROCESSES`, does
not change `MIN_GRANT` or `MAX_GRANT`, and does not introduce any adaptive
sizing. A policy of the form "give a process what is
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

**Raise the grant until the declared ceiling fits.** Rejected, and in the end
unnecessary: the ceiling fits at `54 MiB`. Not on the ground the first draft
gave, either. "A factor of twenty-five" was an extrapolation
of the retaining implementation path, and §6 withdrew it as a statement about
what TOS Core V1 costs. What remains true is narrower and enough: the grant is
bounded by the whole-machine budget shared with three other processes (§7), and
a closure large enough to matter is not made to fit by enlarging one arena. What
the closure actually requires is open until ADR-0070's compact image and the
bounded-residency decision it requires have been measured.

**Lower the declared closure cap here.** Refused as out of scope in §6: choosing
a cap so that it fits a grant is choosing a conformance profile by its memory
bill.
