#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The external instrument of ADR-0066, and first of all a check of itself.

The measurement clock is QEMU's trace timestamp at the device model. The
host-side reader also records `CLOCK_MONOTONIC` arrival times, but only as a
diagnostic comparison and never as the measurement. Neither is a facility of
TOS: the guest is never told what time it is, and nothing here becomes part of
the system's semantics, capability surface or ABI. The system is measured the
way a circuit is measured — the oscilloscope does not become a component.

**The protocol.** The observer sends `GO | tag`, the observed process does the
thing selected by the tag, and answers `OPEN | tag`, `CLOSE | tag`. The echo is
what makes a sample causal rather than coincidental: an answer that does not
name the exact planned request invalidates the whole series.

**Where the clock is.** On QEMU's side of the wire, not this one. The diagnostic
`log` backend prefixes events with a microsecond `gettimeofday` value.  The
pinned conformance observer uses integer nanoseconds from
`CLOCK_THREAD_CPUTIME_ID` in the sole TCG vCPU thread. It records OPEN after the
UART has handled that marker and CLOSE before the UART handles it, then emits
both untouched timestamps in one binary `simple` trace record. Nothing this
program does with its own scheduling can move either boundary. The socket is
kept for the *protocol*: it carries requests, replies and stop, and is not the
clock.

That matters because the obvious alternative is wrong in a direction that
flatters the system. A reader on this side stamps a marker when it manages to
read it; if it is late to `OPEN` while the guest is already working, the interval
it reports is **shorter** than the truth. An instrument that errs towards
passing is not an instrument.

**What a reading contains.** `t(CLOSE) - t(OPEN)`, both taken inside QEMU: all
vCPU execution after the opening marker is delivered and before the closing
marker is delivered. No transport cost, calibration value or floor is
subtracted. The empty interval is measured by the same instrument and published
beside the result; it demonstrates resolution but never corrects a sample.

**Why the reader still spins.** It no longer times anything, but it must not be
the reason a sample is late: the next request is sent when the previous answer is
seen, so a sluggish reader would stretch the gaps *between* samples. Reading one
byte at a time keeps that immediate and keeps the ordinary boot log intact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import statistics
import struct
import subprocess
import sys
import time
from pathlib import Path

GO = 0xC0
OPEN = 0x80
CLOSE = 0xA0
WORK = 0x10
SEQUENCE = 0x0F
TAG = WORK | SEQUENCE
STOP = 0xE0
READY = 0xFF
WARMUPS = 3
# Not a protocol limit; see the note where it is checked.
MAX_SAMPLES = 2000
SIMPLE_HEADER_EVENT_ID = 0xFFFFFFFFFFFFFFFF
SIMPLE_DROPPED_EVENT_ID = 0xFFFFFFFFFFFFFFFE
SIMPLE_HEADER_MAGIC = 0xF2B177CB0AA429B4
SIMPLE_HEADER_VERSION = 4
SIMPLE_MAPPING_RECORD = 0
SIMPLE_EVENT_RECORD = 1
SIMPLE_TRACE_CLOCK = "CLOCK_THREAD_CPUTIME_ID pair on the one TCG vCPU thread"
OBSERVER_SERIAL_SOURCE_SHA256 = (
    "46548454bc48e12b430795fc69cb19f0349bbef3a63ee37c23aa365713978b91"
)
OBSERVER_SERIAL_MODIFIED_SHA256 = (
    "5fb72ef50b75f630e68260c487760d5ad99f4fba28ba1bf573439abc4fe7a876"
)
OBSERVER_EVENTS_SOURCE_SHA256 = (
    "64f70f77897a5e52957f12d55dcb5b0d09f692a56ed70afb757f5f8f5d16e364"
)
OBSERVER_EVENTS_MODIFIED_SHA256 = (
    "7828c2cf29a8ecbc9da05210a29b6132efdc3215d9a72df21cae4841fdb0d466"
)
SIMPLE_OBSERVER_CONFIGURE = [
    "--prefix=/",
    "--target-list=x86_64-softmmu",
    "--enable-trace-backends=simple",
    "--enable-fdt=disabled",
    "--disable-download",
    "--disable-docs",
    "--disable-tools",
    "--disable-guest-agent",
    "--disable-slirp",
    "--disable-plugins",
    "--disable-vnc",
    "--disable-gtk",
    "--disable-sdl",
    "--disable-werror",
    "--disable-debug-info",
]
SIMPLE_CFLAGS_IDENTITY = (
    "-O2 plus file, debug and macro prefix maps from the temporary build root "
    "to /usr/src/qemu-10.0.11"
)


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--qmp-socket", required=True, type=Path)
    parser.add_argument("--serial-log", required=True, type=Path)
    parser.add_argument("--stderr-log", required=True, type=Path)
    parser.add_argument("--samples", required=True, type=int)
    parser.add_argument("--evidence-status", choices=("P1", "P2"), default="P1")
    parser.add_argument("--timeout", required=True, type=float)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--trace", type=Path)
    parser.add_argument("--repository", required=True, type=Path)
    parser.add_argument("--capsule", required=True, type=Path)
    parser.add_argument("--firmware-code", required=True, type=Path)
    parser.add_argument("--firmware-vars", required=True, type=Path)
    parser.add_argument("--loader", required=True, type=Path)
    parser.add_argument("--nucleus", required=True, type=Path)
    parser.add_argument("--runtime-image", required=True, type=Path)
    parser.add_argument("--production-nucleus", required=True, type=Path)
    parser.add_argument("--production-runtime-image", required=True, type=Path)
    parser.add_argument("--production-nucleus-before-sha256")
    parser.add_argument("--production-runtime-image-before-sha256")
    parser.add_argument("--quantum-source", required=True, type=Path)
    parser.add_argument("--measurement-build-manifest", type=Path)
    parser.add_argument("--paired-calibration", action="store_true")
    parser.add_argument("--ipc-measurement", action="store_true")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    if not args.command:
        parser.error("the QEMU command is required after --")
    # The four-bit sequence used to bound the series at one tag per sample.
    # ADR-0068 section 5 makes a latency series 300, which turns the tag space
    # over eighteen times; what makes that admissible is the predeclared exact
    # plan, which says where every repeat belongs and refuses one anywhere else.
    # The remaining bound is a sanity limit rather than a property of the
    # protocol: a series this long already takes minutes under TCG.
    if args.samples < 1 or args.samples > MAX_SAMPLES:
        parser.error(f"--samples must be 1..{MAX_SAMPLES}")
    if args.paired_calibration and args.ipc_measurement:
        parser.error("--paired-calibration and --ipc-measurement are exclusive")
    if (
        args.paired_calibration or args.ipc_measurement
    ) and args.measurement_build_manifest is None:
        parser.error("the selected measurement mode requires --measurement-build-manifest")
    return args


