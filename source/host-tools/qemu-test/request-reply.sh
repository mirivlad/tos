#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# One process asks a question and another answers it, on authority neither made.
#
# `IPC_V1` §4 describes request and reply, and the part that is easy to fake is
# the one this gate is about: **the right to answer**. It is not in anybody's
# endowment. The nucleus makes it when the call is made, hands it to whoever
# receives the request, and it is spent by being used — so what the log shows is
# an authority that came into existence for one question and stopped existing
# when that question was answered.
#
#   - the client holds `call` and not `send` or `receive`; the server holds
#     `receive` and nothing else. Neither can perform the other's half, and
#     neither was given the right to reply;
#   - `endpoint_call` does not return until the answer arrives. It is a block,
#     so the caller is not runnable while it waits and is charged nothing for
#     waiting;
#   - the reply capability arrives in the **last** slot of the transfer table,
#     always, so a receiver knows where to look without being told how many
#     capabilities the caller chose to send;
#   - answering spends it. The second attempt with the same handle is refused,
#     which is what single-use means rather than a claim about it — and it is
#     per *call*, so a client that asks twice is answered twice, by two
#     capabilities neither of which outlives its own question;
#   - and the liveness rule never fires. A run that makes progress must cost
#     nothing at all, or the rule would be a tax on working systems.
#
#   bash host-tools/qemu-test/request-reply.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-request-reply}"
FEATURE=test-call-reply
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-call-reply"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# What the client asks and what the server answers.
QUESTION="what-is-the-answer"
QUESTION_BYTES=18
ANSWER="i32:240"
ANSWER_BYTES=7
# `SYSTEM_ABI_V1` §4, and the two endpoint rights involved.
E_NO_CAPABILITY=-1
RIGHT_RECEIVE=2
RIGHT_CALL=4

fail() {
    echo "request-reply: FAIL: $*" >&2
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
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PROCESS_ENDOWED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE"

LOG="$OUT/events.log"

count() { grep -c "$1" "$LOG" || true; }
exactly() {
    local seen
    seen=$(count "$2")
    [ "$seen" = "$1" ] || fail "$3: saw $seen line(s) matching '$2', expected $1"
}

# --- one may call, the other may receive, and that is all either was given ---
exactly 1 "^TOS\\.RUN\\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=$RIGHT_CALL binding=endpoint\$" \
    "no process holds exactly the right to call"
exactly 1 "^TOS\\.RUN\\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=$RIGHT_RECEIVE binding=endpoint\$" \
    "no process holds exactly the right to receive"

# --- the question arrived whole -----------------------------------------------
exactly 1 "^TOS\\.RUN\\.IPC\\.RECEIVED bytes=$QUESTION_BYTES text=$QUESTION\$" \
    "the server did not read the client's question back whole"

# --- and was answered with a capability nobody was endowed with --------------
# The client asks twice — once carrying nothing, once carrying a capability —
# and both are answered. Each answer spends its own reply capability, and each
# second attempt with the same one is refused: single use is per call, not per
# process.
exactly 2 "^TOS\\.RUN\\.IPC\\.REPLIED status=0 handle=0x[0-9a-f]* again=$E_NO_CAPABILITY\$" \
    "a reply failed, or a reply capability survived being used"

# --- the caller's own call returned the answer -------------------------------
# `endpoint_call` blocks, so this line existing at all means the caller was
# suspended and resumed with a result written into the call it had made.
exactly 1 "^TOS\\.RUN\\.IPC\\.CALLED status=0 bytes=$ANSWER_BYTES answer=$ANSWER\$" \
    "the caller did not receive the answer to its own question"

# --- the liveness rule cost nothing ------------------------------------------
# ADR-0059's rule fires only when nothing can proceed. A run in which anything
# proceeds must never see it, or waiting would be a tax on working systems.
[ "$(count '^TOS\.RUN\.BLOCK_CANCELLED ')" = 0 ] ||
    fail "a wait was cancelled on a run that was making progress"
[ "$(count '^TOS\.RUN\.DEADLOCK ')" = 0 ] ||
    fail "a system that answered its own question was called deadlocked"

# --- and both processes did their own work -----------------------------------
exactly 2 '^TOS\.RUN\.COMPLETED value=i32:240$' \
    "the processes did not both complete their own work"

# --- what the exchange cost, counted --------------------------------------
# `IPC_V1` §8 bounds an inline message at **two payload copies**, and §9.7 asks
# for the number to be counted rather than estimated. The nucleus counts one
# increment beside each copy it makes, so this is the count and not a reading of
# the code.
#
# A reply costs one copy, not two: it goes from the replier's region straight
# into the region of the caller already waiting for it, because there is nobody
# to queue it for. So the ratio comes out below the bound rather than at it, and
# a boot that hit exactly two per message would mean the reply had been queued.
cost=$(grep '^TOS\.RUN\.IPC\.COST ' "$LOG" | head -1)
[ -n "$cost" ] || fail "the nucleus did not report what its IPC cost"
messages=$(printf '%s' "$cost" | sed -n 's/.* messages=\([0-9]*\) .*/\1/p')
copies=$(printf '%s' "$cost" | sed -n 's/.* payload_copies=\([0-9]*\) .*/\1/p')
[ "$messages" -gt 0 ] || fail "the boot carried no message, so its cost measures nothing"
[ "$copies" -le $(( 2 * messages )) ] ||
    fail "$copies payload copies for $messages message(s): IPC_V1 section 8 allows two each"

# --- and every operation that entered came back exactly once -------------------
# The invariant that makes the crossing counter an instrument rather than a
# guess, and it is here because it was once false. A call that blocks does not
# return through the edge: it is set down and picked up later, by the scheduler
# **or** by a timer tick switching to it — and which door depends only on whether
# whoever woke it went on to block or ran into a tick. An earlier counter watched
# one door, so two boots with identical crossing profiles reported different
# totals, and the number moved with the interleaving instead of with the work.
#
# Balanced, it says something worth having: an IPC operation costs exactly two
# crossings, one each way. A request/reply served this way is three operations —
# `endpoint_call`, `endpoint_receive`, `endpoint_reply` — so it costs six, where
# IPC_V1 section 8 allows four. That is what operation 13 was added for
# (ADR-0063), and the bound is met and measured by `exchange-cost.sh`, on boots
# whose only IPC is the exchange. This boot is not one of them: it polls, probes
# and delegates, so its total is a total and not an exchange's cost. What it
# holds is the invariant underneath that measurement.
ipc_in=$(printf '%s' "$cost" | sed -n 's/.* ipc_in=\([0-9]*\) .*/\1/p')
ipc_out=$(printf '%s' "$cost" | sed -n 's/.* ipc_out=\([0-9]*\)$/\1/p')
[ -n "$ipc_in" ] && [ -n "$ipc_out" ] || fail "the nucleus did not report its IPC crossings"
[ "$ipc_in" = "$ipc_out" ] ||
    fail "$ipc_in IPC operations entered the nucleus and $ipc_out came back: the count is losing one"

echo "REQUEST-REPLY PASS: a question was asked, answered, and the right to answer spent"
echo "  the client held only \`call\`, the server only \`receive\`; neither held the right to reply"
echo "  the answer reached the caller inside the call it had blocked in"
echo "  the same reply capability, used twice, was refused the second time"
echo "  $copies payload copies for $messages message(s), counted by the nucleus, not estimated"
echo "  $ipc_in IPC operations in and $ipc_out out: every one that entered came back once"
echo "  no wait was cancelled: on a run that progresses the liveness rule costs nothing"
