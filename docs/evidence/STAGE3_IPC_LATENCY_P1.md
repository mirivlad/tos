<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 IPC latency P1

Evidence level: **P1, locally measured**. Verdict: **the real Stage 3 IPC
request/reply path satisfies both ADR-0066 latency budgets locally; this is not
P2 and does not close Stage 3**.

## Identity and boundary

The one predeclared conformance run was taken from clean commit
`d759ad49e44a76791fb780b0a1a35e6ba86d32ac` on the ADR-0040
q35/qemu64/one-vCPU/256-MiB/TCG profile. It used the same manifest-bound QEMU
10.0.11 symmetric UART observer for denominator and numerator:

- launcher SHA-256:
  `39474e280cac27e6f249db33c5ce49fe23e33282738023c7c58cf5f81f840a89`;
- observer build-manifest SHA-256:
  `66841d3e28a4fe30df4d4449a950c348e4774048abc068f8070036980b2c39d4`;
- denominator measurement-build SHA-256:
  `581e93e7637bd3fefd46da34840aa1590e83b5921d9513b70d08cc939d399472`;
- numerator measurement-build SHA-256:
  `a64bf9f6e7f1b5dc805b324a335501c78eb1e79fedad12137414a4d2f7992afe`.

The denominator build bound `test-measurement-no-preemption` and
`test-measurement-call`. Its 21/21 positive adjacent floor/call pairs qualified
the observer with one-sided exact sign probability
`4.76837158203125e-07`. The numerator build bound the nucleus to
`test-call-reply,test-measurement-port`, the runtime image to the single
`test-measurement-ipc` workload, and timer preemption to active.

One unmeasured 64-byte exchange primed the real server. Three warm-up and 21
retained intervals then each contained one real 64-byte `endpoint_call`, its
64-byte reply from the other process, and the atomic server answer-and-enter-
wait operation. No floor, interrupt tail or observer cost was subtracted.

## Retained result

| Series | n | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|---:|
| Fixed TOS Core call denominator | 21 | 9.744 | 23.513 | 8.445 | 23.513 |
| 64-byte IPC request/reply | 21 | 59.965 | 100.761 | 42.408 | 100.761 |

The numerator p99 is `4.285331518734317x` the denominator p99. It is below the
relative limit of `8x` (`188.104 µs`) and the independent absolute limit of
`200 µs`. The guest and nucleus independently reported 24 measured exchanges
including warm-ups plus one prime, 50 messages, 75 payload copies, 25 total
exchanges, and balanced `51/51` IPC operation crossings. There was no retry,
sample selection, filtering, batching or subtraction.

The retained licensed records are:

- [raw observer and denominator](stage3-ipc-observer-paired-p1.json);
- [observer qualification](stage3-ipc-observer-qualification-p1.json);
- [raw IPC numerator](stage3-ipc-numerator-p1.json);
- [IPC qualification](stage3-ipc-qualification-p1.json).

Their SHA-256 digests in that order are:

- `504bf8bf04bcabc9d201597aa86b7b47771e738a467dfc643a036b5f4a838214`;
- `f275e058451e4c2f1fbd6aec17d93a7c3b906cccffd5783c9b91eaada8b80164`;
- `03bfa730685f51b6e139b1061644071ef04178a0d2cf649586d1331aee511867`;
- `410d050f9dc68411b4a81ce079ac354bd02b718c1e06f700719ae6275da2011f`.

## Claim boundary and reproduction

This is local P1 evidence for both IPC latency budgets. P2 remains reserved for
the GitHub Actions gate on the pushed SHA. Stage 3 also still requires restart
identity evidence, the remaining E3 adversarial coverage and its versioned
identity/trusted-base report; this result does not substitute for them.

From `source/` at the recorded commit, with the QEMU observer bundle first on
`PATH`, the exact command was:

```sh
PATH=/path/to/qemu-simple-observer/bin:$PATH \
  bash host-tools/qemu-test/stage3-ipc-conformance.sh \
  --out target/stage3-ipc-p1-d759ad4 \
  --evidence-status P1
```
