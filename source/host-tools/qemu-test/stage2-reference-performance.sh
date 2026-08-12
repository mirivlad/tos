#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The reference half of the docs/35 Stage 2 pair, taken on the real path.
#
# ADR-0040 section 1a requires the reference measurement to be taken through the
# Stage 2 runtime path on the declared platform, not by a host process wearing
# the platform's name. The workload here is the capsule's canonical boot module:
# it goes through reader, parser, checker, resolution, lowering, the independent
# verifier and the bounded engine inside QEMU on q35/qemu64/1 vCPU/256 MiB/TCG,
# exactly as `init.tos` does.
#
# The fixtures come from `tos-core-performance --emit-fixture`, which is the
# harness that measures them natively. The two halves of a ratio must be the
# same fixture, not two fixtures that resemble each other.
#
# Time comes from host-monotonic timestamps of the `TOS.RUN.*` events the boot
# already emits, so serial transport is inside the measured span. That is stated
# rather than corrected for: the correction would be a number nobody measured.
#
#   bash host-tools/qemu-test/stage2-reference-performance.sh [OUT_DIR] [SAMPLES]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:-target/stage2-reference-performance}"
SAMPLES="${2:-21}"
TOOL="$ROOT/target/release/tos-capsule-tool"
HARNESS="$ROOT/target/release/tos-core-performance"
NOTICES="$ROOT/system/boot/NOTICES.txt"

mkdir -p "$OUT"
[ -x "$TOOL" ] || { echo "missing $TOOL (cargo build --release -p tos-capsule-tool)" >&2; exit 2; }
[ -x "$HARNESS" ] || { echo "missing $HARNESS (cargo build --release -p tos-core-performance)" >&2; exit 2; }

for workload in frontend execute reject; do
    "$HARNESS" --emit-fixture "$workload" > "$OUT/$workload.tos"
    printf '/system/boot/init.tos\t%s\n' "$OUT/$workload.tos" > "$OUT/$workload.manifest"
    "$TOOL" --detached --licence "$NOTICES" \
        --out "$OUT/$workload.bin" "$OUT/$workload.manifest" >/dev/null
done
echo "frontend fixture: $(wc -c < "$OUT/frontend.tos") bytes"

# `reject` is a module the checker refuses, so it halts with
# RESULT_BOOT_MODULE_FAILED (0x25) — QEMU exit 75 — and its required events are
# the refusal rather than a completion.
for workload in frontend execute reject; do
    if [ "$workload" = reject ]; then
        EXPECT=75
        NEED="TOS.RUN.BEGIN TOS.RUN.REFUSED"
        BAN="TOS.PANIC TOS.EXCEPTION TOS.RUN.COMPLETED"
    else
        EXPECT=33
        NEED="TOS.RUN.BEGIN TOS.RUN.COMPLETED"
        BAN="TOS.PANIC TOS.EXCEPTION TOS.RUN.REFUSED TOS.RUN.TRAP"
    fi
    for sample in $(seq 1 "$SAMPLES"); do
        bash "$HERE/run.sh" --out "$OUT/run-$workload" \
            --capsule "$OUT/$workload.bin" --expect "$EXPECT" --timeout 900 \
            --event-timestamps "$OUT/$workload-$sample.json" \
            --require "$NEED" --forbid "$BAN" >/dev/null
    done
done

python3 "$HERE/reference-performance-report.py" --out "$OUT" --samples "$SAMPLES"
echo "records: $OUT/reference.json"