class Wire:
    """The socket, read by spinning and written one byte at a time."""

    def __init__(self, path: Path, deadline: float) -> None:
        self.log = bytearray()
        self.queue: list[tuple[int, int]] = []
        self.socket = None
        while time.monotonic() < deadline:
            try:
                connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                connection.connect(str(path))
            except (FileNotFoundError, ConnectionRefusedError, OSError):
                time.sleep(0.01)
                continue
            connection.setblocking(False)
            self.socket = connection
            return
        raise Invalid("QEMU never offered its serial socket")

    def send(self, byte: int) -> int:
        """Writes one byte and returns the moment it was written."""
        payload = bytes([byte])
        while True:
            try:
                self.socket.send(payload)
                return time.monotonic_ns()
            except BlockingIOError:
                continue

    def _fill(self, deadline: float) -> bool:
        """Reads **one** byte and queues it with the instant it was read.

        One byte per read, deliberately. A larger read takes one timestamp for
        everything it happens to collect, so two markers that arrive in the same
        read become an interval of zero — a broken reading that looks like a
        very fast system. Reading singly costs a syscall per byte on a wire that
        carries almost nothing, and gives every marker its own arrival time.
        """
        while time.monotonic() < deadline:
            try:
                chunk = self.socket.recv(1)
            except BlockingIOError:
                return True
            except ConnectionResetError:
                return False
            if not chunk:
                return False
            self.log.extend(chunk)
            self.queue.append((chunk[0], time.monotonic_ns()))
            return True
        return False

    def wait_for(self, wanted: int, deadline: float) -> bool:
        """Spins until one exact byte arrives, keeping the rest of the log."""
        while time.monotonic() < deadline:
            if not self._fill(deadline):
                return False
            while self.queue:
                byte, _ = self.queue.pop(0)
                if byte == wanted:
                    return True
        return False

    def read_until(self, wanted: int, deadline: float) -> tuple[int, int] | None:
        """Spins until a byte of the wanted marker family arrives.

        Everything else on the wire is the ordinary boot log and is queued, not
        timestamped as a marker. Returns the byte and the instant it was seen,
        or `None` if the deadline passed.
        """
        while time.monotonic() < deadline:
            while self.queue:
                byte, seen = self.queue.pop(0)
                if byte & 0xE0 == wanted:
                    return byte, seen
            if not self._fill(deadline):
                return None
        return None


class Qmp:
    """The control plane that bounds trace collection to the sample series."""

    def __init__(self, path: Path, deadline: float) -> None:
        self.next_id = 1
        while time.monotonic() < deadline:
            try:
                connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                connection.connect(str(path))
            except (FileNotFoundError, ConnectionRefusedError, OSError):
                time.sleep(0.01)
                continue
            self.socket = connection
            self.stream = connection.makefile("rwb", buffering=0)
            greeting = self._read(deadline)
            if "QMP" not in greeting:
                raise Invalid("QMP did not send its greeting")
            self.execute("qmp_capabilities", {}, deadline)
            return
        raise Invalid("QEMU never offered its QMP socket")

    def _read(self, deadline: float) -> dict[str, object]:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise Invalid("QMP response timed out")
        self.socket.settimeout(remaining)
        try:
            line = self.stream.readline()
        except OSError as error:
            raise Invalid(f"QMP response failed: {error}") from error
        if not line:
            raise Invalid("QMP disconnected")
        try:
            response = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise Invalid("QMP sent malformed JSON") from error
        if not isinstance(response, dict):
            raise Invalid("QMP response is not an object")
        return response

    def execute(
        self, command: str, arguments: dict[str, object], deadline: float
    ) -> None:
        command_id = self.next_id
        self.next_id += 1
        request = {"execute": command, "arguments": arguments, "id": command_id}
        try:
            self.stream.write(json.dumps(request).encode("utf-8") + b"\n")
        except OSError as error:
            raise Invalid(f"QMP {command} request failed: {error}") from error
        while True:
            response = self._read(deadline)
            if response.get("id") != command_id:
                continue
            if "error" in response:
                raise Invalid(f"QMP {command} failed: {response['error']}")
            if "return" not in response:
                raise Invalid(f"QMP {command} returned no verdict")
            return

    def trace_event(self, name: str, enabled: bool, deadline: float) -> None:
        self.execute(
            "trace-event-set-state",
            {"name": name, "enable": enabled},
            deadline,
        )


