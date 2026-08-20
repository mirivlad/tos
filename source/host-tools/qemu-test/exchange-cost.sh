#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# What one request/reply costs at the edge, measured rather than argued.
#
# `docs/35`, restated by `IPC_V1` §8, allows **four** user/kernel boundary
# crossings per request/reply, excluding scheduler preemption. ADR-0063 derives
# that four are forced — the client in, the server out, the server in, the client
# out — so the bound is tight and a conforming system must hit the minimum with
# no slack anywhere. This gate is where that stops being an argument.
#
# **The boots are instruments, not demonstrations.** Under the cost constants a
# process does its half of the exchange and nothing else: no poll, no probe, no
# delegation, no second endpoint. So the nucleus's IPC counters describe the
# exchange rather than the exchange plus everything around it, and no subtraction
# stands between the count and the claim.
#
# **The number that matters is a slope.** A server's loop is entered once and
# left once, and those two crossings belong to no exchange; a boot with one
# exchange therefore costs more per exchange than a boot with three. Rather than
# decide which crossings were the prologue — a decision, and therefore an
# estimate — this runs each server shape twice, at one exchange and at three, and
# takes the difference. What is left is the marginal cost of an exchange, and the
# fixed cost cancels without anybody's opinion entering it.
#
# Four boots, two shapes:
#
#   - **two operations**: `endpoint_reply` then `endpoint_receive`, the only
#     shape this ABI had before operation 13;
#   - **one operation**: `endpoint_reply_receive`, which answers and waits again
#     without returning to CPL 3 in between.
#
# Same client, same questions, same answers, same payload copies. The difference
# between the two slopes is the two surplus crossings ADR-0063 identified by
# name — the return of `endpoint_reply` and the entry of the next
# `endpoint_receive`, with nothing happening in between.
#
# And a fifth boot for what the operation must refuse. Each refusal is asked of a
# **live** reply capability, so that what proves nothing was delivered is not an
# assertion here but the answer that follows: the same reply, used afterwards,
# still works, and the client still receives both of its answers.
#
#   bash host-tools/qemu-test/exchange-cost.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-exchange-cost}"
NUCLEUS_FEATURE=test-call-reply

PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TEST_TARGET="$ROOT/target/test-exchange-cost"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
BUILT_IMAGE="$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image"

# `SYSTEM_ABI_V1` §4 and §5, named because this gate checks them.
E_NO_CAPABILITY=-1
E_BAD_HANDLE=-2
E_CANCELLED=-5
ENDPOINT_RECEIVE=2
ENDPOINT_REPLY_RECEIVE=13
# `IPC_V1` §8's bound, and the two payload copies it allows an inline message.
CROSSING_BOUND=4
COPIES_PER_MESSAGE=2

fail() {
    echo "exchange-cost: FAIL: $*" >&2
    exit 1
}

for artifact in "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE"; do
    [ -f "$artifact" ] || {
        echo "missing production artifact: $artifact" >&2
        exit 2
    }
done
before="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features "$NUCLEUS_FEATURE")

# One image per shape and exchange count. Built one after another into the same
# directory and taken away immediately, because what varies is a feature of the
# top crate alone.
image() {
    (cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
        -p tos-runtime-image --target x86_64-unknown-none --features "$2")
    cp "$BUILT_IMAGE" "$1"
}

mkdir -p "$OUT"
image "$OUT/image-two-operations-1.bin" test-exchange-cost
image "$OUT/image-two-operations-3.bin" test-exchange-cost,test-more-exchanges
image "$OUT/image-one-operation-1.bin" test-exchange-cost,test-reply-receive
image "$OUT/image-one-operation-3.bin" \
    test-exchange-cost,test-more-exchanges,test-reply-receive
image "$OUT/image-refusals.bin" test-reply-receive-refusals,test-reply-receive

after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] || {
    echo "a production artifact changed while building the isolated test ones" >&2
    exit 1
}

boot() {
    bash "$ROOT/host-tools/qemu-test/run.sh" \
        --out "$OUT/$1" \
        --nucleus "$TEST_NUCLEUS" \
        --runtime-image "$OUT/image-$1.bin" \
        --expect 33 \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.PROCESS_ENDOWED TOS.HALT" \
        --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.DEADLOCK" \
        >"$OUT/$1.out" 2>&1 || {
        cat "$OUT/$1.out" >&2
        fail "the $1 boot did not complete"
    }
}

# One field of a space-separated `key=value` line, by name rather than by
# position: a reader that counted fields would move with the log's shape.
field() {
    printf '%s\n' "$1" | tr ' ' '\n' | sed -n "s/^$2=//p"
}

count() { grep -c "$1" "$2" || true; }

