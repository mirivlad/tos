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
OUT="${1:?usage: memory-authority.sh OUTDIR}"
mkdir -p "$OUT"

fail() { echo "memory-authority: FAIL: $*" >&2; exit 1; }

bash "$HERE/run.sh" --out "$OUT/boot" --expect 33 > "$OUT/boot.log" 2>&1 || {
    cat "$OUT/boot.log" >&2
    fail "the ordinary boot did not pass"
}

python3 - "$OUT/boot/serial.log" <<'PY'
import re
import sys

serial = open(sys.argv[1], "rb").read().decode("utf-8", "replace").replace("\r", "")

OK, E_BAD_HANDLE, E_BAD_ARGUMENT, E_LIMIT = 0, -2, -3, -6

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

print("MEMORY-AUTHORITY PASS: operation 16 reserves, refuses and shares one budget")
PY
