#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression tests for the Stage 3 IPC latency verdict."""

from __future__ import annotations

import importlib.util
import hashlib
import json
import statistics
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("qualify-ipc.py")
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("qualify_ipc", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
qualify_ipc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qualify_ipc)


def common_environment() -> dict[str, object]:
    return {
        "evidence_status": "P1",
        "source": {"commit": "abc", "dirty": False},
        "observer": {
            "backend": "QEMU simple trace symmetric UART measurement pair",
            "qemu_sha256": "qemu",
            "build_manifest": {"sha256": "observer-manifest"},
        },
        "guest_profile": {
            "accelerator": "tcg",
            "cpu": "qemu64",
            "machine": "q35",
            "memory_mib": 256,
            "vcpus": 1,
        },
        "host": {"cpu_model": "test", "rustc": "rustc test"},
    }


def report(values: list[float], mode: str, environment: dict[str, object]) -> dict[str, object]:
    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "measurement_mode": mode,
        "samples_us": values,
        "warmups": 3,
        "count": 21,
        "median_us": statistics.median(values),
        "p99_us": max(values),
        "min_us": min(values),
        "max_us": max(values),
        "clock": "clock",
        "subtracted": "nothing",
        "environment": environment,
    }


def records(
    denominator_p99: float = 30.0, numerator_p99: float = 100.0
) -> tuple[dict[str, object], dict[str, object], dict[str, object], bytes]:
    denominator_environment = common_environment()
    numerator_environment = common_environment()
    denominator_environment["scheduler"] = {
        "preemption": "inactive",
        "binding": "measurement-build-manifest",
        "quantum_count": 100000,
    }
    numerator_environment["scheduler"] = {
        "preemption": "active",
        "binding": "measurement-build-manifest",
        "quantum_count": 100000,
    }
    denominator_environment["measurement_build"] = {
        "sha256": "denominator-build",
        "contents": {
            "builds": {
                "nucleus": {
                    "features": ["test-measurement-no-preemption"],
                    "artifact_sha256": "denominator-nucleus",
                },
                "runtime_image": {
                    "features": ["test-measurement-call"],
                    "artifact_sha256": "denominator-runtime",
                },
            }
        },
    }
    denominator_environment["artifacts"] = {
        "measurement_nucleus": {"sha256": "denominator-nucleus"},
        "measurement_runtime_image": {"sha256": "denominator-runtime"},
    }
    denominator_environment["production_artifact_isolation"] = {
        "production_nucleus": {"unchanged": True},
        "production_runtime_image": {"unchanged": True},
    }
    numerator_environment["measurement_build"] = {
        "sha256": "ipc-build",
        "contents": {
            "builds": {
                "nucleus": {
                    "features": ["test-call-reply", "test-measurement-port"],
                    "artifact_sha256": "nucleus",
                },
                "runtime_image": {
                    "features": ["test-measurement-ipc"],
                    "artifact_sha256": "runtime",
                },
            }
        },
    }
    numerator_environment["artifacts"] = {
        "measurement_nucleus": {"sha256": "nucleus"},
        "measurement_runtime_image": {"sha256": "runtime"},
    }
    numerator_environment["production_artifact_isolation"] = {
        "production_nucleus": {"unchanged": True},
        "production_runtime_image": {"unchanged": True},
    }
    denominator_values = [10.0] * 20 + [denominator_p99]
    numerator_values = [50.0] * 20 + [numerator_p99]
    denominator = report(
        denominator_values, "adjacent-floor-call-pairs-v1", denominator_environment
    )
    numerator = report(numerator_values, "ipc-request-reply-v1", numerator_environment)
    qualification = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "verdict": "observer-qualified",
        "evidence_status": "P1",
        "source_commit": "abc",
        "observer": {
            "backend": "QEMU simple trace symmetric UART measurement pair",
            "qemu_sha256": "qemu",
            "build_manifest_sha256": "observer-manifest",
        },
        "measurement_build_manifest_sha256": "denominator-build",
        "measurement_report_sha256": hashlib.sha256(
            json.dumps(denominator).encode("utf-8")
        ).hexdigest(),
        "denominator": {
            "median_us": statistics.median(denominator_values),
            "p99_us": denominator_p99,
            "min_us": min(denominator_values),
            "max_us": max(denominator_values),
        },
        "subtracted": "nothing",
    }
    serial = (
        "TOS.RUN.MEASURE.IPC samples=24 answered=24 refused=0 "
        "request_bytes=64 reply_bytes=64 primed=1\n"
        "TOS.RUN.MEASURE.IPC.SERVER served=25 refused=0 payload_bytes=64 last=-5\n"
        "TOS.RUN.IPC.COST messages=50 payload_copies=75 ipc_in=51 other_in=42 "
        "returns=41 resumptions=52 exchanges=25 ipc_out=51\n"
    ).encode("ascii")
    return denominator, qualification, numerator, serial


