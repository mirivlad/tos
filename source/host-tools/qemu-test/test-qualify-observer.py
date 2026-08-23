#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression tests for the ADR-0066 observer qualification verdict."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("qualify-observer.py")
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("qualify_observer", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
qualify_observer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qualify_observer)


def report(samples: list[float], status: str = "P1") -> dict[str, object]:
    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "samples_us": samples,
        "warmups": 3,
        "count": 21,
        "median_us": sorted(samples)[10],
        "p99_us": max(samples),
        "min_us": min(samples),
        "max_us": max(samples),
        "clock": (
            "QEMU trace timestamp (simple backend, CLOCK_MONOTONIC, nanoseconds) "
            "taken immediately before the device model handles the guest's write"
        ),
        "subtracted": "nothing",
        "environment": {
            "evidence_status": status,
            "source": {"commit": "abc", "dirty": False},
            "observer": {
                "backend": "QEMU simple trace serial_write",
                "qemu_sha256": "qemu",
                "build_manifest": {"sha256": "manifest"},
            },
            "guest_profile": {
                "accelerator": "tcg",
                "cpu": "qemu64",
                "machine": "q35",
                "memory_mib": 256,
                "vcpus": 1,
            },
            "host": {"cpu_model": "test", "rustc": "rustc test"},
            "scheduler": {"preemption": "active", "quantum_count": 100000},
            "production_artifact_isolation": {
                "production_nucleus": {"unchanged": True},
                "production_runtime_image": {"unchanged": True},
            },
        },
    }


class QualificationTests(unittest.TestCase):
    def test_separated_floor_and_call_qualify_without_subtraction(self) -> None:
        floor = report([4.0] * 20 + [9.0])
        call = report([13.0] * 20 + [20.0])

        result = qualify_observer.qualify(floor, call, "P1")

        self.assertEqual(result["verdict"], "observer-qualified")
        self.assertEqual(result["range_gap_us"], 4.0)
        self.assertEqual(result["subtracted"], "nothing")

    def test_overlapping_ranges_fail_closed(self) -> None:
        floor = report([4.0] * 20 + [14.0])
        call = report([13.0] * 21)

        with self.assertRaisesRegex(qualify_observer.Invalid, "overlap"):
            qualify_observer.qualify(floor, call, "P1")

    def test_mixed_observer_identity_is_refused(self) -> None:
        floor = report([4.0] * 21)
        call = report([13.0] * 21)
        call["environment"]["observer"]["qemu_sha256"] = "other"

        with self.assertRaisesRegex(qualify_observer.Invalid, "observer identity"):
            qualify_observer.qualify(floor, call, "P1")

    def test_mixed_clock_identity_is_refused(self) -> None:
        floor = report([4.0] * 21)
        call = report([13.0] * 21)
        call["clock"] = "another clock"

        with self.assertRaisesRegex(qualify_observer.Invalid, "clock identities"):
            qualify_observer.qualify(floor, call, "P1")

    def test_incomplete_production_isolation_is_refused(self) -> None:
        floor = report([4.0] * 21)
        call = report([13.0] * 21)
        del call["environment"]["production_artifact_isolation"][
            "production_runtime_image"
        ]

        with self.assertRaisesRegex(qualify_observer.Invalid, "isolation record"):
            qualify_observer.qualify(floor, call, "P1")

    def test_p2_cannot_be_claimed_by_p1_inputs(self) -> None:
        with self.assertRaisesRegex(qualify_observer.Invalid, "status"):
            qualify_observer.qualify(
                report([4.0] * 21), report([13.0] * 21), "P2"
            )


if __name__ == "__main__":
    unittest.main()