def _text_trace(path: Path) -> tuple[list[tuple[int, int]], dict[str, str], str]:
    """Decode the QEMU log backend without rounding its decimal timestamp."""
    markers: list[tuple[int, int]] = []
    for line in path.read_text(errors="replace").splitlines():
        stamp, _, rest = line.partition(":")
        if not rest.startswith("serial_write write addr 0x00 val "):
            continue
        _, separator, seconds = stamp.partition("@")
        whole, point, fraction = seconds.partition(".")
        try:
            if not separator or point != "." or not 1 <= len(fraction) <= 9:
                raise ValueError("timestamp is not seconds.fraction")
            when_ns = int(whole) * 1_000_000_000 + int(fraction.ljust(9, "0"))
            value = int(rest.rsplit(" ", 1)[1], 16)
        except ValueError as error:
            raise Invalid(f"malformed log trace serial_write: {line!r}") from error
        if value & 0x80:
            markers.append((value, when_ns))
    observer = {
        "backend": "QEMU log trace serial_write",
        "clock": "gettimeofday, microsecond text timestamp",
        "timestamp_point": "serial_write in the vCPU thread",
        "trace_format": "QEMU log text",
    }
    clock = (
        "QEMU log trace timestamp (gettimeofday, microseconds) taken in "
        "the vCPU thread while the device model handles the guest's write"
    )
    return markers, observer, clock


def _simple_trace(
    path: Path, clock_identity: str | None
) -> tuple[list[tuple[int, int]], dict[str, str], str]:
    """Independently decode the pinned QEMU simple trace format version 4.

    The retained evidence must not depend on a pretty-printer's output.  This
    decoder accepts only the format fields needed to identify the measurement
    pair (or the older diagnostic `serial_write` record),
    and refuses unknown, truncated, ambiguous, or loss-reporting records.
    """
    data = path.read_bytes()
    header_size = struct.calcsize("=QQQ")
    if len(data) < header_size:
        raise Invalid("truncated QEMU simple trace header")
    header_id, magic, version = struct.unpack_from("=QQQ", data)
    if header_id != SIMPLE_HEADER_EVENT_ID or magic != SIMPLE_HEADER_MAGIC:
        raise Invalid("invalid QEMU simple trace header")
    if version != SIMPLE_HEADER_VERSION:
        raise Invalid(f"QEMU simple trace version {version} is not supported")

    offset = header_size
    mappings: dict[int, str] = {}
    names: dict[str, int] = {}
    markers: list[tuple[int, int]] = []

    def require(size: int, what: str) -> None:
        if size < 0 or len(data) - offset < size:
            raise Invalid(f"truncated QEMU simple trace {what}")

    while offset < len(data):
        require(8, "record type")
        record_type = struct.unpack_from("=Q", data, offset)[0]
        offset += 8
        if record_type == SIMPLE_MAPPING_RECORD:
            require(12, "mapping")
            event_id, name_length = struct.unpack_from("=QL", data, offset)
            offset += 12
            require(name_length, "mapping name")
            try:
                name = data[offset : offset + name_length].decode("utf-8")
            except UnicodeDecodeError as error:
                raise Invalid("invalid UTF-8 in QEMU simple trace mapping") from error
            offset += name_length
            if event_id in mappings or name in names:
                raise Invalid(f"duplicate QEMU simple trace mapping for {name!r}")
            mappings[event_id] = name
            names[name] = event_id
            continue
        if record_type != SIMPLE_EVENT_RECORD:
            raise Invalid(f"unknown QEMU simple trace record type {record_type}")

        require(24, "event header")
        event_id, timestamp_ns, record_length, _pid = struct.unpack_from(
            "=QQII", data, offset
        )
        if record_length < 24:
            raise Invalid(f"invalid QEMU simple trace record length {record_length}")
        payload_length = record_length - 24
        offset += 24
        require(payload_length, "event payload")
        payload = data[offset : offset + payload_length]
        offset += payload_length

        if event_id == SIMPLE_DROPPED_EVENT_ID:
            if payload_length != 8:
                raise Invalid("invalid QEMU simple trace dropped-event payload")
            dropped = struct.unpack("=Q", payload)[0]
            if dropped:
                raise Invalid(f"QEMU simple trace dropped {dropped} event(s)")
            continue
        if event_id not in mappings:
            raise Invalid(f"unmapped QEMU simple trace event id {event_id}")
        event_name = mappings[event_id]
        if event_name == "tos_measurement_pair":
            if payload_length != 32:
                raise Invalid("invalid QEMU measurement-pair payload")
            opened, closed, open_ns, close_ns = struct.unpack("=QQQQ", payload)
            markers.extend(((opened, open_ns), (closed, close_ns)))
        elif event_name == "serial_write" and clock_identity != SIMPLE_TRACE_CLOCK:
            if payload_length != 16:
                raise Invalid("invalid QEMU simple trace serial_write payload")
            address, value = struct.unpack("=QQ", payload)
            if address == 0 and value & 0x80:
                markers.append((value, timestamp_ns))

    if clock_identity == SIMPLE_TRACE_CLOCK:
        if "tos_measurement_pair" not in names:
            raise Invalid("QEMU simple trace has no measurement-pair mapping")
        observer = {
            "backend": "QEMU simple trace symmetric UART measurement pair",
            "clock": SIMPLE_TRACE_CLOCK,
            "timestamp_point": "after OPEN handling and before CLOSE handling in the vCPU thread",
            "trace_format": "QEMU simple trace version 4",
        }
        clock = (
            "QEMU observer pair (simple backend, CLOCK_THREAD_CPUTIME_ID, "
            "nanoseconds of the one TCG vCPU thread) taken after OPEN handling "
            "and before CLOSE handling"
        )
    else:
        if "serial_write" not in names:
            raise Invalid("QEMU simple trace has no serial_write mapping")
        observer = {
            "backend": "QEMU simple trace serial_write",
            "clock": "CLOCK_MONOTONIC, nanosecond binary timestamp",
            "timestamp_point": "trace_record_start before serial_write in the vCPU thread",
            "trace_format": "QEMU simple trace version 4",
        }
        clock = (
            "QEMU trace timestamp (simple backend, CLOCK_MONOTONIC, nanoseconds) "
            "taken immediately before the device model handles the guest's write"
        )
    return markers, observer, clock


