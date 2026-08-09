<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 TCB, dependency and recovery evidence

## Measured now

| Item | Bespoke model | Adapted-Rust model | Interpretation |
|---|---:|---:|---|
| Trusted semantic-model LOC | 275 Rust | 251 Rust | Research models only; neither is predicted production TCB. |
| Harness/test LOC | 56 Python | 77 Python + 22 negative Rust | Research-only and outside future runtime TCB. |
| Stripped host binary | 409,432 B | 409,088 B | Same host Rust backend; this is not a language selection metric. |
| Direct/transitive code dependencies | 0 external crates / 0 | 0 external crates / 0 | Both use only host `std`; no network download. |
| Dynamic dependencies | libc, libgcc_s, ELF loader | libc, libgcc_s, ELF loader | Host experiment dependencies, expressly excluded from the TOS runtime contract. |
| Cold process/corpus execution | 0.01 s resolution | 0.01 s resolution | Shell timer resolution is insufficient for a finer cold-start comparison; raw 21-sample execution timings are in `SUMMARY.md`. |
| Peak/representative RSS | Not measured | Not measured | The sub-15 ms processes cannot be sampled reliably with available host tooling; no invented RSS value is retained. Stage 2 must measure it. |
| Unsafe/native/FFI | no unsafe, no FFI | no unsafe, no FFI | Applies only to the research source; host binary still links the measured libc/libgcc_s. |

Host tool identity is Rust 1.97.1 / LLVM 22.1.6. `rustc` and `cargo` resolve
through a 20,838,840-byte `rustup` proxy on this host; this is a host build-tool
footprint, not an admitted runtime dependency. No Rust-vs-bespoke comparison is
drawn from it because both experiments deliberately use the same host backend.

## Projected architectural TCB shape (not measured LOC)

| Component | Bespoke TOS Core | Adapted Rust |
|---|---|---|
| Inheritable semantics | None: TOS owns the contract | Rust parsing/type/ownership/atomics only to the extent a restricted profile can normatively adopt them |
| Required TOS components | lexer/parser; semantic/type/effect checker; ownership/region checker; IR lowering; independent verifier; reference interpreter; minimal task runtime | profile/restriction checker; capability/resource layer; IR lowering; independent verifier; recovery frontend/runtime; mappings for Rust diagnostics, source maps and atomics |
| Recovery risk | Must implement bounded TOS frontend/interpreter | Must either carry a contained Rust frontend/runtime or make rustc/LLVM/host ABI a recovery dependency; the latter is rejected |
| Toolchain role | Rust may implement Stage 2 and remain build-only | rustc/LLVM build-only unless a future ADR admits a constrained role; never the hidden TOS semantic contract |

## Not measurable until Stage 2

Production parser/frontend time, lowering/checker/verifier time, production
cold start/RSS, final trusted LOC, complete diagnostics, recovery-media size,
two-engine semantic divergence, and real task scheduler resource overhead are
N/A: neither research prototype implements a production parser/lowering/
verification pipeline. Stage 2 must measure them against the accepted contracts;
this report does not substitute an apples-to-oranges proxy.
