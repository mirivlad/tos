#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Research-only native measurement of the exact Stage 1 validation sequence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="$ROOT/target/stage1-native-performance"
EVIDENCE_STATUS="P1"

usage() {
    cat <<'EOF'
Usage: bash host-tools/qemu-test/stage1-native-performance.sh [--out DIR] [--evidence-status P1|P2]

Creates the same detached 1,000-file / exactly-16-MiB capsule and checked
sidecar as the QEMU F-18 harness, then records 3 warm-ups and 21 native release
samples of the exact loader/nucleus validation sequence and its separately
measured unavoidable cryptographic work. This is research evidence, not an
F-18 gate.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --evidence-status) EVIDENCE_STATUS="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done
case "$EVIDENCE_STATUS" in P1|P2) ;; *) echo "invalid evidence status: $EVIDENCE_STATUS" >&2; exit 2 ;; esac

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
case "$OUT" in
    "$ROOT"/target/*) ;;
    *) echo "native performance output must remain under $ROOT/target" >&2; exit 2 ;;
esac

bash "$ROOT/host-tools/qemu-test/stage1-performance.sh" --out "$OUT" --prepare-only
(cd "$ROOT" && cargo build --release -p tos-stage1-performance)
"$ROOT/target/release/tos-stage1-performance" \
    --capsule "$OUT/capsule.bin" \
    --out "$OUT/full-samples.jsonl" \
    --warmups 3 --samples 21
"$ROOT/target/release/tos-stage1-performance" \
    --mode crypto \
    --capsule "$OUT/capsule.bin" \
    --out "$OUT/crypto-samples.jsonl" \
    --warmups 3 --samples 21
python3 "$ROOT/tests/performance/stage1_capsule_workload.py" native-report \
    --fixture "$OUT/fixture" \
    --samples "$OUT/full-samples.jsonl" \
    --out "$OUT/full-report.json" \
    --source-commit "$(git -C "$GITROOT" rev-parse HEAD)" \
    --rustc-version "$(rustc --version)" --evidence-status "$EVIDENCE_STATUS"
python3 "$ROOT/tests/performance/stage1_capsule_workload.py" crypto-report \
    --fixture "$OUT/fixture" \
    --samples "$OUT/crypto-samples.jsonl" \
    --out "$OUT/crypto-report.json" \
    --source-commit "$(git -C "$GITROOT" rev-parse HEAD)" \
    --rustc-version "$(rustc --version)" --evidence-status "$EVIDENCE_STATUS"
python3 "$ROOT/tests/performance/stage1_capsule_workload.py" validation-ratio \
    --full "$OUT/full-report.json" \
    --crypto "$OUT/crypto-report.json" \
    --out "$OUT/ratio.json" --max-p95-ratio 1.30

python3 - "$OUT/full-report.json" "$OUT/crypto-report.json" "$OUT/ratio.json" <<'PY'
import json
import sys

full = json.load(open(sys.argv[1], encoding="utf-8"))
crypto = json.load(open(sys.argv[2], encoding="utf-8"))
ratio = json.load(open(sys.argv[3], encoding="utf-8"))
stats = full["statistics"]
print(
    "STAGE1-NATIVE-RESEARCH: "
    f"median={stats['median_ns'] / 1_000_000:.3f}ms "
    f"p95={stats['p95_ns'] / 1_000_000:.3f}ms "
    f"p99={stats['p99_ns'] / 1_000_000:.3f}ms "
    f"crypto_p95={crypto['statistics']['p95_ns'] / 1_000_000:.3f}ms "
    f"ratio_p95={ratio['full_over_unavoidable_crypto']['p95_ratio']:.3f} "
    f"bytes={crypto['crypto_accounting']['bytes_per_boot']} "
    f"hashes={crypto['crypto_accounting']['hashes_per_boot']} "
    f"evidence={full['evidence_status']} report={sys.argv[1]}"
)
PY
