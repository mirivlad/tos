#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0084 §8's teardown obligations, exercised rather than described.
#
# A driver releases a DMA region while its function is still a bus master. The
# frames must stay out of the pool, the charge must stay against the budget and
# the assignment must not end until the nucleus has **proved** that nothing the
# device issued can still land in them: bus mastering off, then a read of Device
# Status observing Transactions Pending clear. This gate makes the situations in
# which a wrong implementation would give something back early, and reads what
# it gave back.
#
# **Two boots of one canonical source**, differing in one fact about the device:
#
#   test-dma-quarantine                    the reference device, which goes quiet
#   test-dma-quarantine                    a device whose Device Status reports
#     + test-device-never-quiescent        Transactions Pending on every read
#
# | ADR-0084 §8 | what is asserted, and from where |
# |---|---|
# | 4    | the record orders the teardown: mastering stops, then the runs return — and never before |
# | 9a   | a function with no PCI Express capability, which the profile **did** qualify, is refused with `E_BAD_ARGUMENT` (`SYSTEM_ABI_V1` row 30) |
# | 9b   | Relaxed Ordering, No Snoop and both ID-Based Ordering enables are refused, an unchanged write-back is not, and a neighbour in each register changes |
# | 11   | released runs stay out of the pool while an interrupt source keeps the function mastering, and come back when it goes |
# | 12   | the churn case: two cycles spend a two-frame child budget and a third is refused **while the parent funds one at the same moment**; the pool has lost exactly the quarantined frames |
# | 13a  | against the device that never goes quiet: frames out, charge outstanding, assignment pinned, the same BDF refused, and all of it surviving the process's death |
#
# A refund-on-release nucleus passes every obligation of §8 except 12, and a
# nucleus that reclaimed on `BME = 0` alone passes every one except 13a — which
# is why both boots exist.
#
#   bash host-tools/qemu-test/dma-quarantine.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
. "$HERE/stage4-profile.sh"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-dma-quarantine}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"

fail() { echo "dma-quarantine: FAIL: $*" >&2; exit 1; }

# Every evidence build is its own target directory, and none outlives this run.
BUILT=""
cleanup() { [ -z "$BUILT" ] || rm -rf $BUILT; }
trap cleanup EXIT

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }

