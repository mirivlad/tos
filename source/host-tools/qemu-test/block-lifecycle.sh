#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A block service dies, a successor restarts the device-facing path, and the data
# the first one wrote is read back through the second.
#
# **The Stage 4 Branch-A persistence criterion, and ADR-0093's case C.** Four
# canonical textual modules, five processes over four slots, and one sector.
#
#   registry, service A and the client start
#   client -> lookup -> endpoint A
#   client -> a region and a call -> A -> VIRTIO_BLK_T_OUT -> device
#   A ends, still holding the function, window, source and DMA region
#   supervisor: wait_child(A) -> **withdraws publication A** -> then starts B
#   B: writes 0 to DEVICE_STATUS, initializes VirtIO again, rebuilds queue,
#      DMA region, window and interrupt source, and republishes
#   client -> a fresh lookup -> endpoint B, which is not endpoint A
#   client -> a region and a call -> B -> VIRTIO_BLK_T_IN -> device
#   client: **all 512 bytes are the pattern A wrote**
#   client -> a call on endpoint A, while B waits -> cancelled, not served
#
# **What persistence means here, exactly.** Data written through a service that no
# longer exists is observable through one that initialized the device itself. That
# is all: ADR-0092 puts power-loss durability, `VIRTIO_BLK_F_FLUSH`, fsync
# semantics, ungraceful-host termination and crash-consistent filesystem semantics
# outside Stage 4, and none of them is claimed.
#
# **Sector 8 and a pattern of the boot's own.** Sectors 1–4 of the reference image
# are seeded and sector 0 is zero-filled, so a witness in any of them could be
# satisfied by the image; sector 8 is untouched zeros and the pattern is non-zero
# everywhere, so neither the image nor unwritten memory can pass for it.
#
# **Case D is not touched.** If a write had been accepted and the service had died
# before answering, Stage 4 guarantees nothing about whether it happened
# (ADR-0093 §5a). This boot's write is acknowledged before A ends, so it asks no
# such question — and nothing here is an exactly-once test.
#
#   bash host-tools/qemu-test/block-lifecycle.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-block-lifecycle}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/block-lifecycle"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-block-lifecycle"

# The supervisor: three children created, A's ending collected, publication A
# withdrawn, and the successor started.
EXPECTED_SUPERVISOR="i64:63"
# The registry: registration, lookup, **withdrawal**, registration, lookup.
EXPECTED_REGISTRY="i64:31"
# The client: two lookups, two exchanges, a region, all 512 bytes, and a stale
# call that was cancelled rather than served.
EXPECTED_CLIENT="i64:127"
SECTOR_BYTES=512
# Stage 4D-2's twenty device-side facts. The **writer** proves nineteen: a write
# returns no data to inspect, so `PROVED_SECTOR_READ` is not among them, and that
# difference is itself read below.
PROVED_ALL=1048575
PROVED_SECTOR_READ=524288
PROVED_WRITER=$((PROVED_ALL - PROVED_SECTOR_READ))
LEN_SHIFT=1048576
# 2^57, the reader's control: its second receive was cancelled rather than
# satisfied, so the client's call through the dead instance's endpoint never
# arrived.
STALE_BIT=144115188075855872

