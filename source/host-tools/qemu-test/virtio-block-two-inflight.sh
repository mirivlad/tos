#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4D-4: two real block reads outstanding together in one split virtqueue.
#
# Stage 4D-3 proved the queue is reusable — two sequential `VIRTIO_BLK_T_IN`
# through one initialized queue, the first request's descriptors reclaimed to
# compose the second. Its loop was submit, wait, consume, reclaim, submit, and
# at no instant did the device hold more than one of this driver's chains.
#
# This proves queue depth greater than one. Both chains are built out of a pool
# that is exactly two chains wide, both heads go into consecutive available-ring
# slots, and then **one** store moves `avail.idx` by two:
#
#   avail.ring[i] = head A ; avail.ring[i+1] = head B
#   dma_publish -> avail.idx = i + 2 (one store) -> dma_publish -> notify
#
# Two stores of `+1` would be a different program even with nothing between
# them: §2.7.13.3 lets the device access a chain "immediately" once `avail.idx`
# is updated, so after a first `+1` it could legally finish request A before the
# second store landed.
#
# Completion order is not assumed. `VIRTIO_F_IN_ORDER` is not negotiated, §2.6
# says a device need not use buffers in availability order and §5.2.6 says block
# requests are used "not necessarily in order", so identity is `used_elem.id`
# matched against the two outstanding heads, and either order passes.
#
#   Virtual I/O Device (VIRTIO) Version 1.4
#   Committee Specification 01, 8 April 2026
#
#   bash host-tools/qemu-test/virtio-block-two-inflight.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
. "$HERE/stage4-profile.sh"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-block-two-inflight}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
VECTOR="$ROOT/tests/vectors/virtio-block-two-inflight/init.tos"

fail() { echo "virtio-block-two-inflight: FAIL: $*" >&2; exit 1; }

# --- what the module reports ---------------------------------------------------
#
# Thirty facts, one bit each, and eight measurements packed above them. Every
# field is bounded by the module before it is packed, so none reaches into the
# next: 30 bits of mask, then 3, 3, 3, 3, 1, 5, 2 and 8.
PROVED_ALL=1073741823
AVAIL_SHIFT=1073741824
USED_SHIFT=8589934592
HEAD_A_SHIFT=68719476736
HEAD_B_SHIFT=549755813888
ORDER_SHIFT=4398046511104
WAKES_SHIFT=8796093022208
FRESH_SHIFT=281474976710656
QUEUE_SHIFT=1125899906842624

# What the Stage 4 profile seeds the backing image with, and which sectors this
# boot reads. The rule is `sector n holds 512 copies of 0xC0 + n`, and sector 0
# stays zero-filled so Stage 4D-2's witness is untouched.
FIRST_SECTOR=1
OUTSTANDING=2
POOL_COUNT=6
CHAIN_LENGTH=3

# The facts the module reports, in its own bit order.
FACTS="reset:1 version_1:2 features_ok:4 queue_exists:8 size_accepted:16
 layout_fits:32 alignment:64 desc_readback:128 driver_readback:256
 device_readback:512 msix_accepted:1024 enabled:2048 driver_ok:4096
 request_fits:8192 notify_found:16384 wrap_arithmetic:32768 pool_ready:65536
 queue_holds_two_chains:131072 request_blocks_disjoint:262144
 both_chains_allocated:524288 pool_empty_after_both:1048576
 both_chains_prepared:2097152 nothing_consumed_yet:4194304
 publication_added_two:8388608 both_chains_device_owned:16777216
 used_id_matched_an_outstanding_head:33554432
 len_covers_data_and_status:67108864 status_ok:134217728
 both_sectors_exact:268435456 each_chain_reclaimed:536870912"

