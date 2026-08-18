#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The boot path fails closed for the right reason, with the right code.
#
# ADR-0042 gives one exact condition to RESULT_BOOT_MODULE_FAILED (0x25): boot
# ABI and capsule validation succeeded and the nucleus remained operational, but
# the canonical boot module did not complete through the Stage 2 execution path.
# This boots a capsule that is well-formed in every way the nucleus checks
# before Stage 2 and whose boot module the checker refuses, and requires that
# exact code — not RESULT_CAPSULE_INVALID, which would send an operator looking
# for a supply or integrity problem that is not there.
#
#   bash host-tools/qemu-test/boot-module-failure.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:-target/qemu-boot-module-failure}"
CAPSULE="$ROOT/tests/vectors/capsule-v1/boot-module-invalid.bin"

# QEMU's isa-debug-exit reports (value << 1) | 1, so 0x25 arrives as 75.
EXPECT=$(( (0x25 << 1) | 1 ))

[ -f "$CAPSULE" ] || { echo "missing vector: $CAPSULE" >&2; exit 2; }

bash "$HERE/run.sh" --out "$OUT" --capsule "$CAPSULE" --expect "$EXPECT" \
    --require "TOS.NUCLEUS.ENTRY TOS.CAPSULE.OK TOS.RUN.BEGIN TOS.RUN.DIAGNOSTIC TOS.RUN.REFUSED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.PANIC TOS.EXCEPTION TOS.CAPSULE.FAIL TOS.ABI.FAIL TOS.MEM.FAIL TOS.RUN.COMPLETED TOS.HALT"

LOG="$OUT/serial.txt"
tr -d '\r' < "$OUT/serial.log" > "$LOG"

fail() { echo "boot-module-failure: FAIL: $*" >&2; exit 1; }

# The refusal must name the stage that refused, and it must be the checker: a
# capsule problem would have stopped before the reference path started.
STAGE=$(grep -m1 '^TOS\.RUN\.REFUSED ' "$LOG" | tr ' ' '\n' | sed -n 's/^stage=//p')
[ "$STAGE" = check ] || fail "the refusal came from stage '$STAGE', expected check"

# After ADR-0048 the nucleus does not execute stages: the module runs in a
# process, so what the nucleus can assert is that the process ended without
# completing. Which stage refused is in the runtime's own events above, checked
# immediately before this, which is where the contract already delegates the
# detail.
BOOTSTAGE=$(grep -m1 '^TOS\.BOOTMODULE\.FAIL ' "$LOG" | tr ' ' '\n' | sed -n 's/^stage=//p')
[ "$BOOTSTAGE" = process ] || fail "TOS.BOOTMODULE.FAIL named stage '$BOOTSTAGE'"

# A diagnostic must carry the normative locator, or the log says a module failed
# without saying where.
grep -qE '^TOS\.RUN\.DIAGNOSTIC E[0-9]+_[A-Z_]+ severity=error stage=[a-z]+ bytes=[0-9]+\.\.[0-9]+ at=[0-9]+:[0-9]+' "$LOG" ||
    fail "no structured diagnostic with a byte span"

echo "BOOT-MODULE-FAILURE PASS: refused at stage=$STAGE, halted with RESULT_BOOT_MODULE_FAILED (exit $EXPECT)"
