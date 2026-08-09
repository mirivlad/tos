<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — typed IR, verifier, and provenance

- Status: **Proposed Stage 2 contract — not implementation authority**
- IR semantic schema: `tos-ir/v1`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md` through
  `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`

## 1. Role and representation boundary

TOS IR is a versioned, typed, verifier-visible **derived** representation of
TOS Core source. It is never canonical installed source, a substitute recovery
language, or a promise of permanent binary compatibility. A source frontend
lowers normalized `.tos` source deterministically to `tos-ir/v1`; an
independently built verifier validates that IR before any interpreter, bytecode
engine, native backend, or cache executor uses it.

This document defines the semantic schema. It deliberately does not freeze an
on-disk byte encoding before a production cache exists. Any persisted `tos-ir`
object must, before being introduced, receive a bounded versioned format
specification with magic, schema/encoding version, length limits, canonical
encoding, unknown-field behavior, digest, and parser tests under docs/18. That
format is an implementation/storage detail only if it preserves this semantic
schema and is checked by the independent verifier. The absence of a cache
encoding cannot delay source execution or make a binary cache canonical.

## 2. Module schema

An IR module contains the following logical sections in canonical order:

```text
Header
  schema_id = "tos-ir/v1"
  language_version = "1.0"
  profile = bootstrap | full
  module name, source-set identity, path, normalized source content ID
  dependency-closure digest, frontend identity, source-map revision
  declared resource envelope and imported capability-interface digest