# **The judgement, as one function**, so the negative cases below exercise the
# same code the real run does.
judge() {
    local value="$1"
    local proved avail used head_a head_b order wakes fresh queue

    if [ "$value" -le 0 ]; then
        echo "  the module reported $value; a negative names the step that did not hold"
        return 1
    fi
    proved=$((value % AVAIL_SHIFT))
    avail=$(((value / AVAIL_SHIFT) % 8))
    used=$(((value / USED_SHIFT) % 8))
    head_a=$(((value / HEAD_A_SHIFT) % 8))
    head_b=$(((value / HEAD_B_SHIFT) % 8))
    order=$(((value / ORDER_SHIFT) % 2))
    wakes=$(((value / WAKES_SHIFT) % 32))
    fresh=$(((value / FRESH_SHIFT) % 4))
    queue=$(((value / QUEUE_SHIFT) % 256))

    if [ "$proved" != "$PROVED_ALL" ]; then
        echo "  the module proved $proved of $PROVED_ALL"
        for pair in $FACTS; do
            bit="${pair##*:}"
            [ $((proved & bit)) -ne 0 ] || echo "    missing: ${pair%%:*}"
        done
        return 1
    fi
    # **The single publication is the whole difference from Stage 4D-3.** A
    # driver that served the two requests sequentially reaches `avail.idx` 2 as
    # well — 4D-3 does — so the producer index alone proves nothing here. What
    # the bit above proves is that it got there in one store, from a state in
    # which both chains were prepared and the pool was empty.
    if [ "$avail" != "$OUTSTANDING" ]; then
        echo "  avail.idx reached $avail for $OUTSTANDING chains"
        return 1
    fi
    if [ "$used" != "$OUTSTANDING" ]; then
        echo "  the used consumer index reached $used for $OUTSTANDING chains"
        return 1
    fi
    if [ "$head_a" = "$head_b" ]; then
        echo "  both chains reported head $head_a, so they are not disjoint"
        return 1
    fi
    if [ "$head_a" -ge "$POOL_COUNT" ] || [ "$head_b" -ge "$POOL_COUNT" ]; then
        echo "  a chain head ($head_a, $head_b) is outside the $POOL_COUNT-slot pool"
        return 1
    fi
    # **The expected allocation order from the initial FIFO, and nothing more.**
    # It is a determinism check on the pool, not a concurrency proof: with a
    # six-slot FIFO a driver that allocated A, reclaimed it and then allocated B
    # would hand B descriptors 3, 4, 5 as well, because A's three go back behind
    # the three still ahead of the front. What proves the two chains were
    # outstanding together is the conjunction of bits the module establishes
    # immediately before the single publication, listed in the report below.
    if [ "$head_b" != "$((head_a + CHAIN_LENGTH))" ]; then
        echo "  chain B's head is $head_b rather than the $((head_a + CHAIN_LENGTH)) this pool's"
        echo "  initial FIFO order gives it; the allocator did not behave deterministically"
        return 1
    fi
    if [ "$queue" -lt "$POOL_COUNT" ]; then
        echo "  the driver settled on a queue of $queue descriptors, fewer than the"
        echo "  $POOL_COUNT the pool needs for two chains"
        return 1
    fi
    if [ "$wakes" -lt 1 ]; then
        echo "  the module reported $wakes interrupt waits"
        return 1
    fi
    if [ "$fresh" -lt 1 ] || [ "$fresh" -gt "$OUTSTANDING" ]; then
        echo "  the largest batch of used entries one consume made visible was $fresh"
        return 1
    fi
    if [ "$order" != "0" ] && [ "$order" != "1" ]; then
        echo "  the module reported completion order $order"
        return 1
    fi
    return 0
}

