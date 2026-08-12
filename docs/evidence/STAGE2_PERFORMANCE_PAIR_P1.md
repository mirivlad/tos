<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 performance — the complete paired measurement

Evidence level: **P1** (locally measured, docs/35). Verdict: **2 FAIL, 1 PASS**, retained.
Procedure: the normative one — 3 warmups, 21 samples, median/p95/p99, one commit,
one set of fixtures, both halves.
Fixtures: emitted by `tos-core-performance --emit-fixture`, so the native and
reference halves measure the **same bytes**. Frontend fixture is 262 114 bytes,
content `sha256:40ea301db1e52502190794049cecf65ddc40a76cde07a98181cca9a1aa433a98`.
Reference platform: ADR-0040 — q35, qemu64, 1 vCPU, 256 MiB, TCG, OVMF, through
the real freestanding Stage 2 path.
Producers: `tos-core-performance --profile native`,
`host-tools/qemu-test/stage2-reference-performance.sh`.

## The pair

| metric | native p95 | reference p95 | budget | verdict |
|---|---|---|---|---|
| frontend, 256 KiB module | 160 893 us | **1 490 798 us** | 500 000 us | **FAIL** (2.98x over) |
| engine, 1e6 operations | 333 743 us | 5 541 378 us | ratio ≤ 10x | **FAIL** (16.6x) |
| quota rejection | 66 668 us | 763 305 us | ≤ 2x accepted | **PASS** (0.512) |

Reference medians: frontend 1 437 944 us, engine 5 317 426 us, rejection
713 676 us. Every raw sample is in `target/.../reference.json` and in the
harness output; nothing is summarised without the samples behind it.

## The implementation is understood

Two defects were found and fixed before this measurement, and each was fixed
rather than argued around:

1. **The heap searched every block on every allocation.** Segregated free lists
   replaced it; the search cost no longer depends on how many blocks exist, and
   the claim is a measured series rather than a timing
   (`docs/evidence/STAGE2_ALLOCATOR_SEARCH.md`).
2. **The lowerer rendered a debug string to intern every type.** Keying the
   index on the definition itself took the reference frontend from *not
   finishing in 900 seconds* to 1.49 s — while costing 7% natively
   (`docs/evidence/STAGE2_FREESTANDING_PRIMITIVES.md`).

One hypothesis was tested and **refuted**: the freestanding memory primitives
are word-oriented in the real binary, not byte-at-a-time. No work was done on a
defect that did not exist.

The evidence that no third pathology is hiding is that the platform factor is
now **uniform**:

```text
frontend  reference / native = 1 490 798 / 160 893 =  9.3x
engine    reference / native = 5 541 378 / 333 743 = 16.6x
```

Before the fixes these differed by three orders of magnitude. Two independent
workloads landing within a factor of two of each other is what "the platform
costs what the platform costs" looks like from outside.

## Why the engine ratio is not an implementation question

The engine budget is a **ratio of the same implementation to itself** on two
platforms. Optimising the engine moves the numerator and the denominator
together, so the ratio barely moves. A 10x budget stated that way is a claim
about how much slower TCG is than the host — not about the engine — and the
measurement says TCG is 16.6x for this workload.

The only things that can bring that ratio to 10x are a different platform
(ADR-0040 chose TCG deliberately, so it is reproducible on any host) or a
different number. That is a structural argument, not a plea, and it is what
ADR-0043 carries.

The frontend budget is different in kind: 500 ms is absolute, so implementation
work *can* reach it. It needs the native frontend at roughly 54 ms rather than
161 ms — about 3x — and no defect explaining that gap has been identified.

## What is not claimed

- P1. One machine, one build. Not P2, not P3.
- No fixture was changed, no boundary reinterpreted, no budget adjusted to fit.
- The rejection ratio passes **on the reference platform**, not only natively.
- The measurement boundaries are protected against the earlier defect where a
  span between two result events measured line formatting rather than work:
  `reference-performance-report.py` refuses a reduction whose boundary cannot
  contain the work it claims to measure.