Types
Imports and exported signatures
Constants
Functions, ordered by fully qualified source name
Source-map entries, ordered by source unit then byte start/end
```

All strings are normalized UTF-8 and all identifiers/paths obey docs/39/42.
Tables use explicit bounded indexes; no operation encodes a raw host pointer,
host ABI symbol, implicit global capability, or untyped runtime object.
Every table count, byte length, basic-block count, operand count, nesting
depth, and source-map span is bounded by the module resource contract and the
frontend/verifier hard limits from docs/44.

The type table represents exactly the primitive, nominal aggregate, function,
task, capability, region, synchronization, atomic, and approved constructed
types of TOS Core V1. A nominal type records its defining module content ID and
export name. An IR type ID is not valid merely because its host representation
has the same layout.

The IR does not trust a frontend-supplied `Copy` annotation. It recomputes the
docs/40 rule from the ordered type graph: primitive Copy roots and `Shared<T>`
are Copy; tuple/array types are Copy only when every component is Copy; user
records, user enums, `Option`, `Result`, and `TaskResult` are non-Copy in V1.
All other V1 types are non-Copy. This check is part of affine operand
validation.

For constructed types, IR records the same exact arity as docs/39/40:
`Option`, `Task`, `TaskResult`, `Shared`, `Region`, `DmaRegion`, `Mutex`,
`RwLock`, `Channel`, and `slice` have one type argument; `Result` has two;
`Event`, `Semaphore`, `Barrier`, `Latch`, and the three V1 atomic types have
none, as does `ConversionError`. The verifier rejects a forged or mismatched
arity before control-flow or runtime-contract validation.

## 3. Functions, values, and control flow

Each function has an exact type/effect signature, ordered parameters, return
type, source span, maximum declared stack/fuel/cleanup contribution, and a
finite ordered sequence of basic blocks. A block has typed parameters and ends
in exactly one terminator:

```text
return(value?)
branch(target, arguments)
branch_if(condition, true_target, false_target, arguments)
match_enum(subject, complete variant-to-target map)
propagate_error(result)
trap(stable runtime code)
```

Values are typed SSA definitions or explicit affine ownership slots. An operand
can only reference a dominating value/slot under the corresponding ownership
state. There is no implicit fall-through, untyped jump, exception edge, host
stack unwinding, or unbounded recursion edge. A call names a declared imported
or local function signature and supplies an exact ordered operand list; it
cannot resolve a host symbol dynamically.

The frontend lowers every source `name(...)` through one resolved call or
construction family. For a nominal record constructor it first validates the
source-order named arguments against the declared ordered field set, then emits
the corresponding ordered aggregate operands; ordinary functions and tuple
variants accept positional operands only. An IR `return(value)` is the only
normal non-unit function/task/closure result; source blocks, `if`, and `match`
do not lower as value-producing expressions.

The semantic operation families are:

| Family | Required verifier-visible properties |
|---|---|
| constants/aggregate construction | exact type, checked literal range, source map |
| arithmetic/comparison/control | typed operands/results, checked/trap behavior, complete branch targets |
| move/borrow/drop | affine state, borrow exclusivity, bounded cleanup/drop contract |
| Result/error | declared `Ok`/`Err` construction and `?` propagation edge |
| capability | declared imported capability, effect/right/interface match, no construction from scalar data |
| region/DMA | typed grant, rights, checked range/alignment, transfer/share rule, no physical-address exposure |
| resource | reserve/release/check fuel, stack, allocation, task, worker, sync, shared, cleanup, recursion/import bounds |
| async/parallel | scoped spawn, typed captures, affine `Task<T>` token, `TaskResult<T>` await/join result, cancellation request, and scope completion |
| synchronization | typed mutex/RW/channel/event/barrier/latch operation and guard lifetime |
| atomic | exact atomic type, legal operation/order, source map and memory-order contract |
| unsafe/extern | explicit unsafe marker, accepted interface ID, capability/effect/resource contract |

An operation that lowers to a runtime call carries a versioned typed runtime
contract ID and all semantic operands: capability/effect, ownership transfer,
resource reservation, cancellation point, synchronization/atomic order, and
source span. It MUST NOT hide task creation, locking, atomics, shared-memory
access, resource allocation, privilege, or an external host ABI behind an
opaque helper call.

## 4. Lowering boundary

The frontend proves syntactic well-formedness, name resolution, source-level
type/effect checks, lexical ownership checks, profile eligibility, and source
span attachment before it emits IR. Lowering is deterministic: identical
declared inputs yield semantically identical ordered IR tables and mapping
records. The frontend may optimize only when the resulting typed IR preserves
the source evaluation, ownership, capability, resource, atomic, and source-map
semantics.

The verifier does not trust those claims. In particular, the verifier rechecks
all table bounds/schema identity, nominal type references, control-flow targets,
operand types, call/effect signatures, import/capability declarations, affine
value/borrow state, region rights, profile restrictions, resource accounting,
task scope/capture/join/cancel/`TaskResult<T>` behavior, synchronization guard rules, atomic
orders, unsafe interface IDs, and source-map identity/spans. A frontend cannot
mark an arbitrary cache "verified." Only the verifier emits a verified-module
receipt bound to the complete module digest and verifier identity.

## 5. Independent verifier contract

The verifier consumes untrusted IR bytes/in-memory structures plus a declared
module-resolution and capability-interface snapshot. It produces either:

```text
VerifiedModule {
  module digest, schema_id, verifier identity, source/dependency identities,
  profile, effective resource envelope, capability-interface digest,
  checked source-map digest
}
```

or one deterministic primary `V20xx` diagnostic with optional causal entries.
An engine accepts executable IR only with a receipt for the exact module digest,
schema, source/dependency closure, effective resource envelope, capability
contract digest, and engine compatibility range.

Verifier independence is structural: it is a separately buildable component
with its own parser/validation traversal and does not consume a frontend AST,
type-checker success flag, or host compiler validation result as proof. A
shared declarative type/interface table may be used only if its content digest
is input to both components; no frontend callback participates in verifier
acceptance. An alternate/optimized frontend remains untrusted at this boundary.

Primary validation order is:

1. envelope/byte/table-count limits;
2. schema/version/header/source identity;
3. canonical ordering and index/reference range;
4. nominal types/signatures/imports/capability interfaces;
5. control flow and typed operands;
6. ownership/regions/effects/profile/resources;
7. tasks/synchronization/atomics/unsafe contracts; then
8. source maps and cache/provenance binding.

Representative stable errors are `V2001_LIMIT`, `V2002_SCHEMA`,
`V2003_SOURCE_IDENTITY`, `V2004_TABLE_ORDER`, `V2010_TYPE`, `V2011_CFG`,
`V2012_IMPORT`, `V2013_CAPABILITY`, `V2020_OWNERSHIP`, `V2021_REGION`,
`V2022_RESOURCE`, `V2023_PROFILE`, `V2030_TASK_SCOPE`, `V2031_SYNC`,
`V2032_ATOMIC_ORDER`, `V2033_UNSAFE`, and `V2040_SOURCE_MAP`.

## 6. Source maps, cache identity, and observability

Every IR operation has a source-map entry containing source-set identity,
canonical path, normalized source content ID, frontend identity, language
version/profile, byte start/end, and optional derivation parent span. Spawn,
join, cancellation, synchronization, and atomic operations also carry a task
or execution-context event identity at runtime; timing and CPU number are
observations, not part of source identity.

A derived cache key contains at least:

```text
normalized source-content IDs and ordered dependency closure
source-set/commit or detached source-set identity
canonical path/module identity
frontend implementation and semantic-profile identity
language version and feature revision
IR schema and source-map revision
verifier implementation identity
backend implementation and target ABI identity
optimization and safety policy identity
resource-envelope digest
capability-interface contract digest
```

The runtime records this identity with a running component. An identity mismatch
or missing source map rejects cache execution rather than trying a nearby source
or host fallback. Removing all cache objects leaves the canonical source tree,
declared dependencies, frontend/verifier/runtime, and recovery path able to
regenerate functionality, subject only to declared bounded work and time.

## 7. Execution engines and semantic equivalence

The reference interpreter, a future bytecode engine, and a future native/JIT
backend execute the same verified IR and TOS-owned memory semantics. The
Bootstrap interpreter may serialize parallel scopes, but must produce an
allowed result under docs/41 and retain each resource/cancellation rule. A
production-capable Full engine has a real SMP mapping from runnable parallel
tasks to bounded execution contexts. No engine may defer semantics to an
undocumented host ABI or silently give atomics/races different behavior.

Every engine must pass the same relevant conformance vectors. A backend,
including Wasm/LLVM/Cranelift, can only be a derived cache/codegen mechanism
after a separate accepted ADR admits its bounded role. It does not replace the
frontend, verifier, source maps, capability contract, or recovery semantics.
