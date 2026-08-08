#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for Stage 1 embedded-artwork provenance.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECKER="$ROOT/scripts/check-embedded-artwork-provenance.py"

python3 "$CHECKER" --root "$ROOT"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/assets/mascot" "$TMP/source/nucleus/src"
cp "$ROOT/assets/mascot/pyro-stage1-provenance.json" "$TMP/assets/mascot/"
cp "$ROOT/assets/mascot/tos_ascii-art2.txt" "$TMP/assets/mascot/"
cp "$ROOT/assets/mascot/README.md" "$TMP/assets/mascot/"
cp "$ROOT/source/nucleus/src/framebuffer.rs" "$TMP/source/nucleus/src/"
printf 'tamper\n' >> "$TMP/assets/mascot/tos_ascii-art2.txt"

if python3 "$CHECKER" --root "$TMP" >"$TMP/tamper.log" 2>&1; then
    echo 'FAIL: artwork digest tamper was accepted' >&2
    exit 1
fi
grep -Fq 'canonical source digest mismatch' "$TMP/tamper.log" || {
    echo 'FAIL: artwork digest tamper did not produce a provenance diagnosis' >&2
    cat "$TMP/tamper.log" >&2
    exit 1
}
echo 'embedded-artwork-provenance: PASS'
