#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A process creation is a transaction (ADR-0076 §2a).
#
# Nine ways a creation can fail — eight driven deliberately on a real machine,
# with the pool, the page-table reserve and the authority tree measured on both
# sides of it. What the gate asserts is that every one of those numbers is the
# same afterwards as before — not that the failure was reported.
#
# The ninth is not an injection at all: an endowment the launcher decided on
# that cannot be written whole. ADR-0055 makes a half-endowed child invalid, so
# the creation is refused before the process exists — and the capability table
# is measured with the rest, because a refusal that left two of three grants
# behind would be exactly the leak the preflight exists to prevent.
#
# Two of the cases exist for defects that no count of successful boots would
# find. `grant-table` refuses the reserve at the one instant a user frame has
# left the pool and its leaf does not yet exist: an unwinding that reads owned
# frames back out of the page tables walks straight past that frame, so it
# leaked, silently, once per refused creation. `record-mapping` refuses part-way
# through mapping the carved launch record, which leaves a run the page tables
# only partly name.
#
# The build afterwards is the ordinary one: a machine that has refused eight
# creations must still start the process it was asked for, out of the memory
# the refusals gave back.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:?usage: creation-rollback.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "creation-rollback: FAIL: $*" >&2; exit 1; }

(cd "$ROOT" && cargo build --release -p tos-nucleus \
    --target x86_64-unknown-none --features test-creation-rollback) \
    > "$OUT/build.log" 2>&1 || { cat "$OUT/build.log" >&2; fail "the build did not"; }

NUCLEUS_IN="$ROOT/target/x86_64-unknown-none/release/tos-nucleus" \
    bash "$HERE/run.sh" --out "$OUT/boot" --expect 33 > "$OUT/boot.log" 2>&1 || {
    cat "$OUT/boot.log" >&2
    fail "the machine did not finish its ordinary boot after the refusals"
}

python3 - "$OUT/boot/serial.log" <<'PY'
import re
import sys

serial = open(sys.argv[1], "rb").read().decode("utf-8", "replace").replace("\r", "")

# Every failure the transaction has to survive. The first three are refused
# before anything is allocated at all; the rest happen with the machine already
# part-way into building a process.
EXPECTED = [
    "bad-header",
    "record-too-large",
    "over-budget",
    "data-frame",
    "grant-frame",
    "grant-table",
    "record-carve",
    "record-mapping",
    "endowment",
]

seen = {}
for line in serial.splitlines():
    if not line.startswith("TOS.RUN.CREATE_ROLLBACK "):
        continue
    fields = dict(re.findall(r"(\w+)=(\S+)", line))
    seen[fields["case"]] = fields

missing = [case for case in EXPECTED if case not in seen]
if missing:
    raise SystemExit(f"creation-rollback: FAIL: no evidence for {', '.join(missing)}")

for case in EXPECTED:
    fields = seen[case]
    if fields["refused"] != "1":
        raise SystemExit(
            f"creation-rollback: FAIL: {case} did not refuse the creation"
        )
    for name in ("pool", "tables", "free", "committed", "capabilities"):
        before, after = fields[name].split("/")
        if before != after:
            raise SystemExit(
                f"creation-rollback: FAIL: {case} left {name} at {after}, was {before}"
            )
    if fields["holds"] != "1":
        raise SystemExit(f"creation-rollback: FAIL: {case} broke the tree's accounting")
    if fields["diverged"] != "0":
        raise SystemExit(f"creation-rollback: FAIL: {case} diverged the pool and the tree")
    print(f"  {case}: refused, and the machine is as it was")

# And the boot went on to build the process it was actually asked for.
if "TOS.RUN.PROCESS_BEGIN " not in serial:
    raise SystemExit(
        "creation-rollback: FAIL: no process was built after the refusals"
    )

print(f"CREATION-ROLLBACK PASS: {len(EXPECTED)} failures, nothing left behind by any")
PY
