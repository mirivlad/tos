#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A persistent object store, read back by a process that came after — and the whole of its
# refusal surface.
#
# **ADR-0099 and `STATE_STORE_V1`.** Stage 4 owes *"persistent object/state storage"* with
# the engineering exit *"persistent storage works through a textual user-space driver"*.
# This is that, on the reference VirtIO device, with every layer canonical text: a block
# service speaking the accepted `block.device.v1`, a store speaking the accepted
# `state.store.v1` above it, and clients that hold no way to address a sector.
#
# **Two boots, because the contract has two kinds of claim.**
#
# *The ordinary boot*, against the 16 MiB reference image:
#
#   block service            created first, ends last; the only holder of the bus
#   initializer, run 1       capacity >= 65; sector 0 is all zero; one header written
#   state store A            capacity, then the header: opened and validated
#   writer                   ST_OPCODE, ST_ID, ST_NO_REGION; put(1, A); put(2, B)
#   A ends, is retired, and **both** endings are collected before anything else runs
#   initializer, run 2       the **adversarial re-provisioning probe**: the same module,
#                            the same sealed plan, against a header whose occupancy now
#                            has two bits set. It must refuse to format
#   state store B            the same module from the same plan; re-reads and validates
#                            the header from the device
#   reader                   ST_ID, ST_NO_ANSWER, ST_MALFORMED, ST_ABSENT; get(2)
#                            verified byte by byte
#
# *The too-small boot*, against a 64-sector image — one short of the bounded extent:
#
#   initializer              capacity < 65: it formats nothing and says so
#   state store              capacity < 65: it is **not open**, and serves ST_STORE
#   probe                    five differently-shaped requests, five ST_STORE answers
#
# **What makes the first a persistence claim rather than a message-passing one.** The writer
# has ended and been collected. Generation A has ended, been retired — `process::retire`
# clears its capabilities — and had its ending collected. Generation B is a different
# process that was handed nothing but a sealed plan. So the 512 bytes the reader checks came
# off the device, and the header B validated is the one on the device.
#
# **Claims this gate is built around.**
#
# *Only an all-zero sector 0 is permission to format* (§4.4), **and the store it refuses to
# touch survives**. The second initializer run is a second *process* rather than a second
# code path — the same module from the same plan, differing only in what it finds — and it
# runs **after** two objects exist and after every state service has been collected. An
# initializer that wrote unconditionally would reset `occupancy` and lose both objects while
# reporting success; the reader's later `get(2)` is the witness that it did not. Running the
# probe against an *empty* valid store would have proved only the easy half.
#
# *Presence is `occupancy` and nothing else* (§5). `get(3)` is an id never created, and the
# harness seeds sector 3 with 512 copies of `0xC3` — so a store that inferred presence from
# bytes would answer it with that fill. The refusal is required to be exactly
# `REFUSED + ST_ABSENT`, and the fourteen counted device requests say its sector was never
# read.
#
# *Every refusal §9 defines is exercised, by exact reply word where the caller can read
# one.* `ST_OPCODE`, `ST_ID`, `ST_NO_REGION` by the writer; `ST_ID` at the other end of the
# range, `ST_NO_ANSWER` and `ST_ABSENT` by the reader; `ST_STORE` five times in the
# too-small boot. `ST_MALFORMED` is the one whose caller **cannot** read the reply —
# `endpoint_call` is the only row that lets a client choose an inline length, and its result
# is a status — so what is proved is that the call was answered plus the store's own
# account bit, which is the shape `block-protocol`'s malformed evidence already has. **None
# of them reaches the device**, and the counted device requests are what says so.
#
# *`ST_BLOCK` is implemented and not exercised*, and that is stated rather than coloured
# green: the conforming reference endpoint answers every well-formed in-range request with
# `VIRTIO_BLK_S_OK`, so no real lower refusal is reachable from a valid `state.store.v1`
# request, and no device failure is manufactured to make the row look tested.
#
# *An incomplete lower operation is not a refusal* (§2a, §9). The store keeps three classes
# apart — the lower layer succeeded, the lower layer **refused**, or nobody refused anything
# — and only the middle one becomes `ST_BLOCK`. Mutations 8 and 9 below are what prove it.
#
# *A valid header is not enough to be open* (§9). `ST_STORE` is *"no valid header, **or the
# device is too small**"*, so the ordinary store asks the layer below for the capacity
# before it reads sector 0 — and in the too-small boot it reads sector 0 **not at all**,
# which the three resolved device addresses of that boot (the ring's own, and no request's)
# state exactly.
#
# *Control before data, at both layers* (§8b, `BLOCK_DEVICE_V1` §6a), asserted as one
# six-step sequence so that a send before a reply at **either** height turns it red.
#
# *The store's isolation boundary is capability topology*, read from the journal and not
# from source: the bus is requested by the supervisor and the block service and nobody else;
# `block.device.v1`'s endpoint by the two initializer runs and the two store generations;
# and the store's own endpoint only by clients.
#
# *And the corrected endowment accounting is read off the journal too* (ADR-0101 §6): the
# supervisor endowed eight, each store generation four — which is `MAX_ENDOWMENT` — and the
# block service, each initializer run and each client three.
#
# **The nine mutations `ADR-0099` §13 requires plus the two §2a demands**, each verified to
# turn this gate red on its own assertion:
#
#   1 omit the initializer's header write          -> the writer reports i64:-101: its very
#                                                     first probe was answered ST_STORE rather
#                                                     than ST_OPCODE, because the store could
#                                                     not open
#   2 omit the payload device write                -> the reader reports i64:-8, its byte
#                                                     witness
#   3 answer get(2) from object 1's sector         -> the reader reports i64:-8
#   4 ignore the occupancy bitmap                  -> the reader reports i64:-102: get(3), an
#                                                     id never created, was answered instead
#                                                     of refused
#   5 omit PUT(2)'s occupancy-header update        -> the reader reports i64:-6: the
#                                                     successor read the persisted header and
#                                                     object 2 is absent
#   6 send GET's region before replying success    -> §6's sequence sees
#                                                     reply/send/receive/**send/reply**/receive
#   7 remove the initializer's zero-header check   -> the re-provisioning probe formats over a
#                                                     populated store, `occupancy` is reset,
#                                                     and the reader reports i64:-6 — the
#                                                     persisted object is gone. **Not merely a
#                                                     different initializer account**
#   8 make the GET's lower READ call fail at the
#     IPC level (its answer channel is released
#     before the call is made)                     -> the store reports i64:-5340 and the
#                                                     reader's get(2) is **cancelled**
#                                                     (i64:-305). A store that fabricated
#                                                     ST_BLOCK would make the reader report
#                                                     i64:-6 instead
#   9 have the block service reply success to the
#     READ of sector 2 and then never send the
#     owed region                                  -> the same two values: i64:-5340 and
#                                                     i64:-305, never i64:-6
#
# **Mutations 8 and 9 are the ones §2a is about, and the reader's value is the whole point.**
# A cancelled call is the client observing an incomplete operation through the liveness path,
# which is what §8b says happens to it. An `ST_BLOCK` reply would be this store reporting a
# refusal the layer below never made — and a client cannot tell an invented refusal from a
# real one, which is why the two must not be collapsed. Mutation 9 is the honest shape of
# "the service died between the reply and the send", and §8b puts the reply first *precisely*
# so that a client can be in that state.
#
# **Mutations 2 and 3 share an assertion on purpose.** `ADR-0099` §13 names both as the byte
# witness, because what each breaks is the same statement — that the 512 bytes the reader
# verifies are object 2's, written by a process that has ended.
#
# **Not claimed:** power-loss durability, `VIRTIO_BLK_F_FLUSH`, crash consistency,
# journaling, transactions, exactly-once `PUT`, delete, enumeration, more than one owner or
# store, `docs/09`'s `/state` namespace, any filesystem or path semantics, or Stage 4
# closure.
#
#   bash host-tools/qemu-test/state-store.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-state-store}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/state-store"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-state-store"

