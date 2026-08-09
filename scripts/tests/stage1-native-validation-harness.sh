#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression for the research-only native double-validation harness.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SOURCE="$ROOT/source"
TOOL="$SOURCE/target/release/tos-capsule-tool"
WORKLOAD="$SOURCE/tests/performance/stage1_capsule_workload.py"
CHECKER="$ROOT/scripts/check-capsule-provenance.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [[ ! -x "$TOOL" ]]; then
    (cd "$SOURCE" && cargo build --release -p tos-capsule-tool)
fi
python3 "$WORKLOAD" fixture --out "$TMP/fixture"
(
    cd "$TMP/fixture"
    "$TOOL" --detached --licence "$SOURCE/system/boot/NOTICES.txt" \
        --out "$TMP/capsule.bin" --meta "$TMP/capsule.meta.json" manifest.tsv
)
python3 "$CHECKER" --root "$ROOT" --capsule "$TMP/capsule.bin" --manifest "$TMP/capsule.meta.json"

(cd "$SOURCE" && cargo run --release -p tos-stage1-performance -- \
    --capsule "$TMP/capsule.bin" --out "$TMP/samples.jsonl" --warmups 1 --samples 2)
(cd "$SOURCE" && cargo run --release -p tos-stage1-performance -- \
    --mode crypto --capsule "$TMP/capsule.bin" --out "$TMP/crypto-samples.jsonl" \
    --warmups 1 --samples 2)

python3 - "$TMP/samples.jsonl" <<'PY'
import json
import sys

records = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
if [record["phase"] for record in records] != ["warmup", "measurement", "measurement"]:
    raise SystemExit(f"FAIL: unexpected native sample phases: {records!r}")
if any(record["duration_ns"] <= 0 for record in records):
    raise SystemExit("FAIL: native harness recorded a non-positive duration")
if any(record["validations"] != 2 or record["lookup"] != "/system/boot/init.tos" for record in records):
    raise SystemExit("FAIL: sample does not attest two validations and canonical lookup")

crypto = [json.loads(line) for line in open(sys.argv[1].replace("samples.jsonl", "crypto-samples.jsonl"), encoding="utf-8")]
if any(record["mode"] != "unavoidable_crypto" or record["validations"] != 2 for record in crypto):
    raise SystemExit("FAIL: crypto sample does not attest two fresh hash passes")
if any(record["crypto_bytes_per_boot"] <= 0 or record["crypto_hashes_per_boot"] <= 0 for record in crypto):
    raise SystemExit("FAIL: crypto sample lacks byte/hash accounting")
if any(record["crypto_hashes_per_boot"] != 2007 for record in crypto):
    raise SystemExit("FAIL: crypto accounting omitted the required boot-text digest")
PY

echo 'stage1-native-validation-harness: PASS'
