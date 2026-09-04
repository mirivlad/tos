#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4C-1: a claimed function's resource placement cannot be moved.
#
# ADR-0082 §5a. ADR-0081 §13 measures each BAR once, at claim time, and every
# later mapping derives its physical base from that measurement; ADR-0082 §4 maps
# the MSI-X table the same way and §5 computes its extent from a cached BIR. All
# three rest on the cached layout still being the layout the live function
# decodes.
#
# **This began as an audit and reported that it was not.** On the reference
# function, BAR1 — the MSI-X table's own BAR — accepted a CPL-3 write, as did
# both halves of the 64-bit BAR4 pair, and a window was still derived from the
# stale cached base afterwards: `value=434`. It is now the gate for the
# narrowing that closed it, with the same fixture and the same numbers.
#
# A refusal that refused everything would prove nothing, so three of the bits
# must be **set**: writing a placement register back unchanged is still
# permitted, a window still derives the originally measured extent, and an
# ordinary unrelated field is still writable.
#
#   bash host-tools/qemu-test/pci-placement.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-pci-placement}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-pci-discovery"

fail() { echo "pci-placement: FAIL: $*" >&2; exit 1; }

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-pci-discovery)
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
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
[ -n "$value" ] || fail "the probe reported nothing"
[ "$value" -ge 0 ] || fail "the probe could not claim the function: $value"

refused() {
    [ $(( ($value & $1) == 0 )) = 1 ] || fail "$2 accepted a CPL-3 write"
}
permitted() {
    [ $(( ($value & $1) != 0 )) = 1 ] || fail "$2"
}

# --- the header the rule was stated over --------------------------------------
# A Type-1 header puts bridge windows and its expansion ROM at different offsets,
# so a run that read these under the wrong layout would be checking the wrong
# bytes and must not pass quietly.
permitted 256 "the function did not report a Type-0 header; these offsets are wrong for it"

# --- nothing that places a resource can be moved ------------------------------
refused 1   "BAR0"
refused 2   "BAR1 — the MSI-X table's own BAR"
refused 4   "BAR2"
refused 8   "BAR3"
refused 16  "BAR4 low"
refused 32  "BAR4 high — a 64-bit pair is one placement and both halves are protected"
refused 64  "the expansion ROM base address register"

# --- and the consequence the narrowing exists to prevent ----------------------
refused 128 "a window was still derived after a BAR had been rewritten"

# --- while the narrowing stays a narrowing ------------------------------------
permitted 512 "writing a placement register back unchanged was refused"
permitted 1024 "a window no longer derives the extent measured at claim time"
permitted 2048 "an ordinary unrelated configuration field is no longer writable"

# --- the claim leaves the function in a defined state -------------------------
# ADR-0082 §5b, §5c, §5d. The nucleus is the only party that sees what firmware
# left, so it is the only party that can say so.
grep -q '^TOS\.RUN\.PCI_NORMALISED .* device=4 function=0 .* msix=disabled_masked ' \
    "$OUT/events.log" ||
    fail "the claim did not normalise the function's interrupt state: $(grep PCI_NORMALISED "$OUT/events.log")"

echo "PCI-PLACEMENT PASS: a claimed function's resources cannot be moved"
echo "  every base-address register is refused, both halves of the 64-bit pair"
echo "  among them, and so is the expansion ROM register; a window still derives"
echo "  the extent measured at claim time, writing a placement register back"
echo "  unchanged is still permitted, and an unrelated field is still writable"
echo "  this fixture reported value=434 before the narrowing — BAR1, BAR4 low,"
echo "  BAR4 high and a stale window all accepted — and reports $value now"
sed -n 's/^\(TOS\.RUN\.PCI_NORMALISED .*\)$/  \1/p' "$OUT/events.log"
