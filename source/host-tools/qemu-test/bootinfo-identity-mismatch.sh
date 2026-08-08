#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# QEMU evidence for a test-only BootInfo-to-capsule identity mismatch. The
# caller must first build the feature into the isolated target directory:
# CARGO_TARGET_DIR=target/test-corrupt-bootinfo cargo build --release \
#   -p tos-uefi-loader --target x86_64-unknown-uefi \
#   --features test-corrupt-bootinfo-identity
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
EXPECTED_REL="target/test-corrupt-bootinfo/x86_64-unknown-uefi/release/tos-uefi-loader.efi"
EXPECTED_ABS="$ROOT/$EXPECTED_REL"

if [ "$#" -ne 1 ]; then
    echo "usage: $0 $EXPECTED_REL" >&2
    exit 2
fi

LOADER="$1"
if [ "$LOADER" != "$EXPECTED_REL" ] && [ "$LOADER" != "$EXPECTED_ABS" ]; then
    echo "test loader must be the isolated artifact: $EXPECTED_REL" >&2
    exit 2
fi

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$ROOT/target/qemu-bootinfo-identity-mismatch" \
    --loader "$LOADER" --expect 67 \
    --require 'TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.IDENTITY.MISMATCH TOS.CAPSULE.FAIL' \
    --forbid 'TOS.HALT TOS.PANIC'

echo 'BOOTINFO-IDENTITY-MISMATCH PASS: nucleus rejected corrupted mirror with exit 67'
