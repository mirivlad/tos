#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Authority is a handle, and a message crosses between two processes.
#
# `CAPABILITY_V1` §7 and `IPC_V1` §9 ask for evidence rather than description,
# and every check below is a *question a process asked the nucleus*, read off
# the answers the nucleus gave. The process cannot see the capability table, so
# the only thing it can report is which handle it named and what came back —
# which is exactly what makes these answers evidence.
#
#   - ADR-0055: a process holds what whoever launched it decided, before it runs
#     its first instruction, and the decision is on the log as a decision.
#   - ADR-0056: an index outside the caller's table is `E_BAD_HANDLE`; anything
#     past that point is `E_NO_CAPABILITY`. Guessing yields neither.
#   - `CAPABILITY_V1` §7.2: iterating every index in range yields only what the
#     process was granted — nothing, here, because a handle is an index *and* a
#     generation and the generation is not derivable from the index.
#   - `CAPABILITY_V1` §7.3: a released handle refuses by generation afterwards.
#   - `CAPABILITY_V1` §7.4: asking to attenuate to *every* right yields what was
#     already held, proved by the resulting handle still refusing the half this
#     process was never given.
#   - `IPC_V1` §2: `send` and `receive` are separate rights, so the holder of
#     one is refused the other on the very same handle.
#   - `IPC_V1` §9.1: a payload past the inline bound (ADR-0057: 256 bytes) is
#     refused, not truncated.
#   - `IPC_V1` §6 and `CAPABILITY_V1` §4: a capability sent with a message
#     arrives as the receiver's own handle, and is authority rather than
#     decoration — the receiver does with it the very thing its own handle was
#     refused, one line apart.
#
#   bash host-tools/qemu-test/capabilities.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && cd ../.. && pwd)"
OUT="${1:-$ROOT/target/qemu-capabilities}"
FEATURE=test-two-processes
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TEST_TARGET="$ROOT/target/test-scheduler"
TEST_NUCLEUS="$TEST_TARGET/x86_64-unknown-none/release/tos-nucleus"

# What the sender puts in its message slot, and what the receiver must read out
# of its own. One token, because it is reported as the value of a `text=` field.
PAYLOAD="authority-crossed-a-boundary"
BYTES=28
# `SYSTEM_ABI_V1` §4 statuses, by the numbers the contract assigns.
E_NO_CAPABILITY=-1
E_BAD_HANDLE=-2
E_BAD_ARGUMENT=-3

fail() {
    echo "capabilities: FAIL: $*" >&2
    exit 1
}

