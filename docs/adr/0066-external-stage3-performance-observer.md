<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0066: Stage 3 performance is measured by an external observer

- Status: **Accepted** (Project Architect-approved)
- Date: 2026-08-23
- Decision level: 2 — fixes the measurement boundary for both quantitative
  Stage 3 IPC budgets and clarifies the production time model without adding a
  production interface
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-23
- Observer-profile amendment approval: Vladimir Tomashevskiy, 2026-08-23
- Supersedes: ADR-0049 section 1 only where “calibration source” could be read as
  calibrating the Stage 3 tick into a duration unit

## Context

`docs/35` and `IPC_V1` require p99 latency for one 64-byte request/reply to be
both no more than eight times a fixed in-process TOS Core call and no more than
200 microseconds on the ADR-0040 reference platform. ADR-0049 gives Stage 3 a
monotonic timer tick for preemption and bounded timeout accounting, but no wall
clock, trusted time source or calibrated duration unit.

Those statements were incorrectly read as a conflict and the performance
obligation was described in `PROGRESS.md` as deferred to a later stage. They do
not conflict. A system does not need to contain the clock that measures it. The
performance clock can be an external instrument, just as an oscilloscope does
not become a component of the circuit it measures.

The distinction matters. Calibrating the production APIC tick only to make a
gate express microseconds would add time semantics, a new trust dependency and
an attack surface for no system function. Refusing to measure until trusted
time exists would leave a Stage 3 budget unmeasured. Both are avoidable.

## Decision

### 1. Production time semantics do not change

Stage 3 keeps the time model ADR-0049 implements:

- a monotonic tick used by the scheduler and bounded timeout accounting;
- a fixed quantum whose configured count is recorded in Stage 3 evidence;
- no conversion of ticks to nanoseconds or microseconds;
- no wall-clock time, trusted time source or `system.time.Clock` capability;
- no production measurement operation, marker or port permission.

The APIC input, divider and initial count are implementation configuration, not
a physical-duration calibration. Trusted time remains Stage 7 work.

### 2. The Stage 3 performance clock is external

Both quantitative IPC budgets are measured by an observer outside TOS on the
ADR-0040 q35/qemu64/one-vCPU/256-MiB/TCG profile. Its timestamps are physical
duration units supplied by the host-side QEMU measurement path. The observer is
test tooling: it is not part of TOS Core semantics, a capability surface, the
system ABI or the production trusted base.

The conformance observer is a digest-pinned QEMU 10.0.11 build with the upstream
binary `simple` backend and a narrow, hash-bound UART observation patch. While
its measurement event is enabled, the one TCG vCPU thread reads
`CLOCK_THREAD_CPUTIME_ID` after handling an `OPEN` marker and before handling
the corresponding `CLOSE` marker. It emits the two raw timestamps together only
after the closing timestamp exists. UART behavior is unchanged. Host
descheduling, the two marker transports and trace-record construction are
therefore outside the interval; no duration is estimated or subtracted.

The event is enabled through QMP only after the observed process announces
`READY`, and disabled immediately after the last `CLOSE`. Firmware and boot-log
traffic cannot fill or perturb the measurement trace. Clock failure terminates
the observer, and the decoder rejects a missing/malformed pair or any reported
dropped event.

The exact observer backend, source modifications and build identity are part of every report. A
backend becomes a P2 conformance observer only after a versioned repository gate
proves its identity, timestamp point, clock, dropped-event behavior and ability
to resolve the fixed denominator against its own floor. Availability in another
QEMU build or suitability in principle is not evidence.

### 3. One observer measures both sides of the ratio

The same observer, QEMU build, machine profile, marker path and sample discipline
measure:

1. the empty marker interval, published as the observer floor, with timer
   preemption disabled only in this measurement build;
2. the fixed in-process denominator of `IPC_V1` section 8, under that same
   no-preemption measurement profile; and
3. one 64-byte request/reply between two runnable processes with preemption
   active.

Changing the observer between denominator and numerator invalidates the ratio.
The fixed denominator is not replaced by a Rust call, an entry invocation, a
batch average or a deliberately slower TOS Core function.

Disabling preemption for the floor and denominator is conservative: it removes
timer excursions and can only make the denominator smaller. The IPC numerator
must prove preemption active, and the build rejects combining the no-preemption
measurement feature with the two-process/request-reply profiles.

The numerator is the production IPC path under measurement-only endowments, not
a second benchmark implementation. One server is blocked in
`endpoint_reply_receive` before each retained interval. An unmeasured 64-byte
priming exchange establishes that steady state before `READY`. Each subsequent
host request makes the client emit `OPEN`, execute exactly one 64-byte
`endpoint_call` whose 64-byte reply comes from the other process, and emit
`CLOSE` immediately after that call returns. The server answers and waits again
atomically, including after the last measured reply; its final unsatisfiable
wait is cancelled only after the client consumes `STOP` and exits, outside all
intervals. Thus each sample contains the full request/reply, scheduler choices,
four user/nucleus crossings and both address spaces, while preparation, report
lines and shutdown remain outside it.

