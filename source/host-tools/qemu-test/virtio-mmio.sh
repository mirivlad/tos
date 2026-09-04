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

# --- the three ambient paths to interrupt authority are closed -----------------
# ADR-0082 §5. The MSI-X table is in a BAR and the MSI-X capability and the Bus
# Master Enable bit are in the first 256 bytes of configuration space, so a
# holder of `map` and `config_write` reached all three before this. Each is now
# refused, and the two neighbouring accesses still work — a refusal that refused
# everything would prove nothing.
#
#   1 a writable window on the MSI-X table BAR    2 a read-only one, same page
#   4 the MSI-X message control                   8 setting Bus Master Enable
#  16 the capability is still readable           32 a no-change command write works
msix="$(run msix-negative virtio-msix-negative 33 --stage4-block-device)"
[ -n "$msix" ] || fail "the MSI-X refusal probe reported nothing"
[ "$msix" != "-2" ] || fail "the reference device reported no MSI-X capability"
[ "$msix" = 63 ] || fail "the MSI-X authority refusals did not all hold: $msix"
[ "$(grep -c '^TOS\.RUN\.MMIO_MAPPED ' "$OUT/msix-negative/events.log" || true)" = 0 ] ||
    fail "a window was mapped over the MSI-X structures"

# --- and the Bus Master refusal is about one bit of one byte -------------------
# ADR-0082 §5's narrowing must be exactly that and no wider. BME is bit 2 of the
# byte at 0x04; the rest of the Command register is a driver's ordinary business.
# The first form of this rule compared bit 2 of *whatever byte was written*
# against the register's BME, so a legal one-byte write at 0x05 — whose bit 2 is
# INTx Disable — was refused for a bit it does not contain.
#
#   1 w1 @0x04 setting BME            2 w1 @0x04 leaving it       4 w1 @0x05
#   8 w2 @0x04 setting BME           16 w2 @0x04 other bits only
#  32 w4 @0x04 setting BME           64 w4 @0x04 leaving it
# 128 the device really did change where allowed, and did not where refused
bme="$(run bme-precision pci-bme-precision 33 --stage4-block-device)"
[ -n "$bme" ] || fail "the Bus Master precision probe reported nothing"
[ "$bme" -ge 0 ] || fail "the Bus Master precision probe reported a refusal: $bme"
[ $(( bme & 255 )) = 255 ] || fail "the Bus Master refusal is not bit-precise: $bme"
# Bit 8 carries what the *firmware* left, which is a fact about the machine
# rather than a property of the rule — the rule is about a change in either
# direction. It is recorded because whether TOS is handed a function that is
# already a bus master is the initial-state question the lifecycle must answer.
firmware_bme=$(( bme >> 8 ))

# --- an access past its own window refuses before touching the device ----------
run bounds virtio-mmio-bounds 75 --stage4-block-device > /dev/null || true
grep -q 'code=RUNTIME_DEVICE_REFUSED' "$OUT/bounds/events.log" ||
    fail "an access past the mapping was not refused"
grep -q 'detail=a_device_access_past_the_end_of_its_mapping' "$OUT/bounds/events.log" ||
    fail "the refusal did not name the bound it broke"

# --- a device window is not ordinary RAM, coming or going ----------------------
# **The account is exactly where it started.** The probe mapped a device window
# and died holding it; if a device mapping were charged to the pool the pool
# would be short, and if releasing one credited the pool it would be over. Both
# are the mistake ADR-0081 §5 exists to prevent, and this is where a regression
# in either direction shows up as a number.
account="$(grep -m1 '^TOS\.MEM\.ACCOUNT ' "$OUT/probe/events.log")"
reclaimed="$(grep -m1 '^TOS\.RUN\.PROCESS_RECLAIMED ' "$OUT/probe/events.log")"
value_of() { printf '%s' "$1" | tr ' ' '\n' | sed -n "s/^$2=\(.*\)$/\1/p"; }
pool="$(value_of "$account" pool_frames)"
reserve_free="$(value_of "$account" table_reserve_free)"
available="$(value_of "$reclaimed" available)"
tables_free="$(value_of "$reclaimed" tables_free)"
[ -n "$pool" ] && [ -n "$available" ] || fail "the run reported no memory account"
[ "$available" = "$pool" ] ||
    fail "a device mapping changed the pool: $available available, $pool endowed"
[ "$tables_free" = "$reserve_free" ] ||
    fail "a device mapping leaked page tables: $tables_free free, $reserve_free reserved"

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
echo "  the three ambient paths to interrupt authority are closed: no window over"
echo "  the MSI-X table in either form, no write to the MSI-X capability, and no"
echo "  write that would set Bus Master Enable — while reading the capability and"
echo "  writing the command register unchanged both still work"
echo "  and that last refusal is one bit of one byte: all seven width/offset cases"
echo "  hold, including a one-byte write at 0x05 whose own bit 2 is INTx Disable"
if [ "$firmware_bme" = 1 ]; then
    echo "  measured, and load-bearing for the BME lifecycle: this machine's firmware"
    echo "  hands TOS a function that is **already bus-mastering**, so the claim path"
    echo "  cannot assume a clear bit"
else
    echo "  measured: this machine's firmware left Bus Master Enable clear"
fi
echo "  and the memory account is exactly where it started: $available frames"
echo "  available against $pool endowed, so a device window is neither charged"
echo "  to the pool nor credited back to it"
