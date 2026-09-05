#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The same-artifact paired Stage 1 validation-performance report (ADR-0083).

**The refusal is the point of this program.** A ratio between two series that
came from different executables is what the Stage 4C construct-validity
investigation falsified: an inert layout change moved the old cross-artifact
quotient across its conformance boundary while native execution was unmoved.
So this reporter will not compute a ratio at all unless both series report
exactly equal image digests.

It emits no verdict and knows no threshold. Choosing one belongs to ADR-0083 and
to the Project Architect, after the repaired metric has been measured.
"""
import argparse
import json
import statistics
import subprocess
from pathlib import Path


def nearest_rank(values: list[int], percentile: float) -> int:
    """The accepted nearest-rank percentile, unchanged from the old metric."""
    ordered = sorted(values)
    rank = max(1, -(-len(ordered) * percentile // 100))
    return ordered[int(rank) - 1]


def series(path: Path, expected: int) -> dict:
    measured: list[int] = []
    warmups: list[int] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        phase, _index, ns = line.split("\t")
        (measured if phase == "measurement" else warmups).append(int(ns))
    if len(measured) != expected:
        raise SystemExit(
            f"{path}: {len(measured)} measured samples, expected {expected}"
        )
    return {
        "measured_ns": measured,
        "warmup_ns": warmups,
        "median_ns": int(statistics.median(measured)),
        "p95_ns": nearest_rank(measured, 95),
        "p99_ns": nearest_rank(measured, 99),
        "min_ns": min(measured),
        "max_ns": max(measured),
    }


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--label", required=True)
    p.add_argument("--full", required=True, type=Path)
    p.add_argument("--crypto", required=True, type=Path)
    p.add_argument("--full-image-sha256", required=True)
    p.add_argument("--crypto-image-sha256", required=True)
    p.add_argument("--image-bytes", required=True, type=int)
    p.add_argument("--elf-bytes", required=True, type=int)
    p.add_argument("--text-addr", required=True)
    p.add_argument("--text-size", required=True)
    p.add_argument("--compress-block-addr", required=True)
    p.add_argument("--capsule-sha256", required=True)
    p.add_argument("--warmups", required=True, type=int)
    p.add_argument("--samples", required=True, type=int)
    p.add_argument("--repository", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    args = p.parse_args()

    # ---- the same-artifact proof, before any arithmetic ----------------------
    if args.full_image_sha256 != args.crypto_image_sha256:
        raise SystemExit(
            "paired-report: REFUSED: the two series did not come from the same "
            "image.\n"
            f"  full   {args.full_image_sha256}\n"
            f"  crypto {args.crypto_image_sha256}\n"
            "A ratio between separately linked artifacts is the construct this "
            "metric was repaired to remove; it will not be computed."
        )

    full = series(args.full, args.samples)
    crypto = series(args.crypto, args.samples)
    ratio_p95 = full["p95_ns"] / crypto["p95_ns"]
    ratio_median = full["median_ns"] / crypto["median_ns"]

    def commit() -> str:
        try:
            return subprocess.run(
                ["git", "-C", str(args.repository), "rev-parse", "HEAD"],
                capture_output=True, text=True, check=True,
            ).stdout.strip()
        except Exception:
            return "unknown"

    def dirty() -> bool:
        try:
            return bool(subprocess.run(
                ["git", "-C", str(args.repository), "status", "--porcelain"],
                capture_output=True, text=True, check=True,
            ).stdout.strip())
        except Exception:
            return True

    report = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "metric": "same-artifact paired Stage 1 validation performance (ADR-0083)",
        "label": args.label,
        "threshold": None,
        "verdict": None,
        "note": (
            "No threshold is applied. ADR-0083 is Proposed and the replacement "
            "threshold is not chosen; this report is evidence for that choice."
        ),
        "same_artifact": {
            "full_image_sha256": args.full_image_sha256,
            "crypto_image_sha256": args.crypto_image_sha256,
            "equal": True,
            "image_bytes": args.image_bytes,
        },
        "diagnostic_identity": {
            "note": "retained for attribution; never a threshold",
            "elf_bytes": args.elf_bytes,
            "text_addr": args.text_addr,
            "text_size": args.text_size,
            "compress_block_addr": args.compress_block_addr,
        },
        "fixture": {"capsule_sha256": args.capsule_sha256},
        "discipline": {
            "warmups": args.warmups,
            "samples": args.samples,
            "percentile": "nearest-rank",
            "mode_selector": "opt/tos/measurement-mode via firmware configuration",
        },
        "source": {"commit": commit(), "dirty": dirty()},
        "full_exact": full,
        "unavoidable_crypto": crypto,
        "ratio_p95": ratio_p95,
        "ratio_median": ratio_median,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    ms = lambda ns: ns / 1e6
    print(
        f"PAIRED-MEASUREMENT {args.label}: ratio_p95={ratio_p95:.3f} "
        f"ratio_median={ratio_median:.3f}"
    )
    print(
        f"  full   median {ms(full['median_ns']):8.1f} ms  p95 {ms(full['p95_ns']):8.1f} ms  "
        f"p99 {ms(full['p99_ns']):8.1f} ms  n={len(full['measured_ns'])}"
    )
    print(
        f"  crypto median {ms(crypto['median_ns']):8.1f} ms  p95 {ms(crypto['p95_ns']):8.1f} ms  "
        f"p99 {ms(crypto['p99_ns']):8.1f} ms  n={len(crypto['measured_ns'])}"
    )
    print(f"  same artifact: both series from {args.full_image_sha256[:32]}…")
    print(f"  no threshold applied; ADR-0083 is Proposed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
