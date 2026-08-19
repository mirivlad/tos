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

echo "MODULE-OPERATION PASS: a TOS Core module performed a real operation"
echo "  its request was answered by name; endpoint_send returned $OK and"
echo "  endpoint_receive returned $E_NO_CAPABILITY, on the same capability"
