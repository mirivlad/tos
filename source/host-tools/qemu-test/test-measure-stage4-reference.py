#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Self-test of the ADR-0103 window decoder against synthetic simple traces.

The decoder is what turns the observer's records into R1-R3, so it is checked on
its own: a trace that follows the plan is accepted and computed exactly, and each
way a trace can be wrong — a wrong tag, a missing window, an unpaired OPEN, a clock
that stands still, a dropped event, an event the observer does not emit — makes the
whole series invalid rather than a slightly different number.
"""

import importlib.util
import struct
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
# Importing the driver must not leave bytecode in the source tree, where the
# SPDX gate would find an unlicensed file.
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("measure", HERE / "measure-stage4-reference.py")
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)

EVENT_ID = 7
OTHER_ID = 8


def trace(windows, *, dropped=0, foreign=False):
    data = bytearray(struct.pack("=QQQ", measure.SIMPLE_HEADER_EVENT_ID,
                                 measure.SIMPLE_HEADER_MAGIC, measure.SIMPLE_HEADER_VERSION))
    for event_id, name in ((EVENT_ID, measure.TRACE_EVENT), (OTHER_ID, "serial_write")):
        encoded = name.encode()
        data += struct.pack("=QQL", measure.SIMPLE_MAPPING_RECORD, event_id, len(encoded)) + encoded
    for record in windows:
        payload = struct.pack("=QQQQQQ", *record)
        data += struct.pack("=QQQII", measure.SIMPLE_EVENT_RECORD, EVENT_ID, 0,
                            24 + len(payload), 0) + payload
    if dropped:
        data += struct.pack("=QQQII", measure.SIMPLE_EVENT_RECORD,
                            measure.SIMPLE_DROPPED_EVENT_ID, 0, 32, 0) + struct.pack("=Q", dropped)
    if foreign:
        data += struct.pack("=QQQII", measure.SIMPLE_EVENT_RECORD, OTHER_ID, 0, 40, 0)
        data += struct.pack("=QQ", 0, 0x41)
    return bytes(data)


def planned():
    """A plan-conforming series: floor 100 ns, R1 windows 1 s, R2 operation k (k+1) us."""
    records, clock = [], 1_000
    for name, index, tag in measure.plan():
        elapsed = {"floor": 100, "r1": 1_000_000_000}.get(name, (index + 1) * 1_000)
        records.append((measure.OPEN | tag, measure.CLOSE | tag,
                        clock, clock + elapsed, clock * 2, clock * 2 + 2 * elapsed))
        clock += elapsed + 10
    return records


def decode(data):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "trace"
        path.write_bytes(data)
        return measure.summarize(measure.decode_trace(path))


def invalid(data, why):
    try:
        decode(data)
    except measure.Invalid:
        return
    sys.exit(f"test-measure-stage4-reference: FAIL: accepted {why}")


summary = decode(trace(planned()))
r1_bytes = 3 * 2048 * 512
assert summary["r1"]["bytes"] == r1_bytes
assert summary["r1"]["elapsed_ns"] == 3_000_000_000
assert summary["r1"]["throughput_bytes_per_s"] == r1_bytes / 3
assert summary["r3"]["cpu_ns_per_mib"] == 6_000_000_000 * 1024 * 1024 / r1_bytes
# Retained operations are 3..302, lasting 4..303 us; nearest rank 297 is 300 us.
assert summary["r2"]["retained"] == 300
assert summary["r2"]["p99_elapsed_ns"] == 300_000
assert summary["floor_elapsed_ns"]["median"] == 100

good = planned()
wrong_tag = list(good)
wrong_tag[30] = (wrong_tag[30][0] ^ 1,) + wrong_tag[30][1:]
invalid(trace(wrong_tag), "a window whose tag the plan did not predict")
invalid(trace(good[:-1]), "a series one window short")
unpaired = list(good)
unpaired[25] = unpaired[25][:2] + (measure.UNSET,) + unpaired[25][3:]
invalid(trace(unpaired), "an OPEN the observer could not pair")
still = list(good)
still[40] = still[40][:3] + (still[40][2],) + still[40][4:]
invalid(trace(still), "an interval of zero")
invalid(trace(good, dropped=1), "a trace that dropped an event")
invalid(trace(good, foreign=True), "an event the Stage 4 observer does not emit")
invalid(trace(good)[:-5], "a truncated record")
header_only = trace([])[:24]
invalid(header_only + struct.pack("=Q", measure.SIMPLE_MAPPING_RECORD) + b"\x07",
        "a truncated mapping")
print("test-measure-stage4-reference: PASS (plan accepted and computed; 8 corruptions refused)")
