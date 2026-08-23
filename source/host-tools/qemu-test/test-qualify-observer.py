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


def report(
    floor_samples: list[float], call_samples: list[float], status: str = "P1"
) -> dict[str, object]:
    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "measurement_mode": "adjacent-floor-call-pairs-v1",
        "samples_us": call_samples,
        "floor_samples_us": floor_samples,
        "pair_order": [
            "call-floor" if block % 2 else "floor-call" for block in range(3, 24)
        ],
        "warmups": 3,
        "count": 21,
        "median_us": sorted(call_samples)[10],
        "p99_us": max(call_samples),
        "min_us": min(call_samples),
        "max_us": max(call_samples),
        "floor_statistics": {
            "median_us": sorted(floor_samples)[10],
            "p99_us": max(floor_samples),
            "min_us": min(floor_samples),
            "max_us": max(floor_samples),
        },
        "clock": (
            "QEMU observer pair (simple backend, CLOCK_THREAD_CPUTIME_ID, "
            "nanoseconds of the one TCG vCPU thread) taken after OPEN handling "
            "and before CLOSE handling"
        ),
        "subtracted": "nothing",
        "environment": {
            "evidence_status": status,
            "source": {"commit": "abc", "dirty": False},
            "observer": {
                "backend": "QEMU simple trace symmetric UART measurement pair",
                "qemu_sha256": "qemu",
                "build_manifest": {
                    "sha256": "manifest",
                    "contents": {"qemu_sha256": "qemu"},
                },
            },
            "guest_profile": {
                "accelerator": "tcg",
                "cpu": "qemu64",
                "machine": "q35",
                "memory_mib": 256,
                "vcpus": 1,
            },
            "host": {"cpu_model": "test", "rustc": "rustc test"},
            "scheduler": {
                "preemption": "inactive",
                "binding": "measurement-build-manifest",
                "quantum_count": 100000,
            },
            "measurement_build": {
                "sha256": "measurement-manifest",
                "contents": {
                    "builds": {
                        "nucleus": {
                            "features": ["test-measurement-no-preemption"],
                            "artifact_sha256": "nucleus",
                        },
                        "runtime_image": {
                            "features": ["test-measurement-call"],
                            "artifact_sha256": "runtime",
                        },
                    }
                },
            },
            "artifacts": {
                "measurement_nucleus": {"sha256": "nucleus"},
                "measurement_runtime_image": {"sha256": "runtime"},
            },
            "production_artifact_isolation": {
                "production_nucleus": {"unchanged": True},
                "production_runtime_image": {"unchanged": True},
            },
        },
    }


class QualificationTests(unittest.TestCase):
    def test_every_predeclared_pair_qualifies_without_subtraction(self) -> None:
        measurement = report([4.0] * 20 + [9.0], [13.0] * 20 + [20.0])

        result = qualify_observer.qualify(measurement, "P1")

        self.assertEqual(result["verdict"], "observer-qualified")
        self.assertEqual(result["paired_resolution"]["resolved"], 21)
        self.assertEqual(result["paired_resolution"]["minimum_gap_us"], 9.0)
        self.assertEqual(result["subtracted"], "nothing")

    def test_crossing_global_ranges_still_requires_every_declared_pair(self) -> None:
        measurement = report([4.0] * 20 + [14.0], [13.0] * 20 + [20.0])

        result = qualify_observer.qualify(measurement, "P1")

        self.assertEqual(result["paired_resolution"]["resolved"], 21)

    def test_one_unresolved_pair_is_retained_under_predeclared_sign_test(self) -> None:
        measurement = report([4.0] * 20 + [14.0], [13.0] * 20 + [13.0])

        result = qualify_observer.qualify(measurement, "P1")

        self.assertEqual(result["paired_resolution"]["positive_pairs"], 20)
        self.assertLess(result["paired_resolution"]["one_sided_sign_p"], 0.000111)

    def test_fewer_than_nineteen_positive_pairs_fails_closed(self) -> None:
        measurement = report([14.0] * 3 + [4.0] * 18, [13.0] * 21)

        with self.assertRaisesRegex(qualify_observer.Invalid, "18 of 21"):
            qualify_observer.qualify(measurement, "P1")

    def test_mixed_observer_identity_is_refused(self) -> None:
        measurement = report([4.0] * 21, [13.0] * 21)
        measurement["environment"]["observer"]["qemu_sha256"] = "other"

        with self.assertRaisesRegex(qualify_observer.Invalid, "observer identity"):
            qualify_observer.qualify(measurement, "P1")

    def test_mixed_clock_identity_is_refused(self) -> None:
        measurement = report([4.0] * 21, [13.0] * 21)
        measurement["clock"] = "another clock"

        with self.assertRaisesRegex(qualify_observer.Invalid, "thread-CPU"):
            qualify_observer.qualify(measurement, "P1")

    def test_incomplete_production_isolation_is_refused(self) -> None:
        measurement = report([4.0] * 21, [13.0] * 21)
        del measurement["environment"]["production_artifact_isolation"][
            "production_runtime_image"
        ]

        with self.assertRaisesRegex(qualify_observer.Invalid, "isolation record"):
            qualify_observer.qualify(measurement, "P1")

    def test_p2_cannot_be_claimed_by_p1_inputs(self) -> None:
        with self.assertRaisesRegex(qualify_observer.Invalid, "status"):
            qualify_observer.qualify(report([4.0] * 21, [13.0] * 21), "P2")

    def test_unbound_scheduler_feature_is_refused(self) -> None:
        measurement = report([4.0] * 21, [13.0] * 21)
        measurement["environment"]["measurement_build"] = None

        with self.assertRaisesRegex(qualify_observer.Invalid, "measurement build"):
            qualify_observer.qualify(measurement, "P1")

    def test_tampered_pair_order_is_refused(self) -> None:
        measurement = report([4.0] * 21, [13.0] * 21)
        measurement["pair_order"][0] = "floor-call"

        with self.assertRaisesRegex(qualify_observer.Invalid, "alternating order"):
            qualify_observer.qualify(measurement, "P1")

    def test_measurement_manifest_digest_must_match_retained_artifact(self) -> None:
        measurement = report([4.0] * 21, [13.0] * 21)
        measurement["environment"]["artifacts"]["measurement_nucleus"][
            "sha256"
        ] = "tampered"

        with self.assertRaisesRegex(qualify_observer.Invalid, "bind its artifact"):
            qualify_observer.qualify(measurement, "P1")


if __name__ == "__main__":
    unittest.main()
