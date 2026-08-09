<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 Language Foundation Decision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to
> execute this plan task-by-task. The Project Architect's 2026-08-09 mandate
> authorizes inline execution through the single Proposed selection ADR; it
> does not authorize accepting that ADR or beginning Stage 2.

**Goal:** Produce reproducible, comparable evidence for one proposed Level 3
language-foundation selection that satisfies the TOS Core and execution-model
contracts.

**Architecture:** Research artifacts remain Tier 4 under
`docs/research/stage15/`; prototypes are explicit non-production evidence and
never become a normative parser, runtime, bytecode or standard library. The
Tier 2 requirements in `docs/05_TOS_CORE_LANGUAGE.md` and
`docs/06_EXECUTION_AND_IR.md` are the acceptance criteria. Only an accepted
selection ADR can promote a foundation into the Stage 2 contract.

**Tech Stack:** repository-local Markdown/JSON/CSV/Python/Rust research tools;
host Rust 1.97.1, Python 3.13.5 and, where useful, independently downloaded
upstream reference tools pinned by version and digest.

## Global constraints

- Preserve Tier 0 invariants and accepted Tier 1 ADRs; report rather than
  silently resolve conflicts.
- Canonical source remains UTF-8 human-readable text; IR, bytecode and native
  output are disposable derivatives.
- No Stage 2 production parser, runtime, permanent bytecode or standard
  library is implemented by this plan.
- Every final candidate runs the same corpus and multicore exercise; a
  blocking failure cannot be offset by a score.
- Research uses primary upstream specifications/repositories where available;
  each external fact records a URL, version/commit and access date.
- Every publishable commit carries `Signed-off-by`; generated specification,
  release manifest and checksums are regenerated whenever their inputs change.

## File map

- `docs/research/stage15/README.md` — scope, evidence status and reproduction
  entry points.
- `docs/research/stage15/methodology.md` — common corpus, host profile,
  measurement protocol, equivalence criteria and limitations.
- `docs/research/stage15/references.md` — primary-source bibliography with
  access date and exact claim.
- `docs/research/stage15/screening.md` — broad candidate screening and
  blocking rejections.
- `docs/research/stage15/finalists/*.md` — one complete report per finalist.
- `docs/research/stage15/prototypes/` — non-production sources, test vectors,
  run scripts and pinned tool metadata.
- `docs/research/stage15/measurements/` — immutable raw sample records and
  derived summaries.
- `docs/research/stage15/STAGE15_REPORT.md` — final evidence report.
- `docs/adr/0027-language-foundation-selection.md` — the single Proposed
  Level 3 selection ADR; it remains Proposed pending Project Architect review.
- `PROGRESS.md` — current Stage 1.5 state only.

---

### Task 1: Establish the research corpus and measurement protocol

**Files:**

- Create: `docs/research/stage15/README.md`
- Create: `docs/research/stage15/methodology.md`
- Create: `docs/research/stage15/prototypes/common/README.md`
- Create: `docs/research/stage15/prototypes/common/cases.json`
- Create: `docs/research/stage15/prototypes/common/measure.py`

- [ ] Define stable malformed-source, capability, driver-state-machine,
  privileged-operation, fuel, source-map, cache and two-engine vectors in
  `cases.json`, each with an ID and expected semantic result.
- [ ] Define the partitioned deterministic multicore workload: 64 equal ranges
  over fixed integer input, a fixed 64-bit reduction and a work ledger that
  records worker identity and overlap without treating timing as semantic
  output.
- [ ] Implement `measure.py` to record 3 warmups and 21 samples in JSON with
  command, UTC timestamp, host topology, OS, tool version, worker count,
  elapsed nanoseconds, result digest and overlap evidence.
- [ ] Run `python3 docs/research/stage15/prototypes/common/measure.py --help`
  and validate the JSON schema using the same script.
- [ ] Commit the corpus and methodology as `research: add Stage 1.5 common
  evaluation corpus`.

### Task 2: Research and screen candidate classes

**Files:**

- Create: `docs/research/stage15/references.md`
- Create: `docs/research/stage15/screening.md`
- Modify: `docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md` only if a
  discovered contradiction with Tier 2 requirements requires a documented
  correction; otherwise leave it unchanged.

- [ ] Record primary sources for bespoke TOS Core, WebAssembly/formal-core,
  Rust, Pony, Go and any replacement serious candidate, including licence and
  version/commit.
- [ ] Evaluate every candidate against all 15 blocking requirements before
  comparative scoring.
- [ ] Select a small set of finalists only when no blocking failure has been
  hidden by an adapter that would itself be the unselected language foundation.
- [ ] Run link and source-record checks with
  `python3 docs/research/stage15/prototypes/common/measure.py --validate-only`.
- [ ] Commit screening evidence as `research: screen Stage 1.5 language
  candidates`.

### Task 3: Build the bespoke TOS Core evidence prototype

**Files:**

- Create: `docs/research/stage15/prototypes/bespoke/`
- Create: `docs/research/stage15/finalists/bespoke-tos-core.md`

