#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A persistent object store, and a reader that gets back what a writer that has ended
# wrote.
#
# **ADR-0099 and `STATE_STORE_V1`.** Stage 4 owes *"persistent object/state storage"*
# with the engineering exit *"persistent storage works through a textual user-space
# driver"*. This boot is that, on the reference VirtIO device, with every layer canonical
# text: a block service speaking the accepted `block.device.v1`, a store speaking the
# accepted `state.store.v1` above it, and clients that hold no way to address a sector.
#
#   block service            created first, ends last; the only holder of the bus
#   initializer, run 1       capacity >= 65; sector 0 is all zero; one header written
#   initializer, run 2       the same text against that header: it **refuses to format**
#   state store A            opens and validates the header from the device
#   writer                   put(1, pattern A); put(2, pattern B); ends
#   A ends, is retired, and **both** endings are collected before B is created
#   state store B            the same module from the same sealed plan; re-reads and
#                            validates the header from the device
#   reader                   get(3) refused as absent; get(2) verified byte by byte
#
# **What makes this a persistence claim rather than a message-passing one.** The writer
# has ended and been collected. Generation A has ended, been retired — `process::retire`
# clears its capabilities — and had its ending collected. Generation B is a different
# process that was handed nothing but a sealed plan. So the 512 bytes the reader checks
# came off the device, and the header B validated is the one on the device.
#
# **Claims this gate is built around.**
#
# *Only an all-zero sector 0 is permission to format* (§4.4). The second initializer run
# is the negative, and it is a second **process** rather than a second code path: the same
# module from the same plan, differing only in what it finds. That the header is unchanged
# afterwards is then proved twice over — A opens it, and the reader gets an object it
# says is present.
#
# *Presence is `occupancy` and nothing else* (§5). `get(3)` is an id never created, and
# the harness seeds sector 3 with 512 copies of `0xC3` — so a store that inferred presence
# from bytes would answer it with that fill. The refusal is required to be exactly
# `REFUSED + ST_ABSENT`, and the twelve device requests are counted: a store that read an
# absent id's sector would make a thirteenth.
#
# *Control before data, at both layers* (§8b, `BLOCK_DEVICE_V1` §6a). The tail of the
# journal after the last completion wait is one six-step chain — the block service replies
# and then sends, the store takes that region, replies, and then sends — and it is
# asserted as a sequence, so a send before a reply at **either** layer turns it red.
#
# *The store's isolation boundary is capability topology*, read from the journal and not
# from source: the bus is requested exactly twice, by the supervisor and the block
# service; `block.device.v1`'s endpoint exactly four times, by the two initializer runs
# and the two store generations; and the store's own endpoint exactly twice by clients.
#
# *And the corrected endowment accounting is read off the journal too* (ADR-0101 §6):
# five endpoints of six, the supervisor endowed eight, the block service three, each
# initializer run **three** — it needs an inbox of its own, because it reads the header
# before deciding whether to write one — each store generation four, and each client
# three.
#
# **The seven mutations `ADR-0099` §13 requires**, each verified to turn this gate red on
# its own assertion:
#
#   1 omit the initializer's header write         -> §0: two runs formatted, because the
#                                                    second found zeros too — and the
#                                                    writer's PUT was refused ST_STORE
#   2 omit the payload device write               -> the reader reports i64:-8, its byte
#                                                    witness
#   3 answer get(2) from object 1's sector        -> the reader reports i64:-8
#   4 ignore the occupancy bitmap                 -> the reader reports i64:-3: get(3) was
#                                                    answered instead of refused
#   5 omit PUT(2)'s occupancy-header update       -> the reader reports i64:-6: the
#                                                    successor read the persisted header
#                                                    and object 2 is absent
#   6 send GET's region before replying success   -> §5's sequence, which sees
#                                                    reply/send/receive/**send/reply**/receive
#   7 remove the initializer's zero-header check  -> §0 again, and that is not a weakness:
#                                                    1 and 7 are both claims about
#                                                    formatting, and the claim that fails
#                                                    is the same one
#
# **Mutations 2 and 3 share an assertion on purpose.** `ADR-0099` §13 names both as the
# byte witness, because what each breaks is the same statement — that the 512 bytes the
# reader verifies are object 2's, written by a process that has ended.
#
# **Not claimed:** power-loss durability, `VIRTIO_BLK_F_FLUSH`, crash consistency,
# journaling, transactions, exactly-once `PUT`, delete, enumeration, more than one owner
# or store, `docs/09`'s `/state` namespace, any filesystem or path semantics, or Stage 4
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

