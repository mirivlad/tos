#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A Region crosses between two processes (`IPC_V1` §5, §6; ADR-0037, ADR-0075).
#
# Two processes and one endpoint. The **worker** holds a child of the root
# memory authority, so it can make regions; the **peer** holds the right to
# receive and nothing else, so every region it comes to hold arrived in a
# message. Neither can do the other's half.
#
# What is asserted, and why each one is a claim somebody could get wrong:
#
#   mutable_refused     a `Region<mut T>` is neither shareable nor transferable
#                       (ADR-0037), so a message naming one is refused **whole**
#                       rather than delivered with that record dropped
#   still_writable      and the sender still holds it, still writable: a refusal
#                       that had consumed half the message would show here
#   overcount           three regions is past `IPC_V1` §3's bound of two, which
#                       is `E_BAD_ARGUMENT` — a constant the caller knew before
#                       it called, not a "retry later"
#   queue_full,         **the case the send transaction is shaped for.** A
#   refused_full,       linear region taken from its sender and then discovered
#   intact, sent        to have nowhere to go belongs to nobody, and no rollback
#                       can be relied on to put it back — rebuilding the window
#                       needs page tables and can fail on its own. So the room
#                       is asked for first: the send onto a full queue refuses,
#                       the sender's window is intact, and the **same handle**
#                       then sends successfully
#   stale               a successful linear send consumes the sender's handle
#   worker fault        and takes its window with it. `IPC_V1` §9.6 wants that
#                       demonstrated by a fault on the sender's next access,
#                       which is the worker's deliberate last act
#   refused, freed,     **acceptance is all or nothing, and not only for
#   delegated, handle   regions.** The peer fills its own capability table on
#                       purpose, so the first thing it asks of a queued message
#                       is one it cannot be given: `E_LIMIT`, nothing partial,
#                       and the message still queued. One freed slot later the
#                       same message arrives — and that message carries an
#                       ordinary delegated capability and no region at all,
#                       which is what says the property belongs to every message
#   first, second       the two region messages follow, one slot at a time
#   shared_read,        the shared region carries the bytes its sender wrote,
#   shared_length       at an address the nucleus chose in the receiver
#   moved_read,         and so does the affine one — whose sender is dead by
#   moved_tail          then, at both ends of the charged length. Region
#                       identity is not an address and not a sender's CR3
#   distinct            two regions arrive in two lanes, because they are two
#                       slots
#   alone,              a blocking receive **cancelled** is this process learning
#   shared_after,       it is the only one left (ADR-0059), and both regions
#   moved_after         still read after the process that made them has faulted
#                       and been reclaimed
#
# Then the account closes: every frame back to the root's count, every page
# table back to the reserve's baseline. That is what says the backing was
# reclaimed exactly once — not once per holder, and not never.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:?usage: region-transport.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "region-transport: FAIL: $*" >&2; exit 1; }

# Built into their own directories, so the ordinary artifacts at the shared
# paths are not replaced by feature builds a later gate would then boot.
BUILD="$ROOT/target/evidence/region-transport"
(cd "$ROOT" && cargo build --release -p tos-nucleus \
    --target x86_64-unknown-none --features test-region-transport --target-dir "$BUILD") \
    > "$OUT/nucleus.log" 2>&1 || { cat "$OUT/nucleus.log" >&2; fail "the nucleus did not build"; }
(cd "$ROOT" && cargo build --release -p tos-runtime-image \
    --target x86_64-unknown-none --features test-region-transport --target-dir "$BUILD") \
    > "$OUT/image.log" 2>&1 || { cat "$OUT/image.log" >&2; fail "the image did not build"; }

bash "$HERE/run.sh" --nucleus "$BUILD/x86_64-unknown-none/release/tos-nucleus" \
    --runtime-image "$BUILD/x86_64-unknown-none/release/tos-runtime-image" \
    --out "$OUT/boot" --expect 33 > "$OUT/boot.log" 2>&1 || {
    cat "$OUT/boot.log" >&2
    fail "the boot did not pass"
}

python3 - "$OUT/boot/serial.log" <<'PY'
import re
import sys

serial = open(sys.argv[1], "rb").read().decode("utf-8", "replace").replace("\r", "")
lines = serial.splitlines()

OK, E_NO_CAPABILITY, E_BAD_ARGUMENT, E_CANCELLED, E_LIMIT = 0, -1, -3, -5, -6


def one(prefix):
    found = [l for l in lines if l.startswith(prefix)]
    if len(found) != 1:
        raise SystemExit(
            f"region-transport: FAIL: expected one {prefix.strip()} line, found {len(found)}"
        )
    return found[0]


def fields(line):
    return {name: value for name, value in re.findall(r"(\w+)=(\S+)", line)}


def numbers(line):
    return {name: int(value, 0) for name, value in re.findall(r"(\w+)=(-?0x[0-9a-f]+|-?\d+)", line)}


def expect(what, got, wanted):
    for name, want in wanted.items():
        if got.get(name) != want:
            raise SystemExit(
                f"region-transport: FAIL: {what} {name} was {got.get(name)}, expected {want}"
            )
        print(f"  {what} {name}: {got[name]}")


SHARED_MARK = 0x5348415245445F31
SENT_MARK = 0x4D4F5645445F5F31
TAIL_MARK = 0x4D4F5645445F5F32

