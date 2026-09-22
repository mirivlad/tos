#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The Stage 4 data path, in the read direction: all 512 bytes of one sector cross
# the client/service boundary as an ordinary region.
#
# **This is the boundary `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1 draws**,
# and the thing `block-service.sh` does not reach — that gate transports one
# device-derived scalar and says so.
#
#   client --lookup--> registry --the service's endpoint--> client
#   client --call: a sector and a channel--> service
#   service --DMA/VirtIO/IRQ--> device
#   service: copy device-visible memory -> Region<mut u8> -> freeze
#   service --the 512 bytes, through MESSAGE_REGIONS--> the client's channel
#   client: index every byte
#
# **The bytes are the disk's and the client checks every one.** Sector `n` of the
# reference image holds 512 copies of `0xC0 + n` (`run.sh`'s Stage 4 profile), and
# this slice asks for sector **1** rather than sector 0: sector 0 is zero-filled,
# and zero is what untouched memory looks like. The service fills its DMA buffer
# with 0xA5 before the request, so every one of those bytes was a sentinel until
# the device wrote it. Both modules compute the byte they expect from the sector
# that was asked for rather than carrying a table of answers.
#
# **Nothing on the host inspects the payload.** This script judges two reported
# numbers; it does not read the region, does not copy anything across the textual
# boundary, and computes no checksum standing in for the client's own count. The
# only thing it puts on the disk is the disk's contents.
#
# **The one copy is forced, not chosen.** ADR-0037 makes `DmaRegion` neither
# shareable nor transferable in either mode, so a client cannot be handed
# device-visible memory and a driver cannot be handed the client's. Exactly one
# copy exists between them, which is what `docs/35` budgets.
#
#   bash host-tools/qemu-test/block-data-path.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-block-data-path}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/block-data-path"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-block-data-path"

# The launcher: four receiving names let go, then three processes created.
EXPECTED_INIT="i64:127"
# The registry: a registration and a lookup answered with it.
EXPECTED_REGISTRY="i64:3"
# The client: looked up, was delivered a capability, requested a sector, received
# a region on the channel it handed over, and found all 512 bytes.
EXPECTED_CLIENT="i64:31"
# Stage 4D-2's twenty device-side facts, which the service still proves.
PROVED_ALL=1048575
LEN_SHIFT=1048576
SECTOR_BYTES=512

fail() {
    echo "block-data-path: FAIL: $*" >&2
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
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- four modules, four processes ----------------------------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=4$' "$LOG" ||
    fail "the boot did not run a set of four modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 4 ] ||
    fail "four processes did not begin"

# --- the client holds no hardware authority ------------------------------------
# Each requested exactly twice: by the launcher that hands it on and by the one
# service entitled to it. A third requester would be this boundary crossed.
for twice in platform.pci.Bus system.memory.Authority; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$twice ")" = 2 ] ||
        fail "$twice was requested by other than the launcher and the block service"
done
for never in platform.pci.FunctionConfig platform.irq.Source platform.dma.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested by a module, and none may be"
done
# **And nobody imports the region interface**, which is the other half of "the
# client could not have made one": `system.memory.Region` is not
# startup-importable at all (`SYSTEM_INTERFACE_V1` §4.3), so the only regions in
# this boot are the ones an operation produced.
[ "$(count '^TOS\.RUN\.REQUEST binding=.* interface=system\.memory\.Region ')" = 0 ] ||
    fail "a module requested system.memory.Region, which no import can answer"

# --- the path, operation by operation ------------------------------------------
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=0$')" = 2 ] ||
    fail "the registration and the lookup were not both answered"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_carrying status=0$')" = 1 ] ||
    fail "the client's request did not reach the block service"
# The region: made, frozen, sent, received. Once each, and each answering OK.
for operation in region_allocate region_freeze endpoint_send_region endpoint_receive_region; do
    [ "$(count "^TOS\.RUN\.INTERFACE operation=$operation status=0\$")" = 1 ] ||
        fail "$operation did not succeed exactly once: $(grep "operation=$operation" "$LOG" || echo absent)"
done

# --- and the device really was driven ------------------------------------------
grep -q '^TOS\.RUN\.PCI_ASSIGNED ' "$LOG" || fail "no PCI function was claimed"
grep -q '^TOS\.RUN\.DMA_REGION ' "$LOG" || fail "no DMA region was made"
grep -q '^TOS\.RUN\.IRQ_DELIVERED ' "$LOG" ||
    fail "the device raised no interrupt, so nothing completed through the queue"

# --- the four accounts ---------------------------------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_INIT\$")" = 1 ] ||
    fail "the launcher did not report four releases and three processes: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY\$")" = 1 ] ||
    fail "the registry did not report a registration and a lookup"
# **The one that matters.** The client counted the bytes itself; this reads its
# account. A single corrupted byte lowers the count below 512 and the bit is not
# set, so this line fails.
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLIENT\$")" = 1 ] ||
    fail "the client did not report all $SECTOR_BYTES bytes of the sector: $(grep COMPLETED "$LOG")"

# The service's own account is Stage 4D-2's composite, unchanged in what it
# proves about the device.
service="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\([0-9]\{7,\}\)$/\1/p' "$LOG" | head -1)"
[ -n "$service" ] || fail "the block service reported no composite at all"
proved=$((service % LEN_SHIFT))
[ "$proved" = "$PROVED_ALL" ] ||
    fail "the service proved $proved of $PROVED_ALL device-side facts"

echo "BLOCK-DATA-PATH PASS: all $SECTOR_BYTES bytes of one sector crossed IPC"
echo "  client -> IPC -> block service -> real VirtIO DMA read -> copy into an"
echo "  ordinary Region<mut u8> -> freeze -> MESSAGE_REGIONS -> client indexes it"
echo "  the client holds no PCI bus, function, window, interrupt source, DMA"
echo "  region or memory authority, and cannot import system.memory.Region at"
echo "  all — so the region it read can only have arrived in a message"
echo "  sector 1, whose 512 bytes are 0xC1 in the reference image and were 0xA5"
echo "  in the service's buffer until the device wrote them; both modules derive"
echo "  the byte they expect from the sector that was asked for"
echo "  every byte counted in canonical text: this script read two numbers and"
echo "  never the region, and one corrupted byte fails the gate"
echo "  the copy from device-visible to ordinary memory is the one ADR-0037"
echo "  forces, not one the service chose"
echo "  the service still proves all $PROVED_ALL device-side facts of Stage 4D-2"
echo "  NOT claimed: more than one sector, more than one client, writing through"
echo "  this path, durability, zero-copy, request framing, or Stage 4 closure"
