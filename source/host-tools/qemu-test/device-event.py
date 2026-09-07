#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run QEMU once, and make a real device event happen while the guest waits.

**What this is for, and what it deliberately is not.** ADR-0082 §13 requires the
positive evidence to show a *real* MSI-X interrupt from the real device reaching
a process through the authority it holds — "no host-injected boolean, and no
fixture supplying the answer". So this helper never writes to the guest, never
touches the serial line, and produces no value the guest can read. It waits for
the **guest's own** announcement that it is blocked with nothing runnable, and
then asks QEMU to change the device: a disk is resized, which is an ordinary
external event a block device reports to its driver.

Everything after that is the machine's: the device raises its configuration-
change interrupt, the message lands on the vector the nucleus programmed, the
handler finds the source, and the source's waiter is woken. Nothing in this
program is on that path.

The QEMU machine profile is the harness's and is untouched; what is added is a
QMP socket, which no guest-visible device.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import selectors
import socket
import subprocess
import sys
import time
from pathlib import Path


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial-log", required=True, type=Path)
    parser.add_argument("--stderr-log", required=True, type=Path)
    parser.add_argument("--qmp-socket", required=True, type=Path)
    parser.add_argument("--timeout", required=True, type=float)
    parser.add_argument(
        "--await-line",
        required=True,
        help="a regular expression the guest emits when it is ready to be woken",
    )
    parser.add_argument(
        "--then",
        required=True,
        action="append",
        help="a QMP command object, as JSON; repeatable and sent in order",
    )
    parser.add_argument(
        "--between",
        type=float,
        default=0.5,
        help="seconds between successive commands",
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    if not args.command:
        parser.error("command is required after --")
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    return args


class Monitor:
    """The QMP side, opened only when the guest says it is waiting."""

    def __init__(self, path: Path, deadline: float) -> None:
        self.path = path
        self.deadline = deadline
        self.stream: socket.SocketIO | None = None
        self.sock: socket.socket | None = None

    def connect(self) -> None:
        while time.monotonic() < self.deadline:
            try:
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.connect(str(self.path))
            except OSError:
                time.sleep(0.05)
                continue
            self.sock = sock
            self.stream = sock.makefile("rwb")
            # The greeting, then the handshake QMP requires before any command.
            self.read()
            self.send({"execute": "qmp_capabilities"})
            return
        raise SystemExit(f"device-event: no QMP monitor at {self.path}")

    def read(self) -> dict:
        assert self.stream is not None
        while True:
            line = self.stream.readline()
            if not line:
                raise SystemExit("device-event: the QMP monitor closed")
            message = json.loads(line)
            # Asynchronous events are not answers; keep reading for one.
            if "event" in message:
                continue
            return message

    def send(self, command: dict) -> dict:
        assert self.stream is not None
        self.stream.write((json.dumps(command) + "\n").encode("utf-8"))
        self.stream.flush()
        answer = self.read()
        if "error" in answer:
            raise SystemExit(
                f"device-event: QMP refused {command['execute']}: {answer['error']}"
            )
        return answer


def main() -> int:
    args = arguments()
    pattern = re.compile(args.await_line.encode("utf-8"))
    args.serial_log.parent.mkdir(parents=True, exist_ok=True)
    commands = [json.loads(text) for text in args.then]

    read, write = os.pipe()
    with (
        open(args.serial_log, "wb") as serial,
        open(args.stderr_log, "wb") as stderr,
        os.fdopen(read, "rb", buffering=0) as source,
    ):
        process = subprocess.Popen(
            args.command + ["-serial", "stdio", "-display", "none"],
            stdin=subprocess.DEVNULL,
            stdout=write,
            stderr=stderr,
        )
        os.close(write)
        deadline = time.monotonic() + args.timeout
        monitor = Monitor(args.qmp_socket, deadline)
        selector = selectors.DefaultSelector()
        selector.register(source, selectors.EVENT_READ)
        pending = bytearray()
        fired = 0
        next_at = 0.0
        try:
            while True:
                if process.poll() is not None and not selector.get_map():
                    break
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    process.kill()
                    print("device-event: timed out", file=sys.stderr)
                    return 124
                for _key, _events in selector.select(timeout=min(remaining, 0.2)):
                    chunk = source.read(65536)
                    if not chunk:
                        selector.unregister(source)
                        break
                    serial.write(chunk)
                    serial.flush()
                    pending.extend(chunk)
                # The guest's own statement that it is blocked and only hardware
                # can change that. It is read from the ordinary serial log, and
                # matching it sends nothing back.
                if fired < len(commands) and time.monotonic() >= next_at:
                    if fired > 0 or pattern.search(bytes(pending)):
                        if fired == 0:
                            monitor.connect()
                        monitor.send(commands[fired])
                        fired += 1
                        next_at = time.monotonic() + args.between
                if process.poll() is not None and not selector.get_map():
                    break
        finally:
            selector.close()
            if process.poll() is None:
                try:
                    process.wait(timeout=max(0.0, deadline - time.monotonic()))
                except subprocess.TimeoutExpired:
                    process.kill()
        if fired < len(commands):
            print(
                "device-event: the guest never announced that it was waiting "
                f"({args.await_line!r}); no device event was made",
                file=sys.stderr,
            )
            return 1
    return process.returncode if process.returncode is not None else 1


if __name__ == "__main__":
    raise SystemExit(main())
