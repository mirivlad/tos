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

## What that says about the declared limit

docs/44 §2 requires published numeric limits and permits a lower cap "if
reported in the implementation's declared conformance profile". The reference
implementation reports none: `tos_verifier::limits::Limits::default()` is the
accepted V1 ceiling with `modules: 256`, and `tos_core::MAX_SOURCE_BYTES` is
`256 KiB`. So the implementation promises the ceiling.

At the measured slope that promise costs `59.94 + 256 x 25.03 ≈ 6 468 MiB`,
about **`6.3 GiB`** — roughly twenty-five times the whole ADR-0040 reference
platform, which has `256 MiB`. The gap is between the promise and the machine,
not between the promise and the grant.

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

**Memory, through the process table.** A grant much above `55.3 MiB` leaves the
fourth process without one: `(229.8 - 4 x 2.08) / 4 = 55.37 MiB`. Past that the
refusal changes from `reason=no-slot` to `reason=no-grant`, and `MAX_PROCESSES`
becomes three in practice while still saying four.

So the process table size and the grant size are one decision with two names.
Neither can be raised without lowering the other, on this platform.

## Reproduction

From `source/`, on the host:

```sh
cargo run --release -p tos-arena-bound -- --ceiling --modules 8 --unit-bytes 262144
```

Each module count runs in its own process on purpose: the frontier never falls,
so a second measurement in the same process would inherit the first one's
high-water mark and report it as its own.
