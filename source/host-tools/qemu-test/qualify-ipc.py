#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail closed on the ADR-0066 Stage 3 IPC latency budgets."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import statistics
import sys
from pathlib import Path


class Invalid(Exception):
    """The retained records do not prove both IPC latency bounds."""


def samples(report: dict[str, object], name: str) -> list[float]:
    values = report.get("samples_us")
    if not isinstance(values, list) or len(values) != 21:
        raise Invalid(f"{name} does not retain 21 samples")
    if not all(
        not isinstance(value, bool)
        and isinstance(value, (int, float))
        and value > 0
        for value in values
    ):
        raise Invalid(f"{name} contains a non-positive sample")
    numeric = [float(value) for value in values]
    expected = {
        "median_us": statistics.median(numeric),
        "p99_us": max(numeric),
        "min_us": min(numeric),
        "max_us": max(numeric),
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise Invalid(f"{name} {field} does not match its raw samples")
    return numeric


def workload(serial: bytes) -> dict[str, int]:
    printable = bytes(byte if 0x20 <= byte <= 0x7E or byte == 0x0A else 0x20 for byte in serial)
    text = printable.decode("ascii")

    def one(pattern: str, name: str) -> tuple[int, ...]:
        matches = re.findall(pattern, text)
        if len(matches) != 1:
            raise Invalid(f"serial log has {len(matches)} {name} records")
        match = matches[0]
        if isinstance(match, str):
            match = (match,)
        return tuple(int(value) for value in match)

    client = one(
        r"TOS\.RUN\.MEASURE\.IPC samples=(\d+) answered=(\d+) refused=(\d+) "
        r"request_bytes=(\d+) reply_bytes=(\d+) primed=(\d+)",
        "measured-client",
    )
    if client != (24, 24, 0, 64, 64, 1):
        raise Invalid(f"measured client record is {client!r}")
    server = one(
        r"TOS\.RUN\.MEASURE\.IPC\.SERVER served=(\d+) refused=(\d+) "
        r"payload_bytes=(\d+) last=(-?\d+)",
        "measured-server",
    )
    if server != (25, 0, 64, -5):
        raise Invalid(f"measured server record is {server!r}")
    cost_match = re.findall(r"TOS\.RUN\.IPC\.COST ([^\n]+)", text)
    if len(cost_match) != 1:
        raise Invalid(f"serial log has {len(cost_match)} IPC cost records")
    fields = {
        key: int(value)
        for key, value in re.findall(r"([a-z_]+)=(\d+)", cost_match[0])
    }
    expected_cost = {"messages": 50, "exchanges": 25, "ipc_in": 51, "ipc_out": 51}
    for field, expected in expected_cost.items():
        if fields.get(field) != expected:
            raise Invalid(f"IPC cost {field} is {fields.get(field)}, expected {expected}")
    copies = fields.get("payload_copies")
    if copies is None or copies > 100:
        raise Invalid(f"IPC payload copies are {copies}, expected at most 100")
    return {
        "measured_exchanges": 24,
        "priming_exchanges": 1,
        "request_bytes": 64,
        "reply_bytes": 64,
        "messages": fields["messages"],
        "payload_copies": copies,
        "ipc_in": fields["ipc_in"],
        "ipc_out": fields["ipc_out"],
    }


def measurement_build(
    environment: dict[str, object],
    expected_features: dict[str, list[str]],
    name: str,
) -> dict[str, object]:
    measured_build = environment.get("measurement_build")
    contents = measured_build.get("contents") if isinstance(measured_build, dict) else None
    builds = contents.get("builds") if isinstance(contents, dict) else None
    artifacts = environment.get("artifacts")
    if (
        not isinstance(measured_build, dict)
        or not isinstance(measured_build.get("sha256"), str)
        or not isinstance(builds, dict)
        or not isinstance(artifacts, dict)
    ):
        raise Invalid(f"{name} measurement build identity is missing")
    for build_name, features in expected_features.items():
        record = builds.get(build_name)
        artifact = artifacts.get(f"measurement_{build_name}")
        if not isinstance(record, dict) or record.get("features") != features:
            raise Invalid(f"{name} {build_name} features are not qualified")
        if not isinstance(artifact, dict) or record.get("artifact_sha256") != artifact.get(
            "sha256"
        ):
            raise Invalid(
                f"{name} {build_name} artifact does not match its build manifest"
            )
    isolation = environment.get("production_artifact_isolation")
    required_isolation = {"production_nucleus", "production_runtime_image"}
    if not isinstance(isolation, dict) or set(isolation) != required_isolation:
        raise Invalid(f"{name} production-artifact isolation record is missing")
    if any(
        not isinstance(record, dict) or record.get("unchanged") is not True
        for record in isolation.values()
    ):
        raise Invalid(f"{name} measurement changed a production artifact")
    return measured_build


def qualify(
    denominator: dict[str, object],
    denominator_sha256: str,
    observer_qualification: dict[str, object],
    numerator: dict[str, object],
    serial: bytes,
    expected_status: str,
) -> dict[str, object]:
    if numerator.get("record_spdx_license") != "CC-BY-SA-4.0":
        raise Invalid("numerator has no retained-record licence")
    if numerator.get("measurement_mode") != "ipc-request-reply-v1":
        raise Invalid("numerator is not the request/reply workload")
    if numerator.get("warmups") != 3 or numerator.get("count") != 21:
        raise Invalid("numerator does not use the 3+21 discipline")
    if numerator.get("subtracted") != "nothing":
        raise Invalid("numerator subtracts observer cost")
    numerator_samples = samples(numerator, "numerator")

    if denominator.get("record_spdx_license") != "CC-BY-SA-4.0":
        raise Invalid("denominator has no retained-record licence")
    if denominator.get("measurement_mode") != "adjacent-floor-call-pairs-v1":
        raise Invalid("denominator is not the qualified adjacent-pair calibration")
    if denominator.get("warmups") != 3 or denominator.get("count") != 21:
        raise Invalid("denominator does not use the 3+21 block discipline")
    if denominator.get("subtracted") != "nothing":
        raise Invalid("denominator subtracts observer cost")
    denominator_samples = samples(denominator, "denominator")

    if observer_qualification.get("record_spdx_license") != "CC-BY-SA-4.0":
        raise Invalid("observer qualification has no retained-record licence")
    if observer_qualification.get("verdict") != "observer-qualified":
        raise Invalid("denominator observer is not qualified")
    if observer_qualification.get("evidence_status") != expected_status:
        raise Invalid("observer qualification evidence status differs")
    if observer_qualification.get("subtracted") != "nothing":
        raise Invalid("observer qualification subtracts observer cost")
    if observer_qualification.get("measurement_report_sha256") != denominator_sha256:
        raise Invalid("observer qualification does not bind the exact denominator report")
    denominator_stats = observer_qualification.get("denominator")
    expected_denominator_stats = {
        "median_us": statistics.median(denominator_samples),
        "p99_us": max(denominator_samples),
        "min_us": min(denominator_samples),
        "max_us": max(denominator_samples),
    }
    if denominator_stats != expected_denominator_stats:
        raise Invalid("observer qualification does not bind denominator statistics")

    denominator_environment = denominator.get("environment")
    numerator_environment = numerator.get("environment")
    if not isinstance(denominator_environment, dict) or not isinstance(
        numerator_environment, dict
    ):
        raise Invalid("a measurement environment is missing")
    for environment, name in (
        (denominator_environment, "denominator"),
        (numerator_environment, "numerator"),
    ):
        if environment.get("evidence_status") != expected_status:
            raise Invalid(f"{name} evidence status differs")
        source = environment.get("source")
        if not isinstance(source, dict) or source.get("dirty") is not False:
            raise Invalid(f"{name} source tree was not clean")
    for field in ("source", "observer", "guest_profile", "host"):
        if denominator_environment.get(field) != numerator_environment.get(field):
            raise Invalid(f"denominator and numerator {field} identities differ")
    if denominator.get("clock") != numerator.get("clock"):
        raise Invalid("denominator and numerator clock identities differ")
    source = numerator_environment["source"]
    if observer_qualification.get("source_commit") != source.get("commit"):
        raise Invalid("observer qualification source identity differs")

    denominator_scheduler = denominator_environment.get("scheduler")
    if denominator_scheduler != {
        "preemption": "inactive",
        "binding": "measurement-build-manifest",
        "quantum_count": 100000,
    }:
        raise Invalid("denominator does not bind the no-preemption profile")
    denominator_build = measurement_build(
        denominator_environment,
        {
            "nucleus": ["test-measurement-no-preemption"],
            "runtime_image": ["test-measurement-call"],
        },
        "denominator",
    )
    if (
        observer_qualification.get("measurement_build_manifest_sha256")
        != denominator_build["sha256"]
    ):
        raise Invalid("observer qualification does not bind the denominator build")
    observer = denominator_environment.get("observer")
    observer_summary = observer_qualification.get("observer")
    if not isinstance(observer, dict) or observer_summary != {
        "backend": observer.get("backend"),
        "qemu_sha256": observer.get("qemu_sha256"),
        "build_manifest_sha256": (
            observer.get("build_manifest", {}).get("sha256")
            if isinstance(observer.get("build_manifest"), dict)
            else None
        ),
    }:
        raise Invalid("observer qualification does not bind the observer identity")

    scheduler = numerator_environment.get("scheduler")
    if not isinstance(scheduler, dict) or scheduler.get("preemption") != "active":
        raise Invalid("IPC numerator does not bind active preemption")
    if scheduler.get("binding") != "measurement-build-manifest":
        raise Invalid("IPC scheduler state is not manifest-bound")
    measurement_build(
        numerator_environment,
        {
            "nucleus": ["test-call-reply", "test-measurement-port"],
            "runtime_image": ["test-measurement-ipc"],
        },
        "IPC",
    )

    workload_record = workload(serial)
    numerator_p99 = max(numerator_samples)
    denominator_p99 = max(denominator_samples)
    relative_limit = 8.0 * denominator_p99
    relative_pass = numerator_p99 <= relative_limit
    absolute_pass = numerator_p99 <= 200.0

    observer = numerator_environment["observer"]
    return {
        "record_spdx_license": "CC-BY-SA-4.0",
        "adr": "ADR-0066",
        "evidence_status": expected_status,
        "verdict": (
            "ipc-latency-qualified"
            if relative_pass and absolute_pass
            else "ipc-latency-red"
        ),
        "source_commit": source["commit"],
        "observer": {
            "backend": observer["backend"],
            "qemu_sha256": observer["qemu_sha256"],
            "build_manifest_sha256": observer["build_manifest"]["sha256"],
        },
        "denominator": {
            "median_us": statistics.median(denominator_samples),
            "p99_us": denominator_p99,
        },
        "numerator": {
            "median_us": statistics.median(numerator_samples),
            "p99_us": numerator_p99,
            "min_us": min(numerator_samples),
            "max_us": max(numerator_samples),
        },
        "budgets": {
            "relative_ratio": numerator_p99 / denominator_p99,
            "relative_limit": 8.0,
            "relative_limit_us": relative_limit,
            "relative_pass": relative_pass,
            "absolute_us": numerator_p99,
            "absolute_limit_us": 200.0,
            "absolute_pass": absolute_pass,
        },
        "scheduler": scheduler,
        "workload": workload_record,
        "subtracted": "nothing",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--denominator", required=True, type=Path)
    parser.add_argument("--observer-qualification", required=True, type=Path)
    parser.add_argument("--numerator", required=True, type=Path)
    parser.add_argument("--serial-log", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--evidence-status", required=True, choices=("P1", "P2"))
    args = parser.parse_args()
    reports = {
        "denominator": str(args.denominator.resolve()),
        "observer_qualification": str(args.observer_qualification.resolve()),
        "numerator": str(args.numerator.resolve()),
        "serial_log": str(args.serial_log.resolve()),
    }
    try:
        denominator_bytes = args.denominator.read_bytes()
        result = qualify(
            json.loads(denominator_bytes),
            hashlib.sha256(denominator_bytes).hexdigest(),
            json.loads(args.observer_qualification.read_bytes()),
            json.loads(args.numerator.read_bytes()),
            args.serial_log.read_bytes(),
            args.evidence_status,
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, Invalid) as error:
        result = {
            "record_spdx_license": "CC-BY-SA-4.0",
            "adr": "ADR-0066",
            "evidence_status": args.evidence_status,
            "verdict": "ipc-evidence-invalid",
            "failure": str(error),
            "reports": reports,
        }
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(f"qualify-ipc: FAIL: {error}", file=sys.stderr)
        return 1
    result["reports"] = reports
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    if result["verdict"] == "ipc-latency-red":
        print(
            "QUALIFY-IPC RED: "
            f"evidence={args.evidence_status} "
            f"p99={result['numerator']['p99_us']:.3f} us "
            f"ratio={result['budgets']['relative_ratio']:.3f}x "
            f"relative_pass={result['budgets']['relative_pass']} "
            f"absolute_pass={result['budgets']['absolute_pass']}",
            file=sys.stderr,
        )
        return 1
    print(
        "QUALIFY-IPC PASS: "
        f"evidence={args.evidence_status} "
        f"p99={result['numerator']['p99_us']:.3f} us "
        f"ratio={result['budgets']['relative_ratio']:.3f}x"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