# What one boot cost, and everything that has to be true of the boot before its
# cost means anything.
#
#   $1 the boot's name   $2 exchanges asked   $3 the operation the server waits in
#   $4 refused operations the boot performs besides the exchange, if any
#
# Sets `crossings` to `ipc_in + ipc_out`.
measure() {
    local name=$1 exchanges=$2 waits_in=$3 probes=${4:-0}
    local log="$OUT/$name/events.log"
    local cost ipc_in ipc_out counted messages copies asked answered served last
    local client_operations server_operations

    cost=$(grep '^TOS\.RUN\.IPC\.COST ' "$log" | head -1)
    [ -n "$cost" ] || fail "$name: the nucleus did not report what its IPC cost"
    ipc_in=$(field "$cost" ipc_in)
    ipc_out=$(field "$cost" ipc_out)
    counted=$(field "$cost" exchanges)
    messages=$(field "$cost" messages)
    copies=$(field "$cost" payload_copies)

    # The invariant that makes this a counter rather than an estimate: every
    # operation that entered the nucleus came back exactly once, by whichever of
    # the three doors it used. It is asserted here because it was once false.
    [ "$ipc_in" = "$ipc_out" ] ||
        fail "$name: $ipc_in IPC operations entered the nucleus and $ipc_out came back"
    [ "$counted" = "$exchanges" ] ||
        fail "$name: the nucleus counted $counted exchange(s), the client asked $exchanges"

    # Both halves of the exchange, as the processes themselves report them. The
    # client's answers are what say the exchange happened at all, and the
    # server's last status is `E_CANCELLED` because its final wait is one nothing
    # can satisfy — ADR-0059's rule ending a loop the client has left.
    asked=$(sed -n "s/^TOS\\.RUN\\.EXCHANGE\\.ASKED asked=\\([0-9]*\\) .*$/\\1/p" "$log")
    answered=$(sed -n "s/^TOS\\.RUN\\.EXCHANGE\\.ASKED .* answered=\\([0-9]*\\) .*$/\\1/p" "$log")
    [ "$asked" = "$exchanges" ] && [ "$answered" = "$exchanges" ] ||
        fail "$name: the client asked $asked question(s) and was answered $answered"
    served=$(sed -n "s/^TOS\\.RUN\\.EXCHANGE\\.SERVED answered=\\([0-9]*\\) .*$/\\1/p" "$log")
    last=$(sed -n "s/^TOS\\.RUN\\.EXCHANGE\\.SERVED .* last=\\(-\\?[0-9]*\\) .*$/\\1/p" "$log")
    [ "$served" = "$exchanges" ] ||
        fail "$name: the server answered $served question(s) of $exchanges"
    [ "$last" = "$E_CANCELLED" ] ||
        fail "$name: the server's last wait ended with $last, not a cancellation"

    # The nucleus's count of what crossed the edge, against the two processes'
    # count of what they asked for, including the refused ones — an operation
    # that is refused still crossed the edge and still cost what a crossing
    # costs. A counter that cannot be checked from the other side of the
    # boundary is the nucleus talking about itself.
    client_operations=$(sed -n \
        "s/^TOS\\.RUN\\.EXCHANGE\\.ASKED .* operations=\\([0-9]*\\)$/\\1/p" "$log")
    server_operations=$(sed -n \
        "s/^TOS\\.RUN\\.EXCHANGE\\.SERVED .* operations=\\([0-9]*\\)$/\\1/p" "$log")
    [ "$ipc_in" = "$((client_operations + server_operations + probes))" ] ||
        fail "$name: the processes performed $client_operations + $server_operations + $probes IPC operations and the nucleus counted $ipc_in"

    # `IPC_V1` §8's other counted bound, on the same boot.
    [ "$copies" -le "$((COPIES_PER_MESSAGE * messages))" ] ||
        fail "$name: $copies payload copies for $messages message(s)"

    # Exactly one wait was cancelled — the server's last — and the record names
    # the operation it was waiting **in**. Two operations of this ABI wait for a
    # message, so a record that named the wait rather than the operation would
    # name an operation the process never called.
    local cancelled
    cancelled=$(count "^TOS\\.RUN\\.BLOCK_CANCELLED process=[0-9]* operation=$waits_in " "$log")
    [ "$cancelled" = 1 ] ||
        fail "$name: $cancelled wait(s) were cancelled in operation $waits_in, expected exactly 1"

    crossings=$((ipc_in + ipc_out))
}

# --- the four counting boots --------------------------------------------------
boot two-operations-1
boot two-operations-3
boot one-operation-1
boot one-operation-3

