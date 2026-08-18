#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Verify the Stage 1 fatal CPU-exception path using an isolated nucleus build.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCENARIO="${1:-ud2}"

case "$SCENARIO" in
    ud2)
        FEATURE=test-exception-ud2
        VECTOR=6
        ERROR=0x0000000000000000
        CR2=none
        ;;
    gp)
        FEATURE=test-exception-gp
        VECTOR=13
        ERROR=0x0000000000000402
        CR2=none
        ;;
    # A read of the page the nucleus's own address space deliberately leaves
    # absent. Unlike the two above it is injected *after* the substrate exists,
    # because what it proves is a property of the tables the nucleus built: an
    # unmapped page faults, and the fault is the one the architecture defines
    # for a supervisor read of a non-present page.
    paging)
        FEATURE=test-paging-unmapped
        VECTOR=14
        ERROR=0x0000000000000000
        CR2=0x0000000000000000
        ;;
    # A store into the nucleus's own text. What it proves is the conjunction of
    # the read-only mapping and `CR0.WP`: with either one missing the store
    # succeeds silently, which is exactly the failure a table dump cannot see.
    readonly-text)
        FEATURE=test-paging-readonly-text
        VECTOR=14
        ERROR=0x0000000000000003
        CR2=0x0000000002000000
        ;;
    # The payload of SYSTEM_ABI_V1 section 8.6, run at CPL 3: it fills every
    # register the contract says survives a call, calls an unassigned operation,
    # checks that the status is E_NOT_SUPPORTED and that nothing moved, calls
    # `context_yield`, and only then executes UD2. Vector 6 at a RIP inside the
    # user code page is therefore the whole assertion: reaching that instruction
    # required every check to pass, and reaching it *there* required CPL 3.
    ring3)
        FEATURE=test-ring3-abi
        VECTOR=6
        ERROR=0x0000000000000000
        CR2=none
        RIP='0x0000000040000[0-9a-f][0-9a-f][0-9a-f]'
        ;;
    # A process executing a privileged instruction. HLT at CPL 3 is a general
    # protection fault with a null error code — refused by the processor, not by
    # anything the nucleus checked.
    ring3-privileged)
        FEATURE=test-ring3-privileged
        VECTOR=13
        ERROR=0x0000000000000000
        CR2=none
        RIP='0x0000000040000000'
        ;;
    *)
        echo "usage: $0 {ud2|gp|paging|readonly-text|ring3|ring3-privileged}" >&2
        exit 2
        ;;
esac

PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-exception-$SCENARIO"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
OUT="$ROOT/target/qemu-exception-$SCENARIO"

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
    --expect 73 \
    --require "TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.EXCEPTION" \
    --forbid "TOS.HALT TOS.PANIC"

grep -Eq "^TOS\.EXCEPTION vector=$VECTOR error=$ERROR rip=${RIP:-0x[0-9a-f]+} cr2=$CR2$" "$OUT/events.log" || {
    echo "missing canonical exception event for vector $VECTOR" >&2
    exit 1
}
echo "exception-injection-$SCENARIO: PASS"
