#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Conformance checks for the accepted Boot ABI v1 serial-event contract.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ABI="$ROOT/source/interfaces/boot/BOOT_ABI_V1.md"
HARNESS="$ROOT/source/host-tools/qemu-test/run.sh"
PREFLIGHT="$ROOT/scripts/preflight.sh"
LOADER="$ROOT/source/boot/uefi-loader/src/main.rs"
NUCLEUS="$ROOT/source/nucleus/src/main.rs"
EXCEPTION="$ROOT/source/nucleus/src/exception.rs"

fail() {
    echo "check-boot-event-contract: FAIL: $*" >&2
    exit 1
}

require_text() {
    local needle=$1 file=$2
    grep -Fq "$needle" "$file" || fail "missing '$needle' in ${file#$ROOT/}"
}

require_emitted() {
    local event=$1
    if ! grep -Fq "$event" "$LOADER" && ! grep -Fq "$event" "$NUCLEUS"; then
        fail "implementation does not emit '$event'"
    fi
}

for event in \
    TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY \
    TOS.BOOTTEXT.PATH TOS.BOOTTEXT.LINE TOS.BOOTTEXT.DIGEST \
    TOS.IDENTITY TOS.HALT TOS.BOOT.FAILC TOS.BOOT.FAILI TOS.ABI.FAIL \
    TOS.MEM.FAIL TOS.CAPSULE.FAIL TOS.IDENTITY.MISMATCH TOS.PANIC TOS.EXCEPTION
do
    require_text "$event" "$ABI"
    if [ "$event" = TOS.EXCEPTION ]; then
        require_text "$event" "$EXCEPTION"
    else
        require_emitted "$event"
    fi
done

for field in source_kind= source_digest= capsule_digest= arch= builder=
do
    require_text "$field" "$ABI"
    require_text "$field" "$NUCLEUS"
done

require_text 'MAY add a reason token' "$ABI"
require_text 'optional fields' "$ABI"
require_text 'TOS.BOOTTEXT.LINE is optional' "$ABI"

success='TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.CAPSULE.OK TOS.BOOTTEXT.PATH TOS.BOOTTEXT.DIGEST TOS.IDENTITY TOS.HALT'
require_text "33) REQUIRE=\"$success\"" "$HARNESS"
if [ "$(grep -o 'TOS.CAPSULE.OK' <<<"$success" | wc -l)" -ne 2 ]; then
    fail 'success contract must contain two TOS.CAPSULE.OK events'
fi
require_text '67) REQUIRE="TOS.BOOT.ENTRY TOS.BOOT.FAILC"' "$HARNESS"
require_text 'run_gate "QEMU success boot" qemu_success' "$PREFLIGHT"
require_text 'run_gate "QEMU negative suite" qemu_negative' "$PREFLIGHT"
require_text 'run_gate "QEMU exception #UD" qemu_exception_ud2' "$PREFLIGHT"
require_text 'run_gate "QEMU exception #GP" qemu_exception_gp' "$PREFLIGHT"

case "${1-}" in
    '') ;;
    --qemu)
        bash "$ROOT/source/host-tools/qemu-test/run.sh" \
            --out "$ROOT/source/target/event-contract/success" --expect 33
        bash "$ROOT/source/host-tools/qemu-test/negative-suite.sh" \
            "$ROOT/source/target/event-contract/negative"
        ;;
    *)
        fail 'usage: check-boot-event-contract.sh [--qemu]'
        ;;
esac

echo 'check-boot-event-contract: PASS'
