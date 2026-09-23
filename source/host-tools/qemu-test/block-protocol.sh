#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The accepted `block.device.v1` protocol, spoken by canonical text on both sides.
#
# **ADR-0098 and `BLOCK_DEVICE_V1`.** Until now a client and a block service agreed
# on a request shape the documentation called a fixture's: eight payload bytes
# carrying `sector * 2 + direction`, with the payload region sent in a message of
# its own beforehand. This boot replaces both halves of that with the contract.
#
#   word = sector * 4 + opcode        0 READ   1 WRITE   2 CAPACITY   3 reserved
#
#   CAPACITY                     -> the device's own sector count, clamped
#   a call whose length is not 8 -> refused as malformed, device untouched
#   opcode 3                     -> refused, device untouched
#   WRITE with no region         -> refused, device untouched
#   READ with no answer channel  -> refused, device untouched
#   WRITE past the capacity      -> refused as out of range, device untouched
#   a decoy region, sent alone   -> dropped: a send carries no reply
#   WRITE sector 9, atomically   -> one call carrying the word *and* the region
#   READ sector 9                -> reply, then exactly one region, 512 bytes
#
# **Two claims this gate is built around.**
#
# *The write uses the region its own call carried.* The client sends a **decoy**
# region — different bytes, no call behind it — immediately before the atomic write.
# Any service whose payload came from a region that arrived *earlier* rather than
# from this call's own writes the decoy, and the 512-byte read-back fails. That
# distinguishes "the region attached to this call" from "some region that happened to
# be around", which the two-message shape could not. **Verified by mutation**: making
# the service take its payload from the previously dropped region turns this red with
# the client reporting `i64:-14`, its byte-count failure.
#
# *The five pre-device refusals do not touch the device.* §7 requires it, and the
# journal is where it is read: each of the service's receives opens the window in
# which one exchange is served, and only the two windows allowed to reach the device
# contain a `dma_device_address` — the nucleus's own line, three per request, emitted
# only from the submission path. A service that performed a harmless real request and
# then replied with the right refusal code would keep every account bit it reports and
# the later valid requests would still succeed, so the account could not have caught it.
#
# *A read replies before it sends.* `BLOCK_DEVICE_V1` §6a fixes control before data,
# because the reverse leaves an orphan region queued on an endpoint that outlives
# the caller waiting for it. The journal is read for that order.
#
# **And capacity is the device's.** The same two modules run a second time against a
# deliberately small device, and the number the client reports moves with it — which
# no value compiled into the service could do.
#
# **Not claimed:** power-loss durability, `VIRTIO_BLK_F_FLUSH`, crash consistency,
# exactly-once write, ADR-0093 case D, more than one sector per request, batching,
# more than one client, `state.store.v1` (ADR-0099 is accepted and unimplemented),
# or Stage 4 closure.
#
#   bash host-tools/qemu-test/block-protocol.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-block-protocol}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/block-protocol"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-block-protocol"

# The supervisor: two children created, two endings collected.
EXPECTED_SUPERVISOR="i64:15"

# The service. Twenty device-side facts at bits 0..19 — Stage 4D's, all of them,
# because a write and a read between them prove every one — and eleven protocol
# facts above: the device configuration found, its capacity stable under §2.5.1's
# generation protocol, capacity served, a write served, a read served, five
# refusals each with its own bit, and one non-call message dropped.
PROVED_DEVICE_ALL=1048575
PROVED_PROTOCOL_ALL=$(( (1 << 31) - (1 << 20) ))
EXPECTED_SERVICE="i64:$((PROVED_DEVICE_ALL + PROVED_PROTOCOL_ALL))"
# **And one bit that must be absent.** `BLK_DEVICE` is implemented — a device that
# reports a failure is answered with that code rather than tearing the service down —
# but the QEMU reference endpoint answers every well-formed in-range request with
# `VIRTIO_BLK_S_OK`, and no fake device is built to manufacture a failure. So bit 31
# is not in the total above, and a boot that set it would mean the reference device
# had started failing requests rather than that this gate had got stronger.
PROVED_REFUSED_DEVICE=$(( 1 << 31 ))

