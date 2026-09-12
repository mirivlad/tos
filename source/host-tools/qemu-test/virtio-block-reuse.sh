#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4D-3: one split virtqueue, reused — two real block reads, one queue.
#
# Stage 4D-2 proved the device performed DMA through a queue canonical TOS text
# built, **once**, with every ring index written as a constant. This proves the
# queue is reusable: the same descriptor table, available ring and used ring
# serve two sequential `VIRTIO_BLK_T_IN` requests over two different sectors,
# with one device initialization, one `queue_enable`, one DMA region and one
# MSI-X source between them.
#
#   init once -> request 1: 3 of 4 descriptors -> avail.idx 0->1 -> notify
#                           -> irq -> consume -> used.idx 0->1 -> reclaim
#             -> request 2: 3 of 4 descriptors -> avail.idx 1->2 -> notify
#                           -> irq -> consume -> used.idx 1->2 -> reclaim
#
# **The pool is smaller than two chains**, so the second request is impossible
# without reclaiming the first one's descriptors, and the free list is a queue
# rather than a stack, so the second chain is `3 -> 0 -> 1` — a head the first
# request never used and two members it did.
#
#   Virtual I/O Device (VIRTIO) Version 1.4
#   Committee Specification 01, 8 April 2026
#
#   bash host-tools/qemu-test/virtio-block-reuse.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
. "$HERE/stage4-profile.sh"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-block-reuse}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"

fail() { echo "virtio-block-reuse: FAIL: $*" >&2; exit 1; }

# --- what the module reports ---------------------------------------------------
#
# Thirty facts, one bit each, and six measurements packed above them. Every
# field is bounded by the module before it is packed, so none reaches into the
# next: 30 bits of mask, then 3, 3, 2, 2, 2 and 8.
PROVED_ALL=1073741823
AVAIL_SHIFT=1073741824
USED_SHIFT=8589934592
HEAD_FIRST_SHIFT=68719476736
HEAD_SECOND_SHIFT=274877906944
REUSED_SHIFT=1099511627776
QUEUE_SHIFT=4398046511104

# What the Stage 4 profile seeds the backing image with, and which sectors this
# boot reads. The rule is `sector n holds 512 copies of 0xC0 + n`, and sector 0
# stays zero-filled so Stage 4D-2's witness is untouched.
FIRST_SECTOR=1
REQUESTS=2

# The facts the module reports, and the names this gate prints when one is
# missing. The order is the module's own bit order.
FACTS="reset:1 version_1:2 features_ok:4 queue_exists:8 size_accepted:16
 layout_fits:32 alignment:64 desc_readback:128 driver_readback:256
 device_readback:512 msix_accepted:1024 enabled:2048 driver_ok:4096
 request_fits:8192 notify_found:16384 wrap_arithmetic:32768 pool_ready:65536
 allocated_from_pool:131072 used_advanced:262144 used_id_is_the_chain_head:524288
 len_covers_data_and_status:1048576 status_ok:2097152 sector_content_exact:4194304
 descriptors_reclaimed:8388608 every_request:16777216 avail_progressed:33554432
 consumer_progressed:67108864 descriptor_reused:134217728 head_moved:268435456
 reads_distinct:536870912"

