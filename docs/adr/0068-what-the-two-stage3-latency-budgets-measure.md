<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0068: What each of the two Stage 3 latency budgets is measuring

- Status: **Proposed**
- Date: 2026-08-24
- Decision level: 2 — it fixes which measurement profile each of the two
  quantitative IPC budgets is taken under, the size of a retained latency
  series, and one more required identity of the reference platform. It changes
  no threshold, no workload, no observer and no invariant
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Depends on: ADR-0040 (reference platform), ADR-0049 (time model), ADR-0066
  (external observer); amends ADR-0066 sections 3 and 5 if accepted

## The gap, stated once

`docs/35` and `IPC_V1` section 8 give Stage 3 two quantitative latency budgets:
p99 of one 64-byte request/reply is to be at most `8x` a fixed in-process call
and at most `200 µs`. ADR-0066 fixed how both are measured: one external
observer, the numerator with timer preemption active, the denominator with it
inactive — the latter called conservative, because removing timer excursions can
only make the denominator smaller and the ratio stricter.

Two CI runs of byte-identical artifacts then returned different verdicts:
`7.106x` green on `78447b3`, `8.046x` red on `2a7ca20`, with the absolute bound
met by a factor of four in both. The measured cause is retained in
[STAGE3_PREEMPTION_TAIL_P1.md](../evidence/STAGE3_PREEMPTION_TAIL_P1.md): one
timer interrupt landing inside a measured interval costs the same order as the
interval itself, it is not the scheduler and not the address-space switch, and
its arrival rate is `interval / tick period` — a few per cent per sample at the
reference quantum.

The word "conservative" is therefore not describing what happens. Excluding the
timer from the denominator does not merely shrink it; it removes from one side
of a ratio the single largest term on the other side. **The relative budget, as
structured, compares an exchange that usually contains one timer interrupt
against a call that cannot contain one, and reports the comparison at a
percentile which, over 21 samples, is the maximum of the series.** Whether a run
passes is then decided by whether a tick landed.

That is not a threshold question. Both budgets are inside this ADR only as
things being measured; neither number is reopened.

## What the accepted documents already constrain

- ADR-0066 section 5: raw samples, median and **nearest-rank p99** retained; the
  IPC verdict uses the numerator's nearest-rank p99 without retry selection;
  three warm-ups and 21 individual measurements.
- ADR-0066 section 5, again: "a timer/preemption tail is part of the
  active-preemption numerator and is neither removed nor relabelled as observer
  cost". This ADR does not propose removing it. It proposes saying which budget
  it belongs to.
- ADR-0066 section 6: a threshold miss authorizes no change to clock, workload,
  denominator or budget **inside the result that measured it**. This is a
  separate decision taken after that result was retained, red, unaltered.
- ADR-0066 section 3: the build refuses to combine the no-preemption measurement
  feature with the two-process/request-reply profiles.
- ADR-0049: the production time model is a tick for preemption and bounded
  timeouts, with no calibration. The quantum is implementation configuration.
- ADR-0040: the reference platform is q35/qemu64/one vCPU/256 MiB/TCG, changed
  only through a versioned decision.

## 1. The two budgets are not two thresholds on one measurement

They answer different questions, and the evidence makes the difference concrete:

- **`<= 200 µs` is a promise to a client.** What a caller waits for is the whole
  of it, timer interrupt included. This budget must keep production preemption
  active and must keep the tails.
- **`<= 8x` is a statement about the mechanism.** It exists to say that crossing
  an isolation boundary costs a bounded multiple of not crossing one — copies,
  crossings, capability validation, the scheduler decisions that blocking
  forces, two address spaces. An interrupt that would have arrived whatever the
  process was doing is not part of that multiple.

Read that way, the current structure measures the second question with the first
question's instrument on one side only.

## 2. The proposal

**The relative budget is measured with the same measurement profile on both
sides, and that profile is the no-preemption one.**

1. The denominator is unchanged: the fixed in-process TOS Core call of
   `IPC_V1` section 8, timer preemption inactive.
2. The relative numerator is the same real two-process 64-byte request/reply as
   today — the production path under measurement-only endowments, not a second
   implementation — measured with timer preemption inactive.
3. `numerator_p99_no_preemption <= 8 * denominator_p99` — the threshold
   unchanged.

**The absolute budget keeps production preemption and keeps its tails.**

4. The absolute numerator is the same exchange with timer preemption active,
   exactly as ADR-0066 measures it today, and `numerator_p99_active <= 200 µs`
   — the threshold unchanged.
5. Its tail is evidence, not noise: it is the only place either budget reports
   what a client actually waits for.

Consequences that belong in the decision rather than in the implementation:

6. ADR-0066 section 3's build guard **inverts for one of the two builds**: the
   relative numerator must prove preemption inactive, and the absolute numerator
   must prove it active. Both must remain proven from the build manifest, never
   declared by a caller, and the qualifier must refuse a run whose numerator
   profile does not match the budget it is being scored against.
7. A conformance run therefore takes three measurements rather than two: the
   paired floor/denominator, the no-preemption numerator, and the
   active-preemption numerator. Both numerators run the identical workload and
   are reported side by side, so the difference between them **is** the
   platform's preemption exposure, published rather than argued.

