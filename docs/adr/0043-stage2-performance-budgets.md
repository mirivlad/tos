<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0043: The Stage 2 quantitative performance budgets

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-12
- Decision level: 2 — the numbers in docs/35 that a Stage gate is measured against
- Project Architect approval: *(none — this ADR is not accepted)*

## Context

Stage 2 now has one paired measurement: the same fixtures, at one commit,
measured natively and on the ADR-0040 reference platform through the real
freestanding path (`docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md`). Two of the
three docs/35 bootstrap-profile budgets are not met.

```text
engine    reference p95 5 473 077 us / native p95 325 350 us = 16.8x   budget 10x
frontend  256 KiB fixture does not complete on the platform            budget 500 ms p95
reject    0.397 natively                                               budget 2.000  (PASS)
```

**This ADR does not ask for a revision yet.** It exists because the question is
now live and because the evidence that would justify one is a specific thing
that has not been gathered.

One implementation defect has already been found and fixed rather than argued
around: the heap searched first-fit by walking every block, so the frontend was
superlinear in its input. Fixing it took the arena-bound sweep from hours to
16.5 s (`docs/evidence/STAGE2_ALLOCATOR_SEARCH.md`) and did **not** bring the
frontend inside budget. That is the pattern this ADR wants to protect: a budget
is revised after the implementation defects are gone, not instead of finding
them.

## The precedent this follows

Stage 1's initial 250 ms threshold was empirically falsified rather than
defended: the number was written before anything ran, the measurement disagreed,
and the threshold moved because the evidence said so. The same discipline
applies here, in the same order — measure, find the defects, fix them, and only
then ask whether the number itself was research optimism.

## What is not yet known

The engine ratio of 16.8x is measured on work that is arithmetic- and
control-flow-heavy and barely copies. The frontend is copy-heavy and is at least
three orders of magnitude slower than that factor predicts. The leading
hypothesis is that `x86_64-unknown-none` takes `memcpy`, `memset`, `memmove` and
`memcmp` from `compiler_builtins`, whose portable implementations move a byte at
a time, while the host build gets vectorised ones.

That hypothesis is testable and untested. Until it is tested, nobody knows
whether the frontend budget is unreachable or merely unreached.

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

**Option 1.** No budget should move while a known-untested implementation
hypothesis could account for the whole gap. Two of the three budgets are missed
by amounts that differ by three orders of magnitude, which is itself evidence
that they are missed for different reasons — and a single defect explaining the
larger one would change what the smaller one should be compared against.

If the hypothesis is refuted and the frontend is still out of budget with no
identified defect left, options 2 and 3 become the honest ones, and this ADR
should return carrying the measurements that justify each specific number rather
than a request to relax them together.

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
