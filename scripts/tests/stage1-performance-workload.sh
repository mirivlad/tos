#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the deterministic Stage 1 performance workload generator.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORKLOAD="$ROOT/source/tests/performance/stage1_capsule_workload.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

python3 "$WORKLOAD" fixture --out "$TMP/fixture"

python3 - "$TMP/fixture" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
manifest = root / "manifest.tsv"
rows = [line.rstrip("\n").split("\t", 1) for line in manifest.open(encoding="utf-8")]
if len(rows) != 1000:
    raise SystemExit(f"FAIL: expected 1000 manifest rows, got {len(rows)}")
if rows[0][0] != "/system/boot/init.tos":
    raise SystemExit("FAIL: canonical boot file is not first")
if rows != sorted(rows):
    raise SystemExit("FAIL: manifest is not in canonical path order")

payload = sum((root / source).stat().st_size for _, source in rows)
if payload != 16 * 1024 * 1024:
    raise SystemExit(f"FAIL: payload is {payload}, expected exactly 16 MiB")
if any(b"SPDX-License-Identifier: GPL-3.0-or-later" not in (root / source).read_bytes() for _, source in rows):
    raise SystemExit("FAIL: an input lacks the declared SPDX expression")

digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
record = json.loads((root / "workload.json").read_text(encoding="utf-8"))
if record["manifest_sha256"] != digest:
    raise SystemExit("FAIL: workload metadata does not bind manifest bytes")
if record["payload_bytes"] != payload or record["file_count"] != 1000:
    raise SystemExit("FAIL: workload metadata does not bind fixture dimensions")
PY

: > "$TMP/warmups.jsonl"
: > "$TMP/measurements.jsonl"
for index in 1 2 3; do
    printf '{"event":"TOS.BOOT.ENTRY","monotonic_ns":100}\n{"event":"TOS.BOOTTEXT.PATH","monotonic_ns":%s}\n' "$((100 + index))" > "$TMP/timestamps.jsonl"
    python3 "$WORKLOAD" sample --timestamps "$TMP/timestamps.jsonl" --phase warmup \
        --index "$index" --out "$TMP/warmups.jsonl"
done
for index in $(seq 1 21); do
    printf '{"event":"TOS.BOOT.ENTRY","monotonic_ns":100}\n{"event":"TOS.BOOTTEXT.PATH","monotonic_ns":%s}\n' "$((100 + index))" > "$TMP/timestamps.jsonl"
    python3 "$WORKLOAD" sample --timestamps "$TMP/timestamps.jsonl" --phase measurement \
        --index "$index" --out "$TMP/measurements.jsonl"
done
printf firmware > "$TMP/OVMF_CODE.fd"
printf vars > "$TMP/OVMF_VARS.fd"
python3 "$WORKLOAD" report --fixture "$TMP/fixture" --measurements "$TMP/measurements.jsonl" \
    --warmups "$TMP/warmups.jsonl" --out "$TMP/report.json" --source-commit test \
    --qemu-version fake-qemu --rustc-version fake-rustc --ovmf-code "$TMP/OVMF_CODE.fd" \
    --ovmf-vars "$TMP/OVMF_VARS.fd" --evidence-status P1
python3 - "$TMP/report.json" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
stats = report["statistics"]
if (stats["median_ns"], stats["p95_ns"], stats["p99_ns"]) != (11, 20, 21):
    raise SystemExit(f"FAIL: nearest-rank statistics are wrong: {stats!r}")
if stats["p95_rank"] != 20 or stats["p99_rank"] != 21 or not stats["budget_pass"]:
    raise SystemExit(f"FAIL: report did not retain contract statistics: {stats!r}")
if len(report["raw_samples"]["measurements"]) != 21:
    raise SystemExit("FAIL: report omitted raw measured samples")
PY

echo 'stage1-performance-workload: PASS'
