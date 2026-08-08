#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Research-only native measurement of the exact Stage 1 validation sequence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="$ROOT/target/stage1-native-performance"

usage() {
    cat <<'EOF'
Usage: bash host-tools/qemu-test/stage1-native-performance.sh [--out DIR]

Creates the same detached 1,000-file / exactly-16-MiB capsule and checked
sidecar as the QEMU F-18 harness, then records 3 warm-ups and 21 native release
samples of fresh parser -> fresh parser -> canonical boot-file lookup. This is
research evidence, not an F-18 gate.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

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
    --out "$OUT/samples.jsonl" \
    --warmups 3 --samples 21
python3 "$ROOT/tests/performance/stage1_capsule_workload.py" native-report \
    --fixture "$OUT/fixture" \
    --samples "$OUT/samples.jsonl" \
    --out "$OUT/report.json" \
    --source-commit "$(git -C "$GITROOT" rev-parse HEAD)" \
    --rustc-version "$(rustc --version)"

python3 - "$OUT/report.json" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
stats = report["statistics"]
print(
    "STAGE1-NATIVE-RESEARCH: "
    f"median={stats['median_ns'] / 1_000_000:.3f}ms "
    f"p95={stats['p95_ns'] / 1_000_000:.3f}ms "
    f"p99={stats['p99_ns'] / 1_000_000:.3f}ms "
    f"budget={stats['would_meet_existing_budget']} report={sys.argv[1]}"
)
PY
