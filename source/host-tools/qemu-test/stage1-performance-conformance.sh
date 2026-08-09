#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Retained Stage 1 ADR-0026 full/crypto conformance evidence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/target/stage1-performance-conformance"
EVIDENCE_STATUS="P1"
WORKLOAD="$ROOT/tests/performance/stage1_capsule_workload.py"

usage() {
    cat <<'EOF'
Usage: bash host-tools/qemu-test/stage1-performance-conformance.sh [--out DIR] [--evidence-status P1|P2]

Runs the accepted ADR-0026 evidence set without duplicating boot logic:
native full+crypto series, mandatory q35/qemu64/TCG full series, and the
isolated TCG unavoidable-crypto series. It retains raw samples, reports,
fixture/sidecar, the mandatory p95 <= 1.30 ratio report and TCG decomposition
under one target directory. P2 is reserved for GitHub Actions.
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
if [ "$EVIDENCE_STATUS" = P2 ] && [ "${GITHUB_ACTIONS:-}" != true ]; then
    echo "P2 evidence may only be emitted by GitHub Actions" >&2
    exit 2
fi

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
case "$OUT" in
    "$ROOT"/target/*) ;;
    *) echo "conformance evidence output must remain under $ROOT/target" >&2; exit 2 ;;
esac

bash "$ROOT/host-tools/qemu-test/stage1-native-performance.sh" \
    --out "$OUT/native" --evidence-status "$EVIDENCE_STATUS"
bash "$ROOT/host-tools/qemu-test/stage1-performance.sh" \
    --out "$OUT/qemu-full" --evidence-status "$EVIDENCE_STATUS"
bash "$ROOT/host-tools/qemu-test/crypto-baseline.sh" \
    --out "$OUT/qemu-crypto" --evidence-status "$EVIDENCE_STATUS"

python3 "$WORKLOAD" validation-ratio \
    --full "$OUT/qemu-full/report.json" \
    --crypto "$OUT/qemu-crypto/report.json" \
    --out "$OUT/qemu-crypto-ratio.json" \
    --max-p95-ratio 1.30
python3 "$WORKLOAD" decomposition \
    --report "$OUT/qemu-full/report.json" \
    --out "$OUT/qemu-segment-decomposition.json"

python3 - "$OUT" "$EVIDENCE_STATUS" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
expected_status = sys.argv[2]
native_full = json.loads((root / "native" / "full-report.json").read_text(encoding="utf-8"))
native_crypto = json.loads((root / "native" / "crypto-report.json").read_text(encoding="utf-8"))
qemu_full = json.loads((root / "qemu-full" / "report.json").read_text(encoding="utf-8"))
qemu_crypto = json.loads((root / "qemu-crypto" / "report.json").read_text(encoding="utf-8"))
ratio = json.loads((root / "qemu-crypto-ratio.json").read_text(encoding="utf-8"))
segments = json.loads((root / "qemu-segment-decomposition.json").read_text(encoding="utf-8"))
reports = (native_full, native_crypto, qemu_full, qemu_crypto)
if any(report.get("evidence_status") != expected_status for report in reports):
    raise SystemExit("evidence status differs between retained reports")
if native_full.get("source_commit") != native_crypto.get("source_commit"):
    raise SystemExit("native full/crypto source commits differ")
if native_full.get("workload") != native_crypto.get("workload"):
    raise SystemExit("native full/crypto workloads differ")
if qemu_full.get("source_commit") != qemu_crypto.get("source_commit"):
    raise SystemExit("QEMU full/crypto source commits differ")
if qemu_full.get("workload") != qemu_crypto.get("workload"):
    raise SystemExit("QEMU full/crypto workloads differ")
if ratio.get("max_p95_ratio") != 1.3 or ratio["full_over_unavoidable_crypto"]["p95_ratio"] > 1.3:
    raise SystemExit("QEMU p95 ratio does not meet ADR-0026")
if segments.get("sample_count") != 21:
    raise SystemExit("QEMU decomposition does not retain 21 samples")
summary = {
    "adr": "ADR-0026",
    "evidence_status": expected_status,
    "native": {
        "crypto_accounting": native_crypto["crypto_accounting"],
        "full_p95_ns": native_full["statistics"]["p95_ns"],
        "crypto_p95_ns": native_crypto["statistics"]["p95_ns"],
    },
    "qemu64_tcg": {
        "crypto_accounting": qemu_crypto["crypto_accounting"],
        "full_p95_ns": qemu_full["statistics"]["p95_ns"],
        "crypto_p95_ns": qemu_crypto["statistics"]["p95_ns"],
        "p95_ratio": ratio["full_over_unavoidable_crypto"]["p95_ratio"],
    },
    "source_commit": qemu_full["source_commit"],
    "workload": qemu_full["workload"],
}
(root / "conformance-summary.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print(
    "STAGE1-PERFORMANCE-CONFORMANCE PASS: "
    f"evidence={expected_status} "
    f"qemu_p95_ratio={summary['qemu64_tcg']['p95_ratio']:.3f} "
    f"summary={root / 'conformance-summary.json'}"
)
PY
