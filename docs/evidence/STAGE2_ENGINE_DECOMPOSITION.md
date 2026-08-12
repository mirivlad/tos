<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# The engine's 16.8x, decomposed

Evidence level: **P1**, and **diagnostic** rather than a gate result: the
components below are 3 reference boots and 21 native samples each, not the
normative 3+21 pair. The normative record stays
`docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md`.
Producer: `tos-core-performance --decompose` (native) and the same fixtures
emitted with `--emit-fixture` and booted on the ADR-0040 platform.

## Why this experiment

A single aggregate ratio cannot distinguish two very different situations: a
platform that is uniformly ~16x slower, and an implementation with one
pathological path that drags an otherwise healthy average. The first would make
the 10x budget a research assumption incompatible with the chosen platform; the
second would make it an implementation bug not yet found.

So the million-operation benchmark is decomposed into the semantic components it
is actually made of. Each fixture runs the **same** loop the same number of
times and differs only in four extra operations per iteration, so a component's
cost is the difference from `empty`. All of them run on the production
`tos-engine` — there is no second interpreter written to measure the first.

## Measured

200 000 iterations; `empty` is the loop itself: dispatch, the loop compare, the
conditional branch, the back edge, and the fuel charged for each.

| component | native us | reference us | delta native | delta reference | **ratio** |
|---|---|---|---|---|---|
| empty (dispatch + branch + fuel) | 28 259 | 476 500 | — | — | **16.9x** |
| integer arithmetic | 80 917 | 1 420 145 | 52 658 | 943 645 | **17.9x** |
| comparison | 83 316 | 1 324 142 | 55 057 | 847 642 | **15.4x** |
| conditional branch | 145 593 | 2 382 770 | 117 334 | 1 906 270 | **16.2x** |
| local load/store | 43 907 | 713 267 | 15 648 | 236 767 | **15.1x** |
| call / return / frame | 143 921 | 2 268 828 | 115 662 | 1 792 328 | **15.5x** |
| aggregate construction | 120 431 | 2 625 529 | 92 172 | 2 149 029 | **23.3x** |

## What it says

**Six of the seven components lie between 15.1x and 17.9x.** Dispatch,
arithmetic, comparison, branching, locals and calls — every semantic component
the million-operation benchmark actually performs — sit in one narrow band, and
the benchmark's aggregate 16.8x is exactly what a weighted average of that band
should be.

There is no pathological path hiding inside the aggregate. That was the
alternative this experiment existed to rule out, and it is ruled out for the
operations the benchmark performs.

### The one outlier, and why it is not a counterexample

**Aggregate construction is 23.3x** — about 40% worse than the band. It is also
the only component that allocates: building a `Value::Aggregate` takes memory,
which natively comes from the host allocator and in the guest from the bounded
heap under TCG. The gap is the allocator difference, not a dispatch or
evaluation pathology.

It matters that the million-operation benchmark contains **no** aggregate
construction. The outlier is real, worth knowing, and outside the workload the
budget is written against.

## What this does and does not establish

It establishes that the engine's ratio is uniform across the semantic components
the benchmark exercises, so improving any one of them cannot materially move the
aggregate — which is what the earlier experiment showed from the other
direction, when a 1.6x general speedup left the ratio at 16.8x.

It does not establish that *no* architecture could do better. A fundamentally
different execution strategy — one that changes what the guest CPU is asked to
do rather than how efficiently it does it — is outside what was measured. What
can be said is that within the current architecture, the ratio is a property of
the ADR-0040 platform rather than of any one path in this implementation.

That is the evidence ADR-0043 was missing for its engine recommendation.
