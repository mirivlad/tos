#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The same-artifact paired Stage 1 validation-performance report (ADR-0083).

**The refusal is the point of this program.** A ratio between two series that
came from different executables is what the Stage 4C construct-validity
investigation falsified: an inert layout change moved the old cross-artifact
quotient across its conformance boundary while native execution was unmoved.
So this reporter will not compute a ratio at all unless both series report
exactly equal image digests.

**The threshold is optional and is supplied by the caller**, so that one program
serves both roles ADR-0083 §10 separates: with `--max-p95-ratio` it is the
active Stage 1 conformance gate and refuses a run above the bound; without it,
it is the reproduction and research tool that measures and reports.

The accepted bound is `1.30` on the **p95** ratio. The median ratio is computed
and retained beside it as diagnostic and regression evidence, and is never the
conformance statistic.
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
    p.add_argument(
        "--max-p95-ratio",
        type=float,
        help="apply ADR-0083's blocking conformance bound; omit to only report",
    )
    p.add_argument(
        "--baseline-p95-ratio",
        type=float,
        help="the retained accepted baseline, for the regression deviation",
    )
    p.add_argument("--baseline-median-ratio", type=float)
    p.add_argument("--evidence-status", default=None, choices=[None, "P1", "P2"])
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

    # The conformance statistic is the p95 ratio. The median ratio is retained
    # beside it and decides nothing, because it is the diagnostic figure.
    verdict = None
    if args.max_p95_ratio is not None:
        verdict = "pass" if ratio_p95 <= args.max_p95_ratio else "fail"

    regression = None
    if args.baseline_p95_ratio:
        regression = {
            "note": (
                "ADR-0083 section 10: the repository regression policy applies "
                "relative to this retained baseline, not to the constant 1.0"
            ),
            "baseline_p95_ratio": args.baseline_p95_ratio,
            "baseline_median_ratio": args.baseline_median_ratio,
            "p95_deviation": ratio_p95 / args.baseline_p95_ratio - 1.0,
            "median_deviation": (
                ratio_median / args.baseline_median_ratio - 1.0
                if args.baseline_median_ratio
                else None
            ),
            "explanation_above": 0.15,
            "blocking_above": 0.30,
        }

    report = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "metric": "same-artifact paired Stage 1 validation performance (ADR-0083)",
        "adr": "ADR-0083",
        "label": args.label,
        "evidence_status": args.evidence_status,
        "conformance_statistic": "p95 ratio",
        "threshold": args.max_p95_ratio,
        "verdict": verdict,
        "note": (
            "ADR-0083 accepted 2026-09-06. The p95 ratio is conformance; the "
            "median ratio is diagnostic."
            if args.max_p95_ratio is not None
            else (
                "No threshold applied: this run is reproduction/research "
                "evidence, not the conformance gate."
            )
        ),
        "regression": regression,
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
    if regression is not None:
        print(
            f"  retained baseline p95 {args.baseline_p95_ratio:.4f}: "
            f"deviation {regression['p95_deviation'] * 100:+.1f}% "
            f"(explain above +15%, blocks above +30%)"
        )
    if verdict is None:
        print("  no threshold applied; this run is reproduction evidence")
        return 0
    if verdict == "fail":
        print(
            f"PAIRED-MEASUREMENT FAIL: p95 ratio {ratio_p95:.3f} exceeds the "
            f"ADR-0083 bound {args.max_p95_ratio:.2f}"
        )
        return 1
    print(
        f"  ADR-0083 conformance: p95 ratio {ratio_p95:.3f} <= "
        f"{args.max_p95_ratio:.2f} PASS"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