# The client: eleven facts, and the capacity it was told riding above them.
CLIENT_FACTS=2047
CAPACITY_SHIFT=4096
# The reference image is 16 MiB of 512-byte sectors.
REFERENCE_SECTORS=$((16 * 1024 * 1024 / 512))
EXPECTED_CLIENT="i64:$((CLIENT_FACTS + REFERENCE_SECTORS * CAPACITY_SHIFT))"

# And the small device the second boot runs against, which must be large enough to
# hold sector 9 and small enough to be a different number.
SMALL_SECTORS=64
EXPECTED_SMALL_CLIENT="i64:$((CLIENT_FACTS + SMALL_SECTORS * CAPACITY_SHIFT))"

fail() {
    echo "block-protocol: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-block-protocol)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/service/block.tos\t%s/service.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/client/block.tos\t%s/client.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/path.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/path.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/path.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null

LOG="$OUT/live/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- three modules, three processes ---------------------------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=3$' "$LOG" ||
    fail "the boot did not run a set of three modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 3 ] ||
    fail "three processes did not begin: $(count '^TOS\.RUN\.BEGIN path=') did"

# --- no client holds any part of the machine ------------------------------------
for never in platform.pci.FunctionConfig platform.irq.Source platform.dma.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested by a module, and none may be"
done
# The PCI root: the supervisor once and the service once. A third requester would
# be the boundary crossed.
[ "$(count '^TOS\.RUN\.REQUEST binding=.* interface=platform\.pci\.Bus ')" = 2 ] ||
    fail "platform.pci.Bus was not requested by exactly the supervisor and the service"
[ "$(count '^TOS\.RUN\.REQUEST binding=service interface=platform\.pci\.Bus ')" = 0 ] ||
    fail "the client's service binding resolved to a PCI bus"

# --- the accepted call-with-region row was actually used ------------------------
# Two of them: the out-of-range write and the atomic write, and nothing else. The
# decoy is a plain `endpoint_send_region` and the read carries an endpoint instead.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_region status=0$')" = 2 ] ||
    fail "the atomic call-with-region row was not used exactly twice"
# And the receive that serves it: nine messages, one per exchange.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call_region status=')" = 9 ] ||
    fail "the service did not receive exactly nine messages"
# The decoy: exactly one region sent by the client with no call behind it, plus the
# one the service sends back for the read.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$')" = 2 ] ||
    fail "expected exactly two successful region sends, the decoy and the read's answer"
# The dropped non-call: the service's reply to it is refused, because a send
# carries no reply capability and a handle of all zeros names nothing.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=-[0-9]*$')" = 1 ] ||
    fail "expected exactly one refused reply, the one to the message that was not a call"

# --- 1: exactly the requests allowed to reach the device reached it -------------
#
# **`BLOCK_DEVICE_V1` §7 requires the five pre-device refusals to leave the device
# untouched**, and until now this gate took that from the shape of the source and
# from the service's own account. Neither is evidence: a service that performed a
# harmless real request and *then* replied with the right refusal code would keep
# every bit it reports, and the later valid write and read would still succeed.
#
# **So the journal is sliced by the service's own receives.** Each
# `endpoint_receive_call_region` opens the window in which that one exchange is
# served, and the window closes at the next receive. Nine windows, in the order the
# client makes its requests:
#
#   0 CAPACITY   1 malformed   2 opcode   3 no region   4 no answer
#   5 out of range   6 the decoy   7 the atomic WRITE   8 the READ
#
# Windows 7 and 8 are the only two allowed to contain device work; the other seven
# must contain none.
#
# **What counts as device work is `dma_device_address`, and the choice matters.** It
# is emitted by the **nucleus**, once per address the driver resolves against its
# own DMA region — three per request, for the header, the data buffer and the status
# byte — and it is reached only from `perform`, the function that submits. So it
# binds to the submission path rather than to the request decoder, and it is not the
# service's self-report: the service cannot write a journal line, and neither the
# client nor the supervisor declares the operation at all (the capability check above
# is why no other process could).
#
# **Interrupts are deliberately not counted for this.** A driver must tolerate a
# spurious notification (VIRTIO §2.7.7.1) and this one does — it waits past a wake
# that did not complete its request — so "one request, one `irq_wait`" is not a
# contract and asserting it would invent one. What *is* asserted about `irq_wait` is
# only that the two windows which reached the device also completed there.
python3 - "$LOG" <<'UNTOUCHED' || fail "a refusal that must precede device work touched the device"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
RECEIVE = "TOS.RUN.INTERFACE operation=endpoint_receive_call_region status="
ADDRESS = "TOS.RUN.INTERFACE operation=dma_device_address status=0"
WAIT = "TOS.RUN.INTERFACE operation=irq_wait status=0"
ADDRESSES_PER_REQUEST = 3

opened = [i for i, line in enumerate(events) if line.startswith(RECEIVE)]
names = [
    "CAPACITY", "a malformed length", "a reserved opcode", "a write with no region",
    "a read with no answer endpoint", "an out-of-range write", "the decoy region",
    "the atomic write", "the read",
]
if len(opened) != len(names):
    print(f"expected {len(names)} served exchanges, saw {len(opened)}", file=sys.stderr)
    raise SystemExit(1)

bounds = opened + [len(events)]
touched = []
for index, name in enumerate(names):
    window = events[bounds[index] + 1:bounds[index + 1]]
    addresses = window.count(ADDRESS)
    waits = window.count(WAIT)
    allowed = index in (7, 8)
    if allowed:
        if addresses != ADDRESSES_PER_REQUEST:
            print(f"{name} resolved {addresses} device addresses, not "
                  f"{ADDRESSES_PER_REQUEST}", file=sys.stderr)
            raise SystemExit(1)
        if waits < 1:
            print(f"{name} reached the device and never completed there",
                  file=sys.stderr)
            raise SystemExit(1)
        touched.append(name)
    else:
        if addresses != 0 or waits != 0:
            print(f"{name} must be refused before the device is touched, and it "
                  f"resolved {addresses} device address(es) and waited {waits} "
                  f"time(s)", file=sys.stderr)
            raise SystemExit(1)
if len(touched) != 2:
    print(f"expected exactly two requests to reach the device, saw {touched}",
          file=sys.stderr)
    raise SystemExit(1)
UNTOUCHED
# And the same fact stated once over the whole journal, so a window boundary that
# moved could not hide a request. **Nine, and the three are as load-bearing as the
# six:** building the ring resolves one address each for the descriptor table, the
# available ring and the used ring — before any request and before the first receive,
# which is why the windowed pass above does not see them — and each request resolves
# three more, for its header, its data buffer and its status byte.
RING_ADDRESSES=3
REQUEST_ADDRESSES=$((3 * 2))
resolved="$(count '^TOS\.RUN\.INTERFACE operation=dma_device_address status=0$')"
[ "$resolved" = "$((RING_ADDRESSES + REQUEST_ADDRESSES))" ] ||
    fail "the boot resolved $resolved device addresses; one ring and two requests
       resolve $((RING_ADDRESSES + REQUEST_ADDRESSES))"

# --- 2: a read replies before it sends -----------------------------------------
# **The tail after the last completion wait is the read's.** The write's own
# `irq_wait` comes earlier, and every client-side allocation is earlier still
# because the client makes its read last. So what follows the final
# `irq_wait` is the service building its answer, replying, and *then* sending —
# and a service that sent first would put `endpoint_send_region` before
# `endpoint_reply_word` here.
python3 - "$LOG" <<'ORDER' || fail "a read's reply did not precede its region send"
import re
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
waits = [i for i, line in enumerate(events)
         if line == "TOS.RUN.INTERFACE operation=irq_wait status=0"]
if not waits:
    print("no completed interrupt wait in the journal", file=sys.stderr)
    raise SystemExit(1)
tail = events[waits[-1] + 1:]
wanted = [
    "TOS.RUN.INTERFACE operation=region_allocate status=0",
    "TOS.RUN.INTERFACE operation=region_freeze status=0",
    "TOS.RUN.INTERFACE operation=endpoint_reply_word status=0",
    "TOS.RUN.INTERFACE operation=endpoint_send_region status=0",
]
at = 0
for step in wanted:
    while at < len(tail) and tail[at] != step:
        # A region send before the reply is the ordering this gate exists to refuse.
        if step == "TOS.RUN.INTERFACE operation=endpoint_reply_word status=0" and \
                tail[at] == "TOS.RUN.INTERFACE operation=endpoint_send_region status=0":
            print("the region was sent before the reply", file=sys.stderr)
            raise SystemExit(1)
        at += 1
    if at == len(tail):
        print(f"missing after the last completion wait: {step}", file=sys.stderr)
        raise SystemExit(1)
    at += 1
ORDER

# --- 3: the accounts ------------------------------------------------------------
completed=$(grep '^TOS\.RUN\.COMPLETED value=' "$LOG" | sed 's/^TOS\.RUN\.COMPLETED value=//')
for want in "$EXPECTED_SUPERVISOR" "$EXPECTED_SERVICE" "$EXPECTED_CLIENT"; do
    [ "$(printf '%s\n' "$completed" | grep -c "^$want$")" = 1 ] ||
        fail "no module reported $want; the boot reported: $(printf '%s ' $completed)"
done

# And the bit that must not be set, checked rather than assumed: a service account
# at or above it would mean the device refused something.
for value in $completed; do
    number="${value#i64:}"
    case "$number" in
        -*) fail "a module reported the failure $value" ;;
    esac
    [ "$number" -lt "$PROVED_REFUSED_DEVICE" ] ||
        fail "a module reported $value, at or above the device-failure bit — the
       reference device is not supposed to refuse a well-formed request"
