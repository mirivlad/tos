<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 IPC latency P2 — red

Evidence level: **P2, produced by the repository's own reproducible gate on the
pushed commit**. Verdict: **red. The relative budget was missed at
`8.046022830222865x` against the limit of `8x`, by the same four artifacts that
had passed at `7.105872622001654x` one run earlier.**

## Identity and boundary

The run is the `Stage 3 IPC latency conformance` gate of the `qemu` profile in
GitHub Actions run
[32734827424](https://github.com/mirivlad/tos/actions/runs/32734827424), on
pushed commit `2a7ca2033ce7e1d55f50c03cf6ce1ad6b1096dcf` — the commit that
retained [the green record](STAGE3_IPC_LATENCY_P2.md) and changed no source
file. The observer was requalified from its own boot first: 21 of 21 positive
adjacent pairs, one-sided exact sign probability `4.76837158203125e-07`.

## Retained result

| Series | n | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|---:|
| Fixed TOS Core call denominator | 21 | 4.737 | 5.519 | 4.357 | 5.519 |
| 64-byte IPC request/reply | 21 | 30.445 | 44.406 | 29.083 | 44.406 |

The numerator p99 is `44.406 µs` against a relative limit of `44.152 µs`. It
misses by `0.254 µs`. The absolute limit of `200 µs` is met with wide margin and
does not rescue the result: ADR-0066 section 5 requires the one unretried p99 to
satisfy both bounds, and passing one cannot hide failure of the other.

The workload was the contract's, verified as in every other run: one prime, 24
measured exchanges, 25 served answers, 50 messages, 75 payload copies, 25
exchanges, balanced `51/51` crossings. Nothing was subtracted, filtered or
retried.

The retained licensed records are:

- [raw observer and denominator](stage3-ipc-observer-paired-p2-red.json);
- [observer qualification](stage3-ipc-observer-qualification-p2-red.json);
- [raw IPC numerator](stage3-ipc-numerator-p2-red.json);
- [IPC qualification](stage3-ipc-qualification-p2-red.json).

## What the pair of runs establishes

The four measured artifacts of this run are byte-identical to those of the green
run and to the local P1 run: nucleus `db2d100f…` and runtime image `9de69c16…`
for the denominator, nucleus `3cd72f37…` and runtime image `d8401ad6…` for the
numerator. Same observer launcher, same pinned QEMU source, same ADR-0040
profile, same gate, same commit contents for every source file that either run
executed.

So the two verdicts differ by the run and by nothing else:

| Run | Commit | Denominator p99 | Numerator p99 | Ratio | Verdict |
|---|---|---:|---:|---:|---|
| [32644830444](https://github.com/mirivlad/tos/actions/runs/32644830444) | `78447b3` | 7.254 µs | 51.546 µs | 7.106x | green |
| [32734827424](https://github.com/mirivlad/tos/actions/runs/32734827424) | `2a7ca20` | 5.519 µs | 44.406 µs | 8.046x | red |

**One green run therefore did not establish the relative budget, and the green
record has been corrected to say so.** The distribution of the ratio straddles
`8x` on the reference platform; which side a run lands on is decided by the
runner, not by the system under test.

## Where the miss actually is

The numerator's 21 raw samples are `29.083` to `32.488 µs` — except one, at
`44.406 µs`. That single sample **is** the p99 by nearest rank, and it is the
whole of the failure: against the same denominator, the second-largest sample
gives `5.887x`.

That tail is not noise to be removed. ADR-0066 section 5 puts it in the series
on purpose: the numerator runs with timer preemption active, and "a
timer/preemption tail is part of the active-preemption numerator and is neither
removed nor relabelled as observer cost". The denominator, by section 3, is
measured with preemption *inactive*, which is conservative for the denominator
and makes the ratio stricter.

So the ratio compares, at the 99th percentile, a distribution that contains
timer tails against one that by construction cannot. The median ratio is
`6.43x`; the p99 ratio is `8.05x`. The gap between those two numbers is the
preemption tail, and it is what any real fix has to address.

## What may not be done about it

ADR-0066 section 6: a valid observer that measures a value above a budget
reports a red result, and "threshold failure does not authorize changing the
clock, workload, denominator or system architecture inside the same result".
Nothing here re-runs, re-selects, re-scales or re-reads the budget. The gate
stays fail-closed and the `qemu` profile stays red until the measured path
changes or the Project Architect revisits the contract as its own decision.
