#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test: assembly sources use the same SPDX-header rule as other code.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/source/nucleus/src"
cp "$ROOT/scripts/check-spdx.sh" "$TMP/scripts/check-spdx.sh"
printf '%s\n' \
    '# SPDX-License-Identifier: GPL-3.0-or-later' \
    '.global example' \
    'example:' \
    '    hlt' > "$TMP/source/nucleus/src/example.S"
git -C "$TMP" init -q
git -C "$TMP" add scripts/check-spdx.sh source/nucleus/src/example.S

if ! (cd "$TMP" && sh scripts/check-spdx.sh > assembly.log 2>&1); then
    cat "$TMP/assembly.log" >&2
    echo 'FAIL: SPDX gate did not classify an assembly source with a valid header' >&2
    exit 1
fi
echo 'check-spdx-assembly: PASS'
