<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — conformance, limits, and implementation review

- Status: **Accepted Tier 2 contract — production implementation in progress**
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
accepted source/conformance contract evidence until implementation begins. Each
case has a stable identifier, canonical `.tos` input, profile, expected result
or primary diagnostic, source span, and semantic rationale. An implementation
MUST NOT change an expectation merely because its parser/checker finds a more
convenient error.

| Vector class | Required initial evidence |
|---|---|
| lexical/source | UTF-8, BOM, Unicode 17.0.0/UAX #15 Revision 57 NFC, CRLF/bare-CR, tab, identifier, integer, string/bytes, and earliest-error precedence |
| grammar | module/header/import, declaration/block recovery, parenthesized statement-only `if`/`match`, one Call/constructor form, `[]` declarative lists, named record/named-variant constructors, `fn (...) { ... }` closures, `array<T, N>`, no standalone block expression, precedence, complete match, reserved words, invalid profile syntax |
| type resolution | unknown local type, unknown qualified type where the import and module resolve, `Option` and `Result` applied with the wrong arity, and the precedence of an unresolved name over an arity finding (ADR-0034) |
| static type/evaluation | fixed-width literals, `to_*` checked conversion and invalid narrowing, checked overflow/shift/division, Result `?`, `Option` (not `nil`), evaluation order |
| pattern resolution | local bare unit variant, bare binding where the expected type has no such variant, two enums sharing a variant name disambiguated by expected type, payload variant destructuring, explicitly qualified local variant, qualified imported variant, unknown qualified variant, exhaustive match over bare variants, wildcard and binding exhaustiveness, and independence from capitalization (ADR-0033) |
| ownership | move/use-after-move, primitive/tuple/array Copy and affine nominal aggregate rule, immutable/mutable conflict, borrow escape, indexed alias conservatism, task capture |
| capabilities | undeclared effect, forged handle, denied request, invalid attenuation/transfer, untyped privileged operation |
| resources | missing/invalid required limit, metered loop, recursion/import/task/worker/sync/shared/cleanup exhaustion |
| concurrency | one/2/N-worker equivalent deterministic result, actual Full-engine overlap, safe mutable-share rejection, `TaskResult` join/cancel lifecycle, bounded task/worker behavior |
| synchronization/atomics | mutex/channel/event/barrier ordering, valid/invalid memory order, release/acquire publication, no non-atomic race escape |
| visibility | an exported type in a public signature, an imported exported type across a real module boundary, private types confined to a body or a private item, a private type named directly by a `pub fn`, and a private type reached transitively through an exported wrapper |
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
accepts untrusted source/IR. They may be no larger than this accepted V1
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
- Unicode 17.0.0/UAX #15 Revision 57 NFC conformance, including generated-data
  provenance/hash verification and NormalizationTest.txt-derived cases;
- source normalization/path/import ambiguity and cache-substitution negatives;
- capability forgery/widening/ambient-authority negatives;
- ownership/data-race/atomic-order invalid cases;
- resource exhaustion before allocation/worker creation and cancellation
  cleanup bounds;
- source-map identity forgery/mismatch; and
- cross-engine semantic differential testing for every supported engine.

Evidence levels remain those in docs/34: the accepted documents are E0 design;
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

## 7. Diagnostic code registry

This section is the authoritative registry that `docs/41` section 7 refers to.
It enumerates every frontend diagnostic code reachable by the source reader,
lexer and parser, with its stage and the exact condition that raises it. A code
used by a conformance expectation must appear here; the mechanical gate in
`scripts/check-stage2-language-contract.py` enforces that in both directions.

Codes are allocated by the document that owns the rule — `docs/39` for lexical
and grammatical conditions, ADR-0032 for the parser codes it ratified. This
section records them in one enumerable place; it does not create authority the
owning document did not grant.

Human wording may improve. Code, stage and condition are stable for TOS Core 1.0
and change only through a versioned language decision.

<!-- stage2-diagnostic-registry:start -->

### Source transport (stage `lex`)

