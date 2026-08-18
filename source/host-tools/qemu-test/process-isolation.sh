#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A fault in a process ends that process; the system keeps running.
#
# ADR-0049 section 3 draws the line and ADR-0048 asks for the evidence: a fault
# at CPL 3 must terminate exactly one process and leave the system running,
# while the same fault at CPL 0 still ends the boot. This gate is the first
# half, and it is deliberately not satisfied by the fault alone — the boot has
# to go on afterwards and produce the ordinary result, which is the part that
# says "the system survived".
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCENARIO="${1:-abi}"

case "$SCENARIO" in
    # The payload of SYSTEM_ABI_V1 section 8.6, run at CPL 3: it fills every
    # register the contract says survives a call, calls an unassigned operation,
    # checks that the status is E_NOT_SUPPORTED and that nothing moved, calls
    # `context_yield`, and only then executes UD2. Vector 6 at a RIP inside the
    # user code page is therefore the whole assertion: reaching that instruction
    # required every check to pass, and reaching it *there* required CPL 3.
    abi)
        FEATURE=test-ring3-abi
        VECTOR=6
        RIP='0x0000000040000[0-9a-f][0-9a-f][0-9a-f]'
        ;;
    # A process executing a privileged instruction. HLT at CPL 3 is a general
    # protection fault — refused by the processor, not by anything the nucleus
    # checked, and not by anything the verifier accepted.
    privileged)
        FEATURE=test-ring3-privileged
        VECTOR=13
        RIP='0x0000000040000000'
        ;;
    *)
        echo "usage: $0 {abi|privileged}" >&2
        exit 2
        ;;
esac

PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-process-$SCENARIO"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
OUT="$ROOT/target/qemu-process-$SCENARIO"

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

# Exit 33, not 73: the fault below is not the end of the boot.
bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PROCESS_FAULT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC"

grep -Eq "^TOS\.RUN\.PROCESS_FAULT vector=$VECTOR error=0x[0-9a-f]+ rip=$RIP cr2=[^ ]+ cpl=3$" \
    "$OUT/events.log" || {
    echo "missing the process-fault event for vector $VECTOR at CPL 3" >&2
    exit 1
}
# The system did not merely survive the fault: it went on to run the real first
# process to completion afterwards.
grep -Eq '^TOS\.RUN\.COMPLETED value=i32:240$' "$OUT/events.log" || {
    echo "the boot did not complete its own work after the process fault" >&2
    exit 1
}
echo "process-isolation-$SCENARIO: PASS"