# --- the accounts of the ordinary boot, one distinct value each -----------------
# **Distinct on purpose**, and each module's bits are its own range: eight processes
# reporting overlapping numbers would let a gate counting one of them be satisfied by
# another, and this boot's whole subject is what one process observed after another ended.
#
# The supervisor: seven children in five phases, every ending collected as a clean exit.
EXPECTED_SUPERVISOR="i64:2047"
# The block service: Stage 4D's twenty device-side facts, the device configuration found,
# its capacity stable under §2.5.1's generation protocol, and capacity, a write and a read
# served. **And not one refusal bit**, which is its own statement: nothing above it ever
# sent a malformed, out-of-range or ill-formed block request — every refusal in this boot
# was decided above the device.
BLOCK_DEVICE_ALL=1048575
BLOCK_SERVED=$(( (1 << 25) - (1 << 20) ))
EXPECTED_BLOCK="i64:$((BLOCK_DEVICE_ALL + BLOCK_SERVED))"
# The initializer, twice: formatted, then refused to format. Bits 1..16.
EXPECTED_FORMATTED="i64:15"
EXPECTED_REFUSED_TO_FORMAT="i64:19"
# The store, twice. Bits 64..8192: opened, every request answered, three shape refusals —
# plus two objects created for A, and an absent id refused, one object served and one
# malformed length refused for B.
EXPECTED_STATE_A="i64:4544"
EXPECTED_STATE_B="i64:14016"
# The writer: three refusals by exact code, then two objects. Bits 4096..65536.
EXPECTED_WRITER="i64:126976"
# The reader: three refusals, an absent id, the acknowledgement, the region and every byte.
# Bits 131072..8388608.
EXPECTED_READER="i64:16646144"