measure two-operations-1 1 "$ENDPOINT_RECEIVE"
two_operations_1=$crossings
measure two-operations-3 3 "$ENDPOINT_RECEIVE"
two_operations_3=$crossings
measure one-operation-1 1 "$ENDPOINT_REPLY_RECEIVE"
one_operation_1=$crossings
measure one-operation-3 3 "$ENDPOINT_REPLY_RECEIVE"
one_operation_3=$crossings

# --- what an exchange costs, as a difference ----------------------------------
two_operations_slope=$(( (two_operations_3 - two_operations_1) / 2 ))
one_operation_slope=$(( (one_operation_3 - one_operation_1) / 2 ))
[ "$(( (two_operations_3 - two_operations_1) % 2 ))" = 0 ] &&
    [ "$(( (one_operation_3 - one_operation_1) % 2 ))" = 0 ] ||
    fail "two more exchanges cost an odd number of crossings: an operation crossed once"

[ "$one_operation_slope" -le "$CROSSING_BOUND" ] ||
    fail "one exchange costs $one_operation_slope crossings, and IPC_V1 section 8 allows $CROSSING_BOUND"

# The shape without operation 13 is measured beside it, on the same boot machine
# with the same client, because "fewer than before" is a claim about two numbers
# and only one of them was ever recorded.
[ "$two_operations_slope" = 6 ] ||
    fail "the two-operation server cost $two_operations_slope crossings per exchange, not the six an operation-pair costs"
[ "$((two_operations_slope - one_operation_slope))" = 2 ] ||
    fail "answering in one operation saved $((two_operations_slope - one_operation_slope)) crossings, not the two ADR-0063 identified"

# --- and every way the operation refuses --------------------------------------
boot refusals
LOG="$OUT/refusals/events.log"

first=$(grep -m1 '^TOS\.RUN\.EXCHANGE\.REFUSED swapped=' "$LOG")
[ -n "$first" ] || fail "the server did not report what it was refused"
swapped=$(field "$first" swapped)
no_reply=$(field "$first" no_reply)
no_endpoint=$(field "$first" no_endpoint)
sending=$(field "$first" sending)

# Neither position accepts the other's object, and the caller held both. Refused
# by the rights the two capabilities carry, which is `E_NO_CAPABILITY`: the
# contract's own precedence — index, generation, type, rights — makes everything
# past the index the same status on purpose, so what distinguishes these
# refusals is not the number but which capability was refused, and that is
# visible in what survives them.
[ "$swapped" = "$E_NO_CAPABILITY" ] ||
    fail "passing the endpoint where the reply belongs answered $swapped"
[ "$no_reply" = "$E_BAD_HANDLE" ] ||
    fail "a reply handle naming nothing answered $no_reply"
[ "$no_endpoint" = "$E_BAD_HANDLE" ] ||
    fail "an endpoint handle naming nothing answered $no_endpoint"
[ "$sending" = "$E_NO_CAPABILITY" ] ||
    fail "a process holding \`receive\` and a reply was allowed to send: $sending"

second=$(grep -m1 '^TOS\.RUN\.EXCHANGE\.REFUSED carried=' "$LOG")
[ -n "$second" ] || fail "the server did not report the second set of refusals"
no_right=$(field "$second" no_right)
again=$(field "$second" again)
[ "$no_right" = "$E_NO_CAPABILITY" ] ||
    fail "an endpoint capability without \`receive\` was accepted where the operation declares it: $no_right"
[ "$again" = "$E_NO_CAPABILITY" ] ||
    fail "a reply capability already spent was accepted a second time: $again"

# What makes every refusal above a refusal that delivered **nothing**: the same
# reply capabilities were used afterwards and worked, and the client received
# both of its answers. A refusal that had answered the caller, spent the reply or
# entered a wait would have cost one of these.
# Six of them are the refusals above, which crossed the edge like any other
# operation and are counted like any other: `IPC_V1` §8 bounds an exchange, and
# a boot that also asks six questions it expects to be refused is not one.
measure refusals 2 "$ENDPOINT_REPLY_RECEIVE" 6

echo "EXCHANGE-COST PASS: one request/reply costs $one_operation_slope crossings, and IPC_V1 section 8 allows $CROSSING_BOUND"
echo "  measured as a slope: $one_operation_1 crossings for one exchange, $one_operation_3 for three"
echo "  the same client answered in two operations costs $two_operations_slope per exchange:" \
     "$two_operations_1 and $two_operations_3"
echo "  the difference is exactly the two ADR-0063 named: a reply's return and the next receive's entry"
echo "  every operation that entered the nucleus came back once, in all five boots"
echo "  the operation refuses whole: swapped=$swapped no_reply=$no_reply no_endpoint=$no_endpoint" \
     "no_right=$no_right spent=$again"
echo "  and every refusal delivered nothing: the replies it refused were spent afterwards, and both answers arrived"
