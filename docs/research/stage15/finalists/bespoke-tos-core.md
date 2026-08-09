<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Finalist A — bespoke TOS Core

## Boundary evaluated

Canonical input is normalized UTF-8 `.tos` text. A TOS-owned frontend lowers it
deterministically to a versioned typed IR; a separately buildable TOS verifier
checks structure, types, capabilities, regions, resource declarations, source
maps and concurrency operations before any engine executes it. IR/native code
are disposable caches. Bootstrap is a bounded, serialized profile of the same
semantics; a production backend maps verifier-visible scoped parallel tasks to
bounded nucleus execution contexts.

## Common evidence

`prototypes/bespoke/model.rs` covers all 13 cases. It models private
verifier-issued MMIO tokens, fixed task/worker bounds, typed IR operations,
source spans, deterministic cache identity, static mutable-share rejection,
release/acquire publication, structured cancellation and a serial/parallel
fixed-order reduction. Its one/two/four-worker raw data is retained in
`measurements/bespoke-*.json`; 2/4 workers have `overlap=true` and all modes
produce `stage15-common-v1-d000032aaaa80000`.

## Fit and risk

This is not scored for being small today. The proposed TCB is the future TOS
lexer/parser, semantic checker, IR verifier, bounded bootstrap interpreter and
minimal task runtime—not the 275-line experiment. The experiment proves that
the required semantics can be made explicit, but does not erase the substantial
Stage 2 work: full ownership/region typing, diagnostics, frontend grammar,
resource accounting and two independently conforming engines remain required.
No external compiler, LLVM, C ABI, libc or host runtime is part of the TOS
semantic contract. A host compiler can be a build tool only.

The approach preserves capability security, source attribution, address-width
independence, affinity/NUMA evolution and driver-facing MMIO/IRQ/DMA contracts.
Its dominant risk is implementation and audit cost, mitigated by a deliberately
narrow bootstrap profile, IR verifier and conformance-first Stage 2 plan.
