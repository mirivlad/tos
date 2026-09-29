#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A long-lived process produces several report regions' worth of lines, and every
# one of them reaches the transport whole and in order (`RUNTIME_OBSERVABILITY_V1`
# §7).
#
# **What went wrong before.** The report region is a fixed part of a process's
# footprint (ADR-0076 §3). The runtime filled it front to back and, once full,
# dropped each later line so as not to overwrite one the nucleus had not read —
# correct locally, and fatal over a lifetime: after 64 KiB a process's whole
# remaining audit trail, its account and its completion were gone, silently. The
# nucleus now returns a region it has relayed completely to empty, which it may
# do because it drains only while that process is inside the nucleus, where no
# line can be half-published.
#
# **What is asserted, each exactly:**
#
#   4096 refused receives   one audit line per call, the fixed text, none lost,
#                           none split, none duplicated
#   the module's own count  `COMPLETED value=i64:4096`: the module counted what
#                           it did, and the log agrees line for line
#   after the last call     the account and the completion are still on the log,
#                           which is precisely what a full region used to drop
#   more than three regions the relayed bytes exceed three times the region the
#                           nucleus charged, so reclaim happened repeatedly
#   whole lines             every `TOS.` in the raw serial log starts its line,
#                           so no record was glued to a neighbour and filtered
#                           out of `events.log`
#
#   bash host-tools/qemu-test/report-reclaim.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-report-reclaim}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/report-reclaim"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-report-reclaim"
CALLS=4096

fail() { echo "report-reclaim: FAIL: $*" >&2; exit 1; }
cleanup() { rm -rf "$TARGET"; }
trap cleanup EXIT

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }

# The endowment of `module-operation.sh`: one endpoint, `send` only. The launcher
# constant is the only thing this build changes, and the production artifact is
# hashed around it.
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-module-operation \
    >/dev/null 2>&1) || fail "the nucleus does not build with test-module-operation"
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
    fail "the production nucleus changed while building the isolated test artifact"

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/capsule.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/capsule.bin" --manifest "$OUT/capsule.meta.json" >/dev/null

bash "$HERE/run.sh" \
    --out "$OUT/boot" \
    --capsule "$OUT/capsule.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.INTERFACE TOS.RUN.ACCOUNTING TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.REFUSED TOS.RUN.TRAP" \
    > /dev/null || fail "the long-lived boot did not complete"

python3 - "$OUT/boot/serial.log" "$OUT/boot/events.log" "$CALLS" <<'CHECK' ||
import re
import sys

serial = open(sys.argv[1], "rb").read().decode("utf-8", "replace")
events = [line.rstrip("\r\n") for line in open(sys.argv[2], encoding="utf-8", errors="replace")]
calls = int(sys.argv[3])

def fail(message):
    sys.exit("report-reclaim: FAIL: " + message)

# A record glued to the front of another is invisible to `events.log`, which keeps
# only lines that begin with `TOS.`; the raw log is where that would show.
glued = [m.start() for m in re.finditer(r"TOS\.", serial)
         if m.start() and serial[m.start() - 1] not in "\r\n"]
if glued:
    fail("%d TOS. record(s) do not start their line in the raw serial log" % len(glued))

line = "TOS.RUN.INTERFACE operation=endpoint_receive status=-1"
interface = [i for i, text in enumerate(events) if text.startswith("TOS.RUN.INTERFACE")]
if [events[i] for i in interface] != [line] * calls:
    fail("expected exactly %d intact '%s' lines, saw %d interface line(s)"
         % (calls, line, len(interface)))
if interface != list(range(interface[0], interface[0] + calls)):
    fail("the audit lines are not one uninterrupted run")

completed = [i for i, text in enumerate(events)
             if text == "TOS.RUN.COMPLETED value=i64:%d" % calls]
account = [i for i, text in enumerate(events) if text.startswith("TOS.RUN.ACCOUNTING ")]
if len(completed) != 1 or len(account) != 1:
    fail("the module's account and completion did not both reach the log once")
if not interface[-1] < account[0] < completed[0]:
    fail("the account and completion are not after the last audit line")

charge = [m for text in events
          for m in [re.match(r"^TOS\.RUN\.PROCESS_CHARGE .* report=(\d+) ", text)] if m]
if len(charge) != 1:
    fail("expected one process charge naming its report region")
region = int(charge[0].group(1))
relayed = sum(len(events[i]) + 1 for i in interface)
if relayed <= 3 * region:
    fail("relayed %d audit bytes, not more than three %d-byte regions" % (relayed, region))
print("  %d audit lines, %d bytes relayed through a %d-byte region (%.2f regions), "
      "then the account and the completion" % (calls, relayed, region, relayed / region))
CHECK
    fail "the transport did not carry every line of the long-lived process"

echo "report-reclaim: PASS (a process said more than its report region holds, and all of it arrived)"
