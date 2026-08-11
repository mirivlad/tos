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
                       ADR-0032, ADR-0033, ADR-0034, ADR-0035, ADR-0036,
                       ADR-0038, ADR-0039, ADR-0040, ADR-0041
identity_question      Is actual language semantics executing from canonical
                       text with a verifiable mapping to runtime behavior?
```

## required_evidence

| `docs/37` Stage 2 evidence | State |
|---|---|
| normative grammar and semantics | **Present.** `docs/39`–`docs/44` accepted. The diagnostic registry holds 61 codes; every one is implemented, deliberately unreachable under V1, or blocked on a decision named below. |
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
- `source/crates/tos-runtime` — `RuntimeMemoryGrantV1`, the bounded heap, and
  the region chooser that turns free and occupied spans into a grant.
- `source/crates/tos-pipeline` — the composition: canonical source through every
  stage to an observable, source-mapped result, `no_std`, with the `TOS.RUN.*`
  event rendering both the boot log and the host tests read.
- `source/nucleus` — derives the grant from the validated memory map, installs
  the bounded heap, measures the stack it runs on, and drives the reference path
  over the capsule's canonical boot text.
- `source/host-tools/qemu-test/stage2-runtime.sh` — Stage 2 conformance on the
  real boot path, checking values rather than the presence of events.
- `source/tests/arena-bound` — the arena measurement, whole pipeline through the
  bounded heap.
- `source/tests/performance-core` — the `docs/35` Stage 2 measurement harness.
- `docs/language/conformance/v1/` — 25 accepted and 60 rejected vectors, three
  driver-level resolution cases, and the expectations table binding them.
- `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt`, `SHA256SUMS` — generated
  and in sync.

## tests

```text
429 tests pass across 39 binaries
  230 tos-core unit tests
   15 guard-lifetime gates (ADR-0036: each operation value, the precedence over
      E1304/E1305, and the positives a guard must keep)
    2 conformance corpus gates (accept + reject)
    7 lowering gates (determinism, source maps, terminators, operands, digest)
   22 pipeline gates (verifier acceptance + forged-IR negatives by family,
      including V2031_SYNC reached without any frontend involvement)
   34 execution gates (results, traps, tasks, cleanup, closures, accounting)
   13 reference-path gates (every stage entered in order, each stage's refusal
      reaching the caller intact, and the canonical boot module executing)
    8 cache identity gates (key fields, fail-closed, delete and regenerate)
   10 region-chooser gates (a grant never overlaps live memory, and one the
      chooser makes is one the heap accepts)
   11 heap gates (grant validation, reclaim, coalescing, exhaustion, 1000-round
      reuse returning the arena to its starting layout)
  remainder: Stage 0/1 capsule, boot protocol, hash, serial, fuzz, performance
./scripts/preflight.sh --full   34 of 34 gates pass
```

## performance_report

**One half of the required pair, taken.** `docs/evidence/STAGE2_PERFORMANCE_P1.md`
retains the native-host record: raw 3-warmup/21-sample median/p95/p99 for both
Stage 2 metrics and the quota-rejection ratio, with the toolchain, environment
and the exact command.

ADR-0040 (**accepted**) fixes the Stage 2 reference platform as the
q35/qemu64/one-vCPU/256-MiB/TCG profile Stage 1 already mandates, and reads the
`docs/35` execution budget as the ratio of that platform's time to the
native-host time of the same engine at the same commit. The harness takes the
profile as a declared argument and records what it was told; it never concludes
that the machine it runs on is the reference platform.

The reference half is **not taken**, and cannot be until the runtime-independence
gap closes: ADR-0040 section 1a requires the measurement to run the real Stage 2
runtime path, so a number produced by running the engine inside a host guest
would measure the host and would let a host runtime into Stage 2 through the
performance gate. The gate is open. See `known_failures`.

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
| resource exhaustion | Partial. Fuel, recursion, tasks, **allocation**, **cleanup** and **workers** are reserved before the effect and released when the frame that charged them returns; the verifier additionally bounds cleanups per exit statically. `sync` and `shared` are **not** metered, and are not claimed: the operations that would consume them — lock acquisition and `share` — do not exist until ADR-0036 is implemented and ADR-0037 is accepted. |
| source-map identity forgery | Present (`V2040_SOURCE_MAP`). |
| cache substitution | Present (`Rejection::KeyDoesNotMatchIdentity`). |
| runtime independence from the host | **Partly discharged, and now gated rather than argued.** All five production crates are `#![no_std] + alloc` and build for `x86_64-unknown-none`; two preflight gates enforce it — a source gate that no production module names a host facility and that every crate declares `#![no_std]`, and a build gate that compiles all five for the freestanding target. `crates/tos-runtime` supplies the ADR-0041 grant and heap. What remains is linking and running a freestanding **binary**: the crates are proved host-free, the assembled runtime is not yet. |
| cross-engine semantic differential testing | **N/A for the current supported-engine set.** `docs/44` section 3 requires it "for every supported engine" and section 7 requires every engine to pass the same vectors. One engine is supported, so the requirement is vacuously satisfied. It becomes mandatory the moment a second engine is supported, and no engine will be built to satisfy a denominator. |

