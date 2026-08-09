<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 Part A: TOS Core V1 semantics and IR contract plan

> **For agentic workers:** This plan is documentation-only. Do not implement a
> production parser, checker, IR verifier, interpreter, cache, or runtime
> before the Project Architect accepts the resulting contract.

**Goal:** Publish a complete, reviewable Proposed TOS Core V1 contract,
programmer documentation, and conformance corpus that can be accepted before
Stage 2 production implementation begins.

**Architecture:** Keep canonical programs as normalized UTF-8 `.tos` source;
state all language semantics in TOS-owned numbered Tier 2 documents; keep the
typed IR derived and independently verified. The Bootstrap profile is a strict
subset of the same V1 semantics, while Full has an SMP-capable structured
parallelism path.

**Tech stack:** Markdown specifications and canonical `.tos` example sources;
existing deterministic specification and release-manifest generators. No new
runtime dependency or production code is permitted.

## Global constraints

- ADR-0027 is accepted and fixes the language/trust boundary; this plan fills
  in detailed semantics only as **Proposed** contracts pending the required
  Stage 2 Architect decision.
- Preserve Tier 0 invariants, accepted ADRs, source identity, cache
  disposability, capability non-forgeability, bounded Bootstrap, and no safe
  data-race undefined behavior.
- Keep Rust, LLVM, libc, C ABI, host threads, Wasm, and other VMs outside the
  TOS runtime contract unless a later accepted ADR admits a narrow role.
- Examples are proposed canonical source, not claims of an implemented
  frontend; every `.tos` source has an SPDX header.
- Do not start Stage 3 or a Stage 2 production implementation.

---

### Task 1: Establish the Proposed V1 authority boundary

**Files:**
- Create: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`
- Create: `docs/adr/0028-tos-core-v1-semantics-and-ir-contract.md`
- Modify: `docs/SPECIFICATION_SOURCES.txt`
- Modify: `docs/05_TOS_CORE_LANGUAGE.md`
- Modify: `docs/06_EXECUTION_AND_IR.md`
- Modify: `PROGRESS.md`

- [ ] Define the version, status, authority, and exact Stage 1.5/Stage 2
  boundary; link the complete V1 document set without treating its proposal as
  implementation authority.
- [ ] Define source normalization, lexical rules, deterministic EBNF grammar,
  parser recovery, module header, and profile declaration.
- [ ] Record an ADR-0028 impact statement and the single Architect decision
  required before production code.
- [ ] Update source-list ordering and progress status.

### Task 2: Define static, ownership, and execution semantics

**Files:**
- Create: `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`
- Create: `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`

- [ ] Define fixed-width types, conversions, effects, explicit capability
  values, deterministic evaluation, checked arithmetic, and Result/panic
  behavior.
- [ ] Define TOS-owned affine ownership, borrows, regions, shared/DMA handles,
  safe/unsafe boundary, drop/cleanup, and address-width independence.
- [ ] Define async and parallel scopes, join/cancellation, safe sharing,
  synchronization, atomics, happens-before, scheduling neutrality, worker and
  task accounting, Bootstrap restrictions, diagnostic identity and precedence.

### Task 3: Define modules, IR, verifier, provenance, and compatibility

**Files:**
- Create: `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`
- Create: `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`
- Create: `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`

- [ ] Define deterministic module/import resolution, module initialization,
  capability request/grant/transfer/attenuation rules, resource declarations,
  profile/version negotiation, and future FFI containment.
- [ ] Define versioned typed IR, lowering proof obligations, independent
  verifier checks/error precedence, runtime-call visibility, source maps, and
  disposable cache identity.
- [ ] Define backend-neutral accept/reject vectors, expected diagnostics,
  cross-engine and multicore requirements, fuzz/performance evidence, and the
  threat/complexity/implementability review.

### Task 4: Make the proposed language teachable and testable

**Files:**
- Create: `docs/language/TOS_CORE_V1_GUIDE.md`
- Create: `docs/language/LEARNING_TOS_CORE.md`
- Create: `docs/language/EXAMPLE_STATUS.md`
- Create: `docs/language/examples/README.md`
- Create: `docs/language/examples/*.tos`
- Create: `docs/language/conformance/v1/README.md`
- Create: `docs/language/conformance/v1/{accept,reject}/*.tos`
- Create: `docs/language/conformance/v1/EXPECTATIONS.md`

- [ ] Explain the proposed language separately from the normative documents;
  label all features as specified/proposed and not yet executable.
- [ ] Give canonical source examples for ordinary syntax and deliberate
  failures for diagnostics, ownership, capabilities, resources, and
  parallelism.
- [ ] Add the required documentation-status matrix and use one canonical
  source per significant guide/tutorial example.

### Task 5: Validate, publish, and stop

**Files:**
- Modify: `TOS_DEVELOPMENT_SPECIFICATION.md` (generated only)
- Modify: `MANIFEST.txt` (generated only)
- Modify: `SHA256SUMS` (generated only)
- Modify: `PROGRESS.md`

- [ ] Run specification, manifest, SPDX/provenance, documentation, DCO, and
  full preflight gates; repair only documentation-level failures.
- [ ] Commit coherent signed-off documentation changes and push `origin/main`.
- [ ] Report every proposed semantic choice, unresolved question, hierarchy
  impact, commands and evidence, then request exactly:
  `PROJECT ARCHITECT — STAGE 2 SEMANTICS/IR CONTRACT: ACCEPT / REJECT`.
