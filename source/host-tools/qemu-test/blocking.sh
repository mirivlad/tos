#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A wait nobody can end is ended, and said so — not halted as a success.
#
# ADR-0059 decides what it means to wait. The part that needs a system which has
# genuinely stopped, rather than one that looks stopped, is this gate: a process
# is given the right to receive on an endpoint **nobody can send to** — there is
# no other process, and no operation creates an endpoint — so the wait it enters
# is one nothing in the system can satisfy.
#
# What must then happen, in order:
#
#   1. the non-blocking form still answers. A receive that asked not to wait is
#      told `E_WOULD_BLOCK`, which `SYSTEM_ABI_V1` §4 assigns to exactly that;
#   2. the blocking form waits, and the process stops being runnable;
#   3. the scheduler finds nothing runnable and something blocked. In Stage 3
#      one interrupt is routed and it wakes nobody, so that state is not a pause
#      — it is a state nothing can leave. Every block is cancelled at that
#      instant with `E_CANCELLED`, and the nucleus says who was blocked on what;
#   4. `E_CANCELLED` reaches the process, which is proved by what it does next:
#      it reports the status and asks once more, which only a resumed process
#      can do;
#   5. the rule fires a second time with no message delivered in between. That
#      is not a wait that has not been satisfied yet; it is a wait for something
#      that will not happen, and the nucleus ends the blocked contexts;
#   6. **the boot fails.** A system that stopped reports that it stopped. The
#      whole point of the rule is that this is not `TOS.HALT ok`.
#
# The last is why the expected exit code here is 75 — RESULT_BOOT_MODULE_FAILED,
# 0x25 — and not 33.
#
#   bash host-tools/qemu-test/blocking.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-blocking}"
FEATURE=test-deadlock
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-deadlock"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# `SYSTEM_ABI_V1` §4 statuses, by the numbers the contract assigns.
E_WOULD_BLOCK=-4
E_CANCELLED=-5
# `endpoint_receive` is operation 2.
RECEIVE=2
# RESULT_BOOT_MODULE_FAILED (0x25), as QEMU reports it: (code << 1) | 1.
EXPECT=75

fail() {
    echo "blocking: FAIL: $*" >&2
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
    --expect "$EXPECT" \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE"

LOG="$OUT/events.log"

count() { grep -c "$1" "$LOG" || true; }
exactly() {
    local seen
    seen=$(count "$2")
    [ "$seen" = "$1" ] || fail "$3: saw $seen line(s) matching '$2', expected $1"
}

# --- the non-blocking form still answers ------------------------------------
exactly 1 "^TOS\\.RUN\\.IPC\\.POLLED status=$E_WOULD_BLOCK\$" \
    "a receive that asked not to wait was not told there was nothing to take"

# --- the rule fired, once, and named what was waiting ------------------------
exactly 1 "^TOS\\.RUN\\.BLOCK_CANCELLED process=0 operation=$RECEIVE endpoint=[0-9]* reason=no-runnable-context asserted_by=nucleus\$" \
    "the liveness rule did not fire exactly once before the deadlock"

# --- and the cancellation reached the process --------------------------------
# Only a process that was resumed can report and ask again, so this line is the
# evidence that `E_CANCELLED` was delivered rather than merely recorded.
exactly 1 "^TOS\\.RUN\\.IPC\\.WAIT status=$E_CANCELLED attempt=1\$" \
    "the process did not observe its wait being cancelled"

# --- the second firing found nothing had moved -------------------------------
exactly 1 '^TOS\.RUN\.DEADLOCK asserted_by=nucleus$' \
    "the nucleus did not diagnose a wait that would never be satisfied"
exactly 1 "^TOS\\.RUN\\.PROCESS_DEADLOCKED process=0 operation=$RECEIVE endpoint=[0-9]* asserted_by=nucleus\$" \
    "the blocked context was not ended and attributed"

# --- the process did its own work before it waited ---------------------------
# Without this the gate would pass on a boot that never got as far as waiting.
exactly 1 '^TOS\.RUN\.COMPLETED value=i32:240$' \
    "the process did not complete its own work before blocking"

# --- and its memory came back ------------------------------------------------
exactly 1 '^TOS\.RUN\.PROCESS_RECLAIMED ' \
    "a context ended by the deadlock rule did not return its memory"

# --- the boot said it failed --------------------------------------------------
# The whole rule exists so that this line is here instead of `TOS.HALT ok`.
exactly 1 '^TOS\.BOOTMODULE\.FAIL stage=process$' \
    "a system that stopped did not report that it stopped"
[ "$(count '^TOS\.HALT ok')" = 0 ] ||
    fail "the boot halted as a success after the system had stopped"

echo "BLOCKING PASS: a wait nothing could satisfy was ended, diagnosed and reported"
echo "  the non-blocking form answered $E_WOULD_BLOCK; the blocking form waited"
echo "  the liveness rule fired once, the process observed $E_CANCELLED and asked again"
echo "  the second firing found nothing delivered, so the wait was named a deadlock"
echo "  the boot failed with RESULT_BOOT_MODULE_FAILED rather than halting ok"
