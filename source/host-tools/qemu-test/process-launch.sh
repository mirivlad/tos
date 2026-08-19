#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A TOS Core module launches a process.
#
# This is the act a supervisor is made of, and until now no module could perform
# it. `SYSTEM_INTERFACE_V1` §4 left `process_create` out with a reason: it takes
# a module name, "and TOS Core V1 has no way to write into that region, because
# it has no pointers and this schema admits none".
#
# §4.1 answers that without contradicting a word of it. The name is a **value**,
# which §3 already said an operation's parameters after the capability are, and
# the host places its bytes where the ABI already reads them — at an offset the
# nucleus chose and mapped (ADR-0058). The module names a value and never an
# address; the nucleus still walks nothing a process picked. What is new is a
# declared maximum, because `SYSTEM_ABI_V1` §3 bounds every read by a constant of
# the contract rather than by a number a caller chose.
#
# **The evidence is the difference between two answers.** One module of this
# capsule exists and one does not; both names go through the same operation, on
# the same capability, under the same authority. A module that had faked either
# could not produce two. And the value it returns is composed so that neither
# answer alone yields it: refusing both gives 0, accepting both gives 0, and only
# the declared pair gives 3.
#
#   bash host-tools/qemu-test/process-launch.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-process-launch}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/process-launch"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-process-launch"

OK=0
E_BAD_ARGUMENT=-3
# `OK - E_BAD_ARGUMENT`, which no single answer produces.
EXPECTED_VALUE="i64:3"

fail() {
    echo "process-launch: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-process-launch)
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
    exit 1
}

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/process-launch.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/process-launch.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/process-launch.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- the module asked for process authority, and got it ------------------------
[ "$(count '^TOS\.RUN\.REQUEST binding=control interface=system\.process\.Control object=3 wanted=3$')" = 1 ] ||
    fail "the module's request for process authority was not answered by name and kind"

# --- one name made a process, and one did not ---------------------------------
# Same operation, same capability, same authority; only the value differs. That
# is what makes the pair evidence about the *argument* having crossed rather than
# about the call having been made.
[ "$(count "^TOS\\.RUN\\.INTERFACE operation=process_create status=$OK\$")" = 1 ] ||
    fail "naming a module the capsule carries did not create a process"
[ "$(count "^TOS\\.RUN\\.INTERFACE operation=process_create status=$E_BAD_ARGUMENT\$")" = 1 ] ||
    fail "naming a module the capsule does not carry was not refused"

# --- and the module computed from what came back -------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$")" = 1 ] ||
    fail "the module did not return the value only the declared pair of answers gives"

# --- the child is real ---------------------------------------------------------
# A status is a claim; a process that ran and ended is the thing itself. The
# child is endowed nothing (§4 declares no endowment), so it asks for `control`,
# nothing answers, and it is refused at startup — which is exactly what a child
# that can do nothing looks like from outside.
[ "$(count '^TOS\.RUN\.PROCESS_EXIT process=1 ')" = 1 ] ||
    fail "no second process ran, so the successful call created nothing"
[ "$(count '^TOS\.RUN\.REFUSED stage=execute reason=capability-denied binding=control interface=system\.process\.Control$')" -ge 1 ] ||
    fail "the child was endowed something, or did not report being endowed nothing"

echo "PROCESS-LAUNCH PASS: a TOS Core module created a process"
echo "  its module name crossed as a value, at the offset the ABI already reads"
echo "  a name the capsule carries returned $OK; one it does not, $E_BAD_ARGUMENT"
echo "  and the child ran, endowed nothing, exactly as section 4 declares"
