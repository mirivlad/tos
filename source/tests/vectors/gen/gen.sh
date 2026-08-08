#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regenerate the golden capsule vectors from system/boot/ sources.
# Run from source/:  bash tests/vectors/gen/gen.sh
#
# valid-001 is built from the REAL source/system/boot/init.tos (not a
# temporary placeholder) plus the real NOTICES.txt as the licence tail. The
# detached identities are calculated from canonical paths and content digests
# (ADR-0018); real provenance builds use --git-commit.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GEN="$ROOT/tests/vectors/gen"
OUT="$ROOT/tests/vectors/capsule-v1"
TOOL="$ROOT/target/release/tos-capsule-tool"
INIT="$ROOT/system/boot/init.tos"
NOTICES="$ROOT/system/boot/NOTICES.txt"

[ -f "$TOOL" ] || { echo "missing tool: $TOOL (cargo build -p tos-capsule-tool)"; exit 2; }
[ -f "$INIT" ] || { echo "missing source: $INIT"; exit 2; }
[ -f "$NOTICES" ] || { echo "missing source: $NOTICES"; exit 2; }

mkdir -p "$OUT"

# --- content files (trailing newline is part of the pinned capsule) ---
printf '0.2.1\n' > "$GEN/.version.txt"

# --- valid-001 : canonical boot (real init.tos) + system/version + licence ---
printf '/system/boot/init.tos\t%s\n/system/version\t%s\n' "$INIT" "$GEN/.version.txt" > "$GEN/.valid.manifest"
$TOOL --detached --licence "$NOTICES" \
    --out "$OUT/valid-001.bin" --meta "$GEN/.valid.meta" "$GEN/.valid.manifest"

# --- invalid-missing-boot : no canonical boot file ---
printf '/system/version\t%s\n' "$GEN/.version.txt" > "$GEN/.missing.manifest"
$TOOL --detached --out "$OUT/invalid-missing-boot.bin" "$GEN/.missing.manifest"

# --- invalid-traversal : path escapes root ---
printf '/system/boot/init.tos\t%s\n/system/../etc/passwd\t%s\n' "$INIT" "$GEN/.version.txt" > "$GEN/.traversal.manifest"
$TOOL --detached --out "$OUT/invalid-traversal.bin" "$GEN/.traversal.manifest"

# --- invalid-dup : duplicate canonical path ---
printf '/system/boot/init.tos\t%s\n/system/boot/init.tos\t%s\n' "$INIT" "$INIT" > "$GEN/.dup.manifest"
$TOOL --detached --out "$OUT/invalid-dup.bin" "$GEN/.dup.manifest"

# --- invalid-badmagic : magic byte flipped ---
cp "$OUT/valid-001.bin" "$OUT/invalid-badmagic.bin"
printf '\xff' | dd of="$OUT/invalid-badmagic.bin" bs=1 seek=0 conv=notrunc status=none

# --- invalid-truncated : valid minus 1 byte ---
head -c -1 "$OUT/valid-001.bin" > "$OUT/invalid-truncated.bin"

# --- invalid-kind-none : source_identity_kind forced to 0 (offset 96) ---
cp "$OUT/valid-001.bin" "$OUT/invalid-kind-none.bin"
printf '\x00' | dd of="$OUT/invalid-kind-none.bin" bs=1 seek=96 conv=notrunc status=none

# Offsets below follow CAPSULE_FORMAT_V1.md §3 for valid-001: two paths, two
# files. path_tbl_offset=184, name arena "/system/boot/init.tos"+"/system/version"
# (36 bytes), file_tbl_offset=252, payload_offset=380, licence at payload end.
# Read the real offsets from the header so the dd patches stay correct even if
# the source files change length.
PATH_TBL_OFF=$(od -An -tu8 -j40 -N8 "$OUT/valid-001.bin" | tr -d ' ')
FILE_TBL_OFF=$(od -An -tu8 -j56 -N8 "$OUT/valid-001.bin" | tr -d ' ')

# --- invalid-file-reserved : 12-byte reserved block of file entry 0 set ---
cp "$OUT/valid-001.bin" "$OUT/invalid-file-reserved.bin"
printf '\x01' | dd of="$OUT/invalid-file-reserved.bin" bs=1 seek=$((FILE_TBL_OFF + 52)) conv=notrunc status=none