### A precondition this ADR does not hide, and what measuring it found

The counterfactual has been measured, and it is not simply "the same path minus
interrupts". Paired across an interleaved matrix, the inactive-preemption IPC
numerator was **slower** than the active one by `+7.23 µs` at the median,
positive in 9 of 12 iterations.

Those two series came from different builds, so compile-time layout was a live
explanation, and it has since been eliminated. Section 6 of
[STAGE3_PREEMPTION_TAIL_P1.md](../evidence/STAGE3_PREEMPTION_TAIL_P1.md) boots
**one** nucleus and **one** runtime image, byte for byte, in three modes chosen
at boot from a capsule unit before any process exists, with the three capsules
equal in size:

- against ACTIVE's tick-free samples, NO-TIMER is `+16.62 µs` slower (12 of 15
  iterations) and MASKED `+18.45 µs` slower (14 of 15);
- MASKED and NO-TIMER are indistinguishable (`−7.43 µs`, 5 of 15), so the cause
  is interrupt **delivery**, not the APIC being programmed or the timer
  counting;
- all three modes report the contract's counters identically.

**This runs against section 2 rather than for it.** Measuring the relative
budget under a no-preemption profile would remove a `~14 µs` interrupt tail from
the numerator and add a `~17 µs` platform effect to it, on this host — and the
denominator's workload showed no such effect at the median, so the two sides
would not even be inflated together. A ratio built that way would not be
measuring intrinsic IPC overhead; it would be measuring intrinsic overhead plus
an unexplained artifact of the emulator, and calling the sum a property of the
system.

The effect size on the ADR-0040 reference runner is **unknown**: everything
above is one developer machine. So the precondition stands and is now sharper
than "explain the difference". Before section 2 could be adopted, the delivery
effect has to be characterised on the reference platform, and by a route that is
not the diagnostic patch — which exists to remove conformance protections and
must never run a gate. If it turns out to be of the same order there, section 2
is not adoptable as written, and the alternative of dropping the ratio becomes
the serious one.

## 3. Why "make both sides preemptible" is rejected

It is the obvious symmetry and it does not work, because the two sides have
different durations and the hit probability is `interval / period`. From the
retained red run, with a tick period of about a millisecond:

| | Per-sample hit probability | At least one hit in 21 |
|---|---:|---:|
| Numerator (median `30.4 µs`) | 3.04% | **47.8%** |
| Denominator (median `4.74 µs`) | 0.47% | 9.5% |

A run's verdict would then be drawn from a four-way lottery. Using that run's
own numbers and its own `+13.961 µs` tail:

| Outcome | Ratio | Verdict |
|---|---:|---|
| Neither side hit | 5.89x | pass |
| Numerator hit | **8.05x** | fail — this is what happened |
| **Denominator hit** | **1.74x** | pass |
| Both hit | 2.37x | pass |

The third row is the disqualifying one. Making the denominator preemptible
means a rare interrupt in the *comparator* rescues the budget by inflating the
thing the numerator is divided by — the measurement would reward instrument
noise, and reward it most when it is worst. Symmetry of exposure is not achieved
by making both sides eligible for an event whose probability differs five-fold
between them. It is achieved by excluding it from both, which is what section 2
proposes, or by not using a ratio at all, which is a larger decision than this.

## 4. Twenty-one samples cannot carry a p99

This is a separate defect and would remain after section 2 is decided either
way.

Nearest rank at `n = 21` puts the p99 at rank `ceil(0.99 x 21) = 21`: **the p99
is the maximum of the series**. That estimator has three properties worth
stating as arithmetic rather than as unease:

- the maximum of `n` samples sits at the `n/(n+1)` quantile in expectation —
  `21/22 = 95.45%`, not 99%;
- `P(max of 21 < true p99) = 0.99^21 = 81%`, so the reported figure is **below**
  the quantity it names four times in five;
- with a measured hit rate near 3%, the top 1% of the numerator's distribution
  is entirely composed of ticked samples, so the true p99 *is* a ticked value —
  and a 21-sample series contains one only 47.8% of the time.

The estimator is therefore biased low, bimodal, and — at this series length —
decided by a coin flip. Note what this implies for section 2: **a longer series
does not rescue the current structure, it makes its failure reliable.** At
`n = 300` the numerator contains at least one ticked sample with probability
`0.9999`.

Proposal: **the retained latency series is 300 individual measurements after the
unchanged three warm-ups**, for both numerators and the denominator.

- Nearest rank puts the p99 at rank 297 of 300, an interior order statistic
  whose expected level is `297/301 = 98.7%` rather than 95.5%.
- A distribution-free interval for the true p99 can then be stated instead of a
  point that looks exact. Its coverage is exactly
  `P(X_(r) <= xi_p <= X_(s)) = sum_{k=r}^{s-1} C(n,k) p^k (1-p)^(n-k)`, which at
  `n = 300`, `p = 0.99` gives **`X_(290)` to `X_(300)` = 95.07%**. It is `290`
  and not a rank closer to 297: `X_(294)` to `X_(300)` covers only `91.82%`, and
  `X_(297)` to `X_(300)` only `59.82%`. A normal approximation around rank 297
  suggests a much narrower interval and is wrong here, because the binomial is
  skewed at `p = 0.99` and the interval's upper end is truncated at the maximum
  rather than extending past it.
