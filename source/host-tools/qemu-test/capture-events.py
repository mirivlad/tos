#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run QEMU once while timestamping selected existing serial events.

The normal harness owns the QEMU machine profile and verdict. This helper only
copies the serial byte stream to its ordinary log and records host-monotonic
arrival timestamps for the two pre-existing Stage 1 performance boundaries.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import selectors
import subprocess
import sys
import time
from pathlib import Path


EVENT = re.compile(rb"^(TOS\.[A-Z0-9_.]+)(?: |$)")


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial-log", required=True, type=Path)
    parser.add_argument("--stderr-log", required=True, type=Path)
    parser.add_argument("--timestamps", required=True, type=Path)
    parser.add_argument("--timeout", required=True, type=float)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if not args.command:
        parser.error("command is required after --")
    return args


def event_name(line: bytes) -> str | None:
    clean = line.replace(b"\r", b"")
    match = EVENT.match(clean)
    return match.group(1).decode("ascii") if match else None


def record_lines(pending: bytearray, timestamps: list[dict[str, int | str]]) -> None:
    while True:
        end = pending.find(b"\n")
        if end < 0:
            return
        line = bytes(pending[:end])
        del pending[: end + 1]
        event = event_name(line)
        if event is not None:
            timestamps.append({"event": event, "monotonic_ns": time.monotonic_ns()})


def main() -> int:
    args = arguments()
    for path in (args.serial_log, args.stderr_log, args.timestamps):
        path.parent.mkdir(parents=True, exist_ok=True)

    timestamps: list[dict[str, int | str]] = []
    pending = bytearray()
    started = time.monotonic()
    with args.serial_log.open("wb") as serial, args.stderr_log.open("wb") as stderr:
        process = subprocess.Popen(args.command, stdout=subprocess.PIPE, stderr=stderr)
        assert process.stdout is not None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        timed_out = False

        while selector.get_map() or process.poll() is None:
            remaining = args.timeout - (time.monotonic() - started)
            if remaining <= 0:
                timed_out = True
                process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                break

            ready = selector.select(timeout=min(remaining, 0.05))
            for key, _ in ready:
                chunk = os.read(key.fd, 4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                serial.write(chunk)
                serial.flush()
                pending.extend(chunk)
                record_lines(pending, timestamps)

        if pending:
            event = event_name(bytes(pending))
            if event is not None:
                timestamps.append({"event": event, "monotonic_ns": time.monotonic_ns()})
        if not timed_out:
            process.wait()

    with args.timestamps.open("w", encoding="utf-8") as output:
        for record in timestamps:
            output.write(json.dumps(record, sort_keys=True, separators=(",", ":")))
            output.write("\n")
    return 124 if timed_out else process.returncode


if __name__ == "__main__":
    sys.exit(main())
