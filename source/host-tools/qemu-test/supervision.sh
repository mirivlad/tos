#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A supervisor written in TOS Core supervises real services on a real nucleus.
#
# `supervision.rs` drives the same two modules against a scripted system, where
# a tick can be put exactly on a window's boundary. This is the other half: the
# same policy and the same supervisor, on the real ABI, creating real processes
# out of real memory and learning they ended from `process_wait_child`.
#
# Three modules, all canonical text, all in this capsule:
#
#   /system/policy/services.tos   *what* to supervise, how hard to try, and what
#                                 depends on what. ADR-0051 §3 puts supervision
#                                 policy in `/system/policy/` as canonical
#                                 source, and every number the supervisor acts
#                                 on is here rather than there
#   /system/boot/init.tos         *how*: the state machine, the restart window,
#                                 the dependency rule and the latch
#   /system/boot/worker.tos       what gets supervised. It asks for nothing and
#                                 returns, so every service ends on its own and
#                                 the supervisor has something to decide about
#
# **The evidence is what the supervisor said and what the nucleus did**, and the
# two are different kinds of statement kept apart on purpose. The nucleus's
# lines carry the facts only ring 0 can assert — which process was created, out
# of whose authority, which ended and who ended it. The supervisor's journal
# carries what it inferred from those facts, what its policy said about them,
# what it attempted and what came back. Neither is derived from the other.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-supervision}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
# Anything after the output directory is passed to the boot harness unchanged.
# The gate calls this with one argument and is unaffected; what uses the rest is
# the human-facing launcher, which asks for an interactive display. An
# interactive session has no exit code to check and writes no event log, so the
# assertions below are skipped in that mode and the run says so — the gate is
# the evidence, and a demonstration does not pretend to be one.
shift || true
HARNESS_EXTRA=("$@")
INTERACTIVE=0
for argument in "${HARNESS_EXTRA[@]+"${HARNESS_EXTRA[@]}"}"; do
    [ "$argument" = "--interactive" ] && INTERACTIVE=1
done

FIXTURE="$ROOT/tests/vectors/supervision"
TOOL="$ROOT/target/release/tos-capsule-tool"
TARGET="$ROOT/target/test-supervision"

OK=0

