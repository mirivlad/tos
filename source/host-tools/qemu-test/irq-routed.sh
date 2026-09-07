#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4C-1b: a textual driver waits for its own device, and the device wakes it.
#
# ADR-0082 §1's smallest unreachable operation, made reachable:
#
#   a canonical TOS Core process, holding authority over exactly one PCI
#   function and nothing else, blocks; the real device that function names
#   raises a real interrupt; the process resumes because of it; and the record
#   shows the delivery arrived through the authority it holds rather than
#   through anything the host supplied.
#
# **What makes the interrupt happen, and why that is not the host supplying the
# answer.** A configuration change is something the outside world does to a
# device — a disk is resized — and it is the one event this device can report
# with no queue, no descriptor and no DMA, all of which are Stage 4C-2 and
# beyond. So the harness resizes the disk *after the guest's own scheduler has
# announced that it is blocked with nothing runnable*, and everything from there
# is the machine's: the device raises MSI-X, the message lands on the vector the
# nucleus programmed into the table, the handler finds the source, and the
# source's one waiter is woken. No value crosses into the guest, and nothing the
# guest reads comes from this script.
#
#   bash host-tools/qemu-test/irq-routed.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-irq-routed}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-pci-discovery"

fail() { echo "irq-routed: FAIL: $*" >&2; exit 1; }

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-pci-discovery)
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
    exit 1
}
NUCLEUS="$TARGET/x86_64-unknown-none/release/tos-nucleus"

# The guest's own announcement that it is blocked and only hardware can change
# that (`SYSTEM_ABI_V1` §6, ADR-0082 §8). **This is the whole trigger**: the
# scheduler says `routed=1` because it asked the wait what could still end it
# and the answer was a live source. Waiting for `routed=0` would be waiting for
# a system that had stalled.
READY='TOS\.RUN\.LIVENESS blocked=1 routed=1 verdict=awaiting-hardware'

# Builds a capsule from one fixture and runs it, echoing what the module reported.
run() {
    local name="$1" fixture="$2" expect="$3"; shift 3
    local out="$OUT/$name"
    rm -rf "$out"; mkdir -p "$out"
    printf '/system/boot/init.tos\t%s/tests/vectors/%s/init.tos\n' "$ROOT" "$fixture" \
        > "$out/manifest.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$out/fixture.bin" --meta "$out/meta.json" "$out/manifest.txt" > /dev/null
    python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
        --capsule "$out/fixture.bin" --manifest "$out/meta.json" > /dev/null
    bash "$HERE/run.sh" --out "$out" --capsule "$out/fixture.bin" \
        --nucleus "$NUCLEUS" --expect "$expect" "$@" > /dev/null
    sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$out/events.log"
}

# --- the positive: a real interrupt from the real device -----------------------
#   1  the first wait returned OK
# + 2  the device's configuration generation moved across it
# + 4  a second wait returned OK, from a second real interrupt
value="$(run routed virtio-msix-wait 33 --stage4-block-device \
    --timeout 90 --await-line "$READY" \
    --then-qmp '{"execute":"block_resize","arguments":{"device":"stage4blk","size":33554432}}' \
    --then-qmp '{"execute":"block_resize","arguments":{"device":"stage4blk","size":50331648}}')"
LOG="$OUT/routed/events.log"
[ -n "$value" ] || fail "the driver reported nothing"
[ "$value" -gt 0 ] || fail "the driver reported a refusal: $value"
[ "$value" = 7 ] || fail "the driver did not complete the routed wait: $value"

# --- and the record shows where the delivery came from -------------------------
# **The source is entry 0 of the function the module claimed**, and the nucleus
# says so — not the harness, and not the module.
grep -q '^TOS\.RUN\.IRQ_SOURCE process=0 segment=0 bus=0 device=4 function=0 entry=0 transport=msix generation=1 asserted_by=nucleus$' \
    "$LOG" || fail "no routed source was derived from the claimed function: $(grep IRQ_SOURCE "$LOG" || true)"
[ "$(grep -c '^TOS\.RUN\.IRQ_SOURCE ' "$LOG")" = 1 ] ||
    fail "more sources were derived than the module asked for"

