#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0074's T1 build topology, performed (§D of the Stage-3 closure decisions).
#
#   supervisor resident
#     -> create transient build worker
#     -> worker builds + freezes + shares bundle
#     -> bundle handed to supervisor
#     -> worker exits
#     -> only then the target is created
#
# **Every arrow is a refusal until the one before it happened**, and the order
# is what this gate is for. A topology whose steps merely all occurred would be
# a topology that had not been shown to be one.
#
# The two roles are told apart by what they were endowed with, not by a flag.
# The supervisor holds authority over a process with `create`, `terminate` and
# `wait_child`, the root's remainder to spend, and one endpoint it **receives**
# on. The worker holds an authority to spend and the right to **send**, and
# nothing else: it cannot create a process, cannot terminate one, and never
# learns what the artifact it built is for.
#
# The artifact is real. The worker builds a `TOSBUNDLE/v1` over **this capsule's
# own canonical source**, read back out of its own launch record — so the chain
# under test is source -> bundle -> target -> execution, with the capsule's
# identity and source binding carried through it, rather than two constants
# compared in a host test.
#
# What the account then shows is the point of §D: with a supervisor and a worker
# both resident, how much of the reference machine is left for bundle backing.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:?usage: build-topology.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "build-topology: FAIL: $*" >&2; exit 1; }

BUILD="$ROOT/target/evidence/build-topology"
(cd "$ROOT" && cargo build --release -p tos-nucleus \
    --target x86_64-unknown-none --features test-build-topology --target-dir "$BUILD") \
    > "$OUT/nucleus.log" 2>&1 || { cat "$OUT/nucleus.log" >&2; fail "the nucleus did not build"; }
(cd "$ROOT" && cargo build --release -p tos-runtime-image \
    --target x86_64-unknown-none --features test-build-topology --target-dir "$BUILD") \
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

OK = 0
FRAME = 4096
MIB = 1024 * 1024


def one(prefix):
    found = [line for line in lines if line.startswith(prefix)]
    if len(found) != 1:
        raise SystemExit(
            f"build-topology: FAIL: expected one {prefix.strip()} line, found {len(found)}"
        )
    return found[0]


def fields(line):
    return {name: value for name, value in re.findall(r"(\w+)=(\S+)", line)}


def numbers(line):
    return {
        name: int(value, 0)
        for name, value in re.findall(r"(\w+)=(-?0x[0-9a-f]+|-?\d+)", line)
    }


def at(prefix):
    for index, line in enumerate(lines):
        if line.startswith(prefix):
            return index
    raise SystemExit(f"build-topology: FAIL: no {prefix.strip()} line")


# --- the worker was created, funded at its own role's grant --------------------
worker = numbers(one("TOS.RUN.TOPOLOGY.WORKER "))
if worker.get("status") != OK:
    raise SystemExit(f"build-topology: FAIL: the worker was not created: {worker}")
if worker.get("grant") != 96 * MIB:
    raise SystemExit(
        f"build-topology: FAIL: the worker was funded at {worker.get('grant')}, "
        f"not the role's policy figure"
    )
print(f"  a transient worker, funded at {worker['grant'] / MIB:.0f} MiB — its role's own grant")

# --- it built a real artifact over this capsule's own source -------------------
written = fields(one("TOS.RUN.BUNDLE.WRITTEN "))
if int(written["bytes"]) == 0 or int(written["modules"]) < 1:
    raise SystemExit(f"build-topology: FAIL: the bundle is empty: {written}")
shared = numbers(one("TOS.RUN.BUNDLE.SHARED "))
for stage in ("allocate", "freeze", "share"):
    if shared.get(stage) != OK:
        raise SystemExit(f"build-topology: FAIL: {stage} was {shared.get(stage)}")
print(
    f"  a real TOSBUNDLE/v1 of {written['bytes']} bytes over {written['modules']} module(s),"
    " allocated, written, frozen and shared"
)

# --- and handed it over ---------------------------------------------------------
handed = numbers(one("TOS.RUN.TOPOLOGY.HANDED "))
if handed.get("status") != OK:
    raise SystemExit(f"build-topology: FAIL: the handoff was refused: {handed}")
received = numbers(one("TOS.RUN.TOPOLOGY.RECEIVED "))
if received.get("status") != OK or received.get("bundle", 0) == 0:
    raise SystemExit(f"build-topology: FAIL: the supervisor received nothing: {received}")
if received.get("base", 0) == 0:
    raise SystemExit("build-topology: FAIL: the received region has no window")
print(
    f"  handed over and received: the supervisor's own capability 0x{received['bundle']:x}"
    f" and a window the nucleus chose at 0x{received['base']:x}"
)

