#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression tests for the external measurement observer."""

from __future__ import annotations

import importlib.util
import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("measure-channel.py")
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("measure_channel", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
measure_channel = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(measure_channel)


def simple_trace(*records: bytes, version: int = 4) -> bytes:
    header = struct.pack(
        "=QQQ",
        0xFFFFFFFFFFFFFFFF,
        0xF2B177CB0AA429B4,
        version,
    )
    mapping = struct.pack("=QQL", 0, 7, len(b"serial_write")) + b"serial_write"
    return header + mapping + b"".join(records)


def simple_event(event_id: int, timestamp_ns: int, *arguments: int) -> bytes:
    payload = struct.pack(f"={len(arguments)}Q", *arguments)
    return struct.pack(
        "=QQQII", 1, event_id, timestamp_ns, 24 + len(payload), 1234
    ) + payload


class PairingTests(unittest.TestCase):
    def test_valid_pairs_retain_every_sample_after_warmups(self) -> None:
        markers = [
            (measure_channel.OPEN | 0, 1_000_000_000),
            (measure_channel.CLOSE | 0, 1_000_010_000),
            (measure_channel.OPEN | 1, 2_000_000_000),
            (measure_channel.CLOSE | 1, 2_000_020_000),
            (measure_channel.OPEN | 2, 3_000_000_000),
            (measure_channel.CLOSE | 2, 3_000_030_000),
        ]

        samples = measure_channel.pair_markers(markers, 1)
        self.assertEqual(len(samples), 2)
        self.assertAlmostEqual(samples[0], 20.0)
        self.assertAlmostEqual(samples[1], 30.0)

    def test_duplicate_open_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 4, 1_000_000_000),
            (measure_channel.OPEN | 4, 1_000_001_000),
            (measure_channel.CLOSE | 4, 1_000_010_000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "opened twice"):
            measure_channel.pair_markers(markers, 0)

    def test_overlapping_samples_are_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 4, 1_000_000_000),
            (measure_channel.OPEN | 5, 1_000_001_000),
            (measure_channel.CLOSE | 4, 1_000_010_000),
            (measure_channel.CLOSE | 5, 1_000_020_000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "still open"):
            measure_channel.pair_markers(markers, 0)

    def test_close_without_open_is_invalid(self) -> None:
        with self.assertRaisesRegex(measure_channel.Invalid, "without an open"):
            measure_channel.pair_markers(
                [(measure_channel.CLOSE | 7, 1_000_010_000)], 0
            )

    def test_close_for_another_sequence_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 6, 1_000_000_000),
            (measure_channel.CLOSE | 7, 1_000_010_000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "does not match"):
            measure_channel.pair_markers(markers, 0)

    def test_unclosed_sample_is_invalid(self) -> None:
        with self.assertRaisesRegex(measure_channel.Invalid, "never closed"):
            measure_channel.pair_markers(
                [(measure_channel.OPEN | 7, 1_000_000_000)], 0
            )

    def test_timestamp_reversal_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 1, 2_000_000_000),
            (measure_channel.CLOSE | 1, 1_000_000_000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "went backwards"):
            measure_channel.pair_markers(markers, 0)

    def test_zero_interval_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 1, 2_000_000_000),
            (measure_channel.CLOSE | 1, 2_000_000_000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "interval of"):
            measure_channel.pair_markers(markers, 0)


class SimpleTraceTests(unittest.TestCase):
    def test_binary_trace_retains_serial_markers_and_nanosecond_clock(self) -> None:
        trace = simple_trace(
            simple_event(7, 1_000_000_000, 0, measure_channel.OPEN | 3),
            simple_event(7, 1_000_004_250, 0, measure_channel.CLOSE | 3),
        )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory, "serial.trace")
            path.write_bytes(trace)

            markers, observer, clock = measure_channel.read_trace(path)

        self.assertEqual(
            markers,
            [
                (measure_channel.OPEN | 3, 1_000_000_000),
                (measure_channel.CLOSE | 3, 1_000_004_250),
            ],
        )
        self.assertEqual(observer["backend"], "QEMU simple trace serial_write")
        self.assertIn("CLOCK_MONOTONIC", observer["clock"])
        self.assertIn("nanosecond", clock)

    def test_binary_trace_refuses_reported_drops(self) -> None:
        trace = simple_trace(simple_event(0xFFFFFFFFFFFFFFFE, 1, 9))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory, "serial.trace")
            path.write_bytes(trace)

            with self.assertRaisesRegex(measure_channel.Invalid, "dropped 9"):
                measure_channel.read_trace(path)

    def test_binary_trace_refuses_truncated_record(self) -> None:
        trace = simple_trace(simple_event(7, 1, 0, measure_channel.OPEN)[:-1])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory, "serial.trace")
            path.write_bytes(trace)

            with self.assertRaisesRegex(measure_channel.Invalid, "truncated"):
                measure_channel.read_trace(path)

    def test_binary_trace_refuses_another_format_version(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory, "serial.trace")
            path.write_bytes(simple_trace(version=3))

            with self.assertRaisesRegex(measure_channel.Invalid, "version 3"):
                measure_channel.read_trace(path)

    def test_text_trace_is_retained_as_integer_nanoseconds(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory, "serial.trace")
            path.write_text(
                "7@123.000001:serial_write write addr 0x00 val 0x83\n"
                "7@123.000009:serial_write write addr 0x00 val 0xa3\n",
                encoding="utf-8",
            )

            markers, observer, _clock = measure_channel.read_trace(path)

        self.assertEqual(markers, [(0x83, 123_000_001_000), (0xA3, 123_000_009_000)])
        self.assertEqual(observer["backend"], "QEMU log trace serial_write")