| Code | Condition |
|---|---|
| `E1000_SOURCE_LIMIT` | the source unit exceeds the 256 KiB ceiling; reported at the first excluded byte, before UTF-8 and NFC work |
| `E1001_INVALID_UTF8` | the input is not valid UTF-8; reported at the first invalid byte, before normalization |
| `E1002_BOM_FORBIDDEN` | the input begins with a UTF-8 byte order mark; reported at byte 0 |
| `E1003_BARE_CR` | a CR appears that is not part of a CRLF pair; reported at that byte |
| `E1004_NOT_NFC` | the input is not NFC under UCD 17.0.0 and UAX #15 Revision 57; reported at the first non-NFC sequence |
| `E1005_NUL_FORBIDDEN` | a NUL scalar value appears in otherwise valid source; reported at that byte |

### Lexical (stage `lex`)

| Code | Condition |
|---|---|
| `E1010_TAB_OUTSIDE_LITERAL` | a horizontal tab appears outside a literal or comment |
| `E1011_NON_ASCII_WHITESPACE` | a non-ASCII whitespace scalar value appears outside a literal or comment |
| `E1012_INVALID_IDENTIFIER` | a non-ASCII scalar value appears outside a literal or comment, where only an ASCII identifier could be formed; reported at its first byte |
| `E1013_UNEXPECTED_CHARACTER` | a valid UTF-8 character outside a literal or comment neither begins nor continues any admissible lexical form at its position, and is not covered by `E1012_INVALID_IDENTIFIER`; reported at its first byte |
| `E1020_INVALID_INTEGER_LITERAL` | an integer literal has an invalid base digit, a leading or trailing underscore, repeated underscores, or an invalid suffix |
| `E1030_INVALID_STRING` | a string literal has an invalid escape, an invalid scalar value, an unescaped line ending, or no terminator |
| `E1031_INVALID_BYTES` | a `bytes` literal contains a character or escape outside the permitted ASCII set, or has no terminator |

`E1012` and `E1013` are mutually exclusive by construction: a non-ASCII scalar
value takes `E1012`, and every other character that begins no lexical form —
necessarily ASCII, such as `@`, `$`, `#`, `` ` ``, `'` or `\` — takes `E1013`.

### Parser (stage `parse`)

| Code | Condition |
|---|---|
| `E1100_EXPECTED_MODULE_HEADER` | a required module-header keyword (`module`, `version`) is absent at its position |
| `E1101_EXPECTED_IDENTIFIER` | an identifier is required at this position and the token present is not one |
| `E1102_EXPECTED_VERSION_COMPONENT` | a module-header version component is not a decimal integer representable as `u32` |
| `E1103_EXPECTED_PROFILE` | the module-header profile is neither `bootstrap` nor `full` |
| `E1104_EXPECTED_LITERAL` | a literal is required at this position and the token present is not one |
| `E1105_CONTROL_HEAD_PARENS_REQUIRED` | an `if`, `while`, `match` or `for` head is not parenthesized |
| `E1106_LIST_SEPARATOR_REQUIRED` | two members of a comma-separated list are not separated by a comma |
| `E1107_UNEXPECTED_TOKEN` | the token cannot begin or continue the construct being parsed and no more specific parser code applies |

### Type and evaluation (stage `type`)

