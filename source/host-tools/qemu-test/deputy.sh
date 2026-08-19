#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A strong process acting for a weak one does not lend it its own strength.
#
# `CAPABILITY_V1` §7.6, and `docs/37` names it explicitly as the test that fails
# quietly in systems that pass the other five. The pair here is built so that the
# question can actually be asked: the deputy holds `send` **and** `receive` on an
# endpoint, so it can send on its own account and that authority is real; the
# client holds only `call`, so it cannot send at all.
#
# The client asks twice, and the difference between the two answers is the whole
# property.
#
#   1. The first request names its object **by value** — a number in the payload
#      — and carries no capability. That number names something real in the
#      *deputy's* table and nothing in the client's. A deputy that used it would
#      be acting on its own authority at a stranger's direction, which is the
#      confused deputy exactly. It refuses, and the refusal names the request.
#
#   2. The second carries a capability the client actually holds. The deputy acts
#      **with that**, and is refused — the client held `call`, not `send`. One
#      line later the deputy performs the same operation on its own account and
#      succeeds, so the refusal cannot be read as the deputy being weak.
#
# Those two statuses, on the same operation, in the same process, one line apart,
# are the evidence: authority does not attach to the actor, it attaches to what
# the actor was given for the work.
#
#   bash host-tools/qemu-test/deputy.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-deputy}"
FEATURE=test-deputy
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-deputy"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# `SYSTEM_ABI_V1` §4, and the rights involved.
E_NO_CAPABILITY=-1
RIGHT_CALL=4
BOTH_HALVES=3

fail() {
    echo "deputy: FAIL: $*" >&2
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

# --- the pair is genuinely unequal -------------------------------------------
# Without this the rest proves nothing: a deputy that could not send anyway
# would refuse for the wrong reason.
exactly 1 "^TOS\\.RUN\\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=$BOTH_HALVES binding=endpoint\$" \
    "no process holds both halves of the endpoint, so there is no strong deputy"
exactly 1 "^TOS\\.RUN\\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=$RIGHT_CALL binding=endpoint\$" \
    "no process holds only the right to call, so there is no weak client"

# --- naming an object by value gets nothing ----------------------------------
exactly 1 '^TOS\.RUN\.DEPUTY\.REFUSED request=0 reason=named-by-value bytes=[0-9]*$' \
    "a request naming its object by value was not refused"

# --- and acting with what the client held is bounded by what the client held --
# `for_client` is the deputy performing `endpoint_send` with the capability the
# client handed over; `on_own_account` is the same operation with its own. The
# first must be refused and the second must succeed, and they are one line apart
# in one process — so the difference is whose authority was used and nothing
# else.
exactly 1 "^TOS\\.RUN\\.DEPUTY\\.ACTED request=1 for_client=$E_NO_CAPABILITY on_own_account=0\$" \
    "the deputy's own strength attached to work it did for the client"

# --- the client's own view agrees ---------------------------------------------
exactly 1 '^TOS\.RUN\.DEPUTY\.ASKED named_by_value=0 with_capability=0$' \
    "the client did not get an answer to both of its requests"

# --- nothing stalled ----------------------------------------------------------
[ "$(count '^TOS\.RUN\.BLOCK_CANCELLED ')" = 0 ] ||
    fail "a wait was cancelled on a run that was making progress"
[ "$(count '^TOS\.RUN\.DEADLOCK ')" = 0 ] ||
    fail "a system that answered both requests was called deadlocked"

# --- and both processes did their own work -----------------------------------
exactly 2 '^TOS\.RUN\.COMPLETED value=i32:240$' \
    "the processes did not both complete their own work"

echo "DEPUTY PASS: a strong process acted for a weak one without lending it strength"
echo "  a request naming its object by value was refused: a number is not a handle"
echo "  acting with the client's capability was refused ($E_NO_CAPABILITY); the same"
echo "  operation on the deputy's own account succeeded (0), one line later"
