<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 IPC latency P2 — 300 samples, one budget

Evidence level: **P2, produced by the repository's own reproducible gate on the
pushed commit**. Verdict: **the Stage 3 IPC latency budget is met. The p99 of a
300-sample series is `39.459 µs` against the `200 µs` limit.**

This is the first record taken under the structure ADR-0068 accepted: one
conformance latency budget, a 300-sample series, and a relative ratio that is
retained and decides nothing. The records taken under the previous structure —
[green](STAGE3_IPC_LATENCY_P2.md) and [red](STAGE3_IPC_LATENCY_P2_RED.md) —
stand as they were taken and are not superseded, reclassified or renamed.

## Identity and boundary

GitHub Actions run
[32754751228](https://github.com/mirivlad/tos/actions/runs/32754751228), `qemu`
profile, on pushed commit `615dcacbbd145e48f778353f0e64a858aee7fc3c`. The gate
refuses to emit `P2` outside GitHub Actions. Platform: the ADR-0040
q35/qemu64/one-vCPU/256-MiB/TCG profile, with the two identities ADR-0068
section 6 added — **scheduler quantum `100000` and APIC divider `16`, bound by
the qualifier rather than printed by it.** A run reporting either differently is
refused.

The observer was requalified from its own boot before the numerator ran, at its
own unchanged discipline of 21 adjacent pairs: **20 of 21** positive
differences, which is at or above the 19 ADR-0066 requires and is recorded as
what it was rather than rounded up.

Timer preemption was active, proven from the build manifest. One unmeasured
64-byte exchange primed the server; 3 warm-ups and 300 retained intervals each
bracketed one real `endpoint_call` and its 64-byte reply.

## Retained result

| Series | n | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|---:|
| 64-byte IPC request/reply | 300 | 21.362 | **39.459** | 21.131 | 58.338 |
| Fixed TOS Core call (observational) | 21 | 5.077 | 5.498 | — | — |

The p99 is **rank 297 of 300**, not the maximum: exactly three samples stand
above it, at `40.781`, `55.203` and `58.338 µs`. Under the previous 21-sample
series the reported p99 would have been the largest sample of a much smaller
draw.

The workload counters scale with the series exactly as they are derived to:
`303` measured exchanges plus one prime, `304` exchanges, `608` messages, `912`
payload copies, and balanced `609/609` IPC crossings. Nothing was retried,
filtered, batched or subtracted.

## The tail is in the series, which is the point

Ten of the 300 samples sit at least `10 µs` above the median — a rate of
`3.33%`, which is the interrupt-arrival rate ADR-0068 predicted from
`interval / tick period` and measured in the diagnostic at 2.8% to 4.4%.

That rate is why the series is 300. The p99 is a tail value only when at least
four samples are tail values, since rank 297 is the fourth largest; at this rate
that holds with probability `98%`, and this run had ten. At 21 samples the same
distribution produced a reported p99 that was a tail value in under half of runs
— which is what made the previous structure's verdict a coin flip.

## The ratio, retained and deciding nothing

The record carries `relative_ratio: 7.176973444889051` inside an
`observational` block that states `is_a_budget: false` and names why: the
denominator is measured with preemption inactive and is not comparable to an
active-preemption numerator.

It is worth noting what would have happened otherwise. `7.18x` against the
withdrawn `8x` limit is another near-miss of the kind that went green at
`7.11x` and red at `8.05x` on byte-identical artifacts. The budget that
survived is the one that does not turn on which side of a millisecond a timer
interrupt fell.

## Retained records

- [raw observer and denominator](stage3-ipc-observer-paired-p2-300.json);
- [observer qualification](stage3-ipc-observer-qualification-p2-300.json);
- [raw IPC numerator](stage3-ipc-numerator-p2-300.json);
- [IPC qualification](stage3-ipc-qualification-p2-300.json).

Their SHA-256 digests in that order are:

- `277d809af1813c670ac0786f25a7d81bec199bae144acd2da36d5eba3ce078cc`;
- `d265dd66c165423dc68de39c8f3e686fc17995dcda48e89a14c55c44920744db`;
- `042fa39add0c0c4334249dee1cc3a1e81532bddb8f7ed38d211cecf9a25f8e3e`;
- `5cae012ad542b4b66e185e7813fc45a1ed82b6d885daa0204d97c659b11a5000`.

Every one of those four is a value **the gate itself recorded** in the
qualification's `reports_sha256` block, so the retained files are bound by
digest to the verdict computed from them rather than filed beside it. That
binding is the gap this repository closed after the first P2 record was taken
without it.

## Claim boundary and reproduction

This establishes the Stage 3 IPC latency budget of `docs/35` and `IPC_V1`
section 8 as met on the reference platform, at P2, under ADR-0068's structure.
It says nothing about the counted budgets, which are separate evidence and
separately gated, and it closes no other Stage 3 evidence item: restart identity
and audit, the remaining E3 adversarial coverage and the versioned Stage 3
identity/trusted-base report are untouched by it.

The gate runs as part of the `qemu` profile; GitHub Actions is what makes the
status `P2`:

```sh
bash scripts/preflight.sh --profile qemu
```

From `source/` at the recorded commit, with a QEMU bundle produced by
`build-simple-observer.sh` first on `PATH`:

```sh
PATH=/path/to/qemu-simple-observer/bin:$PATH \
  bash host-tools/qemu-test/stage3-ipc-conformance.sh \
  --out target/stage3-ipc-615dcac \
  --evidence-status P1
```

The same command on this repository's own machine, on the clean commit before
the push, returned `p99=102.637 µs of 300 samples <= 200.0 us` — a local P1 on a
noisier host, passing the same single budget.
