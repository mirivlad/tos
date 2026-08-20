#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the CI/preflight parity gate.
#
# A parity gate that is only ever green is a parity gate nobody has tested, and
# this one exists because a rule stated in prose went unenforced for two days.
# So each way it must fail is produced, on a copy of the repository, and each
# way it must **not** fail is produced beside it — an environment step without a
# local counterpart is allowed by ADR-0065 and a gate that rejected it would
# make every workflow lie about its setup.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECKER="$ROOT/scripts/check-gate-parity.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

REPO="$TMP/repo"
mkdir -p "$REPO/scripts/tests" "$REPO/.github/workflows"
cp "$CHECKER" "$REPO/scripts/"

fail() {
    echo "check-gate-parity self-test: FAIL: $*" >&2
    exit 1
}

# A miniature inventory with the same shape as the real one: `--list` prints
# profile, scope and label and runs nothing.
write_inventory() {
    cat > "$REPO/scripts/preflight.sh" <<EOF
#!/bin/sh
set -eu
[ "\${1-}" = --list ] || { echo "self-test stub: only --list" >&2; exit 2; }
printf 'docs\tdefault\tone\n'
printf 'source\tdefault\ttwo\n'
$1
EOF
    chmod +x "$REPO/scripts/preflight.sh"
}

write_workflow() {
    cat > "$REPO/.github/workflows/gates.yml" <<EOF
name: gates
on: [push]
jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: docs
        run: bash scripts/preflight.sh --profile docs
$1
EOF
}

SOURCE_JOB="  source:
    runs-on: ubuntu-latest
    steps:
      - name: source
        run: bash scripts/preflight.sh --profile source"

check() { python3 "$REPO/scripts/check-gate-parity.py" --root "$REPO" >/dev/null 2>&1; }

# --- the state that must pass --------------------------------------------------
write_inventory ""
write_workflow "$SOURCE_JOB"
check || fail "an inventory whose every profile has a job was rejected"

# --- 1. a profile left out of CI ------------------------------------------------
# The gate cannot be removed from a workflow, because a workflow never names one;
# what can happen is that its profile stops being run, and that is this case.
write_workflow ""
check && fail "a profile no job runs was accepted"

# --- 2. a new gate in a profile nothing runs ------------------------------------
write_inventory "printf 'qemu\tfull-only\tthree\n'"
write_workflow "$SOURCE_JOB"
check && fail "a gate added in a profile no job runs was accepted"

# --- 3. an environment step needs no local counterpart --------------------------
write_inventory ""
write_workflow "$SOURCE_JOB
      - name: install things
        env:
          GATE_PARITY: environment
        run: sudo apt-get install -y something"
check || fail "an environment step was treated as a repository gate"

# --- and a command that is neither ----------------------------------------------
# The guard against a repository check written in YAML: a step that runs
# something and does not say it is environment is a second implementation.
write_workflow "$SOURCE_JOB
      - name: a check written in YAML
        run: python3 tools/build-specification.py --check"
check && fail "a repository check written as a workflow step was accepted"

# --- a marker that is a comment rather than YAML --------------------------------
# ADR-0065 requires the marker to be part of the document a parser sees. A
# comment claiming it is not the same thing, and must not be accepted as one.
write_workflow "$SOURCE_JOB
      # GATE_PARITY: environment
      - name: a check with a comment where the marker belongs
        run: python3 tools/build-specification.py --check"
check && fail "a comment was accepted as the environment marker"

# --- a profile the inventory does not declare ------------------------------------
write_workflow "$SOURCE_JOB
      - name: typo
        run: bash scripts/preflight.sh --profile sorce"
check && fail "a workflow running a profile that selects nothing was accepted"

echo "check-gate-parity self-test: PASS (five refusals and two acceptances)"
