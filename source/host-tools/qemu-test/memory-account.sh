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
#   admitted_frames == nucleus_space + table_reserve_frames + pool_frames
#       everything that left the pool before the tree existed, and nothing else.
#       The nucleus's own address space is its own line: it is built before the
#       reserve, from the pool, because until it is active the machine runs on
#       the firmware's map and some of what the memory map reports usable is
#       still mapped read-only there — a reserve taken earlier would fault
#       writing its own free list
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
#   reserve == backing + processes × (identity + windows + region mappings)
#       and **no standalone identity term**. While every tree came from the
#       reserve, one was right: the nucleus's own address space. It stopped
#       being right when that space began to be built from the pool before the
#       reserve exists — the frames are gone by then and are reported as their
#       own line — and the second reservation cost 25 frames the region lanes
#       needed. This is the check that would catch it if boot ordering moves
#       again
#
#   the topologies this system actually runs are affordable, measured
#       **Not** `root >= MAX_PROCESSES × the ordinary charge.** That assertion
#       stood while it was true and stopped being an invariant when the Project
#       Architect fixed what `MAX_PROCESSES` means (ADR-0069 §7, 2026-09-03):
#       it is the bounded number of process **slots**, not a reservation
#       guaranteeing that four simultaneous 54 MiB processes can always be
#       funded. Process slots and memory authority are independent finite
#       resources, and a creation that finds a free slot and an authority that
#       cannot pay is answered `E_LIMIT` — which is correct behaviour, not a
#       failure. Keeping the old assertion would have made every future code
#       page an architecture STOP.
#
#       What replaces it is measured rather than assumed: this prints the
#       ordinary charge and how many of them the root can fund, and asserts the
#       two topologies the system is actually built to run — a supervisor with
#       one child, and a supervisor with a transient build worker beside it.
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
reserve_lines = one("TOS.MEM.RESERVE ")
charge = one("TOS.RUN.PROCESS_CHARGE ")
reclaimed = one("TOS.RUN.PROCESS_RECLAIMED ")

admitted = int(account["admitted_frames"])
own_space = int(account["nucleus_space_actual_frames"])
reserve = int(account["table_reserve_frames"])
pool = int(account["pool_frames"])
root = int(account["root_frames"])

def check(claim, left, right):
    if left != right:
        raise SystemExit(f"memory-account: FAIL: {claim}: {left} != {right}")
    print(f"  {claim}: {left}")

# The reserve left the pool exactly once.
check("admitted == own space + reserve + pool", admitted, own_space + reserve + pool)

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

# The reserve is what its parts say it is, and no part is counted twice.
identity = int(reserve_lines["process_identity_frames"])
windows = int(reserve_lines["process_windows_frames"])
mappings = int(reserve_lines["process_region_mapping_frames"])
backing = int(reserve_lines["region_backing_frames"])
processes = int(reserve_lines["processes"])
check("the reserve is its parts", reserve, backing + processes * (identity + windows + mappings))
check(
    "and its total is the one the boot took",
    reserve,
    int(reserve_lines["total_frames"]),
)
# The nucleus's own space is smaller than the bound one process's identity
# mappings are reserved for — it maps the same machine and no user window — so a
# reserve that had grown by roughly that amount is the signature of the term
# being counted twice.
if own_space > identity:
    raise SystemExit(
        f"memory-account: FAIL: the nucleus's own space is {own_space} frames, "
        f"more than the {identity} one process's identity mappings are bounded by"
    )

# And the topologies this system runs are affordable.
#
# The number that matters is how many ordinary processes one root can fund, and
# it is *reported* rather than assumed to be the table's size. `MAX_PROCESSES`
# bounds the slots; the authority bounds the money; they are independent, and a
# creation that finds a free slot and an authority that cannot pay is answered
# `E_LIMIT` (ADR-0069 §7).
ordinary = int(charge["total"]) // FRAME
affordable = root // ordinary
print(f"  one ordinary process costs {ordinary} frames; the root funds {affordable}")

# Two live processes is the floor, and it is the one this system cannot fall
# below without ceasing to work at all: a supervisor and the thing it
# supervises. Below this there is no topology left.
SUPERVISED = 2
if affordable < SUPERVISED:
    raise SystemExit(
        f"memory-account: FAIL: a supervisor and one child need "
        f"{SUPERVISED * ordinary} frames and the root holds {root}"
    )
print(f"  supervisor + target: {SUPERVISED * ordinary} frames, margin {root - SUPERVISED * ordinary}")

# And the build topology ADR-0074 describes: a resident supervisor, a transient
# build worker, and the bundle backing between them. The worker's arena is the
# same ordinary grant; what is left over after both is what a bundle may occupy,
# and it is reported so that a Capsule-v1 artifact's size can be held against a
# measured number rather than an assumed one.
bundle_frames = root - SUPERVISED * ordinary
print(
    f"  build worker topology: {bundle_frames} frames "
    f"({bundle_frames * FRAME / (1024 * 1024):.2f} MiB) left for bundle backing"
)
if bundle_frames <= 0:
    raise SystemExit(
        "memory-account: FAIL: a supervisor and a build worker leave nothing for a bundle"
    )

print(f"MEMORY-ACCOUNT PASS: {admitted} admitted, {reserve} reserved, {root} funded")
PY