def read_trace(
    path: Path, simple_clock: str | None = None
) -> tuple[list[tuple[int, int]], dict[str, str], str]:
    """Every marker byte QEMU wrote, plus the identity of its trace clock.

    A line is `pid@seconds.microseconds:serial_write write addr 0xNN val 0xNN`.
    Only writes to register 0 are data; the others are the UART being
    configured, and the boot log's own bytes are ASCII, outside the marker
    range.
    """
    prefix = path.read_bytes()[:24]
    if len(prefix) >= 8 and struct.unpack_from("=Q", prefix)[0] == SIMPLE_HEADER_EVENT_ID:
        return _simple_trace(path, simple_clock)
    if b"\x00" in prefix:
        raise Invalid("trace is neither QEMU log text nor QEMU simple version 4")
    return _text_trace(path)


class Invalid(Exception):
    """The clock did something a clock may not do."""


def _command_option(command: list[str], name: str, default: str | None = None) -> str:
    """Return one explicit QEMU option value, refusing ambiguous commands."""
    values: list[str] = []
    for index, argument in enumerate(command):
        if argument == name:
            if index + 1 >= len(command):
                raise Invalid(f"{name} has no value")
            values.append(command[index + 1])
        elif argument.startswith(f"{name}="):
            values.append(argument.split("=", 1)[1])
    if len(values) > 1:
        raise Invalid(f"{name} occurs {len(values)} times")
    if values:
        return values[0]
    if default is not None:
        return default
    raise Invalid(f"{name} is not explicit")


def command_profile(command: list[str]) -> dict[str, str | int]:
    """Prove that a QEMU command names the ADR-0040 reference profile."""
    machine = _command_option(command, "-machine")
    cpu = _command_option(command, "-cpu")
    memory = _command_option(command, "-m")
    vcpus = _command_option(command, "-smp")
    accelerator = _command_option(command, "-accel", "tcg")

    try:
        memory_mib = int(memory.removesuffix("M"))
        vcpu_count = int(vcpus)
    except ValueError as error:
        raise Invalid(f"invalid memory or vCPU count: {error}") from error

    expected = {
        "accelerator": "tcg",
        "cpu": "qemu64",
        "machine": "q35",
        "memory_mib": 256,
        "vcpus": 1,
    }
    actual = {
        "accelerator": accelerator.split(",", 1)[0],
        "cpu": cpu.split(",", 1)[0],
        "machine": machine.split(",", 1)[0],
        "memory_mib": memory_mib,
        "vcpus": vcpu_count,
    }
    for field, wanted in expected.items():
        if actual[field] != wanted:
            raise Invalid(
                f"reference {field} is {wanted!r}, command selects {actual[field]!r}"
            )
    return actual


def apic_divider(path: Path) -> int:
    """Read the APIC timer divider the nucleus compiles in.

    Part of the reference platform's identity for an active-preemption
    measurement (ADR-0068 section 6): how often an interrupt lands inside an
    interval is the interval divided by the tick period, and the period is the
    quantum divided by nothing without this. The divisor is read from the
    constant's name because that is what the write means — a register value of
    `0b0011` is `divide by 16` only by the architecture's table, and a record
    that carried the encoding alone would name a number nobody could compare.
    """
    matches = re.findall(
        r"^\s*const\s+DIVIDE_BY_(\d+)\s*:\s*u32\s*=",
        path.read_text(encoding="utf-8"),
        flags=re.MULTILINE,
    )
    if len(matches) != 1:
        raise Invalid(f"expected exactly one APIC divider constant, found {len(matches)}")
    return int(matches[0])


