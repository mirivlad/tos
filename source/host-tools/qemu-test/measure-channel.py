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

**Where the clock is.** On QEMU's side of the wire, not this one. With
`-msg timestamp=on` the log trace backend prefixes every event with
`pid@seconds.microseconds`, and `serial_write` is emitted by the device model
while it is handling the guest's `out` instruction — in the vCPU thread,
synchronously with the write. So a marker's time is taken where the marker
happens, and nothing this program does with its own scheduling can move it. The
socket is kept for the *protocol*: it carries the request that starts a sample
and the stop that ends the run, and it is not the clock.

That matters because the obvious alternative is wrong in a direction that
flatters the system. A reader on this side stamps a marker when it manages to
read it; if it is late to `OPEN` while the guest is already working, the interval
it reports is **shorter** than the truth. An instrument that errs towards
passing is not an instrument.

**What a reading contains.** `t(CLOSE) - t(OPEN)`, both taken inside QEMU: the
work, plus whatever the `OPEN` write itself still costs after its timestamp was
taken — the rest of the device model's write path and the trace line. Every
reading contains that floor and **none of them has it subtracted**. The floor is
measured by the same instrument over a run in which the work is nothing, and it
is published beside the result. A reading is therefore an upper bound on the
work, which is the direction that makes a budget claim honest.

**Why the reader still spins.** It no longer times anything, but it must not be
the reason a sample is late: the next request is sent when the previous answer is
seen, so a sluggish reader would stretch the gaps *between* samples. Reading one
byte at a time keeps that immediate and keeps the ordinary boot log intact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import socket
import statistics
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


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--serial-log", required=True, type=Path)
    parser.add_argument("--stderr-log", required=True, type=Path)
    parser.add_argument("--samples", required=True, type=int)
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


def trace_markers(path: Path) -> list[tuple[int, float]]:
    """Every marker byte QEMU wrote, with the time it wrote it.

    A line is `pid@seconds.microseconds:serial_write write addr 0xNN val 0xNN`.
    Only writes to register 0 are data; the others are the UART being
    configured, and the boot log's own bytes are ASCII, outside the marker
    range.
    """
    markers: list[tuple[int, float]] = []
    for line in path.read_text(errors="replace").splitlines():
        stamp, _, rest = line.partition(":")
        if not rest.startswith("serial_write write addr 0x00 val "):
            continue
        _, _, seconds = stamp.partition("@")
        try:
            when = float(seconds)
            value = int(rest.rsplit(" ", 1)[1], 16)
        except ValueError:
            continue
        if value & 0x80:
            markers.append((value, when))
    return markers


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

    return {
        "evidence_status": (
            "P1" if not status and production_isolation_proven else "exploratory"
        ),
        "source": {
            "commit": output(["git", "-C", str(repository), "rev-parse", "HEAD"]),
            "dirty": bool(status),
        },
        "observer": {
            "backend": "QEMU log trace serial_write",
            "clock": "gettimeofday, microsecond text timestamp",
            "timestamp_point": "serial_write in the vCPU thread",
            "qemu_path": qemu,
            "qemu_sha256": sha256(Path(qemu)),
            "qemu_version": output([qemu, "--version"]).splitlines()[0],
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


def pair_markers(markers: list[tuple[int, float]], warmups: int) -> list[float]:
    """Intervals of `OPEN`/`CLOSE` pairs that name the same request.

    A pair whose two halves disagree is not a measurement of anything and
    invalidates the run rather than being dropped or repaired.

    **The clock is `CLOCK_REALTIME`**, which is the one QEMU's trace backend
    offers, and a real-time clock may be stepped or slewed underneath a
    measurement. So this refuses rather than repairs: a timestamp that goes
    backwards, or an interval of zero or less, makes the run invalid. Nothing is
    clamped, dropped or re-ordered — a measurement taken across a clock
    adjustment is not a slightly wrong measurement, it is not one.
    """
    samples: list[float] = []
    opened: tuple[int, float] | None = None
    previous = None
    for value, when in markers:
        if previous is not None and when < previous:
            raise Invalid(
                f"the clock went backwards, from {previous:.6f} to {when:.6f}"
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
            interval = (when - open_when) * 1_000_000.0
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
        samples = pair_markers(trace_markers(args.trace), WARMUPS)
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
        "clock": "QEMU trace timestamp (gettimeofday, microseconds) taken in "
        "the vCPU thread while the device model handles the guest's write",
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
