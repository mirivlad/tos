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

# --- time moved while it ran ------------------------------------------------
# ADR-0049: the timer interrupts a process and the process is resumed. A tick
# that advanced between the runtime's two reads is that, measured from inside
# the process — the only place both ends can be observed — and a tick that did
# not advance would mean the process ran uninterruptible, which is the state
# this system left when the timer was enabled.
BEGAN=$(field TOS.RUN.TICKS begin)
ENDED=$(field TOS.RUN.TICKS end)
SPIN_BEGAN=$(field TOS.RUN.TICKS spin_begin)
SPIN_ENDED=$(field TOS.RUN.TICKS spin_end)
[ -n "$BEGAN" ] || fail "the runtime reported no monotonic tick"
[ "$BEGAN" -gt 0 ] || fail "the tick was still zero when the process started"
[ "$ENDED" -gt "$BEGAN" ] || fail "the tick did not advance while the process ran"
# The stronger half: the loop those two bracket makes no system call, so a tick
# that grew across it was advanced by an interrupt taken while the process ran
# its own instructions.
[ "$SPIN_ENDED" -gt "$SPIN_BEGAN" ] ||
    fail "the tick did not advance while the process spun without calling anything"

# --- and it was given no authority it did not ask for -----------------------
# ADR-0055: the launcher's stated constant for the canonical boot is empty,
# because `system.boot.init` requests no capability and the rule is to grant
# nothing a module did not ask for. Checked here rather than assumed, because
# "the boot process holds nothing" is the claim that makes every later grant
# attributable to a decision.
grep -q '^TOS\.RUN\.PROCESS_ENDOWED process=0 capabilities=0 policy=launcher-constant asserted_by=launcher$' "$LOG" ||
    fail "the launcher did not announce an empty endowment for the boot process"
grep -q '^TOS\.RUN\.CAPABILITY held=0 endowment=empty$' "$LOG" ||
    fail "the boot process did not report holding nothing"

# --- and it gave the memory back when it ended ------------------------------
# ADR-0050 section 3: a dead process's frames return to the pool, cleared. The
# grant alone is `granted` bytes, so a reclamation that returned fewer frames
# than the grant occupies did not return the grant.
RECLAIMED=$(field TOS.RUN.PROCESS_RECLAIMED frames)
AVAILABLE=$(field TOS.RUN.PROCESS_RECLAIMED available)
[ -n "$RECLAIMED" ] || fail "the nucleus did not report reclaiming the process's memory"
[ "$RECLAIMED" -ge $((GRANTED / 4096)) ] ||
    fail "only $RECLAIMED frames came back, less than the $((GRANTED / 4096)) the grant holds"
[ "$AVAILABLE" -gt "$RECLAIMED" ] || fail "the pool holds less than it just took back"

echo "STAGE2-RUNTIME PASS: $EXPECTED_MODULE verified and executed on the boot path"
echo "  value=$VALUE  accounting=${ACCOUNTING#TOS.RUN.ACCOUNTING }"
echo "  arena peak=$PEAK of $GRANTED granted; stack used=$USED of $CAPACITY"
echo "  reclaimed $RECLAIMED frames on exit; $AVAILABLE available to the pool"
echo "  tick $BEGAN -> $ENDED while the process ran; $SPIN_BEGAN -> $SPIN_ENDED while it spun without calling anything"