def quantum_count(path: Path) -> int:
    """Read the one scheduler quantum constant that is compiled into the nucleus."""
    matches = re.findall(
        r"^\s*const\s+QUANTUM\s*:\s*u32\s*=\s*([0-9][0-9_]*)\s*;",
        path.read_text(encoding="utf-8"),
        flags=re.MULTILINE,
    )
    if len(matches) != 1:
        raise Invalid(f"expected exactly one QUANTUM definition, found {len(matches)}")
    return int(matches[0].replace("_", ""))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def output(command: list[str]) -> str:
    try:
        return subprocess.run(
            command,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise Invalid(f"cannot identify {' '.join(command)}: {error}") from error


def observer_build_manifest(qemu: Path) -> dict[str, object] | None:
    """Load a build record placed beside a repository-built observer."""
    path = qemu.parent / "observer-build.json"
    if not path.is_file():
        return None
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise Invalid(f"cannot read observer build manifest {path}: {error}") from error
    if not isinstance(manifest, dict):
        raise Invalid(f"observer build manifest {path} is not an object")
    required = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "qemu_version": "10.0.11",
        "qemu_source_sha256": (
            "22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871"
        ),
        "qemu_sha256": sha256(qemu),
        "trace_backends": ["simple"],
        "trace_clock": SIMPLE_TRACE_CLOCK,
        "network_downloads": "disabled",
        "source_date_epoch": 1782452340,
        "build_path_remap": "/usr/src/qemu-10.0.11",
        "configure": SIMPLE_OBSERVER_CONFIGURE,
        "cflags": SIMPLE_CFLAGS_IDENTITY,
    }
    for field, expected in required.items():
        if manifest.get(field) != expected:
            raise Invalid(
                f"observer manifest {field} is {manifest.get(field)!r}, "
                f"expected {expected!r}"
            )
    modifications = manifest.get("observer_modifications")
    expected_modifications = [
        {
            "path": "hw/char/serial.c",
            "upstream_sha256": OBSERVER_SERIAL_SOURCE_SHA256,
            "modified_sha256": OBSERVER_SERIAL_MODIFIED_SHA256,
            "scope": "capture after OPEN and before CLOSE; UART behavior unchanged",
        },
        {
            "path": "hw/char/trace-events",
            "upstream_sha256": OBSERVER_EVENTS_SOURCE_SHA256,
            "modified_sha256": OBSERVER_EVENTS_MODIFIED_SHA256,
            "scope": "one measurement-pair event carrying both raw timestamps",
        },
    ]
    if modifications != expected_modifications:
        raise Invalid("observer manifest does not bind the symmetric marker modification")
    engine_relative = manifest.get("qemu_engine_relative_path")
    if engine_relative != "qemu-system-x86_64.real":
        raise Invalid(f"observer manifest engine path is {engine_relative!r}")
    retained_data = manifest.get("retained_data")
    if not isinstance(retained_data, dict):
        raise Invalid("observer manifest retained_data is not an object")
    retained = {
        engine_relative: manifest.get("qemu_engine_sha256"),
        "../share/qemu/kvmvapic.bin": retained_data.get(
            "../share/qemu/kvmvapic.bin"
        ),
        "../share/qemu/vgabios-stdvga.bin": retained_data.get(
            "../share/qemu/vgabios-stdvga.bin"
        ),
        "../share/qemu/efi-e1000e.rom": retained_data.get(
            "../share/qemu/efi-e1000e.rom"
        ),
    }
    for relative, expected_sha256 in retained.items():
        retained_path = qemu.parent / relative
        if not retained_path.is_file() or sha256(retained_path) != expected_sha256:
            raise Invalid(f"observer retained file does not match manifest: {relative}")
    dynamic_dependencies = manifest.get("dynamic_dependencies")
    if not isinstance(dynamic_dependencies, dict) or not dynamic_dependencies:
        raise Invalid("observer manifest has no dynamic dependency identity")
    for dependency, expected_sha256 in dynamic_dependencies.items():
        dependency_path = Path(dependency)
        if (
            not dependency_path.is_absolute()
            or not dependency_path.is_file()
            or sha256(dependency_path) != expected_sha256
        ):
            raise Invalid(
                f"observer dynamic dependency does not match manifest: {dependency}"
            )
    return {
        "path": str(path.resolve()),
        "sha256": sha256(path),
        "contents": manifest,
    }


def measurement_build_manifest(
    path: Path | None,
    artifacts: dict[str, Path],
    source_commit: str,
    paired_calibration: bool,
    ipc_measurement: bool,
) -> dict[str, object] | None:
    """Bind declared Cargo features to the exact measured guest binaries."""
    if path is None:
        return None
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise Invalid(f"cannot read measurement build manifest {path}: {error}") from error
    if not isinstance(manifest, dict):
        raise Invalid("measurement build manifest is not an object")
    required_top = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "schema": "tos-measurement-build-v1",
        "source_commit": source_commit,
    }
    for field, expected in required_top.items():
        if manifest.get(field) != expected:
            raise Invalid(
                f"measurement build manifest {field} is {manifest.get(field)!r}, "
                f"expected {expected!r}"
            )
    builds = manifest.get("builds")
    if not isinstance(builds, dict) or set(builds) != {"nucleus", "runtime_image"}:
        raise Invalid("measurement build manifest must name nucleus and runtime_image")
    required_builds = {
        "nucleus": {
            "package": "tos-nucleus",
            "artifact": artifacts["measurement_nucleus"],
        },
        "runtime_image": {
            "package": "tos-runtime-image",
            "artifact": artifacts["measurement_runtime_image"],
        },
    }
    for name, required in required_builds.items():
        record = builds.get(name)
        if not isinstance(record, dict):
            raise Invalid(f"measurement build {name} is not an object")
        expected_fields = {
            "package": required["package"],
            "target": "x86_64-unknown-none",
            "profile": "release",
            "artifact_path": str(required["artifact"].resolve()),
            "artifact_sha256": sha256(required["artifact"]),
        }
        for field, expected in expected_fields.items():
            if record.get(field) != expected:
                raise Invalid(
                    f"measurement build {name} {field} is {record.get(field)!r}, "
                    f"expected {expected!r}"
                )
        features = record.get("features")
        if (
            not isinstance(features, list)
            or not all(isinstance(feature, str) for feature in features)
            or len(features) != len(set(features))
        ):
            raise Invalid(f"measurement build {name} features are invalid")
        expected_command = [
            "cargo",
            "build",
            "--release",
            "-p",
            required["package"],
            "--target",
            "x86_64-unknown-none",
            "--features",
            ",".join(features),
        ]
        if record.get("cargo_command") != expected_command:
            raise Invalid(f"measurement build {name} cargo command is not exact")
        target_dir = record.get("cargo_target_dir")
        if not isinstance(target_dir, str) or not Path(target_dir).is_absolute():
            raise Invalid(f"measurement build {name} target directory is not absolute")
    if paired_calibration:
        expected_features = {
            "nucleus": ["test-measurement-no-preemption"],
            "runtime_image": ["test-measurement-call"],
        }
    elif ipc_measurement:
        expected_features = {
            "nucleus": ["test-call-reply", "test-measurement-port"],
            "runtime_image": ["test-measurement-ipc"],
        }
    else:
        expected_features = None
    if expected_features is not None:
        for name, expected in expected_features.items():
            if builds[name].get("features") != expected:
                raise Invalid(
                    f"measurement mode requires {name} features {expected!r}"
                )
    return {
        "path": str(path.resolve()),
        "sha256": sha256(path),
        "contents": manifest,
    }


