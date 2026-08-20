#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The accepted interface schema and the table the frontend checks against say
# the same thing.
#
# `SYSTEM_INTERFACE_V1` decides; `crates/tos-core/src/interfaces.rs` is the same
# content in a form a checker can compare against. Two statements of one fact
# drift, so this gate holds them together — and it compares the *operations*,
# because a document and a table agreeing on their prose while disagreeing on
# what a module may call is the drift that would matter.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DOC="$ROOT/source/interfaces/system/SYSTEM_INTERFACE_V1.md"
TABLE="$ROOT/source/crates/tos-core/src/interfaces.rs"

fail() {
    echo "check-interface-schema: FAIL: $*" >&2
    exit 1
}

# Every operation the document declares, from the tables of section 4: the first
# cell of a row whose last cell is a `SYSTEM_ABI_V1` operation number.
declared=$(sed -n '/^### /,/^## 4.1/p' "$DOC" |
    sed -n 's/^| `\([a-z_]*\)` |.*| [0-9]* |$/\1/p' | sort -u)
# Every operation the table names.
tabled=$(sed -n 's/^ *name: "\([a-z_]*\)",$/\1/p' "$TABLE" | sort -u)
# And the declared maximum of every variable-length parameter (§4.1). The bound
# is part of the contract, not the host's choice, so a document and a table that
# agreed on the parameter while disagreeing on how long it may be would let a
# module be refused against a number nobody accepted.
bounds_in_doc=$(sed -n 's/^| `\([a-z_]*\)` | [^|]* | `[a-z]*: [a-z0-9]*` (\xe2\x89\xa4 \([0-9]*\)) |.*$/\1 \2/p' "$DOC" | sort)
bounds_in_table=$(sed -n -e 's/^ *name: "\([a-z_]*\)",$/\1/p' -e 's/^ *maximum: Some(\([0-9]*\)),$/\1/p' "$TABLE" |
    awk '/^[a-z_]+$/ { name = $0; next } { print name, $0 }' | sort)
[ "$bounds_in_doc" = "$bounds_in_table" ] || {
    echo "the document bounds:" >&2
    echo "$bounds_in_doc" >&2
    echo "the frontend table bounds:" >&2
    echo "$bounds_in_table" >&2
    fail "the accepted schema and the frontend's table disagree about a parameter's maximum"
}

[ -n "$declared" ] || fail "the schema document declares no operations"
[ "$declared" = "$tabled" ] || {
    echo "the document declares:" >&2
    echo "$declared" >&2
    echo "the frontend table names:" >&2
    echo "$tabled" >&2
    fail "the accepted schema and the frontend's table disagree about what exists"
}

# Every interface path the table names must appear in the document, so a table
# cannot admit an interface nobody accepted.
while IFS= read -r path; do
    grep -Fq "\`$path\`" "$DOC" ||
        fail "the frontend's table names an interface the document does not: $path"
done < <(sed -n 's/^ *path: "\([a-z.A-Z]*\)",$/\1/p' "$TABLE")

# And the object kind each interface names (ADR-0061). A path is joined to a
# kind in exactly one place that decides — §4's table — and mirrored in exactly
# one place that checks. This pairs them line for line, because a mirror that
# agreed on which interfaces exist while disagreeing on what kind of object each
# one names would let a grant of the wrong kind through the startup check that
# exists to refuse it.
kinds_in_doc=$(sed -n 's/^| `\(system\.[a-zA-Z.]*\)` | \([a-z ]*\) |$/\1 \2/p' "$DOC" | sort)
kinds_in_table=$(sed -n \
    -e 's/^ *path: "\([a-z.A-Z]*\)",$/\1/p' \
    -e 's/^ *object: ObjectKind::\([A-Za-z]*\),$/\1/p' "$TABLE" |
    paste - - |
    sed -e 's/Endpoint$/endpoint/' -e 's/Reply$/reply/' -e 's/Process$/process/' \
        -e 's/Region$/region/' -e 's/InterfacePublication$/interface publication/' |
    tr '\t' ' ' | sort)

[ -n "$kinds_in_doc" ] || fail "section 4 declares no interface-to-object-kind pairing"
[ "$kinds_in_doc" = "$kinds_in_table" ] || {
    echo "the document pairs:" >&2
    echo "$kinds_in_doc" >&2
    echo "the frontend table pairs:" >&2
    echo "$kinds_in_table" >&2
    fail "the accepted schema and the frontend's table disagree about object kinds"
}