| Code | Condition |
|---|---|
| `E1201_ASSIGN_TO_IMMUTABLE` | an assignment targets a place whose root binding is not mutable |
| `E1202_UNKNOWN_VALUE_NAME` | a value name, or a qualified constructor path in a pattern, resolves to no predeclared value, module item, parameter or in-scope binding |
| `E1203_UNKNOWN_TYPE_NAME` | a type name resolves to no primitive, fixed or predeclared type, local nominal type or reachable imported type; for a qualified name the module or import part resolved first |
| `E1204_TYPE_ARGUMENT_ARITY` | a known parameterized V1 type constructor is applied to the wrong number of type arguments; fields carry the constructor and both arities |
| `E1205_DUPLICATE_RECORD_FIELD` | a named field list declares or supplies the same field name more than once |
| `E1206_MISSING_RECORD_FIELD` | a named constructor omits a field its record or named-field variant declares |
| `E1207_UNKNOWN_RECORD_FIELD` | a named constructor supplies a field its record or named-field variant does not declare |
| `E1222_RETURN_TYPE_MISMATCH` | a `return` carries a value whose type is not the declared result type, or omits a value in a non-`unit` function |
| `E1225_INVALID_DEFER` | a `defer` body performs `return`, `break`, `continue`, `await`, `join`, spawns work, or acquires a new resource |
| `E1210_INTEGER_TYPE_MISMATCH` | a value of one integer type is assigned or passed where a different integer type is required; an unsuffixed literal takes the required type instead |
| `E1211_INDEX_TYPE_MISMATCH` | an array, slice or region index is not of exact type `size`, and is not an integer literal contextually typed as one |
| `E1212_INVALID_AS_CONVERSION` | an `as` conversion is not an integer widening that preserves signedness; a cast of an opaque handle is routed elsewhere by docs/40 section 3 and is not this code |
| `E1220_NONEXHAUSTIVE_MATCH` | a `match` over an enum, `Option`, `Result` or `TaskResult` leaves a variant uncovered and has no wildcard or binding arm |
| `E1221_MISSING_RETURN` | control can reach the end of a function whose declared return type is not `unit`, or of a closure or spawned body that returns a value on another path |

### Module and version (stage `type`)

| Code | Condition |
|---|---|
| `E1601_UNSUPPORTED_LANGUAGE_VERSION` | the module header declares a source-language major version other than 1 |
| `E1602_UNSUPPORTED_LANGUAGE_MINOR` | the module header declares a minor version the frontend does not implement |
| `E1603_MODULE_PATH_MISMATCH` | a source unit's canonical repository path is not the path its declared module name maps to |
| `E1604_IMPORT_NOT_FOUND` | an import names no module in the declared source set |
| `E1605_AMBIGUOUS_IMPORT` | the declared source set contains the same module name more than once, so an import of that name has more than one candidate and nothing in the set decides between them |
| `E1606_IMPORT_CYCLE` | the import graph contains a cycle; the ordered cycle path is a field |
| `E1607_PRIVATE_PUBLIC_TYPE` | a module-private nominal type appears in the transitive public type surface of a `pub` function signature |

### Concurrency (stage `type`)

| Code | Condition |
|---|---|
| `E1401_UNJOINED_TASK` | a task scope is left with a spawned child still unconsumed, or a spawned child's handle is never bound and so can never be consumed; `cancel` is a cooperative request and does not discharge the obligation |
| `E1410_INVALID_ATOMIC_ORDER` | an atomic operation is given an order it does not accept — a load outside `Relaxed`/`Acquire`/`SeqCst`, a store outside `Relaxed`/`Release`/`SeqCst`, a `compare_exchange` failure order outside `Relaxed`/`Acquire`/`SeqCst`, or a failure order stronger than its success order |

### Capability and effect (stage `effect`)

| Code | Condition |
|---|---|
| `E1501_UNDECLARED_CAPABILITY_EFFECT` | an operation requires a capability whose name is not in the enclosing function's effect set, or a call requires an effect the caller's `uses` set does not include; the `required_by` field names the callee, or `operation` for a direct use |
| `E1502_FORGED_CAPABILITY` | a capability interface is constructed or cast into existence rather than received through its declared import; the `interface` field names it and `operation` says which |

### Unsafe and FFI boundary (stage `effect`)

| Code | Condition |
|---|---|
| `E1801_FFI_NOT_AVAILABLE` | an `extern` item names no accepted FFI interface schema; V1 accepts none, so every `extern` item is rejected |
| `E1802_UNSAFE_RATIONALE_REQUIRED` | an `unsafe` block does not open with a line comment beginning `SAFETY:` |

### Ownership (stage `ownership`)

