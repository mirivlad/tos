#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Black-box tests for the non-production bespoke foundation model."""

from __future__ import annotations

import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parent


def run_model(mode: str, workers: int) -> dict[str, str]:
    with tempfile.TemporaryDirectory() as directory:
        binary = pathlib.Path(directory) / "bespoke-model"
        subprocess.run(
            ["rustc", "--edition=2024", "-O", ROOT / "model.rs", "-o", binary],
            check=True,
            capture_output=True,
            text=True,
        )
        completed = subprocess.run(
            [binary, "--mode", mode, "--workers", str(workers)],
            check=True,
            capture_output=True,
            text=True,
        )
    return dict(item.split("=", 1) for item in completed.stdout.split() if "=" in item)


class BespokeModelTests(unittest.TestCase):
    def test_reference_and_parallel_modes_preserve_common_semantics(self) -> None:
        serial = run_model("reference", 1)
        parallel = run_model("parallel", 2)

        self.assertEqual(serial["digest"], parallel["digest"])
        self.assertEqual(serial["cases"], "13")
        self.assertEqual(serial["overlap"], "false")
        self.assertEqual(parallel["overlap"], "true")
        self.assertGreaterEqual(int(parallel["max_active"]), 2)
        self.assertGreaterEqual(int(parallel["cpus"]), 2)

    def test_model_rejects_the_unsafe_share_and_task_quota_cases(self) -> None:
        result = run_model("reference", 1)

        self.assertEqual(result["mutable_share"], "reject")
        self.assertEqual(result["task_quota"], "reject")
        self.assertEqual(result["atomic"], "accept")
        self.assertEqual(result["cancel"], "joined")


if __name__ == "__main__":
    unittest.main()