### 4. Measurement-only instrumentation is narrow and visible

A measurement build may add an observation path if all of these hold:

- the path is selected only by an explicit test feature and is absent from
  ordinary artifacts;
- production nucleus and runtime artifacts are hashed before and after the test
  build and remain byte-for-byte identical;
- IOPL remains zero; if the TSS I/O bitmap is used, it admits only the exact
  marker ports and denies every other port;
- no system call, scheduler action or production service is inserted merely to
  emit a marker;
- the floor is measured and reported beside the workload;
- no estimated or measured observer cost is subtracted from any sample.

Instrumentation that changes the production ABI, grants ambient I/O or moves
work out of the measured interval is not an observer of this contract.

### 5. Samples fail closed

Each series has three warm-ups followed by 21 individual measurements. Raw
samples, median and nearest-rank p99 are retained. A missing marker, duplicate
marker, overlapping pair, sequence mismatch, reversed timestamp, zero or
negative interval, wrong sample count or reported dropped trace event invalidates
the whole series. Nothing is repaired, reordered, filtered or clamped.

Observer resolution is a predeclared paired experiment in one prepared boot.
After three warm-up blocks, each of 21 retained blocks contains an adjacent
empty floor and denominator call with the same four-bit sequence identity. A
fifth tag bit selects the call; both markers echo the complete tag. Order
alternates `floor/call`, then `call/floor`, by block and is fixed before values
are seen. The measured artifacts and exact Cargo feature sets are bound by a
build manifest; scheduler preemption state is derived from that manifest, not
declared by a caller.

Resolution uses the one-sided exact sign test over all 21 paired differences
`call - floor`, with non-positive differences counted conservatively against
resolution. At least 19 of 21 must be positive: under the null of no directional
separation this is `p = 232 / 2^21`, approximately `0.000111`, below the
predeclared `0.000111` threshold. All values, including non-positive differences,
remain in the raw series and determine their respective median and nearest-rank
p99; no value is retried, filtered, reordered or subtracted. This controls
same-boot preparation and drift without demanding that independent clock and
emulation noise make every individual pair ordered.

The p99 is the p99 of one exchange. Repeating N exchanges between two markers
and dividing by N measures throughput/average and cannot satisfy this latency
contract.

The IPC verdict uses the numerator's nearest-rank p99 without retry selection:
that one value must be at most both `8 * denominator_p99` and `200 µs`. Passing
one bound cannot hide failure of the other. A timer/preemption tail is part of
the active-preemption numerator and is neither removed nor relabelled as
observer cost.

### 6. A coarse observer is a result, not a gate

An observer that fails the predeclared floor/denominator sign test may be
retained as P1 diagnostic evidence. It cannot produce the relative conformance
result and does not authorize IPC measurement. The next action is a separate
observer/build decision, not a change to the denominator, budget or arithmetic.

Likewise, a valid observer that measures a value above either budget reports a
red performance result. Threshold failure does not authorize changing the
clock, workload, denominator or system architecture inside the same result.

### 7. `-icount` is not a performance clock

QEMU `-icount` virtual instruction time is not cycle-accurate physical duration
and instruction count does not establish real execution latency. It is excluded
from both Stage 3 quantitative budgets.

## Evidence status and observer-profile amendment

The measurement-only COM1 protocol and QEMU `serial_write` log timestamps have
validated causal pairing, production-artifact isolation and the exact inner-call
semantic boundary at P1. The log backend is not accepted as the conformance
observer: its empty floor overlaps the inner-call distribution. The declared
Debian reference QEMU does not provide the binary `simple` trace backend.

The first unmodified upstream simple-trace series happened to have disjoint
ranges, but six subsequent clean repetitions all overlapped. Ten exploratory
runs with whole-backend thread CPU timestamps and dynamically bounded tracing
still overlapped in four runs. Those results reject both profiles; no successful
run was selected as conformance evidence.

The symmetric-pair patch was first exercised with floor and call in separate
boots. The resulting 210 of 210 index-aligned differences are retained only as
diagnostic evidence: an index does not make observations from distinct boots a
causal pair. Independent review rejected that method before commit.

The corrected candidate runs both observations adjacently in one prepared boot
and alternates their order. Its first exploratory run resolved 20 of 21 pairs;
five subsequent independent boots resolved 21 of 21 each, for 125 of 126 raw
pairs overall. The single non-positive difference was retained. These data
informed instrument development but do not qualify it: the exact sign-test rule
above was accepted before any clean P1/P2 confirmation.

A fresh local gate on clean commit `626876b64d0692443a6bac3aa3ebeb15c7b7d09d`
then resolved 21 of 21 pairs (`p = 2^-21`), with floor median/p99
`2.996/5.570 µs` and denominator median/p99 `9.623/29.896 µs`. The complete P1
record is retained in `docs/evidence/STAGE3_SYMMETRIC_OBSERVER_P1.md`. This
qualifies the observer locally and permits numerator implementation; P2 and the
Stage 3 IPC latency budgets remain open until CI qualification and the
subsequent IPC measurement pass.

