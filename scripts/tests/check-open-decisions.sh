#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the open-decision and open-question gate.
#
# **It exists because the gate could not express the state the project is aiming
# at.** Both lists were read by requiring the extracted text to be non-empty, so
# "every ADR is over" and "every question is answered" were indistinguishable
# from "somebody deleted the fence" — the gate would have gone red on the day the
# last decision closed, and the obvious way out would have been to weaken it.
# Three states have to be three states:
#
#   fence missing                     -> red, the claim cannot be read at all
#   fence present and empty           -> green, but only if the authority is empty
#   fence present with items          -> green only on an exact match
#
# Each case is produced on a miniature journal, along with the non-empty cases
# that must still behave as they did.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GATE="$ROOT/scripts/check-open-decisions.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

REPO="$TMP/repo"

fail() {
    echo "check-open-decisions self-test: FAIL: $*" >&2
    exit 1
}

# --- the miniature journal ------------------------------------------------------
#
# `$1` is the extra ADR files to write, `$2` the open-decisions fence as it should
# appear in PROGRESS.md and `$3` the open-questions fence. A fence argument of
# `MISSING` writes no fence at all, which is the state that must be refused.
#
# One Accepted ADR with no open question is always present, so the authoritative
# sets are empty by default and a case adds to them deliberately.
build() {
    rm -rf "$REPO"
    mkdir -p "$REPO/docs/adr" "$REPO/scripts"
    cp "$GATE" "$REPO/scripts/"
    cat > "$REPO/docs/adr/0001-a-closed-decision.md" <<'EOF'
# ADR-0001: a decision that is over

- Status: **Accepted** (Project Architect-approved, 2026-01-01)
EOF
    eval "$1"

    {
        printf '# journal\n\n'
        case $2 in
            MISSING) printf 'no decisions fence here\n\n' ;;
            *) printf '```open-decisions\n%s```\n\n' "$2" ;;
        esac
        case $3 in
            MISSING) printf 'no questions fence here\n' ;;
            *) printf '```open-questions\n%s```\n' "$3" ;;
        esac
    } > "$REPO/PROGRESS.md"
}

check() { bash "$REPO/scripts/check-open-decisions.sh" --root "$REPO" >/dev/null 2>&1; }

OPEN_ADR='cat > "$REPO/docs/adr/0002-an-open-decision.md" <<EOF
# ADR-0002: a decision awaiting the Project Architect

- Status: **Proposed** (awaiting Project Architect decision)
EOF'

ADR_WITH_QUESTION='cat > "$REPO/docs/adr/0003-closed-with-a-question.md" <<EOF
# ADR-0003: closed, and it left something standing

- Status: **Resolved by existing semantics**

- Open question: ADR-0003-Q1 — the thing it did not settle.
EOF'

ORPHAN_QUESTION='cat > "$REPO/docs/adr/0004-a-question-for-nobody.md" <<EOF
# ADR-0004: a question naming an ADR that is not there

- Status: **Accepted**

- Open question: ADR-0099-Q1 — names no ADR file.
EOF'

# --- 1. a missing fence is red, for each list -----------------------------------
# The state the old gate confused with emptiness. It must stay red: a claim that
# is not written down is not a claim of zero.
build "" MISSING "" 
check && fail "a missing open-decisions fence was accepted"
build "" "" MISSING
check && fail "a missing open-questions fence was accepted"

# --- 2. an empty fence with an empty authority is green -------------------------
# **The state that was unrepresentable**, and the whole reason for this file: no
# ADR is open, no question stands, and the journal says so by saying nothing.
build "" "" ""
check || fail "zero open decisions and zero open questions were refused"

# --- 3. an empty fence with a non-empty authority is red ------------------------
build "$OPEN_ADR" "" ""
check && fail "an empty decisions fence was accepted while an ADR stood Proposed"
build "$ADR_WITH_QUESTION" "" ""
check && fail "an empty questions fence was accepted while a question stood"

# --- 4. a claimed item with an empty authority is red ---------------------------
# The other direction, which is how the drift this gate was built for actually
# happened: a list left saying something was open after it closed.
build "" "ADR-0002
" ""
check && fail "a decisions fence naming an ADR that is over was accepted"
build "" "" "ADR-0003-Q1
"
check && fail "a questions fence naming a question nobody raised was accepted"

# --- 5. exact non-empty sets are green ------------------------------------------
build "$OPEN_ADR
$ADR_WITH_QUESTION" "ADR-0002
" "ADR-0003-Q1
"
check || fail "lists that match the ADR files exactly were refused"

# --- 6. and a question must name an ADR that exists -----------------------------
# Not a zero-state case; it is here because the same walk reads it, and an
# orphaned id is a question nobody can go and read.
build "$ORPHAN_QUESTION" "" "ADR-0099-Q1
"
check && fail "a question naming no ADR file was accepted"

echo "check-open-decisions self-test: PASS (seven refusals and two acceptances)"
