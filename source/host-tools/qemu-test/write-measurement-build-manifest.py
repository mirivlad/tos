#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bind exact Cargo feature declarations to exact measurement artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def features(value: str) -> list[str]:
    parsed = value.split(",") if value else []
    if not parsed or any(not re.fullmatch(r"[a-z0-9-]+", item) for item in parsed):
        raise argparse.ArgumentTypeError("features must be a comma-separated nonempty list")
    if len(parsed) != len(set(parsed)):
        raise argparse.ArgumentTypeError("features must not repeat")
    return parsed


def build_record(
    package: str, artifact: Path, selected: list[str], target_dir: Path
) -> dict[str, object]:
    if not artifact.is_file():
        raise SystemExit(f"measurement artifact is missing: {artifact}")
    return {
        "package": package,
        "target": "x86_64-unknown-none",
        "profile": "release",
        "features": selected,
        "cargo_target_dir": str(target_dir.resolve()),
        "cargo_command": [
            "cargo",
            "build",
            "--release",
            "-p",
            package,
            "--target",
            "x86_64-unknown-none",
            "--features",
            ",".join(selected),
        ],
        "artifact_path": str(artifact.resolve()),
        "artifact_sha256": sha256(artifact),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True, type=Path)
    parser.add_argument("--target-dir", required=True, type=Path)
    parser.add_argument("--nucleus", required=True, type=Path)
    parser.add_argument("--nucleus-features", required=True, type=features)
    parser.add_argument("--runtime-image", required=True, type=Path)
    parser.add_argument("--runtime-features", required=True, type=features)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    commit = subprocess.run(
        ["git", "-C", str(args.repository), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    manifest = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "schema": "tos-measurement-build-v1",
        "source_commit": commit,
        "builds": {
            "nucleus": build_record(
                "tos-nucleus", args.nucleus, args.nucleus_features, args.target_dir
            ),
            "runtime_image": build_record(
                "tos-runtime-image",
                args.runtime_image,
                args.runtime_features,
                args.target_dir,
            ),
        },
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
