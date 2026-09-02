#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A TOS Core module acts on authority it was given **at runtime** (ADR-0078).
#
# `capability_source.rs` proves the representation, the image and the verifier.
# This is the last link the Project Architect's item 3 asks for: **the ABI**.
# Every status below is one the real nucleus produced, over a real process, on a
# real machine.
#
# Two modules, both canonical text. `/system/boot/init.tos` is the supervisor;
# `/system/boot/worker.tos` is what it creates, and asks for nothing, so that
# what is under test is the supervisor's authority and not a second question in
# the same boot.
#
# The chain, and why each link could not have been an import:
#
#   16 capability_attenuate_scoped  a **scoped budget** out of the root's
#                                   remainder. It did not exist when this
#                                   process started, so no `import capability`
#                                   could have answered for it
#   21 launch_plan_create           a builder, likewise
#   22 endow_for_launch             reached **through the scoped budget** — the
#                                   operation's own capability is a value
#   23 launch_plan_seal             the builder, at a non-first position
#   19 process_create_funded        funded from the scoped budget at its
#                                   **second** capability position, which is
#                                   what makes this a proof about every
#                                   position rather than only the first
#   5  capability_attenuate         the child's authority, refined
#   9  process_terminate            the refined child authority as the
#                                   operation's own capability — the case that
#                                   was unrepresentable before ADR-0078
#   6  capability_release           and let go
#
# The launcher's constant grants exactly two things: `create | terminate` over
# this process, and the root's remainder to spend. Nothing else in the run was
# granted by anybody; everything else was produced.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-runtime-authority}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/runtime-authority"
TOOL="$ROOT/target/release/tos-capsule-tool"
TARGET="$ROOT/target/test-runtime-authority"

OK=0
# `process_terminate` and `capability_release` both answered `OK`, plus one.
# Composed so that no single answer produces it.
EXPECTED_VALUE="i64:1"

fail() {
    echo "runtime-authority: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-runtime-authority)

printf '/system/boot/init.tos\t%s/init.tos\n/system/boot/worker.tos\t%s/worker.tos\n' \
    "$FIXTURE" "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/supervisor.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/supervisor.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/supervisor.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.NUCLEUS.INVARIANT" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

exactly() {
    local want="$1" pattern="$2" what="$3"
    local saw
    saw="$(count "$pattern")"
    [ "$saw" = "$want" ] ||
        fail "$what: saw $saw line(s) matching '$pattern', expected $want"
}

# --- the launcher granted two things, and the module asked for both ------------
exactly 1 '^TOS\.RUN\.REQUEST binding=process interface=system\.process\.Control object=3 wanted=3$' \
    "the supervisor's request for process authority was not answered by name and kind"
exactly 1 '^TOS\.RUN\.REQUEST binding=memory interface=system\.memory\.Authority object=6 wanted=6$' \
    "the supervisor's request for a memory authority was not answered by name and kind"
# And nothing else was granted. Everything below is authority the run produced.
[ "$(count '^TOS\.RUN\.REQUEST ')" = 2 ] ||
    fail "the supervisor was granted something it did not ask for"

# --- every operation of the chain reached the nucleus and was answered ---------
for operation in capability_attenuate_scoped launch_plan_create endow_for_launch \
    launch_plan_seal process_create_funded capability_attenuate process_terminate \
    capability_release; do
    exactly 1 "^TOS\\.RUN\\.INTERFACE operation=$operation status=$OK\$" \
        "$operation did not reach the nucleus and answer OK"
done

# --- the child really was created, funded and ended ---------------------------
# The nucleus's own lines, not the module's: a supervisor cannot see the process
# table, and these are ring 0 saying what it did.
grep -q '^TOS\.RUN\.PROCESS_CHARGE .* grant=56623104 ' "$LOG" ||
    fail "no child was charged the runtime grant the module named"
grep -q '^TOS\.RUN\.PROCESS_TERMINATED process=1 by=0 ' "$LOG" ||
    fail "the child was not ended by the process that created it"

# --- and the plan table is empty at the end -----------------------------------
# A plan is destroyed by the loss of the one capability naming it, and a boot
# that ended holding one has leaked a decision (ADR-0077 §6).
grep -q '^TOS\.RUN\.PROCESS_RECLAIMED process=0 .* plans_live=0$' "$LOG" ||
    fail "a launch plan outlived the process that made it"

# --- the number the module returned -------------------------------------------
# `process_terminate` and `capability_release` both OK, plus one. Every earlier
# step has its own arm returning a distinct negative, so this value is reachable
# only when all eight succeeded.
exactly 1 "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$" \
    "the supervisor did not complete the whole chain"

echo "RUNTIME-AUTHORITY PASS: a module acted on authority no import answered for"
echo "  two grants at startup: create|terminate over itself, and the root's remainder"
echo "  a scoped budget, a plan, a child and a refined child authority — all produced"
echo "  22 reached through the scoped budget; 19 funded from it at its second position"
echo "  9 acted on the refined child authority as its own capability, which is the"
echo "  case tos-ir/v1 could not represent before ADR-0078"
echo "  and every plan was destroyed with the process that made it"