# --- the accounts, one distinct value each -------------------------------------
# **Distinct on purpose**, and each module's bits are its own range: eight processes
# reporting overlapping numbers would let a gate counting one of them be satisfied by
# another, and this boot's whole subject is what one process observed after another had
# ended.
#
# The supervisor: seven children created in five phases, and every one of their endings
# collected as a clean exit. Bits 1..1024.
EXPECTED_SUPERVISOR="i64:2047"
# The block service: Stage 4D's twenty device-side facts, the device configuration found,
# its capacity stable under §2.5.1's generation protocol, and capacity, a write and a read
# served. **And not one refusal bit**, which is its own statement: nothing above it ever
# sent a malformed, out-of-range or ill-formed request.
BLOCK_DEVICE_ALL=1048575
BLOCK_SERVED=$(( (1 << 25) - (1 << 20) ))
EXPECTED_BLOCK="i64:$((BLOCK_DEVICE_ALL + BLOCK_SERVED))"
# The initializer, twice: formatted, then refused to format. Bits 1..16.
EXPECTED_FORMATTED="i64:15"
EXPECTED_REFUSED_TO_FORMAT="i64:19"
# The store, twice. Bits 64..1024: opened and every request answered, plus two objects
# created for A, plus an absent id refused and one object served for B.
EXPECTED_STATE_A="i64:448"
EXPECTED_STATE_B="i64:1728"
# The writer and the reader. Bits 4096..131072.
EXPECTED_WRITER="i64:12288"
EXPECTED_READER="i64:245760"

# --- what the journal must count ----------------------------------------------
# The block service's twelve requests, derived rather than chosen:
#
#   initializer run 1   capacity, READ 0, WRITE 0                            3
#   initializer run 2   capacity, READ 0 — then it refuses                   2
#   state A             open; put(1) and put(2), each a payload sector and
#                       then the header                                      5
#   state B             open; one READ of object 2                           2
BLOCK_REQUESTS=12
# Two of them are `CAPACITY`, which is answered from the device's configuration space and
# reaches no ring; the other ten are ring requests at three resolved addresses each, over
# and above the three the ring's own setup resolves.
CAPACITY_REQUESTS=2
RING_REQUESTS=$((BLOCK_REQUESTS - CAPACITY_REQUESTS))
RING_ADDRESSES=3
ADDRESSES_PER_REQUEST=3

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

# --- 0: formatting happened once, and the second attempt was refused -----------
#
# **The boot's precondition, and checked before anything else.** `STATE_STORE_V1` §4.4
# makes only an all-zero sector 0 permission to format, and an initializer that wrote
# unconditionally would silently reset `occupancy` against an existing store — losing every
# object in it while reporting success. So the two runs of the same module must report two
# different accounts: one that formatted and one that refused.
#
# **Both formatting mutations land here, and that is right.** Omitting the header write
# leaves the second run finding zeros and formatting too; removing the zero-header check
# leaves it formatting over a valid header. Either way exactly one of the two accounts is
# missing, and the claim that failed is the same claim.
formatted="$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_FORMATTED\$")"
[ "$formatted" = 1 ] ||
    fail "$formatted of the two initializer runs formatted the device; exactly one may —
       only an all-zero sector 0 is permission to format (STATE_STORE_V1 §4.4)"
