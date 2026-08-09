<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 finalist measurement summary

## Environment

- Date: 2026-08-09 UTC.
- Source baseline: `f20b200a462588ff355001326b3327fa1de430db`.
- OS: Debian GNU/Linux 13 (trixie), kernel `6.5.0-1mx-ahs-amd64`.
- CPU: Intel Xeon E5-2680 v4, 14 physical cores / 28 logical CPUs, one NUMA
  node; 31.2 GiB RAM.
- Compiler: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6.
- Build: `rustc --edition=2024 -D warnings -O`; no Cargo package, external
  crate or downloaded dependency is used by either research prototype.

## Workload and protocol

Both models run all 13 common cases and the same 64-way CPU-bound deterministic
partitioned reduction over `[0, 1_048_576)`. Each record has 3 warmups and 21
raw wall-clock samples. Percentiles use nearest-rank selection from its 21 raw
nanosecond samples. `overlap=true` requires both `max_active >= 2` and two or
more CPU identifiers observed from CPU-bound host worker threads.

The results measure prototype execution including its in-process semantic
checks. They are not a Stage 2 parser/IR performance claim and must not be
compared to the Stage 1 performance contract.

| Candidate/mode | Median | p95 | p99 | Overlap | Result digest | Raw record |
|---|---:|---:|---:|---|---|---|
| Bespoke reference, 1 worker | 12.864 ms | 13.527 ms | 14.021 ms | false | `stage15-common-v1-d000032aaaa80000` | `bespoke-reference-1.json` |
| Bespoke parallel, 2 workers | 8.208 ms | 8.775 ms | 8.926 ms | true | same | `bespoke-parallel-2.json` |
| Bespoke parallel, 4 workers | 5.530 ms | 6.368 ms | 6.439 ms | true | same | `bespoke-parallel-4.json` |
| Adapted Rust reference, 1 worker | 12.587 ms | 13.596 ms | 14.106 ms | false | same | `adapted-rust-reference-1.json` |
| Adapted Rust parallel, 2 workers | 7.849 ms | 8.644 ms | 8.650 ms | true | same | `adapted-rust-parallel-2.json` |
| Adapted Rust parallel, 4 workers | 5.522 ms | 6.170 ms | 6.222 ms | true | same | `adapted-rust-parallel-4.json` |

## Interpretation limits

The two prototypes intentionally share a small host-thread backend so their
microbenchmark is a viability/conformance exercise, not a language-performance
contest. It demonstrates actual multiple runnable execution contexts and a
stable logical result on the observed host CPUs. It does not establish a future
TOS scheduler, NUMA policy, allocation profile or final compiler performance.
