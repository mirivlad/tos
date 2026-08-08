#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# QEMU evidence that the UEFI loader rejects an oversized capsule from EFI
# metadata before it allocates or reads the complete file.
#
# Usage: bash host-tools/qemu-test/capsule-size-limit.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-capsule-size-limit}"
MAX_CAPSULE_BYTES=$((32 * 1024 * 1024))
CAPSULE="$OUT/oversized-capsule.bin"

mkdir -p "$OUT"
# Keep the resource-boundary input untracked. It is sparse on filesystems that
# support it, but its logical length is exactly MAX_CAPSULE_BYTES + 1.
dd if=/dev/zero of="$CAPSULE" bs=1 count=0 seek=$((MAX_CAPSULE_BYTES + 1)) status=none
[ "$(stat -c%s "$CAPSULE")" -eq $((MAX_CAPSULE_BYTES + 1)) ]

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT/boot" --capsule "$CAPSULE" --expect 67

EVENTS="$OUT/boot/events.log"
grep -Fx 'TOS.BOOT.FAILC capsule_err=CapsuleTooLarge' "$EVENTS" >/dev/null
if grep -Fq 'TOS.NUCLEUS.ENTRY' "$EVENTS"; then
    echo 'CAPSULE-SIZE-LIMIT FAIL: oversized capsule reached nucleus' >&2
    exit 1
fi

echo 'CAPSULE-SIZE-LIMIT PASS: loader rejected MAX_CAPSULE_BYTES + 1 before handoff'
