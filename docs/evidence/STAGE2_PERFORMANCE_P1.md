<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 performance record — native-host half of the pair

Produced by `source/tests/performance-core`. Reproduce with:

```bash
cargo run --manifest-path source/Cargo.toml -p tos-core-performance --release -- --profile native
```

## What this is, and what it is not

ADR-0040 (**Proposed**) fixes the Stage 2 reference platform as the same
q35/qemu64/one-vCPU/256-MiB/TCG profile Stage 1 already mandates, and reads
the docs/35 execution budget as a ratio of that platform's time to the
native-host time of the *same* engine at the *same* commit.

This file is the **native half** of that pair. It is the denominator, not a
gate result. The reference half is not taken yet: running this harness under
the ADR-0040 profile needs the engine to execute inside that profile, which
is the remaining work. Presenting this as a reference-platform pass would be
a fabricated PASS, so it is not presented as one.

The harness is **told** which profile it ran under and records what it was
told. It never concludes that whatever machine it happens to be on is the
reference platform — choosing the platform after seeing the number is what
ADR-0040 exists to prevent.

## Environment

```text
source commit   8f76bc1 (working tree at measurement)
toolchain       rustc 1.97.1 (8bab26f4f 2026-07-14)
profile         release, opt-level 2, lto true, panic abort
kernel          Linux 6.5.0-1mx-ahs-amd64
cpu             Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz
cores           28
cache state     cold process, no derived-artifact cache in the path
```

## Samples

```text
TOS Core Stage 2 measurement harness
profile: native (declared, not inferred — ADR-0040)
evidence level: P1 native-host baseline; it is the denominator of the ratio, not a gate
sampling: 3 warmups, 21 samples, median/p95/p99 in microseconds

fixture: canonical module of 262069 bytes, content sha256:6298c4f6df5f1c209644da02169e9beb590651e2859f4b645826b82fe5cf86a2
parse + check + lower + verify, 256 KiB module
  median 137004 us, p95 141335 us, p99 143878 us, min 133067 us, max 143878 us
  raw samples (us): 133067 133407 133967 134659 134837 134869 135730 135894 136475 136568 137004 137296 137310 137326 137652 138852 139016 140396 140654 141335 143878
  docs/35 budget: 500 ms p95 on the reference platform
one-million-operation integer/control-flow benchmark
  median 310375 us, p95 314610 us, p99 314733 us, min 305412 us, max 314733 us
  raw samples (us): 305412 306485 306700 306904 306917 307894 308576 308659 308991 309625 310375 310398 310655 310753 311335 311813 312182 312337 312672 314610 314733
  docs/35 budget: within 10x a host reference interpreter
  this is the native baseline; pass --baseline 314610 to the reference-profile run to obtain the ratio
reject a quota-exceeding module
  median 55220 us, p95 59123 us, p99 64124 us, min 52870 us, max 64124 us
  raw samples (us): 52870 53055 53207 53372 53375 54051 54401 54703 54707 55097 55220 55578 55654 55730 55865 56635 56704 57687 57893 59123 64124
  rejection/acceptance p95 ratio: 0.418 (docs/35 budget: at most 2.000)

This is the native half of the pair. Closing the docs/35 Stage 2 gate
needs the same procedure under the ADR-0040 reference profile, with
both halves retained as raw samples.
```

## Reading

The frontend-to-receipt path over a 256 KiB canonical module is well inside
the 500 ms p95 budget here, and rejecting a quota-exceeding input **of the
same size** costs 0.4 of the accepted input against a bound of 2.0 — the
comparison is against a comparable input on purpose, because docs/35 bounds
a rejection by the accepted-input budget.

The execution ratio is not computed here. It needs the reference half, and
the harness prints the `--baseline` value to carry across so the quotient
is never shown without the measurement it came from.
