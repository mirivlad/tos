#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic fixture and report helpers for the Stage 1 capsule budget."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import platform
import sys
from pathlib import Path


FILE_COUNT = 1_000
PAYLOAD_BYTES = 16 * 1024 * 1024
SPDX = b"# SPDX-License-Identifier: GPL-3.0-or-later\n"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def fixture(output: Path) -> None:
    inputs = output / "inputs"
    boot = inputs / "boot" / "init.tos"
    boot.parent.mkdir(parents=True, exist_ok=True)
    boot_content = SPDX + b"# Stage 1 performance fixture canonical boot text\n"
    boot.write_bytes(boot_content)

    remaining = PAYLOAD_BYTES - len(boot_content)
    per_file, extra = divmod(remaining, FILE_COUNT - 1)
    if per_file < len(SPDX):
        raise RuntimeError("Stage 1 fixture cannot retain SPDX in every generated file")

    rows = [("/system/boot/init.tos", boot.relative_to(output).as_posix())]
    for index in range(FILE_COUNT - 1):
        source = inputs / "lib" / f"file{index:04}.tos"
        source.parent.mkdir(parents=True, exist_ok=True)
        length = per_file + (1 if index < extra else 0)
        source.write_bytes(SPDX + b"x" * (length - len(SPDX)))
        rows.append((f"/system/lib/file{index:04}.tos", source.relative_to(output).as_posix()))

    if rows != sorted(rows):
        raise RuntimeError("fixture paths are not canonical")
    manifest = output / "manifest.tsv"
    manifest.write_text("".join(f"{path}\t{source}\n" for path, source in rows), encoding="utf-8")
    payload = sum((output / source).stat().st_size for _, source in rows)
    if payload != PAYLOAD_BYTES:
        raise RuntimeError(f"fixture payload is {payload}, expected {PAYLOAD_BYTES}")
    write_json(
        output / "workload.json",
        {
            "file_count": FILE_COUNT,
            "fixture": "stage1-capsule-1000-files-16-mib-v1",
            "manifest_sha256": sha256_file(manifest),
            "payload_bytes": payload,
            "source_identity_mode": "detached-source-set",
        },
    )


