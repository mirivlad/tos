#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# ADR-0103's reference measurement: R1, R2 and R3 of `docs/35` §Stage 4, measured on
# the production `block.device.v1` path and on the isolated Rust oracle, by one
# external observer, and compared.
#
# **Two boots, one machine, one instrument.** Both run through `run.sh` with the
# Stage 4 device profile, so the QEMU command is the same one every Stage 4 gate
# boots, and both are read by `measure-stage4-reference.py` under the observer
# `build-stage4-observer.sh` builds. The only difference between them is the EFI
# application the firmware starts: the TOS loader with the measurement capsule, or
# the oracle. The data disk is the same 16 MiB pattern, byte for byte, in both.
#
# **What the TOS boot is.** The production service and runtime path, with two
# measurement-only builds that `run.sh` hashes the production artifacts around:
#
#   nucleus         `test-block-protocol` (the launcher constant every Stage 4
#                   protocol gate uses) and `test-measurement-port` (COM1's eight
#                   ports in the TSS bitmap, IOPL 0 — ADR-0066 §4)
#   runtime image   `test-block-reference`: marks on the wire around every call to
#                   `measured_window` / `measured_operation`, and nothing else
#
# and the capsule is `block-protocol/init.tos`, `block-reference/service.tos` (the
# accepted service with its request count and fuel budget changed) and
# `block-reference/client.tos` (the plan). Every audit line is still produced and
# relayed: this measures the system as it is, not a quieter one.
#
# **What is checked before anything is computed:** both boots exit as they should,
# both digests of the measured sector sequence are equal, the TOS client completed
# with that digest, the oracle's interrupt account balances, the disk images are
# identical, and every window the observer recorded matches the predeclared plan.
# Then R1 = TOS/oracle throughput (>= 0.35), R2 = TOS/oracle p99 (<= 5), R3 =
# TOS/oracle CPU per MiB (<= 8). Thresholds are ADR-0103's and are not parameters.
#
# **Exit status.** 0: a valid measurement and every budget met. 4: a valid
# measurement with at least one budget missed — a result, recorded, not an
# instrument failure (ADR-0066 §6). Anything else: the measurement is not
# evidence.
#
#   bash host-tools/qemu-test/stage4-reference-performance.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-stage4-reference}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
OBSERVER="${TOS_STAGE4_OBSERVER:-$ROOT/target/qemu-stage4-observer/bin}"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION_NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
PRODUCTION_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
TARGET="$ROOT/target/test-stage4-reference"
ORACLE="$ROOT/target/x86_64-unknown-uefi/release/tos-virtio-block-oracle.efi"
# Long enough for ~8700 textual READs under TCG, which is the thing being measured.
TIMEOUT=3600

fail() { echo "stage4-reference: FAIL: $*" >&2; exit 1; }
cleanup() { rm -rf "$TARGET"; }
trap cleanup EXIT

[ -x "$OBSERVER/qemu-system-x86_64" ] && [ -f "$OBSERVER/observer-build.json" ] ||
    fail "no Stage 4 observer at $OBSERVER (build-stage4-observer.sh)"
python3 - "$OBSERVER/observer-build.json" "$HERE/stage4-observer-patch.py" <<'OBSERVER' ||
import json, re, sys
record = json.load(open(sys.argv[1]))
patch = open(sys.argv[2]).read()
pins = dict(re.findall(r'^(\w+_(?:UPSTREAM|MODIFIED)) = "([0-9a-f]{64})"$', patch, re.M))
wanted = [(entry["path"], entry["upstream_sha256"], entry["modified_sha256"])
          for entry in record.get("observer_modifications", [])]
expected = [("hw/char/serial.c", pins["SERIAL_UPSTREAM"], pins["SERIAL_MODIFIED"]),
            ("hw/char/trace-events", pins["EVENTS_UPSTREAM"], pins["EVENTS_MODIFIED"])]
if wanted != expected or record.get("trace_event") != "tos_block_window":
    sys.exit("stage4-reference: FAIL: the observer build is not the pinned Stage 4 observer")
