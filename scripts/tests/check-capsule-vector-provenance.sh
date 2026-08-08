#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the capsule-vector provenance manifest checker.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECKER="$ROOT/scripts/check-capsule-vector-provenance.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
REPO="$TMP/repo"
VECTOR_DIR="$REPO/source/tests/vectors/capsule-v1"

if [[ ! -f "$CHECKER" ]]; then
    echo "FAIL: missing capsule vector provenance checker: $CHECKER" >&2
    exit 1
fi

mkdir -p "$VECTOR_DIR" "$REPO/source/tests/vectors/gen" "$REPO/source/system/boot"
cp "$ROOT/source/tests/vectors/capsule-v1/provenance.schema.json" \
    "$VECTOR_DIR/provenance.schema.json"
printf 'valid capsule fixture\n' > "$VECTOR_DIR/valid-001.bin"
printf 'canonical boot source\n' > "$REPO/source/system/boot/init.tos"
printf 'retained licence notice\n' > "$REPO/source/system/boot/NOTICES.txt"
cat > "$REPO/source/tests/vectors/gen/gen.sh" <<'EOF'
#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
exit 0
EOF
chmod +x "$REPO/source/tests/vectors/gen/gen.sh"

git -C "$REPO" init -q
git -C "$REPO" config user.name provenance-test
git -C "$REPO" config user.email provenance-test@example.invalid
git -C "$REPO" add source
git -C "$REPO" commit -qm 'fixture inputs'

SOURCE_COMMIT="$(git -C "$REPO" rev-parse HEAD)"
VECTOR_SHA="$(sha256sum "$VECTOR_DIR/valid-001.bin" | awk '{print $1}')"
INIT_SHA="$(sha256sum "$REPO/source/system/boot/init.tos" | awk '{print $1}')"
NOTICES_SHA="$(sha256sum "$REPO/source/system/boot/NOTICES.txt" | awk '{print $1}')"
GEN_SHA="$(sha256sum "$REPO/source/tests/vectors/gen/gen.sh" | awk '{print $1}')"

write_valid_manifest() {
    cat > "$VECTOR_DIR/provenance.json" <<EOF
{
  "record_spdx_license": "GPL-3.0-or-later",
  "format": "tos-capsule-vector-provenance-v1",
  "schema_version": 1,
  "vectors": [
    {
      "vector": "valid-001.bin",
      "sha256": "$VECTOR_SHA",
      "generated_artifact": true,
      "provenance_status": "verified",
      "generator": {
        "path": "source/tests/vectors/gen/gen.sh",
        "version": 1,
        "source_commit": "$SOURCE_COMMIT",
        "sha256": "$GEN_SHA",
        "spdx": "GPL-3.0-or-later"
      },
      "source_commit": {
        "kind": "git",
        "algorithm": "sha1",
        "value": "$SOURCE_COMMIT"
      },
      "inputs": [
        {
          "repository_path": "source/system/boot/init.tos",
          "capsule_path": "/system/boot/init.tos",
          "sha256": "$INIT_SHA",
          "spdx": ["GPL-3.0-or-later"],
          "role": "embedded canonical boot source"
        },
        {
          "repository_path": "source/system/boot/NOTICES.txt",
          "capsule_path": null,
          "sha256": "$NOTICES_SHA",
          "spdx": ["GPL-3.0-or-later"],
          "role": "embedded licence notice tail"
        }
      ],
      "container_licensing": {
        "status": "mixed-material-generated",
        "spdx_expression": null
      },
      "derivation": null
    }
  ]
}
EOF
}

expect_fail() {
    local needle=$1
    if python3 "$CHECKER" --root "$REPO" > "$TMP/check.log" 2>&1; then
        echo "FAIL: malformed provenance was accepted" >&2
        exit 1
    fi
    if ! grep -Fq "$needle" "$TMP/check.log"; then
        echo "FAIL: rejection did not identify '$needle'" >&2
        cat "$TMP/check.log" >&2
        exit 1
    fi
}

write_valid_manifest
git -C "$REPO" add source/tests/vectors/capsule-v1/provenance.json
if ! python3 "$CHECKER" --root "$REPO"; then
    echo "FAIL: valid mixed-material provenance was rejected" >&2
    exit 1
fi

sed -i 's/"derivation": null/"derivation": {"base_vector": "ephemeral-base.bin", "base_sha256": "'"$VECTOR_SHA"'", "transformation_recipe": {"kind": "layout-rewrite", "operations": [{"op": "delete-path-entry"}]}}/' \
    "$VECTOR_DIR/provenance.json"
if ! python3 "$CHECKER" --root "$REPO"; then
    echo "FAIL: an explicitly named ephemeral derivation base was rejected" >&2
    exit 1
fi
write_valid_manifest

printf 'unrecorded fixture\n' > "$VECTOR_DIR/unrecorded.bin"
git -C "$REPO" add source/tests/vectors/capsule-v1/unrecorded.bin
expect_fail 'unrecorded.bin'
git -C "$REPO" rm -q --cached source/tests/vectors/capsule-v1/unrecorded.bin
rm "$VECTOR_DIR/unrecorded.bin"

sed -i 's/"spdx_expression": null/"spdx_expression": "GPL-3.0-or-later"/' \
    "$VECTOR_DIR/provenance.json"
expect_fail 'spdx_expression'

write_valid_manifest
sed -i 's/"derivation": null/"derivation": {"base_vector": "valid-001.bin"}/' \
    "$VECTOR_DIR/provenance.json"
expect_fail 'base_sha256'

echo "check-capsule-vector-provenance: PASS"