# --- the accounts of the too-small boot ----------------------------------------
# Its supervisor: four children, every ending collected — the last after the census that
# ends the block service's wait.
EXPECTED_SMALL_SUPERVISOR="i64:127"
# The initializer: the device cannot hold the bounded extent, so it reads nothing and writes
# nothing and reports only that.
EXPECTED_SMALL_INITIALIZER="i64:32"
# The store: not open, and every request refused with ST_STORE.
EXPECTED_SMALL_STATE="i64:2048"
# The probe: five differently-shaped requests, five ST_STORE answers. Bits 2^24..2^28.
EXPECTED_PROBE="i64:520093696"
# The block service of that boot: it configures the device and serves two `CAPACITY` calls,
# so the five facts that need a completed ring request are **absent** — and then every client
# is gone and the liveness rule ends its receive, which is the bit at 2^32.
EXPECTED_SMALL_BLOCK="i64:4302331903"

# --- what the ordinary boot's journal must count -------------------------------
# The block service's fourteen requests, derived rather than chosen:
#
#   initializer run 1   capacity, READ 0, WRITE 0                             3
#   state A             capacity and READ 0 to open; then object 1 and object 2
#                       at a payload sector and a header each                 6
#   initializer run 2   capacity, READ 0 — then it refuses                    2
#   state B             capacity and READ 0 to open, and READ 2               3
BLOCK_REQUESTS=14
# Four of them are `CAPACITY`, answered from the device's configuration space and reaching no
# ring; the other ten are ring requests at three resolved addresses each, over and above the
# three the ring's own setup resolves.
CAPACITY_REQUESTS=4
RING_REQUESTS=$((BLOCK_REQUESTS - CAPACITY_REQUESTS))
RING_ADDRESSES=3
ADDRESSES_PER_REQUEST=3
# And the two store generations, five requests each.
STORE_REQUESTS=5

fail() {
    echo "state-store: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-state-store)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

# **One nucleus and two capsules.** The launcher constant is the same for both boots — the
# same five endpoints and the same bus — so what differs between them is the canonical text
# and the size of the device, which is what makes the second boot a statement about the
# device rather than about the nucleus.
printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
printf '/system/service/block.tos\t%s/block.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/state/initializer.tos\t%s/initializer.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/state/store.tos\t%s/state.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/state/writer.tos\t%s/writer.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
printf '/system/state/reader.tos\t%s/reader.tos\n' "$FIXTURE" >> "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/path.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/path.bin" --manifest "$OUT/capsule.meta.json"

printf '/system/boot/init.tos\t%s/small.tos\n' "$FIXTURE" > "$OUT/small-manifest.txt"
printf '/system/service/block.tos\t%s/block.tos\n' "$FIXTURE" >> "$OUT/small-manifest.txt"
printf '/system/state/initializer.tos\t%s/initializer.tos\n' "$FIXTURE" >> "$OUT/small-manifest.txt"
printf '/system/state/store.tos\t%s/state.tos\n' "$FIXTURE" >> "$OUT/small-manifest.txt"
printf '/system/state/probe.tos\t%s/probe.tos\n' "$FIXTURE" >> "$OUT/small-manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/small.bin" --meta "$OUT/small.meta.json" "$OUT/small-manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/small.bin" --manifest "$OUT/small.meta.json"

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
completed=$(grep '^TOS\.RUN\.COMPLETED value=' "$LOG" | sed 's/^TOS\.RUN\.COMPLETED value=//')

# --- 0: every module reported a success account --------------------------------
#
# **Checked before any count, because it is the most specific thing this boot can say.**
# Every module here returns a bitmask on success and a negative naming the step that did not
# hold, so a negative is the report of whichever layer noticed — the writer saying its `PUT`
# was refused, the reader saying a byte did not match or that a refusal was not the one it
# should be, the store saying a lower operation never completed. A tally that moved as a
# *consequence* of that is a weaker thing to read first.
for value in $completed; do
    case "${value#i64:}" in
        -*) fail "a module reported the failure $value; the boot reported:
       $(printf '%s ' $completed)" ;;
    esac
done

# --- 0b: formatting happened once, and the second attempt was refused -----------
#
# §4.4 makes only an all-zero sector 0 permission to format, and the second run of the same
# module sees a header with two occupancy bits set. The two runs must therefore report two
# different accounts, and neither may be the other's.
formatted="$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_FORMATTED\$")"
[ "$formatted" = 1 ] ||
    fail "$formatted of the two initializer runs formatted the device; exactly one may —
       only an all-zero sector 0 is permission to format (STATE_STORE_V1 §4.4)"