[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"

(cd "$ROOT" && CARGO_TARGET_DIR="$TEST_TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features "$FEATURE")
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building isolated test artifact" >&2
    exit 1
}

bash "$ROOT/host-tools/qemu-test/run.sh" \
    --out "$OUT" \
    --nucleus "$TEST_NUCLEUS" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PROCESS_ENDOWED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE"

LOG="$OUT/events.log"

count() { grep -c "$1" "$LOG" || true; }
exactly() {
    # exactly <n> <pattern> <what>
    local seen
    seen=$(count "$2")
    [ "$seen" = "$1" ] || fail "$3: saw $seen line(s) matching '$2', expected $1"
}

# --- the launcher decided, and said so --------------------------------------
# A grant nobody can attribute is ambient authority with a handle in front of it
# (`CAPABILITY_V1` §3). Each process's endowment is announced by the launcher,
# names the process, and says it came from the launcher's stated constant rather
# than from a default.
exactly 2 '^TOS\.RUN\.PROCESS_ENDOWED process=[01] capabilities=1 policy=launcher-constant asserted_by=launcher$' \
    "the launcher did not announce both endowments"

# --- each process holds one half of one endpoint ----------------------------
# object=1 is OBJECT_ENDPOINT; rights 1 and 2 are `send` and `receive`, which
# are separate rights of the same object (`IPC_V1` §2).
exactly 1 '^TOS\.RUN\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=1 binding=endpoint$' \
    "no process holds exactly the send half"
exactly 1 '^TOS\.RUN\.CAPABILITY held=1 handle=0x[0-9a-f]* object=1 rights=2 binding=endpoint$' \
    "no process holds exactly the receive half"

# --- guessing is worth nothing ----------------------------------------------
exactly 2 "^TOS\\.RUN\\.CAPABILITY\\.PROBE out_of_range=$E_BAD_HANDLE in_range_refused=16 guessed=0\$" \
    "a process guessing handles learned something, or the refusals were not the contract's"

# --- the message crossed, whole, and the bound refused rather than truncated -
# `IPC_V1` §9.1's three bounds, and §9.3's failed transfer, on one line.
#
#   - `oversize`  — a payload one byte past the inline maximum;
#   - `overcount` — one capability past the four ADR-0057 fixes;
#   - `unheld`    — a transfer naming a handle this process does not hold.
#
# The first two answer **the same status**, and that is the point of asserting
# them together: both are constants of the contract the caller knew before it
# called, so both are malformed calls. `E_LIMIT` belongs to the full queue
# (§9.2) alone, and a caller that could not tell "retry later" from "this call
# can never work" would learn nothing from either.
#
# `unheld` is a refusal rather than a send that succeeded carrying nothing: §9.3
# requires a failed send to transfer no capability, and the way to know it
# transferred none is that the send did not happen at all. The status is
# `E_BAD_HANDLE` and not `E_NO_CAPABILITY` because the index named is outside
# this process's table — "you named nothing" rather than "you lack the
# authority", which is the distinction `SYSTEM_ABI_V1` §4 forbids merging and
# ADR-0056's refusal order decides.
exactly 1 "^TOS\\.RUN\\.IPC\\.SENT bytes=$BYTES status=0 oversize=$E_BAD_ARGUMENT other_half=$E_NO_CAPABILITY overcount=$E_BAD_ARGUMENT unheld=$E_BAD_HANDLE\$" \
    "one of IPC_V1 section 3's bounds did not refuse, or a handle nobody holds travelled"
exactly 1 "^TOS\\.RUN\\.IPC\\.RECEIVED bytes=$BYTES text=$PAYLOAD\$" \
    "the receiver did not read the sender's message back whole"
exactly 1 "^TOS\\.RUN\\.IPC\\.RIGHTS other_half=$E_NO_CAPABILITY\$" \
    "the receive half was not refused the send half"

# --- and the capability that travelled with it works ------------------------
# `IPC_V1` §6 and `CAPABILITY_V1` §4: the receiver gets its own handle, in its
# own table, with its own generation. The proof that it is authority and not
# decoration is the line above: with its *own* handle the same call was refused,
# and with this one it succeeds — on the same endpoint, in the same process, one
# line apart.
exactly 1 '^TOS\.RUN\.IPC\.DELEGATED handle=0x[0-9a-f]* send=0$' \
    "a capability sent with the message did not arrive, or arrived unusable"
# The non-blocking form still answers. Which of the two true answers it gets
# depends on whether the sender got there first, so both are accepted and the
# deterministic case is checked by the blocking gate.
exactly 1 '^TOS\.RUN\.IPC\.POLLED status=\(0\|-4\)$' \
    "the non-blocking receive gave neither of the two answers it may give"

# --- attenuation narrows and never widens -----------------------------------
exactly 2 "^TOS\\.RUN\\.CAPABILITY\\.ATTENUATED status=0 asked=all widened_half=$E_NO_CAPABILITY\$" \
    "attenuating to every right produced something wider than what was held"

# --- and a released handle is stale -----------------------------------------
exactly 2 "^TOS\\.RUN\\.CAPABILITY\\.RELEASED status=0 reuse=$E_NO_CAPABILITY\$" \
    "a released handle was not refused when it was named again"

# --- the boot still did its own work ----------------------------------------
exactly 2 '^TOS\.RUN\.COMPLETED value=i32:240$' \
    "the processes did not both complete their own work"

echo "CAPABILITIES PASS: authority is a handle, and a message crossed between two processes"
echo "  endowed by the launcher before either ran; each holds one half of one endpoint"
echo "  guessing refused ${E_BAD_HANDLE} out of range and ${E_NO_CAPABILITY} in it; nothing guessed"
echo "  $BYTES bytes delivered whole; a payload past the 256-byte bound refused, not truncated"
echo "  a fifth transferred capability refused the same way; a handle nobody holds did not travel"
echo "  attenuation to every right stayed inside what was held; released handles went stale"
echo "  a capability arrived with the message and did what the receiver's own handle could not"