fail() {
    echo "block-lifecycle: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-block-lifecycle)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/registry/nameservice.tos\t%s/nameservice.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/service/block.tos\t%s/service.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/client/block.tos\t%s/client.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/path.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/path.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/path.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- four modules, five processes over four slots -------------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=4$' "$LOG" ||
    fail "the boot did not run a set of four modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 5 ] ||
    fail "five processes did not begin: $(grep -c 'TOS.RUN.BEGIN path=' "$LOG") did"
[ "$(count '^TOS\.RUN\.BEGIN path=system/service/block\.tos')" = 2 ] ||
    fail "the service module did not run twice"

# --- the client holds no hardware authority, in either generation ---------------
for never in platform.pci.FunctionConfig platform.irq.Source platform.dma.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested by a module, and none may be"
done
# The PCI root: requested by the supervisor once and by each service instance
# once. A fourth requester would be the boundary crossed.
[ "$(count '^TOS\.RUN\.REQUEST binding=.* interface=platform\.pci\.Bus ')" = 3 ] ||
    fail "platform.pci.Bus was not requested by exactly the supervisor and the two instances"
[ "$(count '^TOS\.RUN\.REQUEST binding=budget interface=platform\.pci\.Bus ')" = 0 ] ||
    fail "the client's budget binding resolved to a PCI bus"

# --- 1/2: the client obtained endpoint A, and A served a write then ended -------
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$')" = 3 ] ||
    fail "the three region sends did not all succeed: two from the client, one from the reader"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_region status=0$')" = 3 ] ||
    fail "the three region receives did not all succeed"

# --- 3: publication A was withdrawn, and before the successor existed -----------
# The registry's own release of the capability it held for A. **This is what makes
# the entry stop existing**: not a flag, not a later entry shadowing it — the name
# is gone, and the status is in the audit record.
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')" -ge 1 ] ||
    fail "no capability was released, so the registry never let go of publication A"
# And the ordering: the registry replied to the withdrawal before the second
# instance began. A single `python3` pass, because ordering is the evidence.
python3 - "$LOG" <<'ORDER' || fail "the withdrawal did not complete before the successor started"
import re
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
steps = [
    ("the first service instance began",
     re.compile(r"^TOS\.RUN\.BEGIN path=system/service/block\.tos")),
    ("it ended, and its process was reclaimed",
     re.compile(r"^TOS\.RUN\.PROCESS_RECLAIMED ")),
    ("the registry released the name it held for it",
     re.compile(r"^TOS\.RUN\.INTERFACE operation=capability_release status=0$")),
    ("and only then did the successor begin",
     re.compile(r"^TOS\.RUN\.BEGIN path=system/service/block\.tos")),
]
at = 0
for label, pattern in steps:
    while at < len(events) and not pattern.match(events[at]):
        at += 1
    if at == len(events):
        print("  missing, or not after the step before it: " + label)
        sys.exit(1)
    at += 1
sys.exit(0)
ORDER

# --- 6/7: two claims of the same function, at different generations -------------
# ADR-0081 §14: the assignment ends when the last handle and the last descendant
# are gone, and its generation advances. A successor that inherited the first
# instance's assignment would show the same number.
generations="$(sed -n 's/^TOS\.RUN\.PCI_ASSIGNED .* generation=\([0-9]*\) .*$/\1/p' "$LOG")"
[ "$(printf '%s\n' "$generations" | grep -c .)" = 2 ] ||
    fail "the function was not claimed exactly twice: generations [$generations]"
[ "$(printf '%s\n' "$generations" | sort -u | grep -c .)" = 2 ] ||
    fail "both claims report the same assignment generation [$generations], so the successor inherited one"

# --- the T1 reset and reinitialization, for each instance separately ------------
# ADR-0092 R1a: the successor writes 0 to VirtIO `DEVICE_STATUS` and performs the
# normal initialization again. `PROVED_RESET` is set only by a module that wrote 0
# and then **read `device_status` back as 0** (§4.1.4.3.2) before touching anything
# else, so it is a device answer and not an inference from a later successful read.
# The bit is checked per instance below, where the composites are read.
#
# And the rebuilding, which is observable in ring 0 rather than claimed in text:
# each instance maps its own window, claims its own interrupt source and allocates
# its own DMA region, and the device raises an interrupt for each.
[ "$(count '^TOS\.RUN\.DMA_REGION ')" = 2 ] ||
    fail "a DMA region was not built twice, so the successor did not rebuild one"
[ "$(count '^TOS\.RUN\.MMIO_MAPPED ')" = 2 ] ||
    fail "a device window was not mapped twice"
[ "$(count '^TOS\.RUN\.IRQ_DELIVERED ')" -ge 2 ] ||
    fail "the device did not raise an interrupt for each instance"

# --- 7: A's teardown, while it still owned everything --------------------------
# The process-death path, not an explicit release: nothing in the module gives any
# of it back. Live DMA backing must enter quarantine before the pool, the
# assignment must be drained, and only then may the run be returned.
python3 - "$LOG" <<'TEARDOWN' || fail "the first instance's teardown is missing or out of order"
import re
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
steps = [
    ("live DMA backing enters quarantine rather than the pool",
     re.compile(r"^TOS\.RUN\.DMA_QUARANTINED .* kept=1 asserted_by=nucleus$")),
    ("the assignment is drained: memory decoding and bus mastering off",
     re.compile(r"^TOS\.RUN\.PCI_ENABLES .* memory_decoding=0 bus_mastering=0 "
                r"memory_space=0 bus_master=0 asserted_by=nucleus$")),
    ("the quarantined run is proved safe, returned and its charge refunded",
     re.compile(r"^TOS\.RUN\.DMA_RECLAIMED .* drained=1 refunded=1 asserted_by=nucleus$")),
    ("the interrupt source is released and its vector retired",
     re.compile(r"^TOS\.RUN\.IRQ_RELEASED .* vector_retired=1 asserted_by=nucleus$")),
    ("the process is reclaimed with no quarantined DMA frames left",
     re.compile(r"^TOS\.RUN\.PROCESS_RECLAIMED .* dma_quarantined=0$")),
]
at = 0
for label, pattern in steps:
    while at < len(events) and not pattern.match(events[at]):
        at += 1
    if at == len(events):
        print("  missing, or not after the step before it: " + label)
        sys.exit(1)
    at += 1
sys.exit(0)
TEARDOWN

# --- the four accounts ---------------------------------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_SUPERVISOR\$")" = 1 ] ||
    fail "the supervisor did not report the whole sequence: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY\$")" = 1 ] ||
    fail "the registry did not report a registration, a lookup, a withdrawal and both again: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLIENT\$")" = 1 ] ||
    fail "the client did not report every step including the cancelled stale call: $(grep COMPLETED "$LOG")"

