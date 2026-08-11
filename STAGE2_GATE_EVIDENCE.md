<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 gate evidence — candidate record

This is the `docs/37` gate report for Stage 2. It describes **one state at one
HEAD**: no section carries a claim from an earlier pass. History belongs in
`PROGRESS.md`; this file says what is true now.

It is **not** a closure claim. Mandatory gates are open, they are named under
`known_failures`, and `architect_approval` is empty because only the Project
Architect grants it.

```text
stage                  2 — Executed-source identity
source_commit          see `git rev-parse HEAD`; the SHA mapping for the one
                       authorized history repair is in
                       PROVENANCE_HISTORY_REWRITE.md
architecture_version   TOS Core 1.0; tos-ir/v1; accepted ADR-0027, ADR-0028,
                       ADR-0032, ADR-0033, ADR-0034, ADR-0035, ADR-0038
identity_question      Is actual language semantics executing from canonical
                       text with a verifiable mapping to runtime behavior?
```

## required_evidence

| `docs/37` Stage 2 evidence | State |
|---|---|
| normative grammar and semantics | **Present.** `docs/39`–`docs/44` accepted. The diagnostic registry holds 59 codes; every one is implemented, deliberately unreachable under V1, or blocked on a decision named below. |
| source → AST → typed IR → execution trace | **Present.** `SourceReader → Parser → Checker → Lowerer → tos-ir/v1 → Verifier → reference engine` runs end to end. All 25 accepted vectors and all 6 canonical examples lower; every IR operation carries a source-map entry and every runtime trap names one. |
| independent verifier | **Present.** `crates/tos-verifier` depends only on `tos-ir` and `tos-hash`; fifteen `V20xx` families; nineteen structured forged-IR negatives plus 200 000 fuzz rounds per preflight. |
| cache deletion/regeneration test | **Present.** `crates/tos-cache`; clearing the store and regenerating from the same canonical source reproduces the same key, receipt and result. |
| source mutation invalidates old cache | **Present.** Each of the seventeen `docs/43` section 6 key fields is changed in turn and must move the key. |
| runtime introspection reports source and engine identity | **Present.** `RunningIdentity` carries module name, canonical path, content ID, frontend, verifier, engine, module digest, source-map digest and cache key, and the test that checks every link then runs what the identity names. |

## produced_artifacts

- `source/crates/tos-core` — source reader, lexer, parser with recovery, checker
  (types, ownership, effects, concurrency, capabilities, modules, resources,
  profile, metering), and the lowerer.
- `source/crates/tos-ir` — the `tos-ir/v1` semantic schema and module digest.
- `source/crates/tos-verifier` — the independent verifier and its receipts.
- `source/crates/tos-engine` — the bounded Bootstrap reference interpreter.
- `source/crates/tos-cache` — derived-artifact identity and cache admission.
- `source/tests/performance-core` — the `docs/35` Stage 2 measurement harness.
- `docs/language/conformance/v1/` — 25 accepted and 60 rejected vectors, three
  driver-level resolution cases, and the expectations table binding them.
- `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt`, `SHA256SUMS` — generated
  and in sync.

## tests

```text
371 tests pass across 29 binaries
  234 tos-core unit tests
    2 conformance corpus gates (accept + reject)
    7 lowering gates (determinism, source maps, terminators, operands, digest)
   19 pipeline gates (verifier acceptance + forged-IR negatives by family)
   28 execution gates (results, traps, tasks, cleanup, closures, accounting)
    8 cache identity gates (key fields, fail-closed, delete and regenerate)
    1 boot-text gate (what init.tos is today)
  remainder: Stage 0/1 capsule, boot protocol, hash, serial, fuzz, performance
./scripts/preflight.sh --full   31 of 31 gates pass
```

## performance_report

**One half of the required pair, taken.** `docs/evidence/STAGE2_PERFORMANCE_P1.md`
retains the native-host record: raw 3-warmup/21-sample median/p95/p99 for both
Stage 2 metrics and the quota-rejection ratio, with the toolchain, environment
and the exact command.

