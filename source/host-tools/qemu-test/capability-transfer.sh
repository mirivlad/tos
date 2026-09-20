#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A capability crosses between two canonical textual processes in a message.
#
# **The prerequisite ADR-0093 P3 turned out to need.** P3 makes the interface
# registry an ordinary textual service, so a client obtains an endpoint
# capability by looking it up — not by being endowed with it at launch. That
# was impossible: the accepted schema had no way for a module to put a
# capability into a message or to take one out, and no way to obtain the reply
# a call leaves in the receiver's transfer table, so a textual service could
# not answer a call at all. Four `.tos` vectors touch IPC and all four only
# drain; `deputy.sh` and `request-reply.sh`, where transfer really happens, use
# no `.tos` vector and are Rust runtime stages.
#
# The nucleus needed nothing. `resolve_transfers` already reads the sender's
# table and delegates at exactly the rights the sender holds; operation 3
# already makes the reply and puts it in the last slot; `hand_over` already
# grants the receiver its own names. What was missing was two schema rows and
# the bridge vocabulary to perform them, and this is the boot that proves them.
#
# **What the evidence is, and what it is not.** A reply proves the server ran.
# It does not prove the server received anything. The transfer is proved by the
# third step: the client delegates `channel` and then receives on `channel`
# itself. A message can only be there if the server sent one, and the server
# could only send one if the capability it was handed really named that
# endpoint. Nothing in the client puts that message there.
#
# **And the negative is in the same boot**, because the declared type of a
# received capability is a claim rather than a proof: `carried` is declared
# `system.ipc.Endpoint` because that is what the service expects, and the
# nucleus is what decides. Round two carries nothing, the slot is zero, and the
# same send must be refused `E_NO_CAPABILITY`. A host that handed out a usable
# capability where none was sent fails here rather than passing quietly.
#
#   bash host-tools/qemu-test/capability-transfer.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-capability-transfer}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/capability-transfer"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-capability-transfer"

# 1 created + 2 first receive + 4 its own handle cannot send
# + 8 the carried one can + 16 it named `channel` + 32 second receive
# + 64 the empty slot refused + 128 a reported status + 256 that status was 3
EXPECTED_VALUE="i64:127"

fail() {
    echo "capability-transfer: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-capability-transfer)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n/system/boot/client.tos\t%s/client.tos\n' \
    "$FIXTURE" "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/transfer.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/transfer.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/transfer.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- two modules, two processes ------------------------------------------------
# Without this the "two processes" could be one module talking to itself, and
# the boundary the whole slice is about would be unevidenced.
grep -q '^TOS\.RUN\.BEGIN .* modules=2$' "$LOG" ||
    fail "the boot did not run a set of two modules"

# --- the client holds no device authority of any kind --------------------------
# It is the ancestor of the eventual Stage 4 client, and the capability boundary
# it must never cross is asserted here, while the boot is small enough that the
# assertion is exhaustive rather than selective.
for forbidden in platform.pci.Bus platform.pci.FunctionConfig platform.mmio \
                 platform.irq.Source platform.dma.Region; do
    grep -q "^TOS\.RUN\.REQUEST .*interface=$forbidden" "$LOG" &&
        fail "a process in this boot asked for $forbidden, which this slice must not reach"
done

# --- the client asked for exactly two authorities, by name ---------------------
# `call` on the server's endpoint and `send` on the other one. It cannot
# receive anything, cannot create, cannot spend. Everything it does is through
# those two, and what it hands over is one of them.
[ "$(count '^TOS\.RUN\.REQUEST binding=inbox interface=system\.ipc\.Endpoint ')" = 2 ] ||
    fail "the inbox was not requested by name and kind in both processes"
[ "$(count '^TOS\.RUN\.REQUEST binding=channel interface=system\.ipc\.Endpoint ')" = 2 ] ||
    fail "the channel was not requested by name and kind in both processes"

# --- the operation that had never been performed -------------------------------
# Two rounds, so two receives that produce what the message carried. Before this
# change the accepted schema could not express one, and no canonical textual
# module had ever answered a call.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call status=0$')" = 2 ] ||
    fail "the server did not serve two calls"

# --- a call that carried a capability, and one that did not --------------------
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=0$')" = 1 ] ||
    fail "the call carrying a capability was not answered"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call status=0$')" = 1 ] ||
    fail "the call carrying nothing was not answered"

# --- the asymmetry, which is the whole evidence --------------------------------
# Three sends by the same process on the same endpoint. Two are refused and one
# succeeds, and the one that succeeds went through a capability that arrived in
# a message. A count is used rather than an order because the log is a set of
# lines; the order is asserted by the composite the module returns, whose bits
# are set in sequence and whose value is 255 only if each held when it ran.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send status=0$')" = 1 ] ||
    fail "the send through the delegated capability did not succeed exactly once"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send status=-1$')" = 2 ] ||
    fail "the two sends without authority were not both refused E_NO_CAPABILITY"

# --- both calls were answered --------------------------------------------------
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply status=0$')" = 2 ] ||
    fail "the server did not answer both calls"

# --- and the composite neither module contains ---------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$")" = 1 ] ||
    fail "the server did not report every step: $(grep 'COMPLETED' "$LOG")"

echo "CAPABILITY-TRANSFER PASS: a capability crossed between two textual processes"
echo "  two canonical textual modules, two processes. The boot process is the"
echo "  server and the launcher: it holds \`receive\` on its inbox, \`receive\` and"
echo "  \`send\` on a second endpoint, and creates a client endowed with \`call\` on"
echo "  the first and \`send\` on the second — and nothing else at all: no PCI, no"
echo "  window, no interrupt source, no DMA region, no memory authority, asserted"
echo "  from the log rather than assumed"
echo "  \`endpoint_receive_call\` is the first operation with which a canonical"
echo "  textual module has ever served a call: the right to answer arrives in the"
echo "  transfer table and until now nothing in the schema could name it"
echo "  round 1 is an asymmetry, which is the evidence. The server sends on its"
echo "  own inbox and is refused -1, because it holds no \`send\` there; it then"
echo "  sends through the capability the call carried and succeeds; and a message"
echo "  is then waiting on \`channel\`, where it receives — so what arrived was the"
echo "  client's name for that endpoint, and it conveyed authority the server did"
echo "  not otherwise have"
echo "  round 2 is the same call carrying nothing: the slot is zero, and the same"
echo "  send is refused -1. The declared type of a received capability is a claim"
echo "  the nucleus checks when it is used, and it fails closed"
echo "  the server reported $EXPECTED_VALUE, a composite neither module contains"
echo "  the nucleus was not changed. Transport, reply creation, delegation rights"
echo "  and the receiver's own naming were already there and already gated by"
echo "  deputy.sh; what this adds is two schema rows and the bridge vocabulary"
echo "  to perform them"
