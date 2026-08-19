#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The nucleus begins with the flags it chose, not the ones it found.
#
# An interrupt gate clears `IF`, `TF`, `NT`, `RF` and `VM`, and leaves `DF`
# exactly as the interrupted context had it. A process is entitled to set `DF`,
# and every Rust program on this system already does: `memmove` sets it to copy
# overlapping bytes backwards and clears it a dozen instructions later. So a
# timer tick that lands inside that window enters the nucleus with `DF` set —
# and the nucleus's handlers are Rust, compiled on the System V AMD64 promise
# that `DF` is clear, including the `memcpy` the compiler emits for the frame
# copies in `preempt`.
#
# That is not a hypothesis. It happened:
#
#   TOS.EXCEPTION vector=14 error=0x11 rip=0x08023000 cr2=0x08023000
#                 cs=0x08 rsp=0x02013738
#
# `rep movsq` ran backwards, writing the 160 bytes *below* the frame instead of
# into it — over the return address the stub had pushed one instruction earlier —
# and the `ret` left for whatever those bytes said. It reproduced roughly once in
# thirty boots of the deputy gate, which is exactly often enough to be dismissed
# as a flake.
#
# This gate removes the luck from both halves:
#
#   - The **process** holds `DF` across hundreds of millions of instructions
#     rather than a dozen, so ticks land inside the window every time. The window
#     is one assembly block making no memory reference, so the process itself is
#     unaffected by the flag it is holding — a boot that fails here fails in the
#     nucleus, which is the only place this gate makes a claim about.
#   - The **nucleus** runs two processes, so a tick that arrives during one
#     reaches the frame copies that switch to the other. With one process
#     `preempt` returns before copying anything and the hostile flag meets
#     nothing that reads it.
#
#   bash host-tools/qemu-test/direction-flag.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-direction-flag}"
NUCLEUS_FEATURE=test-two-processes
IMAGE_FEATURE=test-direction-flag

PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TEST_TARGET="$ROOT/target/test-direction-flag"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
TEST_IMAGE="$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image"

# The value `system/boot/init.tos` computes: 1*2 + 2*3 + ... + 8*9.
EXPECTED_VALUE="i32:240"
# How many processes this boot builds.
PROCESSES=2

fail() {
    echo "direction-flag: FAIL: $*" >&2
    exit 1
}

for artifact in "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE"; do
    [ -f "$artifact" ] || {
        echo "missing production artifact: $artifact" >&2
        exit 2
    }
done
before="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features "$NUCLEUS_FEATURE")
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features "$IMAGE_FEATURE")
after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] || {
    echo "a production artifact changed while building the isolated test ones" >&2
    exit 1
}

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$TEST_IMAGE" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.DIRECTION_FLAG TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE"

LOG="$OUT/events.log"

# --- ticks were actually taken while a process held the flag -------------------
# Without this the gate would pass on a boot where the window closed between two
# ticks and the question was never asked — a green result that means nothing,
# which is the failure mode this whole gate exists to end. The two numbers
# bracket the window and no system call is made inside it, so a tick that moved
# is a tick the nucleus took from a process holding `DF`.
held=0
while read -r first last; do
    [ "$last" -gt "$first" ] ||
        fail "a process held the direction flag from tick $first to tick $last: no tick was taken inside the window"
    held=$((held + 1))
done < <(sed -n 's/^TOS\.RUN\.DIRECTION_FLAG held_begin=\([0-9]*\) held_end=\([0-9]*\)$/\1 \2/p' "$LOG")

[ "$held" = "$PROCESSES" ] ||
    fail "$held of $PROCESSES processes held the direction flag across a tick"

# --- and the nucleus came back from every one of them --------------------------
# `run.sh` has already refused `TOS.EXCEPTION`. What is left is that the
# processes were not merely started but run to their own ends: a scheduler that
# copied a frame backwards does not reach either.
seen="$(grep -c "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$" "$LOG" || true)"
[ "$seen" = "$PROCESSES" ] ||
    fail "$seen of $PROCESSES processes completed their work"

echo "DIRECTION-FLAG PASS: $PROCESSES processes each held DF across a timer tick"
echo "  every tick entered the nucleus from one of them, and every one returned"
