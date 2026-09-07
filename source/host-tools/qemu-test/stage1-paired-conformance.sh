#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The active Stage 1 validation-performance conformance gate (ADR-0083).
#
# **What this asserts**, on the mandatory q35/qemu64/one-vCPU/256-MiB/TCG
# profile over the 1,000-file / 16-MiB fixture:
#
#   same_artifact_full_exact_p95 / same_artifact_unavoidable_crypto_p95 <= 1.30
#
# The complete exact Stage 1 logical validation costs at most 30% more than the
# unavoidable cryptographic subset *of that same logical workload*. Both series
# come from one measurement-only nucleus image whose mode is chosen at run time,
# so linker layout, code addresses and the TCG translation environment are
# shared and cancel.
#
# **It is not the old ADR-0026 ratio with a new name.** That one divided a
# production nucleus's series by a *separately linked* nucleus's series over two
# intervals that did not share a start event, and Stage 4C falsified it as a
# construct: an inert layout displacement executing nothing moved it across its
# boundary while native execution was unmoved. It is preserved, unasserted, by
# `stage1-performance-historical.sh`. That both bounds read `1.30` is a
# coincidence of two distributions and not a carried-over decision.
#
#   bash host-tools/qemu-test/stage1-paired-conformance.sh [--out DIR]
#                                                          [--evidence-status P1|P2]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$ROOT/target/stage1-paired-conformance"
EVIDENCE_STATUS="P1"

# ADR-0083 section 9. One blocking line on the p95 ratio, and deliberately no
# second absolute line: regression is measured against the retained baseline
# below by the repository's own >15%/>30% policy, never against the constant 1.0.
MAX_P95_RATIO=1.30
# ADR-0083 section 10, from the six accepted series at d580fe9.
BASELINE_P95_RATIO=1.0076
BASELINE_MEDIAN_RATIO=0.9977

usage() { sed -n '3,24p' "$0"; }

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

# 1. The workload is the accepted one, proved against a production boot over the
#    same fixture before anything is timed. A ratio between two workloads that
#    are not the same logical operation is the defect this metric was repaired
#    to remove, and it is not detectable from the numbers afterwards.
bash "$HERE/paired-equivalence.sh" "$OUT/equivalence"

# 2. The measurement. The reporter refuses the quotient unless both series came
#    from the same image, fails the wrong sample count, and applies the bound;
#    the harness fails a sample whose guest ran a mode other than the one asked
#    for.
bash "$HERE/paired-measurement.sh" \
    --out "$OUT/paired" \
    --label "stage1-validation-performance" \
    --max-p95-ratio "$MAX_P95_RATIO" \
    --baseline-p95-ratio "$BASELINE_P95_RATIO" \
    --baseline-median-ratio "$BASELINE_MEDIAN_RATIO" \
    --evidence-status "$EVIDENCE_STATUS"

# 3. The retained report says what this gate claims. A report produced without
#    the bound is reproduction evidence, and must not be presentable as a
#    conformance result by having been written to the conformance directory.
python3 - "$OUT" "$EVIDENCE_STATUS" "$MAX_P95_RATIO" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
expected_status = sys.argv[2]
expected_max = float(sys.argv[3])
report = json.loads((root / "paired" / "paired-report.json").read_text(encoding="utf-8"))

if report.get("adr") != "ADR-0083":
    raise SystemExit("the retained report is not an ADR-0083 report")
if report.get("evidence_status") != expected_status:
    raise SystemExit("the retained report's evidence status is not the requested one")
if report.get("threshold") != expected_max:
    raise SystemExit("the retained report did not apply the accepted bound")
if report.get("verdict") != "pass":
    raise SystemExit(f"ADR-0083 conformance verdict: {report.get('verdict')}")
if report.get("conformance_statistic") != "p95 ratio":
    raise SystemExit("the conformance statistic is the p95 ratio, not something else")
same = report["same_artifact"]
if not same["equal"] or same["full_image_sha256"] != same["crypto_image_sha256"]:
    raise SystemExit("the two series did not come from the same image")
discipline = report["discipline"]
if discipline["warmups"] != 3 or discipline["samples"] != 21:
    raise SystemExit("the accepted 3-warmup/21-sample discipline was not used")
for series in ("full_exact", "unavoidable_crypto"):
    if len(report[series]["measured_ns"]) != discipline["samples"]:
        raise SystemExit(f"{series} does not retain {discipline['samples']} samples")

summary = {
    "record_spdx_license": "CC-BY-SA-4.0",
    "adr": "ADR-0083",
    "metric": "same-artifact paired Stage 1 validation performance",
    "evidence_status": expected_status,
    "conformance_statistic": "p95 ratio",
    "threshold": expected_max,
    "verdict": report["verdict"],
    "p95_ratio": report["ratio_p95"],
    "median_ratio_diagnostic": report["ratio_median"],
    "regression": report["regression"],
    "image_sha256": same["full_image_sha256"],
    "capsule_sha256": report["fixture"]["capsule_sha256"],
    "source_commit": report["source"]["commit"],
    "superseded_metric": (
        "ADR-0026's cross-artifact ratio is historical evidence and is no "
        "longer active conformance; see stage1-performance-historical.sh"
    ),
}
(root / "conformance-summary.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print(
    "STAGE1-PAIRED-CONFORMANCE PASS: "
    f"evidence={expected_status} "
    f"p95_ratio={summary['p95_ratio']:.3f} <= {expected_max:.2f} "
    f"median_ratio={summary['median_ratio_diagnostic']:.3f} "
    f"summary={root / 'conformance-summary.json'}"
)
PY