def evidence_status(
    requested: str, dirty: bool, production_isolation: bool, github_actions: bool
) -> str:
    """Reserve P2 for a clean repository gate running in GitHub Actions."""
    if requested == "P2" and not github_actions:
        raise Invalid("P2 evidence may only be emitted by GitHub Actions")
    if dirty or not production_isolation:
        return "exploratory"
    return requested


def environment(args: argparse.Namespace) -> dict[str, object]:
    """Collect the identities needed to reproduce or reject this evidence."""
    repository = args.repository.resolve()
    qemu = shutil.which(args.command[0])
    if qemu is None:
        raise Invalid(f"QEMU executable {args.command[0]!r} was not found")

    paths = {
        "capsule": args.capsule,
        "firmware_code": args.firmware_code,
        "firmware_vars": args.firmware_vars,
        "loader": args.loader,
        "measurement_nucleus": args.nucleus,
        "measurement_runtime_image": args.runtime_image,
        "production_nucleus": args.production_nucleus,
        "production_runtime_image": args.production_runtime_image,
    }
    missing = [str(path) for path in paths.values() if not path.is_file()]
    if missing:
        raise Invalid(f"identity input is missing: {', '.join(missing)}")

    status = output(["git", "-C", str(repository), "status", "--porcelain"])
    source_commit = output(["git", "-C", str(repository), "rev-parse", "HEAD"])
    cpu_model = "unknown"
    try:
        for line in Path("/proc/cpuinfo").read_text(errors="replace").splitlines():
            if line.startswith("model name"):
                cpu_model = line.partition(":")[2].strip()
                break
    except OSError as error:
        raise Invalid(f"cannot identify host CPU: {error}") from error

    production_before = {
        "production_nucleus": args.production_nucleus_before_sha256,
        "production_runtime_image": args.production_runtime_image_before_sha256,
    }
    production_after = {
        name: sha256(paths[name]) for name in production_before
    }
    for name, before in production_before.items():
        if before is not None and before != production_after[name]:
            raise Invalid(
                f"{name} changed: before {before}, after {production_after[name]}"
            )
    production_isolation_proven = all(production_before.values())

    qemu_path = Path(qemu).resolve()
    build_manifest = observer_build_manifest(qemu_path)
    measured_build = measurement_build_manifest(
        args.measurement_build_manifest,
        paths,
        source_commit,
        args.paired_calibration,
        args.ipc_measurement,
    )
    nucleus_features: list[str] = []
    if measured_build is not None:
        nucleus_features = measured_build["contents"]["builds"]["nucleus"][
            "features"
        ]
    scheduler_preemption = (
        "inactive"
        if "test-measurement-no-preemption" in nucleus_features
        else "active"
        if measured_build is not None
        else "unbound"
    )
    retained_status = evidence_status(
        args.evidence_status,
        bool(status),
        production_isolation_proven,
        os.environ.get("GITHUB_ACTIONS") == "true",
    )
    if measured_build is None:
        retained_status = "exploratory"

    return {
        "evidence_status": retained_status,
        "source": {
            "commit": source_commit,
            "dirty": bool(status),
        },
        "observer": {
            "qemu_path": qemu,
            "qemu_sha256": sha256(qemu_path),
            "qemu_version": output([qemu, "--version"]).splitlines()[0],
            "build_manifest": build_manifest,
        },
        "guest_profile": command_profile(args.command),
        "host": {
            "cpu_model": cpu_model,
            "rustc": output(["rustc", "--version", "--verbose"]),
        },
        "scheduler": {
            "preemption": scheduler_preemption,
            "binding": "measurement-build-manifest"
            if measured_build is not None
            else "unbound",
            "quantum_count": quantum_count(args.quantum_source),
            "apic_divider": apic_divider(args.quantum_source),
        },
        "measurement_build": measured_build,
        "production_artifact_isolation": {
            name: {
                "before_measurement_build_sha256": production_before[name],
                "after_measurement_build_sha256": production_after[name],
                "unchanged": production_before[name] == production_after[name]
                if production_before[name] is not None
                else None,
            }
            for name in production_before
        },
        "artifacts": {
            name: {"path": str(path.resolve()), "sha256": sha256(path)}
            for name, path in paths.items()
        },
    }


def measurement_plan(blocks: int, paired: bool) -> list[int]:
    """Return the complete, predeclared request-tag sequence."""
    plan: list[int] = []
    for index in range(blocks):
        sequence = index % (SEQUENCE + 1)
        if paired and index % 2:
            plan.extend((WORK | sequence, sequence))
        elif paired:
            plan.extend((sequence, WORK | sequence))
        else:
            plan.append(sequence)
    return plan


