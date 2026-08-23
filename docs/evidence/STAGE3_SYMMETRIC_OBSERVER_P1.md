<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 symmetric observer P1

Evidence level: **P1, locally measured**. Verdict: **the ADR-0066 observer is
qualified locally; this is not P2 and does not measure IPC**.

## Identity and boundary

The run was taken from clean commit
`626876b64d0692443a6bac3aa3ebeb15c7b7d09d` on the ADR-0040
q35/qemu64/one-vCPU/256-MiB/TCG profile. The observer is the reproducibly built,
manifest-bound QEMU 10.0.11 symmetric UART profile:

- launcher SHA-256:
  `39474e280cac27e6f249db33c5ce49fe23e33282738023c7c58cf5f81f840a89`;
- QEMU engine SHA-256:
  `90f836fdf42f35b5e1b48e1aaebfc64876d0ef4317d1b0ba532597f0f6d74eae`;
- observer build-manifest SHA-256:
  `66841d3e28a4fe30df4d4449a950c348e4774048abc068f8070036980b2c39d4`;
- upstream archive SHA-256:
  `22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871`.

One prepared process supplied three warm-up and 21 retained adjacent blocks.
Every block contained one empty floor and one immutable 64-byte TOS Core call,
with a common sequence, distinct echoed work bit and predeclared alternating
order. The QEMU vCPU thread recorded `CLOCK_THREAD_CPUTIME_ID` after handling
`OPEN` and before handling `CLOSE`. Marker transport, trace construction and
host descheduling were outside the interval; nothing was subtracted.

The measurement-build manifest SHA-256 is
`a9a762259e0e42b30a895a6bdd2c326f9fc3c754f93f4334b5dedfa1f07b412a`.
It binds the exact nucleus to `test-measurement-no-preemption` and the exact
runtime image to `test-measurement-call`. The production nucleus and runtime
image hashes were unchanged before and after those builds.

## Retained result

| Series | n | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|---:|
| Adjacent empty floor | 21 | 2.996 | 5.570 | 1.198 | 5.570 |
| Fixed TOS Core call | 21 | 9.623 | 29.896 | 8.779 | 29.896 |

All 21 paired differences were positive. The smallest was `4.947 µs`, the
median was `7.919 µs`, and the one-sided exact sign probability was
`4.76837158203125e-07`. ADR-0066 requires at least 19 of 21 (`p <= 0.000111`).
No retry, filtering, reordering, batching or subtraction was used.

The retained licensed records are:

- [raw paired measurement](stage3-observer-paired-p1.json);
- [qualification verdict](stage3-observer-qualification-p1.json).

Their SHA-256 digests are respectively
`d871c2ebd7d0c54a2cc624257371543ce582abf5b55c91c8ed01e957689c4ac1`
and
`c0b89736d5771c9bdbe716f4b2a066a576b607abf1fa9b687489d139cfbd64a4`.

## Claim boundary and reproduction

This qualifies the external observer and permits the separately identified IPC
numerator work to begin. It does not establish the relative `8x` or absolute
`200 µs` IPC budget. P2 remains reserved for the GitHub Actions gate and its
retained artifact.

From `source/` at the recorded commit, with a QEMU bundle produced by
`build-simple-observer.sh` first on `PATH`:

```sh
PATH=/path/to/qemu-simple-observer/bin:$PATH \
  bash host-tools/qemu-test/stage3-observer-conformance.sh \
  --out target/stage3-observer-p1-626876b \
  --evidence-status P1
```