worker = numbers(one("TOS.RUN.REGION.WORKER "))
expect(
    "worker",
    worker,
    {
        # An ordinary capability message first, so that the refusal the peer
        # meets below is about a message with no region in it at all.
        "delegated_sent": OK,
        # The shared region: made, frozen, shared, named twice, and sent without
        # being given up.
        "froze_shared": OK,
        "shared": OK,
        "alias": OK,
        "shared_sent": OK,
        "shared_kept": 1,
        "alias_dropped": OK,
        "after_alias": 1,
        # A mutable region may not travel at all, and the refusal is whole.
        "mutable_refused": E_NO_CAPABILITY,
        "still_writable": 1,
        # Three regions is past the contract's bound of two.
        "overcount": E_BAD_ARGUMENT,
        "frozen": OK,
        # A queue nothing drains, filled to its depth and then refusing.
        "queue_full": E_LIMIT,
        "refused_full": E_LIMIT,
        # And the sender kept everything the refusal did not take.
        "intact": 1,
        "sent": OK,
        # A successful linear send consumes the handle.
        "stale": E_NO_CAPABILITY,
    },
)
if worker["filled"] < 1:
    raise SystemExit("region-transport: FAIL: the sink queue was never filled")
print(f"  worker filled: {worker['filled']} message(s) onto a queue nobody drains")

# The worker's last act is a read of the address it no longer owns. The line
# after it is never reached, and the fault is the evidence `IPC_V1` §9.6 asks
# for.
if any(l.startswith("TOS.RUN.REGION.WORKER.UNREACHED") for l in lines):
    raise SystemExit(
        "region-transport: FAIL: the sender could still read the region it sent"
    )
faults = [l for l in lines if l.startswith("TOS.RUN.PROCESS_FAULT ")]
if len(faults) != 1:
    raise SystemExit(
        f"region-transport: FAIL: expected exactly one fault, found {len(faults)}"
    )
print("  the sender's next access to the region it sent faulted")

peer = numbers(one("TOS.RUN.REGION.PEER "))
expect(
    "peer",
    peer,
    {
        # A full table refuses a message it cannot be given whole, and the
        # message stays queued for the attempt after one slot is freed. That
        # message carries a capability and no region, so what is proved is the
        # property of *every* message rather than of region transport.
        "refused": E_LIMIT,
        "freed": OK,
        "delegated": OK,
        "first": OK,
        "second": OK,
        # Exactly what the worker wrote, at both ends of the charged length.
        "shared_read": SHARED_MARK,
        "moved_read": SENT_MARK,
        "moved_tail": TAIL_MARK,
        # Two regions, two lanes, two handles.
        "distinct": 1,
        # **A blocking receive cancelled is this process learning it is alone.**
        # ADR-0059's rule fires when no context is runnable and nothing routed
        # can change that, which in a boot of two processes where the other one
        # never blocks is the instant the other one has ended. Everything after
        # it is a statement about a region whose sender has faulted and been
        # reclaimed.
        "alone": E_CANCELLED,
        "shared_after": SHARED_MARK,
        "moved_after": SENT_MARK,
    },
)
if peer["handle"] == 0:
    raise SystemExit(
        "region-transport: FAIL: the delegated capability arrived as a zero handle, "
        "which is the partial delivery this gate exists to rule out"
    )
print("  an ordinary capability message was refused whole and then delivered whole")
if peer["shared_length"] != 4096 or peer["moved_length"] != 2 * 4096:
    raise SystemExit(
        "region-transport: FAIL: the receiver was told lengths "
        f"{peer['shared_length']} and {peer['moved_length']}"
    )
print("  the receiver was told the charged and mapped length of each")

# The cancellation the receiver observed is the nucleus's, and it is on the log
# beside the receiver's own account of it. A gate that took the process's word
# for its own solitude would be taking the word of the thing under test.
cancelled = [l for l in lines if l.startswith("TOS.RUN.BLOCK_CANCELLED ")]
if len(cancelled) != 1:
    raise SystemExit(
        f"region-transport: FAIL: expected one cancelled block, found {len(cancelled)}"
    )
if fields(cancelled[0])["reason"] != "no-runnable-context":
    raise SystemExit(f"region-transport: FAIL: {cancelled[0]}")
print("  the receiver read the regions again after its sender was gone")

account = fields(one("TOS.MEM.ACCOUNT "))
reserve = fields(one("TOS.MEM.RESERVE "))
reclaimed = [l for l in lines if l.startswith("TOS.RUN.PROCESS_RECLAIMED ")]
if not reclaimed:
    raise SystemExit("region-transport: FAIL: nothing was reclaimed")
last = fields(reclaimed[-1])
if int(last["available"]) != int(account["root_frames"]):
    raise SystemExit(
        f"region-transport: FAIL: the pool came back to {last['available']}, "
        f"not the root's {account['root_frames']}"
    )
baseline = int(reserve["runtime_baseline_frames"])
if int(last["tables_free"]) != baseline:
    raise SystemExit(
        f"region-transport: FAIL: the reserve came back to {last['tables_free']}, "
        f"not its baseline {baseline}"
    )
print(f"  every frame back to {last['available']}; every table back to {baseline}")

if "TOS.NUCLEUS.INVARIANT" in serial:
    raise SystemExit("region-transport: FAIL: an invariant was reported")

print("REGION-TRANSPORT PASS: a region is frozen, shared, refused, queued and delivered")
PY