refused_to_format="$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_REFUSED_TO_FORMAT\$")"
[ "$refused_to_format" = 1 ] ||
    fail "the re-provisioning probe did not refuse to format: the same module against a
       header with occupancy bits set must refuse, and the store must survive it
       (STATE_STORE_V1 §4.4, §12's negative)"

# --- six modules, eight processes, and nothing stalled --------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=6$' "$LOG" ||
    fail "the boot did not run a set of six modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 8 ] ||
    fail "eight processes did not begin: $(count '^TOS\.RUN\.BEGIN path=') did"
grep -q '^TOS\.RUN\.LIVENESS .*verdict=stalled' "$LOG" &&
    fail "the boot stalled: something was waiting for a message nobody could deliver"
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_create_funded status=0')" = 7 ] ||
    fail "seven children were not created"
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_wait_child status=0$')" = 7 ] ||
    fail "seven endings were not collected"

# --- 1: the capability topology, which is the store's isolation boundary --------
#
# **Read from the journal, not from the source** (`STATE_STORE_V1` §2). A client of the store
# cannot address a sector, and what says so is which process resolved which binding.
python3 - "$LOG" 7 4 6 2 2 3 <<'TOPOLOGY' || fail "the boot's capability topology is not the one this store requires"
import sys
from collections import Counter

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
budgets, blocks, inboxes, stores, buses, serves = (int(a) for a in sys.argv[2:8])
PREFIX = "TOS.RUN.REQUEST binding="

seen = Counter()
for line in events:
    if not line.startswith(PREFIX):
        continue
    rest = line[len(PREFIX):].split(" ")
    seen[(rest[0], rest[1].removeprefix("interface="))] += 1

# What each name is and how many processes hold one.
#
#   the supervisor's own, each once: it is the only process the launcher endows
#   budget      every child
#   serve       the block service and each store generation
#   block       block.device.v1's client endpoint: the initializer runs and the store
#               generations, and **nothing else in the boot**
#   inbox       the initializer runs, the store generations and the clients
#   store       state.store.v1's client endpoint: clients only
#   device      the platform bus: the supervisor and the block service, and nothing else
expected = {
    ("process", "system.process.Control"): 1,
    ("memory", "system.memory.Authority"): 1,
    ("block_serve_full", "system.ipc.Endpoint"): 1,
    ("state_serve_full", "system.ipc.Endpoint"): 1,
    ("state_inbox_full", "system.ipc.Endpoint"): 1,
    ("client_inbox_full", "system.ipc.Endpoint"): 1,
    ("init_inbox_full", "system.ipc.Endpoint"): 1,
    ("budget", "system.memory.Authority"): budgets,
    ("serve", "system.ipc.Endpoint"): serves,
    ("block", "system.ipc.Endpoint"): blocks,
    ("inbox", "system.ipc.Endpoint"): inboxes,
    ("store", "system.ipc.Endpoint"): stores,
    ("device", "platform.pci.Bus"): buses,
}
if dict(seen) != expected:
    missing = {k: v for k, v in expected.items() if seen.get(k) != v}
    extra = {k: v for k, v in seen.items() if expected.get(k) != v}
    print(f"expected {expected}", file=sys.stderr)
    print(f"wrong: {missing}; unexpected: {extra}", file=sys.stderr)
    raise SystemExit(1)

# **No process reached any other part of the machine.** A store that mapped a window, an
# interrupt or a DMA region of its own would show here, and so would a client.
for never in ("platform.pci.FunctionConfig", "platform.irq.Source", "platform.dma.Region",
              "platform.mmio.Region"):
    if any(f"interface={never} " in line for line in events if line.startswith(PREFIX)):
        print(f"{never} was requested by name, and only the block service reaches the "
              f"device in this boot", file=sys.stderr)
        raise SystemExit(1)
TOPOLOGY

# --- 1b: the endowment accounting ADR-0101 §6 corrected -------------------------
#
# **Counted by the launcher and the nucleus, not claimed by a module.** `MAX_ENDOWMENT` is
# four; the supervisor is endowed by the launcher and is not a plan.
python3 - "$LOG" 1 2 5 <<'ENDOWMENT' || fail "the endowment accounting is not ADR-0099 §13a's corrected one"
import sys
from collections import Counter

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
supervisors, fours, threes = (int(a) for a in sys.argv[2:5])
PREFIX = "TOS.RUN.PROCESS_ENDOWED "

sizes = Counter()
for line in events:
    if not line.startswith(PREFIX):
        continue
    for field in line.split(" "):
        if field.startswith("capabilities="):
            sizes[int(field.removeprefix("capabilities="))] += 1

#   8   the supervisor: five endpoints, the bus, its own control and the root remainder
#   4   each generation of the store, which is MAX_ENDOWMENT
#   3   the block service, each initializer run and each client
expected = {8: supervisors, 4: fours, 3: threes}
if dict(sizes) != expected:
    print(f"expected endowment sizes {expected}, saw {dict(sizes)}", file=sys.stderr)
    raise SystemExit(1)
ENDOWMENT

# --- 2: exactly the requests the layers above make -----------------------------
messages=$((BLOCK_REQUESTS + 2 * STORE_REQUESTS))
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call_region status=0$')" = "$messages" ] ||
    fail "the two services did not receive exactly $messages messages between them:
       $BLOCK_REQUESTS to the block service and $STORE_REQUESTS to each generation of the
       store"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')" = "$messages" ] ||
    fail "not every request was answered; a store that dropped one would leave its caller
       blocked until the liveness rule cancelled it"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=-[0-9]*$')" = 0 ] ||
    fail "a reply was refused"
