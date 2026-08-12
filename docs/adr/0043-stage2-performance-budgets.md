<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0043: The Stage 2 quantitative performance budgets

- Status: **Accepted** (Project Architect-approved)
- Date: 2026-08-12
- Decision level: 2 — the numbers in docs/35 that a Stage gate is measured against
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-12, accepting the
  engine part with an amended threshold (22x rather than the 25x recommended
  here) and a correction to what a ratio gate is for. The frontend and
  quota-rejection budgets are **unchanged** by this decision.

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
the ratio barely moves. That was an argument when this ADR was first written; it
is now a measurement (`docs/evidence/STAGE2_PERFORMANCE_DECOMPOSITION.md`):

```text
                 native p95     reference p95     ratio
engine, before     333 743 us     5 541 378 us     16.6x
engine, after      215 529 us     3 628 441 us     16.8x
```

A general optimisation worth **1.6x** — removing a per-instruction clone from
the interpreter's hottest loop — left the ratio where it was, at about 16.7x.

That is strong evidence that the ratio is dominated by the cost of the ADR-0040
TCG platform rather than by this engine, and the decomposition that was missing
has now been taken (`docs/evidence/STAGE2_ENGINE_DECOMPOSITION.md`):

| component | ratio |
|---|---|
| dispatch + branch + fuel (the bare loop) | 16.9x |
| integer arithmetic | 17.9x |
| comparison | 15.4x |
| conditional branch | 16.2x |
| local load/store | 15.1x |
| call / return / frame | 15.5x |
| aggregate construction | 23.3x |

Every semantic component the million-operation benchmark actually performs lies
between **15.1x and 17.9x**. The aggregate 16.8x is the weighted average of that
band, not an average hiding an outlier — which is precisely what the
decomposition existed to rule out.

The single outlier, aggregate construction at 23.3x, is the only component that
allocates: the gap is the bounded heap against the host allocator, and the
benchmark contains no aggregate construction at all.

Two independent experiments therefore agree. A 1.6x general speedup did not move
the ratio, and no component of the workload deviates from the band. Within this
architecture the ratio is a platform property.

**The frontend budget is absolute**, so implementation work can reach it, and
work since has moved it. Three further general inefficiencies were found and
fixed — the engine's per-instruction clone, a whole-source NFC normalization
that ASCII makes unnecessary (`read` 23.3 ms → 0.4 ms), and eagerly formatted
verifier finding locations. The frontend now stands at 124 ms native and
1.28 s reference against a 500 ms budget: **2.56x over**, down from 2.98x.

The stages that remain are `check` (~33 ms), `lower` (~28 ms) and `verify`
(~55 ms). `verify` has now been profiled one level down, and **38.8 ms of its
54.7 ms is the module digest** — a third of the entire frontend. That work is
not optional: docs/43 section 5 binds the receipt to the module's complete
digest, and a verifier that skipped it would be issuing a receipt for something
it had not identified.

Its cost is dominated by the *size* of the canonical stream being hashed, and
that in turn by the encoding: every count and every length is written as a
16-byte big-endian `u128`, whatever its magnitude. A module at the ceiling has
tens of thousands of counts. A variable-length encoding would cut the stream
substantially — but the stream **is** the module identity, so changing it
changes every module digest and every receipt and cache key derived from one.
That is an `tos-ir/v1` contract question and is not taken unilaterally; it is
recorded here as the largest identified frontend cost with a known shape.

One optimisation was tried and **rejected by measurement**: hashing the stream
incrementally instead of buffering it, to avoid a multi-megabyte allocation. It
was slower — a compression function called with small fragments pays its
per-call cost repeatedly, and a fixed intermediate window only added a copy. The
buffered form is kept because it measured faster, and the attempt is recorded so
it is not repeated.

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

**For the engine ratio (as recommended before the decision): option 2, revise
the number — the evidence is now in.**
Both experiments the question needed have been run. A 1.6x general optimisation
left the ratio unchanged, and the decomposition shows every component of the
workload inside a 15.1–17.9x band with no path deviating. A budget expressed as
`reference / native` of one implementation measures the platform, and this
platform costs about 16x for the operations this benchmark performs.

**The accepted threshold is 22x.** The components of the standard workload span
15.1x to 17.9x and the full workload sits at 16.8x, so 22x is about +30% on the
measured figure — enough headroom for ordinary variation between hosts, and
consistent with the blocking-regression policy of docs/35.

The 23.3x of aggregate construction is deliberately **not** used to set this
number. The standard million-operation benchmark performs no aggregate
construction, and sizing a budget from a component the workload does not contain
would be sizing it from something it never measures.

**What a ratio gate is for, corrected.** An earlier draft of this ADR argued
that 25x "would still fail immediately on a regression of the kind already found
in this project — the per-instruction clone was worth 1.6x". The experiment in
`docs/evidence/STAGE2_ENGINE_DECOMPOSITION.md` shows the opposite: removing that
clone moved native and reference together and left the ratio at 16.8x. A
platform-neutral regression is invisible to a ratio by construction.

So a ratio gate detects a **disproportionate reference-platform regression** —
something that costs the guest much more than the host, as the whole-arena
allocator search and the per-intern debug string both did. Ordinary regressions
are caught by retained benchmark history against the docs/35 regression policy,
which is a different instrument for a different failure. Claiming one does the
other's job is what this correction removes.

**For the frontend budget: option 1, revise nothing yet.** 500 ms is absolute
and implementation work has already moved it twice. The remaining gap is 2.56x,
and the three stages that hold the remaining cost have not been profiled
internally. Asking for this number to move before that work is exactly what this
ADR refuses to do for the engine's sake, and it would be no more honest here.

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
