#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0083 §6/§9: the paired workloads are the real ones, proved mechanically.
#
# **What changed, and why the old proof no longer fits.** An earlier form of
# FULL_EXACT fell through into the ordinary production boot, so equality of the
# ordered event sequence with a production boot was the right proof. FULL_EXACT
# is now the *two-validator logical workload* — two fresh parses, two fresh
# whole-capsule digests, the canonical lookup taken from the second parse, and a
# fresh boot-text digest — which is deliberately not the shape of one production
# boot. Event-sequence equality would now be the wrong question.
#
# So this proves the workload instead: what the measurement artifact did, and
# that the values it produced are the production values for the same fixture.
#
#   bash host-tools/qemu-test/paired-equivalence.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:-$ROOT/target/paired-equivalence}"
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"

fail() { echo "paired-equivalence: FAIL: $*" >&2; exit 1; }

PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
[ -f "$PRODUCTION" ] || fail "missing production nucleus: $PRODUCTION"
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
MEASURE_TARGET="$ROOT/target/test-paired-measurement"
(cd "$ROOT" && CARGO_TARGET_DIR="$MEASURE_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-paired-measurement) >&2
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
    fail "the production nucleus changed while building the measurement artifact"
MEASURE="$MEASURE_TARGET/x86_64-unknown-none/release/tos-nucleus"

FIXTURE="$OUT/fixture"
[ -f "$FIXTURE/capsule.bin" ] || bash "$HERE/stage1-performance.sh" --out "$FIXTURE" --prepare-only >&2
CAPSULE="$FIXTURE/capsule.bin"

boot() { # $1 = label, $2 = nucleus, $3.. = extra run.sh args
    local label="$1" nucleus="$2"; shift 2
    local dir="$OUT/$label"
    rm -rf "$dir"; mkdir -p "$dir"
    bash "$HERE/run.sh" --out "$dir" --capsule "$CAPSULE" --nucleus "$nucleus" \
        --expect 33 "$@" >/dev/null 2>&1 || fail "$label did not reach RESULT_HALT_OK"
    echo "$dir/events.log"
}

field() { # $1 = log, $2 = event, $3 = key
    awk -v ev="$2" -v key="$3" '
        $1 == ev { for (i = 2; i <= NF; i++) { split($i, kv, "="); if (kv[1] == key) { print kv[2] } } }
    ' "$1" | awk 'NR==1'
}

check() { # $1 = description, $2 = expected, $3 = actual
    [ -n "$2" ] || fail "$1: nothing to compare against"
    [ "$2" = "$3" ] || fail "$1: expected '$2', measurement reported '$3'"
    printf '  %-34s %s\n' "$1" "$2"
}

PROD_LOG="$(boot production "$PRODUCTION")"
FULL_LOG="$(boot full "$MEASURE" --fw-cfg "opt/tos/measurement-mode=full" \
    --require "TOS.BOOT.ENTRY TOS.NUCLEUS.ENTRY TOS.TEST.PAIRED.START TOS.TEST.PAIRED.FULL.DONE")"
CRYPTO_LOG="$(boot crypto "$MEASURE" --fw-cfg "opt/tos/measurement-mode=crypto" \
    --require "TOS.BOOT.ENTRY TOS.NUCLEUS.ENTRY TOS.TEST.PAIRED.START TOS.TEST.CRYPTO.BASELINE.DONE")"

echo "PAIRED-EQUIVALENCE: the measured workloads, against production"

# ---- the shape of the numerator's workload -----------------------------------
# Reported by the guest that performed it, so this is what ran rather than what
# the source is believed to say.
check "fresh production parses"        2 "$(field "$FULL_LOG" TOS.TEST.PAIRED.FULL.DONE parses)"
check "fresh whole-capsule digests"    2 "$(field "$FULL_LOG" TOS.TEST.PAIRED.FULL.DONE capsule_digests)"
check "lookup taken from"         second "$(field "$FULL_LOG" TOS.TEST.PAIRED.FULL.DONE lookup_from)"

# ---- and its values are production's -----------------------------------------
check "files validated"  "$(field "$PROD_LOG" TOS.CAPSULE.OK files)" \
                         "$(field "$FULL_LOG" TOS.TEST.PAIRED.FULL.DONE files)"
check "canonical path"   "$(awk '$1=="TOS.BOOTTEXT.PATH"{print $2; exit}' "$PROD_LOG")" \
                         "$(field "$FULL_LOG" TOS.TEST.PAIRED.FULL.DONE path)"
check "boot-text digest" "$(awk '$1=="TOS.BOOTTEXT.DIGEST"{print $2; exit}' "$PROD_LOG")" \
                         "$(field "$FULL_LOG" TOS.TEST.PAIRED.FULL.DONE boot_digest)"
check "capsule digest"   "$(field "$PROD_LOG" TOS.IDENTITY capsule_digest)" \
                         "$(sha256sum "$CAPSULE" | cut -d' ' -f1)"

# ---- the denominator's accounting is the accepted one -------------------------
check "unavoidable-crypto bytes"   101203397 "$(field "$CRYPTO_LOG" TOS.TEST.CRYPTO.BASELINE.DONE bytes)"
check "unavoidable-crypto hashes"       2007 "$(field "$CRYPTO_LOG" TOS.TEST.CRYPTO.BASELINE.DONE hashes)"

# ---- both modes share the boundary and the prefix -----------------------------
for log in "$FULL_LOG" "$CRYPTO_LOG"; do
    grep -q '^TOS\.TEST\.PAIRED\.START$' "$log" ||
        fail "a mode did not emit the common measurement boundary"
done
printf '  %-34s %s\n' "common boundary in both modes" "TOS.TEST.PAIRED.START"

# ---- no result crosses between the two passes ---------------------------------
# Structural, and checked in the source because it is a property of scope rather
# than of output: the first parsed view is bound inside a block that yields only
# its file count, so nothing parsed in pass one is nameable in pass two.
awk '/fn paired_full_exact/,/^}/' "$ROOT/nucleus/src/main.rs" | grep -q 'let first_files = {' ||
    fail "the first validation pass is no longer scoped away from the second"
printf '  %-34s %s\n' "pass 1 scoped out of pass 2" "yes"

# ---- nothing is reimplemented -------------------------------------------------
sites="$(grep -rn 'test-paired-measurement' "$ROOT/nucleus/src" | wc -l)"
[ "$sites" -le 14 ] ||
    fail "the measurement feature has grown to $sites sites in ring 0"
selector_code="$(sed -e 's://.*::' "$ROOT/nucleus/src/measurement.rs")"
for forbidden in sha256 verify parse validate digest capsule; do
    if printf '%s' "$selector_code" | grep -qi "$forbidden"; then
        fail "measurement.rs executes '$forbidden': the selector must not reimplement measured work"
    fi
done
# The orchestration may sequence the production pieces, and must contain no
# second implementation of any of them.
orchestration="$(awk '/fn paired_full_exact/,/^}$/' "$ROOT/nucleus/src/main.rs"
                 awk '/fn paired_unavoidable_crypto/,/^}$/' "$ROOT/nucleus/src/main.rs")"
for required in 'sha256(' 'parse(' 'boot_file()' 'verify_parser_crypto('; do
    printf '%s' "$orchestration" | grep -qF "$required" ||
        fail "the orchestration does not call the production '$required'"
done
printf '  %-34s %s\n' "no duplicated algorithm" "$sites cfg sites, production calls only"

echo "PAIRED-EQUIVALENCE PASS: FULL_EXACT performs two fresh production"
echo "  validations and takes the canonical lookup from the second, producing the"
echo "  same file count, path and boot-text digest as production;"
echo "  UNAVOIDABLE_CRYPTO reports the exact accepted accounting; both modes"
echo "  share one measurement boundary after an identical untimed prefix"
