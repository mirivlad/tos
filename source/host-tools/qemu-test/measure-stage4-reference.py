#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Drive one ADR-0103 reference boot and read its windows from the Stage 4 observer.

One program for both sides of every ratio. The TOS measurement boot and the Rust
oracle boot run the same plan, emit the same ADR-0066 markers on the same COM1, and
are read by the same observer build through this same code, so nothing that differs
between the two reports can come from the instrument.

**The protocol.** The guest finishes its warm-up and writes READY (0xff). This
program enables the observer's `tos_block_window` trace event over QMP and only
then answers GO (0xc0); the guest then writes 21 empty floor pairs, the three R1
windows and the 303 R2 operations, each as `OPEN | tag` ... `CLOSE | tag`. After the
last CLOSE this program disables the event and only then sends STOP (0xe0), so
boot traffic before READY and the guest's report after the plan never reach the
trace, and a guest that waits for STOP (the oracle does) cannot end the machine
while the event is still being disarmed.

**Where the clocks are.** Inside QEMU, not here. The observer reads
`CLOCK_MONOTONIC_RAW` and the whole QEMU process's `CLOCK_PROCESS_CPUTIME_ID` in
the vCPU thread after handling OPEN and before handling CLOSE, and emits all four
raw values in one record. This program never timestamps a marker; it decodes the
record, checks it against the predeclared plan, and computes. Nothing is
corrected, subtracted, filtered, retried or reordered, and a series that fails
any check is invalid as a whole.