# **The judgement, as one function, so the negative cases below exercise the
# same code the real run does.** Prints the reason it refuses and returns 1;
# prints nothing and returns 0 when the value is an accepted Stage 4D-3 result.
judge() {
    local value="$1"
    local proved avail used head_first head_second reused queue

    if [ "$value" -le 0 ]; then
        echo "  the module reported $value; a negative names the step that did not hold"
        return 1
    fi
    proved=$((value % AVAIL_SHIFT))
    avail=$(((value / AVAIL_SHIFT) % 8))
    used=$(((value / USED_SHIFT) % 8))
    head_first=$(((value / HEAD_FIRST_SHIFT) % 4))
    head_second=$(((value / HEAD_SECOND_SHIFT) % 4))
    reused=$(((value / REUSED_SHIFT) % 4))
    queue=$(((value / QUEUE_SHIFT) % 256))

    if [ "$proved" != "$PROVED_ALL" ]; then
        echo "  the module proved $proved of $PROVED_ALL"
        for pair in $FACTS; do
            bit="${pair##*:}"
            [ $((proved & bit)) -ne 0 ] || echo "    missing: ${pair%%:*}"
        done
        return 1
    fi
    # **The producer index is the whole difference between a reused queue and a
    # reinitialized one.** A driver that reset the queue between requests would
    # report 1 here and 1 below, whatever else it proved.
    if [ "$avail" != "$REQUESTS" ]; then
        echo "  avail.idx reached $avail after $REQUESTS requests; a queue that was"
        echo "  reinitialized between them would start over and reach 1"
        return 1
    fi
    if [ "$used" != "$REQUESTS" ]; then
        echo "  the used consumer index reached $used after $REQUESTS requests"
        return 1
    fi
    if [ "$reused" -lt 1 ]; then
        echo "  no descriptor of request 1's chain appeared in request 2's;"
        echo "  a larger preallocated table would also pass everything else"
        return 1
    fi
    if [ "$head_first" = "$head_second" ]; then
        echo "  both chains had head $head_first, so the second chain is the first"
        echo "  one again rather than one the pool composed"
        return 1
    fi
    if [ "$queue" -lt 4 ]; then
        echo "  the driver settled on a queue of $queue descriptors"
        return 1
    fi
    return 0
}

