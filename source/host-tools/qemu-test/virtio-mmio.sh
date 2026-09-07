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
# Bit 8 is what **CPL 3** saw at the instant it first held the assignment, and it
# must be clear (ADR-0082 §5d): the claim discards whatever firmware left before
# any capability naming the assignment exists. What the firmware actually left is
# not observable from a module — that is the point — and the nucleus states it.
[ $(( bme >> 8 )) = 0 ] ||
    fail "a claimed function reached CPL 3 already bus-mastering"
grep -q '^TOS\.RUN\.PCI_NORMALISED .* device=4 function=0 found_memory_space=1 found_bus_master=1 ' \
    "$OUT/bme-precision/events.log" ||
    fail "the nucleus did not report discarding the firmware's enable state: $(grep PCI_NORMALISED "$OUT/bme-precision/events.log")"

# --- an access past its own window refuses before touching the device ----------
run bounds virtio-mmio-bounds 75 --stage4-block-device > /dev/null || true
grep -q 'code=RUNTIME_DEVICE_REFUSED' "$OUT/bounds/events.log" ||
    fail "an access past the mapping was not refused"
grep -q 'detail=a_device_access_past_the_end_of_its_mapping' "$OUT/bounds/events.log" ||
    fail "the refusal did not name the bound it broke"

# --- conventional MSI is reserved too ------------------------------------------
# ADR-0082 §5c. The authority rule is about interrupts, not about a transport, so
# a device offering conventional MSI must not offer an ambient route around
# `platform.irq.Source`. The reference VirtIO function has no MSI capability, so
# this claims a function of the **same machine** that does: the q35 chipset's own
# SATA controller at 00:1f.2. Nothing is invented and no MSI delivery path is
# built — a refusal needs no backend.
#
#   1 message control   2 message address   4 message data
#   8 the capability is still readable     16 an unrelated field is still writable
#  32 MSI Enable is off when CPL 3 first sees the function
msi="$(run msi-reserved pci-msi-reserved 33 --stage4-block-device)"
[ -n "$msi" ] || fail "the MSI reservation probe reported nothing"
[ "$msi" != "-2" ] || fail "the q35 SATA controller reported no MSI capability"
[ "$msi" = 63 ] || fail "the MSI reservation did not hold: $msi"
# **Derived, not blanket.** MSI's structure is 10, 14, 20 or 24 bytes depending
# on two bits of its own Message Control, and a nucleus reserving the maximum
# would refuse writes to whatever capability follows a shorter one. This function
# reports a 64-bit address and no per-vector masking, so 14 is the derived
# answer and 24 would be the blanket one.
grep -q '^TOS\.RUN\.PCI_NORMALISED .* device=31 function=2 .* msi=disabled msi_bytes=14 ' \
    "$OUT/msi-reserved/events.log" ||
    fail "the MSI extent was not derived from the capability's own control word: $(grep 'device=31' "$OUT/msi-reserved/events.log")"

# --- the two enable predicates are independent ---------------------------------
# ADR-0082 §5b and §5d are two rules, not one spelling of one. An `MmioRegion` is
# a memory-decoding descendant and **not** a bus-mastering one, so mapping a
# window must turn memory decoding on and leave bus mastering exactly where it
# was. A nucleus that had merged the predicates would set both here, and would
# then be wrong about a DMA mapping — which needs the other one and not this one.
grep -q '^TOS\.RUN\.PCI_ENABLES .* device=4 function=0 memory_decoding=1 bus_mastering=0 memory_space=1 bus_master=0 asserted_by=nucleus$' \
    "$OUT/probe/events.log" ||
    fail "mapping a window did not move memory decoding alone: $(grep PCI_ENABLES "$OUT/probe/events.log")"
# Anchored on the leading space so this does not match `found_bus_master=1`,
# which is the *firmware's* state on the normalise line and is expected to be 1.
[ "$(grep -c ' bus_master=1' "$OUT/probe/events.log" || true)" = 0 ] ||
    fail "a run that mapped only a window enabled bus mastering"
# And the last memory-decoding descendant going takes memory decoding with it.
# The probe dies holding its window, so this is the process-death path.
grep -q '^TOS\.RUN\.PCI_ENABLES .* memory_decoding=0 bus_mastering=0 memory_space=0 bus_master=0 asserted_by=nucleus$' \
    "$OUT/probe/events.log" ||
    fail "the last descendant going did not clear memory decoding"

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
echo "  this machine's firmware hands TOS a function with memory decoding **and**"
echo "  bus mastering already on; the claim discards both before any capability"
echo "  naming the assignment exists, and the module never sees them set"
echo "  conventional MSI is reserved on the one function of this machine that has"
echo "  it, and its extent is derived from its own control word — 14 bytes, where"
echo "  a blanket reservation would have said 24 and spilled into its neighbour"
echo "  and the two enable predicates move independently: a window turns memory"
echo "  decoding on and leaves bus mastering alone"
echo "  and the memory account is exactly where it started: $available frames"
echo "  available against $pool endowed, so a device window is neither charged"
echo "  to the pool nor credited back to it"
