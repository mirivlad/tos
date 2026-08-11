<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 performance record — P1, locally measured

Produced by `source/tests/performance-core`. Reproduce with:

```bash
cargo run --manifest-path source/Cargo.toml -p tos-core-performance --release
```

**Evidence level: P1 (locally measured).** docs/35 assigns evidence levels
P0 unmeasured design, P1 locally measured, P2 reproducible CI measurement,
P3 independently reproduced, and says no stage closes on P0 for a metric
assigned to that stage. This record lifts the two Stage 2 metrics off P0.
It is **not** the declared reference-platform measurement a stage gate
needs: this machine is not that platform, and asserting a budget from it
would be a fabricated pass. The Stage 2 performance gate stays open.

## Environment

```text
source commit   a2cedfa (working tree at measurement)
toolchain       rustc 1.97.1 (8bab26f4f 2026-07-14)
profile         release, opt-level 2, lto true, panic abort
kernel          Linux 6.5.0-1mx-ahs-amd64
cpu             Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz
cores           28
cache state     cold process, no derived-artifact cache in the path
```

## Samples

```text
sampling: 3 warmups, 21 samples, median/p95/p99 in microseconds

fixture: canonical module of 262069 bytes, content sha256:6298c4f6df5f1c209644da02169e9beb590651e2859f4b645826b82fe5cf86a2
parse + check + lower + verify, 256 KiB module
  median 145031 us, p95 146719 us, p99 152367 us, min 143022 us, max 152367 us
  raw samples (us): 143022 143381 143410 143866 144616 144799 144855 144912 144958 144974 145031 145041 145054 145414 145458 146513 146570 146662 146712 146719 152367
  docs/35 budget: 500 ms p95 on the reference platform
one-million-operation integer/control-flow benchmark
  median 333423 us, p95 348204 us, p99 394669 us, min 331201 us, max 394669 us
  raw samples (us): 331201 331570 332298 332932 333041 333072 333087 333112 333329 333333 333423 333992 334193 334309 334884 339479 342087 342350 343444 348204 394669
  docs/35 budget: within 10x a host reference interpreter
reject a quota-exceeding module
  median 57683 us, p95 58328 us, p99 58726 us, min 57383 us, max 58726 us
  raw samples (us): 57383 57415 57425 57425 57434 57539 57559 57593 57620 57627 57683 57779 57784 57809 57832 57890 58103 58162 58168 58328 58726
  rejection/acceptance p95 ratio: 0.398 (docs/35 budget: at most 2.000)

This record is P1. Closing the docs/35 Stage 2 gate needs the same
procedure on the declared reference platform, retained as raw samples.
```

## Reading

The frontend-to-receipt path over a 256 KiB canonical module sits well
inside the 500 ms p95 budget on this machine, and rejecting a
quota-exceeding input of the same size costs less than the accepted input
rather than degrading — the ratio docs/35 bounds at 2.0 is 0.4 here.

The one-million-operation benchmark has no comparison recorded: docs/35
states it relative to "the host reference interpreter time under the same
semantic implementation", and no such second implementation exists yet, so
the ratio the budget is written against cannot be computed. The absolute
number is retained so the comparison can be made when one does.
