#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4D-1: one real split virtqueue, configured by canonical TOS text.
#
# The whole modern VirtIO PCI initialization against the real reference
# endpoint, ending with an **empty, enabled** queue. What this gate proves is
# that the device accepted the queue substrate — not that it performed DMA
# through it. Those are two claims and this is only the first: no descriptor is
# exposed, nothing is notified, and no block request is made.
#
# The external contract is cited exactly and is not TOS's:
#
#   Virtual I/O Device (VIRTIO) Version 1.4
#   Committee Specification 01, 8 April 2026
#
# **The device does not report a protocol minor**, and this gate does not claim
# one. A VirtIO device exposes feature negotiation; what is asserted here is
# that the TOS Stage 4 reference driver implements the cited modern PCI /
# split-virtqueue subset and that this device accepted it.
#
#   bash host-tools/qemu-test/virtio-queue.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# The reference profile decides which function every assertion here is about.
# shellcheck source=/dev/null
. "$HERE/stage4-profile.sh"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-queue}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"

fail() { echo "virtio-queue: FAIL: $*" >&2; exit 1; }

# Every evidence build is its own target directory and none outlives this run.
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

capsule_for() {
    local name="$1" fixture="$2"
    printf '/system/boot/init.tos\t%s/init.tos\n' "$fixture" > "$OUT/$name-manifest.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$OUT/$name.bin" --meta "$OUT/$name.meta.json" \
        "$OUT/$name-manifest.txt" >/dev/null
    python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
        --capsule "$OUT/$name.bin" --manifest "$OUT/$name.meta.json" >/dev/null
}

capsule_for queue "$ROOT/tests/vectors/virtio-queue"
capsule_for refused "$ROOT/tests/vectors/virtio-queue-refused"

completed() {
    sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$OUT/$1/events.log"
}

# The fourteen facts the module proves, one bit each. Composed so that no
# partial run and no single outcome can produce it, and so that a missing fact
# names itself.
PROVED_RESET=1
PROVED_VERSION_1=2
PROVED_FEATURES_OK=4
PROVED_QUEUE_EXISTS=8
PROVED_SIZE_ACCEPTED=16
PROVED_LAYOUT_FITS=32
PROVED_ALIGNMENT=64
PROVED_DESC_READBACK=128
PROVED_DRIVER_READBACK=256
PROVED_DEVICE_READBACK=512
PROVED_MSIX_ACCEPTED=1024
PROVED_ENABLED=2048
PROVED_DRIVER_OK=4096
PROVED_NO_DESCRIPTOR=8192
ALL=$((PROVED_RESET + PROVED_VERSION_1 + PROVED_FEATURES_OK + PROVED_QUEUE_EXISTS \
    + PROVED_SIZE_ACCEPTED + PROVED_LAYOUT_FITS + PROVED_ALIGNMENT \
    + PROVED_DESC_READBACK + PROVED_DRIVER_READBACK + PROVED_DEVICE_READBACK \
    + PROVED_MSIX_ACCEPTED + PROVED_ENABLED + PROVED_DRIVER_OK + PROVED_NO_DESCRIPTOR))

# --- the positive boot ---------------------------------------------------------
bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/queue.bin" \
    --nucleus "$(nucleus_for test-dma-driver)" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

# P1 and P5 stay two facts, as ADR-0084 §5c requires and as the Stage 4C-2
# record insisted: both refuse operation 30 the same way, so a boot reporting
# only the refusal could not say which one it was.
grep -q "TOS.RUN.PCI_ASSIGNED .*express=1 " "$OUT/live/events.log" ||
    fail "P1 does not hold: the target endpoint reports no PCI Express capability"
grep -q "TOS.RUN.PCI_ASSIGNED .*dma=1 " "$OUT/live/events.log" ||
    fail "P5 does not hold: the profile did not qualify the target endpoint"

grep -q "TOS.RUN.DMA_REGION .*capability_delta=1 " "$OUT/live/events.log" ||
    fail "the queue substrate is not one region: operation 30 added more than one entry"
grep -q "TOS.RUN.DMA_REGION .*aliases=0" "$OUT/live/events.log" ||
    fail "an alias of the queue region was left behind"

