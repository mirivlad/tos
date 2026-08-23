#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail-closed qualification of the ADR-0066 external observer."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import statistics
import sys
from pathlib import Path


class Invalid(Exception):
    """The adjacent-pair report does not prove one resolvable observer."""


def _samples(report: dict[str, object], field: str, name: str) -> list[float]:
    samples = report.get(field)
    if not isinstance(samples, list) or len(samples) != 21:
        raise Invalid(f"{name} report does not retain 21 samples")
    if not all(
        not isinstance(value, bool)
        and isinstance(value, (int, float))
        and value > 0
        for value in samples
    ):
        raise Invalid(f"{name} report contains a non-positive sample")
    return [float(value) for value in samples]


def _check_statistics(
    samples: list[float], retained: object, name: str
) -> dict[str, float]:
    if not isinstance(retained, dict):
        raise Invalid(f"{name} statistics are missing")
    expected = {
        "median_us": statistics.median(samples),
        "p99_us": max(samples),
        "min_us": min(samples),
        "max_us": max(samples),
    }
    for field, value in expected.items():
        if retained.get(field) != value:
            raise Invalid(f"{name} {field} does not match its raw samples")
    return expected


def qualify(report: dict[str, object], expected_status: str) -> dict[str, object]:
    """Return the qualification summary or reject one adjacent-pair report."""
    if report.get("record_spdx_license") != "CC-BY-SA-4.0":
        raise Invalid("measurement report has no retained-record licence")
    if report.get("measurement_mode") != "adjacent-floor-call-pairs-v1":
        raise Invalid("measurement report is not an adjacent-pair calibration")
    if report.get("subtracted") != "nothing":
        raise Invalid("measurement report subtracts observer cost")
    if report.get("warmups") != 3 or report.get("count") != 21:
        raise Invalid("measurement report does not use the 3+21 block discipline")
    floor_samples = _samples(report, "floor_samples_us", "floor")
    call_samples = _samples(report, "samples_us", "call")
    call_statistics = _check_statistics(call_samples, report, "call")
    floor_statistics = _check_statistics(
        floor_samples, report.get("floor_statistics"), "floor"
    )
    expected_order = [
        "call-floor" if block % 2 else "floor-call" for block in range(3, 24)
    ]
    if report.get("pair_order") != expected_order:
        raise Invalid("measurement report does not retain the predeclared alternating order")

    environment = report.get("environment")
    if not isinstance(environment, dict):
        raise Invalid("measurement report has no environment")
    if environment.get("evidence_status") != expected_status:
        raise Invalid(f"measurement evidence status is not {expected_status}")
    source = environment.get("source")
    if not isinstance(source, dict) or source.get("dirty") is not False:
        raise Invalid("measurement source tree was not clean")
    observer = environment.get("observer")
    if not isinstance(observer, dict):
        raise Invalid("measurement report has no observer identity")
    if observer.get("backend") != "QEMU simple trace symmetric UART measurement pair":
        raise Invalid("measurement did not use the symmetric QEMU simple observer")
    observer_manifest = observer.get("build_manifest")
    if not isinstance(observer_manifest, dict) or not observer_manifest.get("sha256"):
        raise Invalid("observer has no build manifest")
    observer_contents = observer_manifest.get("contents")
    if (
        not isinstance(observer_contents, dict)
        or observer_contents.get("qemu_sha256") != observer.get("qemu_sha256")
    ):
        raise Invalid("observer identity does not match its build manifest")
    clock = report.get("clock")
    if not isinstance(clock, str):
        raise Invalid("measurement has no clock identity")
    if "QEMU observer pair (simple backend, CLOCK_THREAD_CPUTIME_ID" not in clock:
        raise Invalid("reports do not identify the QEMU thread-CPU observer pair")
    scheduler = environment.get("scheduler")
    if not isinstance(scheduler, dict) or scheduler.get("preemption") != "inactive":
        raise Invalid("floor and denominator must use the conservative no-preemption profile")
    if scheduler.get("binding") != "measurement-build-manifest":
        raise Invalid("scheduler state is not bound to the measurement build")
    measurement_build = environment.get("measurement_build")
    if not isinstance(measurement_build, dict) or not measurement_build.get("sha256"):
        raise Invalid("measurement build identity is missing")
    build_contents = measurement_build.get("contents")
    builds = build_contents.get("builds") if isinstance(build_contents, dict) else None
    expected_features = {
        "nucleus": ["test-measurement-no-preemption"],
        "runtime_image": ["test-measurement-call"],
    }
    if not isinstance(builds, dict):
        raise Invalid("measurement build contents are missing")
    artifacts = environment.get("artifacts")
    if not isinstance(artifacts, dict):
        raise Invalid("measurement artifact identities are missing")
    for name, features in expected_features.items():
        record = builds.get(name)
        artifact_name = f"measurement_{name}"
        artifact = artifacts.get(artifact_name)
        if not isinstance(record, dict) or record.get("features") != features:
            raise Invalid(f"measurement build {name} features are not qualified")
        if (
            not isinstance(artifact, dict)
            or record.get("artifact_sha256") != artifact.get("sha256")
        ):
            raise Invalid(f"measurement build {name} does not bind its artifact")
    isolation = environment.get("production_artifact_isolation")
    required_isolation = {"production_nucleus", "production_runtime_image"}
    if not isinstance(isolation, dict) or set(isolation) != required_isolation:
        raise Invalid("measurement has no production-artifact isolation record")
    if any(
        not isinstance(record, dict) or record.get("unchanged") is not True
        for record in isolation.values()
    ):
        raise Invalid("measurement changed a production artifact")
    pair_gaps = [
        call_sample - floor_sample
        for floor_sample, call_sample in zip(floor_samples, call_samples, strict=True)
    ]
    positive_pairs = sum(gap > 0 for gap in pair_gaps)
    sign_p = sum(
        math.comb(len(pair_gaps), successes)
        for successes in range(positive_pairs, len(pair_gaps) + 1)
    ) / (2 ** len(pair_gaps))
    required_positive_pairs = 19
    if positive_pairs < required_positive_pairs:
        raise Invalid(
            f"only {positive_pairs} of {len(pair_gaps)} adjacent pairs resolve "
            f"the call; the contract requires {required_positive_pairs} "
            "(one-sided exact sign p <= 0.000111)"
        )
    if floor_statistics["p99_us"] > 40.0:
        raise Invalid(f"floor p99 {floor_statistics['p99_us']:.3f} us exceeds 40 us")

    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "adr": "ADR-0066",
        "evidence_status": expected_status,
        "verdict": "observer-qualified",
        "source_commit": source["commit"],
        "observer": {
            "backend": observer["backend"],
            "qemu_sha256": observer["qemu_sha256"],
            "build_manifest_sha256": observer["build_manifest"]["sha256"],
        },
        "measurement_build_manifest_sha256": measurement_build["sha256"],
        "floor": {
            **floor_statistics,
        },
        "denominator": {
            **call_statistics,
        },
        "paired_resolution": {
            "resolved": positive_pairs,
            "positive_pairs": positive_pairs,
            "total_pairs": len(pair_gaps),
            "required_positive_pairs": required_positive_pairs,
            "one_sided_sign_p": sign_p,
            "minimum_gap_us": round(min(pair_gaps), 3),
            "median_gap_us": round(statistics.median(pair_gaps), 3),
        },
        "floor_over_denominator": {
            "median": round(
                floor_statistics["median_us"] / call_statistics["median_us"], 6
            ),
            "p99": round(
                floor_statistics["p99_us"] / call_statistics["p99_us"], 6
            ),
        },
        "subtracted": "nothing",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measurement", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--evidence-status", required=True, choices=("P1", "P2"))
    args = parser.parse_args()
    try:
        measurement_bytes = args.measurement.read_bytes()
        measurement = json.loads(measurement_bytes)
        summary = qualify(measurement, args.evidence_status)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, Invalid) as error:
        print(f"qualify-observer: FAIL: {error}", file=sys.stderr)
        return 1
    summary["measurement_report_sha256"] = hashlib.sha256(measurement_bytes).hexdigest()
    summary["report"] = str(args.measurement.resolve())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(
        "QUALIFY-OBSERVER PASS: "
        f"evidence={args.evidence_status} "
        f"positive_pairs={summary['paired_resolution']['positive_pairs']}/"
        f"{summary['paired_resolution']['total_pairs']} "
        f"sign_p={summary['paired_resolution']['one_sided_sign_p']:.6g}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
