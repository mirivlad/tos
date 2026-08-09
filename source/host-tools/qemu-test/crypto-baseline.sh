#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Research-only QEMU measurement of unavoidable Stage 1 crypto work.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="$ROOT/target/stage1-crypto-baseline"
ACCEL=""
EVIDENCE_STATUS="P1"

usage() {
    cat <<'EOF'
Usage: bash host-tools/qemu-test/crypto-baseline.sh [--out DIR] [--accel tcg|kvm] [--evidence-status P1|P2]

Builds an isolated test nucleus below target/test-crypto-baseline and measures
only two fresh unavoidable-crypto validator passes over the same deterministic
capsule used by the ordinary F-18 QEMU runner. The default qemu64/TCG profile
is used when --accel is omitted; --accel kvm is research-only.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --accel) ACCEL="$2"; shift 2 ;;
        --evidence-status) EVIDENCE_STATUS="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done
case "$ACCEL" in ""|tcg|kvm) ;; *) echo "invalid accelerator: $ACCEL" >&2; exit 2 ;; esac
case "$EVIDENCE_STATUS" in P1|P2) ;; *) echo "invalid evidence status: $EVIDENCE_STATUS" >&2; exit 2 ;; esac

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
case "$OUT" in "$ROOT"/target/*) ;; *) echo "crypto evidence output must remain under $ROOT/target" >&2; exit 2 ;; esac

# Shared production fixture, builder and checked provenance sidecar.
bash "$ROOT/host-tools/qemu-test/stage1-performance.sh" --out "$OUT" --prepare-only

PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-crypto-baseline"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
HARNESS="$ROOT/host-tools/qemu-test/run.sh"
WORKLOAD="$ROOT/tests/performance/stage1_capsule_workload.py"
RUN="$OUT/run"
CAPSULE="$OUT/capsule.bin"
SAMPLES="$OUT/samples.jsonl"
TIMESTAMPS="$OUT/event-timestamps.jsonl"
REPORT="$OUT/report.json"

[ -f "$PRODUCTION" ] || {
    (cd "$ROOT" && cargo build --release -p tos-nucleus --target x86_64-unknown-none)
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-crypto-baseline)
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building isolated crypto artifact" >&2
    exit 1
}

: > "$SAMPLES"
run_sample() {
    local phase=$1 index=$2
    local accel_args=()
    if [ -n "$ACCEL" ]; then accel_args=(--accel "$ACCEL"); fi
    bash "$HARNESS" --out "$RUN" --capsule "$CAPSULE" --nucleus "$TEST_NUCLEUS" \
        --expect 33 \
        --require "TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.TEST.CRYPTO.BASELINE.START TOS.TEST.CRYPTO.BASELINE.DONE" \
        --forbid "TOS.HALT TOS.PANIC" \
        --event-timestamps "$TIMESTAMPS" "${accel_args[@]}"
    local done_line crypto_bytes crypto_hashes
    done_line="$(grep '^TOS.TEST.CRYPTO.BASELINE.DONE ' "$RUN/events.log")"
    if [[ ! "$done_line" =~ ^TOS\.TEST\.CRYPTO\.BASELINE\.DONE\ bytes=([0-9]+)\ hashes=([0-9]+)$ ]]; then
        echo "invalid crypto accounting event: $done_line" >&2
        exit 1
    fi
    crypto_bytes="${BASH_REMATCH[1]}"
    crypto_hashes="${BASH_REMATCH[2]}"
    python3 "$WORKLOAD" crypto-qemu-sample --timestamps "$TIMESTAMPS" \
        --phase "$phase" --index "$index" --crypto-bytes "$crypto_bytes" \
        --crypto-hashes "$crypto_hashes" --out "$SAMPLES"
}

for index in 1 2 3; do run_sample warmup "$index"; done
for index in $(seq 1 21); do run_sample measurement "$index"; done

python3 "$WORKLOAD" qemu-crypto-report \
    --fixture "$OUT/fixture" --samples "$SAMPLES" --out "$REPORT" \
    --source-commit "$(git -C "$GITROOT" rev-parse HEAD)" \
    --rustc-version "$(rustc --version)" \
    --qemu-version "$(qemu-system-x86_64 --version | head -n 1)" \
    --ovmf-code "$RUN/OVMF_CODE.fd" --ovmf-vars "$RUN/OVMF_VARS.fd" \
    --accelerator "${ACCEL:-tcg}" --evidence-status "$EVIDENCE_STATUS"

python3 - "$REPORT" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
stats = report["statistics"]
accounting = report["crypto_accounting"]
print(
    "STAGE1-QEMU-CRYPTO-RESEARCH: "
    f"median={stats['median_ns'] / 1_000_000:.3f}ms "
    f"p95={stats['p95_ns'] / 1_000_000:.3f}ms "
    f"p99={stats['p99_ns'] / 1_000_000:.3f}ms "
    f"bytes={accounting['bytes_per_boot']} hashes={accounting['hashes_per_boot']} "
    f"report={sys.argv[1]}"
)
PY
