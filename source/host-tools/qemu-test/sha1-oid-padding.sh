#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Generate a malformed SHA-1 OID capsule only in ignored target/ and prove the
# real loader rejects it before the nucleus gains control.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-negative-sha1-padding}"
BASE="$ROOT/tests/vectors/capsule-v1/valid-001.bin"

[ -f "$BASE" ] || { echo "missing base vector: $BASE" >&2; exit 2; }
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
CAPSULE="$OUT/invalid-sha1-oid-padding.bin"

python3 - "$BASE" "$CAPSULE" <<'PY'
import hashlib
import sys

src, dst = sys.argv[1:]
capsule = bytearray(open(src, "rb").read())
if len(capsule) < 184:
    raise SystemExit("base capsule is shorter than the v1 header")

# CAPSULE_FORMAT_V1.md §3: raw SHA-1 OID occupies bytes 100..120 and ADR-0016
# requires the remaining twelve identity bytes to be zero. Deliberately violate
# only the first unused byte, then make whole_capsule_digest self-consistent so
# the parser must reach the padding rule rather than BadWholeDigest.
capsule[96:100] = bytes((1, 1, 20, 0))
capsule[100:132] = bytes(32)
capsule[100:120] = bytes(range(20))
capsule[120] = 0x01
whole = hashlib.sha256(capsule[:152] + bytes(32) + capsule[184:]).digest()
capsule[152:184] = whole
open(dst, "wb").write(capsule)
PY

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT/boot" --capsule "$CAPSULE" --expect 67 \
    --require 'TOS.BOOT.ENTRY TOS.BOOT.FAILC' \
    --forbid 'TOS.NUCLEUS.ENTRY'

grep -Fx 'TOS.BOOT.FAILC capsule_err=NonZeroOidPadding' "$OUT/boot/events.log" >/dev/null \
    || { echo 'missing NonZeroOidPadding loader evidence' >&2; exit 1; }
echo 'SHA1-OID-PADDING PASS: exit 67, loader rejected before nucleus entry'
