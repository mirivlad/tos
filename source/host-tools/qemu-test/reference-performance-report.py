#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reduce timestamped boot events to the docs/35 Stage 2 reference figures.

The boundaries are the load-bearing part of this file, so they are stated here
rather than left implicit in a shell pipeline.

`TOS.RUN.STAGE` is emitted **before** each stage runs, and every result event —
`TOS.RUN.VERIFIED`, `TOS.RUN.ACCOUNTING`, `TOS.RUN.COMPLETED` — is emitted
**after** the run returns. A span between two result events therefore measures
the cost of formatting lines and nothing else. The first version of this harness
did exactly that and reported a million engine operations in 241 microseconds, a
figure that looked plausible and was meaningless. `check_boundaries` refuses a
reduction that would repeat the mistake.
"""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path

#: Stages the reference path announces, in order. The last announces `execute`.
STAGES = 7

#: docs/35 budgets for the bootstrap profile on the reference platform.
FRONTEND_BUDGET_US = 500_000
REJECTION_RATIO_BUDGET = 2.0


def events(out: Path, workload: str, sample: int) -> list[dict]:
    path = out / f"{workload}-{sample}.json"
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def first(marks: list[dict], name: str) -> int:
    return next(mark["monotonic_ns"] for mark in marks if mark["event"] == name)


def execute_starts(marks: list[dict]) -> int:
    """When the engine was entered: the last stage announcement."""
    stages = [mark["monotonic_ns"] for mark in marks if mark["event"] == "TOS.RUN.STAGE"]
    if len(stages) != STAGES:
        raise SystemExit(f"expected {STAGES} stage marks, saw {len(stages)}")
    return stages[-1]


def check_boundaries(marks: list[dict]) -> None:
    """Refuse a boundary that cannot contain the work it claims to measure.

    The result events arrive together because they are printed together. If the
    span between two of them were used as a measurement, this is what would
    catch it — so the regression that produced `241 us` is a check rather than a
    memory.
    """
    verified = first(marks, "TOS.RUN.VERIFIED")
    completed = first(marks, "TOS.RUN.COMPLETED")
    if completed - verified > 100_000_000:
        raise SystemExit("result events are not adjacent; the event model changed")
    if execute_starts(marks) >= verified:
        raise SystemExit("the execute announcement did not precede the result")


def percentile(values: list[int], which: int) -> int:
    ordered = sorted(values)
    rank = max(0, min(len(ordered) - 1, round(which / 100 * len(ordered) + 0.5) - 1))
    return ordered[rank]


def summarize(label: str, workload: str, values: list[int]) -> dict:
    return {
        "workload": workload,
        "label": label,
        "samples_us": sorted(values),
        "median_us": int(statistics.median(values)),
        "p95_us": percentile(values, 95),
        "p99_us": percentile(values, 99),
        "min_us": min(values),
        "max_us": max(values),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--samples", required=True, type=int)
    arguments = parser.parse_args()
    out, samples = arguments.out, arguments.samples

    spans: dict[str, list[int]] = {"frontend": [], "execute": [], "reject": []}
    for sample in range(1, samples + 1):
        frontend = events(out, "frontend", sample)
        check_boundaries(frontend)
        spans["frontend"].append(
            (execute_starts(frontend) - first(frontend, "TOS.RUN.BEGIN")) // 1000
        )

        execute = events(out, "execute", sample)
        check_boundaries(execute)
        spans["execute"].append(
            (first(execute, "TOS.RUN.VERIFIED") - execute_starts(execute)) // 1000
        )

        reject = events(out, "reject", sample)
        spans["reject"].append(
            (first(reject, "TOS.RUN.REFUSED") - first(reject, "TOS.RUN.BEGIN")) // 1000
        )

    report = [
        summarize(
            "parse + check + lower + independent verify, 256 KiB module",
            "frontend",
            spans["frontend"],
        ),
        summarize(
            "one-million-operation integer/control-flow benchmark",
            "execute",
            spans["execute"],
        ),
        summarize("reject a quota-exceeding module", "reject", spans["reject"]),
    ]
    (out / "reference.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print("profile: reference (ADR-0040 q35/qemu64/1 vCPU/256 MiB/TCG, real Stage 2 path)")
    print(f"sampling: {samples} boots per workload, host-monotonic serial event timestamps")
    for entry in report:
        print(entry["label"])
        print(
            f"  median {entry['median_us']} us, p95 {entry['p95_us']} us, "
            f"p99 {entry['p99_us']} us, min {entry['min_us']} us, max {entry['max_us']} us"
        )
        print(f"  raw samples (us): {' '.join(str(value) for value in entry['samples_us'])}")

    frontend = report[0]
    print(
        f"  docs/35 budget: {FRONTEND_BUDGET_US} us p95 — "
        f"{'PASS' if frontend['p95_us'] <= FRONTEND_BUDGET_US else 'FAIL'}"
    )
    ratio = report[2]["p95_us"] / max(1, frontend["p95_us"])
    print(
        f"rejection/acceptance p95 ratio: {ratio:.3f} "
        f"(docs/35 budget: at most {REJECTION_RATIO_BUDGET:.3f}) — "
        f"{'PASS' if ratio <= REJECTION_RATIO_BUDGET else 'FAIL'}"
    )
    print(
        "execution ratio: divide the reference p95 above by the native p95 of "
        "the same fixture at this commit (docs/35 budget: at most 10x)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