# The production nucleus must be untouched by every evidence build.
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
nucleus_for() {
    local features="$1" name="$2"
    local target="$ROOT/target/$name"
    BUILT="$BUILT $target"
    (cd "$ROOT" && CARGO_TARGET_DIR="$target" cargo build --release \
        -p tos-nucleus --target x86_64-unknown-none --features "$features" >/dev/null 2>&1) ||
        fail "the nucleus does not build with $features"
    [ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
        fail "the production nucleus changed while building $features"
    echo "$target/x86_64-unknown-none/release/tos-nucleus"
}

printf '/system/boot/init.tos\t%s/init.tos\n' "$ROOT/tests/vectors/dma-quarantine" \
    > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/capsule.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/capsule.bin" --manifest "$OUT/capsule.meta.json" >/dev/null

completed() {
    sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$OUT/$1/events.log"
}

boot() {
    local name="$1" nucleus="$2"
    bash "$HERE/run.sh" \
        --out "$OUT/$name" \
        --capsule "$OUT/capsule.bin" \
        --nucleus "$nucleus" \
        --stage4-block-device \
        --expect 33 \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_ASSIGNED TOS.RUN.IRQ_SOURCE TOS.RUN.DMA_REGION TOS.RUN.DMA_QUARANTINED TOS.RUN.COMPLETED TOS.HALT" \
        --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
        > /dev/null || fail "the $name boot did not complete"
}

# What the module observed, one bit per fact (`tests/vectors/dma-quarantine`).
NO_EXPRESS_REFUSED=1
ORDERING_OWNED=2
NEIGHBOURS_WRITABLE=4
CHURN_BOUNDED=8
PARENT_FUNDS=16
REFUND_AFTER_SOURCE=32
RECLAIM_ON_CLAIM=64

# --- the facts both boots share -------------------------------------------------
#
# §8.9a's premise is ring 0's, not the fixture's: the conventional function reports
# **no** Express capability and **is** qualified, so the one thing that can refuse
# its DMA authority is P1. And the target reports both, so the refusals below are
# about the teardown and not about qualification.
shared_facts() {
    local name="$1" events="$OUT/$1/events.log"
    grep -q "^TOS.RUN.PCI_DMA_QUALIFIED segment=0 bus=0 device=31 function=2 " "$events" ||
        fail "$name: the profile did not qualify the conventional function"
    grep -q "^TOS.RUN.PCI_ASSIGNED process=0 segment=0 bus=0 device=31 function=2 .* express=0 dma=1 " "$events" ||
        fail "$name: the conventional function is not the P1-only negative (express=0 dma=1)"
    grep -q "^TOS.RUN.PCI_ASSIGNED process=0 $(stage4_target_fields) .* express=1 dma=1 " "$events" ||
        fail "$name: the target is not fully qualified (express=1 dma=1)"
    # No region was made on the conventional function: the only DMA regions in
    # the boot reach the target.
    ! grep "^TOS.RUN.DMA_REGION " "$events" | grep -qv " $(stage4_target_fields) " ||
        fail "$name: a DMA region reached a function other than the target"
}

# --- the churn, read off the record ----------------------------------------------
#
# Three regions are made and released while the interrupt source lives. Each one's
# run leaves the pool and **stays out**: the pool the nucleus reports after each
# carve falls by exactly one frame per region, which it could not do if a released
# run had come back. And no run returns until bus mastering has stopped.
churn_record() {
    local name="$1" expect_reclaim="$2"
    python3 - "$OUT/$name/events.log" "$expect_reclaim" <<'CHECK' || fail "$name: the churn record is not the contract"
import re, sys

# The runtime's own per-operation lines interleave with the nucleus's; the
# order asserted here is the nucleus's, so they are set aside.
events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")
          if not line.startswith("TOS.RUN.INTERFACE ")]
expect_reclaim = sys.argv[2] == "1"

def find(pattern, start=0):
    for at in range(start, len(events)):
        if re.match(pattern, events[at]):
            return at
    return None

source = find(r"^TOS\.RUN\.IRQ_SOURCE ")
if source is None:
    sys.exit("no interrupt source was claimed")
# The bus master the source made, before any region existed.
if find(r"^TOS\.RUN\.PCI_ENABLES .* bus_mastering=1 .* bus_master=1 ") is None:
    sys.exit("the function never became a bus master")

regions = [at for at in range(source, len(events)) if events[at].startswith("TOS.RUN.DMA_REGION ")]
if len(regions) < 3:
    sys.exit(f"{len(regions)} region(s) after the source; the churn makes three")
pools = []
for at in regions[:3]:
    match = re.search(r" pool_available=(\d+) ", events[at])
    if not match:
        sys.exit("a region record carries no pool figure")
    pools.append(int(match.group(1)))
    # Released into quarantine, never straight back.
    if not re.match(r"^TOS\.RUN\.DMA_QUARANTINED region=\d+ process=0 frames=1 "
                    r"charge_outstanding=1 kept=1 asserted_by=nucleus$", events[at + 1]):
        sys.exit("a released region did not go straight into quarantine: " + events[at + 1])
if pools != [pools[0], pools[0] - 1, pools[0] - 2]:
    sys.exit(f"the pool after each carve was {pools}; three quarantined runs must each "
             "cost the pool one frame")

# Mastering stops when the source goes. Nothing came back before it.
stopped = find(r"^TOS\.RUN\.PCI_ENABLES .* bus_mastering=0 .* bus_master=0 ", regions[2])
if stopped is None:
    sys.exit("bus mastering never stopped after the churn")
if any(events[at].startswith("TOS.RUN.DMA_RECLAIMED ") for at in range(regions[0], stopped)):
    sys.exit("a run came back while the function was still mastering")

released = find(r"^TOS\.RUN\.IRQ_RELEASED source=\d+ entry=0 vector=\d+ deliveries=0 "
                r"cancelled_waiter=0 vector_retired=1 asserted_by=nucleus$", stopped)
if released is None:
    sys.exit("the interrupt source was not released with its vector retired")
back = [at for at in range(stopped, released) if events[at].startswith("TOS.RUN.DMA_RECLAIMED ")]
if expect_reclaim:
    # §8.4: stopped, then proved, then returned — three runs, one line each, and
    # every one of them after the mastering stopped.
    if len(back) != 3 or any(not re.match(r"^TOS\.RUN\.DMA_RECLAIMED process=0 frames=1 "
                                          r"drained=1 refunded=1 asserted_by=nucleus$",
                                          events[at]) for at in back):
        sys.exit(f"{len(back)} run(s) came back when the source went; three were quarantined")
else:
    if any(line.startswith("TOS.RUN.DMA_RECLAIMED ") for line in events):
        sys.exit("a run came back from a device that never went quiet")
print(pools[0])
CHECK
}

# --- boot 1: the reference device, which goes quiet ----------------------------
boot live "$(nucleus_for test-dma-quarantine test-dma-quarantine)"
shared_facts live
value="$(completed live)"
all=$((NO_EXPRESS_REFUSED + ORDERING_OWNED + NEIGHBOURS_WRITABLE + CHURN_BOUNDED \
    + PARENT_FUNDS + REFUND_AFTER_SOURCE + RECLAIM_ON_CLAIM))
[ "$value" = "$all" ] || fail "the live boot observed $value; the contract is $all"
first_pool="$(churn_record live 1)"

# The fourth region — made after the source went — is quarantined and returned in
# the same breath, because nothing keeps the function mastering any more; and it
# is carved out of the runs that came back, so the pool it leaves is the first
# region's.
python3 - "$OUT/live/events.log" "$first_pool" <<'CHECK' || fail "live: the region after the source did not return at once"
import re, sys
# The runtime's own per-operation lines interleave with the nucleus's; the
# order asserted here is the nucleus's, so they are set aside.
events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")
          if not line.startswith("TOS.RUN.INTERFACE ")]
