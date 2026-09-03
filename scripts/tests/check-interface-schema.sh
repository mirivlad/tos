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
# The second accepted schema (ADR-0079). `SYSTEM_INTERFACE_V1` §2 says a schema
# is a class of document rather than one document, and the frontend keeps one
# table for all of them — a checker asks "is this path an interface?" once. So
# this gate reads both and compares the union: a second document with a gate of
# its own would be a second answer to the same question.
PLATFORM="$ROOT/source/interfaces/platform/PLATFORM_INTERFACE_V1.md"
TABLE="$ROOT/source/crates/tos-core/src/interfaces.rs"
HOST="$ROOT/source/runtime-image/src/main.rs"

fail() {
    echo "check-interface-schema: FAIL: $*" >&2
    exit 1
}

# Every operation the document declares, from the tables of section 4: the first
# cell of a row whose last cell is a `SYSTEM_ABI_V1` operation number.
declared=$( { sed -n '/^### /,/^## 4.1/p' "$DOC"; sed -n '/^### /,/^## 4.1/p' "$PLATFORM"; } |
    sed -n 's/^| `\([a-z_]*\)` |.*| [0-9]* |$/\1/p' | sort -u)
# Every operation the table names. Scoped to `ACCEPTED`, because §4.2's records
# are declared in the same file and their fields carry a `name` too — a field
# is not an operation, and a gate that could not tell them apart would report
# every record field as an operation the document forgot.
# Normalised so that a requirement is one line whatever `cargo fmt` decided.
# A long interface path pushes `Requirement::of("...", "...")` onto three lines,
# and a gate that read only the one-line form would silently stop checking the
# requirements that happened to be long — which is every platform one.
operations_in_table=$(sed -n '/^pub const ACCEPTED/,/^];$/p' "$TABLE" |
    awk '{
        line = $0
        if (pending != "") {
            sub(/^ +/, "", line)
            pending = pending line
            if (index(line, ")") == 0) next
            gsub(/, *\)/, ")", pending)
            gsub(/","/, "\", \"", pending)
            print pending
            pending = ""
            next
        }
        if (line ~ /Requirement::(of|held)\($/) { pending = line; next }
        print line
    }')
