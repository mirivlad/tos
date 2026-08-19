#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A TOS Core module reaches the second interface, and ends its own process.
#
# `module-operation.sh` proves a module can reach `system.ipc.Endpoint`. This one
# reaches `system.process.Control`, and the difference is not repetition:
#
#   - its capability names a **process**, so this is the only boot where the
#     startup kind check of `SYSTEM_INTERFACE_V1` §4 is exercised against a
#     second kind rather than against the one that happens to be first;
#   - `process_terminate` is the one operation of the schema whose effect is
#     visible without the module reporting it, because the module it ends is the
#     one that called it.
#
# **One module text, two launcher constants differing by one bit.** The grant is
# authority over this process either way; what changes is whether it carries
# `terminate`. So the two outcomes cannot be explained by the module, the
# capsule, the runtime image or the nucleus's code — only by the rights mask:
#
#   - without it, the operation is refused and the module returns the status;
#   - with it, the module never returns, and the nucleus says who ended whom.
#
# A module that had faked the call could produce one of those. Not both, from
# one text.
#
#   bash host-tools/qemu-test/process-control.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-process-control}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/process-control"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"

# `SYSTEM_ABI_V1` §4 and `tos-launch`'s object kinds.
E_NO_CAPABILITY=-1
OBJECT_PROCESS=3
# A boot whose module could not finish is not a successful boot; the nucleus
# reports `RESULT_BOOT_MODULE_FAILED` and `isa-debug-exit` returns it shifted and
# set, exactly as `boot-module-failure.sh` computes it.
REFUSED_EXIT=$(( (0x25 << 1) | 1 ))

fail() {
    echo "process-control: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"

# Each isolated build gets its own target directory. Overwriting the production
# nucleus is not a hypothetical: it happened once by hand while `module-operation`
# was being written, and two boots that should have differed produced identical
# logs. The digest check below is what would have caught it.
build() {
    (cd "$ROOT" && CARGO_TARGET_DIR="$ROOT/target/$1" cargo build --release \
        -p tos-nucleus --target x86_64-unknown-none --features "$1")
    echo "$ROOT/target/$1/x86_64-unknown-none/release/tos-nucleus"
}
REFUSING="$(build test-process-control)"
ENDING="$(build test-process-terminate)"
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building isolated test artifacts" >&2
    exit 1
}

# Detached: this fixture is test material rather than committed system source.
printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/process-control.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/process-control.bin" --manifest "$OUT/capsule.meta.json"

# --- the grant is a process, and the startup check accepts it ------------------
# Asserted in both boots, because it is the precondition of either outcome
# meaning anything: a module whose request was denied would never call at all,
# and a kind check that rejected a process would make both boots the same
# refusal.
answered() {
    grep -q \
        "^TOS\\.RUN\\.REQUEST binding=control interface=system\\.process\\.Control object=$OBJECT_PROCESS wanted=$OBJECT_PROCESS\$" \
        "$1/events.log" ||
        fail "$2: the module's request for process authority was not answered by name and kind"
}

# --- without `terminate`: refused, and the module lives to say so --------------
bash "$HERE/run.sh" \
    --out "$OUT/refused" \
    --capsule "$OUT/process-control.bin" \
    --nucleus "$REFUSING" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE" \
    > /dev/null
answered "$OUT/refused" "the refusing boot"
grep -q "^TOS\\.RUN\\.INTERFACE operation=process_terminate status=$E_NO_CAPABILITY\$" \
    "$OUT/refused/events.log" ||
    fail "the operation was not refused by the rights mask"
grep -q "^TOS\\.RUN\\.COMPLETED value=i64:$E_NO_CAPABILITY\$" "$OUT/refused/events.log" ||
    fail "the module did not return the status the system gave it"
# And it was not ended: the process reported its own completion.
grep -q '^TOS\.RUN\.PROCESS_TERMINATED ' "$OUT/refused/events.log" &&
    fail "a process was terminated by a grant that does not carry the right"
grep -q '^TOS\.RUN\.PROCESS_EXIT process=0 asserted_by=nucleus self_reported_status=0 ' \
    "$OUT/refused/events.log" ||
    fail "the module that was refused did not run to its own end"

# A consequence, not noise, and asserted rather than filtered out. `create` is
# one of the two rights a process object has, so the runtime image holding this
# grant takes its supervisor path and makes a child — over the same boot text,
# endowed nothing. The child therefore asks for `control` and nothing answers,
# and is refused at startup naming the binding. That is `module-operation.sh`'s
# deliberate case arriving here on its own, and a boot without it would mean the
# grant this half depends on was not a real process authority.
[ "$(grep -c '^TOS\.RUN\.REFUSED stage=execute reason=capability-denied binding=control interface=system\.process\.Control$' "$OUT/refused/events.log")" = 1 ] ||
    fail "the child this grant let its holder create was not refused for want of the same request"
[ "$(grep -c '^TOS\.RUN\.COMPLETED ' "$OUT/refused/events.log")" = 1 ] ||
    fail "more than one process completed, so the child was not refused after all"

# --- with `terminate`: the module does not come back ---------------------------
bash "$HERE/run.sh" \
    --out "$OUT/ended" \
    --capsule "$OUT/process-control.bin" \
    --nucleus "$ENDING" \
    --expect "$REFUSED_EXIT" \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.PROCESS_TERMINATED" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.COMPLETED" \
    > /dev/null
answered "$OUT/ended" "the ending boot"
# The nucleus names who ended whom, and here they are the same process — which is
# what "authority over a process" means when the process holding it is that one.
grep -q '^TOS\.RUN\.PROCESS_TERMINATED process=0 by=0 ' "$OUT/ended/events.log" ||
    fail "the nucleus did not record this process ending itself"
# And there is **no** status line for the operation, which is the sharper half of
# the evidence. The host reports `TOS.RUN.INTERFACE` after the call returns, so
# its absence says the call did not return — the process was ended inside it. In
# the refusing boot the same line is present with a status, from the same host,
# on the same operation. Two boots, one difference, and it is visible from either
# side.
grep -q '^TOS\.RUN\.INTERFACE operation=process_terminate ' "$OUT/ended/events.log" &&
    fail "the operation returned a status, so the process it named was not ended by it"

echo "PROCESS-CONTROL PASS: a module reached system.process.Control"
echo "  its request was answered with a process object, and the kind check accepted it"
echo "  without \`terminate\` the operation returned $E_NO_CAPABILITY and the module returned it"
echo "  with it, the module never returned and the nucleus recorded process 0 ending itself"
