<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 IPC latency P2

Evidence level: **P2, produced by the repository's own reproducible gate on the
pushed commit**. Verdict: **the real Stage 3 IPC request/reply path satisfies
both ADR-0066 latency budgets in CI; this closes the quantitative half of the
Stage 3 IPC performance contract and does not close Stage 3**.

## Identity and boundary

The run is the `Stage 3 IPC latency conformance` gate of the `qemu` profile in
GitHub Actions run
[32644830444](https://github.com/mirivlad/tos/actions/runs/32644830444), job
`97207387908`, on pushed commit
`78447b31c62cd24a1549f5c2ac7833cdd6fe153b`, reported by the record as
`dirty: false`. The gate refuses to emit `P2` outside GitHub Actions. The
platform is the ADR-0040 q35/qemu64/one-vCPU/256-MiB/TCG profile.

One observer measured both sides of the ratio, built inside the same job from
the pinned upstream archive
`22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871`:

- launcher SHA-256:
  `39474e280cac27e6f249db33c5ce49fe23e33282738023c7c58cf5f81f840a89`;
- QEMU engine SHA-256:
  `2189eb13fae58fe8b5522c3aa3f8fcb74e6e0470921a829e8ea7d387b809e04e`;
- observer build-manifest SHA-256:
  `739b51d5c1372a49a96d8465ca0a4dc89b2cf649ccbf4c90f1363fa2d8d4f559`;
- denominator measurement-build SHA-256:
  `5adfd3885cbe78dac33c7a902ad3250d0e1cfead059deb7f97cd96e209d49ea2`;
- numerator measurement-build SHA-256:
  `9e6168dc16f4228deb48b3715059dc851b9b0088cfb0d9a7ec91875cbc2dd642`.

The observer was requalified from its own boot before the numerator ran: 21 of
21 positive adjacent floor/call pairs, minimum gap `3.948 µs`, one-sided exact
sign probability `4.76837158203125e-07`, floor median/p99 `1.944/2.274 µs`. The
[standalone observer gate](STAGE3_SYMMETRIC_OBSERVER_P2.md) in the same job
qualified it independently a second time.

The denominator build bound the nucleus to `test-measurement-no-preemption` and
the runtime image to `test-measurement-call`. The numerator build bound the
nucleus to `test-call-reply,test-measurement-port`, the runtime image to the
single `test-measurement-ipc` workload, and — derived from that manifest, not
declared by a caller — timer preemption to active with a quantum count of
`100000`.

One unmeasured 64-byte exchange primed the real server. Three warm-up and 21
retained intervals then each contained one real 64-byte `endpoint_call`, its
64-byte reply from the other process, and the atomic server
answer-and-enter-wait operation. No floor, interrupt tail or observer cost was
subtracted.

## Retained result

| Series | n | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|---:|
| Fixed TOS Core call denominator | 21 | 6.342 | 7.254 | 5.961 | 7.254 |
| 64-byte IPC request/reply | 21 | 38.071 | 51.546 | 36.007 | 51.546 |

The numerator p99 is `7.105872622001654x` the denominator p99. It is below the
relative limit of `8x` (`58.032 µs`) and below the independent absolute limit of
`200 µs`; both bounds are satisfied by the same single unretried p99.

The guest and nucleus independently reported the workload the contract
describes: 24 measured exchanges including warm-ups plus one prime, 25 served
answers with zero refusals, 50 messages, 75 payload copies, 25 total exchanges
and balanced `51/51` IPC operation crossings. There was no retry, sample
selection, filtering, batching or subtraction.

The retained licensed records are:

- [raw observer and denominator](stage3-ipc-observer-paired-p2.json);
- [observer qualification](stage3-ipc-observer-qualification-p2.json);
- [raw IPC numerator](stage3-ipc-numerator-p2.json);
- [IPC qualification](stage3-ipc-qualification-p2.json).

Their SHA-256 digests in that order are:

- `4bef19fa81f6362604888e7a1ff3d8e28e74b94fc1965737a2e0ef1cdcbf471e`;
- `f5986ef7e449148f8f43364246794996f0810237d9c2cc5041aa1cbd8e0a1426`;
- `ecd2024cdf0f3cfb3d225075bd6faf5f816cd5bbcb7ffda3b7d4ccc43aa1ca97`;
- `1228210a067c6c99170e9ed96862033f06a3a4260205f38872cf330199d467e1`.

The first digest is the value the gate recorded as `measurement_report_sha256`
inside the observer qualification, so that raw series is bound by digest to its
verdict. The IPC qualification record retained here names its denominator,
numerator and serial log by path only, which is checkable while the CI run's
directory exists and not afterwards. The gate has since been changed to record
a `reports_sha256` digest for every input it reads; that applies to runs after
this one and is not claimed for these four files.

## The same artifacts, a different machine

Every artifact the two budgets were measured on is byte-identical to the one
measured locally at P1, across two different hosts, compilers and toolchain
versions:

| Artifact | SHA-256 |
|---|---|
| Denominator nucleus (`test-measurement-no-preemption`) | `db2d100fa0bfc5c0416f633d70a6e657e3eb281d27ccd6d0ef96732df6a6ab9d` |
| Denominator runtime image (`test-measurement-call`) | `9de69c16ecd1845a491994d0dcc5ec6413bc2acf6eef332ff9dd33c524ddfbff` |
| Numerator nucleus (`test-call-reply,test-measurement-port`) | `3cd72f37da843204279441f741d6128900bb138e9ed68b2176aa771cd4a62b97` |
| Numerator runtime image (`test-measurement-ipc`) | `d8401ad6ff3867a4bf842f57984f4d2656ae609f05866e2913773992af9f5f9e` |

So the difference between the local and CI numbers is the environment, and only
the environment.

## The margin, stated rather than implied

Both budgets pass, and the relative one is the narrower of the two. Local P1
reported `4.285331518734317x`; CI reports `7.105872622001654x`. The ratio is
worse in CI even though the absolute latency is better there — `51.546 µs`
against `100.761 µs` — because the quieter runner tightened the denominator far
more (`7.254 µs` against `23.513 µs`) than it tightened the numerator.

That is the honest reading of both records: a noisy host inflates the
denominator and flatters the ratio, so the ratio measured in the quiet
environment is the stricter of the two, and it is the one that must be watched.
`11.2%` of headroom against `8x` is a real pass and a thin one. Recovering it
belongs to IPC path work, not to the denominator, the workload or the
arithmetic — ADR-0066 section 6 forbids answering a threshold with a change to
the instrument.

## Claim boundary and reproduction

This is P2 evidence for both quantitative IPC latency budgets of `docs/35`
section on Stage 3 and `IPC_V1` section 8. The counted half of that section —
copies, crossings, absence of allocation, constant-time capability check — is
separate evidence and is not restated here.

Stage 3 still requires service restart identity/audit evidence, the remaining
E3 adversarial coverage and its versioned identity and trusted-base report.
This result does not substitute for any of them.

The gate runs as part of the `qemu` profile; nothing about the command is
CI-specific except that GitHub Actions is what makes the status `P2`:

```sh
bash scripts/preflight.sh --profile qemu
```

From `source/` at the recorded commit, with a QEMU bundle produced by
`build-simple-observer.sh` first on `PATH`, the single gate is:

```sh
PATH=/path/to/qemu-simple-observer/bin:$PATH \
  bash host-tools/qemu-test/stage3-ipc-conformance.sh \
  --out target/stage3-ipc-78447b3 \
  --evidence-status P1
```
