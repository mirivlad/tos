#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The two ways a region stops authorising an access (ADR-0075 §5a).
#
# Each in a process of its own, because each ends in a fault: a fault is the
# evidence here rather than a failure, and one in a process that was also doing
# something else would be a fault nobody could attribute. Both hold a name for
# one child memory authority — two names, one budget — and neither can reach
# anything the other made.
#
#   nx      operation 17 maps what it allocates writable and **not
#           executable**, and that pairing is the whole of it: memory a process
#           may write is memory a process may not run. This one writes an
#           instruction into a region and jumps to it. What must happen is a
#           page fault at CPL 3 with the instruction-fetch bit set, at the
#           address it jumped to — and not a successful return
#
#   stale   a mapping is derived authority and does not outlive it. Releasing
#           the handle to a region and leaving its window mapped would be the
#           capability model bypassed in one line, so this one releases and then
#           reads. What must happen is a not-present page fault at CPL 3, at the
#           base the nucleus gave it
#
# **Both are the second and third processes, and the first is the ordinary boot
# module.** Only the first process's ending decides the boot's result, and both
# of these end in a fault deliberately — a boot that failed because its evidence
# worked would be a gate that could only ever be red. What the first process
# proves beside them is the thing that matters most here: a machine two of whose
# processes died touching memory they no longer held still finished the work it
# was booted for.
#
# Then the account closes: all three processes are reclaimed, every frame goes
# back to the root's count and every page table to the reserve's baseline. A
# region whose holder faulted is a region whose backing still has to come
# back.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:?usage: region-faults.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "region-faults: FAIL: $*" >&2; exit 1; }

# Built into their own directories, so the ordinary artifacts at the shared
# paths are not replaced by feature builds a later gate would then boot.
BUILD="$ROOT/target/evidence/region-faults"
(cd "$ROOT" && cargo build --release -p tos-nucleus \
    --target x86_64-unknown-none --features test-region-faults --target-dir "$BUILD") \
    > "$OUT/nucleus.log" 2>&1 || { cat "$OUT/nucleus.log" >&2; fail "the nucleus did not build"; }
(cd "$ROOT" && cargo build --release -p tos-runtime-image \
    --target x86_64-unknown-none --features test-region-faults --target-dir "$BUILD") \
    > "$OUT/image.log" 2>&1 || { cat "$OUT/image.log" >&2; fail "the image did not build"; }

bash "$HERE/run.sh" --nucleus "$BUILD/x86_64-unknown-none/release/tos-nucleus" \
    --runtime-image "$BUILD/x86_64-unknown-none/release/tos-runtime-image" \
    --out "$OUT/boot" --expect 33 > "$OUT/boot.log" 2>&1 || {
    cat "$OUT/boot.log" >&2
    fail "the boot did not pass"
}

python3 - "$OUT/boot/serial.log" <<'PY'
import re
import sys

serial = open(sys.argv[1], "rb").read().decode("utf-8", "replace").replace("\r", "")
lines = serial.splitlines()


def one(prefix):
    found = [l for l in lines if l.startswith(prefix)]
    if len(found) != 1:
        raise SystemExit(
            f"region-faults: FAIL: expected one {prefix.strip()} line, found {len(found)}"
        )
    return found[0]


def fields(line):
    return {name: value for name, value in re.findall(r"(\w+)=(\S+)", line)}


def number(text):
    return int(text, 0)


# Page-fault error-code bits, as the architecture defines them.
PRESENT, WRITE, USER, FETCH = 1, 1 << 1, 1 << 2, 1 << 4

nx = fields(one("TOS.RUN.REGION.NX "))
stale = fields(one("TOS.RUN.REGION.STALE "))
if nx["wrote"] != "1":
    raise SystemExit("region-faults: FAIL: the region was not writable before the jump")
if stale["wrote"] != "1" or stale["released"] != "0":
    raise SystemExit(f"region-faults: FAIL: {stale}")
