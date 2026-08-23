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
2. **Conformance observer and IPC report.** Use a reproducibly pinned QEMU build
   with the upstream low-overhead simple-trace backend and the accepted narrow,
   hash-bound symmetric UART observation patch on the unchanged
   q35/qemu64/one-vCPU/256-MiB/TCG machine profile. Validate it before measuring
   IPC. Do not introduce batches, subtraction, a slower denominator, or a
   replacement TOS implementation.
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

The QEMU observer records thread CPU time after handling `OPEN` and before
handling `CLOSE`, and emits both timestamps together. QMP enables this one event
only between `READY` and the final `CLOSE`. The empty floor and fixed denominator
use a measurement-only nucleus with timer preemption inactive; the IPC numerator
must keep preemption active. The smaller denominator makes the ratio stricter.

The denominator is immutable: one ordinary local TOS Core call to an exported
function taking one 64-byte value and returning `unit`, with run setup outside
the marks and the unavoidable call accounting inside them. IPC is not measured
until one prepared boot supplies 21 adjacent, alternating-order floor/call
blocks after three warm-ups and at least 19 paired differences are positive
(one-sided exact sign `p <= 0.000111`). The build manifest binds exact artifacts,
features and no-preemption state. Dropped, duplicated, out-of-plan, reversed,
zero-duration or negative-duration observations invalidate the series.

The numerator uses the production endpoint path with timer preemption active.
One unmeasured 64-byte request/reply primes the server into its atomic
`endpoint_reply_receive` loop. Each of the following three warm-up and 21
retained intervals contains exactly one client `endpoint_call`, one 64-byte
request and one 64-byte reply from the other address space. The server waits
again before the next interval; report generation and process shutdown remain
outside every interval. Its nearest-rank p99 must satisfy both the relative and
absolute budgets in the same series. A failed series is retained and cannot be
replaced by a successful retry.

## Observer choice

Three approaches were considered:

- **Pinned QEMU simple trace plus symmetric UART observation — selected after
  measurement.** The unmodified backend failed six clean stability repetitions;
  applying thread CPU time to the whole backend still failed four of ten. The
  selected observer instead changes exactly two hash-bound QEMU source files:
  the UART captures the two physical thread-CPU timestamps outside marker
  transport, and one trace event carries the untouched pair. Guest-visible UART
  behavior and the machine model do not change. Source version, both before and
  after hashes, configure flags, compiler and resulting binary digest are
  recorded. The build consumes a separately acquired release archive only after
  verifying its fixed SHA-256 and disables all build-time downloads. The
  launcher, engine and retained ROM inputs are each hashed in the manifest.
- **A custom QEMU marker device — rejected.** The accepted patch observes the
  existing measurement-only COM1 path; it adds no replacement device or TOS
  semantic mechanism.
- **Batching, division, overhead subtraction, host-reader timestamps, RDTSC or
  `-icount` — rejected.** They measure throughput/averages, can understate the
  interval, or substitute virtual instruction time for physical duration.

If the selected observer fails the predeclared exact sign test in the clean P2
gate, that is a red measurement result and a new observer/platform decision, not
permission to use the rejected approaches.

## Failure handling and evidence

The harness fails closed on a missing marker, sequence mismatch, timestamp
reversal, zero/negative interval, wrong sample count, dropped trace event,
changed production artifact, wrong QEMU identity, or wrong machine profile.
Raw samples and environment identities are retained. Median and nearest-rank
p99 are derived from the retained samples; no sample is repaired or filtered.
The qualifier writes a red verdict record before returning failure so that a
threshold miss remains reviewable evidence rather than disappearing as a
failed command.

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