refused_to_format="$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_REFUSED_TO_FORMAT\$")"
[ "$refused_to_format" = 1 ] ||
    fail "the second initializer run did not refuse to format: the same module against the
       header the first wrote must refuse, because an already valid header is not
       permission (STATE_STORE_V1 §4.4, §12's negative)"

# --- 0b: every module reported a success account -------------------------------
#
# **Checked before any count, because it is the most specific thing this boot can say.**
# Every module here returns a bitmask on success and a negative naming the step that did
# not hold, so a negative is the report of whichever layer noticed — the writer saying its
# `PUT` was refused, the reader saying a byte did not match, the store saying a lower
# operation failed. A tally that moved as a *consequence* of that is a weaker thing to
# read first.
for value in $completed; do
    case "${value#i64:}" in
        -*) fail "a module reported the failure $value; the boot reported:
       $(printf '%s ' $completed)" ;;
    esac
done

# --- six modules, eight processes, and nothing stalled --------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=6$' "$LOG" ||
    fail "the boot did not run a set of six modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 8 ] ||
    fail "eight processes did not begin: $(count '^TOS\.RUN\.BEGIN path=') did"
grep -q '^TOS\.RUN\.LIVENESS .*verdict=stalled' "$LOG" &&
    fail "the boot stalled: something was waiting for a message nobody could deliver"
# Seven children created and seven endings collected. The supervisor's account says it
# checked each ending's kind and status; this says it collected as many as it created.
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_create_funded status=0')" = 7 ] ||
    fail "seven children were not created"
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_wait_child status=0$')" = 7 ] ||
    fail "seven endings were not collected"

# --- 1: the capability topology, which is the store's isolation boundary --------
#
# **Read from the journal, not from the source** (`STATE_STORE_V1` §2). A client of the
# store cannot address a sector, and what says so is which process resolved which binding.
python3 - "$LOG" <<'TOPOLOGY' || fail "the boot's capability topology is not the one this store requires"
import sys
from collections import Counter

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
PREFIX = "TOS.RUN.REQUEST binding="

seen = Counter()
for line in events:
    if not line.startswith(PREFIX):
        continue
    rest = line[len(PREFIX):].split(" ")
    seen[(rest[0], rest[1].removeprefix("interface="))] += 1

# What each name is and how many processes hold one.
#
#   the supervisor's eight, each once: it is the only process the launcher endows
#   budget      every one of the seven children
#   serve       the block service and the two store generations
#   block       block.device.v1's client endpoint: the two initializer runs and the two
#               store generations, and **nothing else in the boot**
#   inbox       the two initializer runs, the two store generations, and the two clients
#   store       state.store.v1's client endpoint: the writer and the reader
#   device      the platform bus: the supervisor and the block service, and nothing else
expected = {
    ("process", "system.process.Control"): 1,
    ("memory", "system.memory.Authority"): 1,
    ("block_serve_full", "system.ipc.Endpoint"): 1,
    ("state_serve_full", "system.ipc.Endpoint"): 1,
    ("state_inbox_full", "system.ipc.Endpoint"): 1,
    ("client_inbox_full", "system.ipc.Endpoint"): 1,
    ("init_inbox_full", "system.ipc.Endpoint"): 1,
    ("budget", "system.memory.Authority"): 7,
    ("serve", "system.ipc.Endpoint"): 3,
    ("block", "system.ipc.Endpoint"): 4,
    ("inbox", "system.ipc.Endpoint"): 6,
    ("store", "system.ipc.Endpoint"): 2,
    ("device", "platform.pci.Bus"): 2,
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
# **Counted by the launcher and the nucleus, not claimed by a module.** `MAX_ENDOWMENT`
# is four; the supervisor is endowed by the launcher and is not a plan.
python3 - "$LOG" <<'ENDOWMENT' || fail "the endowment accounting is not ADR-0099 §13a's corrected one"
import sys
from collections import Counter

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
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
#   3   the block service, each initializer run, the writer and the reader
expected = {8: 1, 4: 2, 3: 5}
if dict(sizes) != expected:
    print(f"expected endowment sizes {expected}, saw {dict(sizes)}", file=sys.stderr)
    raise SystemExit(1)
ENDOWMENT

# --- 2: exactly the device requests the layers above make ----------------------
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call_region status=0$')" = 16 ] ||
    fail "the two services did not receive exactly sixteen messages between them:
       $BLOCK_REQUESTS to the block service and 2 to each generation of the store"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')" = 16 ] ||
    fail "not every request was answered; a store that dropped one would leave its
       caller blocked until the liveness rule cancelled it"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=-[0-9]*$')" = 0 ] ||
    fail "a reply was refused"
