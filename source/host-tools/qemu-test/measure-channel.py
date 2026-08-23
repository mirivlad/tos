#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The external instrument of ADR-0066, and first of all a check of itself.

The measurement clock is QEMU's trace timestamp at the device model. The
host-side reader also records `CLOCK_MONOTONIC` arrival times, but only as a
diagnostic comparison and never as the measurement. Neither is a facility of
TOS: the guest is never told what time it is, and nothing here becomes part of
the system's semantics, capability surface or ABI. The system is measured the
way a circuit is measured — the oscilloscope does not become a component.

**The protocol.** The observer sends `GO | n`, the observed process does the
thing being measured, and answers `DONE | n`. The echo is what makes a sample
causal rather than coincidental: a `DONE` that does not name the `GO` it
followed is discarded, never repaired.

**Where the clock is.** On QEMU's side of the wire, not this one. The diagnostic
`log` backend prefixes events with a microsecond `gettimeofday` value.  The
pinned conformance candidate uses QEMU's binary `simple` backend: integer
nanoseconds from `CLOCK_MONOTONIC`, taken by `trace_record_start` immediately
before the device model handles `serial_write`.  Both trace in the vCPU thread,
synchronously at the guest's `out` boundary. Nothing this program does with its
own scheduling can move that timestamp. The socket is kept for the *protocol*:
it carries the request that starts a sample and the stop that ends the run, and
it is not the clock.

That matters because the obvious alternative is wrong in a direction that
flatters the system. A reader on this side stamps a marker when it manages to
read it; if it is late to `OPEN` while the guest is already working, the interval
it reports is **shorter** than the truth. An instrument that errs towards
passing is not an instrument.

**What a reading contains.** `t(CLOSE) - t(OPEN)`, both taken inside QEMU: the
work, plus whatever the `OPEN` write itself still costs after its timestamp was
taken. Every reading contains that floor and **none of it is subtracted**. The
floor is measured by the same instrument over a run in which the work is
nothing, and is published beside the result. A reading is therefore an upper
bound on the work, which is the direction that makes a budget claim honest.

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
SEQUENCE = 0x1F
STOP = 0xE0
READY = 0xFF
WARMUPS = 3
SIMPLE_HEADER_EVENT_ID = 0xFFFFFFFFFFFFFFFF
SIMPLE_DROPPED_EVENT_ID = 0xFFFFFFFFFFFFFFFE
SIMPLE_HEADER_MAGIC = 0xF2B177CB0AA429B4
SIMPLE_HEADER_VERSION = 4
SIMPLE_MAPPING_RECORD = 0
SIMPLE_EVENT_RECORD = 1
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
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    if not args.command:
        parser.error("the QEMU command is required after --")
    if args.samples < 1 or args.samples > SEQUENCE:
        parser.error(f"--samples must be 1..{SEQUENCE}")
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
        raise SystemExit("measure-channel: QEMU never offered its serial socket")

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


