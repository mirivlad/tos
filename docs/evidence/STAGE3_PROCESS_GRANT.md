<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 process grant — what the arena costs, measured

Evidence level: **P1, locally measured on the host arena harness**, which is
where an arena bound has always been measured (`STAGE2_ARENA_BOUND.md`): the
production pipeline runs *through* the instrumented heap, so the figure is the
allocator's own high-water mark rather than a sum of requests.

Verdict: **one module at the published source ceiling needs `50.33 MiB`; a
closure of ceiling-sized modules needs about `25 MiB` more per module above a
base near `60 MiB`; and the closure limit the implementation currently declares
would need about `6.3 GiB`.**

## Which number a grant has to cover

`STAGE2_ARENA_BOUND.md` reports two quantities and they are not
interchangeable:

- **committed** — live bytes in whole blocks at an instant. The
  `resolution_over_summaries` figure of `52.01 MiB` is this;
- **frontier** (`peak_extent`) — the highest address the arena was ever carried
  to, which never falls. `one_module_at_the_ceiling` and `an_executed_closure`
  report this.

A grant must cover the **frontier**. An allocator cannot hand out an address
above the region it was given, whatever the live total happens to be when it
tries. Sizing a grant from `committed` would be sizing it from the part of the
run that had already been given back.

The published frontier for one module at the 256 KiB source ceiling is
**`52 770 176 B` — `50.33 MiB`**.

## The closure, measured through `execute_set`

New here, and measured the same way: the whole production `execute_set` over a
set of ceiling-sized modules — an entry importing every dependency and calling
each exactly once — with the answer checked, so a run that skipped a module
could not produce the figure.

**The source corpus is excluded, deliberately.** In TOS the units are bytes of
the capsule, mapped outside the process grant; in the fixture they are host
allocations. So the corpus is built first, the frontier is read before the run,
and what is reported is the extent the pipeline needed *above* it. The frontier
never falls, so the corpus sits below and the difference is the pipeline's own.

| Ceiling-sized modules | Corpus | Above the corpus | Total frontier |
|---:|---:|---:|---:|
| 2 | 1.17 MiB | **109.40 MiB** | 110.57 MiB |
| 4 | 1.66 MiB | **160.07 MiB** | 161.73 MiB |
| 8 | 2.75 MiB | **261.25 MiB** | 264.01 MiB |
| 16 | 4.94 MiB | **460.00 MiB** | 464.94 MiB |

Least squares over those four points: **`25.03 MiB` per module** above a base of
**`59.94 MiB`**, linear across the measured range — the residuals are under a
megabyte at every point.

## Where the slope came from, and what removing it did

The slope above was measured against `execute_set` **as it was**: it parsed
every module and held every parse tree for the whole run, then accumulated every
lowered module beside them. Walking the same phases on the same fixture and
reading the arena between them attributes it, per ceiling-sized module:

| Retained object | Per module |
|---|---:|
| normalized `SourceUnit` | 0.22 MiB |
| **parse tree (`Schema`)** | **13.99 MiB** |
| owned summary | 0.19 MiB |
| **lowered IR (`Module`)** | **15.13 MiB** |

The parse-tree figure is the one `STAGE2_ARENA_BOUND.md` already published —
`14 040 464 B` per ceiling-sized module — and the summary is `74x` smaller,
which is why that evidence has `summarize()` return an owned value and
`check_module_summaries()` take no tree.

`execute_set` now does what that architecture says: parse, check and summarize
one module at a time, drop each tree at the end of its turn, resolve over
summaries, and build a tree again only to lower the module it belongs to — a
second run of the same deterministic parser over the same normalized bytes, with
every frontend and verifier boundary intact.

| Ceiling-sized modules | Retaining path | Phased path |
|---:|---:|---:|
| 2 | 109.40 MiB | **92.83 MiB** |
| 4 | 160.07 MiB | **118.16 MiB** |
| 8 | 261.25 MiB | **168.76 MiB** |
| 16 | 460.00 MiB | **268.15 MiB** |

`25.03 MiB` per module became **`12.52 MiB`**, above a base of `68.09 MiB`. The
remaining slope is the lowered IR: `run_set` is handed every module of the set
at once, so every `Module` stays alive until the run ends. Whether that is
reducible is an open question and is not answered here.

## What the remaining slope is made of

The `12.52 MiB` per module that survived the phasing is the lowered IR, and the
next question is what *that* is: semantic content, or this representation
carrying it. Measured per lowered `tos_ir::Module` on the same fixture, with
`canonical_stream` used **only** as a density estimate of the semantic payload —
docs/43 has deliberately not fixed an on-disk encoding and nothing here proposes
one:

