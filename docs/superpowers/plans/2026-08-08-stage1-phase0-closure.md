<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 Phase 0 Closure Implementation Plan

> **For agentic workers:** Execute inline in the current session. Steps use
> checkbox (`- [ ]`) syntax for tracking; no subagents are authorized.

**Goal:** Restore all non-DCO current-main gates and add one preflight and one
human QEMU entrypoint that reuse the existing authoritative scripts.

**Architecture:** Asset binaries are licensed through a checked per-directory
record. Operational wrappers delegate to existing generators, gates and QEMU
harness; no boot logic is duplicated.

**Tech Stack:** POSIX shell/Bash, Python generators already in-tree, Cargo,
QEMU/OVMF/mtools, Markdown.

## Global constraints

- Preserve the existing uncommitted `PROGRESS.md` edit.
- Do not rewrite `main`, force-push or alter DCO policy.
- Do not implement Phase 1 or Stage 1.5.
- Every commit uses `git commit -s` and contains one reviewable concern.
- Regenerate `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt` and
  `SHA256SUMS` only with their existing generators.

---

### Task 1: Audit and Phase 0 design records

**Files:**

- Create: `source/STAGE1_CLOSURE_AUDIT.md`
- Create: `docs/superpowers/specs/2026-08-08-stage1-phase0-closure-design.md`
- Create: `docs/superpowers/plans/2026-08-08-stage1-phase0-closure.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Verify the audit has every required matrix column and explicitly includes
  the capsule alignment contradiction.
- [ ] Search for placeholders and unresolved scope language.
- [ ] Append the audited HEAD, commands and result summary to Worklog.
- [ ] Commit with `git commit -s -m "docs: record the formal Stage 1 closure audit"`.

### Task 2: Mascot licence and provenance gate

**Files:**

- Create: `assets/mascot/README.md`
- Modify: `assets/mascot/tos_ascii-art.txt`
- Modify: `assets/mascot/tos_ascii-art2.txt`
- Modify: `assets/mascot/tos_ascii-art3.txt`
- Modify: `scripts/check-spdx.sh`
- Create: `scripts/tests/check-spdx-assets.sh`
- Modify: `CONTRIBUTING.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Write a failing test that adds a temporary tracked PNG outside the
  recorded mascot list and expects `check-spdx.sh` to reject it.
- [ ] Run the test and confirm failure is caused by missing binary provenance.
- [ ] Add direct SPDX headers to text artwork and the explicit PNG record.
- [ ] Make `check-spdx.sh` accept only PNG paths declared in that record.
- [ ] Run the focused test and `sh scripts/check-spdx.sh`.
- [ ] Document binary asset provenance, direct text SPDX and `git commit -s`.
- [ ] Commit with `git commit -s -m "legal: record mascot artwork provenance"`.

### Task 3: Local preflight entrypoint

**Files:**

- Create: `scripts/preflight.sh`
- Create: `scripts/tests/preflight.sh`
- Modify: `CONTRIBUTING.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Write a failing CLI test for `--help`, unknown arguments and the ordered
  authoritative command list.
- [ ] Run it and confirm the entrypoint is missing.
- [ ] Implement default and `--full` modes by invoking existing scripts/tools.
- [ ] Run the CLI test and `bash -n scripts/preflight.sh`.
- [ ] Run default preflight, recording the expected sole DCO failure separately
  from any newly discovered failure.
- [ ] Commit with `git commit -s -m "tools: add one-command local preflight"`.

### Task 4: Human QEMU quick start

**Files:**

- Create: `run-tos.sh`
- Modify: `source/host-tools/qemu-test/run.sh`
- Create: `scripts/tests/run-tos.sh`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Write failing CLI/delegation tests for root `--help`, `--check`, missing
  target diagnostics and harness `--interactive` parsing.
- [ ] Run them and confirm the missing behavior.
- [ ] Add interactive presentation as a conditional around the shared QEMU
  argument list in the existing harness.
- [ ] Add the root build/delegation wrapper without copying ESP/QEMU logic.
- [ ] Run CLI tests and shell syntax checks.
- [ ] Run `./run-tos.sh --check` and confirm exit 0 with QEMU-TEST PASS.
- [ ] Exercise interactive missing-display diagnostics in the headless audit
  environment; do not claim a visible window without a graphical session.
- [ ] Commit with `git commit -s -m "tools: add human Stage 1 QEMU launcher"`.

### Task 5: Quick start and generated artifacts

**Files:**

- Modify: `README.md`
- Modify: `CONTRIBUTING.md`
- Modify: `TOS_DEVELOPMENT_SPECIFICATION.md` (generated)
- Modify: `MANIFEST.txt` (generated)
- Modify: `SHA256SUMS` (generated)
- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Add prerequisites, `./run-tos.sh`, `--check`, expected serial/result and
  honest Stage 1 limitations before the documentation map.
- [ ] Verify commands against actual `--help` output.
- [ ] Run `python3 tools/build-specification.py`.
- [ ] Stage all new tracked files, then run
  `python3 tools/build-release-manifest.py` so composition is complete.
- [ ] Run both generators with `--check`, SPDX, checksum verification, cargo
  fmt/test/clippy and `./run-tos.sh --check`.
- [ ] Append exact results to Worklog.
- [ ] Commit with `git commit -s -m "docs: add the Stage 1 quick start"`.

### Task 6: DCO stop gate

**Files:**

- Modify: `WORKLOG_STAGE1_HARDENING.md`

- [ ] Run `sh scripts/check-dco.sh` against current reachable HEAD.
- [ ] Record the exact three failing commit IDs and prove a later signed commit
  does not change their failure.
- [ ] Confirm all other Phase 0 gates independently.
- [ ] Commit the factual Worklog update with sign-off if it does not alter the
  unresolved history.
- [ ] Stop and request the owner's remediation decision. Do not start Phase 1.
