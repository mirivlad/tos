#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A capsule carrying more than one module runs as a set, on the real boot path.
#
# Stage 3 needs to launch services, and a service is a separate module. This
# boots a capsule whose canonical boot text imports a second module of the same
# capsule and returns a value neither module computes alone, so the answer is
# evidence that the import resolved from the capsule and that the call crossed
# the boundary — not that two files were present.
#
# The ordinary artifacts, firmware, machine profile and event checks are the
# ones run.sh already uses. Only the capsule differs.
#
#   bash host-tools/qemu-test/module-set.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-target/qemu-module-set}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
FIXTURE="$ROOT/tests/vectors/module-set"
# 21 doubled by the imported module. No module of this capsule computes it alone.
EXPECTED_VALUE="i32:42"

fail() { echo "module-set: FAIL: $*" >&2; exit 1; }

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

# The capsule is detached: this fixture is test material, not committed system
# source, so it carries a whole-tree digest rather than a Git object identity.
printf '/system/boot/init.tos\t%s/init.tos\n/system/lib/arith.tos\t%s/arith.tos\n' \
    "$FIXTURE" "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/module-set.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/module-set.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" --out "$OUT" --capsule "$OUT/module-set.bin" --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.CAPSULE.OK TOS.RUN.BEGIN TOS.RUN.VERIFIED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.PANIC TOS.EXCEPTION TOS.RUN.REFUSED TOS.RUN.TRAP TOS.RUN.UNSTARTABLE"

LOG="$OUT/serial.txt"
tr -d '\r' < "$OUT/serial.log" > "$LOG"

field() { grep -m1 "^$1 " "$LOG" | tr ' ' '\n' | sed -n "s/^$2=//p" | head -1; }

# --- the set really was a set --------------------------------------------
MODULES=$(field TOS.RUN.BEGIN modules)
[ "${MODULES:-0}" -ge 2 ] || fail "the run carried $MODULES module(s), expected at least 2"

# --- every stage ran, in the reference order ------------------------------
ORDER=$(grep -o 'TOS\.RUN\.STAGE name=[a-z]*' "$LOG" | sed 's/.*name=//' | tr '\n' ' ')
[ "$ORDER" = "read parse check resolve lower verify execute " ] ||
    fail "stages ran as '$ORDER'"

# --- the answer could only come from across the boundary ------------------
VALUE=$(field TOS.RUN.COMPLETED value)
[ "$VALUE" = "$EXPECTED_VALUE" ] ||
    fail "the boot module returned '$VALUE', expected '$EXPECTED_VALUE'"

# --- and the entry is the module the capsule declares canonical -----------
MODULE=$(field TOS.RUN.VERIFIED module)
[ "$MODULE" = "system.boot.init" ] || fail "verified module is '$MODULE'"

echo "MODULE-SET PASS: $MODULES modules booted as one set; $MODULE returned $VALUE"
