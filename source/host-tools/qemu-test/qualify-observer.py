#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail-closed qualification of the ADR-0066 external observer."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path


class Invalid(Exception):
    """The two reports do not prove one resolvable observer."""


def _samples(report: dict[str, object], name: str, expected_status: str) -> list[float]:
    if report.get("record_spdx_license") != "CC-BY-SA-4.0":
        raise Invalid(f"{name} report has no retained-record licence")
    if report.get("subtracted") != "nothing":
        raise Invalid(f"{name} report subtracts observer cost")
    samples = report.get("samples_us")
    if not isinstance(samples, list) or len(samples) != 21:
        raise Invalid(f"{name} report does not retain 21 samples")
    if report.get("warmups") != 3 or report.get("count") != 21:
        raise Invalid(f"{name} report does not use the 3+21 discipline")
    if not all(
        not isinstance(value, bool)
        and isinstance(value, (int, float))
        and value > 0
        for value in samples
    ):
        raise Invalid(f"{name} report contains a non-positive sample")
    environment = report.get("environment")
    if not isinstance(environment, dict):
        raise Invalid(f"{name} report has no environment")
    if environment.get("evidence_status") != expected_status:
        raise Invalid(f"{name} evidence status is not {expected_status}")
    source = environment.get("source")
    if not isinstance(source, dict) or source.get("dirty") is not False:
        raise Invalid(f"{name} source tree was not clean")
    observer = environment.get("observer")
    if not isinstance(observer, dict):
        raise Invalid(f"{name} report has no observer identity")
    if observer.get("backend") != "QEMU simple trace serial_write":
        raise Invalid(f"{name} did not use the QEMU simple observer")
    build_manifest = observer.get("build_manifest")
    if not isinstance(build_manifest, dict) or not build_manifest.get("sha256"):
        raise Invalid(f"{name} observer has no build manifest")
    isolation = environment.get("production_artifact_isolation")
    required_isolation = {"production_nucleus", "production_runtime_image"}
    if not isinstance(isolation, dict) or set(isolation) != required_isolation:
        raise Invalid(f"{name} has no production-artifact isolation record")
    if any(
        not isinstance(record, dict) or record.get("unchanged") is not True
        for record in isolation.values()
    ):
        raise Invalid(f"{name} changed a production artifact")
    numeric = [float(value) for value in samples]
    expected_statistics = {
        "median_us": statistics.median(numeric),
        "p99_us": max(numeric),
        "min_us": min(numeric),
        "max_us": max(numeric),
    }
    for field, expected in expected_statistics.items():
        if report.get(field) != expected:
            raise Invalid(f"{name} {field} does not match its raw samples")
    return numeric


def qualify(
    floor: dict[str, object], call: dict[str, object], expected_status: str
) -> dict[str, object]:
    """Return the qualification summary or reject the complete pair."""
    floor_samples = _samples(floor, "floor", expected_status)
    call_samples = _samples(call, "call", expected_status)
    floor_environment = floor["environment"]
    call_environment = call["environment"]
    for field, label in (
        ("source", "source identity"),
        ("observer", "observer identity"),
        ("guest_profile", "guest profile"),
        ("host", "host identity"),
        ("scheduler", "scheduler profile"),
    ):
        if floor_environment.get(field) != call_environment.get(field):
            raise Invalid(f"floor and call {label} differ")
    clock = floor.get("clock")
    if clock != call.get("clock") or not isinstance(clock, str):
        raise Invalid("floor and call clock identities differ")
    if "QEMU trace timestamp (simple backend, CLOCK_MONOTONIC" not in clock:
        raise Invalid("reports do not identify the QEMU simple monotonic clock")
    floor_max = max(floor_samples)
    call_min = min(call_samples)
    if floor_max >= call_min:
        raise Invalid(
            f"floor/call ranges overlap: floor max {floor_max:.3f} us, "
            f"call min {call_min:.3f} us"
        )
    if floor["p99_us"] > 40.0:
        raise Invalid(f"floor p99 {floor['p99_us']:.3f} us exceeds 40 us")

    observer = floor_environment["observer"]
    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "adr": "ADR-0066",
        "evidence_status": expected_status,
        "verdict": "observer-qualified",
        "source_commit": floor_environment["source"]["commit"],
        "observer": {
            "backend": observer["backend"],
            "qemu_sha256": observer["qemu_sha256"],
            "build_manifest_sha256": observer["build_manifest"]["sha256"],
        },
        "floor": {
            "median_us": floor["median_us"],
            "p99_us": floor["p99_us"],
            "min_us": floor["min_us"],
            "max_us": floor["max_us"],
        },
        "denominator": {
            "median_us": call["median_us"],
            "p99_us": call["p99_us"],
            "min_us": call["min_us"],
            "max_us": call["max_us"],
        },
        "range_gap_us": round(call_min - floor_max, 3),
        "floor_over_denominator": {
            "median": round(floor["median_us"] / call["median_us"], 6),
            "p99": round(floor["p99_us"] / call["p99_us"], 6),
        },
        "subtracted": "nothing",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--floor", required=True, type=Path)
    parser.add_argument("--call", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--evidence-status", required=True, choices=("P1", "P2"))
    args = parser.parse_args()
    try:
        floor = json.loads(args.floor.read_text(encoding="utf-8"))
        call = json.loads(args.call.read_text(encoding="utf-8"))
        summary = qualify(floor, call, args.evidence_status)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, Invalid) as error:
        print(f"qualify-observer: FAIL: {error}", file=sys.stderr)
        return 1
    summary["reports"] = {
        "floor": str(args.floor.resolve()),
        "denominator": str(args.call.resolve()),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(
        "QUALIFY-OBSERVER PASS: "
        f"evidence={args.evidence_status} "
        f"floor_max={summary['floor']['max_us']:.3f} us "
        f"call_min={summary['denominator']['min_us']:.3f} us "
        f"gap={summary['range_gap_us']:.3f} us"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
