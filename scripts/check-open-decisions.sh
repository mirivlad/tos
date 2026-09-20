#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The journal's open-decision list is the set of ADRs that are not Accepted.
#
# **This gate exists because the same drift happened three times.** PROGRESS.md
# is a chronological journal, so every entry in it is true on its date and none
# of them is a statement about today. The failure each time was a *summary*
# sentence read as current: item C left in "requires a decision" after its four
# ADRs were accepted; H and G left there for a month after ADR-0067 and
# ADR-0068 were accepted and implemented; and then a header claiming the Stage
# 4C/4D ruling was the only thing awaiting the Project Architect while ADR-0044
# had stood Proposed since 2026-08-12.
#
# Each time the fix was a rule to remember — "take the status from the ADR's own
# `- Status:` line". A rule that has to be remembered is the same failure one
# level up, which is the reasoning `check-specification-manifest.py` already
# records. So the journal now carries exactly one present-tense list, and this
# compares it against the files.
#
#   authority   the `- Status:` line of each docs/adr/*.md
#   claim       the ```open-decisions fence in PROGRESS.md, one ADR id per line
#
# An ADR counts as open when its status line does not contain "Accepted".
# Bold markers, parenthetical option names and trailing prose are all ignored,
# which is why the test is a substring and not an equality: statuses in this
# tree are written as `**Accepted**`, `**Accepted (option R1a)** (Project
# Architect-approved, 2026-09-21)`, `**Accepted**, amended 2026-09-15 — see §1a`
# and `**Proposed** (awaiting Project Architect decision)`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

fail() {
    echo "check-open-decisions: FAIL: $*" >&2
    exit 1
}

actual="$(
    for adr in "$ROOT"/docs/adr/*.md; do
        status="$(grep -m1 '^- Status:' "$adr" || true)"
        [ -n "$status" ] || fail "$(basename "$adr") has no '- Status:' line"
        case $status in
            *Accepted*) ;;
            *) basename "$adr" | cut -d- -f1 | sed 's/^/ADR-/' ;;
        esac
    done | sort
)"

claimed="$(
    awk '/^```open-decisions$/{inside=1; next} /^```$/{inside=0} inside' \
        "$ROOT/PROGRESS.md" | sed '/^[[:space:]]*$/d' | sort
)"

[ -n "$claimed" ] || fail "PROGRESS.md has no \`\`\`open-decisions fence"

if [ "$actual" != "$claimed" ]; then
    echo "check-open-decisions: the journal's list and the ADR files disagree" >&2
    echo "  not Accepted in docs/adr/:" >&2
    echo "$actual" | sed 's/^/    /' >&2
    echo "  listed in PROGRESS.md:" >&2
    echo "$claimed" | sed 's/^/    /' >&2
    fail "update the \`\`\`open-decisions fence, or the ADR whose status moved"
fi

echo "check-open-decisions: OK ($(echo "$actual" | wc -l | tr -d ' ') open, $(ls "$ROOT"/docs/adr/*.md | wc -l | tr -d ' ') ADR(s) read)"