tabled=$(printf '%s\n' "$operations_in_table" | sed -n 's/^ *name: "\([a-z_]*\)",$/\1/p' | sort -u)
# And the declared maximum of every variable-length parameter (§4.1). The bound
# is part of the contract, not the host's choice, so a document and a table that
# agreed on the parameter while disagreeing on how long it may be would let a
# module be refused against a number nobody accepted.
#
# Read per row rather than per cell: an operation may take more than one bounded
# value, and a check that only saw the first would let the second drift.
bounds_in_doc=$( { sed -n '/^### /,/^## 4.2/p' "$DOC"; sed -n '/^### /,/^## 4.1/p' "$PLATFORM"; } |
    awk -F'|' '/^\| `[a-z_]+` \|/ {
        name = $2; gsub(/[` ]/, "", name);
        line = $4;
        while (match(line, /[0-9]+\)/)) {
            print name, substr(line, RSTART, RLENGTH - 1);
            line = substr(line, RSTART + RLENGTH);
        }
    }' | sort)
bounds_in_table=$(printf '%s\n' "$operations_in_table" |
    sed -n -e 's/^ *name: "\([a-z_]*\)",$/OP \1/p' \
        -e 's/^ *Parameter::bounded("[a-z]*", \([0-9]*\)),\?$/B \1/p' \
        -e 's/^ *parameters: &\[Parameter::bounded("[a-z]*", \([0-9]*\))\],$/B \1/p' |
    awk '$1 == "OP" { name = $2; next } { print name, $2 }' | sort)
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
    grep -Fq "\`$path\`" "$DOC" || grep -Fq "\`$path\`" "$PLATFORM" ||
        fail "the frontend's table names an interface no accepted document does: $path"
done < <(sed -n 's/^ *path: "\([a-z.A-Z]*\)",$/\1/p' "$TABLE")

# --- and §4.2's records, field by field, in order -----------------------------
#
# A record's fields are matched to their names **by position**, so the order is
# part of the contract and not a presentation choice: a document and a table
# that agreed on which fields exist while disagreeing on their order would put
# every value in the wrong field with nothing saying so.
records_in_doc=$(sed -n '/^### `system\.process\.CreatedProcess`/,/^## 5\./p' "$DOC" |
    sed -n 's/^| `\([a-z_]*\)` | `\([^`]*\)` |$/\1 \2/p')
records_in_table=$(sed -n '/^pub const RECORDS/,/^];$/p' "$TABLE" |
    sed -n -e 's/^ *name: "\([a-z_]*\)",$/\1/p' -e 's/^ *ty: "\([^"]*\)",$/\1/p' |
    awk '/^[a-z_]+$/ { name = $0; next } { print name, $0 }')
[ -n "$records_in_doc" ] || fail "section 4.2 declares no record fields"
[ "$records_in_doc" = "$records_in_table" ] || {
    echo "the document declares:" >&2
    echo "$records_in_doc" >&2
    echo "the frontend table declares:" >&2
    echo "$records_in_table" >&2
    fail "the accepted schema and the frontend's table disagree about a record"
}

# And the runtime image builds them in that same order, which is what makes a
# field's position mean the same thing on both sides of the boundary.
built=$(sed -n '/^fn ending_value/,/^}$/p' "$HOST" |
    sed -n -e 's/^ *number(record\.\([a-z_]*\)),$/\1/p' \
        -e 's/^ *optional(record\.has_\([a-z_]*\), record\.[a-z_]*),$/\1/p')
ending_fields=$(sed -n '/path: "system.process.ChildEnding"/,/^    },$/p' "$TABLE" |
    sed -n 's/^ *name: "\([a-z_]*\)",$/\1/p')
[ "$built" = "$ending_fields" ] || {
    echo "the schema declares:" >&2
    echo "$ending_fields" >&2
    echo "the runtime image builds:" >&2
    echo "$built" >&2
    fail "the child-ending record is built in a different order than it is declared"
}

# And the object kind each interface names (ADR-0061). A path is joined to a
# kind in exactly one place that decides — §4's table — and mirrored in exactly
# one place that checks. This pairs them line for line, because a mirror that
# agreed on which interfaces exist while disagreeing on what kind of object each
# one names would let a grant of the wrong kind through the startup check that
# exists to refuse it.
kinds_in_doc=$( { sed -n 's/^| `\(system\.[a-zA-Z.]*\)` | \([a-z ]*\) |$/\1 \2/p' "$DOC";
    sed -n 's/^| `\(platform\.[a-zA-Z.]*\)` | \([a-z ]*\) |$/\1 \2/p' "$PLATFORM"; } | sort)
kinds_in_table=$(printf '%s\n' "$operations_in_table" | sed -n \
    -e 's/^ *path: "\([a-z.A-Z]*\)",$/\1/p' \
    -e 's/^ *object: ObjectKind::\([A-Za-z]*\),$/\1/p' |
    paste - - |
    sed -e 's/Endpoint$/endpoint/' -e 's/Reply$/reply/' -e 's/Process$/process/' \
        -e 's/Region$/region/' -e 's/InterfacePublication$/interface publication/' \
        -e 's/MemoryAuthority$/memory authority/' \
        -e 's/LaunchPlanBuilder$/launch plan builder/' \
        -e 's/LaunchPlan$/launch plan/' \
        -e 's/PciBus$/pci bus/' -e 's/PciFunction$/pci function/' |
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
abi_in_doc=$( { sed -n '/^### /,/^## 4.1/p' "$DOC"; sed -n '/^### /,/^## 4.1/p' "$PLATFORM"; } |
    sed -n 's/^| `\([a-z_]*\)` |.*| \([0-9]*\) |$/\1 \2/p' | sort)
#
# The table is a list of `Performed` records, and what is wanted from each is the
# operation's name beside the `SYSTEM_ABI_V1` constant that performs it. They are
# two adjacent fields, so they are read as a pair rather than by a single
# pattern — one operation may be declared by several interfaces
# (`endow_for_launch` is), so the pairing is many-to-one on purpose.
abi_in_host=$(sed -n '/^const PERFORMED/,/^];$/p' "$HOST" |
    sed -n -e 's/^ *name: "\([a-z_]*\)",$/\1/p' -e 's/^ *operation: \([A-Z_]*\),$/\1/p' |
    awk '/^[a-z_]+$/ { name = $0; next } { print name, $0 }' | sort)
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
requirements_in_doc=$( { sed -n 's/^| `\([a-z_]*\)` | \(`system[^|]*\) | .* | [0-9]* |$/\1 \2/p' "$DOC";
    sed -n 's/^| `\([a-z_]*\)` | \(`platform[^|]*\) | .* | [0-9]* |$/\1 \2/p' "$PLATFORM"; } |
    sed -e 's/`//g' -e 's/ with / /g' -e 's/, then / + /' | sort)
requirements_in_table=$(printf '%s\n' "$operations_in_table" | sed -n \
    -e 's/^ *name: "\([a-z_]*\)",$/OP \1/p' \
    -e 's/^ *capabilities: &\[Requirement::of("\([a-zA-Z.]*\)", "\([a-z_]*\)")\],$/REQ \1 \2/p' \
    -e 's/^ *Requirement::of("\([a-zA-Z.]*\)", "\([a-z_]*\)"),$/REQ \1 \2/p' \
    -e 's/^ *capabilities: &\[Requirement::held("\([a-zA-Z.]*\)")\],$/REQ \1 none/p' \
    -e 's/^ *Requirement::held("\([a-zA-Z.]*\)"),$/REQ \1 none/p' |
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