def pair_markers(
    markers: list[tuple[int, int]], expected_tags: list[int]
) -> list[tuple[int, float]]:
    """Intervals whose complete tags match the predeclared request plan.

    A pair whose two halves disagree is not a measurement of anything and
    invalidates the run rather than being dropped or repaired. A valid duplicate
    is still invalid when the plan calls for another tag: count alone cannot
    allow one completed observation to replace a missing one.

    Timestamps stay integer nanoseconds through pairing.  A timestamp that goes
    backwards, or an interval of zero or less, makes the run invalid. Nothing
    is clamped, dropped or re-ordered.
    """
    samples: list[tuple[int, float]] = []
    opened: tuple[int, int] | None = None
    previous = None
    for value, when in markers:
        if previous is not None and when < previous:
            raise Invalid(
                f"the clock went backwards, from {previous} ns to {when} ns"
            )
        previous = when
        family = value & 0xE0
        tag = value & TAG
        if family == OPEN:
            if opened is not None:
                open_tag, _ = opened
                if tag == open_tag:
                    raise Invalid(f"sample tag {tag} opened twice")
                raise Invalid(
                    f"sample tag {tag} opened while sample tag {open_tag} is still open"
                )
            index = len(samples)
            if index >= len(expected_tags):
                raise Invalid(f"unexpected sample tag {tag} after the complete plan")
            expected = expected_tags[index]
            if tag != expected:
                raise Invalid(
                    f"sample {index} has tag {tag}, expected tag {expected}"
                )
            opened = (tag, when)
        elif family == CLOSE:
            if opened is None:
                raise Invalid(f"sample tag {tag} closed without an open")
            open_tag, open_when = opened
            if tag != open_tag:
                raise Invalid(
                    f"close for sample tag {tag} does not match open sample tag "
                    f"{open_tag}"
                )
            interval = (when - open_when) / 1_000.0
            if interval <= 0:
                raise Invalid(
                    f"an interval of {interval:.3f} us: the two markers of one "
                    "sample carry the same time or worse"
                )
            samples.append((tag, interval))
            opened = None
    if opened is not None:
        tag, _ = opened
        raise Invalid(f"sample tag {tag} was opened but never closed")
    if len(samples) != len(expected_tags):
        raise Invalid(
            f"trace completed {len(samples)} of {len(expected_tags)} planned samples"
        )
    return samples


