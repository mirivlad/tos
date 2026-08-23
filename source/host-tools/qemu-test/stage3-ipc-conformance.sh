#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Versioned ADR-0066 gate for both Stage 3 IPC latency budgets.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/target/stage3-ipc-conformance"
EVIDENCE_STATUS=P1

while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --evidence-status) EVIDENCE_STATUS="$2"; shift 2 ;;
        -h|--help) sed -n '3,18p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done
case "$EVIDENCE_STATUS" in P1|P2) ;; *) echo "invalid evidence status: $EVIDENCE_STATUS" >&2; exit 2 ;; esac
if [ "$EVIDENCE_STATUS" = P2 ] && [ "${GITHUB_ACTIONS:-}" != true ]; then
    echo "P2 evidence may only be emitted by GitHub Actions" >&2
    exit 2
fi

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
case "$OUT" in "$ROOT"/target/*) ;; *) echo "IPC evidence must remain under $ROOT/target" >&2; exit 2 ;; esac

qemu="$(command -v qemu-system-x86_64 || true)"
[ -n "$qemu" ] || { echo "qemu-system-x86_64 was not found" >&2; exit 2; }
[ -f "$(dirname "$qemu")/observer-build.json" ] || {
    echo "the selected QEMU has no ADR-0066 observer-build.json" >&2
    exit 2
}

bash "$ROOT/host-tools/qemu-test/stage3-observer-conformance.sh" \
    --out "$OUT/observer" --evidence-status "$EVIDENCE_STATUS"
bash "$ROOT/host-tools/qemu-test/measurement-ipc.sh" \
    --out "$OUT/numerator" --evidence-status "$EVIDENCE_STATUS"
python3 "$ROOT/host-tools/qemu-test/qualify-ipc.py" \
    --denominator "$OUT/observer/paired/measurement.json" \
    --observer-qualification "$OUT/observer/qualification.json" \
    --numerator "$OUT/numerator/ipc/measurement.json" \
    --serial-log "$OUT/numerator/ipc/serial.log" \
    --out "$OUT/qualification.json" \
    --evidence-status "$EVIDENCE_STATUS"

echo "STAGE3-IPC-CONFORMANCE PASS: evidence=$EVIDENCE_STATUS"
echo "  summary=$OUT/qualification.json"