| | Live `Module` | Canonical stream | Ratio |
|---|---:|---:|---:|
| a ceiling-sized dependency | 12.10 MiB | 5.26 MiB | **2.3x** |
| the entry (more functions) | 19.18 MiB | 8.41 MiB | **2.3x** |
| 8-module closure, total | 104.03 MiB | 45.24 MiB | **2.3x** |

The ratio is the same at 2, 4 and 8 modules, so it is a property of the
representation rather than of the fixture.

Table counts for one ceiling-sized dependency module: `2 268` types, `2 268`
exports, `2 268` functions, `2 268` blocks, `6 801` instructions, `9 068` SSA
values, `1` constant, and **`11 338` source-map entries**.

And the source map is where the repetition lives. Each `SourceMapEntry` owns six
`String`s — source set, path, content id, frontend identity, language version
and normalization baseline — of which five name the *module*, not the operation:

| | Strings held | Distinct bytes | Repetition |
|---|---:|---:|---:|
| dependency module | 1 666 686 B (1.59 MiB) | **147 B** | 11 338x |
| entry module | 3 198 450 B (3.05 MiB) | **150 B** | 21 323x |

So `13–16 %` of a live module is one hundred and fifty bytes of text, written
out once per lowered operation.

**Superseded, and the correction matters.** This section originally read the
canonical stream as the module's semantic payload and the difference as
representation overhead — "of about `15 MiB` live, roughly `6.5 MiB` is meaning".
`STAGE3_COMPACT_IMAGE_P1.md` falsified that: the stream is itself a
representation with its own costs, and the same module — identical `tos-ir/v1`
content, confirmed by an unchanged semantic digest after a round trip through a
compact encoding — is `388 329 B`, `14.32x` below the stream. **No figure here is
a semantic minimum.** The `2.3x` above is one representation against another, and
that is all it was ever entitled to say.

This is diagnostic. What to do about it was ADR-0070's question, and the engine
is not touched until residency is decided as well.

## What that says about the declared limit

docs/44 §2 requires published numeric limits and permits a lower cap "if
reported in the implementation's declared conformance profile". The reference
implementation reports none: `tos_verifier::limits::Limits::default()` is the
accepted V1 ceiling with `modules: 256`, and `tos_core::MAX_SOURCE_BYTES` is
`256 KiB`. So the implementation promises the ceiling.

Extrapolating the **retaining** path gave about `6.3 GiB`, and that figure is
retained here only as what that implementation would have cost. It is **not** the
necessary cost of TOS Core V1: half of it was parse trees the accepted
architecture says to drop. The phased path extrapolates to about `3.2 GiB`,
which is still an extrapolation of an implementation that keeps every lowered
module alive, not a contract requirement either.

No lower conformance cap follows from this. One would become a question only if
a bounded, phased implementation still showed an irreducible requirement
incompatible with ADR-0040, and nothing measured so far shows that.

## The candidate, and its margin

`RUNTIME_GRANT = 54 MiB` (provisional, ADR-0069):

- against one ceiling-sized module at `50.33 MiB`: **`3.67 MiB` of margin**,
  about `7%`;
- against a two-module closure at `109.40 MiB`: **it does not fit**, and misses
  by more than the whole grant.

## Four processes on the reference platform

Measured from the boot log rather than computed:

| | Frames | Bytes |
|---|---:|---:|
| Pool after the nucleus takes its own | 58 839 | ~229.8 MiB |
| One process, everything it holds | 14 356 | 56.08 MiB |
| Four processes | 57 424 | 224.3 MiB |
| Spare | 1 415 | ~5.5 MiB |

The `2.08 MiB` between the grant and the `56.08 MiB` is the process's stack,
report region, argument region, launch record and page tables.

Four fit, and not on paper: the ADR-0067 lifecycle gate now reaches
`TOS.RUN.PROCESS_REFUSED reason=no-slot uncollected=3`, which is only reachable
when the fourth slot is occupied rather than unaffordable. Before the grant
stopped being carved, that phase failed with `reason=no-grant` at 33 000 free
frames.

## What binds next

**Memory, through the process table.** A grant of roughly `55 MiB` or more
leaves the fourth process without one — `(229.8 - 4 x 2.08) / 4 = 55.37 MiB` —
after which the refusal changes from `reason=no-slot` to `reason=no-grant` and
`MAX_PROCESSES` says four while meaning three.

The crossing point is **approximate**: a larger grant needs more page tables, so
the per-process overhead above the grant is itself a function of the grant size.
`MAX_PROCESSES` and the grant are jointly constrained by the ADR-0040 memory
budget rather than independently choosable.

## Reproduction

From `source/`, on the host:

```sh
cargo run --release -p tos-arena-bound -- --ceiling --modules 8 --unit-bytes 262144
```

Each module count runs in its own process on purpose: the frontier never falls,
so a second measurement in the same process would inherit the first one's
high-water mark and report it as its own.
