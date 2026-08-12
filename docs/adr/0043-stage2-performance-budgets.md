<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0043: The Stage 2 quantitative performance budgets

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-12
- Decision level: 2 — the numbers in docs/35 that a Stage gate is measured against
- Project Architect approval: *(none — this ADR is not accepted)*

## Context

Stage 2 now has a complete paired measurement taken by the normative procedure:
3 warmups, 21 samples, median/p95/p99, one commit, one set of fixtures emitted
by the harness that measures them natively, both halves
(`docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md`).

| metric | native p95 | reference p95 | budget | verdict |
|---|---|---|---|---|
| frontend, 256 KiB module | 160 893 us | 1 490 798 us | 500 000 us | **FAIL** (2.98x over) |
| engine, 1e6 operations | 333 743 us | 5 541 378 us | ratio ≤ 10x | **FAIL** (16.6x) |
| quota rejection | 66 668 us | 763 305 us | ≤ 2x accepted | **PASS** (0.512) |

**The implementation defects are gone, and they were found rather than argued
around.** Two were fixed:

- the heap searched every block on every allocation, making the frontend
  superlinear in its input (`docs/evidence/STAGE2_ALLOCATOR_SEARCH.md`);
- the lowerer rendered `format!("{:?}")` to intern every type, which cost 7%
  natively and over 600x on the reference platform
  (`docs/evidence/STAGE2_FREESTANDING_PRIMITIVES.md`).

One hypothesis — that the freestanding memory primitives were byte-at-a-time —
was **tested against the real binary and refuted**. They are `rep movsq`,
`rep stosq` and a 16-byte `memcmp`. No substrate was written for a defect that
did not exist.

The evidence that nothing else is hiding is that the platform factor is now
uniform across two very different workloads: 9.3x for the copy- and
allocation-heavy frontend, 16.6x for the arithmetic-heavy engine. Before the
fixes those differed by three orders of magnitude.

### The two failures are not the same kind of failure

**The engine budget is a ratio of one implementation to itself on two
platforms.** Optimising the engine moves numerator and denominator together, so
the ratio barely moves. Stated that way, "within 10x" is a claim about how much
slower TCG is than the host — not a claim about the engine — and the measurement
says TCG costs 16.6x for this workload. No amount of engine work reaches 10x;
only a different platform or a different number does.

**The frontend budget is absolute**, so implementation work can reach it. 500 ms
on the reference platform needs the native frontend at roughly 54 ms against
today's 161 ms — about 3x. No defect explaining that gap has been identified,
and no claim is made here that none exists.

## The precedent this follows

Stage 1's initial 250 ms threshold was empirically falsified rather than
defended: the number was written before anything ran, the measurement
disagreed, and the threshold moved because the evidence said so. The order
matters and has been followed — measure, find the defects, fix them, and only
then ask whether the number itself was research optimism.

## Options

1. **Do not revise anything yet.** Prove or refute the `memcpy` hypothesis,
   supply optimised memory primitives for the freestanding target if it holds,
   re-measure, and bring this ADR back with numbers from an implementation whose
   known defects are gone.
2. **Revise the engine ratio** from 10x to a number the reference platform can
   hold, on the evidence that TCG interpretation of a bounded reference engine
   costs what it costs.
3. **Revise the frontend budget** from 500 ms p95 to a measured figure.
4. **Change the reference platform** — KVM instead of TCG, or a larger machine —
   so the budgets stand. Rejected as a suggestion: ADR-0040 chose TCG so the
   platform is reproducible on any host, and choosing a faster platform after
   seeing the number is what ADR-0040 exists to prevent.

## Recommendation

**For the engine ratio: option 2, revise the number.** The argument is
structural rather than a plea. A budget expressed as `reference / native` of the
same binary measures the platform and cannot be met by improving the thing being
measured. ADR-0040 chose TCG precisely so the platform is reproducible on any
host, and that choice has a cost which is now measured: 16.6x on this workload.
A budget of 20x would be met with margin and would still fail a genuine
regression; a budget of 10x is unreachable by construction on this platform.

**For the frontend budget: option 1, revise nothing yet.** 500 ms is absolute
and implementation work can move it. The gap is about 3x in native terms, which
is large but not the kind of gap that says a target is impossible, and no
profiling has yet been done on the frontend's *remaining* cost — only on the two
defects that dominated it. Asking for this number to move before that work is
exactly what this ADR refuses for the engine's sake.

**For quota rejection: nothing.** It passes on the reference platform at 0.512
against a budget of 2.000.

Whichever way each is settled, they should be settled **separately**. They fail
for different reasons, by amounts that differ by an order of magnitude, and
moving them together would hide that.

## Consequences

The Stage 2 performance gate stays **FAIL** with retained evidence. Stage 2 is
not a candidate for closure while it does. That is the intended consequence: a
gate that moves to meet the implementation stops being a gate.

## Alternatives considered

**Report the frontend budget as met on a smaller module.** Rejected outright.
The budget is written against the published source-unit ceiling, and measuring
something else and reporting it as the same thing is the failure mode every
evidence rule in this project exists to prevent.

**Compare the reference half against a "typical TCG factor" rather than a
measured native half.** Rejected: that is what the previous record did, and it
produced a number that could not be checked. The pair is now measured on one
fixture at one commit, which is why the 16.8x figure can be argued with.
