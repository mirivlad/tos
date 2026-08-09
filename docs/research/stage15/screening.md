<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 candidate screening

**Assessment date:** 2026-08-09  
**Baseline:** `345fa8c10a3da0715a3c24eb37327ff3277bedc7`  
**Method:** all blocking requirements in
`docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md` apply before any
comparative scoring. A candidate is not rejected for being unfamiliar or hard;
it is rejected only for a recorded technical blocking failure.

## Classes and candidates considered

| ID | Class | Candidate | Screening result | Exact reason |
|---|---|---|---|---|
| A | Bespoke TOS Core | A deliberately specified TOS textual language, typed IR and bounded reference interpreter/runtime model | **Finalist** | It can directly make canonical text, capability tokens, resource accounting, source maps and structured parallelism one contract. Its viability must still be proven by a non-production prototype rather than assumed. |
| B | TOS textual surface over formal core | TOS surface lowered to WebAssembly 2.0 + Threads | **Rejected** | Wasm is a binary execution format, not canonical source; more decisively, its Threads specification says thread creation is handled by the host (W2). Supplying structured parallel spawn, capability/resource contracts and source semantics in the surface/runtime would recreate the bespoke foundation rather than use an independently sufficient core. |
| C | Adapted existing language | Restricted Rust 1.97.1 profile lowered through typed TOS IR | **Finalist** | Ownership, `Send`/`Sync`, scoped threads and typed atomics can model safe shared-memory parallelism. The adaptation must prove a no-ambient-authority bootstrap profile, verifier-visible capabilities/resource quotas and a defined TOS memory contract; Rust's incomplete general memory model (R1) is a material risk, not ignored. |
| D | Existing language unchanged | Rust with its ordinary standard/runtime ecosystem | **Rejected** | Unchanged Rust exposes `std` ambient filesystem/time/network APIs, an explicit general `unsafe` escape hatch and no TOS capability grants, cache verifier or bounded task policy. Removing those is an adaptation, so D cannot truthfully claim an unchanged language. |
| D | Existing language unchanged | Pony | **Rejected** | Pony has valuable static reference capabilities and host multicore scheduling (P1/P3), but each actor is single-threaded and safe sharing is expressed through actors/immutable or isolated transfer (P2). This violates the required direct structured parallel work model: actors/messages cannot be the only way to use multiple cores. |
| D | Existing language unchanged | Go | **Rejected** | Go has goroutines and an explicit memory model, but its own model permits races on multiword values to lead to arbitrary memory corruption (G1); race detection is optional tooling (G2). It also lacks the required static capability/region/verifier/resource contracts unchanged. |

## Serious finalists

The finalists are **A: Bespoke TOS Core** and **C: Adapted restricted Rust**.
They will run exactly the same common corpus, including serial/parallel modes,
static rejection of unsafe mutable sharing, atomic publication,
structured-cancellation and bounded-task evidence. The screening rejects do not
receive bespoke substitute implementations: doing so would manufacture evidence
for a different candidate.

## Preliminary non-decision

No foundation is selected by this screening. A bespoke foundation may fail if
its prototype cannot give a credible bounded memory/concurrency/verifier model;
an adapted Rust profile may fail if its necessary restriction/runtime layer is
too large, too host-dependent or semantically divergent for recovery. The
final decision awaits the same evidence corpus and measurements for both.