# The corrected carried-call row (ADR-0101), at both layers: five lower `READ`s — the two
# initializer runs, both `open`s and object 2 — and three of the reader's five requests.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_carrying status=0$')" = 8 ] ||
    fail "expected eight carried calls: five lower READs and the reader's three"
# Four lower `CAPACITY` calls — one per opening and one per initializer run, which is the
# correction that makes "the device is too small" a reason the store is not open — plus the
# writer's three refusal probes and the reader's one.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word status=0$')" = 8 ] ||
    fail "expected eight word-only calls: $CAPACITY_REQUESTS lower capacity calls and four
       client refusal probes"
# The atomic call-with-region row: one header write by the initializer, four writes by
# generation A, and the writer's two `PUT`s.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_region status=0$')" = 7 ] ||
    fail "expected seven atomic calls carrying a region"
# The one row that lets a client choose an inline length, used once: the malformed probe.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call status=0$')" = 1 ] ||
    fail "expected exactly one call with a length of the caller's choosing, the malformed
       probe — and it must be answered"
# **One region received per lower answer, and one forwarded per served GET.** The block
# service sends five — one per lower `READ` — and the store forwards exactly one.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$')" = 6 ] ||
    fail "expected six region sends: five lower answers and one forwarded object"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_region status=0$')" = 6 ] ||
    fail "expected six regions received: five lower answers and the reader's object"
# **And this is how every early refusal is proved not to have touched the device.** Six
# requests in this boot are refused above the block service — a reserved opcode, two ids out
# of range, a length that is not eight, a `PUT` with no region, a `GET` with no answer
# endpoint — plus an absent `GET`. Not one of them may add a lower request.
resolved="$(count '^TOS\.RUN\.INTERFACE operation=dma_device_address status=0$')"
[ "$resolved" = "$((RING_ADDRESSES + RING_REQUESTS * ADDRESSES_PER_REQUEST))" ] ||
    fail "the boot resolved $resolved device addresses; one ring and $RING_REQUESTS ring
       requests resolve $((RING_ADDRESSES + RING_REQUESTS * ADDRESSES_PER_REQUEST)) — a
       store that read an absent id's sector, or that let a refused request through, would
       resolve three more"

# --- 2b: and each of the fourteen exchanges touched what it should ---------------
#
# **The journal sliced by a service's own receives**, as `block-protocol` does it.
# `dma_device_address` is the nucleus's own line, emitted only from the submission path,
# three per request — so it binds to submitting rather than to deciding, and no module could
# write it.
python3 - "$LOG" "$messages" "$RING_REQUESTS" "$ADDRESSES_PER_REQUEST" <<'WINDOWS' || fail "the device requests are not the ones the layers above this store make"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
messages, ring_requests, per_request = (int(a) for a in sys.argv[2:5])
RECEIVE = "TOS.RUN.INTERFACE operation=endpoint_receive_call_region status="
ADDRESS = "TOS.RUN.INTERFACE operation=dma_device_address status=0"

opened = [i for i, line in enumerate(events) if line.startswith(RECEIVE)]
if len(opened) != messages:
    print(f"expected {messages} received messages, saw {len(opened)}", file=sys.stderr)
    raise SystemExit(1)

# Every window either reaches the device with exactly one request's worth of resolved
# addresses, or does not reach it at all. A window with two requests in it would be a service
# that had batched something nobody asked it to.
bounds = opened + [len(events)]
touched = 0
for index in range(len(opened)):
    window = events[bounds[index] + 1:bounds[index + 1]]
    addresses = window.count(ADDRESS)
    if addresses == 0:
        continue
    if addresses != per_request:
        print(f"an exchange resolved {addresses} device addresses, not {per_request}",
              file=sys.stderr)
        raise SystemExit(1)
    touched += 1
if touched != ring_requests:
    print(f"expected {ring_requests} exchanges to reach the device, saw {touched}",
          file=sys.stderr)
    raise SystemExit(1)
WINDOWS