def qualify_records(
    denominator: dict[str, object],
    observer: dict[str, object],
    numerator: dict[str, object],
    serial: bytes,
) -> dict[str, object]:
    return qualify_ipc.qualify(
        denominator,
        hashlib.sha256(json.dumps(denominator).encode("utf-8")).hexdigest(),
        observer,
        numerator,
        serial,
        "P1",
    )


class QualificationTests(unittest.TestCase):
    def test_both_latency_budgets_and_workload_qualify(self) -> None:
        denominator, observer, numerator, serial = records()

        result = qualify_records(denominator, observer, numerator, serial)

        self.assertEqual(result["verdict"], "ipc-latency-qualified")
        self.assertAlmostEqual(result["budgets"]["relative_ratio"], 100.0 / 30.0)
        self.assertEqual(result["workload"]["request_bytes"], 64)

    def test_absolute_budget_failure_is_not_hidden_by_relative_pass(self) -> None:
        denominator, observer, numerator, serial = records(30.0, 201.0)

        result = qualify_records(denominator, observer, numerator, serial)

        self.assertEqual(result["verdict"], "ipc-latency-red")
        self.assertTrue(result["budgets"]["relative_pass"])
        self.assertFalse(result["budgets"]["absolute_pass"])

    def test_relative_budget_failure_is_not_hidden_by_absolute_pass(self) -> None:
        denominator, observer, numerator, serial = records(10.0, 100.0)

        result = qualify_records(denominator, observer, numerator, serial)

        self.assertEqual(result["verdict"], "ipc-latency-red")
        self.assertFalse(result["budgets"]["relative_pass"])
        self.assertTrue(result["budgets"]["absolute_pass"])

    def test_inactive_preemption_is_refused(self) -> None:
        denominator, observer, numerator, serial = records()
        numerator["environment"]["scheduler"]["preemption"] = "inactive"

        with self.assertRaisesRegex(qualify_ipc.Invalid, "active preemption"):
            qualify_records(denominator, observer, numerator, serial)

    def test_tampered_payload_record_is_refused(self) -> None:
        denominator, observer, numerator, serial = records()
        serial = serial.replace(b"request_bytes=64", b"request_bytes=63")

        with self.assertRaisesRegex(qualify_ipc.Invalid, "measured client record"):
            qualify_records(denominator, observer, numerator, serial)

    def test_observer_identity_mismatch_is_refused(self) -> None:
        denominator, observer, numerator, serial = records()
        numerator["environment"]["observer"]["qemu_sha256"] = "other"

        with self.assertRaisesRegex(qualify_ipc.Invalid, "observer identities differ"):
            qualify_records(denominator, observer, numerator, serial)

    def test_denominator_report_must_match_observer_qualification_hash(self) -> None:
        denominator, observer, numerator, serial = records()
        denominator["samples_us"][0] = 11.0

        with self.assertRaisesRegex(qualify_ipc.Invalid, "exact denominator report"):
            qualify_records(denominator, observer, numerator, serial)

    def test_denominator_build_features_are_checked(self) -> None:
        denominator, observer, numerator, serial = records()
        denominator["environment"]["measurement_build"]["contents"]["builds"][
            "nucleus"
        ]["features"] = ["test-call-reply"]
        observer["measurement_report_sha256"] = hashlib.sha256(
            json.dumps(denominator).encode("utf-8")
        ).hexdigest()

        with self.assertRaisesRegex(qualify_ipc.Invalid, "denominator nucleus features"):
            qualify_records(denominator, observer, numerator, serial)

    def test_failed_budget_is_retained_as_red_evidence(self) -> None:
        denominator, observer, numerator, serial = records(30.0, 201.0)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = {
                "denominator": denominator,
                "observer": observer,
                "numerator": numerator,
            }
            for name, value in inputs.items():
                (root / f"{name}.json").write_text(json.dumps(value), encoding="utf-8")
            (root / "serial.log").write_bytes(serial)
            output = root / "qualification.json"

            completed = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--denominator",
                    str(root / "denominator.json"),
                    "--observer-qualification",
                    str(root / "observer.json"),
                    "--numerator",
                    str(root / "numerator.json"),
                    "--serial-log",
                    str(root / "serial.log"),
                    "--out",
                    str(output),
                    "--evidence-status",
                    "P1",
                ],
                capture_output=True,
                text=True,
            )

            self.assertEqual(completed.returncode, 1)
            retained = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(retained["verdict"], "ipc-latency-red")
            self.assertEqual(retained["numerator"]["p99_us"], 201.0)
            self.assertTrue(retained["budgets"]["relative_pass"])
            self.assertFalse(retained["budgets"]["absolute_pass"])


if __name__ == "__main__":
    unittest.main()