def _simple_trace(path: Path) -> tuple[list[tuple[int, int]], dict[str, str], str]:
    """Independently decode the pinned QEMU simple trace format version 4.

    The retained evidence must not depend on a pretty-printer's output.  This
    decoder accepts only the format fields needed to identify `serial_write`,
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
        if mappings[event_id] != "serial_write":
            continue
        if payload_length != 16:
            raise Invalid("invalid QEMU simple trace serial_write payload")
        address, value = struct.unpack("=QQ", payload)
        if address == 0 and value & 0x80:
            markers.append((value, timestamp_ns))

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


def read_trace(path: Path) -> tuple[list[tuple[int, int]], dict[str, str], str]:
    """Every marker byte QEMU wrote, plus the identity of its trace clock.

    A line is `pid@seconds.microseconds:serial_write write addr 0xNN val 0xNN`.
    Only writes to register 0 are data; the others are the UART being
    configured, and the boot log's own bytes are ASCII, outside the marker
    range.
    """
    prefix = path.read_bytes()[:24]
    if len(prefix) >= 8 and struct.unpack_from("=Q", prefix)[0] == SIMPLE_HEADER_EVENT_ID:
        return _simple_trace(path)
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

    return {
        "evidence_status": evidence_status(
            args.evidence_status,
            bool(status),
            production_isolation_proven,
            os.environ.get("GITHUB_ACTIONS") == "true",
        ),
        "source": {
            "commit": output(["git", "-C", str(repository), "rev-parse", "HEAD"]),
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
            "preemption": "active",
            "quantum_count": quantum_count(args.quantum_source),
        },
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


def pair_markers(markers: list[tuple[int, int]], warmups: int) -> list[float]:
    """Intervals of `OPEN`/`CLOSE` pairs that name the same request.

    A pair whose two halves disagree is not a measurement of anything and
    invalidates the run rather than being dropped or repaired.

    Timestamps stay integer nanoseconds through pairing.  A timestamp that goes
    backwards, or an interval of zero or less, makes the run invalid. Nothing
    is clamped, dropped or re-ordered.
    """
    samples: list[float] = []
    opened: tuple[int, int] | None = None
    previous = None
    for value, when in markers:
        if previous is not None and when < previous:
            raise Invalid(
                f"the clock went backwards, from {previous} ns to {when} ns"
            )
        previous = when
        family = value & 0xE0
        sequence = value & SEQUENCE
        if family == OPEN:
            if opened is not None:
                open_sequence, _ = opened
                if sequence == open_sequence:
                    raise Invalid(f"sample {sequence} opened twice")
                raise Invalid(
                    f"sample {sequence} opened while sample {open_sequence} is still open"
                )
            opened = (sequence, when)
        elif family == CLOSE:
            if opened is None:
                raise Invalid(f"sample {sequence} closed without an open")
            open_sequence, open_when = opened
            if sequence != open_sequence:
                raise Invalid(
                    f"close for sample {sequence} does not match open sample "
                    f"{open_sequence}"
                )
            interval = (when - open_when) / 1_000.0
            if interval <= 0:
                raise Invalid(
                    f"an interval of {interval:.3f} us: the two markers of one "
                    "sample carry the same time or worse"
                )
            samples.append(interval)
            opened = None
    if opened is not None:
        sequence, _ = opened
        raise Invalid(f"sample {sequence} was opened but never closed")
    return samples[warmups:]


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
    for path in (args.serial_log, args.stderr_log):
        path.parent.mkdir(parents=True, exist_ok=True)
    if args.socket.exists():
        args.socket.unlink()

    started = time.monotonic()
    deadline = started + args.timeout
    with args.stderr_log.open("wb") as stderr:
        process = subprocess.Popen(
            args.command, stdout=subprocess.DEVNULL, stderr=stderr
        )
        try:
            wire = Wire(args.socket, deadline)
            # Until this arrives the far end of the wire is the firmware, not
            # the process being measured.
            if not wire.wait_for(READY, deadline):
                print(
                    "measure-channel: the observed process never announced itself",
                    file=sys.stderr,
                )
                args.serial_log.write_bytes(bytes(wire.log))
                return 1
            samples: list[float] = []
            discarded = 0
            total = WARMUPS + args.samples
            for index in range(total):
                sequence = index % (SEQUENCE + 1)
                # The guest is still booting for the first request; give the
                # whole remaining budget to each and let the timeout decide.
                wire.send(GO | sequence)
                opened = wire.read_until(OPEN, deadline)
                closed = wire.read_until(CLOSE, deadline)
                if opened is None or closed is None:
                    break
                (open_byte, open_at), (close_byte, close_at) = opened, closed
                if open_byte & SEQUENCE != sequence or close_byte & SEQUENCE != sequence:
                    # Causality, not tidiness: a pair that names another request
                    # is not a measurement of this one.
                    discarded += 1
                    continue
                if index >= WARMUPS:
                    samples.append((close_at - open_at) / 1000.0)
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

    if discarded:
        print(
            f"measure-channel: {discarded} answer(s) did not name their request",
            file=sys.stderr,
        )
        return 1
    if len(samples) != args.samples:
        print(
            f"measure-channel: {len(samples)} of {args.samples} samples arrived",
            file=sys.stderr,
        )
        return 1
    observed = samples
    if not (args.trace and args.trace.is_file()):
        print("measure-channel: no trace file: QEMU produced no clock", file=sys.stderr)
        return 1
    try:
        markers, observer, trace_clock = read_trace(args.trace)
        run_environment["observer"].update(observer)
        if (
            observer["backend"] == "QEMU simple trace serial_write"
            and run_environment["observer"]["build_manifest"] is None
        ):
            run_environment["evidence_status"] = "exploratory"
        samples = pair_markers(markers, WARMUPS)
    except Invalid as invalid:
        print(f"measure-channel: measurement invalid: {invalid}", file=sys.stderr)
        return 1
    if len(samples) != args.samples:
        print(
            f"measure-channel: the trace holds {len(samples)} pair(s) of {args.samples}",
            file=sys.stderr,
        )
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
