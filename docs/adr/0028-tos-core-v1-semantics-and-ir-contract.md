<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0028: TOS Core V1 semantics and IR contract

- Status: Proposed — ready for Project Architect decision
- Date: 2026-08-09
- Decision level: 2 — versioned language/IR contract within ADR-0027's
  accepted Level 3 foundation boundary

## Context

ADR-0027 selected bespoke TOS Core and fixed its language/trust boundary. It
explicitly assigned the complete lexical, syntactic, type/effect, ownership,
concurrency, module, resource, diagnostic, IR, verifier, and compatibility
contract to Stage 2. Implementing a parser first would make its incidental
choices normative and would risk restoring a hidden Rust/LLVM/libc/C ABI/host
runtime contract.

The proposed numbered specification set is:

- `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`;
- `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`;
- `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`;
- `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`;
- `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`; and
- `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`.

They are one V1 contract: splitting prose does not split authority or allow an
implementation to select only convenient portions.

This resubmission resolves the checkpoint's internal-contract findings without
changing the ADR-0027 foundation: it makes every V1 type form and constructor
arity expressible in grammar; gives control heads an explicit parenthesized
boundary from record initializers; fixes record field-list separation; removes
`nil` as an absence syntax; inventories every identifier-shaped grammar word;
and defines cancellation as a request followed by a consuming
`TaskResult<T>` join/await lifecycle. The companion mechanical consistency gate
checks these boundaries across docs/39–44, canonical examples, and the
conformance corpus.

## Proposed decision

Accept TOS Core V1 as specified by docs/39–44:

- canonical source is normalized UTF-8 NFC/LF `.tos`, bound to source-set,
  path, and SHA-256 source-content identity;
- grammar is deterministic EBNF with explicit parser recovery and no macros,
  ambient imports, pointer syntax, or target-dependent integer defaults;
- tuple types and borrowed `slice<T>` are explicit V1 forms; all predeclared
  synchronization/atomic types have fixed documented arity; control heads are
  parenthesized and record fields are comma-separated so parser boundaries do
  not depend on type resolution;
- static semantics provide nominal types, fixed-width arithmetic, typed
  Result-style errors, capability effects, affine ownership, lexical
  nonescaping borrows, typed regions, and no safe raw-pointer/physical-address
  escape;
- Full execution has structured async and true-SMP-capable structured parallel
  tasks; `join`/`await` consume `Task<T>` into `TaskResult<T>`, so cooperative
  cancellation never conflates with a child `Result` value; Bootstrap is a
  bounded serialized subset of the same semantics;
- safe data races are statically excluded and independently verifier-rejected;
  atomics, synchronization, cancellation, happens-before, and resource
  accounting have TOS-owned semantics;
- module/import resolution is source-set-bound and deterministic; capabilities
  are opaque requests/grants and cannot be forged or widened by source;
- typed `tos-ir/v1` is derived/disposable, verifier-visible, source-mapped,
  and independently validated before execution; and
- diagnostics, provenance/cache identity, conformance, limits, fuzzing,
  performance, and recovery evidence are specified before implementation.

Acceptance authorizes the Part B production reference frontend/verifier/runtime
work in the order stated by docs/44. It does not authorize Stage 3, a C/Rust
FFI, a host runtime semantic shortcut, a persistent IR cache byte format, an
optimized backend, user generics/macros, or a new dependency.

## Architecture impact statement

- **Invariants:** preserves I-01 canonical text, I-02 minimal binary base,
  I-07 explicit authority, I-09 versioned boundaries, I-10 deterministic
  identity, I-11/I-16 observability, I-12 no hidden runtime build dependency,
  I-18 derived provenance, I-19 dependency containment, and I-21 no temporary
  identity debt.
- **Canonical representation:** normalized `.tos` source remains canonical;
  AST/IR/cache/native code remain disposable derivatives.
- **Trusted base:** defines the future TOS parser/checker, independent
  verifier, Bootstrap interpreter, and minimal task runtime; no external
  runtime enters it. Rust remains an implementation/build language only.
- **Source-to-runtime and recovery:** exact source-set/path/content identity
  flows through typed IR, verifier receipt, cache key, diagnostics, and runtime
  events; cache deletion permits source regeneration through bounded recovery
  components.
- **Threat model:** elaborates existing docs/34 language/frontend/cache boundary
  with bounded parsing, forged IR/capability, resource, race, source-map, and
  cache-substitution negative evidence.
- **Performance:** applies docs/35 Stage 1.5–2 parse/check/lower/verify and
  Bootstrap execution measurements without weakening any established budget.
- **Compatibility:** establishes V1 source/profile/IR/verifier versioning and
  rejects unknown versions rather than guessing.
- **Dependencies/licensing/patents:** adds no dependency or external code;
  documents remain CC-BY-SA-4.0, canonical examples GPL-3.0-or-later, and no
  patent-freedom claim is made.
- **Tests:** docs/44 and `docs/language/conformance/v1/` specify backend-neutral
  positives/negatives, forged-IR, multicore, resource, source-map, fuzz, and
  performance evidence before Stage 2 closure.

## Consequences and alternatives

The reference implementation must implement the whole contract incrementally;
it cannot call a host parser/runtime and call that TOS semantics. The narrow
V1 omissions deliberately constrain the first implementation, but all future
extensions remain versioned, verifier-visible, source-mapped, and
resource-accounted.

Keeping grammar/ownership/atomics unspecified until parser code exists was
rejected because it would violate ADR-0015/0027 and make implementation the
de facto language authority. Adopting Rust, Wasm, LLVM, C ABI, libc, or host
threads as the contract was rejected by ADR-0027; they remain possible future
implementation/build/backend tools only under their own accepted decisions.
