<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 preemption tail — P1 diagnostic

Evidence level: **P1 diagnostic. Not conformance evidence, and it replaces
nothing.** The green [P2 record](STAGE3_IPC_LATENCY_P2.md) and the red
[P2 record](STAGE3_IPC_LATENCY_P2_RED.md) stand exactly as taken.

Questions asked: **what produces the single p99 tail of the Stage 3 IPC
numerator, what does it cost, and is it a removable cost of this implementation
or the expected cost of timer preemption on the ADR-0040 TCG profile?** And,
added afterwards: **does the slowdown of the inactive-preemption path survive
when the measured binaries are byte-for-byte identical?**

Answer measured: **it is one timer interrupt landing inside the measured
interval. It is not the scheduler and not the address-space switch. Its rate
follows the tick rate exactly, and at the reference quantum it lands in
roughly one interval of every twenty-one — which is precisely the sample whose
value the contract's nearest-rank p99 reports.**

## What was run, and why none of it can become evidence

Every series was taken in a throwaway `git worktree` at commit
`6f2837b76275c4ea5ab8f4d0491294d74543c687`, carrying
[stage3-preemption-tail-diagnostic.patch](stage3-preemption-tail-diagnostic.patch)
(SHA-256
`f8a67a4fc924421ca68c2d358cd52bf5d82ba394738872d7b210351662282a88`). That patch
removes conformance protections deliberately: it lifts the `compile_error` that
forbids an inactive-preemption IPC numerator, adds a second cfg-selected
scheduler quantum and a masked-delivery variant of `apic::start`, selects a
boot's timer mode from a capsule unit, admits both preemption cases of each
measurement mode in the harness, and reads the monotonic tick **outside both
markers** so a sample can be attributed to a tick. It adds no work between `OPEN` and `CLOSE`: the
measured path is the measured path.

It is retained as a diagnostic artifact and is not a proposed change. Nothing
it produces can reach a gate: `qualify-observer` refuses an active-preemption
denominator and `qualify-ipc` refuses an inactive-preemption numerator, both
fail-closed.

The instrument is the ADR-0066 observer, built by the repository's own
`build-simple-observer.sh` from the pinned archive
`22e410fe…`: launcher `39474e28…`, engine `90f836fd…`.

| | Workload | Preemption | Tick attribution |
|---|---|---|---|
| A | fixed TOS Core call | inactive | — |
| B | fixed TOS Core call | active | — |
| C | 64-byte IPC request/reply | active | — |
| D | 64-byte IPC request/reply | **inactive** | — |
| E | 64-byte IPC request/reply | active | per sample |
| F | 64-byte IPC request/reply | inactive | per sample |
| G | fixed TOS Core call | active | per sample |
| H | 64-byte IPC request/reply | active, quantum ÷10 | per sample |

A and C are the conformance denominator and numerator recipes. D is the
counterfactual the build guard exists to forbid; it performs the same work as C
— 24 measured exchanges, 25 served, 50 messages, 75 payload copies, balanced
`51/51` crossings — with `ticks=0`.

Configurations were **interleaved rather than blocked**: a block design would
let half an hour of host drift land entirely on one configuration.

**This host is not the reference platform and no absolute number here is offered
as a platform figure.** It is materially noisier than the CI runner: with the
timer disabled entirely, series F still produced excursions above `+118 µs`.
Every claim below is therefore a paired or within-run comparison.

## 1. The tail is a timer tick, and the p99 is the ticked sample

| Series | Quantum | Intervals containing a tick | The run's p99 was a ticked sample |
|---|---:|---:|---:|
| E (matrix 2) | 100 000 | 14/315 = 4.4% | 10 of 15 runs |
| E (matrix 4) | 100 000 | 7/252 = 2.8% | 6 of 12 runs |
| F | 100 000, timer off | **0/315** | 0 of 15 runs |
| H | 10 000 | 117/252 = **46.4%** | **12 of 12 runs** |

F is the control: with no timer there are no ticks to attribute, which is what
the attribution reports.

