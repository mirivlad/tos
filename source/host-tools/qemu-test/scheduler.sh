#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Two processes make progress without either yielding to the other.
#
# ADR-0049 section 4 asks for round-robin over runnable contexts with a fixed
# quantum, and its evidence list asks that two runnable processes each make
# progress "measured, not asserted, by an observable both processes advance".
# This gate is that measurement, and it is deliberately taken from both sides:
#
#   - The nucleus's side. Each process's exit event carries the ticks charged to
#     it, the number of turns it was given, and the first and last tick it ran
#     at. Two processes that ran one after the other cannot produce overlapping
#     [first_tick, last_tick] intervals; two that were interleaved cannot
#     produce anything else. Only the nucleus can report this — a process cannot
#     observe how long it was off the processor.
#
#   - The processes' side, without needing to know which process is speaking.
#     Each runtime brackets a loop that makes no system call at all with two
#     reads of the monotonic tick. If the two brackets overlap, then whichever
#     process wrote which line, both were advancing over the same stretch of
#     ticks — on one processor, that is interleaving, and it required no
#     cooperation from either process because neither loop calls anything.
#
# And the result does not move: two processes over the same boot module both
# compute i32:240, which is the value one process computed before the scheduler
# existed.
#
#   bash host-tools/qemu-test/scheduler.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-scheduler}"
FEATURE=test-two-processes
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-scheduler"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# The value `system/boot/init.tos` computes: 1*2 + 2*3 + ... + 8*9.
EXPECTED_VALUE="i32:240"
# How many processes this boot builds.
PROCESSES=2

fail() {
    echo "scheduler: FAIL: $*" >&2
    exit 1
}

[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features "$FEATURE")
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building isolated test artifact" >&2
    exit 1
}

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PROCESS_BEGIN TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.REFUSED TOS.RUN.UNSTARTABLE"

LOG="$OUT/events.log"

# `field <line> <key>` — the value of `key=` on a line already selected.
field() {
    printf '%s\n' "$1" | tr ' ' '\n' | sed -n "s/^$2=//p" | head -1
}

# --- there were two processes, and they are distinct -------------------------
BEGUN=$(grep -c '^TOS\.RUN\.PROCESS_BEGIN ' "$LOG" || true)
[ "$BEGUN" = "$PROCESSES" ] || fail "$BEGUN processes were launched, expected $PROCESSES"
EXITS=$(grep '^TOS\.RUN\.PROCESS_EXIT ' "$LOG" || true)
COUNT=$(printf '%s\n' "$EXITS" | grep -c . || true)
[ "$COUNT" = "$PROCESSES" ] || fail "$COUNT processes ended, expected $PROCESSES"
IDS=$(printf '%s\n' "$EXITS" | tr ' ' '\n' | sed -n 's/^process=//p' | sort -u | tr '\n' ' ')
[ "$IDS" = "0 1 " ] || fail "the processes that ended are '$IDS', expected two distinct slots"

# --- each one ran, and each one was interrupted and resumed ------------------
# `quanta` is how many times a process was given the processor. More than one
# turn means the processor was taken away from it and handed back, which is the
# preemption; it is not something a process can arrange for itself.
FIRST=""
LAST=""
while IFS= read -r line; do
    id=$(field "$line" process)
    status=$(field "$line" self_reported_status)
    ticks=$(field "$line" ticks)
    quanta=$(field "$line" quanta)
    first=$(field "$line" first_tick)
    last=$(field "$line" last_tick)
    [ "$status" = 0 ] || fail "process $id reported status $status"
    [ "$ticks" -gt 0 ] || fail "process $id was charged no ticks: it never ran"
    [ "$quanta" -gt 1 ] ||
        fail "process $id was given the processor $quanta time(s): it was never preempted and resumed"
    [ "$last" -gt "$first" ] || fail "process $id ran within a single tick"
    FIRST="$FIRST $first"
    LAST="$LAST $last"
done <<EOF
$EXITS
EOF

# --- and they ran interleaved, not one after the other -----------------------
set -- $FIRST
FIRST_A=$1
FIRST_B=$2
set -- $LAST
LAST_A=$1
LAST_B=$2
if [ "$FIRST_A" -gt "$LAST_B" ] || [ "$FIRST_B" -gt "$LAST_A" ]; then
    fail "the processes ran over disjoint tick ranges [$FIRST_A,$LAST_A] and [$FIRST_B,$LAST_B]: one ran after the other"
fi

# --- measured again from inside the processes, without asking which is which --
# Each `spin_begin`/`spin_end` pair brackets a loop that makes no system call.
# Overlapping brackets mean both processes were advancing over the same ticks,
# and neither gave anything up to let the other do it.
SPINS=$(grep '^TOS\.RUN\.TICKS ' "$LOG" || true)
SPUN=$(printf '%s\n' "$SPINS" | grep -c . || true)
[ "$SPUN" = "$PROCESSES" ] || fail "$SPUN processes reported a tick bracket, expected $PROCESSES"
SPIN_BEGAN=$(printf '%s\n' "$SPINS" | tr ' ' '\n' | sed -n 's/^spin_begin=//p' | tr '\n' ' ')
SPIN_ENDED=$(printf '%s\n' "$SPINS" | tr ' ' '\n' | sed -n 's/^spin_end=//p' | tr '\n' ' ')
set -- $SPIN_BEGAN
SPIN_BEGAN_A=$1
SPIN_BEGAN_B=$2
set -- $SPIN_ENDED
SPIN_ENDED_A=$1
SPIN_ENDED_B=$2
[ "$SPIN_ENDED_A" -gt "$SPIN_BEGAN_A" ] && [ "$SPIN_ENDED_B" -gt "$SPIN_BEGAN_B" ] ||
    fail "a process spun without the tick advancing: it was not interrupted"
if [ "$SPIN_BEGAN_A" -gt "$SPIN_ENDED_B" ] || [ "$SPIN_BEGAN_B" -gt "$SPIN_ENDED_A" ]; then
    fail "the two call-free loops did not overlap: [$SPIN_BEGAN_A,$SPIN_ENDED_A] and [$SPIN_BEGAN_B,$SPIN_ENDED_B]"
fi

# --- the result did not move -------------------------------------------------
VALUES=$(grep '^TOS\.RUN\.COMPLETED ' "$LOG" | tr ' ' '\n' | sed -n 's/^value=//p' | sort -u)
[ "$VALUES" = "$EXPECTED_VALUE" ] ||
    fail "the processes returned '$VALUES', expected every one to return $EXPECTED_VALUE"
COMPLETED=$(grep -c '^TOS\.RUN\.COMPLETED ' "$LOG" || true)
[ "$COMPLETED" = "$PROCESSES" ] ||
    fail "$COMPLETED runs completed, expected $PROCESSES"

# --- and both gave their memory back -----------------------------------------
RECLAIMED=$(grep -c '^TOS\.RUN\.PROCESS_RECLAIMED ' "$LOG" || true)
[ "$RECLAIMED" = "$PROCESSES" ] ||
    fail "$RECLAIMED processes returned their memory, expected $PROCESSES"

echo "SCHEDULER PASS: $PROCESSES processes made progress without either yielding"
echo "  nucleus: process ran over ticks [$FIRST_A,$LAST_A] and [$FIRST_B,$LAST_B] — overlapping"
echo "  processes: call-free loops spanned [$SPIN_BEGAN_A,$SPIN_ENDED_A] and [$SPIN_BEGAN_B,$SPIN_ENDED_B] — overlapping"
echo "  both returned $EXPECTED_VALUE"