# --- invalid-path-flag : licence-notice bit set on a path entry (flags@+12) ---
cp "$OUT/valid-001.bin" "$OUT/invalid-path-flag.bin"
printf '\x02' | dd of="$OUT/invalid-path-flag.bin" bs=1 seek=$((PATH_TBL_OFF + 12)) conv=notrunc status=none

# --- invalid-dup-file-index : both paths reference file 0 ---
cp "$OUT/valid-001.bin" "$OUT/invalid-dup-file-index.bin"
# path entry 1 file_index field (+8), path entry size 16 -> offset 8+16
printf '\x00\x00\x00\x00' | dd of="$OUT/invalid-dup-file-index.bin" bs=1 seek=$((PATH_TBL_OFF + 16 + 8)) conv=notrunc status=none

# --- invalid-unreferenced-file : file_count > path_table_count ---
# Build a 3-file capsule, then physically remove the third path entry AND its
# name from the arena, shifting every later section (file table, payload,
# licence) back accordingly. Removing the name too is required since ADR-0017:
# a name left behind in the arena would be undescribed data and the parser
# would report UnpackedNameArena (rule 25) before reaching the bijection. The
# result has a packed arena and a consistent layout with 3 files but only 2
# paths, so the bijection check reports UnreferencedFile (checked before the
# whole-capsule digest).
printf '/system/boot/init.tos\t%s\n/system/version\t%s\n/system/etc/extra.tos\t%s\n' \
    "$INIT" "$GEN/.version.txt" "$GEN/.version.txt" > "$GEN/.unref.manifest"
$TOOL --detached --out "$GEN/.unref.bin" "$GEN/.unref.manifest"
python3 - "$GEN/.unref.bin" "$OUT/invalid-unreferenced-file.bin" <<'PY'
import struct, sys
src, dst = sys.argv[1], sys.argv[2]
b = bytearray(open(src, "rb").read())
path_off = struct.unpack_from("<Q", b, 40)[0]
path_cnt = struct.unpack_from("<I", b, 48)[0]
assert path_cnt == 3, path_cnt
# third path entry: [path_off+32, path_off+48); its name is the last one in the
# packed arena, which starts right after the path table.
entry2 = path_off + 32
name_off2, name_len2 = struct.unpack_from("<II", b, entry2)
name_start = path_off + path_cnt * 16
# Delete the name first (it lies after the entry), then the entry itself.
del b[name_start + name_off2:name_start + name_off2 + name_len2]
del b[entry2:entry2 + 16]
shift = 16 + name_len2
for off in (32,):  # total_length
    v = struct.unpack_from("<Q", b, off)[0]
    struct.pack_into("<Q", b, off, v - shift)
for off in (56, 72, 136):  # file_table_offset, payload_offset, licence_notice_offset
    v = struct.unpack_from("<Q", b, off)[0]
    if v > entry2:
        struct.pack_into("<Q", b, off, v - shift)
struct.pack_into("<I", b, 48, 2)  # path_table_count 3 -> 2
open(dst, "wb").write(bytes(b))
print(f"unreferenced vector: path_cnt=2 file_cnt=3, arena stays packed (-{shift} bytes)")
PY

# --- invalid-bootcanon-mismatch : canonical path flags set, file flags cleared ---
cp "$OUT/valid-001.bin" "$OUT/invalid-bootcanon-mismatch.bin"
# file entry 0 file_flags at +48; clear bit 0 (boot-canonical)
printf '\x00' | dd of="$OUT/invalid-bootcanon-mismatch.bin" bs=1 seek=$((FILE_TBL_OFF + 48)) conv=notrunc status=none

# --- invalid-licence-tail : licence_notice_offset shifted forward by 1 ---
cp "$OUT/valid-001.bin" "$OUT/invalid-licence-tail.bin"
# licence_notice_offset is u64 at header offset 136; +1 byte
python3 - "$OUT/invalid-licence-tail.bin" <<'PY'
import struct, sys
p = sys.argv[1]
with open(p, "r+b") as f:
    f.seek(136)
    v = struct.unpack("<Q", f.read(8))[0]
    f.seek(136)
    f.write(struct.pack("<Q", v + 1))
PY

echo "NOTE: tests/vectors/capsule-v1/vectors.tsv is hand-maintained (see"
echo "      CAPSULE_FORMAT_V1.md 10). Add or remove a vector here and you must"
echo "      update that table too - the integration test fails if they disagree."
echo "vectors regenerated:"
ls -1 "$OUT"/*.bin