The rate is geometric — a tick lands inside an interval with probability
`interval / period`. Dividing the median interval by the observed rate gives an
implied tick period of `1.17 ms` and `1.84 ms` for the two reference series and
`0.11 ms` for the ÷10 series: a factor of ten in the constant, a factor of ten
in the period. **The tail count follows the tick count causally, not by
correlation.**

## 2. The same arithmetic explains both CI runs

The CI numerator's median interval is `30.4 µs` (red) and `38.1 µs` (green).
Against a period of about a millisecond, the chance that any one interval
contains a tick is a few per cent, so the expected number of ticked samples in a
retained series of 21 is well under one — and both CI runs show exactly one
sample standing apart from a tight body:

| CI run | Median | Second largest | p99 (the maximum) | Tail excess |
|---|---:|---:|---:|---:|
| green `78447b3` | 38.071 | 41.227 | 51.546 | **+13.475** |
| red `2a7ca20` | 30.445 | 32.488 | 44.406 | **+13.961** |

Two runs of byte-identical artifacts, two tails of the same size to within half
a microsecond. The verdict differed because the denominator differed, not
because the numerator's tail did.

## 3. It is not the scheduler and not the address-space switch

Series G runs the fixed call with preemption active and **one** runnable
process. `preempt()` then finds no other runnable slot, returns without copying
a frame and without reloading `CR3`: a tick there cannot switch context.

| Series | Runnable processes | Ticked samples | Median excess over the run's median |
|---|---:|---:|---:|
| G | 1 (no switch possible) | 4 | **+56.89 µs** |
| E (matrix 3) | 2 | 15 | **+43.60 µs** |

A tick costs as much where a switch is impossible as where it is possible — if
anything more. The table scan, the two frame copies and the `CR3` reload are
therefore not the cost. G's clean samples have a p99 excess of `+19.32 µs`,
against ticked samples starting at `+43.03 µs`: the two populations do not
overlap.

## 4. What the cost is made of, as far as this can be measured here

One device access under TCG can be priced from the retained CI floor, which is
exactly two COM1 writes: `1.373 µs` and `1.944 µs`, so `0.69–0.97 µs` per
device write. The handler's own body is a few dozen instructions, and its one
device access is the `EOI` write.

So of the CI tail's `13.5–14.0 µs`, about a microsecond is the guest's
interrupt handler including `EOI`, and the scheduler is measured at
approximately nothing. **The remainder is QEMU's own interrupt delivery under
TCG** — leaving the translation loop, injecting, and re-entering. That last step
is inferred by subtraction rather than measured: this host has no `perf` and
`perf_event_paranoid` is 3, so the vCPU thread could not be profiled. Splitting
it further needs host-side profiling on a machine that permits it.

## 5. Disabling preemption does not make IPC faster

Paired by iteration across the interleaved matrix, the inactive-preemption IPC
numerator was **slower** than the active one: `median(D − C) = +7.23 µs`,
positive in 9 of 12 iterations. On the fixed call there was no effect at the
median: `median(B − A) = −0.36 µs`.

This is recorded because it matters to any proposal that would measure the
relative budget with preemption inactive on both sides: **that path is not
merely the production path minus the interrupts.** The two series compared there
came from two different builds, so a difference in compile-time layout was a
live explanation. Section 6 removes it.

## 6. One artifact, three boot modes: the slowdown is not a build artifact

Section 5's comparison was between two builds. This one is between three boots
of **the same two files**:

- nucleus `0f4811c1…`, features `test-call-reply,test-measurement-port`;
- runtime image `3c670daf…`, feature `diag-tick-attribution`.

The timer mode is chosen at boot from a capsule unit `/system/diag/timer-mode`,
read before any process exists and long before the observed process says
`READY`. The three mode words are padded to one length, so the three capsules
differ in content and **not** in size — 3256 bytes each — which removes capsule
layout as an explanation:

| Mode | What the nucleus does | Capsule SHA-256 |
|---|---|---|
| ACTIVE | `apic::start()` | `890141b7…` |
| MASKED | `apic::start_masked()` — every write `start` makes, plus the LVT mask bit | `11bbb759…` |
| NO-TIMER | the APIC timer is not started | `8dcd79ce…` |

MASKED exists to separate two things the earlier matrix could not: the APIC
being programmed and counting, and interrupts being delivered. In MASKED the
APIC is enabled, the divider is set, the count runs and reloads and `IF` is set;
only the mask bit differs from ACTIVE.