# --- 3: nothing a request handed over is still held ----------------------------
#
# Derived, so a number that moved would have to be explained:
#
#   supervisor          7 child controls, and the 5 endpoint names it let go before
#                       creating the children that receive on them
#   initializer, each   1 send-only alias and 1 header region                   2 + 2
#   block service       1 payload region per write and 1 answer endpoint per
#                       read                                                    5 + 5
#   generation A        the alias and the region of its one `open`; its three refusals and
#                       its two `PUT`s hand over nothing it keeps                     2
#   generation B        the same, plus the answer endpoint of the id-out-of-range refusal,
#                       plus the absent refusal's, plus the alias and answer endpoint of
#                       the served `GET`                                              6
#   writer              nothing: a `PUT` carries a region and no channel                0
#   reader              3 aliases and the object region it was given                   4
#
# **A `PUT` releases nothing**, and that is the one-copy rule visible as a count: the
# client's region is forwarded to the device rather than copied and released here.
SUPERVISOR_RELEASES=$((7 + 5))
INITIALIZER_RELEASES=$((2 + 2))
BLOCK_RELEASES=$((5 + 5))
STATE_RELEASES=$((2 + 6))
READER_RELEASES=4
EXPECTED_RELEASES=$((SUPERVISOR_RELEASES + INITIALIZER_RELEASES + BLOCK_RELEASES +
    STATE_RELEASES + READER_RELEASES))
releases="$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')"
[ "$releases" = "$EXPECTED_RELEASES" ] ||
    fail "the boot released $releases capabilities; this topology releases
       $EXPECTED_RELEASES"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=-[0-9]*$')" = 0 ] ||
    fail "a release was refused, so something was held that could not be let go"
# One send-only alias per carried call, and none refused: asking for a right a capability
# lacks is an intersection, not an error (ADR-0100, `CAPABILITY_V1` §4).
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=0$')" = 8 ] ||
    fail "expected eight send-only aliases, one per carried call"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=-[0-9]*$')" = 0 ] ||
    fail "an attenuation was refused"

# --- 4: generation A was collected before generation B existed -----------------
#
# **§13a's ordering, and it is this supervisor's policy rather than a nucleus guarantee.**
# The nucleus refuses to create B from the shared plan while A is *live* — `IPC_V1` §2 admits
# one receive-rights holder — but `process::retire` clears A's capabilities at retirement,
# before `wait_child` collects its tombstone, so nothing below makes collection the only
# possible order. The persistence proof needs A's address space and capabilities gone before
# B exists, so the supervisor collects and this asserts it.
python3 - "$LOG" "$EXPECTED_STATE_A" <<'ORDER' || fail "generation B was created before generation A had been collected"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
account = f"TOS.RUN.COMPLETED value={sys.argv[2]}"
CREATED_STORE = "TOS.RUN.INTERFACE operation=process_create_funded status=0 said=system/state/store.tos"
EXIT = "TOS.RUN.PROCESS_EXIT process="
COLLECTED = "TOS.RUN.INTERFACE operation=process_wait_child status=0"

reported = [i for i, line in enumerate(events) if line == account]
if len(reported) != 1:
    print(f"generation A reported its account {len(reported)} times", file=sys.stderr)
    raise SystemExit(1)
created = [i for i, line in enumerate(events) if line == CREATED_STORE]
if len(created) != 2:
    print(f"the store plan was used {len(created)} times, not twice", file=sys.stderr)
    raise SystemExit(1)

first, second = created
if not first < reported[0] < second:
    print("generation A's account is not between the two creations of the store plan",
          file=sys.stderr)
    raise SystemExit(1)

# Between A reporting and B being created: A, the writer and the re-provisioning probe are
# the only children that can end — the block service still owes the requests of everything
# after — so three retirements and three collections must be here.
window = events[reported[0]:second]
retired = sum(1 for line in window if line.startswith(EXIT))
collected = sum(1 for line in window if line == COLLECTED)
if retired != 3 or collected != 3:
    print(f"between A's account and B's creation the journal shows {retired} retirement(s) "
          f"and {collected} collection(s); it must show three of each — generation A, the "
          f"writer and the re-provisioning probe, all gone and all collected",
          file=sys.stderr)
    raise SystemExit(1)

last = max(i for i, line in enumerate(window) if line == COLLECTED)
if not reported[0] + last < second:
    print("the last collection does not precede B's creation", file=sys.stderr)
    raise SystemExit(1)
ORDER

# --- 5: the re-provisioning probe ran with no state service alive ---------------
#
# **Which is what makes it a probe and not a race.** §12's negative is about an initializer
# meeting a populated store; if a store had been serving at the time, the boot would also
# have been testing what happens to a service whose device is reformatted underneath it, and
# a red gate could have meant either.
python3 - "$LOG" <<'ALONE' || fail "the re-provisioning probe did not run alone"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
CREATED = "TOS.RUN.INTERFACE operation=process_create_funded status=0 said="
INITIALIZER = CREATED + "system/state/initializer.tos"
STORE = CREATED + "system/state/store.tos"
EXIT = "TOS.RUN.PROCESS_EXIT process="