# The corrected carried-call row (ADR-0101), at both layers: five lower `READ`s — the two
# initializer runs, both `open`s and object 2 — and the reader's two `GET`s.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_carrying status=0$')" = 7 ] ||
    fail "expected seven carried calls: five lower READs and two GETs"
# The two initializer `capacity()` calls, and nothing else uses that row.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word status=0$')" = 2 ] ||
    fail "expected exactly two capacity calls, one per initializer run"
# The atomic call-with-region row: one header write by the initializer, four writes by
# generation A, and the writer's two `PUT`s.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_word_region status=0$')" = 7 ] ||
    fail "expected seven atomic calls carrying a region"
# **One region received per lower answer, and one forwarded per served GET.** The block
# service sends five — one per lower `READ` — and the store forwards exactly one.
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$')" = 6 ] ||
    fail "expected six region sends: five lower answers and one forwarded object"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_region status=0$')" = 6 ] ||
    fail "expected six regions received: five lower answers and the reader's object"
# **Which is also how `get(3)` is proved not to have read a sector.** An absent id
# refused after reading its sector would be a sixth lower READ and a seventh region.
resolved="$(count '^TOS\.RUN\.INTERFACE operation=dma_device_address status=0$')"
[ "$resolved" = "$((RING_ADDRESSES + RING_REQUESTS * ADDRESSES_PER_REQUEST))" ] ||
    fail "the boot resolved $resolved device addresses; one ring and $RING_REQUESTS ring
       requests resolve $((RING_ADDRESSES + RING_REQUESTS * ADDRESSES_PER_REQUEST)) —
       a store that read an absent id's sector would resolve three more"

# --- 2b: and each of the twelve exchanges touched what it should ----------------
#
# **The journal sliced by the block service's own receives**, as `block-protocol` does it.
# `dma_device_address` is the nucleus's own line, emitted only from the submission path,
# three per request — so it binds to submitting rather than to deciding, and no module
# could write it.
python3 - "$LOG" <<'WINDOWS' || fail "the device requests are not the ones the layers above this store make"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
RECEIVE = "TOS.RUN.INTERFACE operation=endpoint_receive_call_region status="
ADDRESS = "TOS.RUN.INTERFACE operation=dma_device_address status=0"
ADDRESSES_PER_REQUEST = 3

# The block service's receives, in the order the layers above make their requests. The
# store's own two receives per generation are the other four of the sixteen, and they are
# interleaved here — so the windows are named by what the block service was asked, and a
# window that contained device work it should not have is what this pass refuses.
#
# **The two `CAPACITY` windows must contain none**, because §2.5.1's configuration read is
# not a ring request; every other window is a real read or write of one sector.
names = [
    "capacity, first initializer run",
    "READ sector 0, first initializer run",
    "WRITE sector 0, the initial header",
    "capacity, second initializer run",
    "READ sector 0, second initializer run",
    "READ sector 0, generation A opening",
    "WRITE sector 1, object 1's payload",
    "WRITE sector 0, occupancy after object 1",
    "WRITE sector 2, object 2's payload",
    "WRITE sector 0, occupancy after object 2",
    "READ sector 0, generation B opening",
    "READ sector 2, the object the reader asked for",
]
UNTOUCHED = {0, 3}

