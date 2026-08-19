#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A TOS Core module performs a real operation on a real capability.
#
# `docs/37` asks the Stage 3 identity question in one sentence: "do textual
# processes exercise real capability/IPC contracts rather than running as
# decorative scripts around privileged binary services?" Until this gate the
# honest answer was **no**. Everything Phase 4 built — endowment, delegation,
# request and reply, the confused deputy — was exercised by the Rust runtime
# image, which is a privileged binary; the textual module computed a number.
#
# The boot text here is a `.tos` module that asks for a capability, is answered
# by name (ADR-0061), and calls two operations of `SYSTEM_INTERFACE_V1` §4 on
# it. `profile full`, and it has to be — `docs/42` §3 forbids `extern` in
# Bootstrap, which is why the canonical Bootstrap boot text can never do this and
# why this is a capsule of its own rather than a change to `system/boot/init.tos`.
#
# **The evidence is the difference between two answers, not either one.** The
# endowment carries `send` and not `receive`, so the first operation is performed
# and the second is refused — by the nucleus, against a table the module cannot
# address, on authority the module was given rather than authority it named. A
# module that had faked either could not have produced two different statuses;
# and the module's own returned value is composed so that neither status alone
# yields it (`0 - (-1) = 1`; refusing both or allowing both gives 0).
#
#   bash host-tools/qemu-test/module-operation.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-module-operation}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FEATURE=test-module-operation
FIXTURE="$ROOT/tests/vectors/interface-operation"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-module-operation"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# A boot whose module could not start is not a successful boot: the nucleus
# reports `RESULT_BOOT_MODULE_FAILED`, and `isa-debug-exit` returns it shifted
# and set, exactly as `boot-module-failure.sh` computes it. Stated here so that
# the two refusing boots below are *expected* to fail rather than tolerated.
REFUSED_EXIT=$(( (0x25 << 1) | 1 ))

# `SYSTEM_ABI_V1` §4, as the module sees them.
OK=0
E_NO_CAPABILITY=-1
# What the module returns: `OK - E_NO_CAPABILITY`.
EXPECTED_VALUE="i64:1"

