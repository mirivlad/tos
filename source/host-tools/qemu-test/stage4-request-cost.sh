#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# What one completed `block.device.v1` READ costs, counted by the nucleus.
#
# **`docs/35` §Stage 4's hard budgets are counts, and this is where they are
# counted** rather than argued. The accepted protocol boot of `block-protocol.sh`
# runs unchanged — its twelve repeated READs are the steady state — on a nucleus
# whose one addition is that the scheduler counts what it does and reports the
# running totals on every routed interrupt delivery:
#
#   dispatches   the processor given to a context
#   handoffs     those that give it to a different context than last had it; an
#                idle wait counts as nobody, so a context woken out of one is a
#                handoff
#   idles        the machine waiting for an interrupt with nothing runnable
#   preemptions  handoffs made by the timer rather than by a context giving the
#                processor up
#
# Every repeated READ completes with exactly one delivery, so the difference
# between two consecutive deliveries is exactly one request, and the runtime's own
# per-operation lines between them say what that request asked the system for.
#
# **What is asserted is what the accepted design makes deterministic**, and it is
# asserted exactly, so that a change to it cannot pass unnoticed:
#
#   per completed READ    1 region_allocate      the answer region `BLOCK_DEVICE_V1`
#                                                §6a sends, which leaves this process
#                                                linearly and cannot be reused
#                         0 dma_region_allocate  the queue's one DMA region serves
#                                                every request
#                         1 routed delivery      one completion, one wake
#                         1 idle wait            the device's time is nobody's
#                         8 handoffs, before     the reply wakes the client and the
#                           the timer's          region is a second message, and while
#                                                both are runnable each system call
#                                                returns through a round-robin turn;
#                                                the timer adds at most two a tick
#
# **Two of those exceed the budgets `docs/35` states** — "zero dynamic allocation
# per completed block request" and "no more than four address-space/scheduler
# handoffs per unbatched request" — and this gate does **not** turn that into a
# pass. It pins what the design costs so the Stage 4 performance report can cite a
# number the machine produced; the verdict against the budget is the report's, and
# it says EXCEEDED (`docs/evidence/STAGE4_PERFORMANCE_REPORT.md`).
#
# Timer preemptions ride on top and vary from run to run with where the tick
# lands; they are reported and not asserted. So are host wall-clock intervals,
# which are observational: this is not the reference-platform measurement.
#
#   bash host-tools/qemu-test/stage4-request-cost.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-stage4-request-cost}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
FIXTURE="$ROOT/tests/vectors/block-protocol"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-stage4-request-cost"

fail() { echo "stage4-request-cost: FAIL: $*" >&2; exit 1; }

cleanup() { rm -rf "$TARGET"; }
trap cleanup EXIT

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }

before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none \
    --features test-block-protocol,test-request-cost >/dev/null 2>&1) ||
    fail "the nucleus does not build with test-block-protocol,test-request-cost"
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
    fail "the production nucleus changed while building the isolated test artifact"

{
    printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE"
    printf '/system/service/block.tos\t%s/service.tos\n' "$FIXTURE"
    printf '/system/client/block.tos\t%s/client.tos\n' "$FIXTURE"
} > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/capsule.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt" >/dev/null
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/capsule.bin" --manifest "$OUT/capsule.meta.json" >/dev/null

bash "$HERE/run.sh" \
    --out "$OUT/boot" \
    --capsule "$OUT/capsule.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --event-timestamps "$OUT/timestamps.jsonl" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.IRQ_DELIVERED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null || fail "the accepted protocol boot did not complete"

python3 - "$OUT/boot/events.log" "$OUT/timestamps.jsonl" <<'COUNT' || fail "one completed READ does not cost what the accepted design costs"
import json, re, statistics, sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]

