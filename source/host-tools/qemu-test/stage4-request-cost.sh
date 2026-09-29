#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# What one completed `block.device.v1` READ costs, counted by the nucleus.
#
# **`docs/35` §Stage 4's hard budgets are counts, and this is where they are
# counted** rather than argued. The accepted protocol boot of `block-protocol.sh`
# runs unchanged — its twelve repeated READs are the steady state — on isolated
# measurement nuclei. One counts scheduler activity and reports running totals
# on every routed interrupt delivery; the other buffers a causal trace:
#
#   dispatches   the processor given to a context
#   handoffs     those that give it to a different context than last had it; an
#                entry into and return from idle each count as one transition
#   idles        the machine waiting for an interrupt with nothing runnable
#   preemptions  handoffs made by the timer rather than by a context giving the
#                processor up
#
# IRQ-to-IRQ deltas remain a useful allocation/accounting cross-check, but an
# interval contains the tail of one READ and the head of the next. The separate
# buffered trace below measures each READ from the client's committed call to
# its receipt of the owed Region and archives the later release/completion.
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
#                         one device completion  the READ has one submitted request
#
# H1 follows ADR-0103's amended budget. H3 is decided by the logical-request
# trace, including both directions through idle, not by IRQ-to-IRQ subtraction.
#
# Timer preemptions and their follow-on transitions are retained separately.
# Host wall-clock intervals remain observational, not the reference measurement.
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
    m = re.match(r"^TOS\.RUN\.IRQ_DELIVERED .* deliveries=(\d+) woke=([01]) latched=([01]) "
                 r"dispatches=(\d+) handoffs=(\d+) idles=(\d+) preemptions=(\d+) "
                 r"asserted_by=nucleus$", line)
    if m:
        delivery, woke, latched, *counts = (int(x) for x in m.groups())
        if woke + latched != 1:
            sys.exit("delivery %d neither woke one waiter nor latched one IRQ" % delivery)
        deliveries.append(((delivery, *counts), operations))
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
                      ("endpoint_send_region", 1)):
        if row[key] != want:
            problems.append("READ %d: %s=%d, the design's figure is %d" % (n + 1, key, row[key], want))
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
print("    1 region_allocate, 0 dma_region_allocate, 1 routed delivery")
print("    IRQ-to-IRQ handoffs %d..%d, including %d..%d timer preemptions; not H3 boundaries"
      % (handoffs[0], handoffs[-1], preempt[0], preempt[-1]))
print("    dispatches %d..%d" % (min(r["dispatches"] for r in rows), max(r["dispatches"] for r in rows)))
print("  observational, not a budget: host wall-clock between completions median %.2f ms, "
      "min %.2f, max %.2f (TCG, serial event timestamps, n=%d)"
      % (statistics.median(walls), min(walls), max(walls), len(walls)))
COUNT

echo "stage4-request-cost: allocation cross-check PASS"

# A second isolated nucleus records causal events in a fixed buffer and emits
# them only after all processes end. This avoids serial I/O inside the measured
# request and gives the logical READ its own begin/end markers.
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET/trace" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none \
    --features test-block-protocol,test-request-trace >/dev/null 2>&1) ||
    fail "the isolated causal-trace nucleus does not build"
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
    fail "the production nucleus changed while building the trace artifact"
bash "$HERE/run.sh" \
    --out "$OUT/trace-boot" \
    --capsule "$OUT/capsule.bin" \
    --nucleus "$TARGET/trace/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.IRQ_DELIVERED TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED TOS.TRACE.OVERFLOW" \
    > /dev/null || fail "the causal-trace boot did not complete"
python3 "$HERE/analyze-stage4-read-trace.py" \
    "$OUT/trace-boot/events.log" "$OUT/logical-read-trace.json" ||
    fail "logical READ attribution failed"

echo "stage4-request-cost: PASS (H1 within ADR-0103, H3 structural skeleton <= 4)"
