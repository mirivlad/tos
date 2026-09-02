#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A memory authority, exercised from CPL 3 (`SYSTEM_ABI_V1` §5, operation 16).
#
# The boot endows its first process a child of the root memory authority — never
# the root itself, which is the nucleus's accounting anchor — and the process
# then asks the nucleus for everything this gate asserts. A reservation model
# proved only in a host test is a model nothing has actually asked the nucleus
# for; this is the ordinary system-call edge, from ring 3, on a real machine.
#
# What is asserted, and why each one is a claim somebody could get wrong:
#
#   child, distinct     reserving yields a capability naming a *child*, not the
#                       parent again — an operation that returned the same
#                       handle would be operation 5 wearing a new number
#   grandchild          a child can be reserved out of, so this is a tree
#   over, zero          E_LIMIT for a request this budget cannot serve and
#                       E_BAD_ARGUMENT for one no budget could (ADR-0076 §7):
#                       a caller told "limit" for an impossible size retries it
#                       forever
#   bad_handle          a handle naming nothing is refused before anything moves
#   alias, through_alias, after_alias
#                       **the decision-B claim.** Generic attenuation makes a
#                       second name for one authority, and the two spend one
#                       remainder: after the alias reserves what is left, the
#                       original is refused the same bytes. Two reservations
#                       would both have succeeded
#   released, reclaimed a child's last name going returns what it held, so the
#                       parent can reserve that amount again
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:?usage: memory-authority.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "memory-authority: FAIL: $*" >&2; exit 1; }

# **Not the canonical boot.** `system.boot.init` declares no capability
# request, and ADR-0055 gives a process what its launcher decided rather than
# what a gate would find convenient — granting it an authority it never asked
# for would be the launcher answering a request that does not exist. This build
# endows the allowance to a process, which is the same chain with a module at
# the end of it that asked.
# Built into its own directory, so the ordinary nucleus at the shared path is
# not replaced by a feature build that a later gate would then boot.
#
# The **image** is a feature build too, and that is not a convenience: operations
# 18 and 7 asked from CPL 3 are evidence, and evidence compiled into the image a
# canonical boot runs is a page of that image every ordinary process pays for.
BUILD="$ROOT/target/evidence/memory-authority"
(cd "$ROOT" && cargo build --release -p tos-nucleus \
    --target x86_64-unknown-none --features test-memory-authority --target-dir "$BUILD") \
    > "$OUT/build.log" 2>&1 || { cat "$OUT/build.log" >&2; fail "the build did not"; }
(cd "$ROOT" && cargo build --release -p tos-runtime-image \
    --target x86_64-unknown-none --features test-memory-authority --target-dir "$BUILD") \
    >> "$OUT/build.log" 2>&1 || { cat "$OUT/build.log" >&2; fail "the image did not build"; }

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

OK, E_NO_CAPABILITY, E_BAD_HANDLE, E_BAD_ARGUMENT, E_LIMIT = 0, -1, -2, -3, -6

held = [l for l in serial.splitlines() if l.startswith("TOS.RUN.CAPABILITY held=")]
if len(held) != 1:
    raise SystemExit(
        f"memory-authority: FAIL: expected one capability report, found {len(held)}"
    )
fields = dict(re.findall(r"(\w+)=(\S+)", held[0]))
# Object 6 is OBJECT_MEMORY_AUTHORITY, right 128 is RIGHT_SPEND. The process
# holds a child of the root and nothing else: no ambient authority, and no raw
# root handed to ring 3.
if fields["object"] != "6" or fields["rights"] != "128" or fields["binding"] != "memory":
    raise SystemExit(f"memory-authority: FAIL: the endowment is not the allowance: {held[0]}")
print("  the first process holds one memory authority, bound to the name it asked for")

lines = [l for l in serial.splitlines() if l.startswith("TOS.RUN.AUTHORITY ")]
if len(lines) != 1:
    raise SystemExit(
        f"memory-authority: FAIL: expected one authority report, found {len(lines)}"
    )
got = {name: int(value) for name, value in re.findall(r"(\w+)=(-?\d+)", lines[0])}

expected = {
    "child": OK,
    "distinct": 1,
    "grandchild": OK,
    "over": E_LIMIT,
    "zero": E_BAD_ARGUMENT,
    "bad_handle": E_BAD_HANDLE,
    "alias": OK,
    "through_alias": OK,
    "after_alias": E_LIMIT,
    "released": OK,
    "reclaimed": OK,
}
for name, want in expected.items():
    if name not in got:
        raise SystemExit(f"memory-authority: FAIL: the report omitted {name}")
    if got[name] != want:
        raise SystemExit(
            f"memory-authority: FAIL: {name} was {got[name]}, expected {want}"
        )
    print(f"  {name}: {got[name]}")

# Operation 17, on the same boot: a real region, written at both ends, released
# and made again.
region = [l for l in serial.splitlines() if l.startswith("TOS.RUN.REGION ")]
if len(region) != 1:
    raise SystemExit(f"memory-authority: FAIL: expected one region report, found {len(region)}")
