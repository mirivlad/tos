#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Milestone 1 of ADR-0066: the instrument, measured before anything is measured
# with it.
#
# No IPC here and no benchmark. The observed process answers requests with
# **nothing between its two markers**, so that the floor of the channel is known
# as a distribution rather than as a hope. Every later reading contains this
# floor and none of them may have it subtracted, which is why it is published
# first and separately.
#
# **The clock is QEMU's, not this host's.** With `-msg timestamp=on` the log
# trace backend prefixes every event with `pid@seconds.microseconds`, and
# `serial_write` is emitted by the device model while it handles the guest's
# `out` — in the vCPU thread, synchronously with the write. The socket is kept
# for the protocol alone: it carries the request that starts a sample and the
# stop that ends the run.
#
# Two earlier forms were measured and rejected, and both erred towards passing:
#
#   - the whole round trip, host request to guest answer, cost 30 µs median and
#     94 µs p99 — half a 200 µs budget spent on the way *in*, because QEMU
#     delivers a byte to the guest from its main loop while the guest hammers
#     the line-status register from the vCPU thread;
#   - two guest markers timed by a spinning host reader **understated** the
#     interval in 18 of 21 samples, by 1.67 µs at the median and 14.65 µs at the
#     worst, because a reader that is late to `OPEN` reports a shorter interval
#     than the truth. The reader's numbers are still recorded beside the
#     measurement, for exactly that comparison and never as the measurement.
#
#   bash host-tools/qemu-test/measurement-channel.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-measurement-channel}"
NUCLEUS_FEATURE=test-measurement-port
IMAGE_FEATURE=test-measurement-port

PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TEST_TARGET="$ROOT/target/test-measurement-port"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
TEST_IMAGE="$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image"

# docs/35 states the budget in microseconds, so the floor has to be small
# against it or the instrument cannot speak about it at all. This is not the
# budget: it is the point past which this channel stops being an instrument for
# a 200 µs claim, and it is asserted so that a channel which quietly became
# useless says so.
FLOOR_P99_LIMIT_US=40

fail() {
    echo "measurement-channel: FAIL: $*" >&2
    exit 1
}

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
    -p tos-nucleus --target x86_64-unknown-none --features "$NUCLEUS_FEATURE")
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none --features "$IMAGE_FEATURE")

after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] ||
    fail "a production artifact changed while building the measurement ones"
# Said twice on purpose: the measurement build opens one port to CPL 3, and the
# claim that the shipped nucleus does not is worth an assertion rather than an
# assumption.
echo "measurement-channel: production nucleus and runtime image unchanged:"
echo "$after" | sed 's/^/  /'

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$TEST_IMAGE" \
    --production-nucleus-before-sha256 "$production_nucleus_before" \
    --production-runtime-image-before-sha256 "$production_image_before" \
    --measure 21

REPORT="$OUT/measurement.json"
[ -f "$REPORT" ] || fail "the observer produced no report"

summary="$(python3 - "$REPORT" <<'PYTHON'
import json, sys

report = json.load(open(sys.argv[1]))
print(
    report["count"],
    round(report["median_us"], 3),
    round(report["p99_us"], 3),
    round(report["jitter_us"], 3),
    report["subtracted"],
)
PYTHON
)"
read -r count median p99 jitter subtracted <<EOF
$summary
EOF

[ "$count" = 21 ] || fail "$count samples, and the contract asks for 21"
[ "$subtracted" = nothing ] || fail "the observer subtracted '$subtracted'"
grep -Fq "QEMU trace timestamp" "$REPORT" ||
    fail "the report does not name QEMU's trace timestamp as its clock"

# A zero means the observer read both markers in one go and cannot tell them
# apart; a floor this wide means the host descheduled the observer. Either way
# the channel is not measuring what it claims, and neither is repairable by
# arithmetic afterwards.
python3 - "$REPORT" "$FLOOR_P99_LIMIT_US" <<'EOF' || fail "the channel floor is too coarse for a 200 us budget"
import json, sys
report = json.load(open(sys.argv[1]))
limit = float(sys.argv[2])
samples = report["samples_us"]
if min(samples) <= 0:
    print(f"a round trip measured {min(samples)} us: the reader is coalescing bytes",
          file=sys.stderr)
    raise SystemExit(1)
if report["p99_us"] > limit:
    print(f"floor p99 {report['p99_us']:.2f} us exceeds {limit} us", file=sys.stderr)
    raise SystemExit(1)
EOF

echo "MEASUREMENT-CHANNEL PASS: the channel floor is $median us median, $p99 us p99, $jitter us jitter"
echo "  21 individual samples after 3 warm-ups; both markers of every one named their request"
echo "  nothing between the markers, and nothing subtracted from any reading"
echo "  the clock is QEMU's own trace timestamp, taken while it handles the guest's write"
echo "  IOPL stays 0: the measurement nucleus clears the bitmap bits of COM1 and no others"
echo "  the production nucleus and runtime image are unchanged by this build"
