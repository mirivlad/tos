#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check every retained logical READ from client commit through completion."""

import json
import re
import sys
from pathlib import Path


def fail(message):
    raise SystemExit(f"stage4-read-trace: FAIL: {message}")


lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
if any("TOS.TRACE.OVERFLOW" in line for line in lines):
    fail("the fixed trace buffer overflowed")
records = [dict(re.findall(r"(\w+)=([^ ]+)", line)) for line in lines
           if line.startswith("TOS.TRACE sequence=")]
if [int(r["sequence"]) for r in records] != list(range(1, len(records) + 1)):
    fail("trace sequence has a gap or duplicate")

# Roles come from the boot's own records, not from launch order. The service is
# the one process the nucleus assigned the block function to; the client is the
# one context that received the Regions the service sent.
assigned = {m.group(1) for line in lines
            for m in [re.match(r"^TOS\.RUN\.PCI_ASSIGNED process=(\d+) ", line)] if m}
if len(assigned) != 1:
    fail(f"expected one process holding the block function, found {sorted(assigned)}")
SERVICE = assigned.pop()
receivers = set()
for i, r in enumerate(records):
    if r["kind"] == "marker" and r["reason"] == "region_send_committed" and r["from"] == SERVICE:
        after = [q for q in records[i + 1:] if q["kind"] == "marker"
                 and q["reason"] == "region_received"]
        if not after:
            fail(f"service Region send at sequence {r['sequence']} was never received")
        receivers.add(after[0]["from"])
if len(receivers) != 1 or SERVICE in receivers:
    fail(f"expected one client receiving the service's Regions, found {sorted(receivers)}")
CLIENT = receivers.pop()


def marker(record, label, actor):
    return record["kind"] == "marker" and record["reason"] == label and record["from"] == actor


# A logical READ runs from the client's committed call to the client's release
# of the Region it received. Calls that never bring a Region back (the WRITE
# conformance requests) are not READs and are not retained.
begins = [i for i, r in enumerate(records) if marker(r, "request_committed", CLIENT)]
paired = []
for ordinal, begin in enumerate(begins):
    limit = begins[ordinal + 1] if ordinal + 1 < len(begins) else len(records)
    received = [i for i in range(begin + 1, limit)
                if marker(records[i], "region_received", CLIENT)]
    if not received:
        continue
    released = [i for i in range(received[0] + 1, limit)
                if marker(records[i], "region_released", CLIENT)]
    if released:
        paired.append((begin, released[0]))
if len(paired) < 12:
    fail(f"expected twelve completed client READs, found {len(paired)}")
retained = paired[-12:]
required_markers = [
    ("request_committed", CLIENT),
    ("message_received", SERVICE),
    ("device_request_submitted", SERVICE),
    ("irq_completion", "idle"),
    ("success_reply_produced", SERVICE),
    ("region_send_committed", SERVICE),
    ("region_received", CLIENT),
    ("region_released", CLIENT),
]
samples = []
for number, (begin, complete) in enumerate(retained, 1):
    window = records[begin:complete + 1]
    positions = []
    for label, actor in required_markers:
        hits = [i for i, r in enumerate(window) if r["kind"] == "marker"
                and r["reason"] == label and r["from"] == actor]
        if len(hits) != 1:
            fail(f"READ {number}: {label} by {actor} occurs {len(hits)} times")
        positions.append(hits[0])
    if positions != sorted(positions):
        fail(f"READ {number}: lifecycle markers are out of order")

    # BLOCK_DEVICE_V1 completes when the client owns the owed Region. Byte
    # validation, release, and (on the last fixture request) service teardown
    # remain archived after that boundary but are not block-service handoffs.
    protocol_window = window[:positions[6] + 1]
    charged = [r for r in protocol_window if r["charged"] == "1"
               and r["kind"] in ("dispatch", "idle_enter")]
    timers = [r for r in charged if r["timer"] == "1"]
    structural = [r for r in charged if r["timer"] == "0"]

    def matching(kind, source, target, reason, before=None, after=None):
        return [i for i, r in enumerate(protocol_window)
                if r["kind"] == kind and r["from"] == source
                and r["to"] == target and r["reason"] == reason
                and r["charged"] == "1"
                and r["timer"] == "0"
                and (before is None or i < before)
                and (after is None or i > after)]

    first = matching("dispatch", CLIENT, SERVICE, "caller_reply_wait", before=positions[2])
    idle = matching("idle_enter", SERVICE, "idle", "service_irq_wait", after=positions[2])
    resume = matching("dispatch", "idle", SERVICE, "resume_after_idle", after=positions[3])
    final = matching("dispatch", SERVICE, CLIENT, "receiver_request_or_region_wait",
                     after=positions[5])
    if len(first) != 1 or len(idle) != 1 or len(resume) != 1 or len(final) > 1:
        fail(f"READ {number}: expected client block, idle entry, IRQ resume, and at most one final service block")
    mandatory = {first[0], idle[0], resume[0]}
    if final:
        mandatory.add(final[0])
    extra = [(i, r) for i, r in enumerate(protocol_window)
             if r["kind"] in ("dispatch", "idle_enter") and r["charged"] == "1"
             and r["timer"] == "0" and i not in mandatory]
    # A timer can run the client after the success reply but before the Region
    # send. Its receive then blocks, creating one client -> service transition
    # which the unpreempted path does not have. Check that exact causal order;
    # a generic "some timer happened" exemption would hide unrelated work.
    for i, r in extra:
        early_timer = any(t["kind"] == "dispatch" and t["timer"] == "1"
                          and t["from"] == SERVICE and t["to"] == CLIENT
                          for t in protocol_window[positions[4] + 1:i])
        if not (r["kind"] == "dispatch" and r["from"] == CLIENT
                and r["to"] == SERVICE and r["reason"] == "receiver_request_or_region_wait"
                and positions[4] < i < positions[5] and early_timer):
            fail(f"READ {number}: unexplained non-timer handoff at sequence {r['sequence']}")
    if not timers and (len(structural) != 4 or len(mandatory) != 4):
        fail(f"READ {number}: timer-free request has {len(structural)} handoffs, expected four")
    samples.append({
        "read": number,
        "begin_sequence": int(window[0]["sequence"]),
        "complete_sequence": int(window[-1]["sequence"]),
        "protocol_complete_sequence": int(protocol_window[-1]["sequence"]),
        "structural_skeleton": len(mandatory),
        "timer_handoffs": len(timers),
        "timer_interleaved_extra": len(extra),
        "raw_charged_handoffs": len(charged),
        "events": window,
    })

Path(sys.argv[2]).write_text(json.dumps({
    "record_spdx_license": "CC-BY-SA-4.0",
    "service_process": int(SERVICE),
    "client_process": int(CLIENT),
    "samples": samples,
}, indent=2) + "\n", encoding="utf-8")
print(f"stage4-read-trace: PASS ({len(samples)} logical READs, "
      f"{sum(s['timer_handoffs'] == 0 for s in samples)} timer-free, "
      f"structural skeleton <= 4; raw events in {sys.argv[2]})")
