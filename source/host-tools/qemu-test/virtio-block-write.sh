#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4D-5: the first real block write, and a read-back that proves it.
#
# Every Stage 4D boot so far has read. This one writes 512 bytes the module
# composed to sector 5 with `VIRTIO_BLK_T_OUT`, scrubs the buffer it wrote them
# from, and reads the same sector back into a DIFFERENT poisoned buffer with
# `VIRTIO_BLK_T_IN` — so the bytes it finds can only have come back from the
# device.
#
# **The status byte is not the evidence.** §2.7.5.1 says a device "SHOULD NOT
# read a device-writable buffer", so a payload descriptor wrongly marked
# writable would leave the disk untouched and still report VIRTIO_BLK_S_OK. What
# proves the device consumed the module's bytes is the independent read-back.
#
# Capacity is read from the device before anything is published: the module
# walks the capability list for VIRTIO_PCI_CAP_DEVICE_CFG and reads the 64-bit
# `capacity` under §2.5.1's generation protocol, because §5.2.6.1 says "A driver
# MUST NOT submit a request which would cause a read or write beyond capacity."
#
#   Virtual I/O Device (VIRTIO) Version 1.4
#   Committee Specification 01, 8 April 2026
#
#   bash host-tools/qemu-test/virtio-block-write.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
. "$HERE/stage4-profile.sh"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-block-write}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
VECTOR="$ROOT/tests/vectors/virtio-block-write/init.tos"

fail() { echo "virtio-block-write: FAIL: $*" >&2; exit 1; }

# --- what the module reports ---------------------------------------------------
#
# Twenty-nine facts, one bit each, and three measurements packed above them:
# 29 bits of mask, then 16 of capacity in sectors, 3 of generation attempts and
# 8 of queue size.
PROVED_ALL=536870911
CAPACITY_SHIFT=536870912
TRIES_SHIFT=35184372088832
QUEUE_SHIFT=281474976710656

# The sector this boot writes, the payload rule, and the disk the profile makes.
TARGET_SECTOR=5
PATTERN_BASE=92
PATTERN_STEP=7
IMAGE_SECTORS=32768
SEEDED=4
UNDERSIZED_SECTORS=4

FACTS="reset:1 version_1:2 features_ok:4 queue_exists:8 size_accepted:16
 layout_fits:32 alignment:64 queue_addresses_read_back:128 msix_accepted:256
 enabled:512 driver_ok:1024 notify_found:2048 wrap_arithmetic:4096
 pool_ready:8192 device_cfg_found:16384 capacity_generation_stable:32768
 capacity_admits_target_sector:65536 write_and_read_buffers_disjoint:131072
 write_payload_descriptor_is_device_readable:262144
 write_used_id_is_the_chain_head:524288 write_used_len_is_one:1048576
 write_status_ok:2097152 write_chain_reclaimed:4194304
 write_payload_scrubbed:8388608 read_used_id_is_the_chain_head:16777216
 read_used_len_is_513:33554432 read_status_ok:67108864
 read_payload_exact:134217728 pool_whole:268435456"

judge() {
    local value="$1"
    local proved capacity tries queue

    if [ "$value" -le 0 ]; then
        echo "  the module reported $value; a negative names the step that did not hold"
        return 1
    fi
    proved=$((value % CAPACITY_SHIFT))
    capacity=$(((value / CAPACITY_SHIFT) % 65536))
    tries=$(((value / TRIES_SHIFT) % 8))
    queue=$(((value / QUEUE_SHIFT) % 256))

    if [ "$proved" != "$PROVED_ALL" ]; then
        echo "  the module proved $proved of $PROVED_ALL"
        for pair in $FACTS; do
            bit="${pair##*:}"
            [ $((proved & bit)) -ne 0 ] || echo "    missing: ${pair%%:*}"
        done
        return 1
    fi
    # The device this profile presents is 16 MiB, so the capacity it reports is
    # 32768 sectors — but the module is not required to see that number, only a
    # number that admits the sector it writes. Both are checked, separately.
    if [ "$capacity" -le "$TARGET_SECTOR" ]; then
        echo "  the device reported a capacity of $capacity sectors, which does not"
        echo "  admit sector $TARGET_SECTOR; the module should have refused"
        return 1
    fi
    if [ "$capacity" != "$IMAGE_SECTORS" ]; then
        echo "  the device reported $capacity sectors for a $((IMAGE_SECTORS / 2048)) MiB image"
        return 1
    fi
    if [ "$tries" -lt 1 ]; then
        echo "  the module read the capacity in $tries generation attempts"
        return 1
    fi
    if [ "$queue" -lt 4 ]; then
        echo "  the driver settled on a queue of $queue descriptors"
        return 1
    fi
    return 0
}

