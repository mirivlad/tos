<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 performance evidence design

Status: implementation design under accepted ADR-0025 (2026-08-09). This
documents the evidence harness; ADR-0025 controls the performance conformance
contract.

## Scope and authority

`docs/35_PERFORMANCE_CONTRACTS.md`, Stage 1, is the controlling Tier 2
requirement: a release capsule with 1,000 files and exactly 16 MiB of payload
must validate and locate `/system/boot/init.tos` within 250 ms p95 on the
declared QEMU CI profile. Its Measurement rules require the recorded
environment, raw samples and percentile statistics. The threshold and the
guest/loader semantics are not changed here.

The host-harness implementation is Level 1. ADR-0025 is the accepted Level 2
conformance decision. Neither adds a source/runtime ABI, trusted-base
dependency, capsule-format change or stable boot event. The normal QEMU
harness remains the sole capsule/ESP/OVMF/q35 boot path.

## Measured interval

Each run records a host monotonic timestamp for every existing `TOS.*` serial
event. Only the following two retained timestamps define the metric:

```text
start = TOS.BOOT.ENTRY
end   = TOS.BOOTTEXT.PATH
latency = end - start
```

`TOS.BOOT.ENTRY` is emitted before the loader reads and validates the capsule.
`TOS.BOOTTEXT.PATH` is emitted only after loader validation and handoff, the
nucleus's independent capsule/identity validation, and canonical boot-text
lookup. The interval deliberately excludes firmware start-up before the first
event and post-lookup display/terminal work after the second event.

Timestamps are observations by the host process receiving the serial bytes;
they are not guest-cycle or guest-clock values. This avoids introducing a new
guest timing/calibration contract and makes the result directly reproducible
on the QEMU CI profile. The QEMU command, OVMF pair, q35 machine, qemu64 CPU,
256 MiB guest memory and normal `isa-debug-exit` self-judging result are the
same as `host-tools/qemu-test/run.sh`.

## Workload and sampling

The harness deterministically generates its ignored fixture under `target/`:

- exactly 1,000 canonical files: `/system/boot/init.tos` and 999 sorted
  `/system/lib/` files;
- exactly 16 MiB (16 × 1024 × 1024 bytes) across file contents;
- detached capsule source identity, because the synthetic input tree is not a
  Git checkout; and
- the existing GPL licence notice and a provenance sidecar checked by the
  existing capsule provenance verifier.

The generated fixture is an evidence input, not a tracked capsule vector. Its
manifest, capsule and sidecar are retained in the ignored result directory for
reproduction.

The runner performs three unreported warm-up boots followed by 21 measured
boots. It retains a JSONL raw sample for every measured boot, including the
full existing-event timestamp trace and the resulting duration. The report
computes:

- median: the 11th value after ascending sort;
- p95: nearest-rank `ceil(21 × 0.95)`, the 20th value; and
- p99: nearest-rank `ceil(21 × 0.99)`, the 21st value.

The runner fails if p95 exceeds 250 ms. It records the source commit, stage,
QEMU version, hashes and paths of both firmware inputs, host CPU description,
TCG virtualization mode, guest CPU/memory, Rust version, workload identity,
warm-up/sample policy, raw samples, aggregate statistics and the explicit
baseline declaration. The first CI result records `none; initial Stage 1 P2
baseline` rather than inventing a comparison value.

## Evidence status and retention

Locally invoked output is labelled P1. The QEMU GitHub Actions workflow runs
the exact runner under its ordinary TCG profile with P2 status and uploads the
fixture manifest, provenance sidecar, serial/event logs, raw sample JSONL and
report as the `qemu-boot-evidence` artifact. The immutable Stage 1 report may
only cite a successful P2 artifact for closure.

No new serial namespace or guest test mode is introduced. An absent event,
wrong event order, failed self-judging exit or a p95 above the existing budget
is a failed performance run, not a skipped sample.
