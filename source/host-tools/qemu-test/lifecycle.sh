#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0067: a supervisor collects the endings of its own children.
#
# The decision's claim is that a lifecycle notice can be neither lost nor
# forged, without a queue, an allocation or a bound anybody had to choose: the
# record lives in the ended child's own slot until the parent takes it, and the
# storage for the notice is the storage for the process. This boot is where that
# stops being a claim.
#
# One supervisor, endowed with `create`, `terminate` and `wait_child` over
# itself, and four children — which is what a boot affords, because each memory
# grant takes the largest contiguous run there is and the runs get smaller. What
# it demonstrates, in the order the log carries it:
#
#   1. two children that ended before a single wait: both records survive, in
#      the order they ended in, and a third wait has nothing to take;
#   2. identity travels: the instance id `process_create_with_generation` left
#      in the argument region is the one the record carries, and it is not the
#      capability handle;
#   3. the restart generation is repeated verbatim from what the supervisor
#      asserted — 7 and 9 here, chosen so neither could be a default;
#   4. a capability over a collected child does not come back to life;
#   5. a process capability without `wait_child` cannot observe, though it can
#      still create and terminate;
#   6. a child of `process_create` (8) has **no** generation rather than zero,
#      and one that reaches its own `process_exit` carries the self-reported
#      status a terminated child cannot have;
#   7. three uncollected endings hold the three slots this supervisor is not in,
#      so a fourth creation is refused for want of a *slot* — the nucleus names
#      which bound it hit — and one collection frees exactly one;
#   8. a wait nothing can end is ended by the nucleus, as `E_CANCELLED`.
#
# Then a second boot, because the arrangement it needs — a supervisor, a middle
# parent and a delegated observer — is three live processes, and each memory
# grant takes the largest contiguous run there is. It demonstrates §9a and §10:
#
#   8. an observer holding `wait_child` over *another* process blocks on that
#      process's children, and is answered `E_CANCELLED` when that process ends,
#      because the set it subscribed to can gain no further member;
#   9. an ending its parent never collected is released when the parent itself
#      ends, rather than holding a slot for a reader that no longer exists.
#
# And a third, for the half of §6 the other two cannot reach: a delegated
# collector that is **already blocked** when a child of the live parent it
# watches ends. Delivery must find it — being blocked on the relation is the
# authorization, the capability having been checked when the call was made — and
# a delivery that looked only at the parent left it blocked beside a record it
# was entitled to. That is the shape this boot exists to refuse.
#
#   bash host-tools/qemu-test/lifecycle.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-lifecycle}"
FEATURE=test-lifecycle
PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TEST_TARGET="$ROOT/target/test-lifecycle"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
TEST_IMAGE="$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image"

# `SYSTEM_ABI_V1` §4 statuses and ADR-0067's ending kinds, by the numbers the
# contract assigns rather than by the names this script would prefer.
OK=0
E_NO_CAPABILITY=-1
E_WOULD_BLOCK=-4
E_CANCELLED=-5
E_LIMIT=-6
ENDING_EXITED=1
ENDING_TERMINATED=3

fail() {
    echo "lifecycle: FAIL: $*" >&2
    exit 1
}

for artifact in "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE"; do
    [ -f "$artifact" ] || { echo "missing production artifact: $artifact" >&2; exit 2; }
done
before="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features "$FEATURE")
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features "$FEATURE")
after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] ||
    fail "a production artifact changed while building the test ones"

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$TEST_IMAGE" > "$OUT.log" 2>&1 || {
        cat "$OUT.log" >&2
        fail "the lifecycle boot did not complete"
    }

log="$(tr -c '[:print:]\n' ' ' < "$OUT/serial.log")"
field() { # line-prefix field-name
    printf '%s\n' "$log" | sed -n "s/.*$1[^\\n]*[ ]$2=\([-0-9]*\).*/\1/p" | tail -1
}
line() { printf '%s\n' "$log" | grep -F "$1" | tail -1; }

# 1. Two endings, both collected, in the order they happened.
first="$(line 'TOS.RUN.LIFECYCLE.RECORD which=first')"
second="$(line 'TOS.RUN.LIFECYCLE.RECORD which=second')"
[ -n "$first" ] && [ -n "$second" ] || fail "the supervisor collected fewer than two endings"
for record in "$first" "$second"; do
    printf '%s\n' "$record" | grep -q " status=$OK " ||
        fail "a collection did not answer OK: $record"
    printf '%s\n' "$record" | grep -q "kind=$ENDING_TERMINATED " ||
        fail "an ended-by-authority child is not recorded as terminated: $record"
    printf '%s\n' "$record" | grep -q "ended_by=1/1" ||
        fail "the record does not name who ended the child: $record"
    printf '%s\n' "$record" | grep -q "status_present=0" ||
        fail "a terminated child reports a self-reported status it never made: $record"
