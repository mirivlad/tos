#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Stage 2 conformance on the real boot path.
#
# Boots the ordinary artifacts under the ADR-0040 reference platform and checks
# what the reference runtime actually did: that every stage of the reference
# path ran in order, that the independent verifier issued a receipt for the
# module the engine then ran, that the canonical boot module returned the value
# it computes, and that the run stayed inside both the resources the module
# declared and the region the nucleus granted.
#
# Event presence alone is not the check. A boot that emitted the right
# identifiers with the wrong answer, an unverified module or a stack an inch
# from its limit would pass a presence test and would not be Stage 2 working.
#
#   bash host-tools/qemu-test/stage2-runtime.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-target/qemu-stage2-runtime}"

# The value `system/boot/init.tos` computes: 1*2 + 2*3 + ... + 8*9.
EXPECTED_VALUE="i32:240"
# The module the capsule's canonical boot text declares.
EXPECTED_MODULE="system.boot.init"
# Stack headroom the run must keep. A frontend recursing over nested source
# does not fail politely when it runs out: it writes past the region.
MIN_STACK_HEADROOM_FRACTION=4

REQUIRE="TOS.NUCLEUS.ENTRY TOS.RUN.BEGIN"
for stage in read parse check resolve lower verify execute; do
    REQUIRE="$REQUIRE TOS.RUN.STAGE"
done
REQUIRE="$REQUIRE TOS.RUN.VERIFIED TOS.RUN.ACCOUNTING TOS.RUN.COMPLETED TOS.RUN.MEMORY TOS.RUN.STACK TOS.HALT"

bash "$HERE/run.sh" --out "$OUT" --expect 33 \
    --require "$REQUIRE" \
    --forbid "TOS.PANIC TOS.EXCEPTION TOS.RUN.REFUSED TOS.RUN.TRAP TOS.RUN.UNSTARTABLE"

# The serial line terminator is CRLF (BOOT_ABI_V1 section 7). Every field read
# below would otherwise end in a carriage return and compare unequal to the
# value it plainly is, so the log is normalized once rather than in each check.
LOG="$OUT/serial.txt"
tr -d '\r' < "$OUT/serial.log" > "$LOG"

fail() {
    echo "stage2-runtime: FAIL: $*" >&2
    exit 1
}

field() {
    # field <event> <key> — the value of `key=` on the named event line.
    grep -m1 "^$1 " "$LOG" | tr ' ' '\n' | sed -n "s/^$2=//p" | head -1
}

# --- the stages ran in the order the reference path fixes -------------------
ORDER=$(grep -o 'TOS\.RUN\.STAGE name=[a-z]*' "$LOG" | sed 's/.*name=//' | tr '\n' ' ')
[ "$ORDER" = "read parse check resolve lower verify execute " ] ||
    fail "stages ran as '$ORDER'"

# --- the engine ran the module the verifier issued a receipt for ------------
MODULE=$(field TOS.RUN.VERIFIED module)
DIGEST=$(field TOS.RUN.VERIFIED digest)
VERIFIER=$(field TOS.RUN.VERIFIED verifier)
[ "$MODULE" = "$EXPECTED_MODULE" ] || fail "verified module is '$MODULE'"
case "$DIGEST" in
    sha256:????????????????????????????????????????????????????????????????) ;;
    *) fail "verified digest is not a sha256 digest: '$DIGEST'" ;;
esac
[ -n "$VERIFIER" ] || fail "no verifier identity in TOS.RUN.VERIFIED"

# --- the program produced its answer ----------------------------------------
VALUE=$(field TOS.RUN.COMPLETED value)
[ "$VALUE" = "$EXPECTED_VALUE" ] ||
    fail "the boot module returned '$VALUE', expected '$EXPECTED_VALUE'"

# --- it stayed inside every resource it declared ----------------------------
ACCOUNTING=$(grep -m1 '^TOS\.RUN\.ACCOUNTING ' "$LOG") ||
    fail "no accounting event"
for pair in $(printf '%s\n' "$ACCOUNTING" | sed 's/^TOS\.RUN\.ACCOUNTING //'); do
    key=${pair%%=*}
    used=${pair#*=}; limit=${used#*/}; used=${used%%/*}
    [ -n "$limit" ] || fail "accounting field '$pair' has no limit"
    if [ "$used" -gt "$limit" ]; then
        fail "the run exceeded its declared $key: $used of $limit"
    fi
done
# A run that consumed nothing did not happen.
FUEL=$(printf '%s\n' "$ACCOUNTING" | tr ' ' '\n' | sed -n 's/^fuel=//p')
[ "${FUEL%%/*}" -gt 0 ] || fail "the run consumed no fuel"

# --- it stayed inside the region the nucleus granted ------------------------
GRANTED=$(field TOS.RUN.MEMORY granted)
PEAK=$(field TOS.RUN.MEMORY peak)
[ "$PEAK" -gt 0 ] || fail "the run used no memory"
[ "$PEAK" -lt "$GRANTED" ] || fail "peak extent $PEAK reached the $GRANTED grant"

# --- and inside the stack it was given --------------------------------------
USED=$(field TOS.RUN.STACK used)
CAPACITY=$(field TOS.RUN.STACK capacity)
[ "$USED" -gt 0 ] || fail "no stack use was measured"
if [ $((USED * MIN_STACK_HEADROOM_FRACTION)) -ge "$CAPACITY" ]; then
    fail "stack use $USED of $CAPACITY leaves less than the required headroom"
fi

echo "STAGE2-RUNTIME PASS: $EXPECTED_MODULE verified and executed on the boot path"
echo "  value=$VALUE  accounting=${ACCOUNTING#TOS.RUN.ACCOUNTING }"
echo "  arena peak=$PEAK of $GRANTED granted; stack used=$USED of $CAPACITY"
