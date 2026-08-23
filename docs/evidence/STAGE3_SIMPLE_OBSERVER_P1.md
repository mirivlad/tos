<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 QEMU simple observer P1

Evidence level: **P1, locally measured**. Verdict: **observer candidate
qualified locally; not P2 and not IPC conformance**.

## Exact identity

The retained series was taken from clean commit
`e1d2b1e6518c146d2c457fc741fbf8052dbebbe5` on the ADR-0040
q35/qemu64/one-vCPU/256-MiB/TCG profile. The observer was built by
`source/host-tools/qemu-test/build-simple-observer.sh` from the upstream QEMU
10.0.11 release archive:

- source archive SHA-256:
  `22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871`;
- launcher SHA-256:
  `39474e280cac27e6f249db33c5ce49fe23e33282738023c7c58cf5f81f840a89`;
- QEMU engine SHA-256:
  `a8286c54aeea2d1e74e6a060bf044b3be4d742c303660892b103f307d8a0249a`;
- build-manifest SHA-256:
  `9796311047750de8ebab555fef33b137cd0fdabdf398c7bdde1e359f21908f80`.

The build disabled network downloads and `libfdt`, used QEMU's vendored Meson
and pycotap wheels, fixed `SOURCE_DATE_EPOCH`, and remapped the temporary source
and build paths. Two independent builds in different temporary directories
produced the same engine bytes and SHA-256. The retained manifest additionally
binds the launcher, engine, three ROM inputs and 80 dynamic host libraries; the
observer verifies every one before starting QEMU.

The detached upstream archive signature was cryptographically good, but the
QEMU release key had expired before the signature date. This record therefore
does not elevate that signature into a current trust claim: the build is bound
to the exact archive digest obtained from QEMU's official download service.

## Instrument and refusal behavior

The repository decoder independently reads QEMU simple trace format v4. It
keeps timestamps as integer nanoseconds from QEMU's `CLOCK_MONOTONIC` clock and
selects only `serial_write` register-zero marker bytes. It rejects malformed or
truncated records, duplicate mappings, unknown record types or event IDs,
wrong serial payload sizes and every non-zero dropped-event record.

The existing sequence-pairing checks remain in force: duplicate or overlapping
opens, close without open, sequence mismatch, unclosed pairs, clock reversal
and zero or negative intervals invalidate the complete run. The measurement
contains the channel floor and subtracts nothing.

## Retained raw result

Each boot used three warm-ups followed by 21 individual samples:

| Series | Median µs | p99 µs | Min µs | Max µs |
|---|---:|---:|---:|---:|
| empty marker floor | 3.532 | 9.990 | 3.297 | 9.990 |
| fixed 64-byte TOS Core call | 13.760 | 24.732 | 12.661 | 24.732 |

The complete floor range ends **2.671 µs below** the complete call range. The
floor is 25.7% of the call at the median and 40.4% at nearest-rank p99. Unlike
the rejected text-log observer, these distributions do not overlap in this
series, so this observer resolves the immutable denominator locally without
subtraction or selected samples.

Raw licensed reports are retained verbatim as:

- `docs/evidence/stage3-measurement-simple-floor-p1.json`;
- `docs/evidence/stage3-measurement-simple-inner-call-p1.json`.

Both reports prove the production nucleus and runtime image remained
byte-for-byte unchanged by the measurement builds. The measured call is the
ordinary exported TOS Core function named by `IPC_V1` section 8: one 64-byte
value argument, `unit` result, ordinary call accounting, argument read and
writeback inside the markers.

## Boundary of the claim

This is P1 because it was measured locally. It qualifies the pinned observer as
a candidate for the repository P2 gate; it does not itself make the observer
P2, does not measure IPC and does not satisfy either Stage 3 IPC budget. P2
requires the repository gate to reproduce the same separation in CI and retain
its raw artifacts. IPC timing may begin only after that gate exists and passes.

Reproduction on the recorded host:

```sh
source/host-tools/qemu-test/build-simple-observer.sh \
  /path/to/qemu-10.0.11.tar.xz /tmp/tos-qemu-observer
cd source
PATH=/tmp/tos-qemu-observer/bin:$PATH \
  bash host-tools/qemu-test/measurement-denominator.sh \
  target/stage3-simple-observer
```