Both then passed in one GitHub Actions `qemu`-profile job on pushed commit
`78447b31c62cd24a1549f5c2ac7833cdd6fe153b` (run 32644830444). The observer was
qualified twice independently in that job, 21 of 21 positive pairs each time,
and the real 64-byte request/reply gave p99 `51.546 µs` against a denominator
p99 of `7.254 µs` — `7.105872622001654x`, inside both the `8x` relative limit
and the `200 µs` absolute one. The section 2 requirement that a backend become a
P2 conformance observer only through a versioned repository gate is therefore
met: the observer is qualified, and it has been requalified in every job since.

**The IPC verdict did not survive its next run, and the record says so rather
than keeping the better number.** On commit
`2a7ca2033ce7e1d55f50c03cf6ce1ad6b1096dcf` — which retained the green evidence
and changed no source file — run 32734827424 measured the same four
byte-identical artifacts at p99 `44.406 µs` against a denominator p99 of
`5.519 µs`, or `8.046022830222865x`, and went red by `0.254 µs`. The absolute
bound was met in both. The two records are
`docs/evidence/STAGE3_IPC_LATENCY_P2.md` and
`docs/evidence/STAGE3_IPC_LATENCY_P2_RED.md`.

So the relative budget of section 5 is **open, not met**: its ratio straddles
`8x` on the reference platform, and a single passing draw does not close it.
Both runs put the miss in the same place — one preemption tail, at a percentile
where the denominator by section 3 has no tails at all. Section 6 governs what
may be done about that, and it is not this ADR that decides it.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. I-02, I-09, I-11,
  I-13, I-18 and I-19 are served by keeping the observer external, versioned and
  unable to substitute for production behavior.
- **Canonical representation:** unchanged. The benchmark remains canonical TOS
  Core text in the measured capsule; traces and reports are derived evidence.
- **Trusted-base impact:** none in production. The measurement QEMU is an
  identified host test instrument; measurement-only TSS permission is absent
  from the production nucleus.
- **Source-to-runtime impact:** unchanged. The benchmark follows reader, parser,
  checker, lowerer, independent verifier and the production engine path.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** supplies the measurement method for ADR-0048's Stage
  3 performance evidence; it closes no metric by itself.
- **Threat-model impact:** no new production boundary. A test build deliberately
  exposes COM1 to its observed CPL3 process and must prove confinement to those
  ports and production-artifact separation. The host observer rejects malformed
  trace records and terminates if its physical thread clock fails.
- **Performance contract:** both `<= 8x` and `<= 200 microseconds` remain Stage 3
  requirements. Denominator, workload and sample discipline are unchanged.
- **Compatibility profile:** ADR-0040 machine/cpu/vCPU/memory/TCG profile is
  unchanged. The observer backend and QEMU build/configuration are additional
  required report identities and change only through a versioned decision.
- **Dependencies/licence/provenance:** no dependency enters TOS. The local-only
  QEMU test bundle records its official source-archive digest, configuration,
  compiler, dynamic inputs, engine digest and the before/after hashes of both
  modified source files. The observer build/patch script is MIT-licensed so its
  injected code is compatible with GPL-2.0-only QEMU and MIT `serial.c`. QEMU
  remains under its upstream licences and is not vendored or distributed by
  this repository.
- **Patent impact:** no new runtime mechanism and no new identified high-risk
  claim combination. This ADR makes no patent-clearance claim.
- **Tests:** observer pairing self-tests; production/test artifact hash parity;
  TSS bitmap layout and port confinement; 3+21 raw floor and denominator series;
  observer identity and dropped-event refusal; eventual 64-byte IPC P1/P2 gate.

## Alternatives considered

**Calibrate the production APIC tick into microseconds.** Rejected: it adds a
time contract and dependency solely for measurement and contradicts the narrow
Stage 3 time surface.

**Move the quantitative budgets to Stage 7.** Rejected: `docs/35` assigns them
to Stage 3, and an external instrument can measure them without giving the
system trusted time.

**Use host serial-reader arrival timestamps.** Rejected by measurement: a
reader scheduled late for `OPEN` while the guest is already working understates
the interval and can turn observer delay into an apparent pass.

**Subtract a measured marker floor.** Rejected: the two intervals are not one
sample split into known additive components, and subtraction would make a
passing number depend on a correction model.

**Batch calls or exchanges.** Rejected: this contract bounds single-operation
latency, not amortized throughput.

**Use unmodified simple-trace timestamps.** Rejected by repeated measurement:
shared-host descheduling and asymmetric UART/trace work produced overlapping
tails. Keeping a lucky series would be result selection.

**Use QEMU thread CPU time for every simple event.** Rejected by repeated
measurement: it removed host descheduling but left asymmetric marker transport
and trace-record work inside the interval.
