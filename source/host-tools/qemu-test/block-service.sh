#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The Stage 4 client/service data path, end to end and for the first time.
#
#   separate textual client
#     -> IPC
#     -> textual block service / driver
#     -> DMA
#     -> VirtIO
#     -> IRQ
#     -> reply
#
# **What each half contributes.** The right-hand half is Stage 4D-2's, unchanged
# in what it proves: one real `VIRTIO_BLK_T_IN` through one real split
# virtqueue, with a sentinel rather than a success code as the evidence — both
# `VIRTIO_BLK_T_IN` and `VIRTIO_BLK_S_OK` are zero and the reference image is
# zero-filled, so "it returned success and the data is zero" is what untouched
# memory looks like. The left-hand half is new: the request comes from a
# separate process that holds no part of the machine and had no name for the
# service until the ADR-0093 P3 registry gave it one.
#
# **The number that crosses is the device's.** A request's inline length is the
# sector; the answer's is how many of that sector's 512 bytes are zero, capped
# at `IPC_V1` §3's inline bound of 256. The service fills the buffer with 0xA5
# before the request, so a run in which no DMA happened answers 0 and a real one
# answers 256. Neither number is in the client, and the client cannot reach the
# device to check it any other way.
#
# **The capability boundary is the point and is asserted, not assumed.** The
# launcher holds the PCI root and hands it to exactly one child. The client's
# plan names no PCI bus, no function, no window, no interrupt source, no DMA
# region and no memory authority, and the log is read for every one of them.
#
#   bash host-tools/qemu-test/block-service.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-block-service}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/block-service"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-block-service"

# The launcher: four receiving names let go, then three processes created.
EXPECTED_INIT="i64:127"
# The registry: a registration and a lookup answered with it.
EXPECTED_REGISTRY="i64:3"
# The client: a lookup answered, a capability delivered, the service reached,
# and a whole sector of zeroes reported back.
EXPECTED_CLIENT="i64:15"
# Stage 4D-2's twenty facts, which the service still proves on the device side.
PROVED_ALL=1048575
LEN_SHIFT=1048576

fail() {
    echo "block-service: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-block-service)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/registry/nameservice.tos\t%s/nameservice.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/service/block.tos\t%s/service.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/client/block.tos\t%s/client.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/path.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/path.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/path.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- four modules, four processes ----------------------------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=4$' "$LOG" ||
    fail "the boot did not run a set of four modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 4 ] ||
    fail "four processes did not begin"

# --- the client holds no part of the machine -----------------------------------
# Each of these is requested exactly once in this boot, by the block service,
# and the launcher's own PCI root is an endowment rather than a request. A
# second requester of any of them would be the boundary this slice exists to
# draw being crossed.
# Twice each and no more: the launcher imports them to hand them on, and the
# block service imports them to use them. A third requester would be a module
# this launcher decided to give the machine to, and there is no third.
for twice in platform.pci.Bus system.memory.Authority; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$twice ")" = 2 ] ||
        fail "$twice was requested by other than the launcher and the one service entitled to it"
done
# And the service is the one that holds them: its bindings, and nobody else's.
[ "$(count '^TOS\.RUN\.REQUEST binding=budget interface=system\.memory\.Authority ')" = 1 ] ||
    fail "the block service's own authority binding is not there exactly once"
for never in platform.pci.FunctionConfig platform.irq.Source platform.dma.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested by a module, and none may be"
done
[ "$(count '^TOS\.RUN\.REQUEST binding=inbox interface=system\.ipc\.Endpoint ')" = 1 ] ||
    fail "the client's inbox was not requested by name and kind"

# --- the path, operation by operation ------------------------------------------
# The service registers; the client looks up; the registry delivers.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=0$')" = 2 ] ||
    fail "the registration and the lookup were not both answered"
# Two sends that carried a capability: the launcher handing the registry to the
# service — which is where ADR-0077 §2's four-capability bound put it — and the
# registry handing the service's endpoint to the client.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_carrying status=0$')" = 2 ] ||
    fail "the two capability deliveries did not both happen"
# Five receives that produced what a message carried, and the arithmetic is the
# whole path: two by the registry — a registration and a lookup — two by the
# service — the registry handed to it, then the client's request — and one by
# the client, which is how the service's endpoint reaches it at all.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call status=0$')" = 5 ] ||
    fail "the receives do not add up to two by the registry, two by the service and one delivery"
# Answered by the registry twice and by the block service once.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply status=0$')" = 3 ] ||
    fail "three calls were not answered"
# **The call that crosses the whole path**, and the only one whose answer is a
# number rather than a status.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_for status=0$')" = 1 ] ||
    fail "the client did not reach the block service"

# --- and the device really was driven ------------------------------------------
grep -q '^TOS\.RUN\.PCI_ASSIGNED ' "$LOG" ||
    fail "no PCI function was claimed"
grep -q '^TOS\.RUN\.DMA_REGION ' "$LOG" ||
    fail "no DMA region was made"
grep -q '^TOS\.RUN\.IRQ_DELIVERED ' "$LOG" ||
    fail "the device raised no interrupt, so nothing completed through the queue"

# --- the four accounts ---------------------------------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_INIT\$")" = 1 ] ||
    fail "the launcher did not report four releases and three processes: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY\$")" = 1 ] ||
    fail "the registry did not report a registration and a lookup"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLIENT\$")" = 1 ] ||
    fail "the client did not report the whole path: $(grep COMPLETED "$LOG")"

# The service's own account is Stage 4D-2's composite, and its low twenty bits
# are the same twenty facts that gate asserts.
service="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\([0-9]\{7,\}\)$/\1/p' "$LOG" | head -1)"
[ -n "$service" ] || fail "the block service reported no composite at all"
proved=$((service % LEN_SHIFT))
[ "$proved" = "$PROVED_ALL" ] ||
    fail "the service proved $proved of $PROVED_ALL device-side facts"

echo "BLOCK-SERVICE PASS: a client with no part of the machine read a real sector"
echo "  separate textual client -> IPC -> textual block service -> DMA -> VirtIO"
echo "  -> IRQ -> reply, in four canonical textual modules and four processes"
echo "  the client looked \`block.device.v1\` up through the P3 registry, was"
echo "  handed the service's endpoint in a message, and called it naming a sector"
echo "  the service holds the PCI root, claimed the function, mapped the window,"
echo "  claimed one MSI-X source and allocated one DMA region — and the client's"
echo "  plan names none of those: no bus, function, window, source, region or"
echo "  memory authority, read out of the log rather than assumed"
echo "  the answer that crossed back is the device's: 256 of the sector's bytes"
echo "  are zero, where the service's buffer was 0xA5 until the device wrote it,"
echo "  so a run with no DMA would have answered 0"
echo "  the service still proves all $PROVED_ALL device-side facts of Stage 4D-2"
echo "  NOT claimed: more than one request through the queue (Stage 4D-3),"
echo "  writing (4D-5), durability, a payload region crossing IPC, restart, or"
echo "  anything about performance"