def timestamp_records(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def append_sample(timestamps: Path, phase: str, index: int, output: Path) -> None:
    records = timestamp_records(timestamps)
    events = [record.get("event") for record in records]
    required = ("TOS.BOOT.ENTRY", "TOS.BOOTTEXT.PATH")
    if events.count(required[0]) != 1 or events.count(required[1]) != 1:
        raise ValueError(f"measurement boundaries must occur exactly once, got {events!r}")
    start_record = records[events.index(required[0])]
    end_record = records[events.index(required[1])]
    if events.index(required[1]) <= events.index(required[0]):
        raise ValueError(f"measurement boundaries are out of order: {events!r}")
    start, end = (start_record.get("monotonic_ns"), end_record.get("monotonic_ns"))
    if not isinstance(start, int) or not isinstance(end, int) or start <= 0 or end <= start:
        raise ValueError("measurement timestamps must be strictly increasing positive nanoseconds")
    record = {
        "duration_ns": end - start,
        "end_monotonic_ns": end,
        "event_timestamps": records,
        "index": index,
        "phase": phase,
        "start_monotonic_ns": start,
    }
    with output.open("a", encoding="utf-8") as destination:
        destination.write(json.dumps(record, sort_keys=True, separators=(",", ":")))
        destination.write("\n")


def cpu_description() -> str:
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        for line in cpuinfo.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("model name") and ":" in line:
                return line.split(":", 1)[1].strip()
    return platform.processor() or "unknown"


def report(args: argparse.Namespace) -> bool:
    measured = timestamp_records(args.measurements)
    warmups = timestamp_records(args.warmups)
    if len(measured) != 21:
        raise ValueError(f"expected 21 measured samples, got {len(measured)}")
    if len(warmups) != 3:
        raise ValueError(f"expected 3 warm-up samples, got {len(warmups)}")
    if any(record.get("phase") != "measurement" for record in measured):
        raise ValueError("measurement JSONL contains a non-measurement sample")
    if any(record.get("phase") != "warmup" for record in warmups):
        raise ValueError("warm-up JSONL contains a non-warmup sample")
    durations = sorted(record["duration_ns"] for record in measured)
    if any(not isinstance(value, int) or value <= 0 for value in durations):
        raise ValueError("measurement duration is not a positive integer")
    median = durations[len(durations) // 2]
    p95_rank = math.ceil(len(durations) * 0.95)
    p99_rank = math.ceil(len(durations) * 0.99)
    p95 = durations[p95_rank - 1]
    p99 = durations[p99_rank - 1]
    workload = json.loads(args.fixture.joinpath("workload.json").read_text(encoding="utf-8"))
    report_value = {
        "architecture": "x86_64 Stage 1",
        "baseline": "none; initial Stage 1 P2 baseline",
        "evidence_status": args.evidence_status,
        "firmware": {
            "code_path": str(args.ovmf_code),
            "code_sha256": sha256_file(args.ovmf_code),
            "vars_path": str(args.ovmf_vars),
            "vars_sha256": sha256_file(args.ovmf_vars),
        },
        "guest": {"cpu": "qemu64", "memory_mib": 256, "vcpus": 1},
        "host": {
            "cpu": cpu_description(),
            "virtualization_mode": (
                "TCG (QEMU invoked without -enable-kvm)"
                if args.accelerator == "tcg"
                else "KVM research-only alternate backend"
            ),
        },
        "measurement": {
            "end_event": "TOS.BOOTTEXT.PATH",
            "event_clock": "host monotonic serial-byte arrival",
            "start_event": "TOS.BOOT.ENTRY",
        },
        "qemu": {
            "accelerator": args.accelerator,
            "machine": "q35",
            "version": args.qemu_version,
        },
        "raw_samples": {"measurements": measured, "warmups": warmups},
        "rustc_version": args.rustc_version,
        "source_commit": args.source_commit,
        "statistics": {
            "budget_pass": p95 <= 250_000_000,
            "median_ns": median,
            "p95_budget_ns": 250_000_000,
            "p95_ns": p95,
            "p95_rank": p95_rank,
            "p99_ns": p99,
            "p99_rank": p99_rank,
        },
        "workload": workload,
    }
    write_json(args.out, report_value)
    return p95 <= 250_000_000


def native_report(args: argparse.Namespace) -> None:
    records = timestamp_records(args.samples)
    measured = [record for record in records if record.get("phase") == "measurement"]
    warmups = [record for record in records if record.get("phase") == "warmup"]
    if len(records) != len(measured) + len(warmups):
        raise ValueError("native sample JSONL contains an unknown phase")
    if len(measured) != 21 or len(warmups) != 3:
        raise ValueError(
            f"native report requires 3 warm-ups and 21 measurements, got {len(warmups)}/{len(measured)}"
        )
    if any(record.get("phase") != "measurement" for record in measured):
        raise ValueError("native measurement JSONL contains a non-measurement sample")
    if any(record.get("phase") != "warmup" for record in warmups):
        raise ValueError("native warm-up JSONL contains a non-warmup sample")
    all_records = warmups + measured
    if any(record.get("validations") != 2 for record in all_records):
        raise ValueError("native sample does not attest two fresh validations")
    if any(record.get("lookup") != "/system/boot/init.tos" for record in all_records):
        raise ValueError("native sample does not attest canonical boot lookup")
    durations = sorted(record.get("duration_ns") for record in measured)
    if any(not isinstance(value, int) or value <= 0 for value in durations):
        raise ValueError("native measurement duration is not a positive integer")
    p95_rank = math.ceil(len(durations) * 0.95)
    p99_rank = math.ceil(len(durations) * 0.99)
    p95 = durations[p95_rank - 1]
    workload = json.loads(args.fixture.joinpath("workload.json").read_text(encoding="utf-8"))
    write_json(
        args.out,
        {
            "architecture": "x86_64 Stage 1 native logical validation research",
            "evidence_status": "P1 research-only",
            "host": {"cpu": cpu_description(), "os": platform.platform()},
            "measurement": {
                "logical_sequence": "fresh parse -> fresh parse -> canonical boot_file lookup",
                "timer": "host monotonic Instant after capsule bytes are read once",
            },
            "raw_samples": {"measurements": measured, "warmups": warmups},
            "rustc_version": args.rustc_version,
            "source_commit": args.source_commit,
            "statistics": {
                "median_ns": durations[len(durations) // 2],
                "p95_budget_ns": 250_000_000,
                "p95_ns": p95,
                "p95_rank": p95_rank,
                "p99_ns": durations[p99_rank - 1],
                "p99_rank": p99_rank,
                "would_meet_existing_budget": p95 <= 250_000_000,
            },
            "workload": workload,
        },
    )


def comparison(args: argparse.Namespace) -> None:
    native = json.loads(args.native.read_text(encoding="utf-8"))
    qemu = json.loads(args.qemu.read_text(encoding="utf-8"))
    native_p95 = native["statistics"]["p95_ns"]
    qemu_p95 = qemu["statistics"]["p95_ns"]
    if not isinstance(native_p95, int) or not isinstance(qemu_p95, int) or native_p95 <= 0:
        raise ValueError("comparison reports lack positive p95 values")
    write_json(
        args.out,
        {
            "native": {
                "p95_ns": native_p95,
                "source_commit": native["source_commit"],
                "would_meet_existing_budget": native["statistics"]["would_meet_existing_budget"],
            },
            "qemu_tcg": {
                "p95_ns": qemu_p95,
                "source_commit": qemu["source_commit"],
                "would_meet_existing_budget": qemu["statistics"]["budget_pass"],
            },
            "qemu_to_native_p95_ratio": qemu_p95 / native_p95,
            "scope": "research-only comparison; neither result amends ADR-0025",
        },
    )


def nearest_rank_statistics(durations: list[int], *, allow_zero: bool = False) -> dict[str, int]:
    if not durations:
        raise ValueError("statistics require at least one duration")
    if any(not isinstance(value, int) or value < (0 if allow_zero else 1) for value in durations):
        raise ValueError("duration is not an allowed integer")
    ordered = sorted(durations)
    p95_rank = math.ceil(len(ordered) * 0.95)
    p99_rank = math.ceil(len(ordered) * 0.99)
    return {
        "median_ns": ordered[len(ordered) // 2],
        "p95_ns": ordered[p95_rank - 1],
        "p95_rank": p95_rank,
        "p99_ns": ordered[p99_rank - 1],
        "p99_rank": p99_rank,
    }


def decomposition(args: argparse.Namespace) -> None:
    report_value = json.loads(args.report.read_text(encoding="utf-8"))
    measurements = report_value.get("raw_samples", {}).get("measurements")
    if not isinstance(measurements, list) or len(measurements) != 21:
        raise ValueError("decomposition requires exactly 21 measured QEMU samples")
    segment_names = (
        "loader_validation",
        "loader_post_validation",
        "handoff_transition",
        "nucleus_validation",
        "canonical_lookup",
        "post_validation_to_halt",
    )
    segments = {name: [] for name in segment_names}
    expected = (
        "TOS.BOOT.ENTRY",
        "TOS.CAPSULE.OK",
        "TOS.BOOT.HANDOFF",
        "TOS.NUCLEUS.ENTRY",
        "TOS.CAPSULE.OK",
        "TOS.BOOTTEXT.PATH",
        "TOS.HALT",
    )
    for sample in measurements:
        records = sample.get("event_timestamps")
        if not isinstance(records, list):
            raise ValueError("QEMU sample lacks event timestamp records")
        events = [record.get("event") for record in records]
        positions = []
        after = -1
        for event in expected:
            try:
                position = events.index(event, after + 1)
            except ValueError as error:
                raise ValueError(f"QEMU sample lacks ordered event {event}") from error
            positions.append(position)
            after = position
        times = [records[position].get("monotonic_ns") for position in positions]
        if any(not isinstance(value, int) or value <= 0 for value in times):
            raise ValueError("QEMU event timestamp is not a positive integer")
        intervals = [end - start for start, end in zip(times, times[1:])]
        if any(value < 0 for value in intervals):
            raise ValueError("QEMU events are not monotonic")
        for name, interval in zip(segment_names, intervals):
            segments[name].append(interval)
    write_json(
        args.out,
        {
            "event_clock": "host monotonic serial-byte arrival",
            "report_source_commit": report_value.get("source_commit"),
            "sample_count": len(measurements),
            "segments": {
                name: nearest_rank_statistics(values, allow_zero=True)
                for name, values in segments.items()
            },
        },
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    fixture_parser = subcommands.add_parser("fixture")
    fixture_parser.add_argument("--out", required=True, type=Path)
    sample_parser = subcommands.add_parser("sample")
    sample_parser.add_argument("--timestamps", required=True, type=Path)
    sample_parser.add_argument("--phase", choices=("warmup", "measurement"), required=True)
    sample_parser.add_argument("--index", type=int, required=True)
    sample_parser.add_argument("--out", required=True, type=Path)
    report_parser = subcommands.add_parser("report")
    report_parser.add_argument("--fixture", required=True, type=Path)
    report_parser.add_argument("--measurements", required=True, type=Path)
    report_parser.add_argument("--warmups", required=True, type=Path)
    report_parser.add_argument("--out", required=True, type=Path)
    report_parser.add_argument("--source-commit", required=True)
    report_parser.add_argument("--qemu-version", required=True)
    report_parser.add_argument("--rustc-version", required=True)
    report_parser.add_argument("--ovmf-code", required=True, type=Path)
    report_parser.add_argument("--ovmf-vars", required=True, type=Path)
    report_parser.add_argument("--evidence-status", choices=("P1", "P2"), required=True)
    report_parser.add_argument("--accelerator", choices=("tcg", "kvm"), required=True)
    native_report_parser = subcommands.add_parser("native-report")
    native_report_parser.add_argument("--fixture", required=True, type=Path)
    native_report_parser.add_argument("--samples", required=True, type=Path)
    native_report_parser.add_argument("--out", required=True, type=Path)
    native_report_parser.add_argument("--source-commit", required=True)
    native_report_parser.add_argument("--rustc-version", required=True)
    comparison_parser = subcommands.add_parser("comparison")
    comparison_parser.add_argument("--native", required=True, type=Path)
    comparison_parser.add_argument("--qemu", required=True, type=Path)
    comparison_parser.add_argument("--out", required=True, type=Path)
    decomposition_parser = subcommands.add_parser("decomposition")
    decomposition_parser.add_argument("--report", required=True, type=Path)
    decomposition_parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "fixture":
        fixture(args.out)
    elif args.command == "sample":
        append_sample(args.timestamps, args.phase, args.index, args.out)
    elif args.command == "report":
        if not report(args):
            raise SystemExit("p95 exceeds the Stage 1 250 ms budget")
    elif args.command == "native-report":
        native_report(args)
    elif args.command == "comparison":
        comparison(args)
    else:
        decomposition(args)


if __name__ == "__main__":
    main()
