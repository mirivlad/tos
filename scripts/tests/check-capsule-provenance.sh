#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the Stage 1 capsule provenance sidecar contract.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TOOL="$ROOT/source/target/release/tos-capsule-tool"
CHECKER="$ROOT/scripts/check-capsule-provenance.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [[ ! -x "$TOOL" ]]; then
    (cd "$ROOT/source" && cargo build --release -p tos-capsule-tool)
fi

printf '/system/boot/init.tos\tsource/system/boot/init.tos\n' > "$TMP/manifest.txt"
build() {
    local stem=$1
    (
        cd "$ROOT"
        "$TOOL" --git-commit HEAD --licence source/system/boot/NOTICES.txt \
            --out "$TMP/$stem.bin" --meta "$TMP/$stem.json" "$TMP/manifest.txt"
    )
}

build first
python3 "$CHECKER" --root "$ROOT" --capsule "$TMP/first.bin" --manifest "$TMP/first.json"

build second
cmp "$TMP/first.bin" "$TMP/second.bin"
cmp "$TMP/first.json" "$TMP/second.json"

(
    cd "$ROOT"
    "$TOOL" --detached --licence source/system/boot/NOTICES.txt \
        --out "$TMP/detached.bin" --meta "$TMP/detached.json" "$TMP/manifest.txt"
)
python3 "$CHECKER" --root "$ROOT" --capsule "$TMP/detached.bin" --manifest "$TMP/detached.json"

python3 - "$TMP/first.json" <<'PY'
import json
import sys

path = sys.argv[1]
record = json.load(open(path, encoding="utf-8"))
record["artifact"]["sha256"] = "00" * 32
with open(path, "w", encoding="utf-8") as output:
    json.dump(record, output, indent=2)
    output.write("\n")
PY

if python3 "$CHECKER" --root "$ROOT" --capsule "$TMP/first.bin" --manifest "$TMP/first.json" >"$TMP/tamper.log" 2>&1; then
    echo 'FAIL: tampered provenance artifact digest was accepted' >&2
    exit 1
fi
grep -Fq 'artifact.sha256' "$TMP/tamper.log" || {
    echo 'FAIL: artifact digest tamper had no focused diagnosis' >&2
    cat "$TMP/tamper.log" >&2
    exit 1
}

if ! grep -Fq 'check-capsule-provenance.py' "$ROOT/source/host-tools/qemu-test/run.sh"; then
    echo 'FAIL: normal QEMU path does not verify its generated provenance sidecar' >&2
    exit 1
fi

echo 'capsule-provenance: PASS'
