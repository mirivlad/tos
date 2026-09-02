#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The source-level creation operation is withdrawn, and the ABI operations it
# bound to are retired.
#
# This gate used to prove that a TOS Core module could launch a process, through
# `SYSTEM_INTERFACE_V1` §4's `process_create` over `SYSTEM_ABI_V1` operation 8.
# ADR-0076 §4 retires that operation: it funded a process out of the boot's
# accounting anchor with nobody presenting a `MemoryAuthority`, which is the
# ambient spending that decision exists to end. Creation is operation 19 now, and
# it cannot be declared in this schema as it stands — it requires two
# capabilities, an explicit runtime grant and an explicit endowment, and it
# returns the child's *capability* rather than only a status.
#
# **A withdrawal is only real if a module that assumes otherwise cannot run.** So
# the fixture still declares `process_create`, and what this asserts is that the
# module is refused at the boundary check, before its first instruction, with a
# reason naming exactly what is wrong — and that nothing was created.
#
# The other half is the ABI itself: the same boot has a Rust process ask for
# selectors 8 and 15 directly and be answered `E_NOT_SUPPORTED`, which is what
# §7 reserves for an operation of another version of this contract. Their
# numbers stay assigned and are never reused.
#
# What is **not** proved here any more is that a textual supervisor can create a
# process. Nothing can, until the typed bridge exists; that is written up as a
# decision rather than left as an omission, in
# `docs/evidence/STAGE3_CLOSURE_DECISIONS.md`.
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

E_BAD_ARGUMENT=-3
E_LIMIT=-6
E_NOT_SUPPORTED=-7

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
# The supervision probes — the two retired selectors and operation 19's three
# refusals — are evidence, so they are not in the image a canonical boot runs.
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features test-funding-lifecycle)
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
    --runtime-image "$TARGET/x86_64-unknown-none/release/tos-runtime-image" \
    --expect 75 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.DIAGNOSTIC TOS.RUN.REFUSED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.COMPLETED" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- the declaration is refused, by name and by reason -------------------------
# **Before the capability request is even answered.** `TOS.RUN.REQUEST` does not
# appear at all: the boundary check runs at `check`, and a module naming an
# operation the schema does not declare never reaches the point where its
# endowment would be handed to it. The refusal is about the operation's name,
# not about authority — the interface resolves and the capability would have.
[ "$(count '^TOS\.RUN\.DIAGNOSTIC E1801_FFI_NOT_AVAILABLE .*item=process_create reason=the interface declares no operation of this name$')" -ge 1 ] ||
    fail "the withdrawn operation was not refused with the reason that it is not declared"
[ "$(count '^TOS\.RUN\.REFUSED stage=check count=1$')" -ge 1 ] ||
    fail "the module was not refused at the boundary check"
[ "$(count '^TOS\.RUN\.REQUEST ')" = 0 ] ||
    fail "the module reached its capability request despite being refused at check"

# --- the ABI half: both retired selectors, asked and refused -------------------
# By a Rust process holding the very authority the retired operations required,
# so the answer is about the operation rather than about what the caller holds.
[ "$(count "^TOS\.RUN\.PROCESS\.RETIRED create=$E_NOT_SUPPORTED with_generation=$E_NOT_SUPPORTED\$")" = 1 ] ||
    fail "selectors 8 and 15 did not both answer E_NOT_SUPPORTED"

# --- and operation 19 tells its three refusals apart ---------------------------
# An arena no `RuntimeMemoryGrant` could serve and a non-canonical restart record
# are malformed calls; an arena a *particular* authority cannot pay for is a
# bound. A caller told "limit" for the first would retry it forever.
[ "$(count "^TOS\.RUN\.PROCESS\.FUNDING reserved=0 impossible=$E_BAD_ARGUMENT unaffordable=$E_LIMIT malformed=$E_BAD_ARGUMENT distinguished=1\$")" = 1 ] ||
    fail "operation 19 did not distinguish an impossible grant from an unaffordable one"

# --- nothing ran ---------------------------------------------------------------
[ "$(count '^TOS\.RUN\.COMPLETED ')" = 0 ] ||
    fail "the module executed despite declaring an operation the schema does not have"

# --- and the boot says so, by the code it exits with ---------------------------
# A boot module that cannot run is a boot that failed, and the launcher reports
# it as one rather than halting as though the work had been done.
[ "$(count '^TOS\.BOOTMODULE\.FAIL stage=process$')" = 1 ] ||
    fail "the boot did not report its module as having failed"

echo "PROCESS-LAUNCH PASS: the withdrawn creation operation is refused before it runs"
echo "  the interface and the capability resolve; the operation name does not"
echo "  no module executed, and the boot reported the failure rather than halting ok"
echo "  selectors 8 and 15 answered $E_NOT_SUPPORTED; operation 19 told its refusals apart"