done
first_order="$(printf '%s\n' "$first" | sed -n 's/.* order=\([0-9]*\).*/\1/p')"
second_order="$(printf '%s\n' "$second" | sed -n 's/.* order=\([0-9]*\).*/\1/p')"
[ "$first_order" -lt "$second_order" ] ||
    fail "the endings were collected out of order: $first_order then $second_order"

# A third wait, with nothing left: the count is a fact, not a convenience.
empty="$(field 'TOS.RUN.LIFECYCLE.EMPTY' 'status')"
[ "$empty" = "$E_WOULD_BLOCK" ] ||
    fail "a wait with nothing pending answered $empty, expected $E_WOULD_BLOCK"

# 2 and 3. Identity and the asserted generation, neither invented by the nucleus.
created="$(line 'TOS.RUN.LIFECYCLE.CREATED')"
first_instance="$(printf '%s\n' "$created" | sed -n 's/.*first=0\/\([0-9]*\).*/\1/p')"
second_instance="$(printf '%s\n' "$created" | sed -n 's/.*second=0\/\([0-9]*\).*/\1/p')"
[ -n "$first_instance" ] && [ "$first_instance" != 0 ] ||
    fail "process_create_with_generation reported no instance id: $created"
[ "$first_instance" != "$second_instance" ] ||
    fail "two children were given the same instance id: $created"
printf '%s\n' "$first" | grep -q "child=$first_instance " ||
    fail "the record names a child the creator was not told about: $first"
printf '%s\n' "$second" | grep -q "child=$second_instance " ||
    fail "the record names a child the creator was not told about: $second"
printf '%s\n' "$first" | grep -q "generation=7/1" ||
    fail "the first child's asserted generation is not repeated verbatim: $first"
printf '%s\n' "$second" | grep -q "generation=9/1" ||
    fail "the second child's asserted generation is not repeated verbatim: $second"

# 4. A capability over a collected child names nothing, not somebody else.
stale="$(field 'TOS.RUN.LIFECYCLE.STALE' 'status')"
[ "$stale" = "$E_NO_CAPABILITY" ] ||
    fail "a stale child capability answered $stale, expected $E_NO_CAPABILITY"

# 5. Observation is a right, and attenuation takes it away.
unrighted="$(field 'TOS.RUN.LIFECYCLE.UNRIGHTED' 'wait')"
[ "$unrighted" = "$E_NO_CAPABILITY" ] ||
    fail "a capability without wait_child answered $unrighted, expected $E_NO_CAPABILITY"

# 7. An uncollected ending is a slot nobody can have.
exhausted="$(line 'TOS.RUN.LIFECYCLE.EXHAUSTED')"
printf '%s\n' "$exhausted" | grep -q 'filled=3 ' ||
    fail "the supervisor could not fill the table it shares with three children: $exhausted"
printf '%s\n' "$exhausted" | grep -q "full=$E_LIMIT " ||
    fail "a creation with every slot held by a record was not refused: $exhausted"
printf '%s\n' "$exhausted" | grep -q "collected=$OK " ||
    fail "the collection that should free a slot did not happen: $exhausted"
printf '%s\n' "$exhausted" | grep -q "after_one=$OK " ||
    fail "one collection did not free exactly one slot: $exhausted"
printf '%s\n' "$exhausted" | grep -q "full_again=$E_LIMIT" ||
    fail "one collection freed more than one slot: $exhausted"
# And the refusal named the bound, which is the whole reason the log says it:
# `E_LIMIT` alone cannot tell a full table from a pool with nothing left.
printf '%s\n' "$log" | grep -q 'PROCESS_REFUSED reason=no-slot uncollected=3' ||
    fail "the refusal did not name the uncollected endings that caused it"

# 6. The legacy form asserts no generation, and a self-exit carries its status.
legacy="$(line 'TOS.RUN.LIFECYCLE.LEGACY')"
printf '%s\n' "$legacy" | grep -q "created=$OK status=$OK " ||
    fail "the legacy child was not created and collected: $legacy"
printf '%s\n' "$legacy" | grep -q "kind=$ENDING_EXITED " ||
    fail "a child that reached process_exit is not recorded as exited: $legacy"
printf '%s\n' "$legacy" | grep -q "status_present=1" ||
    fail "an exited child carries no self-reported status: $legacy"
printf '%s\n' "$legacy" | grep -q "generation_present=0 generation=0" ||
    fail "a child of operation 8 was given a generation nobody asserted: $legacy"

# 7. A wait nothing can end is ended, and says so exactly.
cancelled="$(field 'TOS.RUN.LIFECYCLE.CANCELLED' 'status')"
[ "$cancelled" = "$E_CANCELLED" ] ||
    fail "an unsatisfiable wait answered $cancelled, expected $E_CANCELLED"

# The nucleus names which bound it hit when it refuses to create. Not asserted as
# a particular value: on this platform the memory grant runs out before the
# process table does, and a gate that demanded `no-slot` would be demanding a
# property of the allocator rather than of this decision.
refusals="$(printf '%s\n' "$log" | grep -c 'TOS.RUN.PROCESS_REFUSED' || true)"

