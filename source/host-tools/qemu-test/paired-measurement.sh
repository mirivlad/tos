#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0083: the same-artifact paired Stage 1 validation-performance measurement.
#
# **One nucleus image supplies both series.** The mode is chosen at run time
# through a measurement-only firmware-configuration value, so linker layout,
# function placement, code addresses, static data placement and the TCG
# translation environment are shared between the numerator and the denominator
# and cancel in the quotient.
#
# The old metric put the numerator in the production nucleus and the denominator
# in a separately linked `test-crypto-baseline` nucleus. The Stage 4C
# construct-validity investigation showed that an inert layout change — one that
# executes nothing and does not alter the image length — moves that quotient
# across the conformance boundary while native execution is unmoved. Two images
# do not cancel; one image does.
#
# **This script computes no conformance verdict and proposes no threshold.** It
# measures, proves the two series came from the same bytes, and reports. The
# threshold is ADR-0083's to propose and the Project Architect's to accept.
#
#   bash host-tools/qemu-test/paired-measurement.sh [--out DIR] [--label NAME]
#                                                   [--samples N] [--warmups N]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="$ROOT/target/paired-measurement"
LABEL="paired"
SAMPLES=21
WARMUPS=3

while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --label) LABEL="$2"; shift 2 ;;
        --samples) SAMPLES="$2"; shift 2 ;;
        --warmups) WARMUPS="$2"; shift 2 ;;
        -h|--help) sed -n '3,24p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"

fail() { echo "paired-measurement: FAIL: $*" >&2; exit 1; }

# ---- one artifact, built once -------------------------------------------------
TARGET="$ROOT/target/test-paired-measurement"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
[ -f "$PRODUCTION" ] || fail "missing production nucleus: $PRODUCTION"
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-paired-measurement) >&2
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
    fail "the production nucleus changed while building the measurement artifact"
NUCLEUS="$TARGET/x86_64-unknown-none/release/tos-nucleus"
NUCLEUS_SHA="$(sha256sum "$NUCLEUS" | cut -d' ' -f1)"
NUCLEUS_BYTES="$(stat -c%s "$NUCLEUS")"

