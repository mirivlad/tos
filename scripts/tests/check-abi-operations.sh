#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The operation numbers the ABI assigns are the numbers the system uses.
#
# `SYSTEM_ABI_V1` §5 assigns them and §7 makes the assignment permanent:
# "Operation numbers are assigned once and never reused: a retired operation
# returns `E_NOT_SUPPORTED` forever rather than being recycled into a different
# meaning." Three parties act on that table — the nucleus's dispatcher, the
# runtime image that calls it, and the schema that says which operations a module
# may reach — and until this gate existed nothing held any of them to it.
#
# It exists because the drift had already happened. ADR-0054 assigned
# `process_exit = 12` and the implementation used it, while §5's table still
# ended at 11: an accepted decision that the normative contract did not carry,
# found by looking rather than by failing. A number assigned in code and not in
# the contract is a number the next operation can be given twice.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ABI="$ROOT/source/interfaces/system/SYSTEM_ABI_V1.md"
NUCLEUS="$ROOT/source/nucleus/src/syscall.rs"
IMAGE="$ROOT/source/runtime-image/src/main.rs"

fail() {
    echo "check-abi-operations: FAIL: $*" >&2
    exit 1
}

# Every row of §5's table: the number and the operation's name.
declared=$(sed -n 's/^| \([0-9]*\) | `\([a-z_]*\)` |.*$/\2 \1/p' "$ABI" | sort)
[ -n "$declared" ] || fail "section 5 assigns no operation numbers"

# The numbers must be unique and contiguous from 1: §7's "assigned once and never
# reused" is unenforceable if the table itself can repeat one, and a gap would
# mean a number was spent somewhere this file cannot see.
numbers=$(printf '%s\n' "$declared" | awk '{print $2}' | sort -n)
expected=$(seq 1 "$(printf '%s\n' "$numbers" | wc -l)")
[ "$numbers" = "$expected" ] || {
    echo "section 5 assigns: $(printf '%s ' $numbers)" >&2
    fail "the assignment is not the numbers 1..n exactly once each"
}

# Each party's own constants, by name. The nucleus dispatches on them and the
# runtime image calls them; both write the number beside the operation's name,
# which is what makes a comparison possible without either being the source.
# `pub` is accepted because visibility is not the number: an operation the
# nucleus exports to its own modules is still that operation, and a gate that
# read only private constants could be evaded by adding a keyword.
constants_of() {
    sed -n 's/^\(pub \)\?const \([A-Z_]*\): u64 = \([0-9]*\);$/\2 \3/p' "$1" |
        awk '{ name = tolower($1); print name, $2 }' | sort
}

for party in "$NUCLEUS" "$IMAGE"; do
    while read -r name number; do
        [ -n "$name" ] || continue
        theirs=$(constants_of "$party" | awk -v n="$name" '$1 == n { print $2 }')
        # A party need not name every operation — the runtime image calls no
        # `region_share` — but one it does name must agree.
        [ -z "$theirs" ] && continue
        [ "$theirs" = "$number" ] ||
            fail "$(basename "$party") calls $name $theirs; section 5 assigns it $number"
    done <<EOF
$declared
EOF
done

# And the party that dispatches must name **every** assigned operation, or a
# number the contract spent would be one the nucleus answers `E_NOT_SUPPORTED`
# for — which §7 reserves for operations of a *later* version, not for ones this
# version assigned and nobody implemented.
missing=""
while read -r name number; do
    [ -n "$name" ] || continue
    constants_of "$NUCLEUS" | grep -q "^$name " || missing="$missing $name($number)"
done <<EOF
$declared
EOF
[ -z "$missing" ] || fail "the dispatcher names no constant for:$missing"

count=$(printf '%s\n' "$declared" | grep -c .)
echo "check-abi-operations: PASS ($count operation number(s), contract and implementation agree)"