- [ ] Write an intentionally non-normative, bounded lexer/parser/type/IR model
  that consumes the common corpus and produces deterministic diagnostics.
- [ ] Model non-forgeable capability tokens, owned/borrowed/shared regions,
  fuel, source spans, cache identities, structured parallel tasks, join,
  cancellation, atomic ordering and bounded worker/task accounting.
- [ ] Run the complete common corpus in a serialized reference mode and a
  production-capable parallel prototype mode; preserve the same semantic
  result digest.
- [ ] Record 1/2/N-worker raw samples, overlap evidence, dependencies, trusted
  code size and known incompleteness.
- [ ] Commit as `research: prototype bespoke TOS Core foundation`.

### Task 4: Build the formal-core hybrid evidence prototype

**Files:**

- Create: `docs/research/stage15/prototypes/formal-core/`
- Create: `docs/research/stage15/finalists/formal-core-hybrid.md`

- [ ] Use a pinned formal execution-core tool only as a disposable backend;
  retain a human-readable TOS-like surface and independently state what the
  verifier trusts.
- [ ] Map every common-corpus operation, especially capabilities, bounded
  loops, shared memory, atomics and structured parallelism, to verifier-visible
  contracts.
- [ ] Run identical serialized/parallel semantic vectors and multicore
  measurement protocol.
- [ ] Record whether the core's memory/concurrency semantics or host ABI makes
  the class a blocking failure.
- [ ] Commit as `research: prototype formal-core language foundation`.

### Task 5: Build the adapted existing-language evidence prototype

**Files:**

- Create: `docs/research/stage15/prototypes/adapted-rust/`
- Create: `docs/research/stage15/finalists/adapted-rust.md`

- [ ] Define the restriction boundary explicitly: no ambient `std` authority,
  no unrestricted unsafe code, capability tokens supplied only by verifier
  contracts, fixed source/IR identity and bounded task creation.
- [ ] Use production Rust ownership, atomics and scoped parallel work to run
  the common corpus and demonstrate the safe rejection of invalid sharing.
- [ ] Run a separately compiled serialized/reference-compatible mode and
  parallel mode, recording exactly where Rust language semantics end and TOS
  runtime policy begins.
- [ ] Record transitive dependencies, compiler/runtime footprint, host ABI
  exposure and recovery bootstrap implications.
- [ ] Commit as `research: prototype adapted Rust foundation`.

### Task 6: Compare final candidates and validate evidence

**Files:**

- Create: `docs/research/stage15/measurements/*.json`
- Create: `docs/research/stage15/measurements/SUMMARY.md`
- Modify: `docs/research/stage15/finalists/*.md`

- [ ] Re-run every finalist from a clean generated-output directory using its
  recorded command.
- [ ] Verify all common vector result digests agree for their declared semantic
  profile; classify an inability to do so as a blocking failure.
- [ ] Compare parser/lowering/verification/cold-start/memory and 1/2/N-worker
  samples only within identical workload/host conditions; do not create a
  false single numeric ranking.
- [ ] Verify source maps, cache invalidation, bounds and unsafe/native
  boundaries with focused negative evidence.
- [ ] Commit validated raw data and summary as `research: measure Stage 1.5
  finalists`.

### Task 7: Publish the decision-ready evaluation and Proposed ADR

**Files:**

- Modify: `docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md`
- Create: `docs/research/stage15/STAGE15_REPORT.md`
- Create: `docs/adr/0027-language-foundation-selection.md`
- Modify: `PROGRESS.md`

- [ ] Complete a PASS/FAIL matrix with exact evidence references, screening
  rejections, finalist comparison, canonical-source, verifier-boundary,
  memory/concurrency, capability, driver, host ABI, licensing/patent and
  scalability analyses.
- [ ] Draft ADR-0027 with `Status: Proposed` and the exact chosen foundation,
  semantic boundary, migration consequences, accepted scope and Stage 2 first
  work; do not set `Accepted`.
- [ ] State the runner-up and its exact blocking or decisive loss reason.
- [ ] Run source/release/SPDX checks and every prototype reproduction command;
  record their command output and commit identities.
- [ ] Commit as `research: propose Stage 1.5 language foundation` and stop for
  the Project Architect decision.

### Task 8: Acceptance-only closure (not authorized until a future explicit ACCEPT)

**Files:**

- Modify: `docs/adr/0027-language-foundation-selection.md`
- Modify: `docs/05_TOS_CORE_LANGUAGE.md`, `docs/06_EXECUTION_AND_IR.md`, and
  `docs/16_DEVELOPMENT_STAGES.md` only to reconcile the accepted decision.
- Create: immutable Stage 1.5 final evidence record under `source/legal/` if
  the accepted Stage 1.5 gate/report convention requires it.

- [ ] Wait for a Project Architect `ACCEPT`; no Stage 2 implementation occurs
  before that message.
- [ ] Change ADR status only after that acceptance, regenerate all derived
  release artifacts, run complete Stage 1.5 gates and full preflight, archive
  the report and update `PROGRESS.md`.
- [ ] Commit and push the acceptance/closure record with DCO, verify
  `origin/main`, then declare Stage 1.5 closed.
