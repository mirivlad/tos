#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for binary artwork classification in check-spdx.sh.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

mkdir -p "$TMP/scripts" "$TMP/assets/mascot"
cp "$ROOT/scripts/check-spdx.sh" "$TMP/scripts/check-spdx.sh"
cat > "$TMP/assets/mascot/README.md" <<'EOF'
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Artwork record

| File | Licence |
|---|---|
| `assets/mascot/recorded.png` | `CC-BY-SA-4.0` |
EOF
: > "$TMP/assets/mascot/recorded.png"

git -C "$TMP" init -q
git -C "$TMP" add scripts/check-spdx.sh assets/mascot/README.md \
    assets/mascot/recorded.png

if ! (cd "$TMP" && sh scripts/check-spdx.sh > recorded.log 2>&1); then
    echo "FAIL: a PNG with an explicit artwork record was rejected" >&2
    cat "$TMP/recorded.log" >&2
    exit 1
fi

: > "$TMP/assets/mascot/unrecorded.png"
git -C "$TMP" add assets/mascot/unrecorded.png
if (cd "$TMP" && sh scripts/check-spdx.sh > unrecorded.log 2>&1); then
    echo "FAIL: an unrecorded PNG was accepted" >&2
    exit 1
fi
if ! grep -q 'assets/mascot/unrecorded.png' "$TMP/unrecorded.log"; then
    echo "FAIL: rejection did not identify the unrecorded PNG" >&2
    cat "$TMP/unrecorded.log" >&2
    exit 1
fi

echo "check-spdx-assets: PASS"