def split_paired_samples(
    samples: list[tuple[int, float]], warmup_blocks: int
) -> tuple[list[float], list[float], list[str]]:
    """Split adjacent same-sequence floor/call blocks without re-pairing."""
    if len(samples) % 2:
        raise Invalid("paired calibration contains an incomplete block")
    floor: list[float] = []
    call: list[float] = []
    order: list[str] = []
    for block_index in range(len(samples) // 2):
        first_tag, first_value = samples[block_index * 2]
        second_tag, second_value = samples[block_index * 2 + 1]
        if first_tag & SEQUENCE != second_tag & SEQUENCE:
            raise Invalid(f"paired block {block_index} uses two sequence identities")
        if bool(first_tag & WORK) == bool(second_tag & WORK):
            raise Invalid(f"paired block {block_index} does not contain floor and call")
        if block_index < warmup_blocks:
            continue
        if first_tag & WORK:
            call.append(first_value)
            floor.append(second_value)
            order.append("call-floor")
        else:
            floor.append(first_value)
            call.append(second_value)
            order.append("floor-call")
    return floor, call, order


def percentile(values: list[float], fraction: float) -> float:
    """The nearest-rank percentile: with 21 samples, p99 is the largest.

    Stated rather than interpolated, because an interpolated p99 of 21 samples
    is a number no sample produced.
    """
    ordered = sorted(values)
    rank = max(1, min(len(ordered), int(-(-fraction * len(ordered) // 1))))
    return ordered[rank - 1]


def encode_report(report: dict[str, object]) -> str:
    """Encode a repository evidence record with its own licence identity."""
    licensed = {"record_spdx_license": "CC-BY-SA-4.0", **report}
    return json.dumps(licensed, indent=2) + "\n"


def main() -> int:
    args = arguments()
    try:
        run_environment = environment(args)
    except Invalid as invalid:
        print(f"measure-channel: environment invalid: {invalid}", file=sys.stderr)
        return 2
    for path in (args.serial_log, args.stderr_log, args.qmp_socket):
        path.parent.mkdir(parents=True, exist_ok=True)
    for path in (args.socket, args.qmp_socket):
        if path.exists():
            path.unlink()

    started = time.monotonic()
    deadline = started + args.timeout
    with args.stderr_log.open("wb") as stderr:
        process = subprocess.Popen(
            args.command, stdout=subprocess.DEVNULL, stderr=stderr
        )
        try:
            qmp = Qmp(args.qmp_socket, deadline)
            wire = Wire(args.socket, deadline)
            build_manifest = run_environment["observer"]["build_manifest"]
            trace_event = "serial_write"
            if (
                build_manifest is not None
                and build_manifest["contents"].get("trace_clock")
                == SIMPLE_TRACE_CLOCK
            ):
                trace_event = "tos_measurement_pair"
            # Until this arrives the far end of the wire is the firmware, not
            # the process being measured.
            if not wire.wait_for(READY, deadline):
                print(
                    "measure-channel: the observed process never announced itself",
                    file=sys.stderr,
                )
                args.serial_log.write_bytes(bytes(wire.log))
                if process.poll() is None:
                    process.terminate()
                return 1
            qmp.trace_event(trace_event, True, deadline)
            reader_pairs: list[tuple[int, float]] = []
            total_blocks = WARMUPS + args.samples
            expected_tags = measurement_plan(total_blocks, args.paired_calibration)
            for index, tag in enumerate(expected_tags):
                # The guest is still booting for the first request; give the
                # whole remaining budget to each and let the timeout decide.
                wire.send(GO | tag)
                opened = wire.read_until(OPEN, deadline)
                closed = wire.read_until(CLOSE, deadline)
                if opened is None or closed is None:
                    # Say which request went unanswered. A silent break leaves
                    # the next failure — a QMP call to a machine that is already
                    # gone — describing the symptom instead of the cause.
                    print(
                        f"measure-channel: request {index} of {len(expected_tags)} "
                        f"(tag {tag}) went unanswered; the guest stopped responding",
                        file=sys.stderr,
                    )
                    args.serial_log.write_bytes(bytes(wire.log))
                    break
                (open_byte, open_at), (close_byte, close_at) = opened, closed
                open_tag = open_byte & TAG
                close_tag = close_byte & TAG
                if open_tag != tag or close_tag != tag:
                    raise Invalid(
                        f"live sample {index} answered tags {open_tag}/{close_tag}, "
                        f"expected tag {tag}"
                    )
                reader_pairs.append((tag, (close_at - open_at) / 1000.0))
            qmp.trace_event(trace_event, False, deadline)
            wire.send(STOP)
            # The boot is not over when the samples are: the process leaves the
            # loop, ends, and the nucleus halts. Those bytes are the ordinary
            # boot log the harness judges the run by, so the observer keeps
            # reading until the machine is gone rather than pulling the wire out
            # from under its own evidence.
            while process.poll() is None and time.monotonic() < deadline:
                if not wire._fill(deadline):
                    break
            args.serial_log.write_bytes(bytes(wire.log))
        except Invalid as invalid:
            print(f"measure-channel: observer control invalid: {invalid}", file=sys.stderr)
            if process.poll() is None:
                process.terminate()
            return 1
        finally:
            try:
                process.wait(timeout=max(1.0, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()

    if len(reader_pairs) != len(expected_tags):
        print(
            f"measure-channel: {len(reader_pairs)} of {len(expected_tags)} "
            "planned live samples arrived",
            file=sys.stderr,
        )
        return 1
    if not (args.trace and args.trace.is_file()):
        print("measure-channel: no trace file: QEMU produced no clock", file=sys.stderr)
        return 1
    try:
        build_manifest = run_environment["observer"]["build_manifest"]
        simple_clock = None
        if build_manifest is not None:
            simple_clock = build_manifest["contents"].get("trace_clock")
        markers, observer, trace_clock = read_trace(args.trace, simple_clock)
        run_environment["observer"].update(observer)
        if (
            observer["backend"].startswith("QEMU simple trace")
            and run_environment["observer"]["build_manifest"] is None
        ):
            run_environment["evidence_status"] = "exploratory"
        trace_pairs = pair_markers(markers, expected_tags)
    except Invalid as invalid:
        print(f"measure-channel: measurement invalid: {invalid}", file=sys.stderr)
        return 1
    if args.paired_calibration:
        floor_samples, samples, pair_order = split_paired_samples(
            trace_pairs, WARMUPS
        )
        floor_reader_samples, observed, reader_order = split_paired_samples(
            reader_pairs, WARMUPS
        )
        if reader_order != pair_order:
            print("measure-channel: live and trace pair orders differ", file=sys.stderr)
            return 1
    else:
        floor_samples = []
        pair_order = []
        samples = [value for _tag, value in trace_pairs[WARMUPS:]]
        observed = [value for _tag, value in reader_pairs[WARMUPS:]]
        floor_reader_samples = []
    if len(samples) != args.samples or (
        args.paired_calibration and len(floor_samples) != args.samples
    ):
        print("measure-channel: the trace does not hold the complete sample plan", file=sys.stderr)
        return 1
    report = {
        "samples_us": samples,
        # The same intervals as this program's own reader saw them, for one
        # purpose only: to show how far a host-side reader is from the truth.
        # It is never the measurement.
        "reader_samples_us": observed,
        "warmups": WARMUPS,
        "count": len(samples),
        "median_us": statistics.median(samples),
        "p99_us": percentile(samples, 0.99),
        "min_us": min(samples),
        "max_us": max(samples),
        "jitter_us": max(samples) - min(samples),
        "clock": trace_clock,
        "subtracted": "nothing",
        "environment": run_environment,
    }
    if args.paired_calibration:
        report.update(
            {
                "measurement_mode": "adjacent-floor-call-pairs-v1",
                "floor_samples_us": floor_samples,
                "floor_reader_samples_us": floor_reader_samples,
                "pair_order": pair_order,
                "floor_statistics": {
                    "median_us": statistics.median(floor_samples),
                    "p99_us": percentile(floor_samples, 0.99),
                    "min_us": min(floor_samples),
                    "max_us": max(floor_samples),
                    "jitter_us": max(floor_samples) - min(floor_samples),
                },
            }
        )
    elif args.ipc_measurement:
        report["measurement_mode"] = "ipc-request-reply-v1"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(encode_report(report))
    print(
        "measure-channel: {count} sample(s) after {warmups} warm-up(s): "
        "median {median_us:.2f} us, p99 {p99_us:.2f} us, "
        "min {min_us:.2f}, max {max_us:.2f}, jitter {jitter_us:.2f} us".format(**report)
    )
    # The protocol succeeded; whether the *boot* did is the harness's ordinary
    # verdict, so its exit code is passed through rather than replaced.
    return process.returncode or 0


if __name__ == "__main__":
    raise SystemExit(main())