ADR-0040 (**Proposed**) fixes the Stage 2 reference platform as the
q35/qemu64/one-vCPU/256-MiB/TCG profile Stage 1 already mandates, and reads the
`docs/35` execution budget as the ratio of that platform's time to the
native-host time of the same engine at the same commit. The harness takes the
profile as a declared argument and records what it was told; it never concludes
that the machine it runs on is the reference platform.

The reference half is **not taken**, so the gate is open. See `known_failures`.

## threat_model_coverage

| `docs/34` / `docs/44` section 3 requirement | State |
|---|---|
| malformed source fuzzing without parser panic | Present (`tests/fuzz`, fixed seed, 200 000 rounds per preflight). |
| malformed/forged IR without panic | Present. Structural mutation of real lowered IR; the verifier must always answer, and any receipt must name the digest of the module it saw. |
| Unicode NFC conformance with provenance | Present (Stage 1.5 gate). |
| source/path/import ambiguity | Present (`E1603`, `E1604`, `E1605` under ADR-0038, `E1606`). |
| capability forgery / ambient authority | Present (`E1501`, `E1502`, `V2013_CAPABILITY`). |
| ownership and data-race negatives | Present (`E1301`–`E1305`, `V2020_OWNERSHIP`). |
| atomic-order negatives | Present (`E1410`, `V2032_ATOMIC_ORDER`). |
| resource exhaustion | Partial. Fuel, recursion and the task budget are enforced at runtime; allocation, workers, sync, shared and cleanup are declared and verified but not metered during execution. |
| source-map identity forgery | Present (`V2040_SOURCE_MAP`). |
| cache substitution | Present (`Rejection::KeyDoesNotMatchIdentity`). |
| cross-engine semantic differential testing | **N/A for the current supported-engine set.** `docs/44` section 3 requires it "for every supported engine" and section 7 requires every engine to pass the same vectors. One engine is supported, so the requirement is vacuously satisfied. It becomes mandatory the moment a second engine is supported, and no engine will be built to satisfy a denominator. |

## compatibility_profiles

TOS Core 1.0, profiles `bootstrap` and `full`. The lowerer, verifier and engine
cover the whole accepted V1 surface of the corpus. Operation families the engine
does not execute — atomics, capability operations, explicit resource operations
— trap with `RUNTIME_OPERATION_NOT_IMPLEMENTED` rather than producing a quietly
wrong answer, and none of them is lowered, so the layers do not disagree.

## known_failures

1. **`/system/boot/init.tos` is not a TOS Core module.** Proved by
   `tests/integration/tests/init_boot.rs`: the file is transport-valid and the
   parser rejects it at `E1013_UNEXPECTED_CHARACTER`. It is the Stage 1 capsule's
   illustrative boot text, which the nucleus reads as text. Replacing it is
   sequenced after the decisions below, by the Project Architect's direction.
2. **The Stage 2 performance gate is open.** The native half of the ratio is
   taken; the reference half needs the harness to execute under the ADR-0040
   profile, and ADR-0040 itself needs approval. No budget is asserted from the
   native record.
3. **Four contract decisions await approval and are unimplemented.** ADR-0036
   (guard representation and `E1402_INVALID_GUARD_LIFETIME`), ADR-0037 (region
   and DMA-region transfer and share model), ADR-0039
   (`E1213_NONCONSTRUCTIBLE_TYPE`) and ADR-0040 (Stage 2 reference platform) are
   **Proposed** with full decision text. Until they are accepted,
   `V2021_REGION` and `V2031_SYNC` have no rules, and casting an integer to
   `Task<i32>` is still accepted in silence.
4. **Runtime resource metering is partial.** See `threat_model_coverage`.

## architect_approval

*(empty — Stage 2 is a candidate for review, not closed. Only the Project
Architect grants this, and it is not the implementation's to write.)*