# The delivery itself, stated by the party that took it. `woke=1` is the fact
# the whole stage exists to produce: a context resumed because a device fired.
delivered="$(grep -c '^TOS\.RUN\.IRQ_DELIVERED ' "$LOG" || true)"
[ "$delivered" -ge 2 ] || fail "only $delivered interrupts were delivered, expected at least 2"
[ "$(grep -c '^TOS\.RUN\.IRQ_DELIVERED source=0 entry=0 vector=48 .* woke=1 ' "$LOG" || true)" -ge 1 ] ||
    fail "no delivery woke the waiting context: $(grep IRQ_DELIVERED "$LOG" || true)"

# --- the liveness rule saw a routed source, and did not cancel the wait ---------
# ADR-0082 §8: this is the first blocking reason that is not a peer's. Before
# Stage 4C-1b every census answered `routed=0`, and this wait would have been
# cancelled at the instant it was made.
grep -q "^$READY" "$LOG" ||
    fail "the scheduler did not classify the wait as routed: $(grep LIVENESS "$LOG" || true)"
[ "$(grep -c 'verdict=stalled' "$LOG" || true)" = 0 ] ||
    fail "a wait with a live routed source was judged stalled"
[ "$(grep -c '^TOS\.RUN\.BLOCK_CANCELLED ' "$LOG" || true)" = 0 ] ||
    fail "the routed wait was cancelled"

# --- both enable predicates moved, and they moved together for this one --------
# ADR-0082 §5b and §5d. An interrupt source is the one descendant class that is
# **both** memory-decoding and bus-mastering: its table is in a BAR, and its
# message is a memory write the device issues. A window moved only the first
# (proved by `virtio-mmio.sh`); this moves both, which is what makes the two
# predicates observably independent rather than merely described as such.
grep -q '^TOS\.RUN\.PCI_ENABLES .* device=4 function=0 memory_decoding=. bus_mastering=1 memory_space=1 bus_master=1 asserted_by=nucleus$' \
    "$LOG" || fail "claiming a source did not turn bus mastering on: $(grep PCI_ENABLES "$LOG" || true)"
# And the last one going takes both back, on the process-death path: the module
# dies holding its source.
grep -q '^TOS\.RUN\.PCI_ENABLES .* memory_decoding=0 bus_mastering=0 memory_space=0 bus_master=0 asserted_by=nucleus$' \
    "$LOG" || fail "the last descendant going did not clear both enables"

# --- the source was released, and its vector retired ---------------------------
grep -q '^TOS\.RUN\.IRQ_RELEASED source=0 entry=0 vector=48 .* vector_retired=1 asserted_by=nucleus$' \
    "$LOG" || fail "the source was not released with its vector retired: $(grep IRQ_RELEASED "$LOG" || true)"

# --- the negatives ADR-0082 §13 requires ---------------------------------------
# Function-side:
#   1 an entry index outside this function's table is E_BAD_ARGUMENT
#   2 a function capability without `interrupt` is refused
#   4 a second live source for one entry is E_LIMIT
#   8 a different entry of the same function is allowed
#  16 and the plain claim works, so the refusals are refusals and not a closure
negatives="$(run negatives irq-authority-negative 33 --stage4-block-device --timeout 60)"
[ -n "$negatives" ] || fail "the function-side negatives reported nothing"
[ "$negatives" = 31 ] || fail "the function-side negatives did not all hold: $negatives"
[ "$(grep -c '^TOS\.RUN\.IRQ_DELIVERED ' "$OUT/negatives/events.log" || true)" = 0 ] ||
    fail "an interrupt was delivered in a run that never armed the device"

# Source-side. §13's "a source capability without `wait` cannot wait" is
# satisfied one step earlier than it asks and the evidence says so: the object
# has exactly one right, and `CAPABILITY_V1` §4 refuses an attenuation to none —
# so no program can produce a source name that would fail the dispatcher's rights
# check, which the dispatcher still performs.
#   1 a source name that cannot wait cannot be made
#   2 attenuating to `wait` itself succeeds, so 1 is about the rights
#   4 a released source refuses: its handle no longer resolves
#   8 releasing it frees the entry, which can be claimed again
#  16 and the plain claim works
waits="$(run wait-negatives irq-wait-negative 33 --stage4-block-device --timeout 60)"
[ -n "$waits" ] || fail "the source-side negatives reported nothing"
[ "$waits" = 31 ] || fail "the source-side negatives did not all hold: $waits"

