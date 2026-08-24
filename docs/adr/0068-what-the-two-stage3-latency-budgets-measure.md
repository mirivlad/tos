<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0068: Which of the two Stage 3 latency budgets is a conformance budget

- Status: **Proposed**
- Date: 2026-08-25
- Decision level: 2 — it removes a quantitative bound from the Stage 3
  conformance set, fixes the size of a retained latency series, and adds two
  required identities to the reference platform. It changes no other threshold,
  no workload, no observer, and no invariant
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Amends, if accepted: `docs/35` Stage 3 latency line, `IPC_V1` section 8's
  relative bound, ADR-0066 sections 3 and 5
- Evidence: `docs/evidence/STAGE3_IPC_LATENCY_P2.md`,
  `docs/evidence/STAGE3_IPC_LATENCY_P2_RED.md`,
  `docs/evidence/STAGE3_PREEMPTION_TAIL_P1.md`

## The gap, stated once

`docs/35` and `IPC_V1` section 8 give Stage 3 two quantitative latency budgets:
the p99 of one 64-byte request/reply is to be at most `8x` a fixed in-process
call and at most `200 µs`. ADR-0066 fixed how both are measured: one external
observer, the numerator with timer preemption active, the denominator with it
inactive — the latter called conservative, on the reasoning that removing timer
excursions can only shrink the denominator and tighten the ratio.

Two CI runs of byte-identical artifacts then returned different verdicts:
`7.105872622001654x` green on `78447b3`, `8.046022830222865x` red on `2a7ca20`,
with the absolute bound met by a factor of four in both.

The cause is measured, not surmised. `STAGE3_PREEMPTION_TAIL_P1.md` establishes
that the numerator's p99 is one timer interrupt landing inside the measured
interval; that it is not the scheduler and not the address-space switch, costing
as much where a context switch is impossible as where it is possible; that its
arrival rate follows the tick rate exactly, at a few per cent per sample under
the reference quantum; and that a nearest-rank p99 over 21 samples is the
maximum of the series, so the ticked sample **is** the reported p99 whenever one
lands.

The reasoning that called the asymmetry conservative was therefore wrong in a
specific way. Excluding the timer from the denominator does not merely shrink
it. It removes from one side of a ratio the single largest term on the other.

## What the accepted documents already constrain

- ADR-0066 section 5: raw samples, median and **nearest-rank p99** retained;
  three warm-ups and 21 individual measurements; the IPC verdict uses the
  numerator's nearest-rank p99 with no retry selection; a timer tail belongs to
  the active-preemption numerator and is neither removed nor relabelled.
- ADR-0066 section 6: a threshold miss authorizes no change to clock, workload,
  denominator or budget **inside the result that measured it**. This is a
  separate decision, taken after that result was retained, red and unaltered.
- ADR-0066 section 3: the build refuses to combine the no-preemption measurement
  feature with the two-process request/reply profiles.
- ADR-0049: the Stage 3 time model is an uncalibrated tick for preemption and
  bounded timeouts. The quantum is implementation configuration.
- ADR-0040: the reference platform is q35/qemu64/one vCPU/256 MiB/TCG, changed
  only through a versioned decision.
- `IPC_V1` section 8's counted half — no dynamic allocation on the fast path, at
  most two payload copies per message, four crossings per exchange, a
  shared-region path that does not copy, constant-time capability validation.

## Decision

### 1. The relative `<= 8x` bound leaves the Stage 3 conformance set

It is **removed from the pass/fail budgets of Stage 3**. It is not replaced by
another coefficient, a wider one, a percentile-adjusted one or a
platform-specific one. The denominator is not redefined, retimed or reprofiled
to produce a passing quotient: a bound that cannot be measured honestly is
withdrawn rather than tuned until it agrees.

### 2. Why: no profile on this platform yields an interpretable ratio

Three profiles are possible and all three have now been measured.

**Asymmetric — numerator active, denominator inactive.** The structure that
produced the green and red records. The verdict is decided by whether a tick
lands inside one of 21 intervals: at a measured per-sample rate near 3%, that is
a `47.8%` chance per series. Two runs of byte-identical artifacts, one green and
one red, are the demonstration, and both tails were the same size to within half
a microsecond — `+13.475 µs` and `+13.961 µs` over their medians. What differed
was the denominator, not the numerator's tail.

**Active on both sides.** The hit probability is `interval / period`, and the
two sides differ about sixfold in duration, so they cannot be equally exposed.
From the red run's own numbers, with its own `+13.961 µs` tail:

| Outcome | Probability in a 21-sample series | Ratio |
|---|---:|---:|
| Neither side hit | 47% | 5.89x |
| Numerator hit | 43% | 8.05x |
| **Denominator hit** | 5% | **1.74x** |
| Both hit | 5% | 2.37x |

The third row disqualifies it: a rare interrupt in the *comparator* would rescue
the budget by inflating what the numerator is divided by. The measurement would
reward instrument noise, and reward it most when the noise is worst.