# --- the negative cases, against the judgement itself --------------------------
#
# **Not a substitute for the device run**, and it does not pretend to be: the
# real boot below is the only thing that proves anything about the device. What
# this proves is that the assertions above would *notice*, which a gate whose
# refusals are never taken cannot claim. Each value is what a specific wrong
# implementation would report.
self_test() {
    local complete refused
    # A correct result, for the shape: everything proved, two chains, heads 0
    # and 3, two descriptors reused, a queue of 128.
    complete=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_FIRST_SHIFT + 3 * HEAD_SECOND_SHIFT \
        + 2 * REUSED_SHIFT + 128 * QUEUE_SHIFT))
    judge "$complete" || fail "the judgement refuses a result it must accept"

    # A one-shot driver executed twice through a reinitialized queue: every
    # per-request fact holds, and both indices start over.
    refused=$((PROVED_ALL + 1 * AVAIL_SHIFT + 1 * USED_SHIFT \
        + 0 * HEAD_FIRST_SHIFT + 3 * HEAD_SECOND_SHIFT \
        + 2 * REUSED_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a queue reinitialized between requests is accepted"

    # A driver with a table big enough that nothing is ever recycled.
    refused=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_FIRST_SHIFT + 3 * HEAD_SECOND_SHIFT \
        + 0 * REUSED_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a run that recycled no descriptor is accepted"

    # A driver whose second chain is the first one again, head and all.
    refused=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 1 * HEAD_FIRST_SHIFT + 1 * HEAD_SECOND_SHIFT \
        + 3 * REUSED_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a second chain identical to the first is accepted"

    # One fact short of the set — the sector content, which is the one the
    # sentinels exist for.
    refused=$((PROVED_ALL - 4194304 + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_FIRST_SHIFT + 3 * HEAD_SECOND_SHIFT \
        + 2 * REUSED_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a run whose data did not match the sector is accepted"

    # And the module's own refusal shape.
    ! judge -57 >/dev/null || fail "a negative completion value is accepted"
}

BUILT=""
cleanup() { [ -z "$BUILT" ] || rm -rf $BUILT; }
trap cleanup EXIT

self_test

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

printf '/system/boot/init.tos\t%s/init.tos\n' "$ROOT/tests/vectors/virtio-block-reuse" \
    > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/reuse.bin" --meta "$OUT/reuse.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/reuse.bin" --manifest "$OUT/reuse.meta.json" >/dev/null

NUCLEUS="$(nucleus_for test-dma-driver)"

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/reuse.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

EVENTS="$OUT/live/events.log"
value="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$EVENTS")"
[ -n "$value" ] || fail "the boot reported no completion at all"

if ! judge "$value"; then
    fail "the queue was not proved reusable"
fi
proved=$((value % AVAIL_SHIFT))
avail=$(((value / AVAIL_SHIFT) % 8))
used=$(((value / USED_SHIFT) % 8))
head_first=$(((value / HEAD_FIRST_SHIFT) % 4))
head_second=$(((value / HEAD_SECOND_SHIFT) % 4))
reused=$(((value / REUSED_SHIFT) % 4))
queue=$(((value / QUEUE_SHIFT) % 256))

# --- what the nucleus itself witnessed -----------------------------------------
#
# These are not the module's report. The nucleus writes one
# `TOS.RUN.IRQ_DELIVERED` line per real MSI-X message, from the handler that
# took it, and counts them itself — so two requests means two messages from the
# device, and no module can produce that line at all.
deliveries="$(grep -c "TOS.RUN.IRQ_DELIVERED " "$EVENTS" || true)"
[ "$deliveries" -ge "$REQUESTS" ] ||
    fail "the device delivered $deliveries interrupts for $REQUESTS requests"
counted="$(sed -n 's/^TOS\.RUN\.IRQ_DELIVERED .* deliveries=\([0-9]*\) .*$/\1/p' "$EVENTS" |
    sort -n | tail -1)"
[ "$counted" -ge "$REQUESTS" ] ||
    fail "the nucleus counted $counted deliveries on the source for $REQUESTS requests"

# **One device, brought up once.** A run that reinitialized between requests
# would claim the function again, map again, or allocate again; all three are
# counted rather than assumed.
assigned="$(grep -c "TOS.RUN.PCI_ASSIGNED " "$EVENTS" || true)"
[ "$assigned" = "1" ] ||
    fail "the boot took $assigned assignments of the endpoint; one device, once"
regions="$(grep -c "TOS.RUN.DMA_REGION " "$EVENTS" || true)"
[ "$regions" = "1" ] ||
    fail "the boot allocated $regions DMA regions; both requests share one"
waits="$(grep -c "TOS.RUN.INTERFACE operation=irq_wait status=0" "$EVENTS" || true)"
[ "$waits" -ge "$REQUESTS" ] ||
    fail "the module completed $waits interrupt waits for $REQUESTS requests"

grep -q "TOS.RUN.DMA_REGION .*capability_delta=1 " "$EVENTS" ||
    fail "operation 30 did not add exactly one capability-table entry"
grep -q "TOS.RUN.PCI_ASSIGNED .*express=1 " "$EVENTS" ||
    fail "P1 does not hold: the target endpoint reports no PCI Express capability"
grep -q "TOS.RUN.PCI_ASSIGNED .*dma=1 " "$EVENTS" ||
    fail "P5 does not hold: the profile did not qualify the target endpoint"

last_sector=$((FIRST_SECTOR + REQUESTS - 1))
echo "virtio-block-reuse: $REQUESTS sequential VIRTIO_BLK_T_IN through one queue on $(stage4_target_fields)"
echo "  sectors $FIRST_SECTOR..$last_sector, each read into the same 512-byte buffer,"
echo "  poisoned with a different sentinel before each request and checked byte for"
echo "  byte against 0xC0 + sector — so neither untouched memory nor the previous"
echo "  request's result can satisfy the witness"
echo "  one device initialization, one feature negotiation, one queue_enable,"
echo "  one DMA region and one MSI-X source for both requests"
echo "  avail.idx progressed 0 -> $avail, consumed used entries 0 -> $used,"
echo "  each at the ring slot its own index named rather than at slot 0"
echo "  descriptor pool: 4 of the $queue table entries, 3 spent per request — so"
echo "  request 2 could not be served without reclaiming request 1's chain"
echo "  chain heads: $head_first then $head_second; descriptors reused: $reused of 3"
echo "  the nucleus counted $counted real MSI-X deliveries on this source"
echo "  16-bit wrap arithmetic checked across 65535 -> 0 before the first request"
echo "  claimed: one initialized split virtqueue served more than one real request."
echo "  NOT claimed: requests in flight together, writes, scheduling, a block"
echo "  service, or a driver framework."
