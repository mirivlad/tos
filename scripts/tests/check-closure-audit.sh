#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A stage closure audit names gates that exist, and verdicts that are real.
#
# A closure audit is only worth reading if its right-hand column can be
# followed. This holds every gate it names against `scripts/preflight.sh`'s own
# inventory, so a gate that is renamed or removed makes the audit fail rather
# than quietly become fiction — and checks that every verdict is one of the four
# the document admits, because "mostly done" is the failure mode a closure audit
# exists to prevent.
#
# **One checker for every stage's audit**, selected by the argument: each stage
# states its own verdicts, and a verdict one audit admits is not thereby admitted
# by another.
#
#   check-closure-audit.sh [stage3|stage4]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
STAGE="${1:-stage3}"
case "$STAGE" in
    stage3)
        AUDIT="$ROOT/docs/evidence/STAGE3_CLOSURE_AUDIT.md"
        VERDICTS='CLOSED|ENVIRONMENT-ONLY|OPEN — blocks Stage 3|OUT OF STAGE 3 by accepted decision'
        MIN_ROWS=40 ;;
    stage4)
        AUDIT="$ROOT/docs/evidence/STAGE4_CLOSURE_AUDIT.md"
        VERDICTS='PASS|PARTIAL|MISSING|NONCLAIM'
        MIN_ROWS=60 ;;
    *) echo "usage: $0 [stage3|stage4]" >&2; exit 2 ;;
esac
PREFLIGHT="$ROOT/scripts/preflight.sh"

fail() { echo "check-closure-audit: FAIL: $*" >&2; exit 1; }

[ -f "$AUDIT" ] || fail "the closure audit is missing"

# Every gate function the inventory declares.
declared=$(sed -nE 's/^gate +[a-z]+ +[a-z-]+ +"[^"]+" +([a-z0-9_]+)$/\1/p' "$PREFLIGHT" | sort -u)
[ -n "$declared" ] || fail "the preflight inventory declares no gates"

# Every name the audit cites in an evidence cell that looks like a gate: a
# backticked lower-case identifier with an underscore, which is the shape a
# gate function has and a prose word does not.
cited=$(grep -oE '`[a-z][a-z0-9_]*_[a-z0-9_]*`' "$AUDIT" | tr -d '`' | sort -u)
[ -n "$cited" ] || fail "the audit cites no gate"

missing=0
for name in $cited; do
    # Rust test files and source paths are cited too; only check names that are
    # not obviously one of those.
    case "$name" in
        *_rs|*_md|*_tos|*_sh|*_py) continue ;;
    esac
    if ! printf '%s\n' "$declared" | grep -qx "$name"; then
        # A cited name that is not a gate must be something else that exists:
        # a Rust item or a file. Anything else is a dangling citation.
        if ! grep -Rqs --exclude-dir=target --include='*.rs' --include='*.sh' \
            --include='*.py' --include='*.tos' -- "$name" \
            "$ROOT/source" "$ROOT/scripts"; then
            echo "  dangling: $name" >&2
            missing=$((missing + 1))
        fi
    fi
done
[ "$missing" = 0 ] || fail "$missing name(s) in the audit refer to nothing"

# Every verdict is one of the four, and the summary counts what the table says.
# Counted over the numbered requirement rows only, which is what the summary
# claims to count — a rule stated in the document and applied here, so the two
# cannot drift.
python3 - "$AUDIT" "$VERDICTS" "$MIN_ROWS" <<'PY' || fail "the audit's verdicts and its summary disagree"
import re
import sys
from collections import Counter

VERDICTS = set(sys.argv[2].split("|"))
text = open(sys.argv[1], encoding="utf-8").read()
rows = [line for line in text.splitlines() if re.match(r"^\| \d+\.\d+ \|", line)]
if len(rows) < int(sys.argv[3]):
    raise SystemExit(f"only {len(rows)} requirement row(s)")
counted = Counter(row.rsplit("|", 2)[1].strip() for row in rows)
for verdict in counted:
    if verdict not in VERDICTS:
        raise SystemExit(f"a row records {verdict!r}, which is not one of this audit's verdicts")

claimed = {}
for verdict, number in re.findall(r"^\| \**([A-Z][^|*]*?)\** \| \**(\d+)\** \|$", text, re.M):
    claimed[verdict.strip()] = int(number)
for verdict in VERDICTS:
    if claimed.get(verdict, 0) != counted.get(verdict, 0):
        raise SystemExit(
            f"the table has {counted.get(verdict, 0)} {verdict!r} and the summary "
            f"claims {claimed.get(verdict, 0)}"
        )
total = re.search(r"of which there are (\d+)", text)
if not total or int(total.group(1)) != len(rows):
    raise SystemExit(f"the summary does not say there are {len(rows)} requirement rows")
print(
    f"  {len(rows)} requirement row(s): "
    + ", ".join(f"{count} {verdict}" for verdict, count in sorted(counted.items()))
)
PY

echo "check-closure-audit: PASS ($(printf '%s\n' "$cited" | grep -c .) cited name(s) resolved)"