# And the `SYSTEM_ABI_V1` call each operation is performed by (ADR-0060 §8).
#
# That column lives in the runtime image rather than in the frontend's table on
# purpose: a frontend that knew the system ABI would be a second place it is
# declared, and `docs/42` §5 keeps the language and the ABI separately
# versioned. So the host that performs them carries the numbers, and this holds
# them against the document that assigns them.
HOST="$ROOT/source/runtime-image/src/main.rs"
abi_in_doc=$(sed -n '/^### /,/^## 4.1/p' "$DOC" |
    sed -n 's/^| `\([a-z_]*\)` |.*| \([0-9]*\) |$/\1 \2/p' | sort)
abi_in_host=$(sed -n '/^const PERFORMED/,/^];$/p' "$HOST" |
    tr -d ' \n' | tr '(' '\n' |
    sed -n 's/^"[a-zA-Z.]*","\([a-z_]*\)",\([A-Z_]*\),Shape::[A-Za-z]*,\?).*$/\1 \2/p' | sort)
# The host names each call by its `SYSTEM_ABI_V1` constant, so resolve those to
# the numbers the document assigns before comparing.
while IFS= read -r pair; do
    [ -n "$pair" ] || continue
    name=${pair%% *}
    symbol=${pair#* }
    number=$(sed -n "s/^const $symbol: u64 = \([0-9]*\);$/\1/p" "$HOST")
    [ -n "$number" ] || fail "the host names $symbol for $name and defines no such operation"
    printf '%s %s\n' "$name" "$number"
done <<EOF > "${TMPDIR:-/tmp}/tos-abi-host.$$"
$abi_in_host
EOF
abi_resolved=$(sort "${TMPDIR:-/tmp}/tos-abi-host.$$")
rm -f "${TMPDIR:-/tmp}/tos-abi-host.$$"

[ -n "$abi_in_doc" ] || fail "section 4 assigns no SYSTEM_ABI_V1 operation numbers"
[ "$abi_in_doc" = "$abi_resolved" ] || {
    echo "the document assigns:" >&2
    echo "$abi_in_doc" >&2
    echo "the runtime image performs:" >&2
    echo "$abi_resolved" >&2
    fail "the accepted schema and the host that performs it disagree about operations"
}

# Every capability requirement, as interface-and-right pairs per operation
# (ADR-0063). The right is half of a requirement: a document and a table that
# agreed on the interface while disagreeing on the right would let an operation
# be declared reachable with authority the system will refuse it for.
requirements_in_doc=$(sed -n 's/^| `\([a-z_]*\)` | \(`system[^|]*\) | .* | [0-9]* |$/\1 \2/p' "$DOC" |
    sed -e 's/`//g' -e 's/ with / /g' -e 's/, then / + /' | sort)
requirements_in_table=$(sed -n \
    -e 's/^ *name: "\([a-z_]*\)",$/OP \1/p' \
    -e 's/^ *capabilities: &\[Requirement::of("\([a-zA-Z.]*\)", "\([a-z]*\)")\],$/REQ \1 \2/p' \
    -e 's/^ *Requirement::of("\([a-zA-Z.]*\)", "\([a-z]*\)"),$/REQ \1 \2/p' "$TABLE" |
    awk '$1 == "OP" { if (name != "") print line; name = $2; line = $2; next }
         $1 == "REQ" { line = line " " $2 " " $3 }
         END { if (name != "") print line }' |
    sed 's/\(system[a-zA-Z.]* [a-z]*\) \(system\)/\1 + \2/' | sort)

[ -n "$requirements_in_doc" ] || fail "section 4 declares no capability requirements"
[ "$requirements_in_doc" = "$requirements_in_table" ] || {
    echo "the document requires:" >&2
    echo "$requirements_in_doc" >&2
    echo "the frontend table requires:" >&2
    echo "$requirements_in_table" >&2
    fail "the accepted schema and the frontend's table disagree about capability requirements"
}

count=$(printf '%s\n' "$declared" | grep -c .)
paired=$(printf '%s\n' "$kinds_in_doc" | grep -c .)
required=$(printf '%s\n' "$requirements_in_doc" | grep -c .)
echo "check-interface-schema: PASS ($count operation(s), $paired object kind(s)," \
     "$count ABI assignment(s) and $required capability requirement(s) checked)"