first = int(sys.argv[2])
released = max(at for at, line in enumerate(events) if line.startswith("TOS.RUN.IRQ_RELEASED "))
after = [at for at in range(released, len(events)) if events[at].startswith("TOS.RUN.DMA_REGION ")]
if len(after) != 1:
    sys.exit(f"{len(after)} region(s) after the source went; the module makes one")
at = after[0]
if f" pool_available={first} " not in events[at]:
    sys.exit("the fourth region did not reuse a returned run: " + events[at])
steps = [
    r"^TOS\.RUN\.DMA_QUARANTINED region=\d+ process=0 frames=1 charge_outstanding=1 kept=1 ",
    r"^TOS\.RUN\.PCI_ENABLES .* bus_mastering=0 .* bus_master=0 ",
    r"^TOS\.RUN\.DMA_RECLAIMED process=0 frames=1 drained=1 refunded=1 ",
]
cursor = at + 1
for step in steps:
    if cursor >= len(events) or not re.match(step, events[cursor]):
        sys.exit("expected " + step + " but found " + (events[cursor] if cursor < len(events) else "the end"))
    cursor += 1
CHECK

# The assignment ended when its last name went, so the same BDF was claimed again
# — at a generation the first claim's capabilities cannot reach.
python3 - "$OUT/live/events.log" <<'CHECK' || fail "live: the target was not claimable again at a new generation"
import re, sys
target = [int(m.group(1)) for line in open(sys.argv[1], encoding="utf-8", errors="replace")
          for m in [re.match(r"^TOS\.RUN\.PCI_ASSIGNED process=0 segment=0 bus=1 device=0 function=0 generation=(\d+) ", line)] if m]
if len(target) != 2 or target[1] <= target[0]:
    sys.exit(f"target claims at generations {target}")
CHECK

# Everything came home: the pool the process's reclamation leaves is the pool
# before the first region, plus the process's own frames.
python3 - "$OUT/live/events.log" "$first_pool" <<'CHECK' || fail "live: the pool did not come back whole"
import re, sys
first = int(sys.argv[2])
line = [l for l in open(sys.argv[1], encoding="utf-8", errors="replace") if l.startswith("TOS.RUN.PROCESS_RECLAIMED ")][-1]
frames = int(re.search(r" frames=(\d+) ", line).group(1))
available = int(re.search(r" available=(\d+) ", line).group(1))
quarantined = int(re.search(r" dma_quarantined=(\d+)", line).group(1))
if quarantined != 0 or available != first + 1 + frames:
    sys.exit(f"available={available} quarantined={quarantined}; expected {first + 1 + frames} and 0")
CHECK
echo "dma-quarantine: live boot observed $value — P1 refused on a qualified conventional" \
     "function, four ordering bits owned with their neighbours free, the churn bounded" \
     "by the child budget while the parent funds, three runs held until mastering" \
     "stopped and returned after it, the refund on return, and the BDF claimable again"

# --- boot 2: a device that never goes quiet (§8.13a) ----------------------------
boot never-quiet "$(nucleus_for test-dma-quarantine,test-device-never-quiescent test-dma-never-quiescent)"
shared_facts never-quiet
value="$(completed never-quiet)"
held=$((NO_EXPRESS_REFUSED + ORDERING_OWNED + NEIGHBOURS_WRITABLE + CHURN_BOUNDED + PARENT_FUNDS))
[ "$value" = "$held" ] ||
    fail "the never-quiet boot observed $value; the contract is $held — no refund, no re-claim"
first_pool="$(churn_record never-quiet 0)"
python3 - "$OUT/never-quiet/events.log" "$first_pool" <<'CHECK' || fail "never-quiet: the quarantine did not hold through the process's death"
import re, sys
events = [l.rstrip("\r\n") for l in open(sys.argv[1], encoding="utf-8", errors="replace")]
first = int(sys.argv[2])
# The assignment was pinned, so the same BDF was never assigned a second time.
target = [l for l in events if re.match(r"^TOS\.RUN\.PCI_ASSIGNED process=0 segment=0 bus=1 device=0 function=0 ", l)]
if len(target) != 1:
    sys.exit(f"the target was assigned {len(target)} time(s); a pinned assignment admits one")
# And after the process died, the three runs are still out of the pool and named.
line = [l for l in events if l.startswith("TOS.RUN.PROCESS_RECLAIMED ")][-1]
frames = int(re.search(r" frames=(\d+) ", line).group(1))
available = int(re.search(r" available=(\d+) ", line).group(1))
quarantined = int(re.search(r" dma_quarantined=(\d+)", line).group(1))
if quarantined != 3:
    sys.exit(f"dma_quarantined={quarantined} after the process died; three runs were never proved")
if available != first + 1 + frames - 3:
    sys.exit(f"available={available}; the pool must have lost exactly the three quarantined "
             f"frames ({first + 1 + frames - 3})")
CHECK
echo "dma-quarantine: never-quiet boot observed $value — bus mastering stopped and no run" \
     "came back, the child budget stayed spent, the same BDF was refused, and after the" \
     "process died the pool had lost exactly the three quarantined frames"

echo "dma-quarantine: PASS (ADR-0084 §8.4, 9a, 9b, 11, 12, 13a)"