# **One region, and exactly one.** Three device addresses come from three
# bounded offsets in the same object, which is what makes "no second allocation
# and no reconstructed base + offset" a fact about the boot rather than a
# reading of the source.
regions="$(grep -c "TOS.RUN.DMA_REGION " "$OUT/live/events.log" || true)"
[ "$regions" = "1" ] ||
    fail "the boot allocated $regions DMA regions; the queue substrate is one"

value="$(completed live)"
[ "$value" = "$ALL" ] || {
    echo "the module reported $value; every proved fact sums to $ALL" >&2
    for pair in "reset:$PROVED_RESET" "version_1:$PROVED_VERSION_1" \
        "features_ok:$PROVED_FEATURES_OK" "queue_exists:$PROVED_QUEUE_EXISTS" \
        "size_accepted:$PROVED_SIZE_ACCEPTED" "layout_fits:$PROVED_LAYOUT_FITS" \
        "alignment:$PROVED_ALIGNMENT" "desc_readback:$PROVED_DESC_READBACK" \
        "driver_readback:$PROVED_DRIVER_READBACK" "device_readback:$PROVED_DEVICE_READBACK" \
        "msix_accepted:$PROVED_MSIX_ACCEPTED" "enabled:$PROVED_ENABLED" \
        "driver_ok:$PROVED_DRIVER_OK" "no_descriptor:$PROVED_NO_DESCRIPTOR"
    do
        bit="${pair##*:}"
        [ "$value" -ge 0 ] && [ $((value & bit)) -ne 0 ] ||
            echo "  missing: ${pair%%:*}" >&2
    done
    fail "the device did not accept the queue substrate"
}

echo "virtio-queue: the device accepted the queue substrate on $(stage4_target_fields)"
echo "  reset observed; VERSION_1 offered and accepted; FEATURES_OK read back"
echo "  queue 0 exists, its size was chosen, written and read back"
echo "  one 4 KiB DMA region holds the whole split ring, and the layout was proved to fit"
echo "  three device addresses from three bounded offsets in that one region,"
echo "  each correctly aligned, each read back from the device's own registers"
echo "  one MSI-X table entry accepted by the device, queue_enable reads 1,"
echo "  DRIVER_OK reads set, and avail.idx is 0 — no buffer is exposed"
echo "  claimed: the device accepted the substrate."
echo "  NOT claimed: that the device performed DMA through it. No descriptor exists."

# --- the refusals the device really exhibits -----------------------------------
#
# Two of the negatives are the reference device's own answer or this driver's
# own checked arithmetic rather than source-level claims about branches nobody
# took, and they run as one boot:
#
#   a queue index it does not have   ->  queue_size reads 0
#   a queue larger than one region   ->  this driver's layout guard refuses
#
# The rest of the negative list — a device that does not offer VERSION_1, one
# that refuses FEATURES_OK, the MSI-X vector, queue_enable or DRIVER_OK — cannot
# be exhibited by this device, and **no fake device is built to manufacture
# them**. Their evidence class is static, and the record says so.
#
# **The MSI-X one was attempted and withdrawn.** With no interrupt source
# claimed the nucleus leaves MSI-X disabled at claim time, and the reference
# device accepted the queue vector anyway — answering 0 rather than NO_VECTOR.
# The positive boot's readback check is still what §4.1.5.1.2.2 requires; this
# device simply cannot be made to fail it.
REFUSED_ABSENT_QUEUE=1
REFUSED_LAYOUT=2
REFUSALS=$((REFUSED_ABSENT_QUEUE + REFUSED_LAYOUT))

bash "$HERE/run.sh" \
    --out "$OUT/refused" \
    --capsule "$OUT/refused.bin" \
    --nucleus "$(nucleus_for test-dma-driver)" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ASSIGNED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

refused="$(completed refused)"
[ "$refused" = "$REFUSALS" ] ||
    fail "the refusal boot reported $refused, not the $REFUSALS the three refusals sum to"

# **And it never reached a live queue**, which is what makes it a negative: no
# region was allocated, and the device was left FAILED rather than DRIVER_OK.
grep -q "TOS.RUN.DMA_REGION " "$OUT/refused/events.log" &&
    fail "the refusal boot allocated a DMA region; it must refuse before one is needed"

echo "virtio-queue: two refusals observed against the real device —"
echo "  an absent queue presents size 0, and a queue larger than one region is"
echo "  refused by this driver's layout guard before anything is programmed"
