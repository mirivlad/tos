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
- `docs/language/conformance/v1/` — 31 accepted and 76 rejected vectors, three
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
| resource exhaustion | All ten declared limits are enforced. Fuel, recursion, tasks, **allocation**, **cleanup**, **workers**, **sync** and **shared** are reserved before the effect and released when the frame that charged them returns; the verifier additionally bounds cleanups per exit statically. `sync` counts live guards, whose lifetime ADR-0036 bounds to the frame that took them; `shared` charges a `Shared<T>` on the same cell model `allocation` uses. `stack` and `imports` are static declarations the frontend and verifier check rather than run-time counters. |
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
   three functions, and runs through the ordinary reference path on the boot
   path. `tests/integration/tests/init_boot.rs` takes the boot content out of
   the golden capsule — the bytes a booting machine actually receives — and runs
   them; `crates/tos-pipeline/tests/boot_module.rs` runs the file itself.
2. *(Resolved.)* **The freestanding runtime runs on the boot path.**
   `crates/tos-pipeline` composes reader, parser, checker, module resolution,
   lowerer, independent verifier and bounded engine; the nucleus derives a
   `RuntimeMemoryGrantV1` from the validated memory map, installs the bounded
   heap as its global allocator, and drives it over the capsule's canonical boot
   text. Verified in QEMU under the ADR-0040 profile by
   `host-tools/qemu-test/stage2-runtime.sh`, which checks the stage order, the
   verifier's receipt, the returned value, every accounting pair against its own
   limit, the arena peak against the grant and the stack use against the stack.
   `host-tools/qemu-test/boot-module-failure.sh` proves the other direction: a
   module the checker refuses halts with `RESULT_BOOT_MODULE_FAILED`.
3. *(Resolved.)* **ADR-0036 is implemented end to end.** The three guard
   constructors, the three lock operations, `E1402_INVALID_GUARD_LIFETIME` with
   all six `operation` values and its precedence over `E1304`/`E1305`,
   `V2031_SYNC` reached by the verifier's own traversal, the engine executing a
   lock operation, and `sync` metered against live guards.
4. *(Resolved.)* **ADR-0037 is implemented end to end.** Region modes in the
   type surface and in the IR type table, `share` as a predeclared operation
   with its own IR node, `E1215_ARGUMENT_TYPE_MISMATCH` for an argument that
   does not satisfy it, `E1201_ASSIGN_TO_IMMUTABLE` for a write through an
   immutable grant, the capture reasons, `V2021_REGION` in the verifier, and
   `shared` metered. The file's `Proposed`/`pending` state, which contradicted
   the earlier Architect decision, is corrected.
5. *(Resolved.)* **Set-wide resolution no longer retains parse trees.**
   `check_module_summaries` resolves over a derived per-module summary —
   name, path, content identity, imports, declared types, qualified uses — so a
   loader holds one parse tree at a time. Verdicts are unchanged and tested both
   ways. `docs/evidence/STAGE2_ARENA_BOUND.md` carries the measured scaling.
6. *(Resolved.)* **The allocator's search no longer depends on the arena.**
   Free blocks are threaded onto size-class lists; the request's own class gets
   a fixed probe budget and any larger class fits without inspection. The
   allocator counts its own probes, and the evidence is a series holding flat
   while live blocks grow 64x, with eight adversarial patterns and the eleven
   pre-existing regressions unchanged
   (`docs/evidence/STAGE2_ALLOCATOR_SEARCH.md`). The arena-bound sweep went from
   hours to 16.5 s and the measured bound moved by under 0.1%.
7. **The Stage 2 performance gate FAILS on two of three metrics**, measured by
   the normative procedure — 3 warmups, 21 samples, median/p95/p99, one commit,
   one set of fixtures, both halves
   (`docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md`).

   | metric | reference p95 | budget | verdict |
   |---|---|---|---|
   | frontend, 256 KiB | 1 490 798 us | 500 000 us | **FAIL** (2.98x) |
   | engine, 1e6 ops | 5 541 378 us | ratio ≤ 10x | **FAIL** (16.6x) |
   | quota rejection | 763 305 us | ≤ 2x accepted | **PASS** (0.512) |

   Five implementation defects have now been found and fixed — the heap's
   whole-arena search, the lowerer's `format!` per intern, the engine's
   per-instruction clone, a whole-source NFC normalization that ASCII makes
   unnecessary, and eagerly formatted verifier finding locations — and one
   hypothesis, byte-at-a-time freestanding memory primitives, was tested against
   the real binary and **refuted**. The current figures after all five are
   frontend 124 ms native / 1.28 s reference (2.56x over budget) and an engine
   ratio of 16.8x; `docs/evidence/STAGE2_PERFORMANCE_DECOMPOSITION.md` carries
   the per-stage breakdown and the finding that a 1.6x engine speedup left the
   ratio unchanged. The platform factor is now uniform across two very different
   workloads (9.3x and 16.6x) where it previously differed by three orders of
   magnitude, which is the evidence that no third pathology is hiding.
   **ADR-0043 (Proposed)** carries the two failures with their measurements and
   recommends settling them separately: the engine ratio measures the platform
   and cannot be met by improving the engine, while the frontend budget is
   absolute and implementation work can still move it.
8. **Differential testing is N/A, not passed.** docs/44 asks for agreement
   between independent implementations, and there is one engine. A second
   implementation is the only thing that can change this.
9. **Evidence is P1.** One machine, one build, no CI reproduction and no
   independent reproduction (docs/35). Nothing here claims P2 or P3.

## architect_approval

*(empty — Stage 2 is a candidate for review, not closed. Only the Project
Architect grants this, and it is not the implementation's to write.)*
