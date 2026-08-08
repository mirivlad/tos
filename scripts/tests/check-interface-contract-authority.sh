#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Ensures every tracked versioned source-interface contract meets the Tier 2
# admission rule in docs/38 instead of self-assigning authority.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HIERARCHY="$ROOT/docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md"
SOURCES="$ROOT/docs/SPECIFICATION_SOURCES.txt"

fail() {
    echo "check-interface-contract-authority: FAIL: $*" >&2
    exit 1
}

grep -Fq 'Accepted versioned interface contracts under `source/interfaces/`' "$HIERARCHY" \
    || fail 'docs/38 does not assign the accepted source-interface contract class'
grep -Fq 'does not by itself grant' "$HIERARCHY" \
    || fail 'docs/38 does not prevent manifest-only authority escalation'

count=0
while IFS= read -r contract; do
    count=$((count + 1))
    file="$ROOT/$contract"
    grep -Fqx 'Status: **Accepted Tier 2 interface contract.**' "$file" \
        || fail "$contract lacks the explicit accepted contract status"
    grep -Fq '`docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`' "$file" \
        || fail "$contract lacks an explicit docs/38 authority reference"
    grep -Fqx "$contract" "$SOURCES" \
        || fail "$contract is absent from docs/SPECIFICATION_SOURCES.txt"
done < <(git -C "$ROOT" ls-files 'source/interfaces/**/*_V*.md')

[ "$count" -gt 0 ] || fail 'no versioned source-interface contracts found'
echo "check-interface-contract-authority: PASS ($count accepted contract(s))"
