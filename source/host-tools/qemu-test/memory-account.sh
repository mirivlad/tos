#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The unified memory account, checked as arithmetic (ADR-0076 §2, §8).
#
# `TOS.MEM.ACCOUNT` and `TOS.RUN.PROCESS_CHARGE` are telemetry until something
# checks that the numbers in them add up. Presence gates do not: a nucleus that
# printed four plausible constants would pass one. What this gate asserts is the
# three equalities the funding model rests on, and the negative that says a
# refusal costs nothing.
#
#   admitted_frames == table_reserve_frames + pool_frames
#       the reserve physically left the pool, once, before anything promised it
#
#   root_frames == pool_frames
#       the root authority is endowed over exactly what is left, with no second
#       subtraction of the reserve
#
#   root_frames == available_after_reclaim
#       and after the funded process has been built and reclaimed, the pool is
#       back to what the root was endowed with — the charge returned exactly
#       what it took
#
#   process charge total == frames reclaimed × 4096
#       what the creation said it would cost is what the reclamation handed
#       back, so the price was known rather than discovered
#
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:?usage: memory-account.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "memory-account: FAIL: $*" >&2; exit 1; }

bash "$HERE/run.sh" --out "$OUT/boot" --expect 33 > "$OUT/boot.log" 2>&1 || {
    cat "$OUT/boot.log" >&2
    fail "the ordinary boot did not pass"
}

python3 - "$OUT/boot/serial.log" <<'PY'
import re
import sys

serial = open(sys.argv[1], "rb").read().decode("utf-8", "replace").replace("\r", "")

def fields(line):
    return dict(re.findall(r"(\w+)=(-?\d+)", line))

def one(prefix):
    found = [l for l in serial.splitlines() if l.startswith(prefix)]
    if len(found) != 1:
        raise SystemExit(
            f"memory-account: FAIL: expected exactly one {prefix} line, found {len(found)}"
        )
    return fields(found[0])

FRAME = 4096

account = one("TOS.MEM.ACCOUNT ")
charge = one("TOS.RUN.PROCESS_CHARGE ")
reclaimed = one("TOS.RUN.PROCESS_RECLAIMED ")

admitted = int(account["admitted_frames"])
reserve = int(account["table_reserve_frames"])
pool = int(account["pool_frames"])
root = int(account["root_frames"])

def check(claim, left, right):
    if left != right:
        raise SystemExit(f"memory-account: FAIL: {claim}: {left} != {right}")
    print(f"  {claim}: {left}")

# The reserve left the pool exactly once.
check("admitted == reserve + pool", admitted, reserve + pool)

# And the root was endowed over what was left, without subtracting it twice.
check("root == pool after the reserve", root, pool)

# The creation's own account: every line, and the total it claims.
lines = ["data", "grant", "stack", "report", "arguments", "record"]
total = sum(int(charge[name]) for name in lines)
check("the charge's lines sum to its total", total, int(charge["total"]))
for name in lines:
    value = int(charge[name])
    if value % FRAME:
        raise SystemExit(
            f"memory-account: FAIL: {name}={value} is not a whole number of frames"
        )

# What it said it would cost is what came back.
check(
    "the charge is what reclamation returned",
    int(charge["total"]),
    int(reclaimed["frames"]) * FRAME,
)

# And the pool is back to what the root was endowed with.
check("the pool returned to the root's endowment", int(reclaimed["available"]), root)

print(f"MEMORY-ACCOUNT PASS: {admitted} admitted, {reserve} reserved, {root} funded")
PY
