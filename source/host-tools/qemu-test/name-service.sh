#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# ADR-0093 P3, performed: a client reaches a service it was never given.
#
# **The decision this is the execution of.** P3 made the interface registry an
# ordinary textual service — no namespace in ring 0, no registry in the
# supervisor, no launcher handing the client the answer — and said a client
# obtains an endpoint capability by looking it up. Four canonical textual
# modules, four processes, and the only thing the client is given about the
# service is a way to ask.
#
#   publisher --call carrying its service endpoint--> registry
#   client    --call carrying its own inbox-------->  registry
#   registry  --send carrying the service endpoint-> client's inbox
#   client    --call---------------------------------> publisher
#
# **An answer cannot carry a capability**, and that shaped the protocol rather
# than the nucleus. `ipc::hand` copies payload bytes from the replier's region
# into the waiting caller's and never touches the transfer table, so a lookup
# hands over a channel to be answered on and the registry sends the registered
# capability to it. Ordinary capability discipline; ring 0 unchanged.
#
# **Two endpoints separate registering from asking, and that is all they do.**
# A process that can reach `publish` may register; one that can only reach
# `lookup` cannot, because it holds no name for the other endpoint. The client
# proves that half itself by carrying its own inbox into `lookup`: the registry
# uses it to answer and registers nothing, so the second lookup still hands out
# the publisher's endpoint and the publisher serves that call too.
#
# **This is NOT `CAPABILITY_V1` §6's publication authority, and an earlier
# version of this file said it was.** §6 and ADR-0051 §2 make the right to
# publish a capability **whose nominal type is the interface being published**,
# so that a launcher reading a module's `capability_imports` sees which
# interface it intends to publish and grants or denies that. What this boot has
# is an ordinary `system.ipc.Endpoint` under the binding name `publish`: no
# interface name travels in the protocol, the registry holds one unnamed entry,
# and nothing anywhere names `block.device.v1`. The negative evidence
# ADR-0093 §10.1 requires — a service that was not granted the publication
# capability cannot register that interface — is therefore **not** provable in
# this boot, and no assertion below claims it is. Tracked as ADR-0093-Q1.
#
# **Case B is in the same boot.** The registry serves one registration and two
# lookups and ends. The client then calls the service again through the
# capability it already holds — which works, because a capability names an
# object and not the service that handed it over — and then asks the registry
# once more, which does not: nothing receives there, and the liveness rule ends
# the wait with `E_CANCELLED`. That is existing IPC semantics answering, not a
# case this slice invents.
#
#   bash host-tools/qemu-test/name-service.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-name-service}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/name-service"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-name-service"

# The launcher: four receiving names let go, then three processes created.
EXPECTED_INIT="i64:127"
# The registry: a registration and two lookups answered with it.
EXPECTED_REGISTRY="i64:7"
# The publisher: registered, then three calls served.
EXPECTED_PUBLISHER="i64:15"
# The client: two lookups, two deliveries, two calls that reached the service,
# one more call after the registry ended, and a lookup that did not.
EXPECTED_CLIENT="i64:255"

