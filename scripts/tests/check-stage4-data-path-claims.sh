#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# No document may say block data crosses IPC while nothing can put it there.
#
# **The claim is tied to the mechanism, not to a list of words.**
# `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1 fixes the Stage 4 data path as
#
#   Region<mut T> -> region_freeze -> Region<T> -> IPC -> service -> copy
#     -> DmaRegion<mut T> -> device
#
# and §2 states that the payload travels as a region capability by `IPC_V1` §5's
# own rules. So the question this gate must answer is exactly one: **can
# canonical text put an ordinary immutable `Region<T>` into a message yet?** While
# it cannot, a slice in which one scalar crossed IPC is not the data path, and no
# document may describe it as one.
#
# ## What does not count, and why it is stated rather than assumed
#
# - **`DmaRegion<T>` and `DmaRegion<mut T>` never count.** ADR-0037's table makes
#   them neither shareable nor transferable in either mode, which `IPC_V1` §5 and
#   the research note's §1 both restate: "a client cannot be handed device-visible
#   memory and a driver cannot be handed the client's", and the one forced copy
#   between the two is the whole shape of the slice. A DmaRegion is what the
#   *service* fills, not what the *client* sends, so its presence anywhere says
#   nothing about this boundary. An earlier version of this gate accepted it and
#   was wrong.
# - **`Region<mut T>` never counts.** `IPC_V1` §5: a writable region handle "may
#   not be delegated or sent at all, and a send that names one is refused whole".
#   The payload is the immutable form, after the freeze the note's §1 shows.
# - **Ordinary capability delegation never counts.** Capabilities travel in the
#   transfer table at `MESSAGE_CAPABILITIES` with their own count and their own
#   bound of four (ADR-0058, `IPC_V1` §6); regions travel in a *separate* area at
#   `MESSAGE_REGIONS` with a separate count and a bound of two (`IPC_V1` §3, §5).
#   `Placed::Transfer` is the first of those and is what `endpoint_call_carrying`
#   already uses. An earlier version of this gate looked at it, which is the same
#   error one level down: it would have lifted the bound on a capability crossing,
#   which has been possible since ADR-0094 and is not a payload.
# - **A Rust evidence workload never counts.** `qemu_region_transport` already
#   moves a region between two processes, and ADR-0094 §0 explicitly refuses "a
#   Rust runtime stage as a stand-in for canonical text". Those workloads are the
#   reason condition 3 below reads *where* `MESSAGE_REGIONS` is reached from and
#   not merely whether it is named.
#
# ## The three conditions, and they are ANDed
#
# **An earlier version of this gate ORed two of them, which made a declaration
# with no implementation — or an implementation with nothing accepted behind it —
# enough to lift the bound.** That is the opposite of what its own comment
# claimed. All three must hold at once:
#
#   1. **accepted schema** — an operation of an IPC interface declares an
#      ordinary immutable `Region<...>` parameter. Detectable today with no
#      guessing, because the type's canonical spelling is fixed by `docs/40` §3.
#   2. **canonical-text bridge** — `MESSAGE_REGIONS` is reached from
#      always-compiled code in the runtime image. Today every reference to it is
#      inside a `#[cfg(feature = "test-…")]` item, which is a Rust evidence
#      workload; the typed bridge (`PERFORMED`, `reach`, `transferred`) is
#      ungated, so a reference appearing outside a feature gate *is* the typed
#      bridge gaining the ability. Name-free on purpose: this gate does not guess
#      what the future `Slot` or `Placed` variant will be called.
#   3. **a conformance gate exists** — `scripts/preflight.sh` declares and defines
#      a gate function named `region_ipc_payload`. This is the marker a future
#      implementation must switch **consciously and together with its evidence**,
#      because in this project evidence follows a passing gate and never precedes
#      one. It cannot be flipped by accident and it cannot be flipped by a
#      document.
#
# **Fail-closed by construction.** Conditions 1 and 3 do not exist yet and
# condition 2 is false, so the bound holds; and the only way to lift it is to add
# a real row, a real bridge path and a gate that proves the two work. Nothing
# here designs that surface, and this gate must not be read as having done so.
#
# ## While the bound holds
#
#   a. every file that makes a claim about the client/service data path must cite
#      the note that says where the boundary is — so the non-claim cannot be
#      quietly dropped while the claim stays;
#   b. none of them may use the wording an external audit found on 2026-09-23,
#      when a slice in which one scalar crossed IPC was described as a client
#      reading a sector and as the Stage 4 path travelled end to end.
#
# The (b) list is that exact regression and is not an attempt to police language
# in general.
#
# **What is read of `PROGRESS.md` is its present-tense section and nothing else.**
# The journal is append-only and every dated entry in it is true on its date; a
# gate that read the whole file would be asking history to describe today, which
# is the error `check-open-decisions.sh` exists for.
#
#   bash scripts/tests/check-stage4-data-path-claims.sh [--root DIR]
#
# `--root` exists for the self-test beside this file, which builds miniature
# repositories to prove the truth table above. Nothing else passes it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
while [ "$#" -gt 0 ]; do
    case $1 in
        --root) ROOT="$(cd "$2" && pwd)"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

