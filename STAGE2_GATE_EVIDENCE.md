<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 gate evidence — candidate record

This is the `docs/37` gate report for Stage 2. It describes **one state at one
HEAD**: no section carries a claim from an earlier pass. History belongs in
`PROGRESS.md`; this file says what is true now.

It is **not** a closure claim. Every mandatory Stage 2 gate is met on the
evidence below; what remains under `known_failures` is `N/A` by an accepted
contract, an honest evidence level, or a decision the Architect has directed is
not a blocker. `architect_approval` is empty because only the Project Architect
grants it, and a candidate record that filled it in would be claiming the thing
it exists to ask for.

```text
stage                  2 — Executed-source identity
source_commit          the commit this file is committed in; MANIFEST.txt and
                       SHA256SUMS in the same commit pin every file it
                       describes, which is the repository's existing provenance
                       convention and avoids a record that must name its own
                       SHA. The SHA mapping for the one authorized history
                       repair is in PROVENANCE_HISTORY_REWRITE.md
architecture_version   TOS Core 1.0; tos-ir/v1; accepted ADR-0027, ADR-0028,
                       ADR-0032, ADR-0033, ADR-0034, ADR-0035, ADR-0036,
                       ADR-0037, ADR-0038, ADR-0039, ADR-0040, ADR-0041,
                       ADR-0042, ADR-0043, ADR-0045, ADR-0046, ADR-0047
                       (ADR-0044 is Proposed and is not a closure condition)
identity_question      Is actual language semantics executing from canonical
                       text with a verifiable mapping to runtime behavior?
```

## required_evidence

| `docs/37` Stage 2 evidence | State |
|---|---|
| normative grammar and semantics | **Present.** `docs/39`–`docs/44` accepted. The diagnostic registry holds 63 codes; every one is implemented, deliberately unreachable under V1, or blocked on a decision named below. |
| source → AST → typed IR → execution trace | **Present.** `SourceReader → Parser → Checker → Lowerer → tos-ir/v1 → Verifier → reference engine` runs end to end. Of the 35 single-module accepted vectors, 35 reach the independent verifier and the bounded engine and none stops at lowering — measured by `crates/tos-pipeline/tests/corpus_coverage.rs`, which ratchets the number rather than asserting it once. Every IR operation carries a source-map entry and every runtime trap names one. |
| independent verifier | **Present.** `crates/tos-verifier` depends only on `tos-ir` and `tos-hash`; fifteen `V20xx` families; 23 structured forged-IR negatives — including `V2031_SYNC` and `V2021_REGION`, reached without any frontend involvement — plus 200 000 fuzz rounds per preflight. |
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
- `docs/language/conformance/v1/` — 37 accepted and 80 rejected vectors, three
  driver-level resolution cases, and the expectations table binding them.
- `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt`, `SHA256SUMS` — generated
  and in sync.

## tests

```text
491 tests pass across 46 binaries
  237 tos-core unit tests
   15 guard-lifetime gates (ADR-0036: each operation value, the precedence over
      E1304/E1305, and the positives a guard must keep)
   12 region gates (ADR-0037: the four facts, share, write-through, captures)
    7 irrefutability gates (ADR-0046: let and for, recursive, match unaffected)
    7 summary gates (resolution over summaries reaches the same verdicts)
    2 conformance corpus gates (accept + reject)
    7 lowering gates (determinism, source maps, terminators, operands, digest)
   25 pipeline gates (verifier acceptance + 23 forged-IR negatives by family,
      including V2031_SYNC and V2021_REGION reached without any frontend)
   34 execution gates (results, traps, tasks, cleanup, closures, accounting)
   19 reference-path gates (every stage entered in order, each stage's refusal
      reaching the caller intact, sync and shared accounting, the canonical
      boot module executing)
   10 match-shape gates (every arm shape the contract admits, executed)
    8 pattern gates (destructuring to an executed result, ownership preserved)
    1 corpus coverage ratchet (35 of 35 single-module accepted vectors reach
      the verifier and the engine; a corpus gate, not a language-coverage claim)
    8 cache identity gates (key fields, fail-closed, delete and regenerate)
   10 region-chooser gates (a grant never overlaps live memory, and one the
      chooser makes is one the heap accepts)
   19 heap gates (grant validation, reclaim, coalescing, exhaustion, repeated
      layout, and the search-work series that bounds allocation cost)
  remainder: Stage 0/1 capsule, boot protocol, hash, serial, fuzz, performance
./scripts/preflight.sh --full   35 of 35 gates pass
```

## performance_report

**Both halves taken, by the normative procedure, at commit `46911ef`.**
3 warmups, 21 samples, median/p95/p99, one commit, one set of fixtures emitted
by the harness that measures them natively so both halves see the same bytes,
and the reference half run through the real freestanding Stage 2 path on the
ADR-0040 platform. `docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md` holds it with
every raw sample.

| metric | native p95 | reference p95 | budget | verdict |
|---|---|---|---|---|
| frontend, 256 KiB module | 120 943 us | 1 212 216 us | 1 500 000 us (ADR-0045) | **PASS** |
| engine, 1e6 operations | 207 873 us | 3 655 641 us | ratio ≤ 22x (ADR-0043) | **PASS** (17.6x) |
| quota rejection | — | 469 738 us | ≤ 2x accepted | **PASS** (0.388) |

