<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0103: Stage-4 performance contract reconciliation and measurement

- Status: **Accepted**
- Date and Project Architect approval: **2026-09-26**
- Decision level: **2** — performance and scheduling contract clarification
- Related: ADR-0040, ADR-0049, ADR-0066, ADR-0097, ADR-0098; `docs/35` §Stage 4

## Decision

After queue initialization, a completed block request performs zero per-request
allocation of DMA regions, queue/ring storage, descriptors, endpoints, launch
plans or driver-private working state. It may newly allocate at most one ordinary
Region for the immediate client/service payload transfer required by
`BLOCK_DEVICE_V1`. That transient Region is funded, bounded and consumed or
released by the request lifecycle and cannot accumulate across requests. This
IPC ownership-transfer cost remains measured and reported. Zero transient
payload-Region allocation is preferable where a future compatible protocol
permits it, but is not required for Stage 4. H2 is unchanged.

A currently running context that remains runnable continues running. Making
another context runnable does not itself force a switch. Round-robin selection
occurs when the current context blocks, exits or faults, its fixed quantum
expires, or another accepted explicit scheduling point requires selection.
Selection begins after the current context and wraps. Timer preemption,
CPU-bound peer progress and the single priority band remain as in ADR-0049.
H3 remains at most four address-space/scheduler handoffs per unbatched request;
timer-preemption noise is separately reported. If ordinary implementation work
cannot meet H3, the structural sequence returns to the Project Architect.

## Reference measurement

The platform is QEMU q35, qemu64, one active vCPU, TCG, the same VirtIO-block
device configuration, queue depth and byte-identical raw image copies, using
release builds. Retained windows run without concurrent QEMU instances. Archive
the exact QEMU, observer, host, runtime-engine and canonical-module identities,
cache state, device configuration, image digest and scheduler quantum.

Use an external observer as in ADR-0066, with measurement-only markers that do
not alter production `block.device.v1`. Elapsed I/O uses
`CLOCK_MONOTONIC_RAW`; CPU cost uses `CLOCK_PROCESS_CPUTIME_ID` for the complete
QEMU process, including vCPU and device-model work. Do not subtract observer
cost. Archive the observer and its build identity.

The Rust VirtIO-block reference is a separately isolated minimal benchmark
artifact that directly drives the same device. It is only an oracle: it is not
linked into the production nucleus, is not a TOS driver or runtime dependency,
and cannot satisfy the Stage-4 identity gate. Keep the production device
vocabulary guard intact.

| Metric | Fixed workload and calculation | Existing threshold |
|---|---|---|
| R1 | Queue depth 1, 512-byte requests, same sequential sectors; warm up 64 KiB, then retain 3 × 1 MiB READ windows. Total retained bytes divided by total retained wall time. | TOS throughput ≥ 35% of Rust reference |
| R2 | One aligned random 4 KiB operation is eight consecutive 512-byte READs on both sides. Same deterministic seed and sector sequence; 3 warm-up and 300 retained logical operations. Nearest-rank p99 is rank 297. | TOS p99 ≤ 5 × Rust reference p99 |
| R3 | R1's retained windows; complete QEMU-process CPU time divided by transferred MiB. | TOS CPU/MiB ≤ 8 × Rust reference CPU/MiB |

Measure before changing any R1–R3 threshold. On a miss, decompose the profile,
separate evidence-only overhead from production work, fix ordinary Level-1
defects and rerun. A new ABI, language primitive, execution-engine semantic
optimization, `BLOCK_DEVICE_V1` change, threshold or architectural boundary
requires a further Project Architect decision. Stage 4 remains open meanwhile.

## Architecture impact

No TOS invariant, canonical representation, source identity, persistent format,
trust boundary, owner recovery or rollback path changes. The production trusted
base gains no dependency; Rust is a reference-oracle build dependency only.
The QEMU/qemu64/TCG single-vCPU compatibility profile is the declared Stage-4
measurement profile. Licensing and provenance of the separately built observer
and oracle must be archived with the measurements. This decision itself adds no
patent mechanism. The Stage-3 scheduler/preemption/IPC gates, Stage-4 gates and
H1–H3 measurement enforce its implementation; R1–R3 require the archived
paired-reference evidence. The Stage-4 threat model and negative device tests
remain applicable and unchanged.
