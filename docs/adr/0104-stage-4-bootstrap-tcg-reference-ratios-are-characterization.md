<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0104: Stage-4 Bootstrap/TCG reference ratios are characterization and regression evidence, not closure thresholds

- Status: **Accepted**
- Date and Project Architect approval: **2026-09-30**
- Decision level: **2** — the closure interpretation of an existing performance
  contract; no invariant, ABI, format or trust boundary changes
- Amends: ADR-0103 §Reference measurement (what a miss means for closure);
  `docs/35` §Stage 4 reference-platform budgets
- Related: ADR-0040, ADR-0066, ADR-0103; `docs/16` §Stage 4; `docs/37` §Stage 4;
  `docs/evidence/STAGE4_PERFORMANCE_REPORT.md` §R1–R3;
  `docs/evidence/stage4-reference-r1-r3.json`;
  `docs/evidence/stage4-reference-decomposition.json`

## Context

`docs/35` stated three Stage-4 reference-platform budgets before anything could
measure them: sequential throughput at least 35 % of a reference implementation
(R1), random 4 KiB p99 latency at most five times the reference (R2), and CPU time
per MiB at most eight times the reference (R3). ADR-0103 fixed how they are
measured and said that no threshold may change before they are.

They have been measured. P1 evidence on clean commit `3708ea7`, against the
isolated Rust oracle and read by the Stage-4 external observer:

| Metric | Measured ratio | Original target |
|---|---|---|
| R1 | 0.00375 | ≥ 0.35 |
| R2 | 246.1 | ≤ 5 |
| R3 | 223.6 | ≤ 8 |

The measurement is valid: the workloads are equivalent (one sector digest on both
sides, every one of the 8 696 READs on the TOS audit record, byte-identical disk
images), the TOS side is the production textual-driver path with its audit trail
intact, and nothing was subtracted. The decomposition, kept as one complete boot per
variant, places about 92 % of process time in the block service's interpreted
engine steps — per-byte loops over the 512-byte sentinel of the −56 defence and over
the one payload copy that H2 permits and ADR-0037 forces. It is not IPC,
scheduling, the audit trail or a defect of the service: the ordinary Level-1
defects found were fixed in `3708ea7` and raised every ratio about 2.3×.

## Decision

1. **The original R1–R3 figures stay in the record as what they were:**
   pre-measurement research targets, and the misses that were actually measured.
   Nothing rewrites them as met, and no threshold is lowered to the numbers the
   current implementation happens to reach.

2. **For the declared Bootstrap/TCG profile they are no longer numeric Stage-4
   closure thresholds.** They are characterization and regression evidence.

3. **Stage-4 performance closure requires, instead:**
   - H1–H5 of `docs/35` §Stage 4 met (H1 as ADR-0103 amended it);
   - a valid ADR-0103 measurement of R1–R3 at evidence status P1 or higher;
   - workload equivalence between TOS and the reference, checked rather than assumed;
   - the production textual-driver path, with no binary or host bypass;
   - no hidden subtraction, correction, filtering or retry;
   - the ordinary Level-1 defects the measurement exposed, fixed;
   - a retained decomposition that explains the dominant cost;
   - the engine identity, cache state and every measurement identity (observer
     build, QEMU command, oracle, artifacts, disk image, scheduler quantum, host).

4. **The P1 result at `3708ea7` is the retained Bootstrap/TCG performance
   baseline** (`docs/evidence/stage4-reference-r1-r3.json`). Later changes must not
   degrade it silently: `docs/35` §Regression policy applies to its three ratios. A
   ratio worse than the baseline by more than 15 % requires an explanation; by more
   than 30 % it blocks a stage or release unless an ADR changes the contract. The
   gate `qemu_stage4_reference_performance` re-measures and applies that policy on
   every full run, so a regression cannot pass unnoticed.

5. **This is not a statement that the present performance is sufficient** for
   production storage or for any future high-throughput workload. It is a measured
   baseline of the reference profile, and nothing more.

## Why

The requirement R1 ≥ 35 % for the present fully interpreted textual-driver path
proved empirically incompatible with the Bootstrap engine. At the oracle's ~48 µs
per READ, R1 would need a TOS READ of about 137 µs — on the order of a hundred
engine steps for the whole service — while a single per-byte loop over 512 bytes is
five times that, and the forced copy is such a loop. Even the diagnostic variant
without the sentinel defence and without the audit trail reaches R1 = 0.0056.
Meeting the original numbers would not be a repair of the Stage-4 driver path; it
would be a new execution tier or a semantic optimization of the engine, which is a
separate architectural project.

That project must not be allowed to block the proof Stage 4 exists to give: that a
canonical textual user-space driver really moves persistent data through
final-style MMIO, interrupt, DMA and IPC boundaries (`docs/37` §Stage 4). Nor may
the numbers be quietly relaxed until they pass. Measuring, keeping the misses in the
record, and holding the measured baseline against regression is the position that
does neither.

## What is deliberately not done for Stage 4

- No compiled or JIT execution tier.
- No bulk `Region`/`DmaRegion` primitive.
- No multi-sector `BLOCK_DEVICE_V1`.
- **No weakening or removal of the −56 defence.** Security is not changed for a
  benchmark.

The strategic remedy for the gap is a faster execution tier, possibly a compiled
and verified derived one, with canonical text remaining the source of truth. Bulk
operations and multi-sector requests may be worth having for their own sake, but
they must come from the requirements of later stages, not as a benchmark-specific
bypass. If Stage 5 or a later performance gate needs engine work, that is its own
architectural decision, made against the real workload that needs it.

## Architecture impact

No invariant, canonical representation, persistent format, source identity, trust
boundary, owner recovery or rollback path changes. Nothing enters the trusted base.
The compatibility profile remains QEMU q35, qemu64, one vCPU, TCG. The measurement
tooling is test-side: the Stage-4 observer build (MIT-licensed patch tooling over the
GPL-2.0-only QEMU instrument, recorded in `THIRD_PARTY.toml`) and the
dependency-free Rust oracle (GPL-3.0-or-later), neither linked into TOS. No patent
mechanism is added. Enforcement: `qemu_stage4_reference_performance` (valid
measurement, workload equivalence and the regression policy against the retained
baseline) and `selftest_stage4_reference_decoder`; H1–H3 remain
`qemu_stage4_request_cost`.
