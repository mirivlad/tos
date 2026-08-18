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
# A TOS Core line comment is `//`; `#` cannot begin one, so a fixture whose
# licence header used `#` produced a boot module the lexer refused at its first
# byte. The marker text is what the capsule builder looks for, and it is
# unchanged.
SPDX = b"// SPDX-License-Identifier: GPL-3.0-or-later\n"
# The bytes one boot hashes, as the nucleus's crypto baseline reports them:
# the parser's two passes over the payload, the capsule digest taken twice, and
# the boot module once. It moved by exactly 198 — the number of bytes the boot
# module grew by when it stopped being a comment and became a program the boot
# can actually run — and by nothing else: the fixture's payload is still
# `PAYLOAD_BYTES`, because the filler absorbs whatever the boot text takes.
STAGE1_CRYPTO_BYTES = 101_203_397
STAGE1_CRYPTO_HASHES = 2_007


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
    # A real module, not a placeholder. Stage 1 only had to carry and validate
    # this capsule, so its boot text was a comment; Stage 2 made the nucleus run
    # the boot module and Stage 3 launches it as a process, and neither can run
    # a file that is not source. What this fixture measures is unchanged — the
    # cost of validating a capsule of this many files and this many bytes — and
    # the boot module is now the smallest thing that lets the measurement end
    # the way the gate says it ends.
    boot_content = SPDX + b"""
module system.boot.init version 1.0 profile bootstrap;

resource [fuel: 65536, stack: 2MiB, allocation: 4KiB, tasks: 1, workers: 1,
          sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4]

pub fn main() -> i32 {
    return 240i32;
}
"""
    boot.write_bytes(boot_content)

    remaining = PAYLOAD_BYTES - len(boot_content)
    per_file, extra = divmod(remaining, FILE_COUNT - 1)
    if per_file < len(SPDX):
        raise RuntimeError("Stage 1 fixture cannot retain SPDX in every generated file")

    rows = [("/system/boot/init.tos", boot.relative_to(output).as_posix())]
    for index in range(FILE_COUNT - 1):
        # Payload, not source. These files exist to give the capsule its size
        # and its file count; naming them `.tos` made every one of them a module
        # of the boot set once the nucleus started offering the frontend every
        # `.tos` file the capsule carries, which turned a Stage 1 validation
        # measurement into a Stage 3 parse of a thousand files that are not
        # programs.
        source = inputs / "lib" / f"file{index:04}.dat"
        source.parent.mkdir(parents=True, exist_ok=True)
        length = per_file + (1 if index < extra else 0)
        source.write_bytes(SPDX + b"x" * (length - len(SPDX)))
        rows.append((f"/system/lib/file{index:04}.dat", source.relative_to(output).as_posix()))

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


def report(args: argparse.Namespace) -> None:
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
            "median_ns": median,
            "p95_ns": p95,
            "p95_rank": p95_rank,
            "p99_ns": p99,
            "p99_rank": p99_rank,
        },
        "workload": workload,
    }
    write_json(args.out, report_value)


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
            "architecture": "x86_64 Stage 1 native exact validation",
            "evidence_status": args.evidence_status,
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
                "p95_ns": p95,
                "p95_rank": p95_rank,
                "p99_ns": durations[p99_rank - 1],
                "p99_rank": p99_rank,
            },
            "workload": workload,
        },
    )


def crypto_report(args: argparse.Namespace) -> None:
    records = timestamp_records(args.samples)
    measured = [record for record in records if record.get("phase") == "measurement"]
    warmups = [record for record in records if record.get("phase") == "warmup"]
    if len(records) != len(measured) + len(warmups):
        raise ValueError("crypto sample JSONL contains an unknown phase")
    if len(measured) != 21 or len(warmups) != 3:
        raise ValueError(
            f"crypto report requires 3 warm-ups and 21 measurements, got {len(warmups)}/{len(measured)}"
        )
    all_records = warmups + measured
    if any(record.get("mode") != "unavoidable_crypto" for record in all_records):
        raise ValueError("crypto sample does not identify the unavoidable-crypto mode")
    if any(record.get("validations") != 2 for record in all_records):
        raise ValueError("crypto sample does not attest two fresh logical validators")
    accounting_pairs = {
        (record.get("crypto_bytes_per_boot"), record.get("crypto_hashes_per_boot"))
        for record in all_records
    }
    if len(accounting_pairs) != 1:
        raise ValueError("crypto accounting changed between samples")
    bytes_per_boot, hashes_per_boot = accounting_pairs.pop()
    if not isinstance(bytes_per_boot, int) or bytes_per_boot <= 0:
        raise ValueError("crypto byte accounting is not positive")
    if not isinstance(hashes_per_boot, int) or hashes_per_boot <= 0:
        raise ValueError("crypto hash accounting is not positive")
    stats = nearest_rank_statistics([record.get("duration_ns") for record in measured])
    workload = json.loads(args.fixture.joinpath("workload.json").read_text(encoding="utf-8"))
    write_json(
        args.out,
        {
            "architecture": "x86_64 Stage 1 unavoidable crypto",
            "crypto_accounting": {
                "bytes_per_boot": bytes_per_boot,
                "hashes_per_boot": hashes_per_boot,
            },
            "evidence_status": args.evidence_status,
            "host": {"cpu": cpu_description(), "os": platform.platform()},
            "measurement": {
                "logical_sequence": "two plain capsule mirrors -> two parser crypto replays -> boot-text digest",
                "timer": "host monotonic Instant after structural fixture setup",
            },
            "raw_samples": {"measurements": measured, "warmups": warmups},
            "rustc_version": args.rustc_version,
            "source_commit": args.source_commit,
            "statistics": stats,
            "workload": workload,
        },
    )