# The two instances' composites, which differ in exactly the two places the
# direction shows: the data-content bit and the used length.
writer="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\([0-9]\{7,\}\)$/\1/p' "$LOG" | head -1)"
reader="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\([0-9]\{7,\}\)$/\1/p' "$LOG" | tail -1)"
[ -n "$writer" ] && [ -n "$reader" ] || fail "one of the two instances reported no composite"
[ "$writer" != "$reader" ] ||
    fail "both instances reported the same composite, so they did the same thing"
[ "$((writer % LEN_SHIFT))" = "$PROVED_WRITER" ] ||
    fail "the writer proved $((writer % LEN_SHIFT)) of $PROVED_WRITER device-side facts"
[ "$((reader % LEN_SHIFT))" = "$PROVED_ALL" ] ||
    fail "the reader proved $((reader % LEN_SHIFT)) of $PROVED_ALL device-side facts"
# **The T1 reset, per instance.** Named separately from the composite equality
# above, because it is the one device-side fact ADR-0092 R1a names and a reader of
# this gate should not have to decompose a sum to find it.
PROVED_RESET=1
for pair in "the writer:$writer" "the successor:$reader"; do
    [ "$(( ${pair##*:} & PROVED_RESET ))" = "$PROVED_RESET" ] ||
        fail "${pair%%:*} did not drive DEVICE_STATUS to 0 and read it back as 0"
done
# **4: the stale name did not reach the successor**, said by the successor itself:
# it was waiting for a request and its wait was cancelled rather than satisfied.
[ "$((reader / STALE_BIT % 2))" = 1 ] ||
    fail "the successor did not report that its second wait was cancelled, so the stale call may have reached it"
[ "$((writer / STALE_BIT % 2))" = 0 ] ||
    fail "the writer reported a cancelled second wait, which is not its role in this boot"

echo "BLOCK-LIFECYCLE PASS: a successor restarted the device path and the data survived"
echo "  four canonical textual modules, five processes over four slots, one sector"
echo "  the client wrote a 512-byte pattern to sector 8 through instance A and A"
echo "  then ended, still holding the function, window, source and DMA region"
echo "  the supervisor collected A's ending, **withdrew publication A** — the"
echo "  registry released the only name it held for A, and replied before any"
echo "  successor existed — and only then created instance B"
echo "  B claimed the same function at a **different assignment generation**,"
echo "  drove DEVICE_STATUS to 0 and read it back as 0, initialized VirtIO again"
echo "  in the ordinary §3.1.1 order, and rebuilt its queue,"
echo "  DMA region, window and interrupt source, then republished"
echo "  a fresh lookup gave the client a working endpoint, and all $SECTOR_BYTES bytes it"
echo "  read back are the pattern written before A died — counted in canonical text"
echo "  and the name the first lookup gave was **not repaired**: a call on it was"
echo "  cancelled while B was waiting for a request, and B reports that its wait"
echo "  was cancelled rather than satisfied"
echo "  A's teardown is the process-death path, in order: quarantine, assignment"
echo "  drain, proved-safe reclaim, vector retired, no quarantined frames left"
echo "  NOT claimed: power-loss durability, VIRTIO_BLK_F_FLUSH, fsync semantics,"
echo "  durability across host termination, crash-consistent filesystem semantics"
echo "  (ADR-0092 puts all of those outside Stage 4), exactly-once delivery, or"
echo "  ADR-0093 case D — a write accepted by a service that then died guarantees"
echo "  nothing about whether it happened, and this boot asks no such question"
echo "  NOT claimed either: a crash in flight, third-party reset T3, more than one"
echo "  sector, more than one client, or Stage 4 closure"
