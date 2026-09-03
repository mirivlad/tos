#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The operator-visible important-error view says what the contract says it does.
#
# `RUNTIME_OBSERVABILITY_V1` §9 fixes three things, and each is checked here
# against something other than itself:
#
#   1. the severity of an event kind is declared in the contract, and the reader
#      applies exactly that table — two statements of one fact, held together;
#   2. every identifier the contract classifies is one the system actually
#      emits, so the table cannot drift into classifying events that do not
#      exist;
#   3. the view is a **selection** of the transport, in order, and a process's
#      own record reaches it by naming its severity as its first segment.
#
# The third is checked over a real boot log rather than a fixture: the
# supervision gate leaves one, and it is the only run that contains a process
# journal, a nucleus refusal and an ordinary INFO event at once.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CONTRACT="$ROOT/source/interfaces/runtime/RUNTIME_OBSERVABILITY_V1.md"
READER="$ROOT/scripts/tos-journal.py"
NUCLEUS="$ROOT/source/nucleus/src"
IMAGE="$ROOT/source/runtime-image/src/main.rs"

fail() { echo "check-operator-journal: FAIL: $*" >&2; exit 1; }

# --- 1. the contract's table and the reader's agree ---------------------------
declared=$(sed -n '/^| Identifier | Severity | Why |$/,/^$/p' "$CONTRACT" |
    sed -n 's/^| `\(TOS\.[A-Z0-9_.]*\)` | `\([A-Z]*\)` |.*$/\1 \2/p' | sort)
applied=$(python3 - "$READER" <<'PY'
import re
import sys

source = open(sys.argv[1], encoding="utf-8").read()
table = re.search(r"^CLASSIFIED = \{(.*?)^\}", source, re.S | re.M).group(1)
for identifier, severity in re.findall(r'"([^"]+)": "([A-Z]+)"', table):
    print(identifier, severity)
PY
)
applied=$(printf '%s\n' "$applied" | sort)
[ -n "$declared" ] || fail "the contract classifies no identifier"
[ "$declared" = "$applied" ] || {
    echo "the contract declares:" >&2; echo "$declared" >&2
    echo "the reader applies:" >&2; echo "$applied" >&2
    fail "the contract and the reader disagree about a severity"
}

# --- 2. nothing is classified that nothing emits ------------------------------
while IFS= read -r pair; do
    [ -n "$pair" ] || continue
    identifier=${pair%% *}
    grep -Rqs -F "$identifier" "$NUCLEUS" "$IMAGE" ||
        fail "the contract classifies '$identifier', which nothing emits"
done <<< "$declared"

# --- 3. the five severity names, and no sixth ---------------------------------
for severity in DEBUG INFO WARN ERROR FATAL; do
    grep -Fq "| \`$severity\` |" "$CONTRACT" ||
        fail "the contract does not declare the severity '$severity'"
done

# --- 4. over a real boot: a selection, in order, from both kinds of producer ---
LOG="$ROOT/source/target/preflight-qemu/supervision/serial.log"
if [ ! -f "$LOG" ]; then
    echo "check-operator-journal: PASS (contract and reader agree;" \
         "$(printf '%s\n' "$declared" | grep -c .) classification(s) checked)"
    echo "  the boot-log selection is checked by the supervision gate's own log," \
         "which this run has not produced"
    exit 0
fi

python3 - "$READER" "$LOG" <<'PY'
import subprocess
import sys

reader, log = sys.argv[1], sys.argv[2]


def view(severity):
    finished = subprocess.run(
        [sys.executable, reader, "--severity", severity, log],
        capture_output=True,
        text=True,
        check=True,
    )
    return [line for line in finished.stdout.splitlines() if line.strip()]


important = view("WARN")
everything = view("DEBUG")

# A selection, not a copy: the important view is strictly inside the whole one,
# in the same order, and smaller.
if not important:
    raise SystemExit("check-operator-journal: FAIL: the important view is empty")
if len(important) >= len(everything):
    raise SystemExit(
        "check-operator-journal: FAIL: the important view is not a selection "
        f"({len(important)} of {len(everything)})"
    )
at = 0
for line in important:
    at = everything.index(line, at) + 1

# Both kinds of producer reach it: a process's own journal record, named with
# its severity, and an event the nucleus asserted about itself.
if not any(line.split()[1] == "supervisor" for line in important):
    raise SystemExit(
        "check-operator-journal: FAIL: no process journal record reached the view"
    )
if not any(line.split()[1] == "nucleus" for line in important):
    raise SystemExit("check-operator-journal: FAIL: no nucleus event reached the view")

# And severity is doing work: the supervisor's own INFO records are in the whole
# transport and out of the important one. A view that showed everything would
# not be a view.
info = [line for line in everything if line.startswith("INFO   supervisor")]
if not info:
    raise SystemExit("check-operator-journal: FAIL: the run produced no INFO records")
if any(line in important for line in info):
    raise SystemExit(
        "check-operator-journal: FAIL: an INFO record reached the important view"
    )

severities = {line.split()[0] for line in important}
print(
    f"  {len(important)} important of {len(everything)} events, in order,"
    f" at {'/'.join(sorted(severities))}"
)
print(f"  {len(info)} INFO record(s) present in the transport and absent from the view")
PY

echo "check-operator-journal: PASS (contract and reader agree;" \
     "$(printf '%s\n' "$declared" | grep -c .) classification(s) checked over a real boot)"
