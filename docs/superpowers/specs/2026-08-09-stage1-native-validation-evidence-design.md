<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 native validation evidence design

Status: Project Architect-directed research scope, 2026-08-09. This document
does not amend ADR-0025, `docs/35_PERFORMANCE_CONTRACTS.md`, or any Stage 1
gate.

## Question

The accepted ADR-0025 qemu64/TCG measurement has p95 2842.450 ms for the
ordinary boot interval. This research distinguishes the cost of the exact
production validation logic from the surrounding firmware/TCG execution cost
without claiming that native timing closes F-18.

## Exact logical workload

One shared preparation path deterministically creates the existing 1,000-file,
exactly-16-MiB detached fixture under `source/target/`, builds it through the
production `tos-capsule-tool`, and checks its production provenance sidecar.
The native runner reads those capsule bytes once before timing. Each timed
sample then performs, in order:

1. a fresh `tos_capsule::parse(&bytes)` and drops that `Capsule` value;
2. a second fresh `tos_capsule::parse(&bytes)`; and
3. `boot_file()` on the second parsed capsule, verifying the canonical
   `/system/boot/init.tos` result.

`parse` is the production no_std parser used by both loader and nucleus. It
therefore performs the v1 structural checks, whole-capsule SHA-256,
per-file SHA-256 values and ADR-0018 detached identity computation on each
pass. The runner passes no prior parsed object, digest or validation result to
the second pass.

This mirrors the logical QEMU sequence `loader parse → nucleus parse →
canonical lookup`. It deliberately does not measure firmware startup, FAT/UEFI
file materialization, BootInfo construction, serial delivery, QEMU device
emulation or display/terminal work. Its result is an exact-validation logical
profile, not a replacement for ordinary QEMU functional evidence.

## Sampling and reports

The runner executes three warm-ups followed by 21 measurements. It writes a
JSONL record for every sample containing phase, index and duration in monotonic
nanoseconds. Its report records median (rank 11), nearest-rank p95 (rank 20),
p99 (rank 21), source commit, Rust version, host CPU/OS and workload/fixture
identity. The raw native report, the existing qemu64/TCG raw report and their
event decomposition are compared in one research summary with an explicit
ratio.

The native result is labelled research P1 and cannot mark F-18 PASS. The QEMU
result remains the ADR-0025 functional/conformance evidence. An alternate
QEMU profile, if host support exists, is recorded only as research evidence
with its complete command/profile; it is never selected by the normal harness
or gate.

## Boundaries and evidence

No loader, nucleus, capsule, Boot ABI, hash implementation, trusted-base
dependency, unsafe code, assembly, CPU extension or QEMU CI profile changes.
The native harness is host test tooling only. Unit tests prove the runner uses
two production parser calls and the second-pass canonical lookup; shell tests
prove fixture/capsule/provenance equivalence with the QEMU runner.

If native double validation does not reasonably meet the existing 250 ms p95
budget, the evidence must not be used to fit an ADR to the desired conclusion.
The investigation instead reports the measurements and architect-reviewed
alternatives. If it does meet the budget, a separate **Proposed** ADR-0026 may
describe a split between mandatory QEMU functional conformance and a native
logical-validation performance profile; it remains non-authoritative until
accepted.
