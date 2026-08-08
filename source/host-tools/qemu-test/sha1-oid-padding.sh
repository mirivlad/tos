#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Boot the tracked, provenance-recorded SHA-1 OID padding fixture and prove the
# real loader rejects it before the nucleus gains control.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-negative-sha1-padding}"
CAPSULE="$ROOT/tests/vectors/capsule-v1/invalid-sha1-oid-padding.bin"

[ -f "$CAPSULE" ] || { echo "missing vector: $CAPSULE" >&2; exit 2; }
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT/boot" --capsule "$CAPSULE" --expect 67 \
    --require 'TOS.BOOT.ENTRY TOS.BOOT.FAILC' \
    --forbid 'TOS.NUCLEUS.ENTRY'

grep -Fx 'TOS.BOOT.FAILC capsule_err=NonZeroOidPadding' "$OUT/boot/events.log" >/dev/null \
    || { echo 'missing NonZeroOidPadding loader evidence' >&2; exit 1; }
echo 'SHA1-OID-PADDING PASS: exit 67, loader rejected before nucleus entry'
