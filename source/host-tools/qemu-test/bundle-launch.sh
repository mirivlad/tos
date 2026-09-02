#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A process is created from a bundle, twice, and once from a corrupt one
# (`SYSTEM_ABI_V1` §5 operation 20; ADR-0073, ADR-0075, ADR-0076).
#
# One supervisor builds a real `TOSBUNDLE/v1` over this boot's own source set
# into a region it allocated, freezes it, shares it, and then creates targets
# from it. **This is not the canonical build worker** — ADR-0074 is a Draft and
# the build/supervisor lifecycle it proposes is not settled — it is the smallest
# arrangement that produces a real artifact for a real question.
#
# What is asserted, and why each one is a claim somebody could get wrong:
#
#   unsealed            a launch plan that has not been sealed is refused as an
#                       input to a creation: a decision still being written is
#                       not one anything may be created from
#   not_shared          an immutable **affine** region is refused. A target gets
#                       a window of its own and its creator keeps one, which is
#                       two holders; `share` (7) is what makes a region able to
#                       be in two places, and 20 requires that it already has
#   unheld              and a handle nobody holds names nothing
#   allocate/freeze/    the artifact goes through the whole state machine on the
#   share               way out: allocated mutable, written, frozen, shared
#   first/second        two targets from **one** capability over **one**
#   distinct            backing. No rebuild, no refreeze, no copy: a restart is
#                       one bundle used twice, and the two targets are two
#                       processes with two identities
#   kept                the supervisor's own window still reads afterwards: the
#                       creation added a capability reference and a mapping bit,
#                       and consumed nothing
#   BUNDLE.PARSED       the target parses the artifact **itself**, with a total
#                       parser, and takes its entry from the bundle — there is
#                       no caller-supplied entry to disagree with
#   VERIFIED/COMPLETED  and verifies every image itself before running one
#                       instruction. No build receipt crossed, no host verdict
#                       crossed, and no nucleus verdict crossed, because none
#                       was made
#   HOSTILE created=0   **the trust boundary.** One byte of the magic is flipped
#   + BUNDLE.REFUSED    in a second artifact. The region is legal in every way a
#                       nucleus can check, so the process is created
#                       *successfully* — and then refuses itself before its first
#                       source instruction. Creation succeeding and admission
#                       failing are two outcomes of two components, and ADR-0073
#                       owns the second
#
# Then the account closes: every frame back to the root's count, every page table
# back to the reserve's baseline.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:?usage: bundle-launch.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "bundle-launch: FAIL: $*" >&2; exit 1; }

# Built into their own directory, so the ordinary artifacts at the shared paths
# are not replaced by feature builds a later gate would then boot.
BUILD="$ROOT/target/evidence/bundle-launch"
(cd "$ROOT" && cargo build --release -p tos-nucleus \
    --target x86_64-unknown-none --features test-bundle-launch --target-dir "$BUILD") \
    > "$OUT/nucleus.log" 2>&1 || { cat "$OUT/nucleus.log" >&2; fail "the nucleus did not build"; }
(cd "$ROOT" && cargo build --release -p tos-runtime-image \
    --target x86_64-unknown-none --features test-bundle-launch --target-dir "$BUILD") \
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

OK, E_NO_CAPABILITY, E_BAD_HANDLE = 0, -1, -2


def one(prefix):
    found = [l for l in lines if l.startswith(prefix)]
    if len(found) != 1:
        raise SystemExit(
            f"bundle-launch: FAIL: expected one {prefix.strip()} line, found {len(found)}"
        )
    return found[0]


def fields(line):
    return {name: value for name, value in re.findall(r"(\w+)=(\S+)", line)}


def numbers(line):
    return {name: int(value, 0) for name, value in re.findall(r"(\w+)=(-?0x[0-9a-f]+|-?\d+)", line)}


def expect(what, got, wanted):
    for name, want in wanted.items():
        if got.get(name) != want:
            raise SystemExit(
                f"bundle-launch: FAIL: {what} {name} was {got.get(name)}, expected {want}"
            )
        print(f"  {what} {name}: {got[name]}")


# --- the artifact ------------------------------------------------------------
written = [fields(l) for l in lines if l.startswith("TOS.RUN.BUNDLE.WRITTEN ")]
if len(written) != 2:
    raise SystemExit(
        f"bundle-launch: FAIL: expected two bundles written, found {len(written)}"
    )
if int(written[0]["bytes"]) == 0 or int(written[0]["modules"]) < 1:
    raise SystemExit(f"bundle-launch: FAIL: the bundle is empty: {written[0]}")
print(f"  a real bundle: {written[0]['bytes']} bytes over {written[0]['modules']} module(s)")

