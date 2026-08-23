#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression tests for the external measurement observer."""

from __future__ import annotations

import importlib.util
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


class PairingTests(unittest.TestCase):
    def test_valid_pairs_retain_every_sample_after_warmups(self) -> None:
        markers = [
            (measure_channel.OPEN | 0, 1.000000),
            (measure_channel.CLOSE | 0, 1.000010),
            (measure_channel.OPEN | 1, 2.000000),
            (measure_channel.CLOSE | 1, 2.000020),
            (measure_channel.OPEN | 2, 3.000000),
            (measure_channel.CLOSE | 2, 3.000030),
        ]

        samples = measure_channel.pair_markers(markers, 1)
        self.assertEqual(len(samples), 2)
        self.assertAlmostEqual(samples[0], 20.0)
        self.assertAlmostEqual(samples[1], 30.0)

    def test_duplicate_open_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 4, 1.000000),
            (measure_channel.OPEN | 4, 1.000001),
            (measure_channel.CLOSE | 4, 1.000010),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "opened twice"):
            measure_channel.pair_markers(markers, 0)

    def test_overlapping_samples_are_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 4, 1.000000),
            (measure_channel.OPEN | 5, 1.000001),
            (measure_channel.CLOSE | 4, 1.000010),
            (measure_channel.CLOSE | 5, 1.000020),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "still open"):
            measure_channel.pair_markers(markers, 0)

    def test_close_without_open_is_invalid(self) -> None:
        with self.assertRaisesRegex(measure_channel.Invalid, "without an open"):
            measure_channel.pair_markers(
                [(measure_channel.CLOSE | 7, 1.000010)], 0
            )

    def test_close_for_another_sequence_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 6, 1.000000),
            (measure_channel.CLOSE | 7, 1.000010),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "does not match"):
            measure_channel.pair_markers(markers, 0)

    def test_unclosed_sample_is_invalid(self) -> None:
        with self.assertRaisesRegex(measure_channel.Invalid, "never closed"):
            measure_channel.pair_markers(
                [(measure_channel.OPEN | 7, 1.000000)], 0
            )

    def test_timestamp_reversal_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 1, 2.000000),
            (measure_channel.CLOSE | 1, 1.000000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "went backwards"):
            measure_channel.pair_markers(markers, 0)

    def test_zero_interval_is_invalid(self) -> None:
        markers = [
            (measure_channel.OPEN | 1, 2.000000),
            (measure_channel.CLOSE | 1, 2.000000),
        ]

        with self.assertRaisesRegex(measure_channel.Invalid, "interval of"):
            measure_channel.pair_markers(markers, 0)


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
