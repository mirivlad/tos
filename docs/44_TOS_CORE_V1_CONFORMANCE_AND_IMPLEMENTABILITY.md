<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — conformance, limits, and implementation review

- Status: **Proposed Stage 2 contract — not implementation authority**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md` through
  `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`

## 1. Conformance model

TOS Core conformance is backend-neutral. A conforming frontend accepts/rejects
the normalized source corpus with the specified primary code, stage, path,
content ID, span, and required structured fields. A conforming lowerer emits
semantically equivalent `tos-ir/v1`; a conforming verifier independently
accepts valid IR and rejects forged/malformed IR; a conforming engine produces
an allowed V1 outcome without relying on host language/ABI behavior.

The initial corpus is retained under `docs/language/conformance/v1/`. It is
proposed source/conformance evidence only until implementation begins. Each
case has a stable identifier, canonical `.tos` input, profile, expected result
or primary diagnostic, source span, and semantic rationale. An implementation
MUST NOT change an expectation merely because its parser/checker finds a more
convenient error.

| Vector class | Required initial evidence |
|---|---|
| lexical/source | UTF-8, BOM, NFC, CRLF/bare-CR, tab, identifier, integer, string/bytes, and earliest-error precedence |
| grammar | module/header/import, declaration/block recovery, parenthesized statement-only `if`/`match`, one Call/constructor form, `[]` declarative lists, named record constructors, tuple/slice/predeclared-type arity, precedence, complete match, reserved words, invalid profile syntax |
| static type/evaluation | fixed-width literals, `to_*` checked conversion and invalid narrowing, checked overflow/shift/division, Result `?`, `Option` (not `nil`), evaluation order |
| ownership | move/use-after-move, primitive/tuple/array Copy and affine nominal aggregate rule, immutable/mutable conflict, borrow escape, indexed alias conservatism, task capture |
| capabilities | undeclared effect, forged handle, denied request, invalid attenuation/transfer, untyped privileged operation |
| resources | missing/invalid required limit, metered loop, recursion/import/task/worker/sync/shared/cleanup exhaustion |
| concurrency | one/2/N-worker equivalent deterministic result, actual Full-engine overlap, safe mutable-share rejection, `TaskResult` join/cancel lifecycle, bounded task/worker behavior |
| synchronization/atomics | mutex/channel/event/barrier ordering, valid/invalid memory order, release/acquire publication, no non-atomic race escape |
| modules/provenance | deterministic import closure, cycle/ambiguity rejection, cache invalidation, source-map preservation through lowering/optimization |
| IR verifier | malformed header/table/order/index/type/CFG/import/capability/region/resource/task/atomic/source-map negatives |
| profiles/unsafe/FFI | Bootstrap reject Full-only constructs, serialized Bootstrap equivalence, unsafe rationale and unavailable FFI rejection |

For Full engines, the required multicore exercise partitions a deterministic
CPU-bound workload. It records 1-worker, 2-worker, and reasonable-N-worker
correct results plus actual overlapping CPU work on multiple host cores. The
same vector runs in serialized Bootstrap/reference mode. Speedup is evidence
of viability, not a selection or correctness score. A negative shared-mutable
case, atomics/synchronization case, structured join/cancel case, and bounded
task/worker case are mandatory; overlap alone is insufficient.

## 2. Frontend, verifier, and runtime hard limits

The production implementation MUST publish exact numeric limits before it
accepts untrusted source/IR. They may be no larger than this proposed V1
ceiling without a contract extension:

```text
normalized source unit             256 KiB
module dependency closure          256 modules
module/import graph depth          64
identifier bytes                   128
string/bytes literal bytes         64 KiB
delimiter nesting                  256
record/enum fields or variants     1024
function parameters                128
diagnostics retained per module    256
IR tables/blocks/instructions      bounded by declared module resource envelope
```

The frontend and verifier check gross byte/count/depth limits before expensive
normalization, graph traversal, type work, lowering, or source-map copying
where structurally possible. A limit error takes precedence over later
semantic errors when its triggering bound is encountered first. Limits prevent
attacker-controlled recursion, quadratic name/module work, unbounded source
duplication, and cache amplification; they are not optional implementation
quality targets.

Any lower cap is allowed if reported in the implementation's declared
conformance profile. Raising a ceiling, changing a rejection precedence, or
accepting a new syntax/IR feature is a versioned contract change with vectors.
The reference parser/verifier remains total over arbitrary bytes and returns
structured errors rather than panicking.

## 3. Required threat and adversarial evidence

This contract extends the existing `docs/34_THREAT_MODEL.md` language/runtime
boundary (T3 malicious frontend/cache producer and T1/T2 resource abuse). It
adds no claim that a language checker defeats malicious firmware, a compromised
nucleus, or all denial of service. Stage 2 implementation evidence MUST cover:

- malformed UTF-8/source and malformed/forged IR fuzzing without parser panic;
- source normalization/path/import ambiguity and cache-substitution negatives;
- capability forgery/widening/ambient-authority negatives;
- ownership/data-race/atomic-order invalid cases;
- resource exhaustion before allocation/worker creation and cancellation
  cleanup bounds;
- source-map identity forgery/mismatch; and
- cross-engine semantic differential testing for every supported engine.

Evidence levels remain those in docs/34: the proposed documents are E0 design;
implemented parser/verifier paths become E1; automated positives/negatives E2;
fuzz/fault evidence E3. No Stage 2 closure claim may elevate a design contract
without the corresponding implementation evidence.

## 4. Performance and recovery evidence

The Stage 1.5–2 contracts in `docs/35_PERFORMANCE_CONTRACTS.md` apply. The
production reference profile must measure parse/type-check/lower/verify a
256 KiB canonical module and the one-million-operation integer/control-flow
benchmark with the required environment, warmups, raw samples, median/p95/p99,
memory, source/build identity, and cache state. Measurements cannot move work
into a host runtime, native cache, nucleus, or an unchecked frontend to claim a
pass.

The recovery/Bootstrap measurement records source size, parser/checker/verifier
and interpreter binary/component sizes, dependencies, dynamic dependencies,
peak memory, cold start, resource envelope, and all host/build tool identities.
Rust may implement those components, but rustc/LLVM/libc/C ABI/host threads are
not recovery/runtime dependencies unless a future ADR explicitly admits them.
The system must be able to delete all derived caches and regenerate from source
using the declared recovery components.

## 5. Implementability review

The contract makes the following deliberate complexity choices:

| Risk | V1 containment |
|---|---|
| parser ambiguity / error recovery | ASCII identifiers, no indentation semantics, no block comments/macros, deterministic EBNF and fixed recovery tokens |
| pathological source / graph | byte, nesting, identifier, diagnostic, closure and import limits with early rejection |
| type/ownership complexity | nominal non-generic types; lexical nonescaping borrows; affine ownership; conservative indexed aliasing |
| capability forgery | opaque nominal imports, effect checking, no scalar representation, independent IR checks |
| concurrency complexity | no detached tasks; lexical scopes; ownership transfer; typed visible synchronization/atomics; Bootstrap serialization |
| resource amplification | mandatory module envelope, reservation before action, bounded cleanup and worker/task count |
| verifier capture | separate build/traversal, no frontend AST-success trust, typed runtime contracts visible in IR |
| source-map loss | identity/span required in every IR operation/cache receipt, verifier checks |
| future native backend | typed IR and explicit checked/atomic/capability semantics; backend cannot redefine them |

Known non-goals are intentional: V1 has no user generics/traits, textual macros,
reflection, implicit ambient prelude, unscoped tasks, stop-the-world collector,
ordinary C ABI, or Stage 3 IPC/driver service API. Their absence does not mean
the contract is temporary: extensions must be versioned, typed, source-mapped,
resource-accounted, verifier-visible, and compatible with the established safe
memory/concurrency boundary.

## 6. Recommended Part B implementation order

After explicit acceptance of ADR-0028, the production order is:

1. bounded normalized source reader and lexer with lexical vectors/fuzzing;
2. deterministic parser and recovery diagnostics;
3. names/types/effects and stable diagnostic records;
4. affine ownership/borrow and module/resource checks;
5. deterministic lowering to the in-memory `tos-ir/v1` semantic schema;
6. independently buildable verifier and forged-IR negatives;
7. bounded serialized Bootstrap reference interpreter;
8. source maps, cache identity/deletion/regeneration and resource accounting;
9. corpus/fuzz/differential/performance evidence; then
10. execute real `/system/boot/init.tos` only after its source conforms.

This order does not authorize a second implementation path. The first parser,
checker, IR, verifier, and interpreter are the intended long-term reference
components; optimized backends remain subordinate derived engines.

## 7. Open matters outside this proposal

There are no unresolved semantic questions needed to begin the intended
Bootstrap reference implementation if ADR-0028 is accepted. Deliberately
deferred, separately versioned contracts are: persistent IR byte encoding;
concrete Stage 3 capability/IPC/MMIO/IRQ/DMA interface schemas; the exact
future FFI ABI; user generics/traits/macros; detached tasks/supervisor API;
NUMA/affinity API; and bytecode/native backend admission. None is silently
provided by a host implementation, and none blocks Bootstrap source semantics.
