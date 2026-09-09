#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4C-2: a textual driver makes a DMA region, reaches it both ways, and
# gives it back — against the real nucleus, on the reference machine.
#
# ADR-0084's authority model and ADR-0085's representation, made real:
#
#   a canonical TOS Core process, holding the PCI bus root and a memory
#   authority and nothing else, claims one function, allocates a DMA region
#   from those two authorities, writes and reads a byte through it, asks the
#   nucleus for its device-visible address, releases it — and then finds both
#   stale paths closed, each in its own way.
#
# **Four boots, differing in one launcher constant each.** The positive and the
# three negatives run the *same source*; what changes is what the launcher
# decided. That is what makes each dimension refuse independently rather than
# together:
#
#   test-dma-region       everything right
#   test-dma-wrong-kind   the function binding answered by a process object
#   test-dma-unqualified  the profile does not qualify the function for DMA
#   test-dma-no-spend     the memory authority carries no `spend`
#
# A fifth boot runs the companion fixture, whose last statement is an indexed
# access through the released region: a refused operation is a value a module
# reports, and a refused access ends the run, so no one module can report both.
#
#   bash host-tools/qemu-test/dma-region.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-dma-region}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"

fail() { echo "dma-region: FAIL: $*" >&2; exit 1; }

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }

# The production nucleus must be untouched by every evidence build, so each is
# built into a target directory of its own and the production digest is compared
# before and after. A gate that quietly rebuilt the shipped artifact would be
# evidence about a different nucleus.
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
nucleus_for() {
    local feature="$1"
    local target="$ROOT/target/$feature"
    (cd "$ROOT" && CARGO_TARGET_DIR="$target" cargo build --release \
        -p tos-nucleus --target x86_64-unknown-none --features "$feature" >/dev/null 2>&1) ||
        fail "the nucleus does not build with $feature"
    [ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
        fail "the production nucleus changed while building $feature"
    echo "$target/x86_64-unknown-none/release/tos-nucleus"
}

capsule_for() {
    local name="$1" fixture="$2"
    printf '/system/boot/init.tos\t%s/init.tos\n' "$fixture" > "$OUT/$name-manifest.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$OUT/$name.bin" --meta "$OUT/$name.meta.json" \
        "$OUT/$name-manifest.txt" >/dev/null
    python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
        --capsule "$OUT/$name.bin" --manifest "$OUT/$name.meta.json" >/dev/null
}

capsule_for live "$ROOT/tests/vectors/dma-region"
capsule_for stale "$ROOT/tests/vectors/dma-region-stale"

# What the module returns, from the journal of one boot.
completed() {
    sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$OUT/$1/events.log"
}

# `SYSTEM_ABI_V1` §4. Named rather than written as a number at each use, because
# a bare -1 in an assertion is a number nobody can check against the contract.
E_NO_CAPABILITY=-1

# --- the positive: the whole life of one region --------------------------------
#
# The module reports the status of the operation it performed **after** the
# release, so completing at all means everything before it succeeded: the claim,
# the allocation, the byte written and read back, the device address, and the
# release itself.
bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/live.bin" \
    --nucleus "$(nucleus_for test-dma-region)" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

# **P1 first, because everything below depends on it and it is the one thing
# this machine does not currently provide.** ADR-0084 §5c requires a PCI Express
# capability on the function DMA is granted for: it is what makes a reclaim
# provable. On the accepted Stage 4 reference machine the function reports
# `express=0`, so operation 30 refuses with `E_NO_CAPABILITY` and no DMA
# authority can be granted through the accepted mechanism at all.
#
# This is checked before the module's own report so the failure names the cause
# rather than a status. **It is a profile decision, not a bug**: see
# `docs/evidence/STAGE4C2_CAPABILITY_REPRESENTATION.md` §6.
grep -q "TOS.RUN.PCI_ASSIGNED .*express=1" "$OUT/live/events.log" || {
    grep "TOS.RUN.PCI_ASSIGNED" "$OUT/live/events.log" >&2 || true
    fail "the reference function has no PCI Express capability, so ADR-0084 §5c's P1
       cannot hold and no DMA authority can be granted through the accepted
       mechanism. This is the STOP recorded in
       docs/evidence/STAGE4C2_CAPABILITY_REPRESENTATION.md §6 — the reference
       machine's topology is an accepted-profile decision, and turning P1 into a
       test-only assumption inside the nucleus is what that record refuses."
}

grep -q "TOS.RUN.DMA_REGION" "$OUT/live/events.log" ||
    fail "no region was made, though the two authorities were held"

value="$(completed live)"
[ "$value" = "$E_NO_CAPABILITY" ] ||
    fail "the live boot reported $value; the stale operation must answer $E_NO_CAPABILITY"
echo "dma-region: live boot: allocation, indexed access, device address, release," \
     "and a stale operation refused with $value — asserted by the nucleus"

# --- §16.6: one allocation, one capability-table entry -------------------------
#
# The host-level evidence already proves one source binding, one engine handle
# and one bridge mapping. This is the fact only the nucleus can report: that
# operation 30 added **exactly one** entry to the caller's table, and that the
# two later operations resolved the same object rather than two.
#
# **No handle value is printed.** The record carries a count and an identity
# verdict, which is what the claim is about; a raw handle on a journal would be
# a secret representation on an audit record for nobody's benefit.
grep -q "TOS.RUN.DMA_REGION .*capability_delta=1 " "$OUT/live/events.log" ||
    fail "operation 30 did not add exactly one capability-table entry"
grep -q "TOS.RUN.DMA_REGION .*aliases=0" "$OUT/live/events.log" ||
    fail "operation 30 left an alias of the region behind"
echo "dma-region: one allocation, one capability entry, no alias — asserted by nucleus" \
     "instrumentation"

# --- the stale indexed path ----------------------------------------------------
#
# The same authorities and the same lifecycle, ending in `region[0B]` after a
# successful release. The bridge holds no mapping for that handle and refuses
# **before any memory access**; the run ends in a trap rather than a value, so
# the absence of `TOS.RUN.COMPLETED` is half the evidence and the refusal code
# is the other half.
bash "$HERE/run.sh" \
    --out "$OUT/stale" \
    --capsule "$OUT/stale.bin" \
    --nucleus "$(nucleus_for test-dma-region)" \
    --stage4-block-device \
    --expect 34 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.DMA_REGION TOS.RUN.TRAP TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.COMPLETED" \
    > /dev/null

grep -q "TOS.RUN.TRAP .*RUNTIME_DEVICE_REFUSED" "$OUT/stale/events.log" ||
    fail "an indexed access through a released region was not refused by the bridge"
echo "dma-region: stale indexed access refused before memory — asserted by the runtime bridge"

# --- negative: the runtime object kind is wrong --------------------------------
#
# The source is unchanged and every static fact is right — the interface, the
# effect, the representation, the call. What differs is that the binding the
# module imports its bus under is answered by authority over a **process**.
#
# The refusal is the grant check's, which is the layer ADR-0085 §16.5 names:
# "wrong runtime object kind **at grant**". It happens before the module's first
# instruction, so no call is ever made.
bash "$HERE/run.sh" \
    --out "$OUT/wrong-kind" \
    --capsule "$OUT/live.bin" \
    --nucleus "$(nucleus_for test-dma-wrong-kind)" \
    --stage4-block-device \
    --expect 35 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.COMPLETED TOS.RUN.DMA_REGION" \
    > /dev/null
echo "dma-region: a grant of the wrong object kind is refused before the first" \
     "instruction — asserted by the runtime image against the accepted schema"

# --- negative: the function carries no `dma` right -----------------------------
#
# The profile does not qualify the reference function, so the claim produces a
# capability without `dma`. The object is right, the generation is right, the
# effect is right, the bytes are right — the one missing thing is the right, and
# the nucleus refuses operation 30 for it.
bash "$HERE/run.sh" \
    --out "$OUT/unqualified" \
    --capsule "$OUT/live.bin" \
    --nucleus "$(nucleus_for test-dma-unqualified)" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ASSIGNED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.DMA_REGION TOS.RUN.PCI_DMA_QUALIFIED" \
    > /dev/null

value="$(completed unqualified)"
[ "$value" = "$((E_NO_CAPABILITY - 100))" ] ||
    fail "a function without dma reported $value, not the allocation refusal"
echo "dma-region: operation 30 refused for a function without dma — asserted by the nucleus"

# --- negative: the authority carries no `spend` --------------------------------
#
# Independently, and it has to be independent: two authorities are required and
# a test that removed both would prove only that removing something refuses.
bash "$HERE/run.sh" \
    --out "$OUT/no-spend" \
    --capsule "$OUT/live.bin" \
    --nucleus "$(nucleus_for test-dma-no-spend)" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.DMA_REGION" \
    > /dev/null

value="$(completed no-spend)"
[ "$value" = "$((E_NO_CAPABILITY - 100))" ] ||
    fail "an authority without spend reported $value, not the allocation refusal"
echo "dma-region: operation 30 refused for an authority without spend — asserted by the nucleus"

echo "dma-region: PASS (one region's whole life, and three dimensions refusing on their own)"