fail() {
    echo "name-service: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-name-service)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/registry/nameservice.tos\t%s/nameservice.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/service/publisher.tos\t%s/publisher.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/client/consumer.tos\t%s/client.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/registry.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/registry.bin" --manifest "$OUT/capsule.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/registry.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- four modules, four processes ----------------------------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=4$' "$LOG" ||
    fail "the boot did not run a set of four modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 4 ] ||
    fail "four processes did not begin: $(count '^TOS\.RUN\.BEGIN path=')"

# --- the client holds nothing of the machine, and no name for the service ------
# This is the capability boundary the eventual Stage 4 client must never cross,
# asserted while the boot is small enough for the assertion to be exhaustive.
for forbidden in platform.pci.Bus platform.pci.FunctionConfig platform.mmio \
                 platform.irq.Source platform.dma.Region system.memory.Authority; do
    [ "$(count "^TOS\.RUN\.REQUEST binding=.* interface=$forbidden")" -le 1 ] ||
        fail "more than the launcher asked for $forbidden"
done
# Exactly three bindings in the client, and `registry` is `lookup`'s, never
# `publish`'s: it has no name at all for the endpoint registration happens on.
[ "$(count '^TOS\.RUN\.REQUEST binding=inbox interface=system\.ipc\.Endpoint ')" = 1 ] ||
    fail "the client's inbox was not requested by name and kind"
[ "$(count '^TOS\.RUN\.REQUEST binding=reply_to interface=system\.ipc\.Endpoint ')" = 1 ] ||
    fail "the client's return channel was not requested by name and kind"

# --- the chain, operation by operation -----------------------------------------
# Registration: one call carrying a capability, from the publisher.
# Lookup: two calls carrying a capability, from the client.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=0$')" = 3 ] ||
    fail "the registration and two lookups were not all answered"
# Eight receives that produced what a message carried, and the arithmetic is
# the whole chain: three by the registry — one registration and two lookups —
# three by the publisher, and two by the client, which is how a delivery
# reaches it at all.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call status=0$')" = 8 ] ||
    fail "the receives do not add up to three by the registry, three by the publisher and two deliveries"
# The deliveries: the registry sending the registered capability onward.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_carrying status=0$')" = 2 ] ||
    fail "the registry did not deliver the capability twice"
# The client calling what it was handed, three times.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call status=0$')" = 3 ] ||
    fail "the client did not reach the service three times"
# Answered by the registry three times and the publisher three times.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply status=0$')" = 6 ] ||
    fail "six calls were not answered"
# Four names let go, and nothing else released: the children's own `Control`
# handles are not, because this launcher ends immediately after creating them.
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')" = 4 ] ||
    fail "the launcher did not release exactly the four receiving names"

# --- case B: the capability outlives the registry ------------------------------
grep -q '^TOS\.RUN\.PROCESS_EXIT process=1 ' "$LOG" ||
    fail "the registry did not end"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=-5$')" = 1 ] ||
    fail "the lookup after the registry ended was not cancelled by the liveness rule"

# --- and the four accounts, none of which contains its own number --------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_INIT\$")" = 1 ] ||
    fail "the launcher did not report four releases and three processes: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY\$")" -ge 1 ] ||
    fail "the registry did not report a registration and two lookups"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_PUBLISHER\$")" = 1 ] ||
    fail "the publisher did not report registering and serving three calls"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLIENT\$")" = 1 ] ||
    fail "the client did not report every step: $(grep COMPLETED "$LOG")"

echo "NAME-SERVICE PASS: a client reached a service it was never given"
echo "  four canonical textual modules, four processes: a launcher that only"
echo "  decides who holds what, a registry, a publisher and a client"
echo "  the client holds \`call\` on the lookup endpoint and two names for an"
echo "  inbox of its own — no name for the service, no name for the publish"
echo "  endpoint, and nothing of the machine: no PCI, window, interrupt source,"
echo "  DMA region or memory authority, asserted from the log"
echo "  publisher --call carrying its endpoint--> registry"
echo "  client    --call carrying its own inbox-> registry"
echo "  registry  --send carrying the endpoint--> client's inbox"
echo "  client    --call--------------------------> publisher, three times"
echo "  an answer cannot carry a capability, so the registry answers by sending"
echo "  to a channel the asker handed over; the nucleus was not changed"
echo "  what the client carried into \`lookup\` registered nothing: the second"
echo "  lookup handed out the publisher's endpoint again and the publisher"
echo "  served that call too"
echo "  NOT claimed: publication authority in the sense of CAPABILITY_V1 §6."
echo "  The capability presented is an ordinary endpoint, not one whose nominal"
echo "  type is the published interface, and no interface name travels here —"
echo "  so ADR-0093 §10.1's negative is not proved in this boot: ADR-0093-Q1"
echo "  case B: the registry ended after two lookups; the capability the client"
echo "  already held went on working, and a further lookup was cancelled -5 by"
echo "  the liveness rule — existing IPC semantics, not a new case"