opened = [i for i, line in enumerate(events) if line.startswith(RECEIVE)]
# The store's receives share this operation, so the block service's are the ones that
# open a window containing device work — except the two capacity windows. Rather than
# guess, the whole set is taken and the count checked against what the boot makes.
if len(opened) != 16:
    print(f"expected sixteen received messages, saw {len(opened)}", file=sys.stderr)
    raise SystemExit(1)

bounds = opened + [len(events)]
touched = 0
for index in range(len(opened)):
    window = events[bounds[index] + 1:bounds[index + 1]]
    addresses = window.count(ADDRESS)
    if addresses == 0:
        continue
    if addresses != ADDRESSES_PER_REQUEST:
        print(f"an exchange resolved {addresses} device addresses, not "
              f"{ADDRESSES_PER_REQUEST}", file=sys.stderr)
        raise SystemExit(1)
    touched += 1
if touched != len(names) - len(UNTOUCHED):
    print(f"expected {len(names) - len(UNTOUCHED)} exchanges to reach the device, "
          f"saw {touched}", file=sys.stderr)
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
#   generation A        the alias and the region of its one `open`                   2
#   generation B        the same, plus the answer endpoint of the absent refusal,
#                       plus the alias and answer endpoint of the served `GET`       5
#   reader              2 aliases and the object region it was given                 3
#
# **A `PUT` releases nothing**, and that is the one-copy rule visible as a count: the
# client's region is forwarded to the device rather than copied and released here.
SUPERVISOR_RELEASES=$((7 + 5))
INITIALIZER_RELEASES=$((2 + 2))
BLOCK_RELEASES=$((5 + 5))
STATE_RELEASES=$((2 + 5))
READER_RELEASES=3
EXPECTED_RELEASES=$((SUPERVISOR_RELEASES + INITIALIZER_RELEASES + BLOCK_RELEASES +
    STATE_RELEASES + READER_RELEASES))
releases="$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')"
[ "$releases" = "$EXPECTED_RELEASES" ] ||
    fail "the boot released $releases capabilities; this topology releases
       $EXPECTED_RELEASES"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=-[0-9]*$')" = 0 ] ||
    fail "a release was refused, so something was held that could not be let go"
# One send-only alias per lower READ and per GET, and none refused: asking for a right a
# capability lacks is an intersection, not an error (ADR-0100, `CAPABILITY_V1` §4).
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=0$')" = 7 ] ||
    fail "expected seven send-only aliases, one per carried call"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=-[0-9]*$')" = 0 ] ||
    fail "an attenuation was refused"

# --- 4: generation A was collected before generation B existed -----------------
#
# **§13a's ordering, and it is this supervisor's policy rather than a nucleus
# guarantee.** The nucleus refuses to create B from the shared plan while A is *live* —
# `IPC_V1` §2 admits one receive-rights holder — but `process::retire` clears A's
# capabilities at retirement, before `wait_child` collects its tombstone, so nothing below
# makes collection the only possible order. The persistence proof needs A's address space
# and capabilities gone before B exists, so the supervisor collects and this asserts it.
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

# Between A reporting and B being created: A and the writer are the only children that can
# end — the block service still owes generation B its two requests — so **both** of their
# retirements and both collections must be here.
window = events[reported[0]:second]
retired = sum(1 for line in window if line.startswith(EXIT))
collected = sum(1 for line in window if line == COLLECTED)
if retired != 2 or collected != 2:
    print(f"between A's account and B's creation the journal shows {retired} retirement(s) "
          f"and {collected} collection(s); it must show two of each — generation A and the "
          f"writer, both gone and both collected", file=sys.stderr)
    raise SystemExit(1)