# --- a vector is retired and never handed to a second source -------------------
# ADR-0082 §5f. The source-side run claims one entry, releases it, and claims the
# **same entry of the same function** again. A recycling allocator would hand the
# same vector back; a retiring one cannot, so the two must differ — and that is
# the whole of the mechanism standing in for a proof the generation model cannot
# give.
vectors="$(sed -n 's/^TOS\.RUN\.IRQ_RELEASED .* vector=\([0-9]*\) .*$/\1/p' \
    "$OUT/wait-negatives/events.log")"
unique="$(printf '%s\n' "$vectors" | sort -nu | wc -l)"
total="$(printf '%s\n' "$vectors" | wc -l)"
[ "$total" -ge 2 ] || fail "the source-side run released $total sources, expected at least 2"
[ "$unique" = "$total" ] ||
    fail "a retired vector was handed to a second source: $(printf '%s ' $vectors)"
# And the entries really were the same one, so the vectors differing is about
# retirement rather than about two different interrupts.
[ "$(sed -n 's/^TOS\.RUN\.IRQ_RELEASED .* entry=\([0-9]*\) .*$/\1/p' \
    "$OUT/wait-negatives/events.log" | sort -u | wc -l)" = 1 ] ||
    fail "the source-side run released sources of more than one entry"

# --- a process with no PCI authority at all cannot reach any of it --------------
# The same module on the **production** nucleus, whose launcher mints no PCI
# root. Its request is unanswered, so it never reaches a call: `CapabilityDenied`
# before the first instruction, which is stronger than a refused call — there is
# no authority to refuse.
rm -rf "$OUT/denied"; mkdir -p "$OUT/denied"
printf '/system/boot/init.tos\t%s/tests/vectors/virtio-msix-wait/init.tos\n' "$ROOT" \
    > "$OUT/denied/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/denied/fixture.bin" --meta "$OUT/denied/meta.json" \
    "$OUT/denied/manifest.txt" > /dev/null
bash "$HERE/run.sh" --out "$OUT/denied" --capsule "$OUT/denied/fixture.bin" \
    --nucleus "$PRODUCTION" --stage4-block-device --expect 75 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REFUSED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.IRQ_SOURCE TOS.RUN.IRQ_DELIVERED TOS.RUN.COMPLETED" \
    > /dev/null
grep -q 'capability-denied' "$OUT/denied/events.log" ||
    fail "a module importing platform.pci.Bus started without one"

# --- and ring 0 still knows nothing about the device it routes ------------------
# ADR-0082 §9. The nucleus may know an MSI-X table entry's layout and how to
# acknowledge through the local APIC; it may not know what a queue is, that
# entry 0 is a configuration vector, or that this is a block device. Checked
# over ring-0 source with comments stripped, exactly as Stage 4B's gate does.
leaked="$(find "$ROOT/nucleus/src" -name '*.rs' -print0 |
    xargs -0 sed -e 's://.*::' -e 's:/\*.*\*/::' |
    grep -niE "virtio|virtqueue|device_status|driver_ok|feature_select|blk" || true)"
[ -z "$leaked" ] || fail "ring 0 mentions device vocabulary: $leaked"

echo "IRQ-ROUTED PASS: a textual driver waited for its own device and the device woke it"
echo "  one bus capability, one claimed function, one interrupt of that function:"
echo "  the module derived a platform.irq.Source through the assignment, told the"
echo "  device which of its own MSI-X entries to use, and blocked in irq_wait"
echo "  the scheduler announced blocked=1 routed=1 verdict=awaiting-hardware — the"
echo "  first census in this system's history that answers routed=1 — and halted"
echo "  the machine instead of cancelling the wait"
echo "  a real configuration change on the real device then raised a real MSI-X"
echo "  interrupt; the nucleus took it on vector 48, matched entry 0 of 00:04.0,"
echo "  and woke the one context waiting on that source ($delivered deliveries)"
echo "  the device's own configuration generation moved across the wait, and a"
echo "  second interrupt was delivered and waited for"
echo "  claiming the source turned memory decoding **and** bus mastering on, where"
echo "  a mapped window turns only the first on; the last descendant going took"
echo "  both back, and the vector was retired rather than returned"
echo "  nine interrupt-authority checks hold on the function and source sides:"
echo "  a fabricated entry index, a function name without \`interrupt\`, a second"
echo "  source for one entry, a source name that cannot wait and cannot be made,"
echo "  a released source that no longer resolves, and the entry claimable again"
echo "  with a **different** vector, because a retired one never comes back"
echo "  a module with no PCI root cannot start at all, and ring 0 contains no"
echo "  device vocabulary"