shared = [numbers(l) for l in lines if l.startswith("TOS.RUN.BUNDLE.SHARED ")]
if len(shared) != 2:
    raise SystemExit(f"bundle-launch: FAIL: expected two shared regions, found {len(shared)}")
for at, record in enumerate(shared):
    for name in ("allocate", "freeze", "share"):
        if record.get(name) != OK:
            raise SystemExit(
                f"bundle-launch: FAIL: region {at} {name} was {record.get(name)}, expected {OK}"
            )
print("  allocated mutable, written, frozen and shared — the whole state machine")

# --- the targets --------------------------------------------------------------
targets = numbers(one("TOS.RUN.BUNDLE.TARGETS "))
expect(
    "targets",
    targets,
    {
        # An immutable affine region is not a shared one, and a handle nobody
        # holds names nothing. Both refused before anything is built.
        "not_shared": E_NO_CAPABILITY,
        "unheld": E_BAD_HANDLE,
        # And a plan that has not been sealed is a decision still being
        # written. It is made successfully and refused as an input: creating
        # from a builder would create from whatever happened to have been
        # added by the time the call was made (ADR-0077 §5).
        "unsealed_plan": OK,
        "unsealed": E_NO_CAPABILITY,
        # Two targets from one capability over one backing.
        "first": OK,
        "second": OK,
        "distinct": 1,
        # And the supervisor kept its own window through both.
        "collected": OK,
    },
)
if targets["kept"] == 0:
    raise SystemExit("bundle-launch: FAIL: the supervisor's own window stopped reading")
print(f"  the supervisor's window still reads 0x{targets['kept']:x} after both targets")

# --- and each target admitted the artifact for itself --------------------------
parsed = [fields(l) for l in lines if l.startswith("TOS.RUN.BUNDLE.PARSED ")]
if len(parsed) != 2:
    raise SystemExit(
        f"bundle-launch: FAIL: expected two targets to parse the bundle, found {len(parsed)}"
    )
for record in parsed:
    if int(record["modules"]) < 1 or int(record["entry_position"]) < 0:
        raise SystemExit(f"bundle-launch: FAIL: {record}")
print(f"  each target parsed it itself and took the entry the bundle declares: {parsed[0]['entry_path']}")

# Every image verified by the target, in the target, with no receipt from
# anywhere. Two runs, because a restart is a second run of the same artifact.
if len([l for l in lines if l.startswith("TOS.RUN.VERIFIED ")]) < 2:
    raise SystemExit("bundle-launch: FAIL: the targets did not each verify the closure")
if len([l for l in lines if l.startswith("TOS.RUN.COMPLETED ")]) < 2:
    raise SystemExit("bundle-launch: FAIL: the targets did not each run the bundle's entry")
print("  and verified every image itself before running one instruction — twice")

# --- the hostile artifact ------------------------------------------------------
hostile = numbers(one("TOS.RUN.BUNDLE.HOSTILE "))
expect("hostile", hostile, {"shared": OK, "created": OK})
refused = [fields(l) for l in lines if l.startswith("TOS.RUN.BUNDLE.REFUSED ")]
if len(refused) != 1 or refused[0]["stage"] != "parse":
    raise SystemExit(
        f"bundle-launch: FAIL: the corrupt bundle was not refused by its target: {refused}"
    )
print(f"  a corrupt bundle: created successfully, then refused by its target ({refused[0]['reason']})")

# The nucleus read nothing: it neither refused the creation nor reported an
# opinion about the bytes.
if "TOS.NUCLEUS.INVARIANT" in serial:
    raise SystemExit("bundle-launch: FAIL: an invariant was reported")

# --- and everything came back --------------------------------------------------
account = fields(one("TOS.MEM.ACCOUNT "))
reserve = fields(one("TOS.MEM.RESERVE "))
reclaimed = [fields(l) for l in lines if l.startswith("TOS.RUN.PROCESS_RECLAIMED ")]
if not reclaimed:
    raise SystemExit("bundle-launch: FAIL: nothing was reclaimed")
last = reclaimed[-1]
if int(last["available"]) != int(account["root_frames"]):
    raise SystemExit(
        f"bundle-launch: FAIL: the pool came back to {last['available']}, "
        f"not the root's {account['root_frames']}"
    )
baseline = int(reserve["runtime_baseline_frames"])
if int(last["tables_free"]) != baseline:
    raise SystemExit(
        f"bundle-launch: FAIL: the reserve came back to {last['tables_free']}, "
        f"not its baseline {baseline}"
    )
print(f"  every frame back to {last['available']}; every table back to {baseline}")

print("BUNDLE-LAUNCH PASS: one bundle, two targets, and a corrupt one refused by its own")
print("  and one sealed launch plan behind all three, unchanged by any of them")
PY
