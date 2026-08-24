<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Performance contracts

## Purpose

TOS does not promise performance by adjective. The architecture must be measurable early enough that a beautiful source model cannot hide an unusable execution path.

Performance contracts do not permit replacing TOS with a conventional implementation. Reference implementations may serve as benchmark oracles only under ADR-0011.

## Measurement rules

Every reported result includes:

- exact source commit;
- architecture and stage;
- QEMU and firmware versions;
- host CPU and virtualization mode;
- guest CPU count and memory;
- workload definition and data size;
- warm-up policy;
- sample count;
- median, p95 and p99 where latency applies;
- throughput and CPU utilization where applicable;
- comparison baseline source and build identity;
- profiler/trace artifact where feasible.

A number without its environment is not a conformance result.

## Budget classes

### Hard architectural budgets

These are topology/count constraints. Exceeding one requires an ADR because it indicates an architectural path change.

### Reference-platform budgets

These are quantitative thresholds on the documented QEMU reference platform. They may be revised with measurements and an ADR, but cannot be silently weakened to close a stage.

### Observational metrics

These are tracked from first implementation without a pass/fail threshold until evidence is sufficient.

## Stage 1 — Boot and capsule

Hard budgets:

- capsule parsing is single-pass or bounded multi-pass;
- no recursion dependent on untrusted capsule depth;
- parser performs no allocation proportional to attacker-declared count before validating total bounds;
- lookup of one canonical path does not require copying every payload.

Reference-platform evidence and conformance:

- the mandatory q35/qemu64/one-vCPU/256-MiB/TCG functional profile runs the
  exact ordinary production boot path for a capsule fixture containing 1,000
  files and exactly 16 MiB total payload. It retains raw 3-warmup/21-sample
  median/p95/p99 wall-clock data, serial/event logs and segment decomposition;
  its wall-clock latency is a retained regression metric, not a physical-CPU
  absolute-latency assertion;
- a declared native release/reference profile records the same exact two fresh
  validations and canonical `/system/boot/init.tos` lookup, including raw
  3-warmup/21-sample median/p95/p99 data and environment/build identities;
- each profile also measures the unavoidable SHA-256 baseline with the same
  fixture/source/provenance identity: two parser whole-capsule traversals, two
  loader/nucleus BootInfo-mirror whole-capsule traversals, two cumulative
  per-file traversals, two detached-identity traversals where applicable and
  the post-lookup boot-text digest. No result may be cached or shared between
  logical validators; and
- on the mandatory qemu64/TCG profile,
  full-exact-validation-p95 / unavoidable-crypto-p95 is no more than 1.30.
  This relative gate constrains validation-architecture overhead without
  weakening the required validations or hard architectural budgets.

The former 250 ms threshold was an empirically falsified initial reference
estimate. ADR-0026 records the measurements and rationale; the absolute native
and TCG series remain retained regression evidence.

## Stage 1.5–2 — Language frontend and runtime

Hard budgets:

- parsing and lowering have explicit source-size, nesting, identifier and diagnostic quotas;
- bootstrap-profile execution has instruction/fuel or equivalent preemption accounting;
- cache validation is bounded by declared dependency closure;
- source maps are retained without requiring an unbounded in-memory duplicate of source.

Reference-platform budgets for the bootstrap profile:

- parse, type-check, lower and verify a 256 KiB canonical module in no more than 1500 ms p95 (ADR-0045; the original 500 ms was a research estimate written before a working Stage 2 implementation existed, and was empirically falsified once the six general implementation defects it uncovered had been fixed);
- execute the standard one-million-operation integer/control-flow benchmark in no more than 22 times the host reference interpreter time under the same semantic implementation (ADR-0043; the original 10 was a research assumption, and a component decomposition of the production engine measured every semantic component of that workload at 15.1–17.9x on the accepted ADR-0040 platform);
- reject quota-exceeding source within 2 times the accepted-input budget rather than degrading without bound.

These are initial research gates, not claims of application-language competitiveness.

## Stage 3 — IPC and capabilities

### Measurement clock

ADR-0066 fixes the distinction between system time and measurement time. Stage
3 does not calibrate its production monotonic tick into a duration unit and
does not add a wall clock or timing capability for this gate. Both quantitative
IPC budgets below are measured by one external observer on the ADR-0040 QEMU
profile. The observer's exact backend/build identity, timestamp point, clock and
dropped-event behavior are retained with the report.

The same observer measures its empty floor, the fixed in-process denominator
and the IPC numerator. No observer cost is subtracted. Three warm-ups and 21
individual samples are required for each series; batching N operations and
dividing by N is a throughput average and is not a latency sample. Missing,
duplicate, overlapping or mismatched markers, reversed/zero/negative intervals,
a wrong sample count or a dropped trace event invalidates the whole series.

ADR-0066's conformance profile uses the one TCG vCPU thread's physical
`CLOCK_THREAD_CPUTIME_ID`. It captures one raw timestamp after QEMU handles
`OPEN` and one before it handles `CLOSE`, then emits the pair. Marker transport,
trace construction and host descheduling are outside the interval; no quantity
is subtracted. Trace collection is enabled only for the 3+21 sample window.

The floor and denominator disable timer preemption in their measurement-only
nucleus; the IPC numerator keeps it active. This makes the denominator smaller,
not easier. The exact measured artifact hashes and Cargo features bind that
scheduler state; a caller-supplied label is not evidence.