def validation_ratio(args: argparse.Namespace) -> None:
    full = json.loads(args.full.read_text(encoding="utf-8"))
    crypto = json.loads(args.crypto.read_text(encoding="utf-8"))
    if full.get("source_commit") != crypto.get("source_commit"):
        raise ValueError("full and crypto reports use different source commits")
    if full.get("workload") != crypto.get("workload"):
        raise ValueError("full and crypto reports use different workloads")
    if full.get("evidence_status") != crypto.get("evidence_status"):
        raise ValueError("full and crypto reports use different evidence statuses")
    fields = ("median_ns", "p95_ns", "p99_ns")
    full_stats = full.get("statistics", {})
    crypto_stats = crypto.get("statistics", {})
    if any(
        not isinstance(full_stats.get(field), int)
        or not isinstance(crypto_stats.get(field), int)
        or crypto_stats[field] <= 0
        for field in fields
    ):
        raise ValueError("full or crypto report lacks positive latency statistics")
    accounting = crypto.get("crypto_accounting")
    expected_accounting = {
        "bytes_per_boot": STAGE1_CRYPTO_BYTES,
        "hashes_per_boot": STAGE1_CRYPTO_HASHES,
    }
    if accounting != expected_accounting:
        raise ValueError("crypto report does not attest the exact Stage 1 workload accounting")
    p95_ratio = full_stats["p95_ns"] / crypto_stats["p95_ns"]
    if p95_ratio > args.max_p95_ratio:
        raise ValueError(
            f"full/crypto p95 ratio {p95_ratio:.9f} exceeds {args.max_p95_ratio:.9f}"
        )
    write_json(
        args.out,
        {
            "crypto_accounting": accounting,
            "evidence_status": full["evidence_status"],
            "full_over_unavoidable_crypto": {
                "median_ratio": full_stats["median_ns"] / crypto_stats["median_ns"],
                "p95_ratio": p95_ratio,
                "p99_ratio": full_stats["p99_ns"] / crypto_stats["p99_ns"],
            },
            "max_p95_ratio": args.max_p95_ratio,
            "scope": "accepted ADR-0026 Stage 1 validation performance conformance",
            "source_commit": full["source_commit"],
            "workload": full["workload"],
        },
    )


def crypto_qemu_sample(args: argparse.Namespace) -> None:
    records = timestamp_records(args.timestamps)
    events = [record.get("event") for record in records]
    start_event = "TOS.TEST.CRYPTO.BASELINE.START"
    end_event = "TOS.TEST.CRYPTO.BASELINE.DONE"
    if events.count(start_event) != 1 or events.count(end_event) != 1:
        raise ValueError(f"crypto QEMU boundaries must occur exactly once, got {events!r}")
    start = records[events.index(start_event)].get("monotonic_ns")
    end = records[events.index(end_event)].get("monotonic_ns")
    if not isinstance(start, int) or not isinstance(end, int) or start <= 0 or end <= start:
        raise ValueError("crypto QEMU timestamps must be strictly increasing positive nanoseconds")
    if args.crypto_bytes <= 0 or args.crypto_hashes <= 0:
        raise ValueError("crypto QEMU accounting must be positive")
    record = {
        "crypto_bytes_per_boot": args.crypto_bytes,
        "crypto_hashes_per_boot": args.crypto_hashes,
        "duration_ns": end - start,
        "event_timestamps": records,
        "index": args.index,
        "mode": "unavoidable_crypto",
        "phase": args.phase,
        "validations": 2,
    }
    with args.out.open("a", encoding="utf-8") as destination:
        destination.write(json.dumps(record, sort_keys=True, separators=(",", ":")))
        destination.write("\n")