# Each delivery, with the counters it carries and the runtime's operations that
# came before it since the previous one.
deliveries, operations = [], {}
for line in events:
    m = re.match(r"^TOS\.RUN\.INTERFACE operation=(\w+) status=(-?\d+)$", line)
    if m:
        operations[m.group(1)] = operations.get(m.group(1), 0) + 1
        continue
    m = re.match(r"^TOS\.RUN\.IRQ_DELIVERED .* deliveries=(\d+) woke=1 latched=0 "
                 r"dispatches=(\d+) handoffs=(\d+) idles=(\d+) preemptions=(\d+) "
                 r"asserted_by=nucleus$", line)
    if m:
        deliveries.append((tuple(int(x) for x in m.groups()), operations))
        operations = {}

# The block-protocol boot reaches the device fourteen times: a conformance WRITE,
# a conformance READ, and twelve repeated READs. The two before the repetition are
# the protocol's own; the steady state is every interval between consecutive
# repeated READs.
if len(deliveries) != 14 or [d[0][0] for d in deliveries] != list(range(1, 15)):
    sys.exit("expected fourteen deliveries numbered 1..14, found %d" % len(deliveries))

rows = []
for (prev, _), (cur, ops) in zip(deliveries[2:], deliveries[3:]):
    delta = [c - p for c, p in zip(cur[1:], prev[1:])]
    dispatches, handoffs, idles, preemptions = delta
    rows.append({
        "dispatches": dispatches,
        "handoffs": handoffs,
        "idles": idles,
        "preemptions": preemptions,
        "structural": handoffs - preemptions,
        "region_allocate": ops.get("region_allocate", 0),
        "dma_region_allocate": ops.get("dma_region_allocate", 0),
        "irq_wait": ops.get("irq_wait", 0),
        "endpoint_send_region": ops.get("endpoint_send_region", 0),
    })

problems = []
for n, row in enumerate(rows):
    for key, want in (("region_allocate", 1), ("dma_region_allocate", 0), ("irq_wait", 1),
                      ("endpoint_send_region", 1), ("idles", 1)):
        if row[key] != want:
            problems.append("READ %d: %s=%d, the design's figure is %d" % (n + 1, key, row[key], want))
    # A tick that lands inside a request moves the processor once and may cost
    # one more handoff when the context it moved to gives it back — so the timer
    # adds at most two per preemption, and never takes any away.
    if not 8 <= row["handoffs"] <= 8 + 2 * row["preemptions"]:
        problems.append("READ %d: %d handoffs with %d preemptions is outside 8..8+2p"
                        % (n + 1, row["handoffs"], row["preemptions"]))
# And the design's own figure is what a request the timer did not touch costs.
if min(r["handoffs"] - r["preemptions"] for r in rows) != 8:
    problems.append("no READ costs exactly the design's 8 handoffs once the timer is set aside")
if problems:
    sys.exit("\n".join(problems))

# Host wall-clock between consecutive completions: observational, and said so.
stamps = [json.loads(line) for line in open(sys.argv[2], encoding="utf-8")]
completions = [s["monotonic_ns"] for s in stamps if s["event"] == "TOS.RUN.IRQ_DELIVERED"]
if len(completions) != 14:
    sys.exit("the timestamp record has %d deliveries, not fourteen" % len(completions))
walls = [(b - a) / 1e6 for a, b in zip(completions[2:], completions[3:])]

handoffs = sorted(r["handoffs"] for r in rows)
preempt = sorted(r["preemptions"] for r in rows)
print("  %d steady-state READs of 512 bytes through block.device.v1, each:" % len(rows))
print("    1 region_allocate, 0 dma_region_allocate, 1 routed delivery, 1 idle wait")
print("    handoffs %d..%d: 8 from the design, the rest from %d..%d timer preemptions"
      % (handoffs[0], handoffs[-1], preempt[0], preempt[-1]))
print("    dispatches %d..%d" % (min(r["dispatches"] for r in rows), max(r["dispatches"] for r in rows)))
print("  observational, not a budget: host wall-clock between completions median %.2f ms, "
      "min %.2f, max %.2f (TCG, serial event timestamps, n=%d)"
      % (statistics.median(walls), min(walls), max(walls), len(walls)))
COUNT

echo "stage4-request-cost: PASS (the counts are the design's; docs/35's verdict on them is the Stage 4 performance report's)"