Observer resolution is proved in one prepared boot by 21 predeclared adjacent
blocks after three warm-up blocks. Each block contains one floor and one call
with a common sequence and distinct echoed work bit; block order alternates
`floor/call`, then `call/floor`. At least 19 of 21 `call - floor` differences
must be positive, the one-sided exact sign-test threshold `p <= 0.000111` (`232
/ 2^21` at 19). Missing, duplicated or out-of-plan tags invalidate the series;
non-positive differences remain in it and count against qualification. Every
tail still determines its series' nearest-rank p99 and nothing is reordered,
filtered or subtracted. QEMU `-icount` is virtual instruction time and is not
an admissible physical-duration clock.

Hard budgets for steady-state small-message IPC after initialization:

- no dynamic allocation in the nucleus fast path;
- no more than two payload copies for an inline message;
- large payload transfer uses shared regions rather than copying payload through the nucleus;
- one request/reply exchange requires no more than four user/kernel boundary crossings excluding scheduler preemption;
- capability validation is constant-time with respect to the process's total capability count, or the alternative bound is documented and tested.

Reference-platform budget:

- p99 request/reply latency for a 64-byte message between two runnable processes is no more than 200 microseconds on the declared QEMU CI profile.

The latency numerator uses the real endpoint path. After one unmeasured
64-byte exchange primes a server already cycling through atomic
`endpoint_reply_receive`, each of 3 warm-up and 300 retained intervals brackets
exactly one client `endpoint_call` and its 64-byte reply. Timer preemption stays
active and any interrupt tail remains in the sample. The retained nearest-rank
numerator p99 must satisfy `numerator_p99 <= 200 µs`; no successful retry may
replace a failed series.

The series is 300 samples because the nearest-rank p99 of 21 is the maximum of
21, which sits at the `21/22` quantile in expectation and is below the true p99
four times in five. At 300 the p99 is rank 297 — an interior order statistic at
`297/301` — and the distribution-free interval `X_(290)` to `X_(300)` covers it
with 95.07% confidence. ADR-0068 records the arithmetic.

**A relative bound is deliberately absent, and its absence is a decision rather
than an omission.** ADR-0068 removed `numerator_p99 <= 8 * denominator_p99` from
the Stage 3 conformance budgets after measuring that no measurement profile
available on this platform yields a ratio interpretable as intrinsic IPC
overhead: a timer interrupt landing inside one interval dominates the percentile,
and neither exposing both sides to it nor exposing neither produces a comparable
pair. It was withdrawn rather than widened, and the denominator was not
redefined to make a quotient pass.

The in-process function-call benchmark is retained, measured and reported beside
the IPC series as **observational and regression data**, and so is the ratio it
forms. Neither carries a threshold, closes a Stage 3 evidence item or fails a
run. It is fixed by `source/interfaces/system/IPC_V1.md` section 8, and was
fixed there before any IPC measurement existed: a call to an exported TOS Core
function taking one 64-byte value parameter and returning `unit`, executed by
the same engine build, in the same process, on the ADR-0040 reference platform.

For active-preemption measurements the reference platform's identity includes
the scheduler quantum and the APIC divider, because how often an interrupt lands
inside an interval is the interval divided by the tick period. A record that
does not bind both describes a platform it cannot name.

## Stage 4 — VirtIO block textual driver

Hard budgets after queue initialization:

- zero dynamic allocation per completed block request on the steady-state path;
- no more than one payload copy between client memory and device-visible memory; zero-copy is preferred where the DMA contract permits it;
- no more than four address-space/scheduler handoffs per unbatched request;
- one interrupt wakeup may complete a batch of requests; the implementation must not require one scheduling cycle per descriptor when batching is available;
- no global driver lock serializes independent queues in the long-term contract.

Reference baseline:

A minimal, separately isolated Rust VirtIO-block benchmark implementation may be built only as a host/reference oracle. It is not an accepted nucleus driver and cannot satisfy the TOS stage gate.

Stage 4 reference-platform budgets:

- sequential throughput is at least 35% of the reference baseline for the same queue depth and image;
- random 4 KiB p99 latency is no more than 5 times the reference baseline;
- CPU time per MiB is no more than 8 times the reference baseline;
- performance results include textual-runtime engine identity and cache state.

Failure to meet a target does not justify hiding the driver in the nucleus. It triggers profiling, execution-engine work or an explicit architecture review.

## Stage 5 — Repository and activation

Hard budgets:

- mounting a commit tree does not require eager checkout of all blobs;
- lookup cost depends on path depth and object/index access, not total repository size;
- protected-ref activation uses a bounded transactional record and does not rewrite the system tree;
- rollback does not copy all system files;
- garbage collection cannot scan or mutate the live namespace while holding an unbounded global stop-the-world lock.

Reference fixtures:

- 100,000 paths, 20,000 commits and a 10 GiB logical object set;
- deep but bounded trees;
- adversarially long histories and malformed object graphs within parser quotas.

Reference-platform budgets:

- resolve and expose a selected commit root within 2 seconds p95 when required indexes are warm and within 10 seconds cold;
- switch candidate boot metadata in under 100 ms excluding health checks;
- `status` over a 10,000-file overlay completes within 3 seconds p95;
- failed activation returns to last-known-good without work proportional to total `/system` bytes.

## Stage 7 — Network

Before implementation, Stage 7 adds explicit budgets for packet copies, context crossings, throughput, p99 latency, interrupt moderation and memory pressure. “Line rate” is not a valid requirement without link speed, packet size and CPU budget.

## Regression policy

CI retains benchmark history. A regression above 15% in a hard-gated metric requires explanation; above 30% blocks a stage/release unless an ADR changes the contract.

Debug builds are never compared to release baselines. Benchmark fixtures and parsers are versioned.

## Reporting status

Each performance claim is labelled:

- **P0 unmeasured design**;
- **P1 locally measured**;
- **P2 reproducible CI measurement**;
- **P3 independently reproduced**.

No stage closes on P0 for a metric assigned to that stage.
