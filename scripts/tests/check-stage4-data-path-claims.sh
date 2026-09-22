#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# No document may say block data crosses IPC while nothing can put it there.
#
# **The claim is tied to the mechanism, not to a list of words.**
# `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1 fixes the Stage 4 data path as
# client memory -> IPC -> service -> DMA memory, with the payload travelling as a
# region capability (`IPC_V1` §5). Whether that is reachable from canonical text
# is a fact about the accepted schema: a row must exist that places a region in a
# message. None does, and ADR-0094 §10 left region transfer from text undecided.
#
# So this gate asks the schema first. While no such row exists:
#
#   1. every file that makes a claim about the client/service data path must cite
#      the note that says where the boundary is — so the non-claim cannot be
#      quietly dropped while the claim stays;
#   2. none of them may use the wording an external audit found on 2026-09-23,
#      when a slice in which one scalar crossed IPC was described as a client
#      reading a sector and as the Stage 4 path travelled end to end.
#
# The second list is the exact regression and is not an attempt to police
# language in general. When a region-carrying row is accepted, the first branch
# below changes and both requirements lift on their own.
#
# **What is read of `PROGRESS.md` is its present-tense section and nothing else.**
# The journal is append-only and every dated entry in it is true on its date; a
# gate that read the whole file would be asking history to describe today, which
# is the error `check-open-decisions.sh` exists for.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TABLE="$ROOT/source/crates/tos-core/src/interfaces.rs"
HOST="$ROOT/source/runtime-image/src/main.rs"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NOTE="STAGE4_DATA_PATH_BOUNDARY.md"

fail() {
    echo "check-stage4-data-path-claims: FAIL: $*" >&2
    exit 1
}

# --- can a region cross IPC from canonical text at all? ------------------------
#
# Two independent readings, because either one alone could go stale. The schema:
# an operation whose declared parameters name a region type. And the host: a row
# that places a capability in a message's transfer table (`Placed::Transfer`)
# whose own requirement is the region interface.
region_parameters=$(sed -n '/^pub const ACCEPTED/,/^];$/p' "$TABLE" |
    grep -cE 'Parameter::fixed\("(platform\.dma\.Region|Region<|DmaRegion<)' || true)
region_transfers=$(sed -n '/^const PERFORMED/,/^];$/p' "$HOST" |
    grep -B 8 'Placed::Transfer' | grep -cE 'interface: "platform\.dma\.Region"' || true)

if [ "$region_parameters" != 0 ] || [ "$region_transfers" != 0 ]; then
    echo "check-stage4-data-path-claims: a region-carrying IPC row exists" \
         "($region_parameters schema, $region_transfers host); the claim bound below no longer applies"
    exit 0
fi

# --- what is read, and how much of it ------------------------------------------
#
# The slice's own files whole; the journal's present-tense section only; the
# README whole, because every word of it is a statement about today.
awk '/^### Состояние на .* настоящее время/{inside=1} /^## Checklist Stage 1$/{inside=0} inside' \
    "$ROOT/PROGRESS.md" > "$WORK/progress-now.md"
[ -s "$WORK/progress-now.md" ] ||
    fail "PROGRESS.md has no present-tense state section to read"

cp "$ROOT/README.md" "$WORK/README.md"
SLICE_FILES="source/host-tools/qemu-test/block-service.sh
source/tests/vectors/block-service/client.tos
source/tests/vectors/block-service/service.tos"
while IFS= read -r file; do
    [ -f "$ROOT/$file" ] || fail "$file is in scope and does not exist"
    cp "$ROOT/$file" "$WORK/$(basename "$file")"
done <<EOF
$SLICE_FILES
EOF

# --- 1. the boundary is cited wherever the path is described --------------------
for read_file in "$WORK"/*; do
    grep -Fq "$NOTE" "$read_file" ||
        fail "$(basename "$read_file") describes the client/service data path and does not cite $NOTE, so its non-claim cannot be checked"
done

# --- 2. and the wording the audit found is refused ------------------------------
#
# One extended regular expression per entry, matched case-insensitively.
#
# **A line that states the claim in order to deny it is not the claim**, and the
# documents corrected on 2026-09-23 are full of such lines on purpose. So a match
# on a line that also carries a negation is not a finding. That is looser than a
# parser and it is the right looseness here: the gate's job is to catch the
# affirmative wording coming back, not to understand prose.
NEGATION='(^|[^[:alnum:]])([Nn]ot|NOT|never|нет?|Нет?|НЕ)([^[:alnum:]]|$)'
patterns=(
    'client[^.]*read[s]?[^.]*real sector'
    'read[s]?[^.]*real sector[^.]*client'
    'end[ -]to[ -]end'
    'клиент[^.]*(прочитал|читает|прочёл)[^.]*сектор'
    'путь Stage 4 пройден'
    'payload region cross(es|ed)? IPC'
)
for read_file in "$WORK"/*; do
    for pattern in "${patterns[@]}"; do
        found=$(grep -inE "$pattern" "$read_file" | grep -vE "$NEGATION" || true)
        if [ -n "$found" ]; then
            echo "check-stage4-data-path-claims: in $(basename "$read_file"):" >&2
            printf '%s\n' "$found" | sed 's/^/    /' >&2
            fail "$(basename "$read_file") claims a data path no accepted schema row can carry"
        fi
    done
done

read_count=$(find "$WORK" -maxdepth 1 -type f | wc -l | tr -d ' ')
echo "check-stage4-data-path-claims: PASS (no region-carrying IPC row;" \
     "$read_count claim text(s) cite $NOTE and none overstates the path)"
