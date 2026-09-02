#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A process creates a process, and ends it, on authority it was given.
#
# ADR-0055 puts the root of authority in the launcher and makes every grant
# after it an attenuation of something the grantor already held. This gate is
# that chain, exercised end to end by a real process:
#
#   - the launcher endows one process with authority over **itself**, carrying
#     the two rights a process object has. That capability is the one nobody but
#     a launcher can issue, because it names a process that does not exist until
#     the instant it is granted;
#   - the process exercises it: `process_create` (8) builds a child, and the
#     caller receives authority over what it made, carrying exactly the rights
#     the authority it used carried — never more;
#   - `process_terminate` (9) ends the child, and the nucleus records who ended
#     it. That is the third way a process can end, and it is neither the child's
#     own claim nor the architecture's: it is another party's decision, so the
#     record names the party;
#   - the same handle over the now-dead child refuses. A capability's lifetime
#     is bounded by its object (`CAPABILITY_V1` §3), so it does not survive to
#     name whoever occupies that slot next;
#   - the module is named by **path**, not by an ordinal: an ordinal is a
#     position in a list nobody published, and two boots whose capsules differ
#     would give the same one to different modules. A name the set does not hold
#     is refused rather than matched to something near it;
#   - an endowment naming a capability the parent does not hold refuses the whole
#     creation, because a child half-endowed would hold authority nobody decided
#     to give it.
#
# The child is endowed with what its parent decided and no more: the parent holds
# `create` and `terminate`, and gives its child only the second.
#
#   bash host-tools/qemu-test/supervisor.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-supervisor}"
FEATURE=test-supervisor
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-supervisor"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# `SYSTEM_ABI_V1` §4 statuses, by the numbers the contract assigns.
OK=0
E_NO_CAPABILITY=-1
E_BAD_ARGUMENT=-3
E_BAD_HANDLE=-2
E_LIMIT=-6
E_NOT_SUPPORTED=-7
# `RIGHT_CREATE | RIGHT_TERMINATE`, and `OBJECT_PROCESS`.
RIGHTS=24
OBJECT=3

fail() {
    echo "supervisor: FAIL: $*" >&2
    exit 1
}

[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features "$FEATURE")
# The image is a feature build too. The funding-lifecycle probes create and end
# processes, which is evidence rather than anything a canonical boot does, and
# evidence compiled into the image every process runs is memory every process
# pays for.
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features test-funding-lifecycle)
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building isolated test artifact" >&2
    exit 1
}

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image" \
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

# --- the launcher put the root of the chain in exactly one place -------------
exactly 1 "^TOS\\.RUN\\.PROCESS_ENDOWED process=0 capabilities=2 policy=launcher-constant asserted_by=launcher\$" \
    "the launcher did not endow the first process with authority over itself and a memory authority"
exactly 1 "^TOS\\.RUN\\.CAPABILITY held=2 handle=0x[0-9a-f]* object=$OBJECT rights=$RIGHTS binding=self\$" \
    "the first process does not hold authority over a process with both rights"

# --- it created a child, and the child was given nothing ---------------------
exactly 1 '^TOS\.RUN\.PROCESS\.CREATED status=0 child=0x[0-9a-f]*$' \
    "the process did not create a child on its own authority"
exactly 1 "^TOS\\.RUN\\.PROCESS_ENDOWED process=1 capabilities=1 policy=launcher-constant asserted_by=launcher\$" \
    "the child was not endowed with exactly what its parent decided"

# --- and what it was given is less than its parent held ----------------------
# The parent holds `create` and `terminate` (24); the child is given only the
# second. Attenuation at the moment of creation, not after it — and a parent
# cannot give what it does not hold, which is the same rule one level down.
exactly 1 "^TOS\\.RUN\\.PROCESS\\.REFUSED reason=endowment-not-held status=$E_BAD_HANDLE\$" \
    "an endowment naming a capability the parent does not hold was not refused"
# The whole creation failed: a child half-endowed would hold authority nobody
# decided to give it. Four processes were announced — this one and the three the
# funding-lifecycle evidence below creates — and the refused creation is not one
# of them.
exactly 4 '^TOS\.RUN\.PROCESS_ENDOWED ' \
    "a refused creation left a process behind"

# --- and ended it, attributably ----------------------------------------------
exactly 1 '^TOS\.RUN\.PROCESS_TERMINATED process=1 by=0 ticks=[0-9]* quanta=[0-9]* asserted_by=nucleus$' \
    "the nucleus did not record the child being ended by the process that held authority over it"