made = {name: int(value) for name, value in re.findall(r"(\w+)=(-?\d+)", region[0])}
expected_region = {
    # A size that is not a whole number of frames comes back rounded up, which
    # is what was charged and what was mapped (ADR-0076 §7).
    "allocate": OK,
    "rounded": 1,
    # And placed in the lane its slot determines, not anywhere the caller chose.
    "in_lane": 1,
    # A fresh region is zero. The pool clears what it hands out, so this is what
    # says operation 17 did not find frames by some other road.
    "zeroed": 1,
    # Writable at both ends of the *rounded* length, not just the requested one.
    "wrote": 1,
    "released": OK,
    "again": OK,
    # The slot and its lane are reusable once the retirement has finished...
    "same_lane": 1,
    # ...and the handle to the region that was there is not. The index is in
    # range and the generation has moved on, which `CAPABILITY_V1` §2 and
    # ADR-0056's refusal order make `E_NO_CAPABILITY` rather than a bad handle.
    "stale": E_NO_CAPABILITY,
    "freed": OK,
}
for name, want in expected_region.items():
    if made.get(name) != want:
        raise SystemExit(
            f"memory-authority: FAIL: region {name} was {made.get(name)}, expected {want}"
        )
    print(f"  region {name}: {made[name]}")

# Operations 18 and 7: the two consuming transitions, on a region this process
# allocated, wrote and then could no longer write.
state = [l for l in serial.splitlines() if l.startswith("TOS.RUN.REGION.STATE ")]
if len(state) != 1:
    raise SystemExit(f"memory-authority: FAIL: expected one state report, found {len(state)}")
moved = {name: int(value) for name, value in re.findall(r"(\w+)=(-?\d+)", state[0])}
expected_state = {
    # `share` presupposes immutability rather than producing it, so a mutable
    # region carries no share right and operation 7 refuses it.
    "share_mutable": E_NO_CAPABILITY,
    "freeze": OK,
    # A **different** handle. An operation that changed the rights under the
    # number the caller already held would leave a process unable to tell a
    # frozen region from one it wrote a moment ago.
    "rehandled": 1,
    # And the presented one is stale, by generation.
    "stale_mutable": E_NO_CAPABILITY,
    # The transition has no inverse and cannot be repeated: what it returned
    # carries no write right.
    "refreeze": E_NO_CAPABILITY,
    # Nothing moved. Same backing, same base, one bit of each leaf cleared —
    # so the bytes written before the freeze are the bytes read after it.
    "kept": 1,
    "share": OK,
    "reshaped": 1,
    "stale_frozen": E_NO_CAPABILITY,
    # A shared region carries `read` and nothing else: there is nothing left
    # for a second share to consume, and nothing to freeze.
    "reshare": E_NO_CAPABILITY,
    "freeze_shared": E_NO_CAPABILITY,
    "after_share": 1,
    # Generic attenuation refuses an affine region and admits a shared one:
    # a second name for something that has no owner to duplicate.
    "alias": OK,
    "dropped_alias": OK,
    # Two names, one window. Dropping one of them leaves the memory readable;
    # only the last one takes the mapping with it.
    "survived": 1,
    "last_name": OK,
}
for name, want in expected_state.items():
    if moved.get(name) != want:
        raise SystemExit(
            f"memory-authority: FAIL: state {name} was {moved.get(name)}, expected {want}"
        )
    print(f"  state {name}: {moved[name]}")

# A full capability table refuses operation 17 before anything moves. A region
# with no handle naming it is one nobody could use or return, so the slot is
# found before the authority is charged and the backing laid down.
table = [l for l in serial.splitlines() if l.startswith("TOS.RUN.REGION.TABLE ")]
if len(table) != 1:
    raise SystemExit(f"memory-authority: FAIL: expected one table report, found {len(table)}")
filled = {name: int(value) for name, value in re.findall(r"(\w+)=(-?\d+)", table[0])}
if filled.get("aliases", 0) < 1:
    raise SystemExit("memory-authority: FAIL: the table was never filled")
for name, want in {"full": E_LIMIT, "freed": OK, "after": OK}.items():
    if filled.get(name) != want:
        raise SystemExit(
            f"memory-authority: FAIL: table {name} was {filled.get(name)}, expected {want}"
        )
    print(f"  table {name}: {filled[name]}")
print("  a full capability table refuses 17, and one freed slot makes it succeed again")

# And everything a region was made of came back: its frames to the pool, its
# lane's page tables to the reserve. The reserve's baseline is one below its
# size for the life of the boot — the backing index's root is permanent — so a
# gate expecting the raw size would be reporting a leak that is not one.
account = dict(
    re.findall(r"(\w+)=(-?\d+)", next(l for l in serial.splitlines() if l.startswith("TOS.MEM.ACCOUNT ")))
)
reserve_line = dict(
    re.findall(r"(\w+)=(-?\d+)", next(l for l in serial.splitlines() if l.startswith("TOS.MEM.RESERVE ")))
)
reclaimed = dict(
    re.findall(
        r"(\w+)=(-?\d+)",
        next(l for l in serial.splitlines() if l.startswith("TOS.RUN.PROCESS_RECLAIMED ")),
    )
)
if int(reclaimed["available"]) != int(account["root_frames"]):
    raise SystemExit(
        f"memory-authority: FAIL: the pool came back to {reclaimed['available']}, "
        f"not the root's {account['root_frames']}"
    )
baseline = int(reserve_line["runtime_baseline_frames"])
if int(reclaimed["tables_free"]) != baseline:
    raise SystemExit(
        f"memory-authority: FAIL: the reserve came back to {reclaimed['tables_free']}, "
        f"not its baseline {baseline}"
    )
print(f"  every frame back to {reclaimed['available']}; every table back to {baseline}")

if "TOS.NUCLEUS.INVARIANT" in serial:
    raise SystemExit("memory-authority: FAIL: an invariant was reported")

print("MEMORY-AUTHORITY PASS: operations 16 and 17 reserve, map, refuse and return")
PY