runs = [i for i, line in enumerate(events) if line == INITIALIZER]
stores = [i for i, line in enumerate(events) if line == STORE]
if len(runs) != 2 or len(stores) != 2:
    print(f"expected two initializer runs and two store generations, saw {len(runs)} and "
          f"{len(stores)}", file=sys.stderr)
    raise SystemExit(1)

probe = runs[1]
# The probe is created after generation A was created, and before generation B — so A is the
# only store that could still have been alive, and it must already have ended.
if not stores[0] < probe < stores[1]:
    print("the second initializer run is not between the two store generations",
          file=sys.stderr)
    raise SystemExit(1)
if not any(line.startswith(EXIT) for line in events[stores[0]:probe]):
    print("no process had ended between generation A's creation and the probe's, so a "
          "store may still have been serving when the probe ran", file=sys.stderr)
    raise SystemExit(1)
ALONE

# --- 6: control before data, at both layers ------------------------------------
#
# **`STATE_STORE_V1` §8b and `BLOCK_DEVICE_V1` §6a are the same rule at two heights**, and
# the tail of this journal is where both are read. What follows the boot's last completion
# wait is the block service answering generation B's read of object 2 and the store answering
# the reader, and nothing else — so the sequence is exactly six steps.
#
# A reply means "the read succeeded and exactly one region is owed". The reverse order admits
# an orphan region: an endpoint object lives for the boot, so a region queued on an answer
# endpoint whose waiter then died stays there, and a later holder of receive — including a
# successor created from the same launch plan — could take it as the answer to its own
# request.
python3 - "$LOG" <<'LAYERS' || fail "a reply did not precede its region send"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
WAIT = "TOS.RUN.INTERFACE operation=irq_wait status=0"
STEPS = {
    "TOS.RUN.INTERFACE operation=endpoint_reply_word status=0": "reply",
    "TOS.RUN.INTERFACE operation=endpoint_send_region status=0": "send",
    "TOS.RUN.INTERFACE operation=endpoint_receive_region status=0": "receive",
}

last = max((i for i, line in enumerate(events) if line == WAIT), default=None)
if last is None:
    print("the boot never waited on the device", file=sys.stderr)
    raise SystemExit(1)

seen = [STEPS[line] for line in events[last + 1:] if line in STEPS]
wanted = ["reply", "send", "receive", "reply", "send", "receive"]
if seen != wanted:
    print(f"expected {wanted} after the last completion wait, saw {seen}", file=sys.stderr)
    raise SystemExit(1)
LAYERS

# --- 7: the accounts ----------------------------------------------------------
for want in "$EXPECTED_SUPERVISOR" "$EXPECTED_BLOCK" "$EXPECTED_FORMATTED" \
    "$EXPECTED_REFUSED_TO_FORMAT" "$EXPECTED_STATE_A" "$EXPECTED_STATE_B" \
    "$EXPECTED_WRITER" "$EXPECTED_READER"; do
    [ "$(printf '%s\n' "$completed" | grep -c "^$want$")" = 1 ] ||
        fail "no module reported $want exactly once; the boot reported:
       $(printf '%s ' $completed)"
done
[ "$(printf '%s\n' "$completed" | wc -l)" = 8 ] ||
    fail "eight accounts were not reported"

# --- 8: the same store against a device too small to hold it -------------------
#
# **§9's other reason, and the correction this boot exists for.** A store that validated
# sector 0 and stopped would open on a device that cannot hold the sectors the header
# describes. So the ordinary service asks the layer below for the capacity **first**, and on a
# 64-sector device it never reads sector 0 at all — which the three resolved device addresses
# below, the ring's own and no request's, state exactly.
bash "$HERE/run.sh" \
    --out "$OUT/small" \
    --capsule "$OUT/small.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --stage4-block-sectors 64 \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null

SMALL="$OUT/small/events.log"
small_count() { grep -c "$1" "$SMALL" || true; }
small_completed=$(grep '^TOS\.RUN\.COMPLETED value=' "$SMALL" | sed 's/^TOS\.RUN\.COMPLETED value=//')

for value in $small_completed; do
    case "${value#i64:}" in
        -*) fail "a module of the too-small boot reported the failure $value; it reported:
       $(printf '%s ' $small_completed)" ;;
    esac
done
for want in "$EXPECTED_SMALL_SUPERVISOR" "$EXPECTED_SMALL_INITIALIZER" \
    "$EXPECTED_SMALL_STATE" "$EXPECTED_PROBE" "$EXPECTED_SMALL_BLOCK"; do
    [ "$(printf '%s\n' "$small_completed" | grep -c "^$want$")" = 1 ] ||
        fail "the too-small boot did not report $want exactly once; it reported:
       $(printf '%s ' $small_completed)"
done
[ "$(printf '%s\n' "$small_completed" | wc -l)" = 5 ] ||
    fail "the too-small boot did not report five accounts"