OBSERVER
    exit 1
export PATH="$OBSERVER:$PATH"

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
for artifact in "$PRODUCTION_NUCLEUS" "$PRODUCTION_IMAGE"; do
    [ -f "$artifact" ] || { echo "missing production artifact: $artifact" >&2; exit 2; }
done
nucleus_before="$(sha256sum "$PRODUCTION_NUCLEUS" | awk '{print $1}')"
image_before="$(sha256sum "$PRODUCTION_IMAGE" | awk '{print $1}')"

(cd "$ROOT" && cargo build --release -p tos-virtio-block-oracle \
    --target x86_64-unknown-uefi >/dev/null 2>&1) || fail "the oracle does not build"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET/nucleus" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none \
    --features test-block-protocol,test-measurement-port >/dev/null 2>&1) ||
    fail "the measurement nucleus does not build"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET/image" cargo build --release \
    -p tos-runtime-image --target x86_64-unknown-none \
    --features test-block-reference >/dev/null 2>&1) ||
    fail "the measurement runtime image does not build"
[ "$nucleus_before" = "$(sha256sum "$PRODUCTION_NUCLEUS" | awk '{print $1}')" ] &&
    [ "$image_before" = "$(sha256sum "$PRODUCTION_IMAGE" | awk '{print $1}')" ] ||
    fail "a production artifact changed while the measurement artifacts were built"
NUCLEUS="$TARGET/nucleus/x86_64-unknown-none/release/tos-nucleus"
IMAGE="$TARGET/image/x86_64-unknown-none/release/tos-runtime-image"

# The data disk: sector s holds le64(s) and then (s + i) mod 256 for i = 8..511.
# The warm-up of both sides checks the first word, so the READs are proved to be
# the device's before any of them is timed.
python3 - "$OUT/pattern.img" <<'PATTERN'
import struct, sys
with open(sys.argv[1], "wb") as image:
    for sector in range(32768):
        image.write(struct.pack("<Q", sector) + bytes((sector + i) & 0xff for i in range(8, 512)))
PATTERN

{
    printf '/system/boot/init.tos\t%s/tests/vectors/block-protocol/init.tos\n' "$ROOT"
    printf '/system/service/block.tos\t%s/tests/vectors/block-reference/service.tos\n' "$ROOT"
    printf '/system/client/block.tos\t%s/tests/vectors/block-reference/client.tos\n' "$ROOT"
} > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/capsule.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/capsule.bin" --manifest "$OUT/capsule.meta.json" >/dev/null

# The oracle first: it takes seconds, and a broken instrument should be found
# before an hour of TOS READs is spent on it. Retained windows never overlap:
# the two boots run one after the other.
bash "$HERE/run.sh" --out "$OUT/oracle" --capsule "$OUT/capsule.bin" \
    --loader "$ORACLE" --stage4-block-device --stage4-block-overlay "0:$OUT/pattern.img" \
    --measure-stage4 --timeout 600 >/dev/null ||
    fail "the oracle boot is not a valid measurement (see $OUT/oracle)"
bash "$HERE/run.sh" --out "$OUT/tos" --capsule "$OUT/capsule.bin" \
    --nucleus "$NUCLEUS" --runtime-image "$IMAGE" \
    --stage4-block-device --stage4-block-overlay "0:$OUT/pattern.img" \
    --measure-stage4 --timeout "$TIMEOUT" >/dev/null ||
    fail "the TOS boot is not a valid measurement (see $OUT/tos)"

python3 "$HERE/stage4-reference-report.py" \
    --repository "$GITROOT" --out "$OUT" \
    --tos "$OUT/tos" --oracle "$OUT/oracle" \
    --capsule-meta "$OUT/capsule.meta.json" --pattern "$OUT/pattern.img" \
    --nucleus "$NUCLEUS" --runtime-image "$IMAGE" --oracle-efi "$ORACLE" \
    --production-nucleus-sha256 "$nucleus_before" \
    --production-runtime-image-sha256 "$image_before" \
    --quantum-source "$ROOT/nucleus/src/apic.rs"