**Inactive on both sides.** This was the candidate this ADR carried in its first
form. Section 6 of `STAGE3_PREEMPTION_TAIL_P1.md` measured it with one nucleus
and one runtime image, byte for byte, booted in three modes selected from a
capsule unit before any process exists, with the three capsules equal in size.
Against ACTIVE's tick-free samples, NO-TIMER is `+16.62 µs` slower (12 of 15
iterations) and MASKED `+18.45 µs` slower (14 of 15); MASKED and NO-TIMER are
indistinguishable, so the cause is interrupt **delivery** rather than the APIC
being programmed. A no-preemption numerator would therefore remove a `~14 µs`
interrupt tail and add a `~17 µs` artifact of the emulator — and the fixed-call
workload showed no such median effect in the two-build comparison, so the two
sides would not even be inflated together.

**Conclusion.** On the ADR-0040 TCG profile there is no measurement profile in
which this ratio can honestly be read as the intrinsic overhead of crossing an
isolation boundary. The quantity the bound was written to constrain is real; the
platform this stage is measured on cannot express it. The question is deferred
to a platform that can, and this ADR promises no stage for that.

### 3. The benchmark and the ratio survive as observation

The fixed in-process call benchmark of `IPC_V1` section 8 is kept, measured and
retained, and the ratio is still computed and reported beside both series. It
becomes **observational and regression data**: a large movement in either series
between commits is a signal worth investigating.

It is not a gate. It closes no Stage 3 evidence item and blocks none, it carries
no threshold, and no run is red because of it. A report that presents it must
say which of those two things it is.

### 4. The absolute `p99 <= 200 µs` remains the conformance budget

Measured on the real 64-byte request/reply between two runnable processes, with
**production preemption active**, on the production IPC path under
measurement-only endowments. Its tails are part of it: what a client waits for
includes the interrupt that arrived while it waited. Nothing about this budget's
threshold, workload or discipline changes.

This is the budget that says the thing a latency contract exists to say, and it
is the one that survives the platform honestly.

### 5. A latency series is 300 retained samples

After the unchanged three warm-ups, for the absolute numerator and for the
observational benchmark alike.

Nearest rank at `n = 21` puts the p99 at rank `ceil(0.99 x 21) = 21`: **the p99
is the maximum of the series**. That estimator sits at the `n/(n+1)` quantile in
expectation — `21/22 = 95.45%`, not 99% — and `P(max of 21 < true p99) =
0.99^21 = 81%`, so it names a quantity it is usually below.

At `n = 300` the p99 is rank 297, an interior order statistic whose expected
level is `297/301 = 98.7%`. A distribution-free interval can then be stated
instead of a point that looks exact; its coverage is exactly
`P(X_(r) <= xi_p <= X_(s)) = sum_{k=r}^{s-1} C(n,k) p^k (1-p)^(n-k)`, which at
`n = 300`, `p = 0.99` gives **`X_(290)` to `X_(300)` = 95.07%**. It is `290` and
not something nearer 297: `X_(294)` to `X_(300)` covers `91.82%` and `X_(297)`
to `X_(300)` only `59.82%`. A normal approximation around rank 297 suggests a
far narrower interval and is wrong here, because the binomial is skewed at
`p = 0.99` and the interval's upper end is truncated at the maximum.

The length also settles the surviving budget's stability. At a per-sample hit
rate near 3%, a 300-sample series contains at least one ticked interval with
probability `0.9999`: the absolute p99 reliably *includes* the tail instead of
sampling it by luck. Measured tails of `44–52 µs` sit inside `200 µs` by
roughly a factor of four, so making the tail certain does not endanger the
budget — it makes the verdict deterministic.

Cost is not an obstacle: one sample is one host round trip, of order
milliseconds. The marker protocol's four-bit sequence identity wraps every 16
blocks, which is admissible only because the decoder verifies a **predeclared
exact tag plan**; the plan for a 300-sample series states the wrap, and a
duplicate the plan did not predict still invalidates the run. Sampling
discipline is otherwise unchanged: raw samples retained, nothing filtered,
reordered, batched or subtracted, no retry selection.

**The observer qualification is not touched.** Its three warm-ups and 21
adjacent floor/call pairs, its exact sign test and its `p <= 0.000111`
threshold stay exactly as ADR-0066 accepted them. That experiment asks whether
the instrument can resolve the denominator against its own floor; it is not a
latency series and its size is not this ADR's business.

### 6. The quantum and the divider become platform identity

How often an interrupt lands is `interval / period`, and the period is set by
the APIC divider and the scheduler quantum — `100_000` bus ticks divided by 16
today. A platform that changes either changes the tail exposure of every
absolute measurement taken under it without changing a line of the measured
path.

Both therefore become **required identities of the ADR-0040 reference platform
for active-preemption measurements**, recorded in every report — as
`quantum_count` already is — and **bound by the qualifier** rather than merely
printed. Changing either is then a versioned platform change that invalidates
comparison with earlier records, exactly as changing the machine type would be.

