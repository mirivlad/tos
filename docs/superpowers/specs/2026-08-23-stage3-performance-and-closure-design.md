<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 performance evidence and closure design

## Goal

Close Stage 3 without changing its architecture to suit a benchmark: preserve
the production tick model, measure the fixed TOS Core call and IPC exchange with
one external observer on the ADR-0040 profile, and complete the remaining
restart, adversarial and identity evidence before Stage 4 begins.

## Scope decomposition

The work has three reviewable results and is implemented in this order:

1. **Measurement decision and diagnostic instrument.** Publish accepted
   ADR-0066, repair the existing test-only COM1 instrument, and retain its
   channel and denominator result as P1 diagnostic evidence. The current QEMU
   log backend is explicitly insufficient for the relative budget; it is not a
   conformance observer.
2. **Conformance observer and IPC report.** Use an upstream, reproducibly pinned
   low-overhead QEMU trace backend on the unchanged q35/qemu64/one-vCPU/256-MiB/
   TCG machine profile. Validate it before measuring IPC. If no such upstream
   backend can resolve one call, stop with evidence; do not introduce batches,
   subtraction, a slower denominator, or a custom replacement implementation.
3. **Stage 3 closure evidence.** Implement the accepted restart identity
   contract, add the E3 capability and ABI adversarial tests required by
   `docs/34`, and produce the trusted-base/identity report required by
   ADR-0048 and `docs/37`.

Each result must be independently green and reviewable. A failure in result 2
does not weaken a budget, and a complete result 1 is not reported as an IPC
performance pass.

## Measurement architecture

The production system keeps ADR-0049's monotonic tick for scheduling and bounded
timeout accounting. It gains no wall clock, calibrated duration unit,
`system.time.Clock`, measurement ABI, or production capability.

The observer is a host-side instrument. The measurement build alone permits
CPL 3 to access COM1 through a TSS I/O bitmap with IOPL 0. The observed process
emits sequence-tagged `OPEN` and `CLOSE` markers. Both the fixed in-process call
and the IPC exchange use the same marker path, QEMU build, machine profile,
warm-up policy, and 21 individual samples. No observer cost is subtracted.

The denominator is immutable: one ordinary local TOS Core call to an exported
function taking one 64-byte value and returning `unit`, with run setup outside
the marks and the unavoidable call accounting inside them. IPC is not measured
until the observer separates that call from its own floor without dropped,
reversed, zero, or negative observations.

## Observer choice

Three approaches were considered:

- **Pinned upstream QEMU simple trace backend — selected.** It is the same QEMU
  implementation and machine model with a build-time upstream trace backend,
  not a new TOS mechanism. Its source version, configure flags, compiler and
  resulting digest must be recorded. Source acquisition must follow the
  repository's pinning and mirroring policy.
- **A custom QEMU marker device or trace patch — reserve only.** It could reduce
  overhead but would create a project-maintained measurement implementation.
  It is not admitted while an upstream backend can do the job.
- **Batching, division, overhead subtraction, host-reader timestamps, RDTSC or
  `-icount` — rejected.** They measure throughput/averages, can understate the
  interval, or substitute virtual instruction time for physical duration.

If the selected backend remains unable to resolve the call, that is a red
measurement result and a new observer/platform decision, not permission to use
the rejected approaches.

## Failure handling and evidence

The harness fails closed on a missing marker, sequence mismatch, timestamp
reversal, zero/negative interval, wrong sample count, dropped trace event,
changed production artifact, wrong QEMU identity, or wrong machine profile.
Raw samples and environment identities are retained. Median and nearest-rank
p99 are derived from the retained samples; no sample is repaired or filtered.

The current log-trace series remains useful negative evidence: it validates the
protocol and the semantic call boundary but its floor overlaps the call. It
must be labelled diagnostic and must not enter the conformance gate.

## Stage 3 closure after performance

The next functional work is not Stage 4. It is the missing Stage 3 evidence:

- restart creates a new process instance, increments restart generation, and
  preserves module/source/supervisor lineage in the audit record;
- E3 mutation/property tests cover guessed, forged, stale and re-encoded
  handles, attenuation bounds and arbitrary system-ABI register inputs;
- a versioned Stage 3 report inventories nucleus dependencies and privileged
  behavior, proving service policy remains in source-identified textual
  processes;
- the report names the exact performance evidence and all known limitations.

## Architecture impact statement

- **Change level:** Level 2 for ADR-0066, the observer identity and the
  conformance gate; Level 1 for test-only implementation of accepted contracts.
- **Invariants:** I-02, I-09, I-11, I-13, I-18 and I-19 are served and none is
  amended.
- **Canonical representation:** canonical TOS source is unchanged. Measurement
  fixtures are canonical text; reports and QEMU binaries are derived evidence.
- **Trusted base:** no production TOS component enters or leaves it. The QEMU
  observer is an external test instrument and is identified as such.
- **Source-to-runtime:** unchanged in production; the benchmark capsule retains
  the ordinary source, digest, verifier and engine chain.
- **Recovery/rollback:** unchanged.
- **Stage identity gate:** Stage 3 performance, restart and authority-bearing
  textual-service evidence; no Stage 4 claim.
- **Threat model:** no new production boundary. Measurement builds deliberately
  widen CPL 3 I/O only to COM1 and must prove production artifacts unchanged.
- **Performance contract:** both `<= 8x` and `<= 200 us` remain mandatory; no
  threshold or denominator changes.
- **Compatibility profile:** ADR-0040 machine profile unchanged; the measurement
  QEMU build identity becomes an additional recorded observer identity.
- **Dependencies/licence/provenance:** only upstream QEMU source/build material
  may enter the observer path, pinned and inventoried under its existing
  licence. No dependency enters the loader, nucleus or runtime.
- **Patent risk:** no new high-risk runtime mechanism; the observer is test
  tooling. Stage 3 IPC architecture is unchanged.

## Success criteria

Stage 3 closes only when the repository has accepted ADR-0066, green local and
CI gates on one SHA, retained raw P2 IPC samples satisfying both budgets, restart
identity evidence, required E3 adversarial tests, and a complete Stage 3
identity report. Anything less is reported as partial.
