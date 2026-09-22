#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A client with no part of the machine reaches a block service over IPC, and the
# answer it gets back is one the device produced.
#
#   separate textual client
#     -> IPC
#     -> textual block service / driver
#     -> DMA
#     -> VirtIO
#     -> IRQ
#     -> reply
#
# **What each half contributes.** The right-hand half is Stage 4D-2's, unchanged
# in what it proves: one real `VIRTIO_BLK_T_IN` through one real split
# virtqueue, with a sentinel rather than a success code as the evidence — both
# `VIRTIO_BLK_T_IN` and `VIRTIO_BLK_S_OK` are zero and the reference image is
# zero-filled, so "it returned success and the data is zero" is what untouched
# memory looks like. The left-hand half is new: the request comes from a
# separate process that holds no part of the machine and had no name for the
# service until the ADR-0093 P3 registry gave it one.
#
# **What crosses IPC is one number, and this gate does not say otherwise.** The
# request's payload is the sector; the answer's is how many of that sector's 512
# bytes are zero. The 512 bytes themselves never cross: the client receives a
# count the service computed after a real device read, not block data. The path
# `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1 describes — client memory
# through IPC to the service's DMA memory — is **not** what passes here.
#
# **Both numbers travel as payload, and 512 is why.** `IPC_V1` §3 bounds an
# inline message at 256 bytes, so 512 cannot be the length of any message this
# contract carries. An earlier form of this slice put the sector in the
# request's length register and the count in the answer's, which made a message
# whose declared size was not its size and forced the count to be capped at 256;
# the assertions below read the payload and the lengths separately so that the
# earlier form cannot come back green.
#
# **The capability boundary is the point and is asserted, not assumed.** The
# launcher holds the PCI root and hands it to exactly one child. The client's
# plan names no PCI bus, no function, no window, no interrupt source, no DMA
# region and no memory authority, and the log is read for every one of them.
#
#   bash host-tools/qemu-test/block-service.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-block-service}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/block-service"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-block-service"

# The launcher: four receiving names let go, then three processes created.
EXPECTED_INIT="i64:127"
# The registry: a registration and a lookup answered with it.
EXPECTED_REGISTRY="i64:3"
# The client: a lookup answered, a capability delivered, the service reached,
# an answer of the shape the protocol requires, and 512 in its payload.
EXPECTED_CLIENT="i64:31"
# Stage 4D-2's twenty facts, which the service still proves on the device side.
PROVED_ALL=1048575
LEN_SHIFT=1048576

fail() {
    echo "block-service: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-block-service)
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
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.PCI_DMA_QUALIFIED TOS.RUN.PCI_ASSIGNED TOS.RUN.DMA_REGION TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- four modules, four processes ----------------------------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=4$' "$LOG" ||
    fail "the boot did not run a set of four modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 4 ] ||
    fail "four processes did not begin"

# --- the client holds no part of the machine -----------------------------------
# Each of these is requested exactly once in this boot, by the block service,
# and the launcher's own PCI root is an endowment rather than a request. A
# second requester of any of them would be the boundary this slice exists to
# draw being crossed.
# Twice each and no more: the launcher imports them to hand them on, and the
# block service imports them to use them. A third requester would be a module
# this launcher decided to give the machine to, and there is no third.
for twice in platform.pci.Bus system.memory.Authority; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$twice ")" = 2 ] ||
        fail "$twice was requested by other than the launcher and the one service entitled to it"
done
# And the service is the one that holds them: its bindings, and nobody else's.
[ "$(count '^TOS\.RUN\.REQUEST binding=budget interface=system\.memory\.Authority ')" = 1 ] ||
    fail "the block service's own authority binding is not there exactly once"
for never in platform.pci.FunctionConfig platform.irq.Source platform.dma.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested by a module, and none may be"
done
[ "$(count '^TOS\.RUN\.REQUEST binding=inbox interface=system\.ipc\.Endpoint ')" = 1 ] ||
    fail "the client's inbox was not requested by name and kind"

# --- the path, operation by operation ------------------------------------------
# The service registers; the client looks up; the registry delivers.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=0$')" = 2 ] ||
    fail "the registration and the lookup were not both answered"
# Two sends that carried a capability: the launcher handing the registry to the
# service — which is where ADR-0077 §2's four-capability bound put it — and the
# registry handing the service's endpoint to the client.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_carrying status=0$')" = 2 ] ||
    fail "the two capability deliveries did not both happen"
# Five receives that produced what a message carried, and the arithmetic is the
# whole path: two by the registry — a registration and a lookup — two by the
# service — the registry handed to it, then the client's request — and one by
# the client, which is how the service's endpoint reaches it at all.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call status=0$')" = 5 ] ||
    fail "the receives do not add up to two by the registry, two by the service and one delivery"
# Answered by the registry twice, with nothing in the payload.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply status=0$')" = 2 ] ||
    fail "the registry did not answer its two calls"
