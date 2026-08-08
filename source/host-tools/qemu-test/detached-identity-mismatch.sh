#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Derive a digest-consistent detached-identity corruption under ignored target/
# and prove the real loader rejects it before nucleus entry.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-negative-detached-identity}"
BASE="$ROOT/tests/vectors/capsule-v1/valid-001.bin"

[ -f "$BASE" ] || { echo "missing base vector: $BASE" >&2; exit 2; }
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
CAPSULE="$OUT/invalid-detached-identity.bin"

python3 - "$BASE" "$CAPSULE" <<'PY'
import hashlib
import sys

src, dst = sys.argv[1:]
capsule = bytearray(open(src, "rb").read())
if len(capsule) < 184:
    raise SystemExit("base capsule is shorter than the v1 header")
capsule[100] ^= 0x01
capsule[152:184] = hashlib.sha256(capsule[:152] + bytes(32) + capsule[184:]).digest()
open(dst, "wb").write(capsule)
PY

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT/boot" --capsule "$CAPSULE" --expect 67 \
    --require 'TOS.BOOT.ENTRY TOS.BOOT.FAILC' \
    --forbid 'TOS.NUCLEUS.ENTRY'

grep -Fx 'TOS.BOOT.FAILC capsule_err=DetachedIdentityMismatch' "$OUT/boot/events.log" >/dev/null \
    || { echo 'missing DetachedIdentityMismatch loader evidence' >&2; exit 1; }
echo 'DETACHED-IDENTITY-MISMATCH PASS: exit 67, loader rejected before nucleus entry'