Fifteen interleaved iterations of each. Every mode reported the contract's own
counters identically — `50` messages, `75` payload copies, `25` exchanges,
`51/51` crossings, `24/24/0` client answers — and the guest reported which mode
it booted in, with the per-sample tick deltas corroborating it: 14 ticked
intervals of 315 in ACTIVE, **0 of 315** in both MASKED and NO-TIMER.

Paired by iteration, against ACTIVE's tick-free samples only:

| Comparison | Median | Positive |
|---|---:|---:|
| MASKED − ACTIVE(clean) | **+18.45 µs** | 14/15 |
| NO-TIMER − MASKED | −7.43 µs | 5/15 |
| NO-TIMER − ACTIVE(clean) | **+16.62 µs** | 12/15 |

Pooled medians: ACTIVE clean `50.33 µs` (n=301), ACTIVE ticked `98.98 µs`
(n=14), MASKED `71.26 µs`, NO-TIMER `67.39 µs`.

Three things follow, and the first is the answer to the question this
experiment was run to settle.

1. **The systematic slowdown survives byte-identical artifacts.** It is not a
   compile-time feature or build-layout effect. Section 5 measured `+7.23 µs`
   between two builds; the same comparison between two boots of one build is
   `+16.62 µs`.
2. **It is caused by delivery, not by the APIC being programmed.** MASKED and
   NO-TIMER are indistinguishable — `−7.43 µs`, positive in 5 of 15, a coin
   flip — while both differ from ACTIVE. A timer that counts and reloads but
   delivers nothing behaves like no timer at all.
3. **Delivering periodic interrupts makes the interrupt-free intervals faster**,
   by roughly the size of the tail an interrupt adds when it does land. Nothing
   measured here explains why; it is a property of the guest's execution under
   this emulator, not of anything the measured path does differently — the path
   is identical machine code performing identical counted work.

The scheduler-dependent counters differ by one between some ACTIVE runs
(`returns`/`resumptions` of `89/53` against `90/52`), which is the second door
ADR-0063 describes counting a preemption-driven resumption. The contract's
counters do not move.

## Conclusion

The tail is the cost of taking one timer interrupt on the ADR-0040 TCG profile.
It is not a removable inefficiency of the TOS IPC path, of `preempt()`, or of
the address-space switch, and no change to the measured path removes it: it is
additive and independent of what the interval contains.

The contract's structure then decides the verdict. The numerator runs with
preemption active and ADR-0066 section 5 keeps its tails; the denominator runs
with preemption inactive under section 3 and can have none; and a nearest-rank
p99 over 21 samples is the maximum of the series. The relative budget therefore
compares *an exchange that usually includes one timer interrupt* against *a call
that cannot be interrupted*, and whether a given run passes depends on whether a
tick landed — which the measurements above show is close to a coin flip at the
reference quantum.

Deciding what to do about that is a Level 2 question about the performance
contract and is not settled here. ADR-0066 section 6 governs: a threshold miss
does not authorize changing the clock, the workload, the denominator or the
budget inside the result that measured it.

## Reproduction

Raw series for every run of all five matrices — samples, floor samples, pair
order, per-sample tick deltas, build features and effective quantum — are
retained in
[stage3-preemption-tail-diagnostic-p1.json](stage3-preemption-tail-diagnostic-p1.json).
Every figure quoted above recomputes from that file alone.

One defect in the diagnostic's own metadata is recorded there rather than
repaired: because the patch gives `apic.rs` two cfg-selected quantum constants
and makes the harness report the smaller, every matrix-4 run records
`quantum_count` 10 000 whatever it was built with. The field is retained as
reported, with the effective value beside it derived from the build's features.
The measurements distinguish the builds regardless — 2.8% against 46.4% of
intervals ticked is a ratio one shared quantum could not produce.

To repeat the experiment, apply the retained patch to a worktree of the recorded
commit, put an observer bundle built by `build-simple-observer.sh` first on
`PATH`, and run each configuration's features through
`host-tools/qemu-test/run.sh` with `--measure 21`, interleaving the
configurations.