# --- the negative cases, against the judgement itself --------------------------
#
# **Not a substitute for the device run.** What this proves is that the
# assertions above would notice, which a gate whose refusals are never taken
# cannot claim. Each value is what a specific wrong implementation would report.
self_test() {
    local complete refused
    # A correct result: everything proved, heads 0 and 3, A completed first, one
    # wake covering both, a queue of 128.
    complete=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_A_SHIFT + 3 * HEAD_B_SHIFT + 0 * ORDER_SHIFT \
        + 1 * WAKES_SHIFT + 2 * FRESH_SHIFT + 128 * QUEUE_SHIFT))
    judge "$complete" || fail "the judgement refuses a result it must accept"

    # **The same run with B completing first must also be accepted**, because
    # `VIRTIO_F_IN_ORDER` is not negotiated and both orders are legal.
    complete=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_A_SHIFT + 3 * HEAD_B_SHIFT + 1 * ORDER_SHIFT \
        + 2 * WAKES_SHIFT + 1 * FRESH_SHIFT + 128 * QUEUE_SHIFT))
    judge "$complete" || fail "the judgement refuses a legal B-then-A completion"

    # Stage 4D-3's own discipline, which reaches avail.idx 2 and consumer 2 and
    # proves every per-request fact — and cannot prove that both chains were
    # prepared at once, that the pool was empty, or that one store exposed them.
    refused=$((PROVED_ALL - 8388608 - 1048576 - 2097152 \
        + 2 * AVAIL_SHIFT + 2 * USED_SHIFT + 0 * HEAD_A_SHIFT + 3 * HEAD_B_SHIFT \
        + 0 * ORDER_SHIFT + 2 * WAKES_SHIFT + 1 * FRESH_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a sequential submit/wait/reclaim/submit run is accepted"

    # Two publications of +1 rather than one of +2: the producer index arrives
    # at the same place and the publication-shape bit does not.
    refused=$((PROVED_ALL - 8388608 \
        + 2 * AVAIL_SHIFT + 2 * USED_SHIFT + 0 * HEAD_A_SHIFT + 3 * HEAD_B_SHIFT \
        + 0 * ORDER_SHIFT + 1 * WAKES_SHIFT + 2 * FRESH_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "two +1 publications are accepted as one +2"

    # Two chains reported with the same head, which cannot be two disjoint
    # chains whatever the allocator did.
    refused=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_A_SHIFT + 0 * HEAD_B_SHIFT + 0 * ORDER_SHIFT \
        + 1 * WAKES_SHIFT + 2 * FRESH_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "two chains sharing a head are accepted"

    # One fact short of the set — both sectors exact, which the two poisons and
    # the two buffers exist for.
    refused=$((PROVED_ALL - 268435456 + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_A_SHIFT + 3 * HEAD_B_SHIFT + 0 * ORDER_SHIFT \
        + 1 * WAKES_SHIFT + 2 * FRESH_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a run whose data did not match its sector is accepted"

    # A device offering fewer descriptors than two chains need.
    refused=$((PROVED_ALL + 2 * AVAIL_SHIFT + 2 * USED_SHIFT \
        + 0 * HEAD_A_SHIFT + 3 * HEAD_B_SHIFT + 0 * ORDER_SHIFT \
        + 1 * WAKES_SHIFT + 2 * FRESH_SHIFT + 4 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a queue too small for two chains is accepted"

    # And the module's own refusal shape.
    ! judge -88 >/dev/null || fail "a negative completion value is accepted"
}

# **The driver reaches no callable value.** `TypeDef::Function` erases a
# parameter's `PassMode`, which is a recorded open item; the only way to hold a
# value of that type is a closure, and this module has none. Stage 4 vectors are
# `profile full`, so nothing in the profile forbids one — which is exactly why
# it is checked here rather than assumed.
no_callable_values() {
    ! grep -nE '(^|[^_[:alnum:]])fn[[:space:]]*\(' "$VECTOR" ||
        fail "the vector declares a function type or a closure, which reaches the callable PassMode gap"
}

# **The ownership publication happens once, and the source is what proves it.**
#
# No observation a driver can make about itself distinguishes one store of `+2`
# from two stores of `+1`: both arrive at the same counter, and a program that
# did the second immediately after the first would satisfy every runtime bit the
# module reports. So the shape is checked against the text — exactly one store
# into `avail.idx`, and exactly one use of the `+= added` helper — which is a
# deterministic check on the thing that actually differs.
one_ownership_publication() {
    local stores adds nexts
    stores="$(grep -cE 'put_le16\(borrow mut region, avail_offset \+ 2B,' "$VECTOR" || true)"
    [ "$stores" = "1" ] ||
        fail "the vector stores avail.idx $stores times; the claim is one publication"
    adds="$(grep -cE 'avail_idx = ring_add\(avail_idx, OUTSTANDING\);' "$VECTOR" || true)"
    [ "$adds" = "1" ] ||
        fail "the vector advances the producer index $adds times by the batch size"
    nexts="$(grep -cE 'avail_idx = ring_next\(' "$VECTOR" || true)"
    [ "$nexts" = "0" ] ||
        fail "the vector advances avail.idx one at a time in $nexts places"
}

BUILT=""
cleanup() { [ -z "$BUILT" ] || rm -rf $BUILT; }
trap cleanup EXIT

self_test
no_callable_values
one_ownership_publication

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

printf '/system/boot/init.tos\t%s/init.tos\n' "$ROOT/tests/vectors/virtio-block-two-inflight" \
    > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/inflight.bin" --meta "$OUT/inflight.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/inflight.bin" --manifest "$OUT/inflight.meta.json" >/dev/null

NUCLEUS="$(nucleus_for test-dma-driver)"

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/inflight.bin" \
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
    fail "two requests were not proved outstanding together"
fi
proved=$((value % AVAIL_SHIFT))
avail=$(((value / AVAIL_SHIFT) % 8))
used=$(((value / USED_SHIFT) % 8))
head_a=$(((value / HEAD_A_SHIFT) % 8))
head_b=$(((value / HEAD_B_SHIFT) % 8))
order=$(((value / ORDER_SHIFT) % 2))
wakes=$(((value / WAKES_SHIFT) % 32))
fresh=$(((value / FRESH_SHIFT) % 4))
queue=$(((value / QUEUE_SHIFT) % 256))

first_done="A"
second_done="B"
if [ "$order" = "1" ]; then
    first_done="B"
    second_done="A"
fi

# --- what the nucleus itself witnessed -----------------------------------------
#
# Not the module's report. The nucleus writes one `TOS.RUN.IRQ_DELIVERED` line
# per real MSI-X message, from the handler that took it, and counts them itself.
# **How many there are is a measurement, not a gate**: one message may cover both
# completions (ADR-0082 §10) or each may bring its own, and both are legal.
deliveries="$(grep -c "TOS.RUN.IRQ_DELIVERED " "$EVENTS" || true)"
[ "$deliveries" -ge 1 ] ||
    fail "the device delivered no interrupt at all for $OUTSTANDING requests"
[ "$deliveries" -le "$OUTSTANDING" ] ||
    fail "the device delivered $deliveries interrupts for $OUTSTANDING requests"
counted="$(sed -n 's/^TOS\.RUN\.IRQ_DELIVERED .* deliveries=\([0-9]*\) .*$/\1/p' "$EVENTS" |
    sort -n | tail -1)"
latched="$(grep -c "TOS.RUN.IRQ_DELIVERED .*latched=1" "$EVENTS" || true)"

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
[ "$waits" -ge 1 ] || fail "the module completed no interrupt wait"

# **One region, twice the size, and still one capability-table entry.** The
# extent grew because two request blocks do not fit 4 KiB; the authority, the
# lineage and the table did not.
grep -q "TOS.RUN.DMA_REGION .*bytes=8192 " "$EVENTS" ||
    fail "the boot did not allocate the 8 KiB region two request blocks need"
grep -q "TOS.RUN.DMA_REGION .*contiguous=1 " "$EVENTS" ||
    fail "the 8 KiB region is not one contiguous run"
grep -q "TOS.RUN.DMA_REGION .*capability_delta=1 " "$EVENTS" ||
    fail "operation 30 did not add exactly one capability-table entry"
grep -q "TOS.RUN.PCI_ASSIGNED .*express=1 " "$EVENTS" ||
    fail "P1 does not hold: the target endpoint reports no PCI Express capability"
grep -q "TOS.RUN.PCI_ASSIGNED .*dma=1 " "$EVENTS" ||
    fail "P5 does not hold: the profile did not qualify the target endpoint"

last_sector=$((FIRST_SECTOR + OUTSTANDING - 1))
echo "virtio-block-two-inflight: $OUTSTANDING VIRTIO_BLK_T_IN outstanding together on $(stage4_target_fields)"
echo "  two disjoint chains of $CHAIN_LENGTH out of a $POOL_COUNT-descriptor pool — exactly two"
echo "  chains wide, so the pool held zero free descriptors once both were built"
echo "  heads $head_a and $head_b, in a queue of $queue descriptors — the allocation order"
echo "  this pool's initial FIFO gives, which is a determinism fact and not a"
echo "  concurrency one: a reclaim between the two allocations would produce it too"
echo "  both heads written into consecutive available-ring slots, then ONE store"
echo "  moved avail.idx 0 -> $avail; §2.7.13 step 5 increases it by the number of"
echo "  chain heads added, and two stores of +1 would let the device legally"
echo "  finish request A before the second one landed"
echo "  what proves they were outstanding together is the conjunction established"
echo "  immediately before that store: all six descriptors PREPARED and distinct,"
echo "  the pool free count 0, the consumer index still 0, both ring slots already"
echo "  holding the heads, one dma_publish before it, and no irq_wait, dma_consume"
echo "  or reclaim anywhere before it — then all six device-owned after it"
echo "  sectors $FIRST_SECTOR..$last_sector, each into its OWN 512-byte buffer poisoned with its"
echo "  own sentinel and checked byte for byte against 0xC0 + sector, so neither"
echo "  completion can be satisfied by the other request's bytes"
echo "  completion order observed: $first_done then $second_done — identity is used_elem.id"
echo "  matched against the outstanding heads, never the ring slot, because"
echo "  VIRTIO_F_IN_ORDER is not negotiated and either order is legal"
echo "  consumed used entries 0 -> $used, advancing once per entry and not per wake"
echo "  measurements, not gates: $counted MSI-X deliveries, $waits irq_wait returns,"
echo "  largest batch one consume made visible: $fresh, latched returns: $latched"
echo "  one wake exposed $fresh completed used-ring entries, exercising ADR-0082's"
echo "  batching property; this corroborates the drain-until-empty path and does"
echo "  NOT establish parallel device execution"
echo "  one 8 KiB DMA region, contiguous, capability_delta=1, one MSI-X source,"
echo "  one device initialization, one feature negotiation, one queue_enable"
echo "  all $POOL_COUNT descriptors free again and named once at the end"
echo "  claimed: both chains crossed the driver/device ownership boundary together,"
echo "  at the single avail.idx += 2 publication, and both completed, were"
echo "  associated by used_elem.id and were reclaimed per chain in whichever order"
echo "  the device produced them."
echo "  NOT claimed: that the device executed them at the same physical instant,"
echo "  parallel disk I/O, writes, a second queue, scheduling, a block service,"
echo "  or a driver framework."
