<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 measurement channel P1 diagnostic

## Verdict

**P1 diagnostic-insufficient. This is not IPC performance conformance
evidence.**

The external protocol, exact TOS Core denominator boundary, reference guest
profile, artifact isolation and fail-closed sampling all completed. The QEMU log
trace backend remains too large a part of the value it observes: its floor
median is 45.5% of the call median. Under ADR-0066 that is a comparable floor,
so this observer cannot establish the relative `8x` budget. IPC timing was not
started and neither the relative nor the absolute IPC budget is claimed.

The retained run's empirical ranges happen not to overlap: the largest floor
sample is 15.020 us and the smallest call sample is 21.219 us, a 6.199 us gap.
That does not overturn the verdict. Earlier diagnostic runs recorded by
ADR-0066 did overlap, while this call series has a 111.103 us p99 outlier. A
backend whose own work is almost half the median denominator and whose result
changes shape at this scale is not made into a conformance clock by selecting
the favorable run.

## Frozen input and observer

- Source commit: `d4f788a4017e15d87963c7338abe3c3285e5d616`, clean tree.
- Guest: q35, qemu64, one vCPU, 256 MiB, TCG; scheduler preemption active,
  quantum count `100000`.
- Observer: QEMU log trace `serial_write`, `gettimeofday` microsecond text
  timestamp in the vCPU thread.
- QEMU: `10.0.11 (Debian 1:10.0.11+ds-0+deb13u1)`, executable SHA-256
  `184914c77ba4074281a6e7bd5d1959f0115abb553688a8c8f02940627ad197fa`.
- Host CPU: Intel Xeon E5-2680 v4; Rust `1.97.1`.
- Both series contain three discarded warm-ups followed by 21 retained
  individual samples. No value was filtered, repaired, batched or corrected;
  nothing was subtracted.

The complete environment, firmware, loader, capsule, measurement-artifact and
production-artifact identities are embedded in the raw reports:

- [empty marker floor](stage3-measurement-log-floor.json)
- [64-byte inner call](stage3-measurement-log-inner-call.json)

## Result

| Series | n | Median us | p99 us | Min us | Max us | Jitter us |
|---|---:|---:|---:|---:|---:|---:|
| Empty floor | 21 | 10.967 | 15.020 | 9.775 | 15.020 | 5.245 |
| Inner call | 21 | 24.080 | 111.103 | 21.219 | 111.103 | 89.884 |

Floor/call ratios are `0.455` at the median and `0.135` at p99. The p99 ratio is
not evidence of better resolution: it is dominated by the call series' largest
sample under nearest-rank p99. No ratio has been subtracted from another value.

The observed process reported `samples=24 calls=24 refused=0`: all three
warm-ups and 21 retained attempts executed the exported TOS Core function with
one 64-byte value argument and a `unit` result. The engine marks include reading
that argument from the caller, call-frame and resource accounting, the empty
callee body, return and writeback. Run setup remains outside the marks.

## Production isolation

Production hashes were taken before the feature builds and again after them.
Both pairs are equal:

- nucleus:
  `9da0d65199ce21851b90813b66e25321fed2dbb4dd5496012244a6b29fccb665`;
- runtime image:
  `45bf0b9e5abc156226f6c333b411f14b554b21ae1160a3a53fc7a82b916c3ee1`.

The measurement nucleus hash is
`7db15e49d86658bfe20346e1533c6f9242f9238b98832218601612e637624bcc`.
The floor and call runtime-image hashes differ because only the latter contains
the denominator workload; both are identified in the raw reports. IOPL remains
zero and the compile-time TSS bitmap proof permits only COM1
`0x3f8..=0x3ff`.

## Reproduction

From `source/` at the source commit above:

```sh
bash host-tools/qemu-test/measurement-denominator.sh \
  target/stage3-measurement-log-p1-d4f788a
```

The next admissible step is validation of a pinned upstream low-overhead QEMU
observer. It is not IPC timing, and it does not change the denominator or the
budgets.
