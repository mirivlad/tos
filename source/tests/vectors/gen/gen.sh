#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regenerate the golden capsule vectors from system/boot/ sources.
# Run from source/:  bash tests/vectors/gen/gen.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GEN="$ROOT/tests/vectors/gen"
OUT="$ROOT/tests/vectors/capsule-v1"
TOOL="$ROOT/target/debug/tos-capsule-tool"
IDENT="4242424242424242424242424242424242424242424242424242424242424242"

mkdir -p "$OUT"

# --- content files (trailing newline is part of the pinned capsule) ---
printf '# TOS boot text\nprint("hello from boot")\n' > "$GEN/.init.txt"
printf '0.2.1\n' > "$GEN/.version.txt"

# --- valid-001 : canonical boot + system/version ---
printf '/system/boot/init.tos\t%s\n/system/version\t%s\n' "$GEN/.init.txt" "$GEN/.version.txt" > "$GEN/.valid.manifest"
$TOOL --identity "$IDENT" --out "$OUT/valid-001.bin" --meta "$GEN/.valid.meta" "$GEN/.valid.manifest"

# --- invalid-missing-boot : no canonical boot file ---
printf '/system/version\t%s\n' "$GEN/.version.txt" > "$GEN/.missing.manifest"
$TOOL --identity "$IDENT" --out "$OUT/invalid-missing-boot.bin" "$GEN/.missing.manifest"

# --- invalid-traversal : path escapes root ---
printf '/system/boot/init.tos\t%s\n/system/../etc/passwd\t%s\n' "$GEN/.init.txt" "$GEN/.version.txt" > "$GEN/.traversal.manifest"
$TOOL --identity "$IDENT" --out "$OUT/invalid-traversal.bin" "$GEN/.traversal.manifest"

# --- invalid-dup : duplicate canonical path ---
printf '/system/boot/init.tos\t%s\n/system/boot/init.tos\t%s\n' "$GEN/.init.txt" "$GEN/.init.txt" > "$GEN/.dup.manifest"
$TOOL --identity "$IDENT" --out "$OUT/invalid-dup.bin" "$GEN/.dup.manifest"

# --- invalid-badmagic : magic byte flipped ---
cp "$OUT/valid-001.bin" "$OUT/invalid-badmagic.bin"
printf '\xff' | dd of="$OUT/invalid-badmagic.bin" bs=1 seek=0 conv=notrunc status=none

# --- invalid-truncated : valid minus 1 byte ---
head -c -1 "$OUT/valid-001.bin" > "$OUT/invalid-truncated.bin"

# --- invalid-kind-none : source_identity_kind forced to 0 (offset 96) ---
cp "$OUT/valid-001.bin" "$OUT/invalid-kind-none.bin"
printf '\x00' | dd of="$OUT/invalid-kind-none.bin" bs=1 seek=96 conv=notrunc status=none

echo "vectors regenerated:"
ls -1 "$OUT"/*.bin