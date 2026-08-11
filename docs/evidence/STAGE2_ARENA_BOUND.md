<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 implementation-arena bound

Evidence level: **P1** (locally measured, docs/35).
Producer: `source/tests/arena-bound` — the whole production path run with
`tos_runtime::BoundedHeap` installed as the global allocator.
Reproduce: `cargo run --release -p tos-arena-bound` (add `--full` for the
256-module sweep).

ADR-0041 accepts two disciplines for allocation failure. The one this
implementation relies on is "a proved upper memory bound and an arena at least
that large", so the bound has to be measured. Every figure below comes from
running source reader, parser, checker, module resolution, lowerer, independent
verifier and bounded engine *through* the heap being measured — not from
modelling it.

## What is measured, and why the metric is what it is

`peak_extent` is the highest address the arena was ever carried to. A sum of
requested payloads is not a bound: every block carries tags, a request is
rounded up to the grain, a remainder too small to be its own block stays with
the allocation, and a hole below the highest live block is arena the run still
needed. `peak_extent` includes all of it, and never falls when memory is freed —
a bound must err upward.

`committed` is the live figure in whole blocks, and `block_census` is the layout.
Both are needed: **equal live bytes do not prove an arena is in the same state.**
The same total can sit in twice as many pieces, and a reference runtime that
fragments a little further on every repetition is not a recovery oracle.

## Results

| workload | arena needed |
|---|---|
| one module at the published 256 KiB source ceiling | **52 808 656 B — 50.36 MiB** |
| a source set of 256 modules processed one at a time | **52 808 656 B — 50.36 MiB** |
| 64 repeated whole-pipeline executions | frontier unchanged after the first |
| set-wide resolution over parse trees, 256 modules of 8 KiB | 109 953 616 B — 104.86 MiB *(measured)* |
| set-wide resolution over parse trees, 256 ceiling-sized modules | 3 594 357 104 B — 3.35 GiB *(fitted from a measured slope)* |
| **set-wide resolution over derived summaries, per ceiling-sized module** | **221 488 B — 0.21 MiB** |

Each run produced the right answer. A measurement of a pipeline that did not
compute anything would measure nothing.

### The executable path does not accumulate across modules

Processing **256** modules one at a time, releasing each module's state before
the next begins, moved the frontier by **0 bytes**. Not "a little": none.

The mechanism is not luck. `peak_extent` is a high-water mark, and the heap is
first-fit with immediate coalescing of both neighbours — so the memory the
previous module returned is the memory the next module is handed, and only a
*deeper* run can move the frontier. The bound for a source set processed this
way is therefore the bound for its **largest single module**, not the sum of its
modules, and multiplying the single-module figure by a module count would
overstate it by two orders of magnitude.

### Repeated execution returns the arena to the same *layout*

Sixty-four consecutive runs of the whole pipeline over the same module. From the
second round on, every observable is identical to the first: 19 696 bytes
committed, 6 blocks, 2 of them free, and a frontier that never moves again.

The layout is asserted, not only the total. Accumulating fragmentation breaks the
block census long before it breaks the committed figure, so a test that compared
totals alone would pass while the arena slowly shattered.

### Resolution over summaries is the architecture; the slope says so

docs/42 module resolution is the part of the path that cannot be phased away
module by module: it compares every module's declared name, imports and type
surface against every other's. What *can* change is what it reads them from.

Reading parse trees costs **14 040 456 bytes per ceiling-sized module**, measured
at 1, 2 and 4 modules — about 54x the module's own source, which is what a parse
tree costs. Reading derived summaries costs **221 488 bytes per ceiling-sized
module**, measured at 1 and 8. That is a **63x reduction**, and it puts the
largest closure the two published ceilings admit together — 256 modules of
256 KiB — at roughly **57 MB** of live resolution state instead of 3.35 GiB.

The reason the reduction is that large is structural rather than incidental: a
summary holds a module's *interface*, and an interface does not grow with a
body. There is a test asserting that a module with a two-hundred-function body
summarizes to exactly the same size as one with a single function.

## The constraint this removes, and the one that remains

Before the summary architecture, the two published ceilings of docs/44 section 2
multiplied to a closure whose resolution needed ~3.35 GiB against the ADR-0040
platform's 256 MiB — so this implementation resolved roughly 19 ceiling-sized
modules rather than the 256 the ceilings jointly admit. docs/44 permits a
declared lower implementation cap, so that was never a contradiction in the
language; it was an architecture nobody should be asked to keep.

At 221 488 bytes per module, a full 256-module closure of ceiling-sized modules
needs about 57 MB of resolution state. That fits the reference platform with
room for the arena the executable path needs, and it is bounded by interfaces
rather than by bodies — so it stays fitting as modules grow.

What remains is **time, not memory**. A ceiling-sized module does not finish the
frontend within 900 seconds on the reference platform, because
`BoundedHeap::try_allocate` searches first-fit by walking every block from the
base of the arena and the frontend allocates constantly. Every figure above is
correct; several of them took minutes to obtain that should have taken seconds.
`docs/evidence/STAGE2_REFERENCE_PLATFORM_P1.md` records that finding and why the
fix is not attempted in the change that found it.

## What is not claimed

- Not P2 or P3: one machine, one build, no CI reproduction and no independent
  reproduction (docs/35).
- The tree-based ceiling-closure figure is **fitted from a measured slope**, and
  is labelled so wherever it appears. Measuring it outright needed more memory
  than the machine has — which is itself the finding, and the reason the
  architecture changed.
- The summary figure at the 256-module ceiling is likewise **extrapolated from
  a measured per-module cost** (1 and 8 ceiling-sized modules). Its linearity is
  structural — a summary holds one module's interface and nothing shared — but
  the 256-module point has not been measured directly, and this record does not
  say it has.
- The measured figures cover the reference implementation. Another conforming
  implementation's arena is its own to measure.
- The grant the nucleus makes is a separate, declared decision
  (`crates/tos-runtime/src/region.rs`), deliberately not derived from this
  number automatically: a bound that silently became a configuration would stop
  being a bound anyone had to look at.