This asserts no calibration. The tick remains the uncalibrated counter ADR-0049
describes; what is fixed is that records being compared were taken under the
same one.

### 7. The counted budgets do not move

`IPC_V1` section 8's hard bounds — no dynamic allocation on the nucleus fast
path, at most two payload copies per message, four crossings per exchange, the
shared-region path that does not copy, constant-time capability validation —
are unchanged, still conformance, still gated. They are the half of section 8
that does not depend on a clock, and nothing in this ADR touches them.

### 8. The retained `8x` records stay what they are

`STAGE3_IPC_LATENCY_P2.md` and `STAGE3_IPC_LATENCY_P2_RED.md` remain as taken:
historical evidence of measurements made under the old structure. **Nothing is
renamed to a pass, reclassified, or quietly superseded.** The red run stays red.
Both records gain, when this ADR is accepted, a pointer saying which structure
they were taken under; their numbers, verdicts and wording do not change.

## What this ADR does not decide

It does not change `200 µs`, the workload, the observer, the benchmark's
definition, the counted budgets, or the observer-qualification experiment. It
does not claim Stage 3 meets the surviving budget: the absolute bound was met in
both retained runs, but a conformance verdict under the series length of section
5 has not been taken. It does not touch `docs/35`, `IPC_V1` or the gate, which
change when and if it is accepted, in the same change as the implementation.

## What replaces the removed bound

Nothing pretends to, and the question is what still guards the IPC path:

- the absolute budget, which is the client-visible promise;
- the counted budgets of section 7, which bound the mechanism without a clock
  and are the reason a regression in copies or crossings cannot hide behind a
  fast machine;
- the observational ratio of section 3, which makes a large movement visible
  without deciding anything;
- and the exchange-cost gate, which already proves four crossings per exchange
  by counting rather than timing.

A stage that measures what it can measure and says so is in a better position
than one carrying a bound whose verdict is a coin flip.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended.
- **Canonical representation:** unchanged. The benchmark and the workload remain
  canonical TOS Core text through the same engine.
- **Trusted-base impact:** none. No measurement build becomes production, and
  section 1 removes a gate rather than adding a mechanism.
- **Source-to-runtime impact:** unchanged.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** the Stage 3 performance evidence keeps one
  quantitative latency budget and its counted budgets. Removing a bound that
  could not be measured honestly does not close any evidence item, and the
  identity gates are unchanged.
- **Threat-model impact:** none new. Resource-exhaustion evidence rests on the
  counted budgets and the absolute latency figure, both retained.
- **Performance contract:** this changes the contract's composition. One bound
  leaves the conformance set and becomes observational; one series length
  changes; no surviving threshold moves. Records taken under the old structure
  stay valid as records of what they measured and are not comparable, for the
  removed bound, with anything taken later.
- **Compatibility profile:** ADR-0040's machine profile is unchanged and gains
  two qualifier-bound identity fields for active-preemption measurements.
- **Dependencies, licence, patents:** none.
- **Tests the decision requires:** the qualifier computes and retains the ratio
  but cannot fail a run on it; it fails a run whose numerator manifest does not
  prove preemption active; a series of the wrong length is refused; a wrapping
  tag plan is verified exactly; the quantum and divider are bound rather than
  printed, and a run whose platform reports different ones is refused.

## Alternatives considered

**Keep the bound and re-run until green.** Rejected: that is retry selection,
which ADR-0066 section 5 forbids, and the measured `47.8%` per-series hit rate
makes it selection with a known bias rather than measurement.

**Keep the bound and widen the coefficient.** Rejected: the defect is not that
`8` is too small. A wider coefficient inherits the same stochastic verdict, and
choosing its width from the failing measurements is fitting a threshold to a
result.

**Keep the bound and measure both sides under one profile.** Rejected in section
2 on the arithmetic and the measurements there, for both available profiles.

**Subtract a measured interrupt cost from the numerator.** Rejected: ADR-0066
section 4 forbids subtracting any excursion or observer cost, and a subtracted
tail is an estimate presented as a measurement.

**Raise the quantum so interrupts are rarer.** Rejected: it changes the system's
scheduling behaviour to move a measurement. The quantum is chosen for scheduling
reasons and then recorded, which is what section 6 makes binding.

**Defer the whole question to a non-TCG reference platform.** Not chosen,
because it would leave a bound in force that no run can honestly satisfy, while
the platform that could measure it does not exist in this stage. Section 2
defers the *question*; leaving the bound in place would defer nothing and fail
runs meanwhile.

## Consequences

Stage 3 keeps one latency budget that means what it says, one benchmark that
informs without judging, and a set of counted budgets that constrain the
mechanism without a clock. The p99 becomes an estimate of a p99 rather than of
the 95th percentile, and the surviving verdict stops being a coin flip.

What is lost is stated plainly: there is no longer a Stage 3 bound on how much
more an isolated exchange costs than an in-process call. That question is open,
and this ADR answers it with a measurement showing it cannot be answered here
rather than with a number that could not be trusted.
