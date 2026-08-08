#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Reproducible QEMU evidence for the existing Stage 1 capsule p95 budget.
#
# This script deliberately delegates every boot to run.sh. It only generates
# the approved detached fixture, collects its existing serial boundaries and
# summarizes their host-monotonic interval.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="$ROOT/target/stage1-performance"
EVIDENCE_STATUS="P1"
PREPARE_ONLY=0
ACCEL=""

usage() {
    cat <<'EOF'
Usage: bash host-tools/qemu-test/stage1-performance.sh [--out DIR] [--evidence-status P1|P2] [--prepare-only] [--accel tcg|kvm]

Generates the deterministic 1,000-file / exactly-16-MiB detached capsule
fixture under source/target/, performs 3 warm-ups and 21 QEMU measurements,
and writes raw JSONL samples plus report.json. P2 is reserved for the declared
GitHub Actions QEMU profile; local output is P1.
--prepare-only stops after fixture/capsule/provenance preparation for the
research-only native double-validation runner.
--accel is research-only when explicitly set. Omitting it preserves the
mandatory qemu64/TCG conformance profile.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --evidence-status) EVIDENCE_STATUS="$2"; shift 2 ;;
        --prepare-only) PREPARE_ONLY=1; shift ;;
        --accel) ACCEL="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$EVIDENCE_STATUS" in P1|P2) ;; *) echo "invalid evidence status: $EVIDENCE_STATUS" >&2; exit 2 ;; esac
case "$ACCEL" in ""|tcg|kvm) ;; *) echo "invalid accelerator: $ACCEL" >&2; exit 2 ;; esac
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
case "$OUT" in
    "$ROOT"/target/*) ;;
    *) echo "performance output must remain under $ROOT/target" >&2; exit 2 ;;
esac

TOOL="$ROOT/target/release/tos-capsule-tool"
LOADER="$ROOT/target/x86_64-unknown-uefi/release/tos-uefi-loader.efi"
NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
WORKLOAD="$ROOT/tests/performance/stage1_capsule_workload.py"
HARNESS="$ROOT/host-tools/qemu-test/run.sh"
PROVENANCE="$GITROOT/scripts/check-capsule-provenance.py"
FIXTURE="$OUT/fixture"
RUN="$OUT/run"
CAPSULE="$OUT/capsule.bin"
META="$OUT/capsule.meta.json"
WARMUPS="$OUT/warmups.jsonl"
MEASUREMENTS="$OUT/measurements.jsonl"
TIMESTAMPS="$OUT/event-timestamps.jsonl"
REPORT="$OUT/report.json"

if [ ! -x "$TOOL" ]; then
    (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
fi
if [ ! -f "$LOADER" ]; then
    (cd "$ROOT" && cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi)
fi
if [ ! -f "$NUCLEUS" ]; then
    (cd "$ROOT" && cargo build --release -p tos-nucleus --target x86_64-unknown-none)
fi

# `OUT` is constrained above to the ignored source/target tree. Recreate only
# the runner-owned fixture so old inputs cannot contaminate its exact size.
rm -rf "$FIXTURE"
mkdir -p "$FIXTURE"
python3 "$WORKLOAD" fixture --out "$FIXTURE"
(
    cd "$FIXTURE"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$CAPSULE" --meta "$META" manifest.tsv
)
python3 "$PROVENANCE" --root "$GITROOT" --capsule "$CAPSULE" --manifest "$META"

if [ "$PREPARE_ONLY" -eq 1 ]; then
    echo "STAGE1-PERFORMANCE PREPARED: fixture=$FIXTURE capsule=$CAPSULE"
    exit 0
fi

: > "$WARMUPS"
: > "$MEASUREMENTS"
run_sample() {
    local phase=$1
    local index=$2
    local accel_args=()
    if [ -n "$ACCEL" ]; then
        accel_args=(--accel "$ACCEL")
    fi
    bash "$HARNESS" --out "$RUN" --capsule "$CAPSULE" --expect 33 \
        --event-timestamps "$TIMESTAMPS" "${accel_args[@]}"
    python3 "$WORKLOAD" sample --timestamps "$TIMESTAMPS" --phase "$phase" \
        --index "$index" --out "$OUT/${phase}s.jsonl"
}

for index in 1 2 3; do
    run_sample warmup "$index"
done
for index in $(seq 1 21); do
    run_sample measurement "$index"
done

python3 "$WORKLOAD" report \
    --fixture "$FIXTURE" \
    --measurements "$MEASUREMENTS" \
    --warmups "$WARMUPS" \
    --out "$REPORT" \
    --source-commit "$(git -C "$GITROOT" rev-parse HEAD)" \
    --qemu-version "$(qemu-system-x86_64 --version | head -n 1)" \
    --rustc-version "$(rustc --version)" \
    --ovmf-code "$RUN/OVMF_CODE.fd" \
    --ovmf-vars "$RUN/OVMF_VARS.fd" \
    --evidence-status "$EVIDENCE_STATUS" \
    --accelerator "${ACCEL:-tcg}"

python3 - "$REPORT" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
stats = report["statistics"]
print(
    "STAGE1-PERFORMANCE PASS: "
    f"median={stats['median_ns'] / 1_000_000:.3f}ms "
    f"p95={stats['p95_ns'] / 1_000_000:.3f}ms "
    f"p99={stats['p99_ns'] / 1_000_000:.3f}ms "
    f"report={sys.argv[1]}"
)
PY
