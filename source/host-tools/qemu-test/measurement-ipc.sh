#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0066 numerator: one real 64-byte request/reply per retained interval.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
HERE="$ROOT/host-tools/qemu-test"
OUT="$ROOT/target/qemu-measurement-ipc"
EVIDENCE_STATUS=P1

while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --evidence-status) EVIDENCE_STATUS="$2"; shift 2 ;;
        -h|--help) sed -n '3,20p' "$0"; exit 0 ;;
        --*) echo "unknown option: $1" >&2; exit 2 ;;
        *) OUT="$1"; shift ;;
    esac
done
case "$EVIDENCE_STATUS" in P1|P2) ;; *) echo "invalid evidence status: $EVIDENCE_STATUS" >&2; exit 2 ;; esac

PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TEST_TARGET="$ROOT/target/test-measurement-ipc"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"
TEST_IMAGE="$TEST_TARGET/x86_64-unknown-none/release/tos-runtime-image"

fail() {
    echo "measurement-ipc: FAIL: $*" >&2
    exit 1
}

for artifact in "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE"; do
    [ -f "$artifact" ] || { echo "missing production artifact: $artifact" >&2; exit 2; }
done
production_nucleus_before="$(sha256sum "$PRODUCTION_NUCLEUS" | cut -d' ' -f1)"
production_image_before="$(sha256sum "$PRODUCTION_IMAGE" | cut -d' ' -f1)"
before="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none \
    --features test-call-reply,test-measurement-port)
(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none \
    --features test-measurement-ipc)

after="$(sha256sum "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE")"
[ "$before" = "$after" ] || fail "a production artifact changed during the measurement build"

mkdir -p "$OUT"
BUILD_MANIFEST="$OUT/measurement-build.json"
python3 "$HERE/write-measurement-build-manifest.py" \
    --repository "$GITROOT" \
    --target-dir "$TEST_TARGET" \
    --nucleus "$TEST_NUCLEUS" \
    --nucleus-features test-call-reply,test-measurement-port \
    --runtime-image "$TEST_IMAGE" \
    --runtime-features test-measurement-ipc \
    --out "$BUILD_MANIFEST"

bash "$HERE/run.sh" \
    --out "$OUT/ipc" \
    --nucleus "$TEST_NUCLEUS" \
    --runtime-image "$TEST_IMAGE" \
    --production-nucleus-before-sha256 "$production_nucleus_before" \
    --production-runtime-image-before-sha256 "$production_image_before" \
    --measurement-evidence-status "$EVIDENCE_STATUS" \
    --measurement-build-manifest "$BUILD_MANIFEST" \
    --measurement-ipc \
    --measure 21 > "$OUT/ipc.out" 2>&1 || {
        cat "$OUT/ipc.out" >&2
        fail "the IPC measurement boot did not complete"
    }
grep -E '^measure-channel:' "$OUT/ipc.out" || true

printable="$(tr -c '[:print:]\n' ' ' < "$OUT/ipc/serial.log")"
client="$(printf '%s\n' "$printable" | sed -n \
    's/.*TOS\.RUN\.MEASURE\.IPC samples=\([0-9]*\) answered=\([0-9]*\) refused=\([0-9]*\) request_bytes=\([0-9]*\) reply_bytes=\([0-9]*\) primed=\([0-9]*\).*/\1 \2 \3 \4 \5 \6/p' | tail -1)"
[ -n "$client" ] || fail "the measured client did not report its exchanges"
set -- $client
[ "$1" = 24 ] && [ "$2" = 24 ] && [ "$3" = 0 ] ||
    fail "client samples=$1 answered=$2 refused=$3, expected 24/24/0"
[ "$4" = 64 ] && [ "$5" = 64 ] && [ "$6" = 1 ] ||
    fail "the client did not measure primed 64-byte request/reply"

server="$(printf '%s\n' "$printable" | sed -n \
    's/.*TOS\.RUN\.MEASURE\.IPC\.SERVER served=\([0-9]*\) refused=\([0-9]*\) payload_bytes=\([0-9]*\) last=\(-\?[0-9]*\).*/\1 \2 \3 \4/p' | tail -1)"
[ -n "$server" ] || fail "the measured server did not report its exchanges"
set -- $server
[ "$1" = 25 ] && [ "$2" = 0 ] && [ "$3" = 64 ] && [ "$4" = -5 ] ||
    fail "server served=$1 refused=$2 bytes=$3 last=$4, expected 25/0/64/-5"

cost="$(printf '%s\n' "$printable" | sed -n '/TOS\.RUN\.IPC\.COST /p' | tail -1)"
[ -n "$cost" ] || fail "the nucleus did not report IPC cost"
field() { printf '%s\n' "$cost" | tr ' ' '\n' | sed -n "s/^$1=//p"; }
messages="$(field messages)"
copies="$(field payload_copies)"
exchanges="$(field exchanges)"
ipc_in="$(field ipc_in)"
ipc_out="$(field ipc_out)"
[ "$messages" = 50 ] && [ "$exchanges" = 25 ] ||
    fail "nucleus counted messages=$messages exchanges=$exchanges, expected 50/25"
[ "$copies" -le 100 ] || fail "$copies copies for 50 inline messages exceeds two each"
[ "$ipc_in" = 51 ] && [ "$ipc_out" = 51 ] ||
    fail "IPC crossings in/out=$ipc_in/$ipc_out, expected balanced 51/51"

python3 - "$OUT/ipc/measurement.json" <<'PYTHON'
import json, sys
report = json.load(open(sys.argv[1]))
if report.get("measurement_mode") != "ipc-request-reply-v1":
    raise SystemExit("report is not the IPC numerator")
if report["environment"]["scheduler"] != {
    "preemption": "active",
    "binding": "measurement-build-manifest",
    "quantum_count": 100000,
}:
    raise SystemExit("scheduler preemption is not bound active")
if report["subtracted"] != "nothing" or report["count"] != 21 or report["warmups"] != 3:
    raise SystemExit("report changed the 3+21 individual-sample discipline")
PYTHON

echo "MEASUREMENT-IPC: measured, not yet judged"
echo "  24 real exchanges including 3 warm-ups; one additional exchange primed the server"
echo "  request=64 bytes reply=64 bytes preemption=active; nothing subtracted"
echo "  nucleus counted messages=$messages copies=$copies exchanges=$exchanges crossings=$ipc_in+$ipc_out"