ADR-0040 fixes the reference platform and reads the docs/35 execution budget as
the ratio of that platform's time to the native-host time of the same engine at
the same commit. The harness takes the profile as a declared argument and
records what it was told; it never concludes that the machine it runs on is the
reference platform. Section 1a's requirement — that the measurement run the real
Stage 2 runtime path rather than a host process wearing the platform's name — is
met: the workload is the capsule's canonical boot module and it goes through
reader, parser, checker, resolution, lowering, the independent verifier and the
bounded engine inside the guest.

Both revised budgets are Architect decisions on measured evidence, not
adjustments to fit. ADR-0043 revised the engine ratio from 10x after a component
decomposition put every semantic component of the workload in a 15.1–17.9x band;
ADR-0045 revised the frontend from 500 ms after six general implementation
defects the gate uncovered were fixed and the remaining cost was decomposed and
explained. The measurement boundaries are protected against the earlier defect
where a span between two result events measured line formatting rather than
work: `reference-performance-report.py` refuses a reduction whose boundary
cannot contain the work it claims to measure.

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
| runtime independence from the host | **Discharged, and gated rather than argued.** Every production crate is `#![no_std] + alloc` and builds for `x86_64-unknown-none`; two preflight gates enforce it — a source gate that no production module names a host facility and that every crate declares `#![no_std]`, and a build gate that compiles them for the freestanding target. `crates/tos-runtime` supplies the ADR-0041 grant and heap, and the assembled binary **runs**: the nucleus derives a grant from the validated memory map, installs the bounded heap, and drives the capsule's canonical boot module through the whole reference path inside QEMU (`host-tools/qemu-test/stage2-runtime.sh`, a preflight gate that checks the stage order, the receipt, the returned value, every accounting pair, the arena peak and the stack margin). The earlier audit that found this gap is retained and marked superseded at `docs/evidence/STAGE2_RUNTIME_INDEPENDENCE_AUDIT.md`. |
| cross-engine semantic differential testing | **N/A for the current supported-engine set.** `docs/44` section 3 requires it "for every supported engine" and section 7 requires every engine to pass the same vectors. One engine is supported, so the requirement is vacuously satisfied. It becomes mandatory the moment a second engine is supported, and no engine will be built to satisfy a denominator. |

## compatibility_profiles

TOS Core 1.0, profiles `bootstrap` and `full`. The lowerer, verifier and engine
cover the whole accepted V1 surface of the corpus. Operation families the engine
does not execute — atomics, capability operations, explicit resource operations
— trap with `RUNTIME_OPERATION_NOT_IMPLEMENTED` rather than producing a quietly
wrong answer, and none of them is lowered, so the layers do not disagree.

## known_failures

The six items this record carried through the performance work are resolved and
are now history rather than caveats; the git log holds them. What remains is
this, and it is short on purpose — an audit that finds nothing is an audit that
did not look, so each entry below states what was checked as well as what stands.

1. **Differential testing is N/A, not passed.** docs/44 asks for agreement
   between independent implementations, and there is exactly one engine. This is
   not a defect to fix and not a gate to claim: a second implementation is the
   only thing that can change it, and none exists.
2. **Evidence is P1.** One machine, one build, no CI reproduction and no
   independent reproduction (docs/35). Every measurement in this record says so,
   and none is promoted for looking stable.
3. **ADR-0044 is Proposed and unimplemented, deliberately.** The canonical
   digest stream is 22.8x the module it describes, because every count is a
   16-byte `u128` and the source map repeats six module-level identity strings
   in each of its 12 058 entries. That is 46.6 ms of a 47.5 ms verifier stage.
   By Architect direction it is a documented future improvement waiting on an
   operational reason, **not** a Stage 2 blocker: the current scheme is correct,
   receipts and caches are derived artifacts, and no accepted contract is
   violated by keeping it.
4. **The lowerer's boundary is measured against the contract, not the corpus.**
   The pattern gap the last audit dismissed on corpus grounds is closed:
   destructuring `let`, nested tuple patterns and single-arm tuple `match` all
   lower, execute and are covered by conformance vectors (ADR-0046). The
   corpus ratchet — 35 of 35 single-module accepted vectors reach the verifier
   and the engine — is retained as a *regression* gate and explicitly does not
   claim language coverage. `crates/tos-pipeline/tests/patterns.rs` and
   `match_matrix.rs` exercise the grammar's pattern and `match` families
   directly and require an **executed result** for valid source rather than
   accepting a refusal. The remaining `Gap` arms are guards against malformed
   input the checker has already rejected.
5. **ADR-0047 is Accepted and implemented.** `match` evaluates its subject once,
   considers arms in lexical source order, and runs the first whose pattern
   matches; exactly one body executes and later arms are unreachable.
   Unreachable arms are permitted and have no diagnostic. docs/40 section 4
   carries the rule, `tos-ir/v1` is unchanged, and
   `crates/tos-pipeline/tests/match_matrix.rs` requires an exact executed result
   for every shape the contract admits rather than accepting a diagnostic.

## architect_approval

*(empty — Stage 2 is a candidate for review, not closed. Only the Project
Architect grants this, and it is not the implementation's to write.)*
