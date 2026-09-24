#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A process makes a send-only name for an endpoint it receives on, and hands it over.
#
# **ADR-0100.** `IPC_V1` §6 delegates a capability at exactly the rights the sender
# holds, and §2 admits one receive-rights holder per endpoint at a time. So a process
# that handed over the name it receives on would be asking the receiver to become a
# second receiver — and the accepting receive refuses the **whole message**, leaving
# the request in the queue while both processes wait for each other. `block-protocol`
# deadlocked exactly there before it was given two startup names for one endpoint,
# and `ADR-0099`'s accepted endowment counts cannot afford a second name.
#
# `CAPABILITY_V1` §4's attenuation is the answer and the nucleus has always accepted
# an endpoint for it; what did not exist until ADR-0100 was a row by which canonical
# text could name it. This is that row's evidence.
#
#   holder: alias = capability_attenuate(link, RIGHT_SEND)
#   holder: hands the alias to the peer over `channel`, and releases its own copy
#   holder: sends to `link` and receives it     the original was not consumed and
#                                              is still the receiver
#   peer:   receives **through the alias**      -> E_NO_CAPABILITY
#   peer:   sends through the alias
#   holder: receives that message on `link`     the alias carries `send`, and the
#                                              original is where its messages go
#   holder: send_only  = attenuate(link, SEND)
#           widened    = attenuate(send_only, SEND | RECEIVE)
#           widened sends; a receive through it is refused — intersection, not
#           validation, and it never acquires `receive`
#
# **Why the refusals are about rights.** A resolve that finds the capability but not
# the right the operation requires answers `E_NO_CAPABILITY`; a receive whose rights
# are fine and whose queue is empty answers `E_WOULD_BLOCK` when told not to wait and
# blocks when not. The exact status is therefore what distinguishes "this name may not
# receive" from "nothing has arrived", and both modules assert the exact status. In
# the holder's case there is also a message waiting at that moment, which makes it
# unambiguous twice over.
#
# **The mutation is the one-receiver rule itself.** Delegating the *unattenuated*
# `send | receive` name must be refused, and by `IPC_V1` §2's existing rule rather
# than by anything this gate adds.
#
#   bash host-tools/qemu-test/endpoint-attenuation.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-endpoint-attenuation}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/endpoint-attenuation"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-endpoint-attenuation"

# The supervisor: two children created, two endings collected.
EXPECTED_SUPERVISOR="i64:15"
# The holder: attenuated, delegated, released, the original still receives, the
# alias's message arrived, the widened alias still sends, and it cannot receive.
EXPECTED_HOLDER="i64:127"
# The peer: the alias arrived, it cannot receive through it, it can send.
EXPECTED_PEER="i64:7"
# `E_NO_CAPABILITY`. The resolve refusing a right the capability does not carry.
E_NO_CAPABILITY=-1

fail() {
    echo "endpoint-attenuation: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-endpoint-attenuation)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/test/holder.tos\t%s/holder.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/test/peer.tos\t%s/peer.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
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
    platform.dma.Region platform.mmio.Region; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$never ")" = 0 ] ||
        fail "$never was requested by a module, and this boot needs no part of the machine"
done

