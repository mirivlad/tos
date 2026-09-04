#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4B: canonical text discovers the real VirtIO PCI capability structures.
#
# Stage 4A proved a textual module can read a device's configuration space under
# a capability. This is the next thing that must be true before any BAR or MMIO
# authority can exist: **the interpretation of what is in that space is textual
# too.** The nucleus performs a configuration transaction against the function a
# capability names; it does not know what a vendor-specific PCI capability is,
# that `cfg_type` 1 means the common configuration, or that this is a block
# device. All of that lives in `/system/boot/init.tos`.
#
# What this gate does *not* do is map anything. BAR and MMIO authority is the
# decision Stage 4B stopped on — see `docs/evidence/STAGE4B_MMIO_BOUNDARY.md`.
#
#   bash host-tools/qemu-test/virtio-caps.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-virtio-caps}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/virtio-caps"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-pci-discovery"

fail() {
    echo "virtio-caps: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-pci-discovery)
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
    exit 1
}

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/vcaps.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/vcaps.bin" --manifest "$OUT/capsule.meta.json"

observe() {
    local out="$1"; shift
    bash "$HERE/run.sh" \
        --out "$out" \
        --capsule "$OUT/vcaps.bin" \
        --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
        --expect 33 \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
        --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
        "$@" > /dev/null
    sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$out/events.log"
}

field() { printf '%s' "$(( ($1 >> $2) & $3 ))"; }

# --- the real modern device ----------------------------------------------------
value="$(observe "$OUT" --stage4-block-device)"
[ -n "$value" ] || fail "the module reported no observation"
[ "$value" -gt 0 ] || fail "the module reported a refusal: $value"

found="$(field "$value" 0 15)"
wellformed="$(field "$value" 4 1)"
bars="$(field "$value" 8 65535)"
common_offset="$(field "$value" 24 1048575)"
common_length="$(field "$value" 44 524287)"

# All four modern structures, identified by the textual parser from `cfg_type`.
[ "$found" = 15 ] ||
    fail "not all four VirtIO structures were identified: found=0x$(printf %X "$found")"
[ "$wellformed" = 1 ] ||
    fail "a structure had an out-of-range BAR index or a zero length"
# Every one of them in the same BAR, which is what a modern virtio-blk-pci does.
for shift in 0 4 8 12; do
    bar="$(( (bars >> shift) & 0xF ))"
    [ "$bar" = 4 ] || fail "a VirtIO structure names BAR $bar, expected 4"
done
# The common configuration's own extent, read out of the capability.
[ "$common_offset" = 0 ] ||
    fail "common configuration offset is 0x$(printf %X "$common_offset"), expected 0x0"
[ "$common_length" = 4096 ] ||
    fail "common configuration length is 0x$(printf %X "$common_length"), expected 0x1000"

# --- wrong transport: absence is reported, not invented ------------------------
# The same device class over the *transitional* transport. It has no modern
# VirtIO PCI capability structures at all, so a parser that had defaults to fall
# back on would report them anyway. This one reports none.
legacy="$(observe "$OUT/legacy" --stage4-block-device-legacy)"
[ -n "$legacy" ] || fail "the legacy-transport run reported nothing"
[ "$(field "$legacy" 0 15)" = 0 ] ||
    fail "modern VirtIO structures were reported for a transitional device: $legacy"

# --- no device at all ----------------------------------------------------------
# An absent function reads all-ones, so the capability pointer is 0xFF — outside
# the range a capability may begin at. The traversal refuses it rather than
# following it, and reports nothing found.
absent="$(observe "$OUT/absent")"
[ -n "$absent" ] || fail "the device-absent run reported nothing"
[ "$(field "$absent" 0 15)" = 0 ] ||
    fail "VirtIO structures were reported with no device present: $absent"
[ "$(field "$absent" 4 1)" = 0 ] ||
    fail "an all-ones capability pointer was treated as well-formed"

# --- the traversal is bounded --------------------------------------------------
# Every run above terminated, including the one whose chain pointer was 0xFF.
# The bound is in the module — 64 entries, which no well-formed chain can exceed
# in 256 bytes of configuration space — and not in a timeout.
grep -q '^TOS\.RUN\.COMPLETED ' "$OUT/absent/events.log" ||
    fail "the malformed-chain run did not terminate on its own"

# --- and the nucleus still knows nothing about VirtIO --------------------------
# The boundary this whole slice is about, checked mechanically rather than
# asserted: if a VirtIO constant ever appears in ring 0, the interpretation has
# moved out of canonical text.
# Comment lines are stripped first: a comment saying the nucleus does *not* know
# what a VirtIO device is documents the boundary rather than crossing it. What
# must not appear is VirtIO in the code — a constant, a type, a match arm.
leaked="$(find "$ROOT/nucleus/src" -name '*.rs' -print0 |
    xargs -0 sed -e 's://.*::' -e 's:/\*.*\*/::' |
    grep -ni "virtio" || true)"
[ -z "$leaked" ] || fail "the nucleus code mentions VirtIO: $leaked"

echo "VIRTIO-CAPS PASS: canonical text discovered the real VirtIO PCI structures"
echo "  all four identified from cfg_type: common, notify, ISR, device"
echo "  every one in BAR 4; common configuration at offset 0x0, length 0x1000"
echo "  a transitional-transport device reports none, and so does an absent one"
echo "  the traversal is bounded in the module, and the nucleus contains no VirtIO"
echo "  what is NOT here: BAR or MMIO authority. Stage 4B stopped on that decision;"
echo "  see docs/evidence/STAGE4B_MMIO_BOUNDARY.md"
