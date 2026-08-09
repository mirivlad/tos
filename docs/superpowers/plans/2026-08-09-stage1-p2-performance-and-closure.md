<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 P2 performance and closure implementation plan

> **For agentic workers:** Execute inline under the Project Architect's
> accepted ADR-0026 decision. Keep each published commit signed and green.

**Goal:** Turn the accepted ADR-0026 Stage 1 performance contract into a
reproducible P2 CI artifact gate, then close F-18 only from that evidence and
complete F-21 only when the full Stage 1 closure matrix passes.

**Architecture:** The existing production full-path and isolated crypto scripts
remain the sole measurement implementations. A thin conformance orchestrator
calls them with identical fixture/provenance identities, asks the existing
Python report helper to enforce the accepted p95 ratio, and emits retained raw
CI evidence. No production loader, nucleus, parser or SHA behavior changes.

**Tech stack:** Existing Bash QEMU harnesses, production `tos-hash` test-only
baseline, Python report helper, GitHub Actions artifacts and generated release
metadata.

## Global constraints

- ADR-0026 is accepted Level 2 authority; its exact `docs/35` amendment is
  applied verbatim in substance.
- q35/qemu64/one-vCPU/256-MiB/TCG remains mandatory; KVM stays research-only.
- Full path retains two independent loader/nucleus validations; no cache shares
  a digest/parser result between them.
- The P2 fixture is exactly 1,000 files / 16 MiB with 101,203,198 SHA-256
  input bytes and 2,007 fresh invocations per boot.
- No unsafe SHA, assembly, mandatory CPU extension, external crypto dependency,
  capsule/BootInfo change or trust-boundary fusion is permitted.
- Do not mark F-18 PASS before an actual retained P2 CI run has ratio ≤1.30.
- Do not start Stage 1.5. Preserve `PROGRESS.md` byte-for-byte.

---

### Task 1: Accept ADR-0026 and synchronize Tier 2 authority

**Files:**

- Modify: `docs/adr/0026-stage1-validation-performance-metric.md`
- Modify: `docs/35_PERFORMANCE_CONTRACTS.md`
- Modify: `docs/SPECIFICATION_SOURCES.txt`
- Modify: `source/STAGE1_CLOSURE_AUDIT.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`
- Regenerate: `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt`, `SHA256SUMS`

- [ ] Mark ADR-0026 `Accepted (Project Architect-approved)` with Vladimir
  Tomashevskiy's approval date, preserving its evidence and the historical
  250-ms falsification record.
- [ ] Replace only the Stage 1 quantitative paragraph in docs/35 with the
  accepted paired full/crypto measurement requirements and p95 ≤1.30 bound.
- [ ] Add ADR-0026 to the ordered source list; verify the documentation CI
  rule sees no accepted ADR omission.
- [ ] Keep F-18 BLOCKER pending actual P2 artifact evidence.
- [ ] Regenerate/check documents and release metadata; commit with DCO.

### Task 2: Make P2 evidence status and ratio enforcement explicit

**Files:**

- Modify: `source/host-tools/qemu-test/stage1-native-performance.sh`
- Modify: `source/host-tools/qemu-test/crypto-baseline.sh`
- Modify: `source/tests/performance/stage1_capsule_workload.py`
- Modify: `scripts/tests/stage1-performance-workload.sh`

- [ ] Add RED regression inputs showing a crypto report cannot claim P2 unless
  requested and a ratio above 1.30 produces a nonzero result.
- [ ] Add `--evidence-status P1|P2` to native and QEMU crypto reports, then
  require matching evidence status/source/workload/accounting in ratio output.
- [ ] Add an explicit `--max-p95-ratio 1.30` check to the existing ratio helper;
  retain full/crypto raw measurements and record the applied bound in JSON.
- [ ] Run the focused workload and native-harness tests; commit with DCO.

### Task 3: Add one authoritative P2 conformance orchestrator and CI retention

**Files:**

- Create: `source/host-tools/qemu-test/stage1-performance-conformance.sh`
- Modify: `.github/workflows/qemu-boot.yml`
- Modify: `scripts/tests/stage1-performance-workload.sh` if a focused contract
  test is needed for the orchestrator's output names/ratio requirement

- [ ] The orchestrator calls, rather than copies, native full+crypto,
  qemu64/TCG full and qemu64/TCG crypto scripts; then calls the existing
  ratio/decomposition helper.
- [ ] It writes raw series, reports, fixture, provenance sidecar, ratio and
  decomposition beneath one caller-selected ignored `source/target/` directory.
- [ ] It accepts `--evidence-status P1|P2`; it supplies P2 only from the CI
  workflow and passes the accepted `--max-p95-ratio 1.30` to the helper.
- [ ] The QEMU workflow runs it after normal/negative boot gates and uploads
  all evidence with a 90-day retention policy, even on failure.
- [ ] Run an actual local P1 conformance series and the normal QEMU success /
  exception evidence; commit with DCO.

### Task 4: Promote F-18 only from the CI artifact

**Files:**

- Modify: `source/STAGE1_CLOSURE_AUDIT.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] After push, inspect the triggering GitHub Actions P2 artifact/run rather
  than treating local P1 as P2.
- [ ] Verify exactly 3 warmups + 21 measurements for native/full/native-crypto,
  TCG/full and TCG/crypto; matching fixture/source/accounting; segment output;
  ratio ≤1.30; normal events and raw QEMU exit 33.
- [ ] Record immutable CI run/artifact identity and set F-18 PASS only if that
  evidence is complete; regenerate/check metadata and commit with DCO.

### Task 5: Produce F-21 immutable Stage 1 report

**Files:**

- Create: `source/legal/release-manifests/<commit>-stage1-report.md`
- Modify: `source/STAGE1_CLOSURE_AUDIT.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Start only after every closure finding is PASS or accepted limitation.
- [ ] Generate a commit-addressed report with required docs/37 fields, exact
  tests/fixtures/limitations, G0, provenance/R0, P2 performance artifact and
  explicit Project Architect approval status.
- [ ] Audit every Stage 1 closure criterion against current code, gates and
  report, then run full preflight and required QEMU evidence.
- [ ] Commit/push the report with DCO and request formal Stage 1 closure only
  when the matrix has no blocker.
