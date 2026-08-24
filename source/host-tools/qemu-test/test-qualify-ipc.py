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


def nearest_rank_p99(values: list[float]) -> float:
    ordered = sorted(values)
    return ordered[max(1, -(-99 * len(ordered) // 100)) - 1]


def report(values: list[float], mode: str, environment: dict[str, object]) -> dict[str, object]:
    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "measurement_mode": mode,
        "samples_us": values,
        "warmups": 3,
        "count": len(values),
        "median_us": statistics.median(values),
        "p99_us": nearest_rank_p99(values),
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
        "apic_divider": 16,
    }
    numerator_environment["scheduler"] = {
        "preemption": "active",
        "binding": "measurement-build-manifest",
        "quantum_count": 100000,
        "apic_divider": 16,
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
    # 300 samples: rank 297 is the p99, so the three above it are the ones a
    # maximum-based reader would have reported instead.
    numerator_values = [50.0] * 296 + [numerator_p99] + [numerator_p99 + 5.0] * 3
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
    bound = qualify_ipc.expected_workload(qualify_ipc.LATENCY_SAMPLES)
    serial = (
        f"TOS.RUN.MEASURE.IPC samples={bound['measured']} answered={bound['measured']} "
        "refused=0 request_bytes=64 reply_bytes=64 primed=1\n"
        f"TOS.RUN.MEASURE.IPC.SERVER served={bound['served']} refused=0 "
        "payload_bytes=64 last=-5\n"
        f"TOS.RUN.IPC.COST messages={bound['messages']} "
        f"payload_copies={3 * bound['exchanges']} ipc_in={bound['crossings']} "
        f"other_in=42 returns=41 resumptions=52 exchanges={bound['exchanges']} "
        f"ipc_out={bound['crossings']}\n"
    ).encode("ascii")
    return denominator, qualification, numerator, serial


INPUT_FILES = {
    "denominator": "denominator.json",
    "observer_qualification": "observer.json",
    "numerator": "numerator.json",
    "serial_log": "serial.log",
}


def write_records(
    root: Path,
    denominator: dict[str, object],
    observer: dict[str, object],
    numerator: dict[str, object],
    serial: bytes,
) -> Path:
    """Lay the four gate inputs out on disk under their CLI names."""
    for name, value in (
        ("denominator", denominator),
        ("observer_qualification", observer),
        ("numerator", numerator),
    ):
        (root / INPUT_FILES[name]).write_text(json.dumps(value), encoding="utf-8")
    (root / INPUT_FILES["serial_log"]).write_bytes(serial)
    return root


def run_cli(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(MODULE_PATH),
            "--denominator",
            str(root / INPUT_FILES["denominator"]),
            "--observer-qualification",
            str(root / INPUT_FILES["observer_qualification"]),
            "--numerator",
            str(root / INPUT_FILES["numerator"]),
            "--serial-log",
            str(root / INPUT_FILES["serial_log"]),
            "--out",
            str(root / "qualification.json"),
            "--evidence-status",
            "P1",
        ],
        capture_output=True,
        text=True,
    )


def on_disk_sha256(root: Path, name: str) -> str:
    return hashlib.sha256((root / INPUT_FILES[name]).read_bytes()).hexdigest()


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
    def test_the_absolute_budget_and_the_workload_qualify(self) -> None:
        denominator, observer, numerator, serial = records()

        result = qualify_records(denominator, observer, numerator, serial)

        self.assertEqual(result["verdict"], "ipc-latency-qualified")
        self.assertTrue(result["budgets"]["absolute_pass"])
        self.assertEqual(result["workload"]["request_bytes"], 64)
        self.assertEqual(
            result["workload"]["measured_exchanges"],
            3 + qualify_ipc.LATENCY_SAMPLES,
        )

    def test_the_absolute_budget_is_the_one_that_can_fail(self) -> None:
        denominator, observer, numerator, serial = records(30.0, 201.0)

        result = qualify_records(denominator, observer, numerator, serial)

        self.assertEqual(result["verdict"], "ipc-latency-red")
        self.assertFalse(result["budgets"]["absolute_pass"])

    def test_a_ratio_far_above_eight_still_qualifies(self) -> None:
        # ADR-0068: the ratio is retained and decides nothing. 100 over 10 is
        # 10x, which the withdrawn bound would have failed.
        denominator, observer, numerator, serial = records(10.0, 100.0)

        result = qualify_records(denominator, observer, numerator, serial)

        self.assertEqual(result["verdict"], "ipc-latency-qualified")
        self.assertAlmostEqual(result["observational"]["relative_ratio"], 10.0)
        self.assertFalse(result["observational"]["is_a_budget"])
        self.assertNotIn("relative_pass", result["budgets"])
        self.assertNotIn("relative_limit_us", result["budgets"])

    def test_the_p99_is_rank_297_and_not_the_maximum(self) -> None:
        denominator, observer, numerator, serial = records(30.0, 100.0)

        result = qualify_records(denominator, observer, numerator, serial)

        # The fixture puts three larger samples above rank 297 on purpose.
        self.assertEqual(result["numerator"]["p99_us"], 100.0)
        self.assertEqual(result["numerator"]["max_us"], 105.0)

    def test_a_series_of_the_wrong_length_is_refused(self) -> None:
        denominator, observer, numerator, serial = records()
        numerator["samples_us"] = numerator["samples_us"][:-1]
        numerator["count"] = len(numerator["samples_us"])

        with self.assertRaisesRegex(qualify_ipc.Invalid, "3\\+300 discipline"):
            qualify_records(denominator, observer, numerator, serial)

    def test_a_platform_with_another_quantum_is_refused(self) -> None:
        denominator, observer, numerator, serial = records()
        numerator["environment"]["scheduler"]["quantum_count"] = 10000

        with self.assertRaisesRegex(qualify_ipc.Invalid, "quantum is 10000"):
            qualify_records(denominator, observer, numerator, serial)

    def test_a_platform_with_another_apic_divider_is_refused(self) -> None:
        denominator, observer, numerator, serial = records()
        numerator["environment"]["scheduler"]["apic_divider"] = 128

        with self.assertRaisesRegex(qualify_ipc.Invalid, "APIC divider is 128"):
            qualify_records(denominator, observer, numerator, serial)

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
        with tempfile.TemporaryDirectory() as directory:
            root = write_records(Path(directory), *records(30.0, 201.0))

            completed = run_cli(root)

            self.assertEqual(completed.returncode, 1)
            retained = json.loads((root / "qualification.json").read_text("utf-8"))
            self.assertEqual(retained["verdict"], "ipc-latency-red")
            self.assertEqual(retained["numerator"]["p99_us"], 201.0)
            self.assertFalse(retained["budgets"]["absolute_pass"])
            self.assertIn("relative_ratio", retained["observational"])

    def test_retained_verdict_binds_each_input_by_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = write_records(Path(directory), *records())

            completed = run_cli(root)

            self.assertEqual(completed.returncode, 0)
            retained = json.loads((root / "qualification.json").read_text("utf-8"))
            self.assertEqual(
                retained["reports_sha256"],
                {name: on_disk_sha256(root, name) for name in INPUT_FILES},
            )

    def test_a_digest_is_recorded_for_every_input_actually_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = write_records(Path(directory), *records())
            (root / INPUT_FILES["serial_log"]).unlink()

            completed = run_cli(root)

            self.assertEqual(completed.returncode, 1)
            retained = json.loads((root / "qualification.json").read_text("utf-8"))
            self.assertEqual(retained["verdict"], "ipc-evidence-invalid")
            # The serial log was never read, so no digest may be invented for
            # it; the three files that were read are still bound by theirs.
            self.assertEqual(
                retained["reports_sha256"],
                {
                    name: on_disk_sha256(root, name)
                    for name in ("denominator", "observer_qualification", "numerator")
                },
            )


if __name__ == "__main__":
    unittest.main()
