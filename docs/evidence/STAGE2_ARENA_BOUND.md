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
| one module at the published 256 KiB source ceiling | **52 268 096 B — 49.85 MiB** |
| a source set processed module by module (16 × 32 KiB) | **52 268 096 B — 49.85 MiB** |
| 64 repeated whole-pipeline executions | frontier unchanged after the first |
| set-wide resolution, 256 modules of 8 KiB | 107 579 136 B — 102.60 MiB *(fitted)* |
| set-wide resolution, 256 modules at the 256 KiB ceiling | 3 455 961 328 B — 3.22 GiB *(fitted)* |

Each run produced the right answer. A measurement of a pipeline that did not
compute anything would measure nothing.

### The executable path does not accumulate across modules

Processing sixteen modules one at a time, releasing each module's state before
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

### Set-wide resolution is the one linear term, and it is measured

docs/42 module resolution is the part of the path that cannot be phased away.
`check_module_set` compares every module's declared name, imports and type table
against every other's, and it reads them **from parse trees** — so every module
of a closure is live at once, and this term is linear in the closure size.

The slope is measured, not assumed: 1, 8 and 32 modules of 8 KiB give
419 928 bytes per module; 1, 2 and 4 modules at the 256 KiB ceiling give
13 499 848 bytes per module. Both are ~51× the module's source size, which is
what a parse tree costs.

## The constraint this exposes

The two published ceilings of docs/44 section 2 are independent: a source unit
may be 256 KiB, and a dependency closure may hold 256 modules. Their product is
a closure whose resolution needs **~3.2 GiB** with the current architecture, and
the ADR-0040 reference platform has 256 MiB.

So, on the reference platform, this implementation resolves a closure of roughly
**19 ceiling-sized modules**, or **256 modules averaging about 8 KiB** — not the
maximal closure the ceilings jointly admit. That is an implementation limit and
it is stated as one: no accepted document requires a maximal closure to resolve
in 256 MiB, and nothing here weakens either published ceiling.

It is also avoidable, and worth recording as the next architectural step rather
than as a defect. Resolution needs each module's declared name, its imports and
its declared type names — a bounded summary of a few kilobytes — not its parse
tree. Extracting summaries in a first pass and resolving over those would make
the linear term about three orders of magnitude smaller and leave the bound at
"the largest single module", which the phased measurement above already shows is
where the executable path sits. That refactor is not done.

## What is not claimed

- Not P2 or P3: one machine, one build, no CI reproduction and no independent
  reproduction (docs/35).
- The two ceiling-closure figures are **fitted from a measured slope**, and are
  labelled so wherever they appear. The 256-module ceiling-sized case needs more
  memory than the machine this was measured on has, which is itself the finding.
- The measured figures cover the reference implementation. Another conforming
  implementation's arena is its own to measure.
- The grant the nucleus makes is a separate, declared decision
  (`crates/tos-runtime/src/region.rs`), deliberately not derived from this
  number automatically: a bound that silently became a configuration would stop
  being a bound anyone had to look at.