| Code | Condition |
|---|---|
| `E1301_USE_AFTER_MOVE` | a place is used after its value moved out on some reachable path, by an assignment, an owning argument, a return, placement in an aggregate, a match subject, or a capture; a deferred cleanup body is checked on the exit path that runs it |
| `E1302_CONFLICTING_BORROW` | an operation violates the exclusivity of a live borrow of an overlapping place: a new borrow incompatible with a live overlapping borrow, an owner read or use while a mutable borrow is live, an owner mutation while a mutable borrow is live, or a move or other invalidation while any borrow is live; the `operation` field names which |
| `E1303_MUTATE_WHILE_BORROWED` | a write lands on a place that a live immutable, shared borrow overlaps |
| `E1304_INVALID_TASK_CAPTURE` | a task captures a value that is not `Transferable`: a borrow, a lock guard, a mutable region, a non-transferable capability, or a mutable binding by alias |
| `E1305_INVALID_CLOSURE_CAPTURE` | a closure captures a borrow, a mutable binding by alias, a lock guard, a non-transferable capability, or a plain mutable region |

### Resource and profile (stage `resource`)

| Code | Condition |
|---|---|
| `E1702_PROFILE_NOT_SUPPORTED` | a `profile bootstrap` module uses a Full-profile construct — `async fn`, `spawn async`, `await`, a closure, `defer`, `unsafe` or `extern` — or declares `workers` greater than 1; the first such feature in source order is reported |
| `E1700_RESOURCE_DECLARATION_REQUIRED` | the module resource declaration omits one of the ten required keys of section 6 of docs/41 |
| `E1703_DUPLICATE_RESOURCE_DECLARATION` | a resource declaration is made more than once, whether as a second `resource` item or as a repeated key inside one |
| `E1708_UNBOUNDED_CLEANUP` | a declared type's cleanup has no finite documented bound. V1 source has no drop-contract declaration form (docs/39 section 4), so no V1 module can raise this condition; it is registered for the contract that introduces one |
| `E1701_UNMETERED_LOOP` | a loop has neither a statically proven finite bound nor fuel to meter its back edges: a `while` or bare `loop` in a module declaring `fuel: 0`. A `for` is bounded by the length of the sequence it iterates |
| `E1704_UNKNOWN_RESOURCE_LIMIT` | a resource key is not one of the required keys, or its value is not the literal class that key takes |

<!-- stage2-diagnostic-registry:end -->

`E1107_UNEXPECTED_TOKEN` is the defined residual of the parse stage. A more
specific code always wins where one applies. A recurring `E1107` condition with a
distinct meaning is a reason to allocate a new code through a versioned language
decision, not a reason to keep using the residual.

Lexical diagnostics precede parse diagnostics: a source unit that fails to
tokenize produces exactly one lexical diagnostic and no parse diagnostics. Within
one stage the earliest source span wins, as required by `docs/41` section 7.

### Later-stage families

The `E12xx` type/evaluation, `E13xx` ownership, `E14xx` concurrency/atomic,
`E15xx` capability/effect, `E16xx` module/version, `E17xx` resource/profile,
`E18xx` unsafe/FFI and `V20xx` IR verifier families are defined by
`docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`,
`docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`,
`docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md` and
`docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`. Those documents state each condition;
this registry does not restate them while the checker, verifier and runtime that
raise them are unimplemented.

Each family MUST be folded into the table above — with its stage and exact
condition — by the stage that implements it, before that stage closes. A stage
that raises a code absent from this registry has not met its exit gate. The
stage label for a family is fixed when the family is folded in, not guessed in
advance: `docs/41` section 7 enumerates the stages `lex`, `parse`, `type`,
`ownership`, `effect`, `resource`, `IR` and `runtime`, and assigning families to
them is part of contracting the corresponding checker.

## 8. Open matters outside this proposal

There are no unresolved semantic questions needed to begin the intended
Bootstrap reference implementation if ADR-0028 is accepted. Deliberately
deferred, separately versioned contracts are: persistent IR byte encoding;
concrete Stage 3 capability/IPC/MMIO/IRQ/DMA interface schemas; the exact
future FFI ABI; user generics/traits/macros; detached tasks/supervisor API;
NUMA/affinity API; and bytecode/native backend admission. None is silently
provided by a host implementation, and none blocks Bootstrap source semantics.