# Diagnostic identity, retained but never a threshold: the addresses this
# investigation showed the old metric was accidentally measuring.
ELF_TARGET="$ROOT/target/test-paired-measurement-elf"
(cd "$ROOT" && TOS_NUCLEUS_ELF=1 CARGO_TARGET_DIR="$ELF_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-paired-measurement) >&2
ELF="$ELF_TARGET/x86_64-unknown-none/release/tos-nucleus"
TEXT_ADDR=$(objdump -h "$ELF" | awk '$2==".text"{print $4}')
TEXT_SIZE=$(objdump -h "$ELF" | awk '$2==".text"{print $3}')
# No early-exit consumer: `pipefail` would turn the producer's SIGPIPE into a
# failure of the whole script.
COMPRESS_ADDR=$(nm -n --defined-only "$ELF" | awk '/compress_block/ && !seen {print $1; seen=1}')
ELF_BYTES=$(stat -c%s "$ELF")

# ---- the fixture, built once and shared by both series ------------------------
FIXTURE="$OUT/fixture"
if [ ! -f "$FIXTURE/capsule.bin" ]; then
    bash "$HERE/stage1-performance.sh" \
        --out "$FIXTURE" --prepare-only >&2
fi
CAPSULE="$FIXTURE/capsule.bin"
[ -f "$CAPSULE" ] || fail "the shared fixture capsule was not prepared"
CAPSULE_SHA="$(sha256sum "$CAPSULE" | cut -d' ' -f1)"

# ---- one series ---------------------------------------------------------------
# `end` closes the timed interval for this mode; both series start at
# TOS.NUCLEUS.ENTRY, so both cover the same component of the same image. The old
# metric started its numerator at TOS.BOOT.ENTRY — including the UEFI loader,
# a different binary doing its own hashing — and its denominator at
# TOS.TEST.CRYPTO.BASELINE.START, which is a sub-interval of the nucleus alone.
series() { # $1 = mode word, $2 = end event, $3 = out dir, $4 = required events
    local mode="$1" end="$2" dir="$3" require="$4"
    mkdir -p "$dir"
    : > "$dir/samples.tsv"
    local index total=$((WARMUPS + SAMPLES))
    for index in $(seq 1 "$total"); do
        local phase=measurement
        [ "$index" -le "$WARMUPS" ] && phase=warmup
        local run="$dir/run-$index"
        rm -rf "$run"; mkdir -p "$run"
        bash "$HERE/run.sh" --out "$run" --capsule "$CAPSULE" --nucleus "$NUCLEUS" \
            --expect 33 --event-timestamps "$run/timestamps.jsonl" \
            --require "$require" \
            --fw-cfg "opt/tos/measurement-mode=$mode" >/dev/null 2>&1 ||
            fail "$mode sample $index did not reach RESULT_HALT_OK"
        # The guest states which series it believes it is in. A harness that
        # mislabelled a sample is caught here rather than in the arithmetic.
        local reported
        reported="$(awk 'match($0, /^TOS\.TEST\.PAIRED\.MODE mode=[a-z-]+/) && !seen {split($2,kv,"="); print kv[2]; seen=1}' "$run/events.log")"
        [ -n "$reported" ] || fail "$mode sample $index did not report its mode"
        case "$mode:$reported" in
            full:full-exact|crypto:unavoidable-crypto) ;;
            *) fail "sample $index was asked for '$mode' and ran '$reported'" ;;
        esac
        local ns
        ns="$(python3 "$HERE/paired-interval.py" \
                --timestamps "$run/timestamps.jsonl" --end "$end")"
        [ -n "$ns" ] || fail "$mode sample $index produced no interval"
        printf '%s\t%s\t%s\n' "$phase" "$index" "$ns" >> "$dir/samples.tsv"
        # Keep the small evidence — what the guest said and when — and drop the
        # per-sample firmware, ESP and capsule copies. A 3+21 series over a
        # 16 MiB capsule otherwise retains gigabytes of identical bytes, and the
        # bytes that matter are already identified by their digests.
        mkdir -p "$dir/evidence"
        cp "$run/events.log" "$dir/evidence/events-$index.log"
        cp "$run/timestamps.jsonl" "$dir/evidence/timestamps-$index.jsonl"
        rm -rf "$run"
    done
}

# Each mode's required events are the ones that mode actually reaches. FULL_EXACT
# is the ordinary successful boot; UNAVOIDABLE_CRYPTO halts as soon as it has
# performed the accepted unavoidable cryptographic work, so it never reaches the
# canonical lookup or the launcher.
series full   TOS.BOOTTEXT.PATH             "$OUT/full-exact" \
    "TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.TEST.PAIRED.MODE TOS.CAPSULE.OK TOS.BOOTTEXT.PATH TOS.BOOTTEXT.DIGEST TOS.IDENTITY TOS.HALT"
series crypto TOS.TEST.CRYPTO.BASELINE.DONE "$OUT/unavoidable-crypto" \
    "TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.TEST.PAIRED.MODE TOS.TEST.CRYPTO.BASELINE.START TOS.TEST.CRYPTO.BASELINE.DONE"

python3 "$HERE/paired-report.py" \
    --label "$LABEL" \
    --full "$OUT/full-exact/samples.tsv" \
    --crypto "$OUT/unavoidable-crypto/samples.tsv" \
    --full-image-sha256 "$NUCLEUS_SHA" \
    --crypto-image-sha256 "$NUCLEUS_SHA" \
    --image-bytes "$NUCLEUS_BYTES" \
    --elf-bytes "$ELF_BYTES" \
    --text-addr "$TEXT_ADDR" --text-size "$TEXT_SIZE" \
    --compress-block-addr "$COMPRESS_ADDR" \
    --capsule-sha256 "$CAPSULE_SHA" \
    --warmups "$WARMUPS" --samples "$SAMPLES" \
    --repository "$GITROOT" \
    --out "$OUT/paired-report.json"
