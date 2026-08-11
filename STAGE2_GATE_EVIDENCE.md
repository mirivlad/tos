<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 gate evidence — candidate record

This is the docs/37 gate report for Stage 2, filled in with what exists today.
It is **not** a closure claim. Several mandatory gates are open, they are named
below under `known_failures`, and `architect_approval` is empty because only the
Project Architect grants it.

```text
stage                  2 — Executed-source identity
source_commit          f7cbaf4 (origin/main)
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

**Absent.** No docs/35 measurement has been run for the Stage 2 components. The
existing `tests/performance` smoke covers Stage 1 capsule work only. The
required reference-platform procedure — parse/type-check/lower/verify a 256 KiB
canonical module, the one-million-operation integer benchmark, quota rejection,
with warmups, sample counts and median/p95/p99 — has not been performed, and no
P-level is claimed.

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
2. **Lowering covers 28 of 38 accepted vectors.** `async fn`, `spawn`,
   closures, `defer`, `for`, `cancel`, `unsafe` and payload-binding `match`
   arms produce a named gap. Each is a construct, not a defect.
3. **No cache or provenance plane.** docs/43 section 6 cache identity, deletion
   and regeneration, and source-mutation invalidation are unimplemented.
4. **No performance evidence.** See above.
5. **DCO gate fails on commit `80bfcc1`.** That commit reached `origin/main`
   without the `Signed-off-by` trailer docs/23 requires. Repairing it needs
   published history rewritten, which the standing instruction forbids, so
   `./scripts/preflight.sh --full` reports 1 of 31 gates failing until the
   Architect decides. Every later commit carries the trailer.
6. **Open contract questions**, recorded in `PROGRESS.md`: region and lock-guard
   `Transferable` and the missing guard type constructor; the
   nonconstructible-type error for non-capability opaque handles; module-root
   precedence for `E1605`.

## architect_approval

*(empty — Stage 2 is a candidate for review, not closed. Only the Project
Architect grants this.)*
