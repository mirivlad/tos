<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Execution model and intermediate representation

> ADR-0027 accepts bespoke TOS Core as the language foundation. This document
> specifies the accepted execution boundary; Stage 2 defines the complete IR
> schema and parser/lowering implementation within it.

## Principle

TOS promises canonical text, not mandatory slow interpretation. Execution is a pipeline whose derived stages remain subordinate to source identity.

```text
UTF-8 source
  -> language frontend
  -> syntax tree
  -> semantic analysis
  -> typed TOS IR
  -> verified module image
  -> interpreter, bytecode engine, or native-code backend
```

No generated stage becomes the authoritative installed program.

## TOS IR

TOS IR is a versioned, typed, capability-aware intermediate representation shared by all supported language frontends.

It must represent:

- typed values and control flow;
- functions and calls;
- explicit error edges;
- capability operations;
- IPC send/receive operations;
- memory-region operations;
- async suspension points;
- structured task spawn, parallel task spawn, join and cancellation;
- synchronization operations and atomic operations with their required memory-order semantics;
- typed shared-memory-region access;
- execution-context and resource-accounting operations;
- source maps;
- resource limits;
- module imports and exports;
- driver-specific operations only through typed service contracts.

Parallel semantics MUST remain directly represented or pass through versioned,
typed verifier-visible runtime contracts. They MUST NOT disappear into an opaque
host runtime API that the verifier and resource model cannot understand. A
lowered runtime call is acceptable only when its contract specifies the relevant
task, cancellation, synchronization, atomic, shared-memory and resource
semantics.

TOS IR is not a public promise of permanent binary compatibility between arbitrary versions. Its schema is versioned, and caches state the exact runtime and verifier versions that produced them.

## Verification

Before execution, the verifier checks:

- structural validity;
- type correctness;
- valid control-flow targets;
- no use of undeclared imports;
- capability operation compatibility;
- memory-region bounds rules;
- bootstrap-profile restrictions;
- maximum declared stack and resource limits where required;
- source map consistency.

Invalid IR is never executed, even if loaded from a local cache.

## Cache identity

A generated module cache key includes at least:

```text
source_content_id
language_frontend_content_id
language_version
runtime_abi_version
ir_schema_version
verifier_version
optimization_profile
target_architecture
capability_contract_digest
```

Changing any component invalidates the cache.

## Cache location

Generated artifacts live under `/cache/tos/` or another explicitly disposable cache store. They never appear as required tracked files in `/system`.

## Execution engines

The architecture supports several engines:

1. **Reference interpreter** — simplest auditable semantics; mandatory for tests and recovery. It MAY execute otherwise-parallel work serially only when it preserves the specified semantics.
2. **Bytecode engine** — compact efficient default.
3. **JIT backend** — optional for long-running services and applications.
4. **Ahead-of-use native cache** — generated locally or by a trusted builder, always verified against source identity.

All engines must pass the same conformance suite, including concurrency,
atomic and memory-model vectors. A production-capable backend/runtime MUST have
a path to true simultaneous SMP execution; every engine MUST implement one
language/IR memory semantics rather than silently giving races or atomics
different behavior. Wasm or another binary format may serve as a backend or
cache profile only when canonical text, verifier independence and source
identity remain authoritative.

## Performance contract

Execution engines, parsing, verification and cache validation are measured under `docs/35_PERFORMANCE_CONTRACTS.md`. An optimized engine is accepted only if semantic and provenance conformance remains identical to the reference path.

## Hot activation

A running service may be replaced by a new source revision through a supervisor transaction:

1. parse and verify replacement;
2. start replacement with a new capability set;
3. perform versioned state handoff if supported;
4. route new requests to replacement;
5. drain or cancel old instance;
6. commit activation record;
7. roll back automatically if health checks fail.

Code is not patched in-place inside a process. Replacement preserves clear identity and rollback.

## Source maps

Every executable instruction maps to:

- repository commit;
- path;
- source content ID;
- language frontend ID;
- byte span;
- optional macro expansion chain.

Logs, traces, crashes, and profiling data use this mapping.
For concurrently executing work, the mapping also identifies the originating
task/execution context and the source span of spawned, joined, cancelled or
synchronized work without making scheduler timing part of source identity.

## Determinism

Parsing and lowering must be deterministic for identical inputs and declared environment. Frontends cannot read time, network, random state, or untracked files while producing IR unless such inputs are explicitly part of the cache key and build record.

## Provenance contract

Every IR or executable cache object is keyed by more than source text alone. The cache identity includes:

- normalized source object IDs and dependency closure;
- source commit or detached source-set identity;
- frontend implementation and semantic profile;
- IR schema and verifier version;
- execution backend and target ABI;
- optimization and safety policy;
- capability import contract.

The runtime refuses stale or ambiguous caches. Deleting all cache stores must leave a recoverable, functionally complete system, subject only to regeneration time.

## Backend neutrality

A backend such as an interpreter, bytecode VM, Wasm engine or native compiler may be used without becoming canonical. Backend adoption is reviewed separately from source-language adoption. External engines default to isolated services or test oracles until an ADR accepts their trust and dependency consequences.
