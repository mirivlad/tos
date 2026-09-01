#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# An endpoint with one receiver cannot acquire a second.
#
# `IPC_V1` §2, stated once and load-bearing: "An endpoint has exactly one
# receive-rights holder at a time. A second one would make delivery
# non-deterministic in a way no schema could describe." §9.4 asks for it as
# conformance evidence, and until this gate existed nothing checked it — a
# launcher could endow two processes with `receive` on one endpoint and both
# would have been able to take messages, with which of them got a given message
# decided by which called first.
#
# **The launcher constant this boots is wrong on purpose.** It asks for exactly
# what §2 forbids. That is the only way to ask the question: a rule that is only
# ever obeyed is a rule nobody has tested, and a gate over a correct constant
# would prove that the launcher is correct rather than that the rule is enforced.
#
# The refusal is at the door authority comes through — the one place a table
# entry is ever written — and not at `endpoint_receive`. Checking it there would
# be checking it after it was already broken: both processes would hold the
# right, and refusing the second *call* would make the outcome depend on call
# order, which is the non-determinism the rule exists to prevent.
#
# Three lines are needed and none is sufficient alone:
#
#   1. The refusal happened, and it names the rule rather than a full table.
#   2. The first process **did** get the right — otherwise a `grant` that
#      refused everything would pass this gate.
#   3. The second process started anyway, holding nothing. A rule that killed
#      the boot would be a different rule; `CAPABILITY_V1` §2 wants the
#      endowment named, and "you did not get it" is part of naming it.
#
#   bash host-tools/qemu-test/second-receiver.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-second-receiver}"
FEATURE=test-second-receiver
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-second-receiver"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# `IPC_V1` §2's two halves, as `tos-launch` numbers them.
RIGHT_SEND=1
RIGHT_RECEIVE=2
BOTH_HALVES=$((RIGHT_SEND | RIGHT_RECEIVE))
# The value `system/boot/init.tos` computes.
EXPECTED_VALUE="i32:240"

fail() {
    echo "second-receiver: FAIL: $*" >&2
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
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PROCESS_REFUSED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE"

LOG="$OUT/events.log"

count() { grep -c "$1" "$LOG" || true; }
exactly() {
    local seen
    seen=$(count "$2")
    [ "$seen" = "$1" ] || fail "$3: saw $seen line(s) matching '$2', expected $1"
}

# --- the refusal happened, once, and for the right reason ----------------------
# `endpoint-already-received`, not `table-full`: the second process's table is
# empty, so a nucleus that refused for want of room would be refusing a different
# thing and this gate would be reporting the wrong fact as proved.
exactly 1 '^TOS\.RUN\.PROCESS_REFUSED reason=endowment-endpoint-received asserted_by=nucleus$' \
    "the launcher's second receive-rights grant was not refused by the rule"
exactly 0 '^TOS\.RUN\.PROCESS_REFUSED reason=endowment-table-full.*$' \
    "a grant was refused for want of room, which is not what this boot is about"
exactly 1 '^TOS\.TEST\.SECOND_NOT_CREATED reason=endowment$' \
    "the boot did not report that the second process was never created"

# --- and the first process did get it ------------------------------------------
# Without this the gate passes on a nucleus whose `grant` refuses everything.
exactly 1 "^TOS\\.RUN\\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=$BOTH_HALVES binding=endpoint\$" \
    "the one process allowed to receive did not get the right"

# --- and the second was never created at all -----------------------------------
# An endowment is written whole or not at all (ADR-0055), so the launcher does
# not get a process holding less than it decided. Until this gate was brought to
# the contract, it got exactly that: a published, runnable process short of the
# one capability it was launched for, with no way to know it was missing.
exactly 0 '^TOS\.RUN\.PROCESS_ENDOWED process=1.*$' \
    "a second process was endowed, when the whole creation should have been refused"
exactly 0 '^TOS\.RUN\.PROCESS_BEGIN process=1.*$' \
    "a second process was begun, when the whole creation should have been refused"
exactly 1 '^TOS\.RUN\.CAPABILITY held=1 .*$' \
    "more or fewer than one process reported what it holds"

# --- the boot is a working system with one refusal in it -----------------------
exactly 1 "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$" \
    "the one process that was startable did not complete its own work"

# --- and the refusal has the consequence it should -----------------------------
# The refused process was the only other party on that endpoint, so once it does
# not exist there is nobody to send. The one holder's wait is therefore one nothing
# in the system can satisfy, and ADR-0059's liveness rule ends it. Asserted
# rather than tolerated: a wait that was *not* cancelled here would mean somebody
# sent, which would mean the refusal did not take.
exactly 1 '^TOS\.RUN\.BLOCK_CANCELLED process=0 operation=2 endpoint=0 reason=no-runnable-context asserted_by=nucleus$' \
    "the surviving receiver's wait was satisfied by a process that should hold nothing"

echo "SECOND-RECEIVER PASS: one endpoint, one receive-rights holder"
echo "  the launcher asked for a second and the whole creation was refused, by the rule"
echo "  the first process kept both halves; the second was never created"