- Cost is not the obstacle: one sample is one host round trip, of order
  milliseconds, so a 300-sample series is under a second of guest time.
- The marker protocol's four-bit sequence identity wraps every 16 blocks. That
  is admissible only because the decoder already verifies a **predeclared exact
  tag plan**; the plan for a 300-sample series states the wrap, and a duplicate
  that the plan did not predict still invalidates the run.
- Sampling discipline is otherwise unchanged: raw samples retained, nothing
  filtered, reordered, batched or subtracted, no retry selection.

## 5. The quantum is part of the reference platform

The absolute budget keeps timer interrupts, and how often one lands is
`interval / period`, where the period is set by the APIC divider and the
scheduler quantum — `100_000` bus ticks divided by 16 today, an implementation
choice ADR-0049 deliberately left open. A platform that changes the quantum
changes the tail exposure of every absolute measurement ever taken under it,
without changing a line of the measured path.

Proposal: the scheduler quantum and the APIC divider become **required
identities of the ADR-0040 reference platform for active-preemption
measurements**, recorded in every report — as `quantum_count` already is — and
bound by the qualifier rather than merely printed. Changing either is then a
versioned platform change that invalidates comparison with earlier records,
exactly as changing the machine type would be.

This is not a claim that the tick is calibrated. It remains an uncalibrated
counter under ADR-0049; what is being fixed is that the same uncalibrated
counter is used across records being compared.

## What this ADR does not decide

It does not change `8x`, `200 µs`, the workload, the observer, the denominator's
definition, or the discipline of section 5 beyond series length. It does not
decide whether Stage 3 can meet either budget. It does not touch `docs/35`,
`IPC_V1` or the gate: those change when and if it is accepted, in the same
change as the implementation.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended.
- **Canonical representation:** unchanged. Both numerators run the same
  canonical text through the same engine.
- **Trusted-base impact:** none. Two measurement builds exist today; this makes
  the second one a required part of a conformance run rather than a forbidden
  combination.
- **Source-to-runtime impact:** unchanged.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** it fixes how the Stage 3 performance evidence is
  taken. It closes no gate and asserts no result.
- **Threat-model impact:** none new. A measurement build that disables
  preemption is already confined to test features and proven from a build
  manifest; the same proof now has to name which budget it belongs to, which is
  a strengthening.
- **Performance contract:** this is a change to the contract's *structure*. No
  threshold moves. Every retained record stays valid as a record of what it
  measured; records taken under the old structure are not comparable to records
  taken under the new one for the relative budget, and the evidence must say so.
- **Compatibility profile:** ADR-0040's machine profile is unchanged and gains
  two identity fields for active-preemption measurements.
- **Dependencies, licence, patents:** none.
- **Tests the decision requires:** the qualifier refuses a relative numerator
  whose manifest proves preemption active, and an absolute numerator whose
  manifest proves it inactive; a series of the wrong length is refused; a tag
  plan that wraps is verified exactly; both numerators are reported side by side
  from the same workload; the quantum and divider are bound rather than printed.

## Alternatives considered

**Leave the structure and re-run until green.** Rejected: it is retry selection,
which ADR-0066 section 5 forbids, and section 3 above shows the outcome is close
to a coin flip — so it is selection with a known bias rather than a measurement.

**Make both sides preemptible.** Rejected in section 3, on the arithmetic there.

**Subtract a measured interrupt cost from the numerator.** Rejected: ADR-0066
section 4 forbids subtracting any observer or excursion cost, and a subtracted
tail is an estimate presented as a measurement.

**Drop the relative budget and keep only `200 µs`.** Not proposed here, but the
measurement of section 2's precondition has moved weight towards it: a
no-preemption numerator is not a clean numerator on the one platform where this
has been measured. It is the honest alternative if the precondition cannot be
met: a ratio whose two sides cannot be measured under one profile is not a
measurement of anything stable. It would need its own decision and its own justification for
losing the mechanism-level statement the ratio exists to make.

**Raise the quantum so ticks are rarer.** Rejected as a measurement decision: it
changes the system's scheduling behaviour to move a measurement, which is
section 6 of ADR-0066 in substance if not in letter. The quantum should be
chosen for scheduling reasons and then recorded, which is what section 5
proposes.

## Consequences

Each budget states what it measures and is measured under the profile that makes
that statement true. The ratio stops being a lottery, at the cost of one more
measurement per conformance run and a precondition that must be explained first.
The absolute budget keeps the tail and becomes the place where the platform's
preemption exposure is reported, beside the same workload without it. And the
p99 becomes an estimate of a p99 rather than of the 95th percentile.

Nothing here says Stage 3 meets either budget. The last measured verdict is
still red, and it stays red until a measurement says otherwise under whatever
structure is accepted.
