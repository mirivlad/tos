#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A call that carries a capability reads the reply it is answered with.
#
# **ADR-0101.** `endpoint_call_word_carrying` was admitted producing `i64`, so its
# caller learned that the call had been answered and nothing about the answer — the
# exact failure `SYSTEM_INTERFACE_V1` §4.2 gives as the reason `system.ipc.Answer`
# exists, and the reason `BLOCK_DEVICE_V1` §5's "a client reads
# `system.ipc.Answer{length, word}`" was unreadable by the §6a `READ` client that has
# to obey it. The reply was never missing: `ipc::hand` copies the replier's payload
# into the woken caller's own argument region and the nucleus returns its inline
# length. The row discarded both. Now it does not.
#
# **This gate is deliberately not about blocks.** `block-protocol` carries the same
# correction inside `block.device.v1`, and it should — but a claim about one schema
# row should be provable without a device, a sector or a driver. So: two processes,
# one endpoint between them, a second endpoint that exists only to be a capability
# worth carrying, and a protocol invented for this boot alone.
#
#   asker:    ask 5                       -> Answer{length: 8, word: 1000005}
#   asker:    ask 9001                    -> Answer{length: 8, word: 1009001}
#   asker:    ask REFUSED + 11 - 1000000  -> Answer{length: 8, word: REFUSED + 11}
#   asker:    ask 0                       -> Answer{length: 0, ...}
#   answerer: releases the carried name, then replies
#
# **Four assertions that are not four copies of one.**
#
# *The word is not a constant.* The first two answers differ, so nothing about the row
# can be supplying the number.
#
# *The word is not the caller's own.* `Produced::Answer` reads the payload offset of
# the caller's **own** argument region — which is exactly where the caller wrote its
# request word. A row that copied nothing back would hand the caller the number it had
# just written and satisfy an echo protocol. So the answer is the request plus a fixed
# shift, and a stale payload is a different observation from a copied reply.
#
# *The refusal class exists.* One answer's word is above the top bit, which is the
# class `BLOCK_DEVICE_V1` §5 needs a caller to see and the one an `i64` result could
# not carry.
#
# *The length is the replier's.* One exchange is answered with `endpoint_reply` and no
# payload at all, and the asker requires exactly zero back — so `length` cannot be a
# constant this row reports.
#
# **Not claimed:** anything about `block.device.v1` (that is `block-protocol`'s),
# anything about `state.store.v1` (ADR-0099 is accepted and unimplemented), any
# ordering between a reply and a region, or any rule about what a carried channel is
# for — nothing receives on the endpoint whose name is carried here, and this boot
# says so rather than building a second protocol to look complete.
#
#   bash host-tools/qemu-test/carried-call-answer.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-carried-call-answer}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/carried-call-answer"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-carried-call-answer"

# How many exchanges the boot makes. Everything counted below is derived from it.
EXCHANGES=4

# The supervisor: two children created, two endings collected. Bits 1..8.
EXPECTED_SUPERVISOR="i64:15"
# The answerer: four eight-byte requests, four carried names let go, four replies
# accepted, one of them empty. Bits 16..128.
EXPECTED_ANSWERER="i64:240"
# The asker: the first word, a different second word, the refusal class, and a length
# the replier chose. Bits 256..2048.
EXPECTED_ASKER="i64:3840"

