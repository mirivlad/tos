#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Versioned ADR-0066 qualification gate for the external Stage 3 observer.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/target/stage3-observer-conformance"
EVIDENCE_STATUS=P1

usage() {
    cat <<'EOF'
Usage: stage3-observer-conformance.sh [--out DIR] [--evidence-status P1|P2]

Measures adjacent empty-floor and immutable 64-byte TOS Core observations in
one prepared boot with one manifest-bound QEMU simple observer. The gate passes
only when at least 19 of 21 predeclared, alternating-order pairs resolve the
call above its adjacent floor (one-sided exact sign p <= 0.000111), floor p99 is
at most 40 us, and the observer, guest build and production-isolation identities
agree. P2 is reserved for GitHub Actions.
EOF
}

while [ "$#" -gt 0 ]; do
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
    *) echo "observer evidence output must remain under $ROOT/target" >&2; exit 2 ;;
esac

qemu="$(command -v qemu-system-x86_64 || true)"
[ -n "$qemu" ] || { echo "qemu-system-x86_64 was not found" >&2; exit 2; }
[ -f "$(dirname "$qemu")/observer-build.json" ] || {
    echo "the selected QEMU has no ADR-0066 observer-build.json" >&2
    exit 2
}

bash "$ROOT/host-tools/qemu-test/measurement-denominator.sh" \
    --out "$OUT" --evidence-status "$EVIDENCE_STATUS"
python3 "$ROOT/host-tools/qemu-test/qualify-observer.py" \
    --measurement "$OUT/paired/measurement.json" \
    --out "$OUT/qualification.json" \
    --evidence-status "$EVIDENCE_STATUS"

echo "STAGE3-OBSERVER-CONFORMANCE PASS: evidence=$EVIDENCE_STATUS"
echo "  summary=$OUT/qualification.json"
