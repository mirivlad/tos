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
declared=$(sed -n '/^### /,/^## 5/p' "$DOC" |
    sed -n 's/^| `\([a-z_]*\)` |.*| [0-9]* |$/\1/p' | sort -u)
# Every operation the table names.
tabled=$(sed -n 's/^ *name: "\([a-z_]*\)",$/\1/p' "$TABLE" | sort -u)

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

count=$(printf '%s\n' "$declared" | grep -c .)
echo "check-interface-schema: PASS ($count operation(s) declared and checked)"