# And the last collection precedes the creation, which is the claim in one line.
last = max(i for i, line in enumerate(window) if line == COLLECTED)
if not reported[0] + last < second:
    print("the second collection does not precede B's creation", file=sys.stderr)
    raise SystemExit(1)
ORDER

# --- 5: control before data, at both layers ------------------------------------
#
# **`STATE_STORE_V1` §8b and `BLOCK_DEVICE_V1` §6a are the same rule at two heights**, and
# the tail of this journal is where both are read. What follows the boot's last completion
# wait is the block service answering generation B's read of object 2 and the store
# answering the reader, and nothing else — so the sequence is exactly six steps.
#
# A reply means "the read succeeded and exactly one region is owed". The reverse order
# admits an orphan region: an endpoint object lives for the boot, so a region queued on an
# answer endpoint whose waiter then died stays there, and a later holder of receive —
# including a successor created from the same launch plan — could take it as the answer to
# its own request.
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
#   the block service replies, then sends; the store takes the region, replies, then
#   sends; the reader takes it
wanted = ["reply", "send", "receive", "reply", "send", "receive"]
if seen != wanted:
    print(f"expected {wanted} after the last completion wait, saw {seen}", file=sys.stderr)
    raise SystemExit(1)
LAYERS

# --- 6: the accounts ----------------------------------------------------------
for want in "$EXPECTED_SUPERVISOR" "$EXPECTED_BLOCK" "$EXPECTED_FORMATTED" \
    "$EXPECTED_REFUSED_TO_FORMAT" "$EXPECTED_STATE_A" "$EXPECTED_STATE_B" \
    "$EXPECTED_WRITER" "$EXPECTED_READER"; do
    [ "$(printf '%s\n' "$completed" | grep -c "^$want$")" = 1 ] ||
        fail "no module reported $want exactly once; the boot reported:
       $(printf '%s ' $completed)"
done
[ "$(printf '%s\n' "$completed" | wc -l)" = 8 ] ||
    fail "eight accounts were not reported"

echo "state-store: PASS: a persistent object store, read back by a process that came after"
echo "  state.store.v1 (ADR-0099) over block.device.v1 (ADR-0098) over the reference"
echo "  VirtIO device, every layer canonical text and no host path anywhere"
echo "  an initializer formats a zeroed device once and **refuses** the second time,"
echo "  because only an all-zero sector 0 is permission to format — and it is the same"
echo "  module from the same sealed plan both times, differing only in what it finds"
echo "  a writer puts two objects and ends; its ending is collected"
echo "  generation A ends, is retired and is collected **before** generation B exists,"
echo "  which is this supervisor's policy and is asserted on the journal"
echo "  generation B re-reads and validates the header from the device, having been"
echo "  handed nothing but a sealed plan"
echo "  a reader gets object 2 and checks all 512 bytes in canonical text — so the"
echo "  bytes came off the device and not from anybody's memory"
echo "  get(3), an id never created, is refused as exactly REFUSED + ST_ABSENT and its"
echo "  sector is never read: presence is the occupancy bitmap and nothing else, which"
echo "  the twelve counted device requests confirm — the harness seeds sector 3"
echo "  control before data at both layers, asserted as one six-step sequence"
echo "  the store's isolation is capability topology read from the journal: the bus is"
echo "  held by the supervisor and the block service, block.device.v1's endpoint by the"
echo "  initializer runs and the store generations, and a client holds neither"
echo "  five endpoints of six and four endowments at most, ADR-0101 §6's corrected"
echo "  accounting, counted by the launcher rather than claimed by a module"
echo "  NOT claimed: power-loss durability, VIRTIO_BLK_F_FLUSH, crash consistency,"
echo "  journaling, transactions, exactly-once PUT, delete, enumeration, a second owner"
echo "  or store, docs/09's /state namespace, any path semantics, or Stage 4 closure"
