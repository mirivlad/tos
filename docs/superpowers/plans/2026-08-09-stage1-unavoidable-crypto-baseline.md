<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 unavoidable-crypto baseline implementation plan

> **For agentic workers:** Execute inline under the Project Architect's
> 2026-08-09 F-18 direction. Every code task starts with a failing regression.

**Goal:** Measure the exact cryptographic work required by the current two
independent Stage 1 validators, account for its input bytes, and decide from
evidence whether a Level-2 performance-contract proposal is supportable.

**Architecture:** A feature-gated, no-allocation capsule helper replays only
the successful parser's mandatory SHA-256 operations over an already validated
borrowed capsule: each file digest, detached identity and whole-capsule digest.
It recomputes and compares every digest on each invocation; it does not retain
a prior hash. Native and isolated-QEMU test runners invoke that helper twice
per timed sample. The default loader, nucleus and QEMU success path do not
select the feature or test artifact.

**Tech stack:** existing `tos-capsule`, production dependency-free `tos-hash`,
Rust release harnesses, the existing `run.sh`/serial timestamp capture, Python
report helpers and no new dependency.

## Global constraints

- Keep capsule v1 bytes, parser results, error precedence, source identity and
  two independent production validations unchanged.
- Do not add unsafe SHA, assembly, CPU extensions or an external crypto crate.
- The normal q35/qemu64/TCG profile remains the functional profile; KVM is
  explicit research only.
- Baseline accounting includes two parser whole-capsule passes, two additional
  mandatory BootInfo-mirror whole-capsule passes (one in loader and one in
  nucleus), two per-file hash sequences and two detached-identity sequences
  for the same detached fixture.
- `parse` used to establish an immutable borrowed view is outside the baseline
  timer; no digest computed by it is passed to either timed crypto pass.
- Record three warm-ups and 21 measurements for each profile. Do not declare
  F-18 PASS or ADR-0026 Accepted.

---

### Task 1: feature-gated exact crypto replay

**Files:**

- Modify: `source/crates/capsule/Cargo.toml`
- Modify: `source/crates/capsule/src/lib.rs`
- Modify: `source/tests/performance/Cargo.toml`
- Modify: `source/tests/performance/src/main.rs`
- Test: `source/tests/performance/src/main.rs`

**Interfaces:**

- Produces `tos_capsule::test_crypto_baseline::verify(&Capsule) ->
  Result<CryptoAccounting, CapsError>` behind feature
  `test-crypto-baseline`.
- `CryptoAccounting` records hash invocation count and bytes fed to SHA-256 for
  one logical validator. `verify` rehashes each file, detached identity and
  whole capsule, comparing every fresh output to the encoded expected value.
- The native binary accepts `--mode full|crypto`; crypto records state
  `validations=2`, mode, accounting and duration.

- [ ] Add a test that requests `--mode crypto` before the mode exists; verify
  it fails as an unknown option.
- [ ] Add a unit test that a detached two-file capsule reports the two file
  hashes plus detached and whole hashes, and that two calls produce equal
  accounting without retaining a result.
- [ ] Implement the feature-gated helper using `tos_hash::Sha256`, then make
  the native runner call it twice inside the timer after an untimed `parse`.
- [ ] Run `cargo test -p tos-stage1-performance` and the shell native-harness
  regression; confirm both full and crypto records have two fresh passes.
- [ ] Commit the test-only baseline with DCO sign-off.

### Task 2: isolated QEMU crypto profile and reports

**Files:**

- Modify: `source/nucleus/Cargo.toml`
- Modify: `source/nucleus/src/main.rs`
- Create: `source/host-tools/qemu-test/crypto-baseline.sh`
- Modify: `source/tests/performance/stage1_capsule_workload.py`
- Modify: `scripts/tests/stage1-performance-workload.sh`

**Interfaces:**

- A `test-crypto-baseline` nucleus feature is built only beneath
  `target/test-crypto-baseline`; it emits test-only start/done markers after
  an untimed setup parse and exits through the existing debug-exit path.
- The normal nucleus target is hashed before and after the isolated build.
- Report helpers validate 3+21 raw samples, equal fixture accounting and emit
  `full / unavoidable_crypto` ratios for native and QEMU.

- [ ] Add shell regressions for an unknown crypto report mode and a missing
  test marker; verify they fail.
- [ ] Add the isolated feature, runner and report validation with default
  artifact hash assertions; the standard `run.sh` path receives no new flag.
- [ ] Run normal QEMU exit 33 and an isolated one-sample crypto run to prove
  marker ordering and artifact separation.
- [ ] Commit the isolated profile with DCO sign-off.

### Task 3: evidence and architecture conclusion

**Files:**

- Modify: `source/STAGE1_CLOSURE_AUDIT.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`
- Create only if supported: `docs/adr/0026-stage1-validation-performance-metric.md`
- Modify only if supported: `docs/35_PERFORMANCE_CONTRACTS.md`

- [ ] Run native, qemu64/TCG and available KVM research 3+21 full and crypto
  series from the same fixture; retain raw JSONL/reports under ignored
  `source/target/`.
- [ ] Verify byte accounting agrees across native/QEMU and state the exact
  whole, file and detached-identity bytes/hash calls.
- [ ] Compute ratios and residual overhead. If crypto does not explain a
  substantial enough fraction to support the new metric, record the evidence
  and stop without ADR-0026.
- [ ] If evidence supports it, write ADR-0026 as **Proposed** with exact
  `docs/35` normative diff, falsified-250-ms classification, unchanged hard
  budgets and 15%/30% regression policy. Regenerate documentation artifacts,
  run gates, commit and request Architect review.