# --- the worker ended, and was **collected**, before the target was created -----
#
# This is the ordering claim, and it is read off the log rather than assumed
# from the code: a topology where the worker merely happened to finish first
# would be a different topology from one where the supervisor waited for it.
if received.get("collected") != OK:
    raise SystemExit("build-topology: FAIL: the worker's ending was never collected")
if received.get("ended") != worker.get("instance"):
    raise SystemExit(
        f"build-topology: FAIL: the ending collected was instance "
        f"{received.get('ended')}, not the worker's {worker.get('instance')}"
    )
worker_gone = at("TOS.RUN.PROCESS_RECLAIMED process=1 ")
target_made = at("TOS.RUN.TOPOLOGY.TARGET ")
if not worker_gone < target_made:
    raise SystemExit(
        "build-topology: FAIL: the target was created before the worker's frames came back"
    )
print("  the worker ended and was collected — by wait_child, not by waiting — before anything else")

# --- and only then the target -----------------------------------------------------
target = numbers(one("TOS.RUN.TOPOLOGY.TARGET "))
if target.get("status") != OK:
    raise SystemExit(f"build-topology: FAIL: the target was not created: {target}")
if target.get("kept", 0) == 0:
    raise SystemExit("build-topology: FAIL: the supervisor's own window stopped reading")
if len([line for line in lines if line.startswith("TOS.RUN.BUNDLE.PARSED ")]) < 1:
    raise SystemExit("build-topology: FAIL: the target did not parse the bundle itself")
if len([line for line in lines if line.startswith("TOS.RUN.VERIFIED ")]) < 1:
    raise SystemExit("build-topology: FAIL: the target did not verify the closure itself")
print("  and the target parsed and verified the artifact itself, with no receipt from anywhere")

# --- the capacity claim, measured -------------------------------------------------
#
# §D asks for the T1 obligation against Capsule v1 rather than against the size
# of this boot's own evidence bundle. The measurement is what the machine has
# left with **both** roles resident; the number it is held against is the
# largest Capsule-v1 bundle `STAGE3_BUILD_WORKSPACE.md` records.
account = fields(one("TOS.MEM.ACCOUNT "))
charges = [numbers(line) for line in lines if line.startswith("TOS.RUN.PROCESS_CHARGE ")]
root = int(account["root_frames"])
supervisor_charge = charges[0]["total"] // FRAME
worker_charge = max(charge["total"] for charge in charges) // FRAME
resident = supervisor_charge + worker_charge
headroom = (root - resident) * FRAME
# `STAGE3_BUILD_WORKSPACE.md` measures the three configurations a Capsule v1 can
# hold. The largest **bundle** among them is 50.52 MiB, at 255 modules of
# 128 KiB — not the 1147 bytes this boot's own evidence artifact happens to be,
# which is what §D said not to use as the capacity argument.
LARGEST_CAPSULE_V1_BUNDLE = int(50.52 * MIB)
print(
    f"  supervisor {supervisor_charge} + worker {worker_charge} = {resident} frames of "
    f"{root}; {headroom / MIB:.2f} MiB left for bundle backing"
)
if headroom <= LARGEST_CAPSULE_V1_BUNDLE:
    raise SystemExit(
        f"build-topology: FAIL: {headroom} bytes of headroom does not hold the largest "
        f"recorded Capsule-v1 bundle of {LARGEST_CAPSULE_V1_BUNDLE}"
    )
print(
    f"  which holds the largest bundle any Capsule v1 configuration produces "
    f"({LARGEST_CAPSULE_V1_BUNDLE / MIB:.2f} MiB) with "
    f"{(headroom - LARGEST_CAPSULE_V1_BUNDLE) / MIB:.2f} MiB to spare"
)

# --- nothing was weakened to get here ---------------------------------------------
if "TOS.NUCLEUS.INVARIANT" in serial:
    raise SystemExit("build-topology: FAIL: an invariant was reported")
reserve = fields(one("TOS.MEM.RESERVE "))
reclaimed = [fields(line) for line in lines if line.startswith("TOS.RUN.PROCESS_RECLAIMED ")]
last = reclaimed[-1]
if int(last["available"]) != root:
    raise SystemExit(
        f"build-topology: FAIL: the pool came back to {last['available']}, not the root's {root}"
    )
if int(last["tables_free"]) != int(reserve["runtime_baseline_frames"]):
    raise SystemExit("build-topology: FAIL: the page-table reserve did not come back")
if int(last["plans_live"]) != 0:
    raise SystemExit("build-topology: FAIL: a launch plan outlived the process that made it")
print(f"  every frame back to {last['available']}; every table and every plan back")

print("BUILD-TOPOLOGY PASS: T1, performed rather than described")
PY
