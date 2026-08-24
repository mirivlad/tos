<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 symmetric observer P2

Evidence level: **P2, produced by the repository's own reproducible gate on the
pushed commit**. Verdict: **the ADR-0066 observer is qualified by CI; this
qualifies the instrument and measures no IPC budget**.

## Identity and boundary

The run is the `Stage 3 ADR-0066 observer conformance` gate of the `qemu`
profile in GitHub Actions run
[32644830444](https://github.com/mirivlad/tos/actions/runs/32644830444), job
`97207387908`, on pushed commit
`78447b31c62cd24a1549f5c2ac7833cdd6fe153b`. The gate refuses to emit `P2`
outside GitHub Actions, so this status is a property of where the record was
taken, not a label chosen for it. The record reports `dirty: false` and the
ADR-0040 q35/qemu64/one-vCPU/256-MiB/TCG profile, exactly as the local P1 run
did.

The observer is the reproducibly built, manifest-bound QEMU 10.0.11 symmetric
UART profile, built inside the same job from the pinned upstream archive:

- upstream archive SHA-256:
  `22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871`;
- launcher SHA-256:
  `39474e280cac27e6f249db33c5ce49fe23e33282738023c7c58cf5f81f840a89`;
- QEMU engine SHA-256:
  `2189eb13fae58fe8b5522c3aa3f8fcb74e6e0470921a829e8ea7d387b809e04e`;
- observer build-manifest SHA-256:
  `739b51d5c1372a49a96d8465ca0a4dc89b2cf649ccbf4c90f1363fa2d8d4f559`.

The engine digest is **not** the local P1 engine digest
`90f836fdf42f35b5e1b48e1aaebfc64876d0ef4317d1b0ba532597f0f6d74eae`, and the
build manifest differs from the local one because it records host paths, the
compiler (`cc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0` against Debian 14.2.0
locally) and the host Python. What is identical across the two hosts is
everything the observer contract names: the upstream archive digest, the
`configure` line, `SOURCE_DATE_EPOCH`, the build path remap, the vendored
build wheels, the launcher, and the upstream and modified digests of both
patched files, `hw/char/serial.c` and `hw/char/trace-events`. The instrument is
therefore the same instrument, compiled by a different compiler; it is not a
copied binary.

One prepared process supplied three warm-up and 21 retained adjacent blocks.
Each block contained one empty floor and one immutable 64-byte TOS Core call
with a common sequence, a distinct echoed work bit and the predeclared
alternating order, which the retained record shows beginning `call-floor`. The
measurement build manifest SHA-256 is
`5adfd3885cbe78dac33c7a902ad3250d0e1cfead059deb7f97cd96e209d49ea2`; it binds
the nucleus to `test-measurement-no-preemption` and the runtime image to
`test-measurement-call`.

## Retained result

| Series | n | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|---:|
| Adjacent empty floor | 21 | 1.834 | 2.264 | 1.623 | 2.264 |
| Fixed TOS Core call | 21 | 6.492 | 7.484 | 5.960 | 7.484 |

All 21 paired differences were positive. The smallest was `4.238 µs`, the
median was `4.479 µs`, and the one-sided exact sign probability was
`4.76837158203125e-07`, against the ADR-0066 requirement of at least 19 of 21
(`p <= 0.000111`). Nothing was retried, filtered, reordered, batched or
subtracted.

The `Stage 3 IPC latency conformance` gate in the same job qualified the
observer a second time, independently, from its own boot: 21 of 21 positive
pairs, minimum gap `3.948 µs`, floor median/p99 `1.944/2.274 µs`, denominator
median/p99 `6.342/7.254 µs`. That second qualification is retained with the IPC
record in [STAGE3_IPC_LATENCY_P2.md](STAGE3_IPC_LATENCY_P2.md).

The retained licensed records are:

- [raw paired measurement](stage3-observer-paired-p2.json);
- [qualification verdict](stage3-observer-qualification-p2.json).

Their SHA-256 digests are respectively
`d60a3be4f437e5eeb101993dc4824b59912cf716211be50be56200f15aaaf3a0`
and
`73a1c520dbe33e98bdd855d43e22b592383c3c425a138793d7a5c2144215f343`.
The first of those is the value the gate itself recorded as
`measurement_report_sha256` inside the qualification record, so the retained raw
series is bound by digest to the verdict taken from it rather than merely
filed beside it.

## What the CI environment changed and what it did not

The CI floor and denominator are both markedly tighter than the local P1 run
(`2.996/5.570 µs` floor and `9.623/29.896 µs` denominator there). This is an
environment difference, not a code difference: the measured nucleus and runtime
image are byte-identical to the local ones (`db2d100f…` and `9de69c16…`). A
tighter denominator makes the separate `8x` IPC ratio **harder** to satisfy, so
the quieter environment is the stricter test of the relative budget, not a
favourable one.

## Claim boundary and reproduction

This is the versioned repository gate ADR-0066 section 2 requires before a
backend may be called a P2 conformance observer. It is not an IPC result and
establishes neither the relative `8x` nor the absolute `200 µs` budget.

The gate runs from the `qemu` profile; nothing about the command is
CI-specific except that GitHub Actions is what makes the status `P2`:

```sh
bash scripts/preflight.sh --profile qemu
```

From `source/` at the recorded commit, with a QEMU bundle produced by
`build-simple-observer.sh` first on `PATH`, the single gate is:

```sh
PATH=/path/to/qemu-simple-observer/bin:$PATH \
  bash host-tools/qemu-test/stage3-observer-conformance.sh \
  --out target/stage3-observer-78447b3 \
  --evidence-status P1
```
