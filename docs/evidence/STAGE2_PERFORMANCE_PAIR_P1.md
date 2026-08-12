<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 performance — one paired measurement, and what it says

Evidence level: **P1** (locally measured, docs/35). Verdict: **FAIL**, retained.
Commit: the one this file is committed in. Both halves are from it.
Fixtures: emitted by `tos-core-performance --emit-fixture`, so the native and
reference halves measure the **same bytes**, not two fixtures that resemble each
other. Frontend fixture content id `sha256:40ea301db1e52502190794049cecf65ddc40a76cde07a98181cca9a1aa433a98`.

## Native half — 3 warmups, 21 samples

```text
parse + check + lower + independent verify, 262 114-byte module
  median 163 031 us, p95 167 775 us, p99 167 923 us
one-million-operation integer/control-flow benchmark
  median 321 903 us, p95 325 350 us, p99 328 063 us
reject a quota-exceeding module
  median  64 884 us, p95  66 668 us, p99  67 063 us
  rejection/acceptance p95 ratio 0.397 (budget at most 2.000) — PASS
```

## Reference half — ADR-0040 platform, real Stage 2 path, 5 samples

```text
one-million-operation integer/control-flow benchmark
  samples (us) 5 226 952  5 233 254  5 329 502  5 368 337  5 473 077
  median 5 329 502 us, p95 5 473 077 us
```

### Engine metric: **FAIL**

```text
reference p95 / native p95 = 5 473 077 / 325 350 = 16.8x
docs/35 budget: at most 10x
```

Same fixture, same commit, same work definition, measured on both sides. This is
not the "ordinary TCG factor" argument the previous record leaned on; it is the
ratio the contract asks for, and it is over budget by 1.7x.

### Frontend metric: **FAIL, and not yet measurable**

The 256 KiB fixture does not complete on the reference platform. It reaches
`TOS.RUN.STAGE name=lower` and does not emit `verify` within ten minutes. The
budget is 500 ms p95, so a measurement is not needed to know the verdict, but the
*cause* is needed before anything is claimed about it.

## What the two verdicts, read together, point at

The allocator defect that produced the previous 900-second failure is fixed and
its evidence is in `docs/evidence/STAGE2_ALLOCATOR_SEARCH.md`: the arena-bound
sweep went from hours to 16.5 s, and the same fixture natively through the
bounded heap now costs a fraction of a second.

So the frontend's remaining cost is not allocation search. The engine metric
gives the platform's honest factor for work that is arithmetic- and
control-flow-heavy and barely copies: **16.8x**. The frontend is copy-heavy —
every string, every vector, every interned type — and it is at least three
orders of magnitude slower than that factor predicts.

The leading hypothesis, **stated as a hypothesis and not as a finding**, is the
freestanding target's memory primitives. `x86_64-unknown-none` has no libc, so
`memcpy`, `memset`, `memmove` and `memcmp` come from `compiler_builtins`, whose
portable implementations move a byte at a time. A host build gets glibc's
vectorised versions. A byte-at-a-time copy interpreted by TCG would multiply
every copy in the frontend by a large constant, and would leave the engine —
which copies almost nothing — at the ordinary factor. That is exactly the shape
observed.

It is not proved. Proving it means measuring the guest with and without an
optimised `memcpy`, and that is the next piece of work rather than something
this record asserts.

## What is not done here

- No fixture was changed to make a number smaller.
- No budget was reinterpreted. The engine ratio is 16.8x against 10x, and the
  frontend does not complete.
- The rejection ratio is measured natively (0.397, PASS) and **not** on the
  reference platform, because it is a ratio against an accepted-input
  measurement that the reference platform cannot yet produce.
- Evidence stays P1. One machine, one build, five reference samples rather than
  twenty-one — stated, not rounded up.