# --- 1: nothing waited for a message it could never be given --------------------
#
# **This is `IPC_V1` §2's rule, read from the waiter's side.** A send that would make
# its receiver a second receive-rights holder is *queued* — the sender is told
# nothing, because `has_room` is all a send checks — and then refused at delivery:
# the accepting receive cannot take the whole message, so the message stays in the
# queue and the waiter stays blocked until the liveness rule cancels it
# (`SYSTEM_ABI_V1` §6). A cancelled receive is therefore exactly what an
# undeliverable message looks like, and the mutation that delegates the
# **unattenuated** `send | receive` name produces one.
#
# So: no receive in this boot was cancelled. Checked before anything else, so that
# the mutation fails on the rule it is about rather than on a later count.
for cancelled in endpoint_receive endpoint_receive_call; do
    [ "$(count "^TOS\.RUN\.INTERFACE operation=$cancelled status=-5\$")" = 0 ] ||
        fail "$cancelled was cancelled, which is what a message that cannot be
       delivered looks like: IPC_V1 §2 refuses a delivery that would make a second
       receive-rights holder, and a delegated name carrying \`receive\` asks for
       exactly that"
done
grep -q '^TOS\.RUN\.LIVENESS .*verdict=stalled' "$LOG" &&
    fail "the boot stalled: something was waiting for a message nobody could deliver"

# --- 2: the attenuations happened, and none of them was refused -----------------
# Three: the alias the peer is given, the send-only name of the widening test, and
# the widening itself. A fourth would be a fixture nobody read.
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=0$')" = 3 ] ||
    fail "expected exactly three successful endpoint attenuations, saw
       $(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=0$')"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=-[0-9]*$')" = 0 ] ||
    fail "an attenuation was refused; asking for a right a capability lacks is an
       intersection and not an error (CAPABILITY_V1 §4)"

# --- 3: the alias crossed, which is the deadlock's absence ----------------------
# One carrying send, and it succeeded. Under the mutation this is the line that
# refuses: `IPC_V1` §2 forbids a second receive-rights holder, and the accepting
# receive refuses the whole message rather than stripping the right.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_carrying status=0$')" = 1 ] ||
    fail "the alias was not delegated exactly once"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_carrying status=-[0-9]*$')" = 0 ] ||
    fail "delegating the alias was refused, which is what an unattenuated name would do"

# --- 4: exactly two receives were refused, and for the right reason -------------
# The peer through the alias it was handed, and the holder through the widened one.
# **The status is the claim**: `E_NO_CAPABILITY` is a resolve refusing a right the
# capability does not carry, and it is not what an empty queue answers.
refused="$(count "^TOS\.RUN\.INTERFACE operation=endpoint_receive status=$E_NO_CAPABILITY\$")"
[ "$refused" = 2 ] ||
    fail "expected exactly two receives refused as E_NO_CAPABILITY, saw $refused"
# And no receive was refused any other way, so neither of those two is an empty
# queue wearing a different number.
other="$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive status=-[0-9]*$')"
[ "$other" = 2 ] ||
    fail "a receive was refused for some reason other than the rights check: $other
       refusals in all, and only two may exist"

# --- 5: the accounts ------------------------------------------------------------
completed=$(grep '^TOS\.RUN\.COMPLETED value=' "$LOG" | sed 's/^TOS\.RUN\.COMPLETED value=//')
for want in "$EXPECTED_SUPERVISOR" "$EXPECTED_HOLDER" "$EXPECTED_PEER"; do
    [ "$(printf '%s\n' "$completed" | grep -c "^$want$")" = 1 ] ||
        fail "no module reported $want; the boot reported: $(printf '%s ' $completed)"
done

echo "endpoint-attenuation: PASS: a send-only name for an endpoint its holder receives on"
echo "  capability_attenuate over SYSTEM_ABI_V1 operation 5, named from canonical"
echo "  text for the first time (ADR-0100) — no ABI operation, no nucleus change,"
echo "  no capability or object kind, no bound, no language-version move"
echo "  the original is not consumed and stays the receiver: the holder sends to"
echo "  its own endpoint through the name it attenuated and takes the message back"
echo "  the alias crosses a message, which is the deadlock's absence: a delegation"
echo "  carries the rights the sender holds, so an unattenuated name would have"
echo "  been refused whole by IPC_V1 §2's one-receiver rule"
echo "  the peer sends through the alias and the message lands on the holder's own"
echo "  queue — so the alias carries send, and the receiver did not move"
echo "  and it carries send **only**: a receive through it answers E_NO_CAPABILITY,"
echo "  the resolve refusing a right the capability does not have, which is a"
echo "  different status from the one an empty queue gives"
echo "  attenuation is an intersection and not a validation: a send-only name asked"
echo "  for send|receive yields send — it still sends, and it never receives"
echo "  NOT claimed: anything about STATE_STORE_V1, which ADR-0099 accepted and"
echo "  nothing implements; and no rule about who may attenuate — holding the"
echo "  capability is the authority, as on the four interfaces that already had it"
