#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4D-2: one real block read, through one real split virtqueue.
#
# Stage 4D-1 proved the device accepted an empty queue. This proves the device
# **used** it: one `VIRTIO_BLK_T_IN` of sector 0, 512 bytes, through a
# three-descriptor chain on the real reference endpoint.
#
#   3 descriptors -> publish -> avail.idx=1 -> publish -> real PCI notify
#                 -> irq_wait -> dma_consume -> used.idx=1 -> used.id=0
#                 -> status=OK -> 512 device-written bytes
#
# **The proof that DMA happened is a sentinel, not a success code.** Both
# `VIRTIO_BLK_T_IN` and `VIRTIO_BLK_S_OK` are 0 and the reference backing image
# is explicitly zero-filled, so "status 0 and data 0" is what untouched memory
# looks like. The driver writes 0xA5 across all 512 data bytes and 0xFF into the
# status byte first; what is checked is that the device replaced them.
#
#   Virtual I/O Device (VIRTIO) Version 1.4
#   Committee Specification 01, 8 April 2026
#
#   bash host-tools/qemu-test/virtio-block-read.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
. "$HERE/stage4-profile.sh"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-block-read}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"

fail() { echo "virtio-block-read: FAIL: $*" >&2; exit 1; }

BUILT=""
cleanup() { [ -z "$BUILT" ] || rm -rf $BUILT; }
trap cleanup EXIT

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }

before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
nucleus_for() {
    local feature="$1"
    local target="$ROOT/target/$feature"
    BUILT="$BUILT $target"
    (cd "$ROOT" && CARGO_TARGET_DIR="$target" cargo build --release \
        -p tos-nucleus --target x86_64-unknown-none --features "$feature" >/dev/null 2>&1) ||
        fail "the nucleus does not build with $feature"
    [ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
        fail "the production nucleus changed while building $feature"
    echo "$target/x86_64-unknown-none/release/tos-nucleus"
}

printf '/system/boot/init.tos\t%s/init.tos\n' "$ROOT/tests/vectors/virtio-block-read" \
    > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/read.bin" --meta "$OUT/read.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/read.bin" --manifest "$OUT/read.meta.json" >/dev/null

# The twenty facts the module proves, one bit each, and `used.len` carried above
# them so the harness can record what the device reported without this gate
# inventing a conformance assertion about it (§2.7.8.2 permits the device to
# write more than it reports).
PROVED_ALL=1048575
LEN_SHIFT=1048576
QUEUE_SHIFT=4294967296

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/read.bin" \
    --nucleus "$(nucleus_for test-dma-driver)" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

value="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$OUT/live/events.log")"
[ -n "$value" ] || fail "the boot reported no completion at all"
[ "$value" -gt 0 ] ||
    fail "the module reported $value; a negative names the step that did not hold"

proved=$((value % LEN_SHIFT))
used_len=$(((value / LEN_SHIFT) % (QUEUE_SHIFT / LEN_SHIFT)))
queue_size=$((value / QUEUE_SHIFT))

[ "$proved" = "$PROVED_ALL" ] || {
    echo "the module proved $proved of $PROVED_ALL" >&2
    for pair in "reset:1" "version_1:2" "features_ok:4" "queue_exists:8" \
        "size_accepted:16" "layout_fits:32" "alignment:64" "desc_readback:128" \
        "driver_readback:256" "device_readback:512" "msix_accepted:1024" \
        "enabled:2048" "driver_ok:4096" "request_fits:8192" "notify_found:16384" \
        "used_advanced:32768" "used_id:65536" "len_covers_data:131072" \
        "status_ok:262144" "sector_read:524288"
    do
        bit="${pair##*:}"
        [ $((proved & bit)) -ne 0 ] || echo "  missing: ${pair%%:*}" >&2
    done
    fail "the request did not complete as the contract requires"
}

# **One region for the whole thing**, ring and request storage together: the
# request added no allocation to the one the queue was built from.
regions="$(grep -c "TOS.RUN.DMA_REGION " "$OUT/live/events.log" || true)"
[ "$regions" = "1" ] ||
    fail "the boot allocated $regions DMA regions; ring and request share one"

grep -q "TOS.RUN.DMA_REGION .*capability_delta=1 " "$OUT/live/events.log" ||
    fail "operation 30 did not add exactly one capability-table entry"
grep -q "TOS.RUN.PCI_ASSIGNED .*express=1 " "$OUT/live/events.log" ||
    fail "P1 does not hold: the target endpoint reports no PCI Express capability"
grep -q "TOS.RUN.PCI_ASSIGNED .*dma=1 " "$OUT/live/events.log" ||
    fail "P5 does not hold: the profile did not qualify the target endpoint"

echo "virtio-block-read: one VIRTIO_BLK_T_IN of sector 0 completed on $(stage4_target_fields)"
echo "  three descriptors: 16-byte header device-readable, 512 bytes device-writable,"
echo "  one status byte device-writable — one chain, head 0, in one DMA region"
echo "  published before avail.idx, published again before the notification,"
echo "  notified through the device's own VIRTIO_PCI_CAP_NOTIFY_CFG location"
echo "  woken by a real MSI-X interrupt, consumed before anything was read"
echo "  used.idx advanced 0 -> 1, used.ring[0].id = 0, status = VIRTIO_BLK_S_OK"
echo "  and all 512 bytes of the 0xA5 sentinel were replaced by sector 0's contents"
echo "  queue size this driver settled on: $queue_size descriptors"
echo "  used.len as the device reported it: $used_len bytes (recorded, not asserted"
echo "  beyond covering the 512 data bytes — the device MAY write more than len)"
echo "  claimed: the device performed DMA into this region."
echo "  NOT claimed: any client-facing block interface, or a steady-state budget."