fail() {
    echo "carried-call-answer: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-carried-call-answer)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/test/answerer.tos\t%s/answerer.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/test/asker.tos\t%s/asker.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/path.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/path.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/path.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null

LOG="$OUT/live/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- three modules, three processes, and nothing of the machine -----------------
grep -q '^TOS\.RUN\.BEGIN .* modules=3$' "$LOG" ||
    fail "the boot did not run a set of three modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 3 ] ||
    fail "three processes did not begin: $(count '^TOS\.RUN\.BEGIN path=') did"
for never in platform.pci.Bus platform.pci.FunctionConfig platform.irq.Source \
    platform.dma.Region platform.mmio.Region system.memory.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested, and this boot needs no part of the machine and no
       region: what is proved is one schema row's result"
done

# --- 1: nothing waited for a message it could never be given --------------------
# Checked first, so that a topology mistake fails on the rule it is about rather than
# on a later count. A cancelled receive or call is what an undeliverable message looks
# like (`SYSTEM_ABI_V1` §6), and it would make every assertion below vacuous.
for cancelled in endpoint_receive_call endpoint_call_word_carrying; do
    [ "$(count "^TOS\.RUN\.INTERFACE operation=$cancelled status=-5\$")" = 0 ] ||
        fail "$cancelled was cancelled, so something waited for a message nobody
       could deliver"
done
grep -q '^TOS\.RUN\.LIVENESS .*verdict=stalled' "$LOG" &&
    fail "the boot stalled: something was waiting for a message nobody could deliver"

# --- 2: the corrected row was the one used, four times, and none refused ---------
# **This is the row ADR-0101 changes**, and no sibling stands in for it: there is no
# `endpoint_call_word` and no `endpoint_call_word_region` in this boot at all, so
# every answer asserted below came through the row whose result was corrected.
used="$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_carrying status=0$')"
[ "$used" = "$EXCHANGES" ] ||
    fail "the corrected row was used $used times; this boot makes $EXCHANGES calls"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_carrying status=-[0-9]*$')" = 0 ] ||
    fail "a carried call was refused, so the answer it reported is not an answer"
for sibling in endpoint_call_word endpoint_call_word_region endpoint_call; do
    [ "$(count "^TOS\.RUN\.INTERFACE operation=$sibling status=")" = 0 ] ||
        fail "$sibling was used; this boot's answers must all come through the row
       ADR-0101 corrects"
done

# --- 3: both reply rows were used, and in the numbers the protocol says ----------
# Three words and one empty reply. The empty one is what makes `Answer.length` the
# replier's: a row reporting a constant eight would pass every other check here.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')" = "$((EXCHANGES - 1))" ] ||
    fail "expected $((EXCHANGES - 1)) word replies, saw
       $(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply status=0$')" = 1 ] ||
    fail "expected exactly one reply with no payload, the one that proves the length
       is the replier's"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=-[0-9]*$')" = 0 ] ||
    fail "a word reply was refused"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply status=-[0-9]*$')" = 0 ] ||
    fail "the empty reply was refused"

# --- 4: nothing a request handed over is still held -----------------------------
# Derived rather than observed, so a number that moved would have to be explained:
#
#   supervisor   2 child controls, and the 2 endpoint names it let go before creating
#                the children that hold them
#   answerer     1 carried name per exchange
#   asker        nothing: a delegation copies, and the asker keeps its own name for
#                the whole boot, which is what makes four calls one endowment
SUPERVISOR_RELEASES=4
ANSWERER_RELEASES=$EXCHANGES
EXPECTED_RELEASES=$((SUPERVISOR_RELEASES + ANSWERER_RELEASES))
releases="$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')"
[ "$releases" = "$EXPECTED_RELEASES" ] ||
    fail "the boot released $releases capabilities; this topology releases
       $EXPECTED_RELEASES"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=-[0-9]*$')" = 0 ] ||
    fail "a release was refused, so something was held that could not be let go"

# --- 5: the accounts, one range of bits each ------------------------------------
# Three distinct numbers, so each says which module reported it.
completed=$(grep '^TOS\.RUN\.COMPLETED value=' "$LOG" | sed 's/^TOS\.RUN\.COMPLETED value=//')
for want in "$EXPECTED_SUPERVISOR" "$EXPECTED_ANSWERER" "$EXPECTED_ASKER"; do
    [ "$(printf '%s\n' "$completed" | grep -c "^$want$")" = 1 ] ||
        fail "no module reported $want; the boot reported: $(printf '%s ' $completed)"
done

echo "carried-call-answer: PASS: a call that carries a capability reads its reply"
echo "  endpoint_call_word_carrying over SYSTEM_ABI_V1 operation 3 now produces"
echo "  Result<system.ipc.Answer, i64> (ADR-0101) — no ABI operation, no nucleus"
echo "  change, no capability or object kind, no bound, no language-version move"
echo "  two distinct success words, so the number is not the row's"
echo "  each answer is the request plus a fixed shift, so a row that copied nothing"
echo "  and left the caller reading its own request word would fail rather than pass"
echo "  one answer above the top bit: the refusal class BLOCK_DEVICE_V1 §5 needs a"
echo "  caller to see, and the one an i64 result could not carry"
echo "  one answer with no payload at all, so Answer.length is the replier's choice"
echo "  and not a constant this row reports"
echo "  no device, no region, no driver: the claim is about the schema row, so this"
echo "  boot is provable without block.device.v1"
echo "  NOT claimed: block.device.v1 (block-protocol's), state.store.v1 (ADR-0099"
echo "  is accepted and unimplemented), any reply-before-region ordering, or any"
echo "  rule about what a carried channel is for"