class StatisticsTests(unittest.TestCase):
    def test_nearest_rank_p99_of_twenty_one_samples_is_the_largest(self) -> None:
        samples = [float(value) for value in range(1, 22)]

        self.assertEqual(measure_channel.percentile(samples, 0.99), 21.0)

    def test_report_declares_its_record_licence_in_the_first_field(self) -> None:
        encoded = measure_channel.encode_report({"count": 21})

        self.assertEqual(
            encoded.splitlines()[1],
            '  "record_spdx_license": "CC-BY-SA-4.0",',
        )


class EnvironmentTests(unittest.TestCase):
    def test_p2_status_is_reserved_for_clean_ci_measurements(self) -> None:
        self.assertEqual(
            measure_channel.evidence_status("P2", False, True, True), "P2"
        )
        with self.assertRaisesRegex(measure_channel.Invalid, "GitHub Actions"):
            measure_channel.evidence_status("P2", False, True, False)
        self.assertEqual(
            measure_channel.evidence_status("P2", True, True, True), "exploratory"
        )

    def test_observer_manifest_binds_launcher_engine_and_roms(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "bin"
            data = root / "share" / "qemu"
            binary.mkdir(parents=True)
            data.mkdir(parents=True)
            launcher = binary / "qemu-system-x86_64"
            engine = binary / "qemu-system-x86_64.real"
            launcher.write_bytes(b"launcher")
            engine.write_bytes(b"engine")
            retained = {}
            for name in ("kvmvapic.bin", "vgabios-stdvga.bin", "efi-e1000e.rom"):
                path = data / name
                path.write_bytes(name.encode("ascii"))
                retained[f"../share/qemu/{name}"] = measure_channel.sha256(path)
            manifest = {
                "record_spdx_license": "CC-BY-SA-4.0",
                "qemu_version": "10.0.11",
                "qemu_source_sha256": "22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871",
                "qemu_sha256": measure_channel.sha256(launcher),
                "qemu_engine_relative_path": "qemu-system-x86_64.real",
                "qemu_engine_sha256": measure_channel.sha256(engine),
                "trace_backends": ["simple"],
                "network_downloads": "disabled",
                "source_date_epoch": 1782452340,
                "build_path_remap": "/usr/src/qemu-10.0.11",
                "configure": measure_channel.SIMPLE_OBSERVER_CONFIGURE,
                "cflags": measure_channel.SIMPLE_CFLAGS_IDENTITY,
                "retained_data": retained,
                "dynamic_dependencies": {
                    str(engine.resolve()): measure_channel.sha256(engine)
                },
            }
            (binary / "observer-build.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )

            self.assertIsNotNone(measure_channel.observer_build_manifest(launcher))
            (data / "kvmvapic.bin").write_bytes(b"tampered")
            with self.assertRaisesRegex(measure_channel.Invalid, "does not match"):
                measure_channel.observer_build_manifest(launcher)

    def test_reference_profile_is_read_from_the_command(self) -> None:
        command = [
            "qemu-system-x86_64",
            "-machine",
            "q35",
            "-cpu",
            "qemu64",
            "-m",
            "256M",
            "-smp",
            "1",
        ]

        self.assertEqual(
            measure_channel.command_profile(command),
            {
                "accelerator": "tcg",
                "cpu": "qemu64",
                "machine": "q35",
                "memory_mib": 256,
                "vcpus": 1,
            },
        )

    def test_non_reference_accelerator_is_refused(self) -> None:
        command = [
            "qemu-system-x86_64",
            "-machine",
            "q35",
            "-cpu",
            "qemu64",
            "-m",
            "256M",
            "-smp",
            "1",
            "-accel",
            "kvm",
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "accelerator"):
            measure_channel.command_profile(command)

    def test_non_reference_machine_is_refused(self) -> None:
        command = [
            "qemu-system-x86_64",
            "-machine",
            "pc",
            "-cpu",
            "qemu64",
            "-m",
            "256M",
            "-smp",
            "1",
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "machine"):
            measure_channel.command_profile(command)

    def test_quantum_is_read_from_the_compiled_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "apic.rs")
            source.write_text("const QUANTUM: u32 = 100_000;\n", encoding="utf-8")

            self.assertEqual(measure_channel.quantum_count(source), 100_000)

    def test_ambiguous_quantum_source_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "apic.rs")
            source.write_text(
                "const QUANTUM: u32 = 100_000;\nconst QUANTUM: u32 = 1;\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(measure_channel.Invalid, "exactly one"):
                measure_channel.quantum_count(source)


if __name__ == "__main__":
    unittest.main()
