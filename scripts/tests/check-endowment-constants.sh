#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Every launcher endowment constant is counted by the guard that excludes them.
#
# The nucleus decides what the boot process is endowed with in one of a set of
# test constants, each binding `first_endowment`. Two of them enabled together
# do **not** fail to compile — the second `let` shadows the first — so the boot
# would be one nobody asked for, reported as the boot that was. `main.rs` guards
# that by counting the enabled constants and refusing more than one.
#
# **This gate is why that count can be trusted.** The guard it replaced was a
# hand-written list of pairs, and a pair list needs `n` new rows per constant:
# it had gone partial, naming 17 of 32 features, with every constant added since
# Stage 4A — `test-pci-discovery`, `test-supervision`, `test-runtime-authority`,
# `test-build-topology` — appearing in it nowhere. A count needs one row, and
# this holds the count against the source it is about, so the same rot cannot
# happen quietly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MAIN="$ROOT/source/nucleus/src/main.rs"

fail() {
    echo "check-endowment-constants: FAIL: $*" >&2
    exit 1
}

# Every feature named by a `#[cfg(...)]` that guards a `first_endowment`
# binding. Read from the source rather than from a list beside it: the thing
# that makes a feature an endowment constant is that it binds the constant.
deciding=$(awk '
    /^ *#\[cfg\(/ { attr = ""; collecting = 1 }
    collecting { attr = attr $0 }
    collecting && /\]$/ { collecting = 0 }
    /let \(?first_endowment/ {
        if (attr !~ /not\(any/) {
            while (match(attr, /feature = "[a-z0-9-]+"/)) {
                name = substr(attr, RSTART + 11, RLENGTH - 12)
                print name
                attr = substr(attr, RSTART + RLENGTH)
            }
        }
        attr = ""
    }
' "$MAIN" | sort -u)

[ -n "$deciding" ] || fail "no launcher endowment constant was found at all"

# Every feature the guard counts.
counted=$(sed -n '/^const ENDOWMENT_CONSTANTS/,/^    )) as usize;$/p' "$MAIN" |
    sed -n 's/.*cfg!(feature = "\([a-z0-9-]*\)").*/\1/p; s/^ *feature = "\([a-z0-9-]*\)",\?$/\1/p' |
    sort -u)

[ -n "$counted" ] || fail "the guard counts no constant at all"

missing=$(comm -23 <(printf '%s\n' "$deciding") <(printf '%s\n' "$counted"))
[ -z "$missing" ] || {
    echo "these constants bind first_endowment and are not counted:" >&2
    printf '  %s\n' $missing >&2
    fail "a launcher endowment constant can be enabled beside another"
}

stray=$(comm -13 <(printf '%s\n' "$deciding") <(printf '%s\n' "$counted"))
[ -z "$stray" ] || {
    echo "these are counted and bind no first_endowment:" >&2
    printf '  %s\n' $stray >&2
    fail "the guard counts a feature that decides no endowment"
}

count=$(printf '%s\n' "$deciding" | grep -c .)
echo "check-endowment-constants: PASS ($count launcher endowment constant(s), each counted)"