fail() {
    echo "supervision: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-supervision)

printf '/system/boot/init.tos\t%s/init.tos\n/system/policy/services.tos\t%s/services.tos\n/system/boot/worker.tos\t%s/worker.tos\n' \
    "$FIXTURE" "$FIXTURE" "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/supervision.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/supervision.bin" --manifest "$OUT/capsule.meta.json"

if [ "$INTERACTIVE" -eq 1 ]; then
    bash "$HERE/run.sh" \
        --out "$OUT" \
        --capsule "$OUT/supervision.bin" \
        --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
        "${HARNESS_EXTRA[@]}"
    echo "supervision: interactive session ended; serial log: $OUT/serial.log"
    echo "supervision: assertions are skipped in this mode — run without"
    echo "supervision: --interactive, or the qemu_supervision gate, for evidence"
    exit 0
fi

bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/supervision.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REQUEST TOS.RUN.INTERFACE TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.NUCLEUS.INVARIANT" \
    "${HARNESS_EXTRA[@]+"${HARNESS_EXTRA[@]}"}" \
    > /dev/null

LOG="$OUT/events.log"
SERIAL="$OUT/serial.log"
count() { grep -c "$1" "$LOG" || true; }
said() { grep -c "^TOS\.RUN\.INTERFACE operation=endpoint_send_text status=$OK said=$1\$" "$LOG" || true; }

# --- three modules, resolved as one set ---------------------------------------
# Without this the supervisor could be one module pretending, and the policy's
# separateness — the whole point — would be unevidenced.
grep -q '^TOS\.RUN\.BEGIN .* modules=3$' "$LOG" ||
    fail "the boot did not run a set of three modules"

# --- the supervisor asked for what it uses, by name ----------------------------
for request in \
    'binding=process interface=system\.process\.Control object=3 wanted=3' \
    'binding=memory interface=system\.memory\.Authority object=6 wanted=6' \
    'binding=journal interface=system\.ipc\.Endpoint object=1 wanted=1'; do
    [ "$(count "^TOS\\.RUN\\.REQUEST $request\$")" = 1 ] ||
        fail "the supervisor's request '$request' was not answered by name and kind"
done
[ "$(count '^TOS\.RUN\.REQUEST ')" = 3 ] ||
    fail "the supervisor was granted something it did not ask for"

# --- the policy is read, and the supervisor acts on what it says ---------------
# The policy names three services and the module path they run; the supervisor's
# own text names none of them. Every service it journals is one the policy said.
[ "$(said 'system/boot/worker\.tos')" -ge 3 ] ||
    fail "the supervisor never named a service the policy declares"

# --- a real child, created out of a real authority, really ended ---------------
# The nucleus's own account, which the supervisor cannot forge: it can neither
# see the process table nor write these lines.
grep -q '^TOS\.RUN\.PROCESS_CHARGE .* grant=56623104 ' "$LOG" ||
    fail "no child was charged the grant the policy names"
[ "$(count '^TOS\.RUN\.PROCESS_EXIT process=[1-9]')" -ge 1 ] ||
    fail "no supervised child ever ran and ended"

# --- and the supervisor learned it through the accepted mechanism --------------
# `process_wait_child`, answering `OK` — not inferred from a timer, not assumed
# from a status, and not told to it by anything on the host side.
[ "$(count "^TOS\\.RUN\\.INTERFACE operation=process_wait_child status=$OK\$")" -ge 1 ] ||
    fail "the supervisor never observed an ending through process_wait_child"

# --- the five kinds of journal record, in the order the machine makes them -----
#
# A nucleus fact, then what was inferred from it, then what policy said, then
# what was attempted, then what came back. A journal that could not tell those
# apart would be a log of things that happened rather than a record of decisions.
for decision in \
    'info\.supervisor\.policy\.start-permitted' \
    'info\.supervisor\.action\.create' \
    'info\.supervisor\.result\.created' \
    'info\.supervisor\.observed\.ending' \
    'warn\.supervisor\.inferred\.own-failure'; do
    [ "$(said "$decision")" -ge 1 ] ||
        fail "the supervisor never journalled '$decision'"
done

# The order, read off the journal itself: an ending is observed before anything
# is inferred from it, and an action is attempted before its result is recorded.
python3 - "$LOG" <<'PY'
import re
import sys

records = []
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    found = re.match(r"^TOS\.RUN\.INTERFACE operation=endpoint_send_text status=0 said=(.*)$", line.strip())
    if found:
        records.append(found.group(1))

def first(name):
    return records.index(name) if name in records else None

for earlier, later in [
    ("info.supervisor.policy.start-permitted", "info.supervisor.action.create"),
    ("info.supervisor.action.create", "info.supervisor.result.created"),
    ("info.supervisor.observed.ending", "warn.supervisor.inferred.own-failure"),
    ("warn.supervisor.inferred.own-failure", "info.supervisor.policy.restart-permitted"),
]:
    a, b = first(earlier), first(later)
    if a is None or b is None or a >= b:
        raise SystemExit(
            f"supervision: FAIL: '{earlier}' does not precede '{later}' in the journal"
        )
    print(f"  {earlier} before {later}")

# The whole journal, so a reader of the evidence sees the decisions rather than
# a count of them.
print(f"  {len(records)} journal record(s), the first eight:")
for record in records[:8]:
    print(f"    {record}")
PY

# --- and the states are distinguishable on a real machine ----------------------
#
# Not "the code has a branch for each" — the boot took each of them. A blocked
# service, a latched one, and a restart, all in one run, all journalled as
# different decisions.
[ "$(said 'warn\.supervisor\.state\.blocked')" -ge 1 ] ||
    fail "no service was ever blocked, so BLOCKED is unevidenced on the real machine"
# Two services have a window wider than the boot and a budget of two, so both
# latch; the third's window is one tick and it never does. That the third goes
# on restarting *after* both latches is what says a latch is one service's.
[ "$(said 'error\.supervisor\.state\.failed')" = 2 ] ||
    fail "the two wide-window services should have exhausted their budgets"
[ "$(said 'info\.supervisor\.policy\.restart-permitted')" -ge 1 ] ||
    fail "nothing was ever restarted, so the budget was never exercised"
[ "$(said 'warn\.supervisor\.policy\.latched-no-start')" -ge 1 ] ||
    fail "a latched service was never asked again, so the latch proves nothing"

python3 - "$LOG" <<'PY'
import re
import sys

records = []
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    found = re.match(
        r"^TOS\.RUN\.INTERFACE operation=endpoint_send_text status=0 said=(.*)$",
        line.strip(),
    )
    if found:
        records.append(found.group(1))

latched = records.index("error.supervisor.state.failed")
last_latch = len(records) - 1 - records[::-1].index("error.supervisor.state.failed")
for at, record in enumerate(records):
    if record == "warn.supervisor.policy.latched-no-start" and at < latched:
        raise SystemExit(
            "supervision: FAIL: a service refused to start for being latched "
            "before anything latched"
        )
after = records[last_latch:]
if "info.supervisor.result.created" not in after:
    raise SystemExit(
        "supervision: FAIL: nothing was started after the latch, so the latch "
        "cannot be told apart from the run ending"
    )
refusals = after.count("warn.supervisor.policy.latched-no-start")
print(f"  the latch at record {latched}; {refusals} refusal(s) after it, and")
print("  other services still starting — so the latch is one service's and not the run's")
PY

# --- the number the supervisor returned ---------------------------------------
# 1000 + created x10 + latched x100 + blocked. Composed so that no single
# outcome produces it, and every part of it is a decision checked above.
grep -q '^TOS\.RUN\.COMPLETED value=i64:1302$' "$LOG" ||
    fail "the supervisor did not report the run this policy produces"

# --- the machine reached rest, and the account closed -------------------------
grep -q '^TOS\.RUN\.PROCESS_RECLAIMED process=0 .* plans_live=0$' "$LOG" ||
    fail "a launch plan outlived the process that made it"

echo "SUPERVISION PASS: a textual supervisor supervised real services"
echo "  three canonical modules: the policy, the supervisor and the thing supervised"
echo "  three grants at startup, and every service it acted on named by the policy"
echo "  children created out of a presented authority and ended on their own"
echo "  endings observed through process_wait_child, as the record §4.2 declares"
echo "  and the supervisor's own decisions journalled in the order it made them"
echo "  blocked, restarted and latched, all in one run and all told apart"
