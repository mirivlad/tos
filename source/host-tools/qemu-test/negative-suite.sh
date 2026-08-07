#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS Stage 1 negative boot suite.
#
# Boots every rejected fixture listed in tests/vectors/capsule-v1/vectors.tsv
# and requires each one to fail closed in the loader:
#
#   * QEMU exits 67 (RESULT_CAPSULE_INVALID);
#   * TOS.NUCLEUS.ENTRY never appears — control must not reach the nucleus
#     (enforced by run.sh --expect 67);
#   * the capsule_err reported over serial equals the error the host parser
#     declares for that fixture in vectors.tsv.
#
# The third check is the point of this suite: a unit test proving the host
# parser rejects a fixture says nothing about which rule fires inside a real
# boot. This pins them to the same rule.
#
# Usage: bash host-tools/qemu-test/negative-suite.sh [OUT_DIR]
# Exit: 0 all fixtures behaved, 1 at least one did not, 2 environment problem.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/qemu-negative}"
VECTORS="$ROOT/tests/vectors/capsule-v1"
TSV="$VECTORS/vectors.tsv"

[ -f "$TSV" ] || { echo "missing vector table: $TSV" >&2; exit 2; }
mkdir -p "$OUT"

pass=0
fail=0
failed_names=""

while IFS=$'\t' read -r name outcome; do
    case "$name" in ''|\#*) continue ;; esac
    case "$outcome" in
        accept) continue ;;
        reject:*) want_err="${outcome#reject:}" ;;
        *) echo "unknown outcome for $name: $outcome" >&2; exit 2 ;;
    esac

    log="$OUT/$name.log"
    if bash "$ROOT/host-tools/qemu-test/run.sh" \
            --out "$OUT/$name" --capsule "$VECTORS/$name" --expect 67 > "$log" 2>&1; then
        got_err="$(sed -n 's/.*capsule_err=\([A-Za-z]*\).*/\1/p' "$log" | head -1)"
        if [ "$got_err" = "$want_err" ]; then
            echo "PASS  $name  exit 67  capsule_err=$got_err"
            pass=$((pass + 1))
        else
            echo "FAIL  $name  exit 67 but capsule_err=$got_err, vectors.tsv declares $want_err"
            fail=$((fail + 1)); failed_names="$failed_names $name"
        fi
    else
        rc=$?
        echo "FAIL  $name  harness rc=$rc :: $(grep -m1 'QEMU-TEST FAIL' "$log" || echo 'see '"$log")"
        fail=$((fail + 1)); failed_names="$failed_names $name"
    fi
done < "$TSV"

echo "---"
if [ "$fail" -eq 0 ]; then
    echo "NEGATIVE-SUITE PASS: $pass fixtures failed closed with the declared rule"
    exit 0
fi
echo "NEGATIVE-SUITE FAIL: $fail of $((pass + fail)) fixtures wrong:$failed_names" >&2
exit 1
