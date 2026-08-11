<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 gate evidence — candidate record

This is the docs/37 gate report for Stage 2, filled in with what exists today.
It is **not** a closure claim. Several mandatory gates are open, they are named
below under `known_failures`, and `architect_approval` is empty because only the
Project Architect grants it.

```text
stage                  2 — Executed-source identity
source_commit          see PROVENANCE_HISTORY_REWRITE.md for the SHA mapping
architecture_version   TOS Core 1.0; tos-ir/v1; ADR-0027, ADR-0028, ADR-0032,
                       ADR-0033, ADR-0034, ADR-0035
identity_question      Is actual language semantics executing from canonical
                       text with a verifiable mapping to runtime behavior?
```

## required_evidence

| docs/37 Stage 2 evidence | State |
|---|---|
| normative grammar and semantics | **Present.** docs/39–44 accepted; the diagnostic registry holds 59 codes, each implemented, deliberately unreachable under V1, or blocked with the exact conflict recorded. |
| source → AST → typed IR → execution trace | **Present for the lowered subset.** `SourceReader → Parser → Checker → Lowerer → tos-ir/v1 → Verifier → Reference interpreter` runs end to end; every IR operation carries a source-map entry and every runtime trap names one. |
| independent verifier | **Present.** `crates/tos-verifier` depends only on `tos-ir` and `tos-hash`; fifteen `V20xx` families; nineteen forged-IR negatives. |
| cache deletion/regeneration test | **Absent.** No derived-artifact cache exists yet. |
| source mutation invalidates old cache | **Absent.** The module digest changes with the module and a receipt binds to it, which is the mechanism a cache would use, but no cache consumes it yet. |
| runtime introspection reports source and engine identity | **Partial.** A `VerifiedModule` receipt carries source-set, path, content ID, dependency digest, profile, envelope, capability digest, source-map digest and verifier identity; the engine checks the receipt against the module digest. No running-component introspection surface exists. |

## produced_artifacts

- `source/crates/tos-core` — source reader, lexer, parser with recovery,
  checker (types, ownership, effects, modules, resources, profile), lowerer.
- `source/crates/tos-ir` — the `tos-ir/v1` semantic schema and module digest.
- `source/crates/tos-verifier` — the independent verifier and its receipts.
- `source/crates/tos-engine` — the bounded Bootstrap reference interpreter.
- `docs/language/conformance/v1/` — 28 accepted and 60 rejected vectors with
  their expectations table.
- `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt`, `SHA256SUMS` — generated
  and in sync.

## tests

```text
349 tests pass across 21 binaries
  231 tos-core unit tests
    2 conformance corpus gates (accept + reject)
    7 lowering gates (determinism, source maps, terminators, operands, digest)
   19 pipeline gates (verifier acceptance + forged-IR negatives by family)
   18 execution gates (end-to-end results, traps, accounting, determinism)
    1 boot-text gate (what init.tos is today)
  remainder: Stage 0/1 capsule, boot protocol, hash, serial, fuzz, performance
```

## performance_report

**P1, locally measured.** `docs/evidence/STAGE2_PERFORMANCE_P1.md` retains the
raw 3-warmup/21-sample median/p95/p99 record for the two Stage 2 metrics and the
quota-rejection ratio, with the environment and toolchain it was taken on and
the exact command to reproduce it. That lifts both metrics off P0, which docs/35
forbids for a stage's own metrics.

It is **not** a closure. docs/35 wants the declared reference platform, and this
machine is not it; the gate stays open until the same procedure runs there. The
one-million-operation budget is stated relative to a host reference interpreter
under the same semantic implementation, and no second implementation exists yet,
so the absolute number is retained and the ratio is not claimed.

## threat_model_coverage

| docs/34 / docs/44 section 3 requirement | State |
|---|---|
| malformed source fuzzing without parser panic | Present (`tests/fuzz`, retained corpus). |
| malformed/forged IR without panic | Partial: nineteen structured forged-IR cases; no IR fuzzer. |
| Unicode NFC conformance with provenance | Present (Stage 1.5 gate). |
| source/path/import ambiguity | Present (`E1603`, `E1604`, `E1605`, `E1606`). |
| capability forgery / ambient authority | Present (`E1501`, `E1502`, `V2013_CAPABILITY`). |
| ownership and data-race negatives | Present (`E1301`–`E1305`, `V2020_OWNERSHIP`). |
| atomic-order negatives | Present (`E1410`, `V2032_ATOMIC_ORDER`). |
| resource exhaustion | Partial: fuel and recursion enforced at runtime; allocation, tasks, workers, sync, shared and cleanup are declared and verified but not yet metered. |
| source-map identity forgery | Present (`V2040_SOURCE_MAP`). |
| cache substitution | Absent — no cache. |
| cross-engine differential testing | Not applicable — one engine exists. |

## compatibility_profiles

TOS Core 1.0, profiles `bootstrap` and `full`. The lowerer and engine implement
the Bootstrap computation subset; Full-profile constructs are named gaps rather
than approximations.

## known_failures

1. **`/system/boot/init.tos` is not TOS Core source.** Proved by
   `tests/integration/tests/init_boot.rs`: the real file is transport-valid but
   the parser rejects it at `E1013_UNEXPECTED_CHARACTER`. It is the Stage 1
   capsule's illustrative boot text — the file says so — and the nucleus reads
   it as text. Making it a TOS Core module changes the Stage 1 capsule's boot
   text, which is an Architect decision, not an implementation one.
2. *(Resolved.)* Lowering covers all 38 accepted vectors; no named gap remains.
3. *(Resolved.)* The cache identity plane is implemented in `crates/tos-cache`:
   the docs/43 section 6 key, fail-closed lookup, substitution refusal, and
   deletion followed by regeneration to the same identity.
4. **Performance evidence is P1, not reference-platform.** See above.
5. *(Resolved.)* The missing `Signed-off-by` on `80bfcc1` was repaired under a
   one-time Architect authorization; every tree hash is unchanged and the
   mapping is in `PROVENANCE_HISTORY_REWRITE.md`. `preflight --full` passes.
6. **Four contract decisions await approval.** ADR-0036 (guard representation),
   ADR-0037 (region transferability), ADR-0038 (module-root precedence and the
   exact `E1605` condition) and ADR-0039 (`E1213_NONCONSTRUCTIBLE_TYPE`) are
   written with their full decision text and are **Proposed**. None is
   implemented; each needs one Project Architect approval line, and that line is
   not the implementation's to write.

## architect_approval

*(empty — Stage 2 is a candidate for review, not closed. Only the Project
Architect grants this.)*
