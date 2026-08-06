#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS Stage 1 QEMU boot test.
#
# Builds the capsule from system/boot/ sources, lays out an ESP (FAT32 via
# mtools), boots OVMF + QEMU, captures the serial boot-event log and the
# isa-debug-exit result code (RESULT_PORT = 0x501; QEMU exits with
# (code << 1) | 1, so HALT_OK=0x10 -> 33, CAPSULE_INVALID=0x21 -> 67).
#
# Usage:
#   bash host-tools/qemu-test/run.sh [OUT_DIR] [CAPSULE_FILE]
# CAPSULE_FILE overrides the capsule placed on the ESP (negative tests, e.g.
# tests/vectors/capsule-v1/invalid-kind-none.bin).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# Repository root (two more levels up from source/): identity gate resolves
# repo-relative paths against the current git commit.
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-test}"
CAPSULE_IN="${2:-}"
mkdir -p "$OUT"

TOOL="$ROOT/target/release/tos-capsule-tool"
LOADER="$ROOT/target/x86_64-unknown-uefi/release/tos-uefi-loader.efi"
NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS="${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}"

for f in "$TOOL" "$LOADER" "$NUCLEUS" "$OVMF_CODE" "$OVMF_VARS"; do
    [ -f "$f" ] || { echo "missing: $f"; exit 2; }
done

# --- 1. capsule ---
if [ -n "$CAPSULE_IN" ]; then
    cp "$CAPSULE_IN" "$OUT/capsule.bin"
    echo "capsule: $CAPSULE_IN (used as-is)"
else
    # Identity gate: the capsule is built from the current git commit (HEAD),
    # repo-relative paths, so tos-capsule-tool verifies that init.tos bytes
    # really are the bytes committed at HEAD (source->commit->digest link).
    printf '/system/boot/init.tos\tsource/system/boot/init.tos\n' > "$OUT/manifest.txt"
    ( cd "$GITROOT" && "$TOOL" --git-commit HEAD --licence source/system/boot/NOTICES.txt \
        --out "$OUT/capsule.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt" )
fi

# --- 2. ESP image (FAT32, 64 MiB) ---
ESP="$OUT/esp.img"
rm -f "$ESP"
dd if=/dev/zero of="$ESP" bs=1M count=64 status=none
mformat -F -i "$ESP" ::
mmd -i "$ESP" ::/EFI
mmd -i "$ESP" ::/EFI/BOOT
mcopy -i "$ESP" "$LOADER" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$ESP" "$OUT/capsule.bin" ::/capsule.bin
mcopy -i "$ESP" "$NUCLEUS" ::/nucleus.bin

# --- 3. boot ---
cp "$OVMF_CODE" "$OUT/OVMF_CODE.fd"
cp "$OVMF_VARS" "$OUT/OVMF_VARS.fd"
rm -f "$OUT/serial.log"
set +e
timeout 90 qemu-system-x86_64 \
    -machine q35 \
    -cpu qemu64 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file="$OUT/OVMF_CODE.fd" \
    -drive if=pflash,format=raw,file="$OUT/OVMF_VARS.fd" \
    -drive if=none,id=esp0,format=raw,file="$ESP" \
    -device ahci,id=ahci0 \
    -device ide-hd,bus=ahci0.0,drive=esp0 \
    -device isa-debug-exit \
    -serial file:"$OUT/serial.log" \
    -display none -no-reboot -monitor none \
    > "$OUT/qemu.stdout" 2> "$OUT/qemu.stderr"
RC=$?
set -e
echo "qemu exit code: $RC (HALT_OK=33, CAPSULE_INVALID=67, ABI_INVALID=69)"
echo "--- serial boot-event log ---"
cat "$OUT/serial.log" 2>/dev/null || echo "(empty serial.log)"
exit $RC