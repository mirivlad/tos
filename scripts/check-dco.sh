#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS DCO gate.
#
# docs/23_CONTRIBUTION_PROVENANCE.md: TOS uses Developer Certificate of Origin
# 1.1 sign-off rather than a copyright-assignment CLA, and the required trailer
# is `Signed-off-by: Real Name <email@example.com>`. The certificate is made by
# the person who contributes the work, so this gate requires:
#
#   1. at least one well-formed Signed-off-by trailer on every commit;
#   2. one of those trailers to name the commit author.
#
# It deliberately does NOT hard-code a repository owner. The previous version
# matched one literal identity, which meant any second contributor - the whole
# point of a DCO-without-CLA model - failed the gate.
#
# Usage:
#   scripts/check-dco.sh                 # every commit reachable from HEAD
#   scripts/check-dco.sh origin/main..   # a range, e.g. a pull request
#   scripts/check-dco.sh <base> <head>   # two endpoints
set -eu
cd "$(dirname "$0")/.."

case $# in
    0) range="HEAD" ;;
    1) range="$1" ;;
    2) range="$1..$2" ;;
    *) echo "usage: check-dco.sh [<range> | <base> <head>]" >&2; exit 2 ;;
esac

commits=$(git rev-list "$range")
if [ -z "$commits" ]; then
    echo "check-dco: no commits in range '$range' (nothing to verify)"
    exit 0
fi

fail=0
count=0
for c in $commits; do
    count=$((count + 1))
    author=$(git show -s --format='%an <%ae>' "$c")
    subject=$(git show -s --format='%s' "$c")

    # Trailer lines only; `git show -s --format=%B` keeps the full message, and
    # a well-formed trailer is "Signed-off-by: <name> <<email>>" with a
    # non-empty name and an address containing '@' and no spaces.
    signoffs=$(git show -s --format='%B' "$c" \
        | grep -E '^[[:space:]]*Signed-off-by:[[:space:]]*.+[[:space:]]+<[^<>[:space:]]+@[^<>[:space:]]+>[[:space:]]*$' \
        | sed -e 's/^[[:space:]]*Signed-off-by:[[:space:]]*//' -e 's/[[:space:]]*$//' || true)

    if [ -z "$signoffs" ]; then
        if git show -s --format='%B' "$c" | grep -qi '^[[:space:]]*Signed-off-by:'; then
            echo "malformed DCO trailer: $c $subject"
            echo "    expected 'Signed-off-by: Real Name <email@example.com>'"
        else
            echo "missing DCO trailer: $c $subject"
        fi
        fail=1
        continue
    fi

    # The certificate is the author's. Compare case-insensitively: Git preserves
    # the case a contributor typed, mail addresses are not case-sensitive in
    # practice, and a case difference is not a provenance problem.
    author_lc=$(printf '%s' "$author" | tr '[:upper:]' '[:lower:]')
    if printf '%s\n' "$signoffs" | tr '[:upper:]' '[:lower:]' | grep -Fxq "$author_lc"; then
        :
    else
        echo "DCO sign-off does not name the author: $c $subject"
        echo "    author:    $author"
        printf '    signed by: %s\n' "$signoffs"
        fail=1
    fi
done

if [ "$fail" -eq 0 ]; then
    echo "check-dco: OK ($count commit(s) in '$range' carry a DCO sign-off naming their author)"
else
    echo "check-dco: FAIL" >&2
fi
exit "$fail"