def qemu_crypto_report(args: argparse.Namespace) -> None:
    crypto_report(args)
    report_value = json.loads(args.out.read_text(encoding="utf-8"))
    report_value["architecture"] = "x86_64 Stage 1 QEMU unavoidable crypto research"
    report_value["firmware"] = {
        "code_path": str(args.ovmf_code),
        "code_sha256": sha256_file(args.ovmf_code),
        "vars_path": str(args.ovmf_vars),
        "vars_sha256": sha256_file(args.ovmf_vars),
    }
    report_value["guest"] = {"cpu": "qemu64", "memory_mib": 256, "vcpus": 1}
    report_value["host"] = {
        "cpu": cpu_description(),
        "virtualization_mode": (
            "TCG (QEMU invoked without -enable-kvm)"
            if args.accelerator == "tcg"
            else "KVM research-only alternate backend"
        ),
    }
    report_value["measurement"] = {
        "end_event": "TOS.TEST.CRYPTO.BASELINE.DONE",
        "event_clock": "host monotonic serial-byte arrival",
        "logical_sequence": "two plain capsule mirrors -> two parser crypto replays -> boot-text digest",
        "start_event": "TOS.TEST.CRYPTO.BASELINE.START",
    }
    report_value["qemu"] = {
        "accelerator": args.accelerator,
        "machine": "q35",
        "version": args.qemu_version,
    }
    write_json(args.out, report_value)


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
            },
            "qemu_tcg": {
                "p95_ns": qemu_p95,
                "source_commit": qemu["source_commit"],
            },
            "qemu_to_native_p95_ratio": qemu_p95 / native_p95,
            "scope": "observational comparison; ratio conformance is reported separately",
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
    native_report_parser.add_argument("--evidence-status", choices=("P1", "P2"), required=True)
    crypto_report_parser = subcommands.add_parser("crypto-report")
    crypto_report_parser.add_argument("--fixture", required=True, type=Path)
    crypto_report_parser.add_argument("--samples", required=True, type=Path)
    crypto_report_parser.add_argument("--out", required=True, type=Path)
    crypto_report_parser.add_argument("--source-commit", required=True)
    crypto_report_parser.add_argument("--rustc-version", required=True)
    crypto_report_parser.add_argument("--evidence-status", choices=("P1", "P2"), required=True)
    validation_ratio_parser = subcommands.add_parser("validation-ratio")
    validation_ratio_parser.add_argument("--full", required=True, type=Path)
    validation_ratio_parser.add_argument("--crypto", required=True, type=Path)
    validation_ratio_parser.add_argument("--out", required=True, type=Path)
    validation_ratio_parser.add_argument("--max-p95-ratio", required=True, type=float)
    crypto_qemu_sample_parser = subcommands.add_parser("crypto-qemu-sample")
    crypto_qemu_sample_parser.add_argument("--timestamps", required=True, type=Path)
    crypto_qemu_sample_parser.add_argument("--phase", choices=("warmup", "measurement"), required=True)
    crypto_qemu_sample_parser.add_argument("--index", type=int, required=True)
    crypto_qemu_sample_parser.add_argument("--crypto-bytes", type=int, required=True)
    crypto_qemu_sample_parser.add_argument("--crypto-hashes", type=int, required=True)
    crypto_qemu_sample_parser.add_argument("--out", required=True, type=Path)
    qemu_crypto_report_parser = subcommands.add_parser("qemu-crypto-report")
    qemu_crypto_report_parser.add_argument("--fixture", required=True, type=Path)
    qemu_crypto_report_parser.add_argument("--samples", required=True, type=Path)
    qemu_crypto_report_parser.add_argument("--out", required=True, type=Path)
    qemu_crypto_report_parser.add_argument("--source-commit", required=True)
    qemu_crypto_report_parser.add_argument("--rustc-version", required=True)
    qemu_crypto_report_parser.add_argument("--qemu-version", required=True)
    qemu_crypto_report_parser.add_argument("--ovmf-code", required=True, type=Path)
    qemu_crypto_report_parser.add_argument("--ovmf-vars", required=True, type=Path)
    qemu_crypto_report_parser.add_argument("--accelerator", choices=("tcg", "kvm"), required=True)
    qemu_crypto_report_parser.add_argument("--evidence-status", choices=("P1", "P2"), required=True)
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
        report(args)
    elif args.command == "native-report":
        native_report(args)
    elif args.command == "crypto-report":
        crypto_report(args)
    elif args.command == "validation-ratio":
        validation_ratio(args)
    elif args.command == "crypto-qemu-sample":
        crypto_qemu_sample(args)
    elif args.command == "qemu-crypto-report":
        qemu_crypto_report(args)
    elif args.command == "comparison":
        comparison(args)
    else:
        decomposition(args)


if __name__ == "__main__":
    main()
