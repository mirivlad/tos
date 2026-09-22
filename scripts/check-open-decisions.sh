#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The journal's present-tense lists, against the ADR files themselves.
#
# **Two lists, because a decision and a question inside one end differently.**
# An ADR is open while its `- Status:` line says so, and it stops being open when
# that line moves. A *question* an ADR raised and did not answer does not move
# when the status does — ADR-0094 closed on the day it was raised and left §3's
# question standing — so a list of open ADRs says "nothing is open" while
# something is. That is the second failure this gate now covers.
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
# An ADR counts as open when its status line contains neither "Accepted" nor
# "Resolved". **Two ways for a decision to be over, and they are not the same
# thing.** Most end by being accepted; ADR-0094 ended by the implementation
# showing there was nothing to decide — the nucleus needed no change and no
# option of its own §6 was taken — and calling that "Accepted" would claim a
# decision nobody made.
# Bold markers, parenthetical option names and trailing prose are all ignored,
# which is why the test is a substring and not an equality: statuses in this
# tree are written as `**Accepted**`, `**Accepted (option R1a)** (Project
# Architect-approved, 2026-09-21)`, `**Accepted**, amended 2026-09-15 — see §1a`
# `**Proposed** (awaiting Project Architect decision)`, and
# `**Resolved by existing semantics and a minimal textual extension**`.
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
            *Accepted* | *Resolved*) ;;
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

# --- the second list: questions an ADR raised and did not answer ---------------
#
#   authority   every `- Open question: <id> — …` line in docs/adr/*.md
#   claim       the ```open-questions fence in PROGRESS.md, one id per line
#
# **It is read whatever the ADR's status says, and that is the whole point.** A
# question survives its parent decision: the ADR that raised it may be Accepted,
# Resolved or Proposed, and the line stays until somebody answers the question
# and removes it. So "the machine says no decision is open" can no longer be true
# while a question stands, which is what an external audit found on 2026-09-23.
#
# The id is `ADR-NNNN-QN` and its ADR must exist, so a question cannot be
# orphaned by a file rename or invented for an ADR nobody wrote.
questions="$(
    grep -h '^- Open question: ' "$ROOT"/docs/adr/*.md |
        sed 's/^- Open question: \([A-Za-z0-9-]*\).*$/\1/' | sort -u
)"

for id in $questions; do
    case $id in
        ADR-[0-9][0-9][0-9][0-9]-Q[0-9]*) ;;
        *) fail "an open question's id is not of the form ADR-NNNN-QN: $id" ;;
    esac
    number="${id#ADR-}"
    number="${number%%-*}"
    ls "$ROOT"/docs/adr/"$number"-*.md > /dev/null 2>&1 ||
        fail "open question $id names no ADR file in docs/adr/"
done

asked="$(
    awk '/^```open-questions$/{inside=1; next} /^```$/{inside=0} inside' \
        "$ROOT/PROGRESS.md" | sed '/^[[:space:]]*$/d' | sort
)"

[ -n "$asked" ] || fail "PROGRESS.md has no \`\`\`open-questions fence"

if [ "$questions" != "$asked" ]; then
    echo "check-open-decisions: the journal's questions and the ADR files disagree" >&2
    echo "  raised and unanswered in docs/adr/:" >&2
    echo "$questions" | sed 's/^/    /' >&2
    echo "  listed in PROGRESS.md:" >&2
    echo "$asked" | sed 's/^/    /' >&2
    fail "update the \`\`\`open-questions fence, or the ADR whose question was answered"
fi

echo "check-open-decisions: OK ($(echo "$actual" | wc -l | tr -d ' ') open," \
     "$(echo "$questions" | grep -c .) unanswered question(s)," \
     "$(ls "$ROOT"/docs/adr/*.md | wc -l | tr -d ' ') ADR(s) read)"
