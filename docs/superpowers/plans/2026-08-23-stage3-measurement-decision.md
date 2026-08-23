<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 Measurement Decision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans
> to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for
> tracking.

**Goal:** Publish ADR-0066 and land a green, test-only measurement instrument
whose current QEMU log result is retained honestly as insufficient diagnostic
evidence.

**Architecture:** Production TOS keeps its uncalibrated tick and gains no timing
API. A measurement-only CPL3 COM1 path emits causal markers observed by QEMU;
the fixed inner-call boundary is measured beside the empty channel floor, with
no subtraction and no IPC measurement until a lower-overhead observer is
validated.

**Tech Stack:** Rust `no_std`, Bash, Python 3, QEMU q35/qemu64/TCG, normative
Markdown and the existing preflight inventory.

## Global Constraints

- The production nucleus and runtime image must remain byte-for-byte unchanged.
- IOPL remains 0; the test TSS bitmap permits only `0x3f8..=0x3ff`.
- Exactly 3 warm-ups and 21 individual samples are retained.
- No batching, division, overhead subtraction, sample repair or fitted
  denominator is permitted.
- The current log observer is diagnostic evidence, not an IPC conformance gate.
- No direct edit of `TOS_DEVELOPMENT_SPECIFICATION.md`.

---

### Task 1: Make the measurement feature pass repository source gates

**Files:**
- Modify: `source/nucleus/src/exception.rs`
- Modify: `source/runtime-image/src/main.rs`
- Test: `scripts/check-unsafe-safety.py`

**Interfaces:**
- Consumes: existing `test-measurement-port` and `test-measurement-call` features.
- Produces: production and feature builds that are warning-free and formatted.

- [x] **Step 1: Reproduce each failure independently**

Run:

```sh
cd source
python3 ../scripts/check-unsafe-safety.py
cargo fmt --all -- --check
cargo clippy -p tos-nucleus --target x86_64-unknown-none -- -D warnings
```

Expected: unsafe-rationale failures at the three measurement wire functions,
the two long `cfg` attributes fail formatting, and production clippy reports
`IO_MAP_OFFSET` dead code.

- [x] **Step 2: Apply the smallest source-only correction**

Use local `// SAFETY:` statements directly above each unsafe function as the
checker requires, format the existing attributes with `cargo fmt`, and place
`IO_MAP_OFFSET` under the same measurement feature that consumes it. Do not
alter runtime behavior.

- [x] **Step 3: Re-run the three focused checks**

Expected: all three commands exit 0 with no warnings.

### Task 2: Add regression checks for the observer's fail-closed rules

**Files:**
- Modify: `source/host-tools/qemu-test/measure-channel.py`
- Create: `source/host-tools/qemu-test/test-measure-channel.py`
- Modify: `scripts/preflight.sh`

**Interfaces:**
- Consumes: `pair_markers`, `trace_markers`, marker family constants.
- Produces: a self-test gate that proves reversal, zero/negative intervals,
  mismatched/missing pairs and exact nearest-rank p99 handling.

- [x] **Step 1: Write failing standard-library unit tests**

The tests import `measure-channel.py` by path and assert that malformed marker
streams raise `Invalid`, including duplicate/unclosed/mismatched sequences.
They also assert 21-sample nearest-rank p99 selects the largest retained sample.

- [x] **Step 2: Run the test and observe the missing refusals**

Run:

```sh
cd source
python3 host-tools/qemu-test/test-measure-channel.py
```

Expected: tests for malformed pairing fail against the current permissive
pairing logic.

- [x] **Step 3: Make pairing total and fail closed**

Reject duplicate opens, closes without opens, mismatched sequences, unclosed
pairs and any retained count different from the requested count. Preserve all
valid sample values unchanged.

- [x] **Step 4: Register one self-test gate**

Add the test command as one `selftest` inventory entry. Do not add the actual
performance measurement to default preflight.

- [x] **Step 5: Verify red-to-green and parity**

Run the unit test, `./scripts/preflight.sh --list`, and the gate-parity self-test.
Expected: the new self-test passes and every CI profile still covers the single
authoritative inventory.

### Task 3: Publish accepted ADR-0066 and align normative documents

