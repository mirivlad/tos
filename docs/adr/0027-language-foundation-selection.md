<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0027: Select bespoke TOS Core language foundation

- Status: Proposed — Ready for Project Architect decision
- Date: 2026-08-09
- Decision level: 3 — canonical language semantics, verifier and runtime trust boundary

## Decision

Select a **bespoke TOS Core foundation**, not an unchanged existing language or
an external execution core. The selected boundary is:

- canonical installed programs are normalized UTF-8 human-readable `.tos` text;
- TOS owns lexical/syntactic/type/effect/ownership/concurrency semantics;
- a versioned typed TOS IR is a disposable derivative, validated independently
  before execution;
- the verifier checks types, capabilities, region/ownership rules, resource
  declarations, source maps, structured async/parallel operations, atomics and
  memory-order contracts;
- the bootstrap profile is a bounded serialized execution profile of these same
  semantics; full profile adds bounded structured parallel execution;
- the reference interpreter may serialize parallel tasks, while a
  production-capable backend/runtime must execute them simultaneously on SMP;
- TOS parser, checker, verifier, bootstrap interpreter and minimal task runtime
  form the future language trusted base. rustc, LLVM, libc, C ABI, host thread
  APIs and external VMs are build/research tools unless a later ADR separately
  admits a narrowly defined role.

## Rationale and alternatives

The completed matrix and common 13-case corpus show the bespoke model can state
capability non-forgeability, source maps, bounded resources, safe mutable-share
rejection, atomics, join/cancel and 1/2/4-worker semantics explicitly. The
adapted Rust runner-up demonstrates useful ownership and compiler rejections,
but its necessary restriction/runtime/verifier layer recreates the TOS semantic
boundary while retaining incomplete upstream memory-model and recovery/host ABI
risks. WebAssembly Threads is rejected as a formal-core option because it
requires host thread creation; Pony's actor-only model conflicts with direct
parallel task requirements; unchanged Rust and Go fail their ambient/unsafe or
safe-race/resource boundaries.

## Impact statement

The decision preserves I-01, I-02, I-07, I-10, I-11, I-12, I-16, I-18 and
I-19. No persistent format, boot ABI or existing Stage 1 trusted code changes.
Derived IR/caches remain regenerable and source-addressed. Stage 2 will first
write the normative semantics and a bounded bootstrap frontend/verifier, with
conformance/fuzz/resource tests before a production runtime. Licence remains
GPL-3.0-or-later for official implementation; public schemas/conformance may be
explicitly Apache-2.0. No patent-freedom claim is made.

## Evidence and limitations

Evidence is retained under `docs/research/stage15/`, including raw 3+21
measurements, primary references, screening and both finalist prototypes. It is
not Stage 2 code. The selected approach's main risk is the still-unimplemented
complexity of complete ownership, diagnostics, resource accounting and multiple
engines; acceptance authorizes Stage 2 to implement those contracts, not to
skip them.