TABLE="$ROOT/source/crates/tos-core/src/interfaces.rs"
HOST="$ROOT/source/runtime-image/src/main.rs"
INVENTORY="$ROOT/scripts/preflight.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NOTE="STAGE4_DATA_PATH_BOUNDARY.md"

fail() {
    echo "check-stage4-data-path-claims: FAIL: $*" >&2
    exit 1
}

for required in "$TABLE" "$HOST" "$INVENTORY"; do
    [ -f "$required" ] || fail "$(basename "$required") is not there, so no condition can be read"
done

# --- condition 1: does an accepted schema row carry an ordinary Region<T>? ------
#
# Scoped to the IPC interfaces, because a region parameter on some other
# interface is not a message payload. `Region<mut ...>` and every `DmaRegion`
# spelling are excluded above and are excluded here.
schema_region_payload=$(python3 - "$TABLE" <<'PY'
import pathlib
import re
import sys

text = pathlib.Path(sys.argv[1]).read_text()
start = text.index("pub const ACCEPTED")
end = text.index("\n];", start)
interface = None
found = 0
for line in text[start:end].splitlines():
    stripped = line.strip()
    path = re.match(r'path: "([a-zA-Z.]+)",', stripped)
    if path:
        interface = path.group(1)
        continue
    if not (interface or "").startswith("system.ipc."):
        continue
    # A *parameter*, because the boundary needs the client to send one. A region
    # an operation produced is how one comes into existence (`docs/42` §2) and
    # says nothing about whether it can travel.
    if "Parameter::" not in stripped:
        continue
    # The canonical spellings `docs/40` §3 fixes. `DmaRegion<` is rejected by the
    # word boundary, and `mut` inside the type argument is rejected outright.
    for parameter in re.findall(r'"([^"]*)"', stripped):
        if not re.search(r'(^|[^A-Za-z])Region<', parameter):
            continue
        if "DmaRegion<" in parameter:
            continue
        if re.search(r'Region<\s*mut\b', parameter):
            continue
        found += 1
print(found)
PY
)

# --- condition 2: is MESSAGE_REGIONS reached from the canonical-text bridge? ----
#
# Every reference to it today sits in an item carrying a `#[cfg(feature = …)]`
# attribute, which is a Rust evidence workload and not canonical text. The typed
# bridge is always compiled, so an ungated reference is the bridge gaining the
# ability. Read structurally: for each reference, walk back to the item it is in
# and look at the attributes immediately above it.
bridge_region_transport=$(python3 - "$HOST" <<'PY'
import pathlib
import re
import sys

lines = pathlib.Path(sys.argv[1]).read_text().splitlines()
ITEM = re.compile(r"^(pub )?(unsafe )?(fn|const|static|impl|struct|enum) ")
ungated = 0
for index, line in enumerate(lines):
    if "MESSAGE_REGIONS" not in line:
        continue
    # The item this line belongs to: the nearest declaration at column zero.
    start = 0
    for back in range(index, -1, -1):
        if ITEM.match(lines[back]):
            start = back
            break
    # And the attribute block immediately above that declaration.
    gated = False
    probe = start - 1
    while probe >= 0:
        above = lines[probe].strip()
        if above.startswith("#["):
            if above.startswith("#[cfg("):
                gated = True
            probe -= 1
            continue
        if above.startswith("//") or above.startswith("///") or above == "":
            probe -= 1
            continue
        break
    if not gated:
        ungated += 1
print(ungated)
PY
)

# --- condition 3: is there a gate proving a region payload crossed IPC? --------
#
# Declared in the inventory and defined as a shell function. Both, because the
# parity self-test found that a declared-and-undefined gate keeps every count
# agreeing while proving nothing.
conformance_gate=0
if grep -qE '^gate +[a-z]+ +[a-z-]+ +".*" +region_ipc_payload$' "$INVENTORY" &&
        grep -qE '^region_ipc_payload\(\) *\{' "$INVENTORY"; then
    conformance_gate=1
fi

# --- and the bound lifts only when all three hold ------------------------------
if [ "$schema_region_payload" != 0 ] &&
        [ "$bridge_region_transport" != 0 ] &&
        [ "$conformance_gate" != 0 ]; then
    echo "check-stage4-data-path-claims: the accepted textual region payload path exists" \
         "($schema_region_payload schema row(s), $bridge_region_transport ungated bridge" \
         "reference(s), a region_ipc_payload gate); the claim bound no longer applies"
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

# --- a. the boundary is cited wherever the path is described --------------------
for read_file in "$WORK"/*; do
    grep -Fq "$NOTE" "$read_file" ||
        fail "$(basename "$read_file") describes the client/service data path and does not cite $NOTE, so its non-claim cannot be checked"
done

# --- b. and the wording the audit found is refused ------------------------------
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
echo "check-stage4-data-path-claims: PASS (no textual region payload path:" \
     "schema=$schema_region_payload bridge=$bridge_region_transport" \
     "conformance-gate=$conformance_gate;" \
     "$read_count claim text(s) cite $NOTE and none overstates the path)"
