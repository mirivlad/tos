#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Record reproducible Stage 1.5 prototype samples without benchmark magic."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import re
import subprocess
import sys
import time
from typing import Any


RESULT_RE = re.compile(r"(?:^|\s)digest=([0-9A-Za-z._-]+)(?:\s|$)")
OVERLAP_RE = re.compile(r"(?:^|\s)overlap=(true|false)(?:\s|$)")


def load_cases(path: pathlib.Path) -> list[dict[str, Any]]:
    """Load and validate the common corpus before a prototype may use it."""
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("format") != "tos-stage15-common-cases-v1":
        raise ValueError("unsupported common-case format")
    cases = document.get("cases")
    if not isinstance(cases, list) or len(cases) < 10:
        raise ValueError("common corpus must contain at least ten cases")
    identifiers: set[str] = set()
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("case must be an object")
        identifier = case.get("id")
        if not isinstance(identifier, str) or not identifier or identifier in identifiers:
            raise ValueError("case identifiers must be unique non-empty strings")
        if not isinstance(case.get("category"), str) or not isinstance(case.get("source"), str):
            raise ValueError(f"case {identifier} lacks category or source")
        if not isinstance(case.get("expected"), dict) or "status" not in case["expected"]:
            raise ValueError(f"case {identifier} lacks expected status")
        identifiers.add(identifier)
    return cases


def _run(command: list[str]) -> tuple[int, int, str]:
    started = time.monotonic_ns()
    completed = subprocess.run(command, capture_output=True, text=True, check=False, timeout=120)
    elapsed = time.monotonic_ns() - started
    output = completed.stdout + completed.stderr
    if completed.returncode != 0:
        raise RuntimeError(f"measurement command failed ({completed.returncode}): {output}").with_traceback(None)
    return elapsed, completed.returncode, output


def _evidence_from(output: str) -> tuple[str, bool]:
    digest = RESULT_RE.search(output)
    overlap = OVERLAP_RE.search(output)
    if digest is None or overlap is None:
        raise ValueError("measurement output must include digest=<id> and overlap=<true|false>")
    return digest.group(1), overlap.group(1) == "true"


def measure(
    *,
    command: list[str],
    warmups: int,
    samples: int,
    label: str,
    workers: int,
    output: pathlib.Path,
) -> dict[str, Any]:
    """Run exact commands and write raw elapsed-nanosecond samples as JSON."""
    if not command or warmups < 0 or samples <= 0 or workers <= 0:
        raise ValueError("command, positive samples/workers and non-negative warmups are required")
    for _ in range(warmups):
        _run(command)
    elapsed_samples: list[int] = []
    digest: str | None = None
    overlap: bool | None = None
    for _ in range(samples):
        elapsed, _, text = _run(command)
        found_digest, found_overlap = _evidence_from(text)
        if digest is not None and found_digest != digest:
            raise ValueError("measurement command changed its logical result")
        if overlap is not None and found_overlap != overlap:
            raise ValueError("measurement command changed its overlap evidence")
        digest, overlap = found_digest, found_overlap
        elapsed_samples.append(elapsed)
    record: dict[str, Any] = {
        "record_spdx_license": "GPL-3.0-or-later",
        "format": "tos-stage15-measurement-v1",
        "timestamp_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "label": label,
        "command": command,
        "workers": workers,
        "warmups": warmups,
        "samples": samples,
        "samples_ns": elapsed_samples,
        "result_digest": digest,
        "overlap_observed": overlap,
        "host": {"platform": sys.platform, "cpu_count": os.cpu_count()},
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=pathlib.Path, default=pathlib.Path(__file__).with_name("cases.json"))
    parser.add_argument("--validate-only", action="store_true")
    parser.add_argument("--label")
    parser.add_argument("--workers", type=int)
    parser.add_argument("--warmups", type=int, default=3)
    parser.add_argument("--samples", type=int, default=21)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    cases = load_cases(args.cases)
    if args.validate_only:
        print(f"common corpus valid: {len(cases)} cases")
        return 0
    if args.label is None or args.workers is None or args.output is None or not args.command:
        parser.error("--label, --workers, --output and a command after -- are required")
    command = args.command[1:] if args.command[0] == "--" else args.command
    record = measure(
        command=command,
        warmups=args.warmups,
        samples=args.samples,
        label=args.label,
        workers=args.workers,
        output=args.output,
    )
    print(f"measurement recorded: {record['label']} samples={len(record['samples_ns'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