# A capability's lifetime is bounded by its object: the handle still resolves,
# and what it named is gone.
exactly 1 "^TOS\\.RUN\\.PROCESS\\.ENDED status=0 again=$E_NO_CAPABILITY\$" \
    "ending the child failed, or its handle still named something afterwards"

# --- a module this boot does not have is refused, not clamped ----------------
exactly 1 "^TOS\\.RUN\\.PROCESS\\.REFUSED reason=no-such-module status=$E_BAD_ARGUMENT\$" \
    "a module name the source set does not hold was not refused"

# --- the child never finished, and the supervisor did ------------------------
# One completion, not two: the child was ended long before a run of its own
# could finish, which is what "ended by authority" has to mean.
exactly 1 '^TOS\.RUN\.COMPLETED value=i32:240$' \
    "the wrong number of runs completed"
# Every process gave its memory back — the supervisor and each of the three
# children it created.
exactly 4 '^TOS\.RUN\.PROCESS_RECLAIMED ' \
    "not every process returned its memory"


# --- the two retired creation operations answer E_NOT_SUPPORTED ----------------
# Asked by a process holding the very authority they used to require, so the
# refusal is about the operation rather than about what the caller holds
# (ADR-0076 §4, `SYSTEM_ABI_V1` §7). Their numbers stay assigned and are never
# reused.
exactly 1 "^TOS\\.RUN\\.PROCESS\\.RETIRED create=$E_NOT_SUPPORTED with_generation=$E_NOT_SUPPORTED\$" \
    "selectors 8 and 15 did not both answer E_NOT_SUPPORTED"

# --- operation 19 tells its three refusals apart --------------------------------
# An arena no `RuntimeMemoryGrant` could serve and a non-canonical restart record
# are malformed calls; an arena a *particular* authority cannot pay for is a
# bound (ADR-0076 §7). The unaffordable one is funded from a megabyte reserved
# out of what this process holds, so the same request through the parent
# authority succeeding a moment earlier is what says the charge follows the
# authority that was presented.
exactly 1 "^TOS\\.RUN\\.PROCESS\\.FUNDING reserved=$OK impossible=$E_BAD_ARGUMENT unaffordable=$E_LIMIT malformed=$E_BAD_ARGUMENT distinguished=1\$" \
    "operation 19 did not distinguish an impossible grant from an unaffordable one"

# --- and the funding lifecycle holds over the whole life of a child ------------
#   sealed           one plan, made and sealed once, and used for all four
#                    creations below. A creation does not consume it, so the
#                    fourth is the same decision as the first (ADR-0077 §5)
#   reserved/first   a child funded from a reserved authority, endowed a name
#                    for that same authority — two names, one budget
#   still_held       the creation placed a charge and did not consume the
#                    capability: the creator can still spend through it
#   second           and cannot fund a second child, because the first one's
#                    bytes are spent rather than promised
#   again            the child ends, is retired, and the same request works
#                    again: the exact charge came back to the node that paid
#   released/stale   the creator lets go of its own handle for that node while a
#                    child it funded is still running, and that handle stops
#                    resolving
#   held_by_plan     with the parent drained to less than one reservation, it
#                    still cannot reserve: the handle was not the last name,
#                    because the plan took one of its own when the entry was
#                    written (ADR-0077 §3)
#   returned         and releasing the plan — the loss of the **last** name —
#                    sends the bytes up the lineage, so the parent can reserve
#                    that amount once more where a moment ago it could not.
#                    The pair is the evidence: the same request, refused and
#                    then granted, with only the plan's release in between
#
# That is the claim in one line: process funding is an allocation held by the
# accounting, not by the continued existence of the funding capability.
exactly 1 "^TOS\\.RUN\\.PROCESS\\.LIFECYCLE sealed=$OK reserved=$OK first=$OK still_held=$OK second=$E_LIMIT again=$OK released=$OK stale=$E_NO_CAPABILITY held_by_plan=$E_LIMIT returned=$OK\$" \
    "the funding lifecycle did not hold across a child's whole life"

echo "SUPERVISOR PASS: a process created a process and ended it, on authority it was given"
echo "  the launcher endowed one process with authority over itself; nothing else could have"
echo "  the child was given only what its parent chose of what its parent held"
echo "  an endowment naming a capability the parent lacks refused the whole creation"
echo "  the handle over the dead child refused afterwards; an unknown module name was refused"
echo "  selectors 8 and 15 answered $E_NOT_SUPPORTED; 19 told its three refusals apart"
echo "  and the funding charge outlived the capability that placed it, then came back"
echo "  one sealed launch plan served every creation, and none of them consumed it"
echo "  the same reservation was refused and then granted with only the plan's"
echo "  release in between: the plan, not the handle, was holding that authority"