## compatibility_profiles

TOS Core 1.0, profiles `bootstrap` and `full`. The lowerer, verifier and engine
cover the whole accepted V1 surface of the corpus. Operation families the engine
does not execute — atomics, capability operations, explicit resource operations
— trap with `RUNTIME_OPERATION_NOT_IMPLEMENTED` rather than producing a quietly
wrong answer, and none of them is lowered, so the layers do not disagree.

## known_failures

1. *(Resolved.)* **`/system/boot/init.tos` is a TOS Core module and executes.**
   It declares `module system.boot.init`, its resource envelope, a record and
   three functions, and it runs through the ordinary reference path on the boot
   path. `tests/integration/tests/init_boot.rs` takes the boot content out of
   the golden capsule — the bytes a booting machine actually receives — and runs
   them, and `crates/tos-pipeline/tests/boot_module.rs` runs the file itself.
2. *(Resolved.)* **The freestanding runtime runs on the boot path.**
   `crates/tos-pipeline` composes reader, parser, checker, module resolution,
   lowerer, independent verifier and bounded engine; the nucleus derives a
   `RuntimeMemoryGrantV1` from the validated memory map, installs the bounded
   heap as its global allocator, and drives it over the capsule's canonical boot
   text. Verified in QEMU under the ADR-0040 profile by
   `host-tools/qemu-test/stage2-runtime.sh`, which checks the stage order, the
   verifier's receipt, the returned value, every accounting pair against its own
   limit, the arena peak against the grant and the stack use against the stack —
   not merely that the events appeared.
3. **The Stage 2 performance gate is open.** The native half of the ratio is
   taken. The reference half is now *takeable* — the reference path executes
   under the ADR-0040 profile — but it has not been taken, and no budget is
   asserted from the native record.
4. *(Resolved.)* **ADR-0036 is implemented.** The three guard constructors, the
   three lock operations, `E1402_INVALID_GUARD_LIFETIME` with all six
   `operation` values and its precedence over `E1304`/`E1305`, and `V2031_SYNC`
   reached by the verifier's own traversal with forged-IR negatives.
   `sync` accounting still has nothing to meter: see item 6.
5. **ADR-0037 is accepted but not yet implemented.** `Region<mut T>`,
   `DmaRegion<mut T>`, `share`, the transfer and share model, `V2021_REGION` and
   `shared` accounting are decided and unbuilt. Its diagnostic dependency —
   `E1215_ARGUMENT_TYPE_MISMATCH` — **is** implemented and bound to the corpus.
6. **`sync` and `shared` are not metered.** The engine executes no lock
   operation and no `share`, so there is nothing to count. `sync` follows the
   engine's side of ADR-0036, which is not built; `shared` follows item 5.
7. **The conformance corpus has no ADR-0036 vectors.** The nine cases ADR-0036
   section 7 lists are covered by `crates/tos-core/tests/guards.rs` and by the
   forged-IR negatives in `tests/integration/tests/pipeline.rs`, and they are
   not yet expressed as `docs/language/conformance/v1` vectors with
   `EXPECTATIONS.md` rows.
8. **A maximal dependency closure does not fit the reference platform.** The two
   published ceilings of docs/44 section 2 multiply to a closure whose
   resolution needs about 3.2 GiB, measured by slope; ADR-0040's platform has
   256 MiB. This implementation resolves roughly 19 ceiling-sized modules, or
   256 modules averaging 8 KiB. Neither ceiling is weakened, and no accepted
   document requires the maximal closure to resolve in 256 MiB.
   `docs/evidence/STAGE2_ARENA_BOUND.md` records the measurement and names the
   architectural fix — resolving over per-module summaries rather than parse
   trees — which is not done.
9. **ADR-0042 is Proposed and unresolved.** Boot ABI v1 does not settle whether
   identifiers from another vocabulary may be interleaved with its success
   sequence, nor what result code means "the canonical boot module did not
   execute". The implementation fails closed with the existing
   `RESULT_CAPSULE_INVALID` and changes no normative text meanwhile.

## architect_approval

*(empty — Stage 2 is a candidate for review, not closed. Only the Project
Architect grants this, and it is not the implementation's to write.)*
