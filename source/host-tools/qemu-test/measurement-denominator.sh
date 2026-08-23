#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Milestone 6 of ADR-0066: the denominator, measured — and not yet judged.
#
# `IPC_V1` §8 fixes what the relative budget is measured against: "a call to an
# exported TOS Core function taking one 64-byte value parameter and returning
# `unit`, executed by the same engine build, in the same process". This runs
# that call between the same two markers as an adjacent empty observation, on
# the same clock, in the same prepared process and in one boot.
#
# Every predeclared block contains one floor and one call with the same sequence
# identity. Their order alternates by block. Neither is subtracted from the
# other: adjacency controls boot, preparation and drift; the pair only decides
# whether this instrument resolves the immutable denominator.
#
# Nothing here decides whether `8×` is provable. That needs the numerator, and
# the numerator is not measured until the denominator is known to be resolvable.
#
#   bash host-tools/qemu-test/measurement-denominator.sh [--out DIR] [--evidence-status P1|P2]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
HERE="$ROOT/host-tools/qemu-test"
OUT="$ROOT/target/qemu-measurement-denominator"
EVIDENCE_STATUS=P1

while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --evidence-status) EVIDENCE_STATUS="$2"; shift 2 ;;
        -h|--help) sed -n '3,24p' "$0"; exit 0 ;;
        --*) echo "unknown option: $1" >&2; exit 2 ;;
        *) OUT="$1"; shift ;;
    esac
done
case "$EVIDENCE_STATUS" in
    P1|P2) ;;
    *) echo "invalid evidence status: $EVIDENCE_STATUS" >&2; exit 2 ;;
esac

TOOL="$ROOT/target/release/tos-capsule-tool"
FIXTURE="$ROOT/tests/vectors/measurement"
PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TEST_TARGET="$ROOT/target/test-measurement-call"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
TEST_IMAGE="$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image"

fail() {
    echo "measurement-denominator: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
for artifact in "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE"; do
    [ -f "$artifact" ] || {
        echo "missing production artifact: $artifact" >&2
        exit 2
    }
done
production_nucleus_before="$(sha256sum "$PRODUCTION_NUCLEUS" | cut -d' ' -f1)"
production_image_before="$(sha256sum "$PRODUCTION_IMAGE" | cut -d' ' -f1)"
before="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none \
    --features test-measurement-no-preemption)
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features test-measurement-call)

after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] ||
    fail "a production artifact changed while building the measurement ones"

# The benchmark's text travels the way every other module's text does: in the
# capsule, beside the boot module, as canonical source. It is not compiled into
# the image, because a benchmark the image carried would be a benchmark nobody
# could read from the system's own source.
mkdir -p "$OUT"
printf '/system/boot/init.tos\t%s\n/system/bench/call.tos\t%s\n' \
    "$ROOT/system/boot/init.tos" "$FIXTURE/call.tos" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/measurement.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/measurement.bin" --manifest "$OUT/capsule.meta.json"

BUILD_MANIFEST="$OUT/measurement-build.json"
python3 "$HERE/write-measurement-build-manifest.py" \
    --repository "$GITROOT" \
    --target-dir "$TEST_TARGET" \
    --nucleus "$TEST_NUCLEUS" \
    --nucleus-features test-measurement-no-preemption \
    --runtime-image "$TEST_IMAGE" \
    --runtime-features test-measurement-call \
    --out "$BUILD_MANIFEST"

# One boot, one prepared engine, and adjacent floor/call observations. The tag
# selects only whether the already-prepared Work executes its immutable call.
bash "$HERE/run.sh" \
    --out "$OUT/paired" \
    --capsule "$OUT/measurement.bin" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$TEST_IMAGE" \
    --production-nucleus-before-sha256 "$production_nucleus_before" \
    --production-runtime-image-before-sha256 "$production_image_before" \
    --measurement-evidence-status "$EVIDENCE_STATUS" \
    --measurement-build-manifest "$BUILD_MANIFEST" \
    --measurement-paired-calibration \
    --measure 21 > "$OUT/paired.out" 2>&1 || { cat "$OUT/paired.out" >&2; fail "the paired boot did not complete"; }
grep -E "^measure-channel:" "$OUT/paired.out" || true

# Every call has to have happened. A run whose engine refused them would show
# the same interval shape and mean nothing.
# The serial log carries the marker bytes as well as the text, so the line is
# read out of the printable part of it rather than out of the raw stream.
calls="$(tr -c '[:print:]\n' ' ' < "$OUT/paired/serial.log" |
    sed -n 's/.*TOS\.RUN\.MEASURE\.ANSWERED samples=\([0-9]*\) calls=\([0-9]*\) refused=\([0-9]*\).*/\1 \2 \3/p' |
    tail -1)"
[ -n "$calls" ] || fail "the measured process did not report what its calls did"
set -- $calls
[ "$3" = 0 ] || fail "$3 of the engine's calls were refused"
[ "$1" = 48 ] || fail "$1 answers, expected 48 adjacent observations"
[ "$2" = 24 ] || fail "$2 calls, expected one in each of 24 blocks"

python3 - "$OUT/paired/measurement.json" <<'PYTHON'
import json, statistics, sys

report = json.load(open(sys.argv[1]))
if report["subtracted"] != "nothing":
    raise SystemExit(f"{sys.argv[1]}: something was subtracted")
floor = report["floor_samples_us"]
call = report["samples_us"]

def line(name, values):
    ordered = sorted(values)
    print(
        f"  {name:<10} n={len(values)}  median {statistics.median(values):8.3f}  "
        f"p99 {ordered[-1]:8.3f}  min {ordered[0]:8.3f}  max {ordered[-1]:8.3f}"
    )

print("raw samples, microseconds, in the order they were taken:")
print("  floor:", " ".join(f"{value:.3f}" for value in floor))
print("  call :", " ".join(f"{value:.3f}" for value in call))
print("distribution:")
line("floor", floor)
line("call", call)
print("ratios, floor over call — the instrument against what it must resolve:")
print(
    f"  median {statistics.median(floor) / statistics.median(call):.3f}   "
    f"p99 {sorted(floor)[-1] / sorted(call)[-1]:.3f}"
)
print("nothing above is corrected, subtracted or filtered.")
PYTHON

echo "MEASUREMENT-DENOMINATOR: measured, not judged"
echo "  the call is the one IPC_V1 section 8 names: exported, 64-byte value, unit result"
echo "  same boot, process, clock and markers; adjacent pair order alternates by block"
