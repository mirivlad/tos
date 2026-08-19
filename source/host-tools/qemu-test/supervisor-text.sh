#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A supervisor written in TOS Core starts what a policy module names.
#
# `docs/37`'s Stage 3 identity question is "do textual processes exercise real
# capability/IPC contracts rather than running as decorative scripts around
# privileged binary services?" `module-operation.sh` answers it for one
# operation and `process-launch.sh` for one launch. This answers it for the
# system's own structure: **the thing that starts the services is text**, and so
# is the thing that says which services those are.
#
# Two modules, both canonical text, both in this capsule:
#
#   - `/system/policy/services.tos` says *what* to start. ADR-0051 §3 puts
#     supervision policy in `/system/policy/` as "canonical source keyed by
#     module name … canonical text like any other component; not a binary
#     configuration database", and a TOS Core module is the most literal reading
#     of that available.
#   - `/system/boot/init.tos` says *how*: it reads the policy's count, reads each
#     name, and calls `process_create` on the capability it asked for by name.
#
# They are separate on purpose. A supervisor carrying its own workload would be
# a component that grants itself what to do, which is the shape docs/37 names as
# a failure.
#
# **The evidence is a number neither module contains.** The policy names two
# components, one of which this capsule carries and one of which it does not, so
# exactly one starts. A supervisor that had ignored the policy could not have
# looped twice; one that had faked the calls could not have got two different
# answers; and `1` appears in neither module's text.
#
#   bash host-tools/qemu-test/supervisor-text.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-supervisor-text}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/supervisor"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-process-launch"

OK=0
E_BAD_ARGUMENT=-3
# One of the policy's two components started.
EXPECTED_VALUE="i64:1"

fail() {
    echo "supervisor-text: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
# The same launcher constant `process-launch.sh` uses: authority over this
# process carrying `create`, under the name a module asks for it. Nothing about
# the nucleus is specific to a supervisor — what makes this one is the text.
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-process-launch)
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
    exit 1
}

printf '/system/boot/init.tos\t%s/init.tos\n/system/policy/services.tos\t%s/services.tos\n' \
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
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- the capsule really is two modules, resolved as a set ----------------------
# Without this the supervisor could be one module pretending, and the policy's
# separateness — the whole point — would be unevidenced.
grep -q '^TOS\.RUN\.BEGIN .* modules=2$' "$LOG" ||
    fail "the boot did not run a set of two modules"

# --- the supervisor asked for the authority it uses, by name -------------------
[ "$(count '^TOS\.RUN\.REQUEST binding=control interface=system\.process\.Control object=3 wanted=3$')" = 1 ] ||
    fail "the supervisor's request for process authority was not answered by name and kind"

# --- it looped over the policy's count, not its own ----------------------------
# Two calls means `policy.count()` returned two, and that number is in the policy
# module alone. One call, or three, would mean the supervisor was not reading it.
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_create ')" = 2 ] ||
    fail "the supervisor did not make one call per component the policy names"

# --- and used the policy's names --------------------------------------------
# One name this capsule carries and one it does not. The pair is what shows the
# names came from somewhere: a supervisor with a name of its own would produce
# two of the same answer.
[ "$(count "^TOS\\.RUN\\.INTERFACE operation=process_create status=$OK\$")" = 1 ] ||
    fail "the component the capsule carries was not started"
[ "$(count "^TOS\\.RUN\\.INTERFACE operation=process_create status=$E_BAD_ARGUMENT\$")" = 1 ] ||
    fail "the component the capsule does not carry was not refused"

# --- the number it returns is in neither module --------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$")" = 1 ] ||
    fail "the supervisor did not report how many of the policy's components started"

# --- and the one that started is a process, not a status -----------------------
[ "$(count '^TOS\.RUN\.PROCESS_EXIT process=1 ')" = 1 ] ||
    fail "no second process ran, so the successful call started nothing"

echo "SUPERVISOR-TEXT PASS: the thing that starts the services is text"
echo "  a policy module named two components; the supervisor read its count and its names"
echo "  one started ($OK) and one was refused ($E_BAD_ARGUMENT); it reported $EXPECTED_VALUE"
echo "  a number neither module contains, and a process the second one ran as"