self_test() {
    local complete refused
    complete=$((PROVED_ALL + IMAGE_SECTORS * CAPACITY_SHIFT + 1 * TRIES_SHIFT \
        + 128 * QUEUE_SHIFT))
    judge "$complete" || fail "the judgement refuses a result it must accept"

    # A run that never proved the read-back matched. Everything else holds, and
    # this is the fact the whole slice exists for.
    refused=$((PROVED_ALL - 134217728 + IMAGE_SECTORS * CAPACITY_SHIFT \
        + 1 * TRIES_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a run whose read-back did not match the payload is accepted"

    # A run whose write payload descriptor was device-writable: the device is
    # told not to read it, so the status means nothing.
    refused=$((PROVED_ALL - 262144 + IMAGE_SECTORS * CAPACITY_SHIFT \
        + 1 * TRIES_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null ||
        fail "a device-writable write payload is accepted"

    # A write that reported the read's used length.
    refused=$((PROVED_ALL - 1048576 + IMAGE_SECTORS * CAPACITY_SHIFT \
        + 1 * TRIES_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null || fail "a write with the wrong used.len is accepted"

    # A run that never scrubbed the payload, so the read-back could have been
    # answered by memory.
    refused=$((PROVED_ALL - 8388608 + IMAGE_SECTORS * CAPACITY_SHIFT \
        + 1 * TRIES_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null || fail "an unscrubbed payload buffer is accepted"

    # A device whose reported capacity does not admit the sector written.
    refused=$((PROVED_ALL + 3 * CAPACITY_SHIFT + 1 * TRIES_SHIFT + 128 * QUEUE_SHIFT))
    ! judge "$refused" >/dev/null || fail "a capacity that excludes the target sector is accepted"

    # And the module's own refusal shape.
    ! judge -44 >/dev/null || fail "a negative completion value is accepted"
}

# --- what the vector's own text must show ---------------------------------------
#
# Two properties no runtime observation can establish, checked against the source
# rather than asserted by the program.
source_shape() {
    # **No callable value.** `TypeDef::Function` erases a parameter's `PassMode`,
    # a recorded open item; the only way to hold such a value is a closure, and
    # Stage 4 vectors are `profile full`, so nothing in the profile forbids one.
    ! grep -nE '(^|[^_[:alnum:]])fn[[:space:]]*\(' "$VECTOR" ||
        fail "the vector declares a function type or a closure, which reaches the callable PassMode gap"

    # **The write payload descriptor carries no write flag.** The chain is built
    # from a `data_flags` the loop selects, and the write branch must leave it at
    # NEXT alone. A grep is not the proof — the module reads the flags back out
    # of the descriptor table — but a vector that wrote `DESC_F_WRITE` into the
    # write branch would be a different program and is refused here first.
    grep -q 'let mut data_flags: u64 = DESC_F_NEXT;' "$VECTOR" ||
        fail "the write branch does not build a device-readable payload descriptor"

    # **The two buffers are separate declarations.** Aliasing them would make the
    # read-back satisfiable from memory, and the module also checks the extents
    # at run time.
    grep -q 'let write_data: size = align_up(write_header + HEADER_BYTES, 16B);' "$VECTOR" ||
        fail "the write payload is not laid out where this gate expects"
    grep -q 'let read_data: size = align_up(read_header + HEADER_BYTES, 16B);' "$VECTOR" ||
        fail "the read-back buffer is not laid out where this gate expects"

    # **And the read branch must actually use it.** Laying two buffers out and
    # then pointing the read at the write's is an alias the module's own
    # disjointness check cannot see — it compares the two offsets, which stay
    # different — and the scrub makes it harmless rather than detectable. The
    # claim says the read-back goes into independent storage, so the source is
    # required to say so.
    grep -q 'data_offset = read_data;' "$VECTOR" ||
        fail "the read branch does not read into the independent buffer"
    grep -q 'data_address = read_data_address;' "$VECTOR" ||
        fail "the read branch does not point the device at the independent buffer"
    grep -q 'status_offset = read_status;' "$VECTOR" ||
        fail "the read branch does not use its own status byte"

    # **`dma_consume` precedes every read of what the device wrote**, and this
    # has to be checked against the text because no observation on this profile
    # distinguishes it. Measured: removing the call leaves the boot reporting a
    # byte-identical correct result, because ADR-0086 §11 lowers it to a
    # compiler and execution barrier in ring 3 and x86 under QEMU makes the
    # device's writes visible anyway. A hardware race is not deterministically
    # producible here, so it is not offered as evidence — the source is.
    python3 - "$VECTOR" <<'    CHECK' || fail "the vector reads the used ring before dma_consume"
import sys
text = open(sys.argv[1], encoding="utf-8").read()
consume = text.find("dma_consume(region);")
used = text.find("read_le16(borrow region, used_offset")
status = text.find("region[status_offset] as u64")
sys.exit(0 if 0 <= consume < used and consume < status else 1)
    CHECK

    # **A published payload is not written again.** Once the chain is device
    # owned the driver may not touch its memory, and the only writes into the
    # write payload's extent are the scrub after its chain came back — two
    # references, the store and the check that it took. Enforced here rather
    # than by a timing-sensitive race, which would not be valid evidence.
    payload_writes="$(grep -c 'region\[write_data' "$VECTOR" || true)"
    [ "$payload_writes" = "2" ] ||
        fail "the vector touches the write payload extent $payload_writes times, not the two the scrub needs"
}

# The reference image this profile builds, plus sector 5 holding the payload:
# what the backing file must look like afterwards, byte for byte.
expected_image() {
    local path="$1"
    dd if=/dev/zero of="$path" bs=1M count=16 status=none
    local sector byte
    sector=1
    while [ "$sector" -le "$SEEDED" ]; do
        byte="$(printf '\\%o' $((0xC0 + sector)))"
        head -c 512 /dev/zero | tr '\000' "$byte" |
            dd of="$path" bs=512 seek="$sector" conv=notrunc status=none
        sector=$((sector + 1))
    done
    python3 -c "
import sys
path, base, step, target = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
payload = bytes((base + step * i) % 256 for i in range(512))
with open(path, 'r+b') as image:
    image.seek(target * 512)
    image.write(payload)
" "$path" "$PATTERN_BASE" "$PATTERN_STEP" "$TARGET_SECTOR"
}

BUILT=""
cleanup() { [ -z "$BUILT" ] || rm -rf $BUILT; }
trap cleanup EXIT

self_test
source_shape

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

printf '/system/boot/init.tos\t%s/init.tos\n' "$ROOT/tests/vectors/virtio-block-write" \
    > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/write.bin" --meta "$OUT/write.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/write.bin" --manifest "$OUT/write.meta.json" >/dev/null

NUCLEUS="$(nucleus_for test-dma-driver)"

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/write.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

EVENTS="$OUT/live/events.log"
IMAGE="$OUT/live/stage4-block.img"
value="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$EVENTS")"
[ -n "$value" ] || fail "the boot reported no completion at all"

if ! judge "$value"; then
    fail "the write was not proved"
fi
proved=$((value % CAPACITY_SHIFT))
capacity=$(((value / CAPACITY_SHIFT) % 65536))
tries=$(((value / TRIES_SHIFT) % 8))
queue=$(((value / QUEUE_SHIFT) % 256))

# --- the host's own look at the disk -------------------------------------------
#
# **A target-and-location corroboration, and not a durability claim.** What it
# catches is a write that landed on another sector, a guest that wrote and read
# back the same wrong sector — which its own witness cannot see — and any
# collateral change. What it does not say is anything about when or whether the
# bytes were committed to host storage; that needs a different evidence design
# and this slice does not attempt it.
[ -f "$IMAGE" ] || fail "the run left no backing image to inspect"
expected_image "$OUT/expected.img"
cmp -s "$OUT/expected.img" "$IMAGE" ||
    fail "the backing image is not the reference image with sector $TARGET_SECTOR rewritten"

# Stated separately, so the two facts are two facts: the target sector holds the
# payload, and nothing else moved.
written="$(dd if="$IMAGE" bs=512 skip="$TARGET_SECTOR" count=1 status=none | sha256sum | awk '{print $1}')"
wanted="$(dd if="$OUT/expected.img" bs=512 skip="$TARGET_SECTOR" count=1 status=none | sha256sum | awk '{print $1}')"
[ "$written" = "$wanted" ] || fail "sector $TARGET_SECTOR does not hold the payload"
zeros="$(head -c 512 /dev/zero | sha256sum | awk '{print $1}')"
[ "$written" != "$zeros" ] || fail "sector $TARGET_SECTOR is still the zero fill it started as"

# --- what the nucleus witnessed ------------------------------------------------
deliveries="$(grep -c "TOS.RUN.IRQ_DELIVERED " "$EVENTS" || true)"
[ "$deliveries" -ge 2 ] ||
    fail "the device delivered $deliveries interrupts for a write and a read"
assigned="$(grep -c "TOS.RUN.PCI_ASSIGNED " "$EVENTS" || true)"
[ "$assigned" = "1" ] || fail "the boot took $assigned assignments of the endpoint"
regions="$(grep -c "TOS.RUN.DMA_REGION " "$EVENTS" || true)"
[ "$regions" = "1" ] || fail "the boot allocated $regions DMA regions"
grep -q "TOS.RUN.DMA_REGION .*bytes=8192 " "$EVENTS" ||
    fail "the boot did not allocate the 8 KiB region two request blocks need"
grep -q "TOS.RUN.DMA_REGION .*capability_delta=1 " "$EVENTS" ||
    fail "operation 30 did not add exactly one capability-table entry"
grep -q "TOS.RUN.PCI_ASSIGNED .*express=1 " "$EVENTS" ||
    fail "P1 does not hold: the target endpoint reports no PCI Express capability"
grep -q "TOS.RUN.PCI_ASSIGNED .*dma=1 " "$EVENTS" ||
    fail "P5 does not hold: the profile did not qualify the target endpoint"

# --- the capacity negative, on a device too small to hold the sector -----------
#
# **The same module, the same capsule, a smaller disk.** It must refuse before it
# publishes anything, and the proof that it did is the nucleus's own interrupt
# count: no notification means no completion means no delivery. A module cannot
# write that line and cannot suppress it.
SMALL="$OUT/undersized"
mkdir -p "$SMALL"
bash "$HERE/run.sh" \
    --out "$SMALL/live" \
    --capsule "$OUT/write.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --stage4-block-sectors "$UNDERSIZED_SECTORS" \
    --expect 33 \
    --require "TOS.RUN.COMPLETED" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.TRAP" \
    > /dev/null
small_value="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$SMALL/live/events.log")"
[ "$small_value" = "-44" ] ||
    fail "a $UNDERSIZED_SECTORS-sector device gave $small_value, not the capacity refusal -44"
small_deliveries="$(grep -c "TOS.RUN.IRQ_DELIVERED " "$SMALL/live/events.log" || true)"
[ "$small_deliveries" = "0" ] ||
    fail "the refused boot still caused $small_deliveries interrupts, so a request reached the device"
judge "$small_value" >/dev/null &&
    fail "the judgement accepts the undersized-device refusal"

echo "virtio-block-write: one VIRTIO_BLK_T_OUT of sector $TARGET_SECTOR, proved by an independent read-back, on $(stage4_target_fields)"
echo "  the device's own capacity was read before anything was published: the"
echo "  VIRTIO_PCI_CAP_DEVICE_CFG capability was found by walking the real list,"
echo "  its form, BAR, offset alignment and length were validated, and the 64-bit"
echo "  capacity was read under §2.5.1's generation protocol in $tries attempt(s)"
echo "  capacity $capacity sectors, which admits sector $TARGET_SECTOR — §5.2.6.1 forbids a"
echo "  request beyond it, and this is the first Stage 4 vector to derive that"
echo "  bound from the device rather than from the profile's known disk"
echo "  write:  T_OUT, sector $TARGET_SECTOR, 512 bytes (0x5C + 7*i) mod 256, payload"
echo "          descriptor device-readable and read back out of the table to prove"
echo "          it, used.len == 1, status VIRTIO_BLK_S_OK, chain reclaimed"
echo "  then the payload buffer was scrubbed to zero, so the pattern existed"
echo "  nowhere in this machine's memory when the read-back was built"
echo "  read:   T_IN, sector $TARGET_SECTOR, into a SEPARATE buffer poisoned 0x3C,"
echo "          used.len == 513, status OK, and all 512 bytes recomputed from the"
echo "          index rather than compared against a copy of what was sent"
echo "  queue of $queue descriptors, a four-descriptor pool, three per request, the"
echo "  write's chain reclaimed and reused for the read — Stage 4D-3's substrate"
echo "  one 8 KiB DMA region, capability_delta=1, one MSI-X source, $deliveries deliveries"
echo "  host corroboration: the backing image is the reference image with sector"
echo "  $TARGET_SECTOR rewritten and no other byte changed — a target-and-location check,"
echo "  NOT a durability claim"
echo "  negative: the same capsule on a $UNDERSIZED_SECTORS-sector device refuses with -44 before"
echo "  publishing anything, and the nucleus counted 0 interrupts, so no request"
echo "  reached the device"
echo "  claimed: the textual driver composed 512 bytes, a real VirtIO block device"
echo "  consumed them, and an independent read through the same device returned"
echo "  exactly those bytes."
echo "  NOT claimed: crash durability, host-storage persistence, fsync semantics,"
echo "  persistence across QEMU termination, a flush implementation, a block"
echo "  service or a filesystem."
