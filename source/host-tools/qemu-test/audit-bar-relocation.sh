#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4C-1 audit: can a `config_write` holder move a resource after the
# assignment measured where it was?
#
# **This is an audit, not a gate, and it is deliberately not in preflight.** It
# asserts nothing. It reports what the reference device does today, because the
# Project Architect's review of ADR-0082 asked for the question to be exercised
# rather than reasoned from documentation.
#
# What it found, 2026-09-05, on `virtio-blk-pci` at 00:04.0 of the Stage 4
# profile — `TOS.RUN.COMPLETED value=i64:434`:
#
#     BAR1  accepted the write   ← and BAR1 is the MSI-X table's own BAR
#     BAR4  accepted the write   ← the modern VirtIO structures, low half
#     BAR5  accepted the write   ← the high half of the 64-bit BAR4 pair
#     and a window was still derived from BAR4 after BAR4 had been rewritten
#
# BAR0, BAR2, BAR3 and the Expansion ROM BAR are unimplemented on this function,
# so "no" for them means the device holds them read-only — not that the nucleus
# protects them.
#
# The finding is written up in `docs/evidence/STAGE4C1_REVIEW_FINDINGS.md` §2.
# **When the narrowing proposed there is accepted and implemented, this becomes a
# negative gate** — the same fixture, the same numbers, and every bit expected to
# be zero — and moves into `virtio-mmio.sh` beside the other refusals.
#
#   bash host-tools/qemu-test/audit-bar-relocation.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/audit-bar-relocation}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-pci-discovery"

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-pci-discovery)
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] || {
    echo "production nucleus changed while building the isolated audit artifact" >&2
    exit 1
}
NUCLEUS="$TARGET/x86_64-unknown-none/release/tos-nucleus"

printf '/system/boot/init.tos\t%s/tests/vectors/pci-bar-relocation/init.tos\n' "$ROOT" \
    > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/fixture.bin" --meta "$OUT/meta.json" "$OUT/manifest.txt" > /dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/fixture.bin" --manifest "$OUT/meta.json" > /dev/null
bash "$HERE/run.sh" --out "$OUT" --capsule "$OUT/fixture.bin" \
    --nucleus "$NUCLEUS" --expect 33 --stage4-block-device > /dev/null

value="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$OUT/events.log")"
[ -n "$value" ] || { echo "the audit reported nothing" >&2; exit 1; }
[ "$value" -ge 0 ] || { echo "the audit could not claim the function: $value" >&2; exit 1; }

say() {
    if [ $(( ($value & $1) != 0 )) = 1 ]; then
        printf '  %-22s ACCEPTED THE WRITE\n' "$2"
    else
        printf '  %-22s no change observed\n' "$2"
    fi
}

echo "BAR-RELOCATION AUDIT: value=$value on 00:04.0 of the Stage 4 profile"
[ $(( ($value & 256) != 0 )) = 1 ] &&
    echo "  header type            Type-0 (these offsets are a Type-0 header's)" ||
    echo "  header type            NOT Type-0 — the offsets below are misread"
say 1 "BAR0"
say 2 "BAR1 (MSI-X table)"
say 4 "BAR2"
say 8 "BAR3"
say 16 "BAR4 low"
say 32 "BAR4 high"
say 64 "Expansion ROM BAR"
say 128 "stale window derived"
echo "  a register reported 'no change' is one this device holds read-only,"
echo "  not one the nucleus protects"