# --- The second boot: a delegated observer, and an uncollected ending ---------
(cd "$ROOT" && CARGO_TARGET_DIR="$ROOT/target/test-lifecycle-delegate" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features test-lifecycle-delegate)
delegate_image="$ROOT/target/test-lifecycle-delegate/x86_64-unknown-none/release/tos-runtime-image"
after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] ||
    fail "a production artifact changed while building the delegate image"

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT-delegate" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$delegate_image" > "$OUT-delegate.log" 2>&1 || {
        cat "$OUT-delegate.log" >&2
        fail "the delegated-observer boot did not complete"
    }
log="$(tr -c '[:print:]\n' ' ' < "$OUT-delegate/serial.log")"

# 8. The delegated observer waited on a relation it did not own, and was
#    cancelled with it rather than left blocked or answered with an empty record.
arranged="$(line 'TOS.RUN.LIFECYCLE.ARRANGED')"
printf '%s\n' "$arranged" | grep -q "attenuate=$OK watcher=$OK" ||
    fail "the observer was not endowed with an attenuated authority: $arranged"
printf '%s\n' "$log" | grep -q 'TOS.RUN.LIFECYCLE.WATCHER waiting=1' ||
    fail "the observer never reached its wait"
watcher="$(printf '%s\n' "$log" | sed -n 's/.*WATCHER status=\(-\?[0-9]*\).*/\1/p' | tail -1)"
[ "$watcher" = "$E_CANCELLED" ] ||
    fail "a delegated wait answered $watcher when its relation ended, expected $E_CANCELLED"

# The supervisor collected the parent's own ending, which is the ending that
# cancelled the observer: the two are one event seen from two authorities.
orphaned="$(line 'TOS.RUN.LIFECYCLE.ORPHANED')"
printf '%s\n' "$orphaned" | grep -q "collected=$OK " ||
    fail "the supervisor did not collect the ending it caused: $orphaned"
printf '%s\n' "$orphaned" | grep -q "kind=$ENDING_TERMINATED" ||
    fail "the middle parent is not recorded as terminated: $orphaned"

# 9. And the ending nobody was left to collect was released, not kept.
released="$(line 'TOS.RUN.NOTICE_RELEASED')"
[ -n "$released" ] ||
    fail "an uncollected ending survived its parent without being released"
printf '%s\n' "$released" | grep -q 'reason=parent-ended asserted_by=nucleus' ||
    fail "the release is not asserted by the nucleus with its reason: $released"

# --- The third boot: a collector already blocked when the ending happens ------
(cd "$ROOT" && CARGO_TARGET_DIR="$ROOT/target/test-lifecycle-collector" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features test-lifecycle-collector)
collector_image="$ROOT/target/test-lifecycle-collector/x86_64-unknown-none/release/tos-runtime-image"
after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] ||
    fail "a production artifact changed while building the collector image"

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT-collector" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$collector_image" > "$OUT-collector.log" 2>&1 || {
        cat "$OUT-collector.log" >&2
        fail "the blocked-collector boot did not complete"
    }
log="$(tr -c '[:print:]\n' ' ' < "$OUT-collector/serial.log")"

# The order matters and the log carries it: the collector was waiting before the
# child existed, so what reached it was a delivery and not a poll.
printf '%s\n' "$log" | grep -q 'TOS.RUN.LIFECYCLE.WATCHER waiting=1' ||
    fail "the delegated collector never reached its wait"
printf '%s\n' "$log" | grep -q 'TOS.RUN.LIFECYCLE.PARENT child=0' ||
    fail "the watched parent did not create the child whose ending is the test"
waiting_at="$(printf '%s\n' "$log" | grep -n 'WATCHER waiting=1' | head -1 | cut -d: -f1)"
child_at="$(printf '%s\n' "$log" | grep -n 'PARENT child=0' | head -1 | cut -d: -f1)"
[ "$waiting_at" -lt "$child_at" ] ||
    fail "the collector was not already blocked when the child was created"

collected="$(printf '%s\n' "$log" | sed -n 's/.*WATCHER status=\(-\?[0-9]*\) child=\([0-9]*\).*/\1 \2/p' | tail -1)"
set -- $collected
[ "${1:-}" = "$OK" ] ||
    fail "a blocked delegated collector answered ${1:-nothing}, expected $OK"
[ "${2:-0}" != 0 ] ||
    fail "the delivered record names no child: $collected"

echo "LIFECYCLE PASS: two endings collected in order, identity and generation"
echo "  carried verbatim, stale authority refused, observation requires the right,"
echo "  operation 8 asserts no generation, and an unsatisfiable wait is cancelled"
echo "  ($refusals creation refusal(s) named their bound in the log)"
echo "  and a second boot: a delegated observer cancelled with the relation it"
echo "  watched, and an ending nobody was left to collect released rather than kept;"
echo "  and a third: an ending delivered to a delegated collector already blocked"
echo "  for it, which is the authority being consumptive rather than a broadcast"