**Files:**
- Create: `docs/adr/0066-external-stage3-performance-observer.md`
- Modify: `docs/35_PERFORMANCE_CONTRACTS.md`
- Modify: `docs/adr/0049-stage3-interrupt-and-preemption-baseline.md`
- Modify: `source/interfaces/system/IPC_V1.md`
- Modify: `docs/SPECIFICATION_SOURCES.txt`
- Modify: `PROGRESS.md`
- Regenerate: `TOS_DEVELOPMENT_SPECIFICATION.md`

**Interfaces:**
- Consumes: Project Architect decision in the current task and ADR-0040 profile.
- Produces: one accepted Level 2 measurement contract with no Tier 1/Tier 2
  conflict.

- [x] **Step 1: Write ADR-0066 with the complete impact statement**

State the external-observer model, unchanged production time semantics, fixed
workloads, same-observer rule, fail-closed sampling, no-subtraction rule and P1/
P2 evidence boundary. Record Project Architect approval dated 2026-08-23.

- [x] **Step 2: Make lower-tier texts conform**

Clarify ADR-0049's calibration sentence without calibrating the tick; make
`docs/35` and `IPC_V1` name the external observer and retain both budgets.

- [x] **Step 3: Record the accepted source and regenerate**

Add ADR-0066 in numerical order, run `python3 tools/build-specification.py`,
and record the validated channel plus unresolved denominator in `PROGRESS.md`.

- [ ] **Step 4: Run documentation gates**

Run:

```sh
./scripts/preflight.sh --profile docs
```

Expected: every docs-profile gate passes, including manifest completeness and
generated-spec reproducibility.

### Task 4: Retain P1 diagnostic evidence without a false gate

**Files:**
- Create: `docs/evidence/STAGE3_MEASUREMENT_CHANNEL_P1.md`
- Create: `docs/evidence/stage3-measurement-log-floor.json`
- Create: `docs/evidence/stage3-measurement-log-inner-call.json`
- Modify: `source/host-tools/qemu-test/measurement-denominator.sh`

**Interfaces:**
- Consumes: raw fresh 3+21 reports from the same frozen tree.
- Produces: immutable P1 evidence labelled `diagnostic-insufficient`, including
  source/QEMU/firmware/host/profile identities and overlap.

- [x] **Step 1: Extend report identity before taking final samples**

Make the harness record source tree identity, QEMU version, firmware digests,
machine/cpu/vCPU/memory/accelerator, build mode, quantum/preemption state and
observer backend. A missing identity field invalidates evidence.

- [ ] **Step 2: Run the channel and inner-call series on one frozen tree**

Run `measurement-denominator.sh` once after all source changes. Preserve exactly
the two emitted JSON reports; do not copy earlier scratch results.

- [ ] **Step 3: Write the evidence interpretation**

Report raw samples, median/p99/min/max, floor/call ratios and overlap. State that
the semantic boundary passed but the log backend is not a conformance observer
and IPC was not measured.

- [ ] **Step 4: Verify production identity**

Hash ordinary production nucleus/runtime before and after the measurement build
and retain both equal pairs in the report.

### Task 5: Frozen-tree verification and commits

**Files:** all files above plus mechanically regenerated `MANIFEST.txt` and
`SHA256SUMS`.

- [ ] **Step 1: Refresh manifests mechanically**

Use the repository's existing manifest/hash generator discovered from current
scripts; do not edit either file manually.

- [ ] **Step 2: Run focused and full verification**

Run the observer self-test, targeted feature builds, denominator diagnostic and
`./scripts/preflight.sh --full` on the same frozen tree. Expected: every gate
passes; the diagnostic may report overlap but exits successfully because it is
reporting, not claiming conformance.

- [ ] **Step 3: Commit and push two signed-off results**

First commit the green ADR and instrument with a DCO `Signed-off-by` trailer.
Run the retained P1 series only from that clean exact SHA, then commit its report
and evidence as a second signed-off result. This avoids the circular and false
identity of claiming that an evidence file was measured from the commit that
first contains that file. Push `main` and verify `origin/main` names the second
SHA.

- [ ] **Step 4: Verify all repository-conformance CI profiles on that SHA**

Wait for Documentation, Source, QEMU and Provenance. Do not call the task green
until all four complete successfully on the pushed commit.