# **The call that crosses the whole path**, and the answer to it. Two rows, one
# each way, and both put their number where `IPC_V1` §3 puts a payload.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word status=0$')" = 1 ] ||
    fail "the client did not reach the block service"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')" = 1 ] ||
    fail "the block service did not answer with a payload"

# --- and the length register is not a protocol channel -------------------------
# **The regression that matters, and it is structural.** Neither number of this
# protocol may travel as a message length. `endpoint_call_for` — the row that
# produced a call's answer length as the answer, and the affordance an earlier
# form of this slice used in both directions — is gone from the accepted schema,
# and the two `_word` rows fill the length register themselves, so a module
# cannot reach it through them. Read out of the tree as well as out of the boot,
# because a row that exists is a row a later fixture can reach for.
SCHEMA="$ROOT/interfaces/system/SYSTEM_INTERFACE_V1.md"
TABLE="$ROOT/crates/tos-core/src/interfaces.rs"
HOST="$ROOT/runtime-image/src/main.rs"
for party in "$SCHEMA" "$TABLE" "$HOST"; do
    if grep -q 'endpoint_call_for' "$party"; then
        fail "$(basename "$party") still declares a row producing a call's answer length as its answer"
    fi
done
# And neither fixture reaches an operation whose declared value is a length.
# The client's request and the service's answer are `_word` rows or they are
# nothing: a fixture that called `endpoint_reply` or `endpoint_call` here would
# be one that had a length register to put a number in.
for forbidden in endpoint_reply endpoint_call; do
    for fixture in "$FIXTURE/client.tos" "$FIXTURE/service.tos"; do
        if grep -qE "$forbidden\\(" "$fixture"; then
            fail "$(basename "$fixture") reaches $forbidden, whose declared value is a message length"
        fi
    done
done

# --- and the device really was driven ------------------------------------------
grep -q '^TOS\.RUN\.PCI_ASSIGNED ' "$LOG" ||
    fail "no PCI function was claimed"
grep -q '^TOS\.RUN\.DMA_REGION ' "$LOG" ||
    fail "no DMA region was made"
grep -q '^TOS\.RUN\.IRQ_DELIVERED ' "$LOG" ||
    fail "the device raised no interrupt, so nothing completed through the queue"

# --- the four accounts ---------------------------------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_INIT\$")" = 1 ] ||
    fail "the launcher did not report four releases and three processes: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY\$")" = 1 ] ||
    fail "the registry did not report a registration and a lookup"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLIENT\$")" = 1 ] ||
    fail "the client did not report the whole path: $(grep COMPLETED "$LOG")"

# The service's own account is Stage 4D-2's composite, and its low twenty bits
# are the same twenty facts that gate asserts.
service="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\([0-9]\{7,\}\)$/\1/p' "$LOG" | head -1)"
[ -n "$service" ] || fail "the block service reported no composite at all"
proved=$((service % LEN_SHIFT))
[ "$proved" = "$PROVED_ALL" ] ||
    fail "the service proved $proved of $PROVED_ALL device-side facts"

echo "BLOCK-SERVICE PASS: a client holding no part of the machine received a"
echo "  number that originated in a real device read"
echo "  separate textual client -> IPC -> textual block service -> DMA -> VirtIO"
echo "  -> IRQ -> reply, in four canonical textual modules and four processes"
echo "  the client held no name for the service until the P3 registry sent it"
echo "  one in a message, and then called it with a sector in the payload"
echo "  the service holds the PCI root, claimed the function, mapped the window,"
echo "  claimed one MSI-X source and allocated one DMA region — and the client's"
echo "  plan names none of those: no bus, function, window, source, region or"
echo "  memory authority, read out of the log rather than assumed"
echo "  the answer that crossed back is the device's: 512 of the sector's bytes"
echo "  are zero, where the service's buffer was 0xA5 until the device wrote it,"
echo "  so a run with no DMA would have answered 0 — and 512 is outside"
echo "  \`IPC_V1\` §3's inline bound, so it could not have travelled as a length"
echo "  the service still proves all $PROVED_ALL device-side facts of Stage 4D-2"
echo "  NOT claimed: that the client read a sector. 512 bytes of block data did"
echo "  not cross IPC; one number computed from them did. The boundary of"
echo "  docs/research/STAGE4_DATA_PATH_BOUNDARY.md §1 — client memory through"
echo "  IPC to the service's DMA memory — is not reached here"
echo "  the authority is the publish endpoint's identity and possession of a"
echo "  call name for it (CAPABILITY_V1 §6 as ADR-0095 amended it), so no"
echo "  interface name travels in this protocol"
echo "  NOT claimed here: the negative, which is publication-authority.sh's"
echo "  NOT claimed either: more than one request through the queue (4D-3),"
echo "  writing (4D-5), durability, restart, or anything about performance"
