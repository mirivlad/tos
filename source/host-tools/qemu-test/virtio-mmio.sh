#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4B: canonical text reads the real VirtIO common configuration.
#
# The whole chain in one module, with nothing outside it supplying an answer:
#
#   root PCI bus authority  →  minted by the launcher, unobtainable otherwise
#   pci_function_claim      →  one function of the real machine
#   pci_config_read         →  the device's own capability list, walked in text
#   pci_bar_map_read        →  a bounded window on the BAR that list named
#   mmio_read_*             →  the registers behind it
#
# **The nucleus knows PCI and page mappings and nothing about VirtIO.** The gate
# checks that mechanically over ring-0 code, comments stripped.
#
#   bash host-tools/qemu-test/virtio-mmio.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-mmio}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-pci-discovery"

fail() { echo "virtio-mmio: FAIL: $*" >&2; exit 1; }

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-pci-discovery)
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
    exit 1
}
NUCLEUS="$TARGET/x86_64-unknown-none/release/tos-nucleus"

# Builds a capsule from one fixture and runs it, echoing what the module reported.
run() {
    local name="$1" fixture="$2" expect="$3"; shift 3
    local out="$OUT/$name"
    mkdir -p "$out"
    printf '/system/boot/init.tos\t%s/tests/vectors/%s/init.tos\n' "$ROOT" "$fixture" \
        > "$out/manifest.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$out/fixture.bin" --meta "$out/meta.json" "$out/manifest.txt" > /dev/null
    python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
        --capsule "$out/fixture.bin" --manifest "$out/meta.json" > /dev/null
    bash "$HERE/run.sh" --out "$out" --capsule "$out/fixture.bin" \
        --nucleus "$NUCLEUS" --expect "$expect" "$@" > /dev/null
    sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$out/events.log"
}

field() { printf '%s' "$(( ($1 >> $2) & $3 ))"; }

# --- the real device -----------------------------------------------------------
value="$(run probe virtio-mmio 33 --stage4-block-device)"
[ -n "$value" ] || fail "the probe reported nothing"
[ "$value" -gt 0 ] || fail "the probe reported a refusal: $value"

queues="$(field "$value" 0 65535)"
status="$(field "$value" 16 255)"
generation="$(field "$value" 24 255)"
queue_size="$(field "$value" 32 65535)"
features="$(field "$value" 48 32767)"

# The BAR the *device's own capability structure* named, and the window the
# nucleus derived from that BAR's measured extent. Neither number is the
# module's and neither is the harness's.
grep -q '^TOS\.RUN\.MMIO_MAPPED process=0 segment=0 bus=0 device=4 function=0 bar=4 offset=0 length=4096 access=read_only asserted_by=nucleus$' \
    "$OUT/probe/events.log" ||
    fail "the window was not derived from BAR 4 read-only: $(grep MMIO_MAPPED "$OUT/probe/events.log")"
[ "$(grep -c '^TOS\.RUN\.MMIO_MAPPED ' "$OUT/probe/events.log")" = 1 ] ||
    fail "more windows were mapped than the module asked for"

# `num-queues=1` is what the Stage 4 profile configures, and the device says so.
[ "$queues" = 1 ] || fail "num_queues is $queues, expected 1"
# Untouched: this probe sets no status bit and does not reset the device.
[ "$status" = 0 ] || fail "device_status is 0x$(printf %02X "$status"), expected 0x00"
[ "$generation" = 0 ] || fail "config_generation is $generation, expected 0"
# A real queue size, reported by the device rather than chosen here.
[ "$queue_size" -gt 0 ] || fail "queue 0 size is $queue_size"
[ $((queue_size & (queue_size - 1))) = 0 ] || fail "queue 0 size $queue_size is not a power of two"
# Feature bits: non-zero is what shows the common configuration is live rather
# than a window of zeros.
[ "$features" -gt 0 ] || fail "the device reported no features; the window is not live"

# --- without the device, the same probe fails ----------------------------------
# An absent function has no BARs, so no window can be derived from it and the
# module reports the refusal instead of a reading.
absent="$(run absent virtio-mmio 33)"
[ -n "$absent" ] || fail "the device-absent run reported nothing"
[ "$absent" -lt 0 ] || fail "the probe produced a reading with no device present: $absent"
[ "$(grep -c '^TOS\.RUN\.MMIO_MAPPED ' "$OUT/absent/events.log" || true)" = 0 ] ||
    fail "a window was mapped with no device present"

# --- eight mapping refusals, executed ------------------------------------------
#   1 BAR index out of range      2 unimplemented BAR is not authority
#   4 unaligned offset            8 unaligned length
#  16 zero length                32 past the BAR's extent
#  64 offset+length overflows   128 a function with no BARs cannot be mapped
negatives="$(run negatives virtio-mmio-negative 33 --stage4-block-device)"
[ "$negatives" = 255 ] || fail "the mapping negatives did not all hold: $negatives"
[ "$(grep -c '^TOS\.RUN\.MMIO_MAPPED ' "$OUT/negatives/events.log" || true)" = 0 ] ||
    fail "a refused request produced a window anyway"

# --- an access past its own window refuses before touching the device ----------
run bounds virtio-mmio-bounds 75 --stage4-block-device > /dev/null || true
grep -q 'code=RUNTIME_DEVICE_REFUSED' "$OUT/bounds/events.log" ||
    fail "an access past the mapping was not refused"
grep -q 'detail=a_device_access_past_the_end_of_its_mapping' "$OUT/bounds/events.log" ||
    fail "the refusal did not name the bound it broke"

# --- and the nucleus still knows nothing about VirtIO --------------------------
leaked="$(find "$ROOT/nucleus/src" -name '*.rs' -print0 |
    xargs -0 sed -e 's://.*::' -e 's:/\*.*\*/::' |
    grep -ni "virtio" || true)"
[ -z "$leaked" ] || fail "the nucleus code mentions VirtIO: $leaked"

echo "VIRTIO-MMIO PASS: canonical text read the real VirtIO common configuration"
echo "  the module walked the capability list, found BAR 4, and mapped one page"
echo "  read-only; the nucleus derived the physical base from the BAR it measured"
echo "  at claim time, and the module never named an address"
echo "  it read num_queues=$queues device_status=0x$(printf %02X "$status")" \
     "config_generation=$generation queue0_size=$queue_size features=0x$(printf %04X "$features")"
echo "  without the device the same module reports a refusal, not a reading"
echo "  eight mapping refusals hold, and an access past the window refuses"
echo "  before the device is touched"
