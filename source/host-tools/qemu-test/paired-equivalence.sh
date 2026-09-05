#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0083 §6: FULL_EXACT is the production path, proved mechanically.
#
# The measurement artifact is not the production nucleus, so "it runs the same
# validations" must be checked rather than asserted in prose. It is checked the
# only way that cannot drift: boot the **production** nucleus and the
# **measurement** nucleus in FULL_EXACT mode over the same capsule, and require
# that everything either of them says about the work is identical.
#
# There is no duplicated algorithm to compare, and that is the design: the
# measurement feature adds a mode selector and a branch *into* the crypto
# baseline. FULL_EXACT does not take that branch and falls through into the same
# `nucleus_main` body, calling the same capsule hashing, the same parser
# validation, the same detached identity, the same canonical lookup and the same
# boot-text digest. This gate is what proves the fall-through is real.
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

# One fixture, both boots.
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

PROD_LOG="$(boot production "$PRODUCTION")"
MEAS_LOG="$(boot measurement "$MEASURE" --fw-cfg "opt/tos/measurement-mode=full")"

# ---- the work each boot reports ----------------------------------------------
# Every field the ruling names, taken from what the guest itself said. A value
# absent from one side and present in the other is a difference, so extraction
# never defaults.
field() { # $1 = log, $2 = event, $3 = key
    awk -v ev="$2" -v key="$3" '
        $1 == ev { for (i = 2; i <= NF; i++) { split($i, kv, "="); if (kv[1] == key) { print kv[2]; exit } } }
    ' "$1"
}

check() { # $1 = description, $2 = production value, $3 = measurement value
    [ -n "$2" ] || fail "$1: production reported nothing"
    [ "$2" = "$3" ] || fail "$1: production '$2' vs measurement '$3'"
    printf '  %-28s %s\n' "$1" "$2"
}

echo "PAIRED-EQUIVALENCE: FULL_EXACT against the production boot"
check "capsule sha256"        "$(field "$PROD_LOG" TOS.IDENTITY capsule_digest)" \
                              "$(field "$MEAS_LOG" TOS.IDENTITY capsule_digest)"
check "fixture identity"      "$(field "$PROD_LOG" TOS.IDENTITY source_digest)" \
                              "$(field "$MEAS_LOG" TOS.IDENTITY source_digest)"
check "files validated"       "$(field "$PROD_LOG" TOS.CAPSULE.OK files)" \
                              "$(field "$MEAS_LOG" TOS.CAPSULE.OK files)"
check "canonical lookup"      "$(awk '$1=="TOS.BOOTTEXT.PATH"{print $2; exit}' "$PROD_LOG")" \
                              "$(awk '$1=="TOS.BOOTTEXT.PATH"{print $2; exit}' "$MEAS_LOG")"
check "boot-text digest"      "$(awk '$1=="TOS.BOOTTEXT.DIGEST"{print $2; exit}' "$PROD_LOG")" \
                              "$(awk '$1=="TOS.BOOTTEXT.DIGEST"{print $2; exit}' "$MEAS_LOG")"
# The memory account is deliberately **not** compared. The measurement artifact
# is a larger image, so it occupies one more frame and admits one fewer to the
# pool; that is a property of it being a different binary, which the ruling
# already accepts, and not a difference in validation work.

# ---- ordered phase/event identity --------------------------------------------
# The measurement artifact says one extra thing — which series it is — and that
# line is the only difference permitted. Everything else, in order, must match.
prod_seq="$(grep -oE '^TOS\.[A-Z0-9_.]+' "$PROD_LOG")"
meas_seq="$(grep -oE '^TOS\.[A-Z0-9_.]+' "$MEAS_LOG" | grep -v '^TOS\.TEST\.PAIRED\.MODE$')"
[ "$prod_seq" = "$meas_seq" ] || {
    diff <(echo "$prod_seq") <(echo "$meas_seq") | head -20
    fail "the ordered event sequences differ"
}
printf '  %-28s %s\n' "ordered event identity" "$(echo "$prod_seq" | sha256sum | cut -c1-32)"

# ---- no duplicated algorithm --------------------------------------------------
# The equivalence above shows the two boots report the same work. This shows
# there is only one implementation of it: the measurement feature may add a mode
# selector and a branch, and must not add a second copy of anything it measures.
sites="$(grep -rn 'test-paired-measurement' "$ROOT/nucleus/src" | wc -l)"
[ "$sites" -le 6 ] ||
    fail "the measurement feature has grown to $sites sites in ring 0; it must stay a selector and a branch"
# Comments stripped, as the VirtIO boundary gate does: the rule is about what
# the selector *executes*, and prose that explains why it executes nothing is
# not a violation of it.
selector_code="$(sed -e 's://.*::' "$ROOT/nucleus/src/measurement.rs")"
for forbidden in sha256 verify parse validate digest capsule; do
    if printf '%s' "$selector_code" | grep -qi "$forbidden"; then
        fail "measurement.rs executes '$forbidden': the selector must not reimplement measured work"
    fi
done
printf '  %-28s %s cfg sites, selector reimplements nothing\n' "no duplicated algorithm" "$sites"

# ---- the crypto accounting the other mode reports -----------------------------
# The denominator's declared work, recorded here so the two modes' accounting is
# on one page rather than in two harnesses.
CRYPTO_LOG="$(boot crypto "$MEASURE" --fw-cfg "opt/tos/measurement-mode=crypto" \
    --require "TOS.BOOT.ENTRY TOS.NUCLEUS.ENTRY TOS.TEST.CRYPTO.BASELINE.DONE")"
printf '  %-28s bytes=%s hashes=%s\n' "unavoidable-crypto model" \
    "$(field "$CRYPTO_LOG" TOS.TEST.CRYPTO.BASELINE.DONE bytes)" \
    "$(field "$CRYPTO_LOG" TOS.TEST.CRYPTO.BASELINE.DONE hashes)"

echo "PAIRED-EQUIVALENCE PASS: FULL_EXACT reports the same work as the production"
echo "  boot, in the same order, over the same capsule, and differs only by the"
echo "  one line naming which measured series it is"
