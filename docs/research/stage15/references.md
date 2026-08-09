<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 external-source bibliography

All links were accessed 2026-08-09. A cited implementation remains a research
input under accepted ADR-0011 unless a later accepted ADR admits it to a TOS
runtime role.

| ID | Primary source, version/context | Claim used in this evaluation |
|---|---|---|
| R1 | [Rust Reference: memory model](https://doc.rust-lang.org/reference/memory-model.html), current reference | Rust explicitly says its general memory model is incomplete/not fully decided; an adapted profile must state its own TOS semantic boundary. |
| R2 | [Rust `core::sync::atomic` 1.97.1](https://doc.rust-lang.org/stable/core/sync/atomic/), host toolchain `rustc 1.97.1 (8bab26f4f 2026-07-14)` | Rust offers typed atomic primitives and documents C++20-style orderings; atomic sharing is safe when used through the typed API. |
| R3 | [Rust `std::thread` 1.97.1](https://doc.rust-lang.org/stable/std/thread/), accessed with host toolchain version above | Rust provides OS-thread/scoped-thread primitives, message passing and shared synchronization. They are not a bounded TOS task/resource policy by themselves. |
| R4 | [Rust license policy](https://www.rust-lang.org/policies/licenses) | Rust compiler/runtime source is dual Apache-2.0/MIT; using it as a build/research tool does not automatically make it a TOS runtime dependency. |
| W1 | [WebAssembly 2.0 + Threads specification](https://webassembly.github.io/threads/core/), draft 2023-10-10 | Wasm is a typed, validated binary execution format with shared memories and atomic operations. It is not canonical human-readable TOS source. |
| W2 | [WebAssembly Threads change history](https://webassembly.github.io/threads/core/appendix/changes.html), draft 2023-10-10 | The Threads proposal supplies shared memory and atomics, but thread creation is handled by the host. |
| P1 | [Pony reference-capability tutorial](https://tutorial.ponylang.io/reference-capabilities/reference-capabilities.html) | Pony reference capabilities statically distinguish isolated mutable and immutable shareable data. |
| P2 | [Pony actor model tutorial](https://tutorial.ponylang.io/types/actors.html) | Every Pony actor is single-threaded; data is shared across actors by immutable/isolated transfer. |
| P3 | [Pony runtime FAQ](https://www.ponylang.io/faq/runtime/) | Pony normally starts one actor scheduler thread per available CPU, demonstrating genuine host multicore scheduling but not direct parallel execution contexts within an actor. |
| P4 | [Pony compiler source licence](https://github.com/ponylang/ponyc/blob/main/LICENSE) | Upstream licence status is reviewed as part of dependency analysis; no Pony runtime is admitted by this research. |
| G1 | [Go memory model](https://go.dev/ref/mem), 2022-06-06 version | Go describes happens-before and DRF-SC, but says races on multiword values can produce inconsistent values and arbitrary memory corruption. |
| G2 | [Go data-race detector](https://go.dev/doc/articles/race_detector) | Go's race detector is optional diagnostic tooling rather than static prevention. |
| F1 | [Wasm SpecTec paper and project record](https://arxiv.org/abs/2311.07223) | A formal core can improve executable-specification evidence, but that does not itself supply a TOS source, capability or task model. |

## Licence and patent treatment

This bibliography identifies licence sources and externally stated behaviour;
it is not legal advice. No patent clearance is inferred from upstream adoption,
and no external implementation is copied into a TOS prototype. The candidate
reports distinguish a build/reference tool from a prospective runtime
dependency as required by `docs/27_THIRD_PARTY_COMPONENT_POLICY.md`.