print("  each process made a region, wrote it, and said what it was about to do")

for unreached in ("TOS.RUN.REGION.NX.UNREACHED", "TOS.RUN.REGION.STALE.UNREACHED"):
    if any(l.startswith(unreached) for l in lines):
        raise SystemExit(f"region-faults: FAIL: {unreached} was reached")

faults = [fields(l) for l in lines if l.startswith("TOS.RUN.PROCESS_FAULT ")]
if len(faults) != 2:
    raise SystemExit(f"region-faults: FAIL: expected two faults, found {len(faults)}")

# **Told apart by the error code, not by the address.** Both processes were
# given slot zero of their own address space, so both bases are the same number
# in two different trees; what distinguishes the two faults is what the
# processor says it was doing.
def only(what, admit):
    found = [fault for fault in faults if admit(number(fault["error"]))]
    if len(found) != 1:
        raise SystemExit(
            f"region-faults: FAIL: expected one {what} fault, found {len(found)}"
        )
    fault = found[0]
    if fault["vector"] != "14" or fault["cpl"] != "3":
        raise SystemExit(f"region-faults: FAIL: the {what} fault was {fault}")
    return fault


# The instruction fetch. The bits say what the processor was doing and to whom:
# present, user, and an instruction fetch — a leaf that is mapped and readable
# refusing to be executed, rather than an address that is not there.
fetch = only("instruction fetch", lambda e: e & FETCH)
if number(fetch["cr2"]) != number(nx["base"]):
    raise SystemExit(
        f"region-faults: FAIL: the fetch faulted at {fetch['cr2']}, not at {nx['base']}"
    )
if not (number(fetch["error"]) & USER) or not (number(fetch["error"]) & PRESENT):
    raise SystemExit(
        f"region-faults: FAIL: the fetch fault's error code was {fetch['error']}: "
        "expected a present, user-mode instruction fetch"
    )
print(f"  a region is data: fetching from {nx['base']} faulted at CPL 3, error {fetch['error']}")

# And the read after the release. Not present, because the window went with the
# handle: a mapping is derived authority and does not outlive it.
gone = only("not-present", lambda e: not e & PRESENT)
if number(gone["cr2"]) != number(stale["base"]):
    raise SystemExit(
        f"region-faults: FAIL: the read faulted at {gone['cr2']}, not at {stale['base']}"
    )
if not (number(gone["error"]) & USER) or number(gone["error"]) & WRITE:
    raise SystemExit(
        f"region-faults: FAIL: the stale fault's error code was {gone['error']}: "
        "expected a not-present, user-mode read"
    )
print(f"  a released region is unreachable: reading {stale['base']} faulted at CPL 3")

# And both are reclaimed. A region whose holder faulted is a region whose
# backing still has to come back, and the two accounts say so.
account = fields(one("TOS.MEM.ACCOUNT "))
reserve = fields(one("TOS.MEM.RESERVE "))
reclaimed = [fields(l) for l in lines if l.startswith("TOS.RUN.PROCESS_RECLAIMED ")]
if len(reclaimed) != 3:
    raise SystemExit(
        f"region-faults: FAIL: expected three reclamations, found {len(reclaimed)}"
    )
last = reclaimed[-1]
if int(last["available"]) != int(account["root_frames"]):
    raise SystemExit(
        f"region-faults: FAIL: the pool came back to {last['available']}, "
        f"not the root's {account['root_frames']}"
    )
baseline = int(reserve["runtime_baseline_frames"])
if int(last["tables_free"]) != baseline:
    raise SystemExit(
        f"region-faults: FAIL: the reserve came back to {last['tables_free']}, "
        f"not its baseline {baseline}"
    )
print(f"  every frame back to {last['available']}; every table back to {baseline}")

if "TOS.NUCLEUS.INVARIANT" in serial:
    raise SystemExit("region-faults: FAIL: an invariant was reported")

print("REGION-FAULTS PASS: a region is data, and a released one is nothing")
PY