fail() {
    echo "module-operation: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
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

# Detached: this fixture is test material rather than committed system source,
# so it carries a whole-tree digest rather than a Git object identity.
printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/module-operation.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/module-operation.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/module-operation.bin" \
    --nucleus "$TEST_NUCLEUS" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.REFUSED"

LOG="$OUT/events.log"

count() { grep -c "$1" "$LOG" || true; }
exactly() {
    local seen
    seen=$(count "$2")
    [ "$seen" = "$1" ] || fail "$3: saw $seen line(s) matching '$2', expected $1"
}

# --- the request was answered by name, and the kind was checked ----------------
# `binding=endpoint` on both sides is the arrow ADR-0061 draws: the module asked
# under that name and the launcher's grant answers that name. `object=wanted` is
# `SYSTEM_INTERFACE_V1` §4's kind check, which is what would refuse a grant of a
# process where an endpoint was asked for.
exactly 1 '^TOS\.RUN\.REQUEST binding=endpoint interface=system\.ipc\.Endpoint object=1 wanted=1$' \
    "the module's capability request was not answered by the name it was bound to"

# --- and both operations really reached the nucleus ----------------------------
# Performed, then refused, on the same capability. Two statuses from one grant is
# what makes this a system judging authority rather than a script printing.
exactly 1 "^TOS\\.RUN\\.INTERFACE operation=endpoint_send status=$OK\$" \
    "the operation the endowment permits was not performed"
exactly 1 "^TOS\\.RUN\\.INTERFACE operation=endpoint_receive status=$E_NO_CAPABILITY\$" \
    "the operation the endowment withholds was not refused"

# --- in that order -------------------------------------------------------------
# `docs/40`'s evaluation order is the module's, and ADR-0060 keeps it
# deterministic across the boundary: the module wrote `send` first.
sent=$(grep -n '^TOS\.RUN\.INTERFACE operation=endpoint_send ' "$LOG" | head -1 | cut -d: -f1)
taken=$(grep -n '^TOS\.RUN\.INTERFACE operation=endpoint_receive ' "$LOG" | head -1 | cut -d: -f1)
[ -n "$sent" ] && [ -n "$taken" ] && [ "$sent" -lt "$taken" ] ||
    fail "the two operations did not reach the system in the order the module wrote them"

# --- and the module computed from what came back -------------------------------
# Neither status alone yields this: refusing both or allowing both gives 0.
exactly 1 "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$" \
    "the module did not return the value only the declared pair of statuses gives"

# --- the module is the one that reached, and it is verified text ---------------
exactly 1 '^TOS\.RUN\.VERIFIED module=system\.boot\.init digest=sha256:[0-9a-f]* verifier=.*$' \
    "the module that reached the system was not the verified one"

# --- and the two ways a request is *not* answered ------------------------------
# ADR-0061's evidence list, items 3 and 4. Both are startup refusals: a module
# that got as far as a call before discovering it holds nothing would already
# have done work under an assumption that was false, and
# `SYSTEM_INTERFACE_V1` §10.3 says it "never reaches the call".
#
# The same capsule each time. Only the launcher's constant differs, which is
# what makes these three boots one experiment rather than three.

# The refusal is the same in both, and that is the trap: two boots whose logs
# agree line for line are one piece of evidence, not two. So each also states
# *how far the request got* — nobody answered it at all, or somebody answered it
# with the wrong object — and the gate checks that they differ there. Written
# after building the wrong-kind nucleus over the production one by hand and
# watching both boots produce identical logs.
denied() {
    local what=$1 nucleus=$2 out=$3 answered=$4
    bash "$HERE/run.sh" \
        --out "$out" \
        --capsule "$OUT/module-operation.bin" \
        --nucleus "$nucleus" \
        --expect "$REFUSED_EXIT" \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.REFUSED" \
        --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.INTERFACE TOS.RUN.COMPLETED" \
        > /dev/null
    grep -q \
        '^TOS\.RUN\.REFUSED stage=execute reason=capability-denied binding=endpoint interface=system\.ipc\.Endpoint$' \
        "$out/events.log" ||
        fail "$what: the refusal did not name the denied request by its binding"
    local seen
    seen=$(grep -c '^TOS\.RUN\.REQUEST ' "$out/events.log" || true)
    [ "$seen" = "$answered" ] ||
        fail "$what: $seen grant(s) were offered for the request, expected $answered"
}

# 3. Nothing answers the request. The production nucleus's constant grants
#    nothing, because `system.boot.init` normally asks for nothing — so this is
#    the ordinary launcher meeting a module that asks, with no feature involved.
# Nothing is offered at all, so there is no `TOS.RUN.REQUEST` line: the host
# had nothing whose name matched.
denied "a request nobody answered" "$PRODUCTION" "$OUT/denied" 0

# 4. Something answers it with the wrong kind of object: authority over a
#    process, under the name the module asks for an endpoint under. The name
#    matches; the kind does not. Refused at startup rather than at the first
#    call, which is the whole reason §4 has an object column.
(cd "$ROOT" && CARGO_TARGET_DIR="$ROOT/target/test-wrong-kind" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-wrong-kind)
# Something *is* offered — the name matched — and the line says which object it
# was and which was wanted, which is why this refusal is a different fact from
# the one above rather than the same one twice.
denied "a grant of the wrong kind" \
    "$ROOT/target/test-wrong-kind/x86_64-unknown-none/release/tos-nucleus" \
    "$OUT/wrong-kind" 1
grep -q '^TOS\.RUN\.REQUEST binding=endpoint interface=system\.ipc\.Endpoint object=3 wanted=1$' \
    "$OUT/wrong-kind/events.log" ||
    fail "the wrong-kind boot did not offer a process where an endpoint was asked for"

echo "MODULE-OPERATION PASS: a TOS Core module performed a real operation"
echo "  its request was answered by name; endpoint_send returned $OK and"
echo "  endpoint_receive returned $E_NO_CAPABILITY, on the same capability"
echo "  the same module, unanswered, is refused at startup and the refusal names the binding"
echo "  and so is one answered with a process where it asked for an endpoint"
