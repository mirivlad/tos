#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for JSON provenance/schema record classification.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

mkdir -p "$TMP/scripts" "$TMP/source/tests/vectors/capsule-v1"
cp "$ROOT/scripts/check-spdx.sh" "$TMP/scripts/check-spdx.sh"
cat > "$TMP/source/tests/vectors/capsule-v1/provenance.schema.json" <<'EOF'
{
  "record_spdx_license": "GPL-3.0-or-later",
  "format": "test-schema"
}
EOF

git -C "$TMP" init -q
git -C "$TMP" add scripts/check-spdx.sh \
    source/tests/vectors/capsule-v1/provenance.schema.json
if ! (cd "$TMP" && sh scripts/check-spdx.sh > good.log 2>&1); then
    echo "FAIL: JSON provenance metadata with record_spdx_license was rejected" >&2
    cat "$TMP/good.log" >&2
    exit 1
fi

cat > "$TMP/source/tests/vectors/capsule-v1/bad.json" <<'EOF'
{"format": "no-licence-record"}
EOF
git -C "$TMP" add source/tests/vectors/capsule-v1/bad.json
if (cd "$TMP" && sh scripts/check-spdx.sh > bad.log 2>&1); then
    echo "FAIL: JSON without record_spdx_license was accepted" >&2
    exit 1
fi
if ! grep -q 'missing record_spdx_license' "$TMP/bad.log"; then
    echo "FAIL: missing JSON licence record was not diagnosed" >&2
    cat "$TMP/bad.log" >&2
    exit 1
fi

echo "check-spdx-json: PASS"