done

# --- 4: capacity is the device's, not the service's -----------------------------
# The same two modules against a deliberately small device. If the answer were
# compiled in, this number could not move.
bash "$HERE/run.sh" \
    --out "$OUT/small" \
    --capsule "$OUT/path.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --stage4-block-sectors "$SMALL_SECTORS" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null
small=$(grep '^TOS\.RUN\.COMPLETED value=' "$OUT/small/events.log" |
    sed 's/^TOS\.RUN\.COMPLETED value=//')
[ "$(printf '%s\n' "$small" | grep -c "^$EXPECTED_SMALL_CLIENT$")" = 1 ] ||
    fail "against a $SMALL_SECTORS-sector device the client did not report
       $EXPECTED_SMALL_CLIENT; it reported: $(printf '%s ' $small)"

echo "block-protocol: PASS: the accepted block.device.v1 protocol, both sides canonical text"
echo "  word = sector * 4 + opcode, with all three operations of ADR-0093 §0's surface"
echo "  CAPACITY answers the device's own sector count under §2.5.1's generation"
echo "  protocol — $REFERENCE_SECTORS on the reference image and $SMALL_SECTORS on a"
echo "  deliberately small one, from the same compiled module"
echo "  one atomic call carries the request word and the payload region together,"
echo "  and a decoy region sent just before it is dropped rather than written:"
echo "  a service taking its payload from a preceding receive would fail the"
echo "  512-byte read-back, so the region proved is the one that call carried"
echo "  a read replies **before** it sends, asserted on the journal after the"
echo "  device's own completion wait — the reverse order would leave an orphan"
echo "  region queued on an endpoint that outlives its caller"
echo "  five refusals, each with its own code and its own bit: reserved opcode,"
echo "  out of range, malformed length, a write with no region, a read with"
echo "  nowhere to answer — and every one of them a reply, not a dropped call"
echo "  and each of those five left the device **untouched**: the journal is"
echo "  sliced by the service's own receives, and only the two windows that"
echo "  are allowed to reach the device resolved a device address in them —"
echo "  three each, and nine in the boot with the ring's own three, none"
echo "  anywhere else"
echo "  BLK_DEVICE is implemented and **not** exercised: the reference endpoint"
echo "  answers every well-formed in-range request with VIRTIO_BLK_S_OK and no"
echo "  fake device is built to manufacture a failure, so that bit is required"
echo "  to be absent rather than pretended — the class virtio-queue.sh records"
echo "  for the MSI-X negative it attempted and withdrew"
echo "  NOT claimed: power-loss durability, VIRTIO_BLK_F_FLUSH, crash consistency,"
echo "  exactly-once write, ADR-0093 case D, more than one sector per request,"
echo "  batching, more than one client, state.store.v1, or Stage 4 closure"