# **Sector 0 was never read.** Three resolved device addresses is the ring's own setup and
# nothing else: not one request reached the ring, because the only two block requests of the
# whole boot are `CAPACITY` calls answered from the configuration space.
small_resolved="$(small_count '^TOS\.RUN\.INTERFACE operation=dma_device_address status=0$')"
[ "$small_resolved" = "$RING_ADDRESSES" ] ||
    fail "the too-small boot resolved $small_resolved device addresses; it must resolve
       exactly the ring's own $RING_ADDRESSES — a store that read sector 0 of a device it
       will not use would resolve three more"
[ "$(small_count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_region status=0$')" = 0 ] ||
    fail "a region was received in the too-small boot; nothing there reads a sector"
[ "$(small_count '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$')" = 0 ] ||
    fail "a region was sent in the too-small boot"
# Two lower `CAPACITY` calls — the initializer's and the store's — and the probe's four
# word-only requests.
[ "$(small_count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word status=0$')" = 6 ] ||
    fail "expected six word-only calls in the too-small boot: two lower capacity calls and
       the probe's four"
# Seven messages received and seven answered: two by the block service and five by the store,
# every one of the store's a refusal.
[ "$(small_count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')" = 7 ] ||
    fail "the too-small boot did not answer every request"
# **One census, and it is the mechanism rather than a fault.** Once the initializer, the
# store and the probe have ended, no live process holds a name the block service can be sent
# on — so the receive it is blocked on is one nothing can satisfy, and `SYSTEM_ABI_V1` §6 ends
# it. The service reads that for what it is and exits; the supervisor's own wait is ended in
# the same census and is made again.
[ "$(small_count '^TOS\.RUN\.LIVENESS .*verdict=stalled')" = 1 ] ||
    fail "the too-small boot did not end with exactly one liveness census; that census is how
       the block service learns its clients are gone"
[ "$(small_count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call_region status=-5$')" = 1 ] ||
    fail "the block service's receive was not cancelled exactly once"
[ "$(small_count '^TOS\.RUN\.INTERFACE operation=process_wait_child status=0$')" = 4 ] ||
    fail "the too-small boot did not collect four endings"

echo "state-store: PASS: a persistent object store, read back by a process that came after"
echo "  state.store.v1 (ADR-0099) over block.device.v1 (ADR-0098) over the reference"
echo "  VirtIO device, every layer canonical text and no host path anywhere"
echo "  an initializer formats a zeroed device once, and the same module from the same"
echo "  sealed plan **refuses** when it meets that header again with two occupancy bits"
echo "  set — the adversarial re-provisioning probe, run with no state service alive"
echo "  a writer puts two objects and ends; its ending is collected"
echo "  generation A ends, is retired and is collected **before** generation B exists,"
echo "  which is this supervisor's policy and is asserted on the journal"
echo "  generation B re-reads and validates the header from the device, having been"
echo "  handed nothing but a sealed plan"
echo "  a reader gets object 2 and checks all 512 bytes in canonical text — so the"
echo "  bytes came off the device, and the probe did not reset the occupancy that"
echo "  says the object is there"
echo "  every refusal STATE_STORE_V1 §9 defines is exercised: ST_OPCODE, ST_ID at both"
echo "  ends of the range, ST_NO_REGION, ST_NO_ANSWER and ST_ABSENT by exact reply"
echo "  word; ST_MALFORMED by an answered call plus the store's own account bit,"
echo "  because endpoint_call produces a status and not an answer; and ST_STORE five"
echo "  times over, in a second boot against a device one sector too small"
echo "  a valid header is not enough to be open: the store asks the layer below for the"
echo "  capacity first, and on a 64-sector device it never reads sector 0 at all"
echo "  none of those refusals reaches the device, which the fourteen counted device"
echo "  requests of the first boot and the three of the second say exactly"
echo "  control before data at both layers, asserted as one six-step sequence"
echo "  an incomplete lower operation is **not** a refusal: the store keeps three"
echo "  classes apart, and only \"the block layer answered and refused\" becomes"
echo "  ST_BLOCK — mutations 8 and 9 turn a fabricated one red"
echo "  the store's isolation is capability topology read from the journal: the bus is"
echo "  held by the supervisor and the block service, block.device.v1's endpoint by the"
echo "  initializer runs and the store generations, and a client holds neither"
echo "  five endpoints of six and four endowments at most, ADR-0101 §6's corrected"
echo "  accounting, counted by the launcher rather than claimed by a module"
echo "  NOT exercised: ST_BLOCK, because the conforming reference endpoint answers every"
echo "  well-formed in-range request with VIRTIO_BLK_S_OK and no device failure is"
echo "  manufactured to colour the row green"
echo "  NOT claimed: power-loss durability, VIRTIO_BLK_F_FLUSH, crash consistency,"
echo "  journaling, transactions, exactly-once PUT, delete, enumeration, a second owner"
echo "  or store, docs/09's /state namespace, any path semantics, or Stage 4 closure"