**The serial log it keeps is the text.** After READY every byte with the high bit
set is a marker (the guest's text is ASCII), and it is recorded in the marker list
instead of the log: a marker glued to the front of a `TOS.` line would otherwise
hide that line from every reader of `events.log`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path

GO = 0xC0
STOP = 0xE0
READY = 0xFF
OPEN = 0x80
CLOSE = 0xA0
FAMILY = 0xE0
WORK = 0x10
SEQUENCE = 0x0F

FLOOR_PAIRS = 21
R1_WINDOWS = 3
R1_WINDOW_READS = 2048
R2_OPERATIONS = 303
R2_WARMUPS = 3
R2_READS_PER_OPERATION = 8
SECTOR_BYTES = 512
MIB = 1024 * 1024
UNSET = 0xFFFFFFFFFFFFFFFF

TRACE_EVENT = "tos_block_window"
SIMPLE_HEADER_EVENT_ID = 0xFFFFFFFFFFFFFFFF
SIMPLE_DROPPED_EVENT_ID = 0xFFFFFFFFFFFFFFFE
SIMPLE_HEADER_MAGIC = 0xF2B177CB0AA429B4
SIMPLE_HEADER_VERSION = 4
SIMPLE_MAPPING_RECORD = 0
SIMPLE_EVENT_RECORD = 1


class Invalid(Exception):
    """The series is not evidence; the reason says why."""


def plan() -> list[tuple[str, int, int]]:
    """Every pair the guest must emit, in order: (series, index, tag)."""
    pairs = [("floor", i, i & SEQUENCE) for i in range(FLOOR_PAIRS)]
    pairs += [("r1", w, WORK | (w & SEQUENCE)) for w in range(R1_WINDOWS)]
    pairs += [("r2", k, WORK | (k & SEQUENCE)) for k in range(R2_OPERATIONS)]
    return pairs


def socket_to(path: Path, qemu: subprocess.Popen, deadline: float, what: str) -> socket.socket:
    """Connects to one of QEMU's sockets, failing as soon as QEMU has ended."""
    while True:
        try:
            connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            connection.connect(str(path))
            return connection
        except OSError:
            connection.close()
            if qemu.poll() is not None:
                raise Invalid(f"QEMU ended with {qemu.returncode} before offering its {what}")
            if time.monotonic() > deadline:
                raise Invalid(f"QEMU never offered its {what}")
            time.sleep(0.01)


class Qmp:
    def __init__(self, path: Path, qemu: subprocess.Popen, deadline: float) -> None:
        self.next_id = 1
        connection = socket_to(path, qemu, deadline, "QMP socket")
        self.stream = connection.makefile("rwb", buffering=0)
        if b"QMP" not in self.stream.readline():
            raise Invalid("QMP did not greet")
        self.execute("qmp_capabilities", {})

    def execute(self, command: str, arguments: dict[str, object]) -> None:
        try:
            self._execute(command, arguments)
        except OSError as error:
            raise Invalid(f"QMP {command} failed: {error}") from error

    def _execute(self, command: str, arguments: dict[str, object]) -> None:
        identity = self.next_id
        self.next_id += 1
        self.stream.write(json.dumps({"execute": command, "arguments": arguments,
                                      "id": identity}).encode() + b"\n")
        while True:
            line = self.stream.readline()
            if not line:
                raise Invalid(f"QMP closed during {command}")
            answer = json.loads(line)
            if answer.get("id") != identity:
                continue
            if "error" in answer:
                raise Invalid(f"QMP {command} failed: {answer['error']}")
            return

    def trace(self, enabled: bool) -> None:
        self.execute("trace-event-set-state", {"name": TRACE_EVENT, "enable": enabled})


def drive(args: argparse.Namespace, text: bytearray, markers: list[int]) -> int:
    """Runs QEMU through the protocol and returns its exit code.

    The text and the markers are the caller's, so they survive an invalid series:
    a run that fails is diagnosed from exactly what it said.
    """
    deadline = time.monotonic() + args.timeout
    stderr = open(args.stderr_log, "wb")
    qemu = subprocess.Popen(args.command, stdout=subprocess.DEVNULL, stderr=stderr)
    try:
        wire = socket_to(args.socket, qemu, deadline, "serial socket")
        wire.setblocking(False)
        qmp = Qmp(args.qmp_socket, qemu, deadline)
        state = "boot"
        closes = 0
        expected_closes = len(plan())
        while True:
            if time.monotonic() > deadline:
                raise Invalid(f"timed out in state {state} after {closes} CLOSE marker(s)")
            try:
                chunk = wire.recv(65536)
            except BlockingIOError:
                if qemu.poll() is not None:
                    break
                time.sleep(0.0005)
                continue
            except ConnectionResetError:
                break
            if not chunk:
                break
            for byte in chunk:
                if state == "boot":
                    if byte == READY:
                        qmp.trace(True)
                        wire.setblocking(True)
                        wire.sendall(bytes([GO]))
                        wire.setblocking(False)
                        state = "plan"
                        markers.append(byte)
                    else:
                        text.append(byte)
                elif byte & 0x80:
                    markers.append(byte)
                    if state == "plan" and byte & FAMILY == CLOSE:
                        closes += 1
                        if closes == expected_closes:
                            qmp.trace(False)
                            wire.setblocking(True)
                            wire.sendall(bytes([STOP]))
                            wire.setblocking(False)
                            state = "report"
                else:
                    text.append(byte)
        code = qemu.wait(timeout=max(1.0, deadline - time.monotonic()))
        if state != "report":
            raise Invalid(f"the guest stopped in state {state} after {closes} CLOSE marker(s)")
        return code
    finally:
        if qemu.poll() is None:
            qemu.kill()
            qemu.wait()
        stderr.close()


def decode_trace(path: Path) -> list[tuple[int, int, int, int, int, int]]:
    """Independently decodes QEMU simple trace v4; accepts only the window event.

    Total over arbitrary bytes: anything malformed is `Invalid`, never a crash.
    """
    try:
        return _decode_trace(path.read_bytes())
    except (struct.error, UnicodeDecodeError) as error:
        raise Invalid(f"malformed simple trace: {error}") from error


def _decode_trace(data: bytes) -> list[tuple[int, int, int, int, int, int]]:
    if len(data) < 24:
        raise Invalid("truncated simple trace header")
    header_id, magic, version = struct.unpack_from("=QQQ", data)
    if (header_id, magic, version) != (SIMPLE_HEADER_EVENT_ID, SIMPLE_HEADER_MAGIC,
                                       SIMPLE_HEADER_VERSION):
        raise Invalid("not a QEMU simple trace v4")
    offset, mappings, windows = 24, {}, []
    while offset < len(data):
        if len(data) - offset < 8:
            raise Invalid("truncated record type")
        kind = struct.unpack_from("=Q", data, offset)[0]
        offset += 8
        if kind == SIMPLE_MAPPING_RECORD:
            event_id, length = struct.unpack_from("=QL", data, offset)
            offset += 12
            name = data[offset:offset + length].decode("utf-8")
            offset += length
            if event_id in mappings:
                raise Invalid(f"duplicate mapping {name}")
            mappings[event_id] = name
            continue
        if kind != SIMPLE_EVENT_RECORD:
            raise Invalid(f"unknown record type {kind}")
        if len(data) - offset < 24:
            raise Invalid("truncated event header")
        event_id, _stamp, length, _pid = struct.unpack_from("=QQII", data, offset)
        if length < 24 or len(data) - offset < length:
            raise Invalid("truncated event")
        payload = data[offset + 24:offset + length]
        offset += length
        if event_id == SIMPLE_DROPPED_EVENT_ID:
            if struct.unpack("=Q", payload)[0]:
                raise Invalid("the trace dropped events")
            continue
        if mappings.get(event_id) != TRACE_EVENT:
            raise Invalid(f"unexpected trace event {mappings.get(event_id)!r}")
        if len(payload) != 48:
            raise Invalid("malformed window payload")
        windows.append(struct.unpack("=QQQQQQ", payload))
    if TRACE_EVENT not in mappings.values():
        raise Invalid("the trace has no window event mapping")
    return windows


def nearest_rank(values: list[int], rank: int) -> int:
    return sorted(values)[rank - 1]


def summarize(windows: list[tuple[int, int, int, int, int, int]]) -> dict[str, object]:
    expected = plan()
    if len(windows) != len(expected):
        raise Invalid(f"{len(windows)} window record(s), the plan has {len(expected)}")
    series: dict[str, list[dict[str, int]]] = {"floor": [], "r1": [], "r2": []}
    for (name, index, tag), record in zip(expected, windows):
        opened, closed, open_raw, close_raw, open_cpu, close_cpu = record
        if opened != OPEN | tag or closed != CLOSE | tag:
            raise Invalid(f"{name}[{index}]: markers 0x{opened:02x}/0x{closed:02x}, "
                          f"plan 0x{OPEN | tag:02x}/0x{CLOSE | tag:02x}")
        if UNSET in (open_raw, open_cpu):
            raise Invalid(f"{name}[{index}]: an OPEN was not paired")
        if close_raw <= open_raw or close_cpu < open_cpu:
            raise Invalid(f"{name}[{index}]: a clock ran backwards or stood still")
        series[name].append({"elapsed_ns": close_raw - open_raw,
                             "cpu_ns": close_cpu - open_cpu,
                             "open_raw_ns": open_raw, "close_raw_ns": close_raw,
                             "open_cpu_ns": open_cpu, "close_cpu_ns": close_cpu})
    floor = [s["elapsed_ns"] for s in series["floor"]]
    r1_bytes = R1_WINDOWS * R1_WINDOW_READS * SECTOR_BYTES
    r1_elapsed = sum(s["elapsed_ns"] for s in series["r1"])
    r1_cpu = sum(s["cpu_ns"] for s in series["r1"])
    retained = [s["elapsed_ns"] for s in series["r2"][R2_WARMUPS:]]
    return {
        "floor_elapsed_ns": {"n": len(floor), "median": nearest_rank(floor, (len(floor) + 1) // 2),
                             "max": max(floor), "samples": floor},
        "r1": {"windows": len(series["r1"]), "bytes": r1_bytes,
               "elapsed_ns": r1_elapsed, "cpu_ns": r1_cpu,
               "throughput_bytes_per_s": r1_bytes * 1e9 / r1_elapsed,
               "window_elapsed_ns": [s["elapsed_ns"] for s in series["r1"]]},
        "r2": {"operations": len(series["r2"]), "warmups": R2_WARMUPS,
               "retained": len(retained),
               "p99_elapsed_ns": nearest_rank(retained, 297),
               "median_elapsed_ns": nearest_rank(retained, 150),
               "retained_elapsed_ns": retained},
        "r3": {"cpu_ns_per_mib": r1_cpu * MIB / r1_bytes},
        "raw": series,
    }


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--qmp-socket", required=True, type=Path)
    parser.add_argument("--serial-log", required=True, type=Path)
    parser.add_argument("--stderr-log", required=True, type=Path)
    parser.add_argument("--trace", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--timeout", required=True, type=float)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    if not args.command:
        parser.error("the QEMU command is required after --")
    return args


def main() -> int:
    args = arguments()
    text = bytearray()
    markers: list[int] = []
    try:
        code = drive(args, text, markers)
        summary = summarize(decode_trace(args.trace))
    except Invalid as error:
        print(f"measure-stage4-reference: INVALID: {error}", file=sys.stderr)
        return 3
    finally:
        args.serial_log.write_bytes(bytes(text))
    qemu = Path(subprocess.run(["sh", "-c", f"command -v {args.command[0]}"], check=True,
                               text=True, stdout=subprocess.PIPE).stdout.strip())
    manifest = qemu.parent / "observer-build.json"
    report = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "qemu_exit": code,
        "markers_seen": len(markers),
        "observer_build": json.loads(manifest.read_text()) if manifest.is_file() else None,
        "observer_build_sha256": (hashlib.sha256(manifest.read_bytes()).hexdigest()
                                  if manifest.is_file() else None),
        "qemu_command": args.command,
        **summary,
    }
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"measure-stage4-reference: exit={code} R1 {summary['r1']['throughput_bytes_per_s']:.0f} B/s "
          f"R2 p99 {summary['r2']['p99_elapsed_ns'] / 1e6:.3f} ms "
          f"R3 {summary['r3']['cpu_ns_per_mib'] / 1e9:.3f} s CPU/MiB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
