#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Black-box tests for the non-production adapted-Rust profile."""

from __future__ import annotations

import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parent


def compile_profile() -> pathlib.Path:
    directory = tempfile.TemporaryDirectory()
    binary = pathlib.Path(directory.name) / "adapted-rust-profile"
    subprocess.run(
        ["rustc", "--edition=2024", "-D", "warnings", "-O", ROOT / "profile.rs", "-o", binary],
        check=True,
        capture_output=True,
        text=True,
    )
    return binary, directory


def run_profile(mode: str, workers: int) -> dict[str, str]:
    binary, directory = compile_profile()
    try:
        completed = subprocess.run(
            [binary, "--mode", mode, "--workers", str(workers)],
            check=True,
            capture_output=True,
            text=True,
        )
    finally:
        directory.cleanup()
    return dict(item.split("=", 1) for item in completed.stdout.split() if "=" in item)


class AdaptedRustProfileTests(unittest.TestCase):
    def test_reference_and_parallel_profiles_preserve_the_common_result(self) -> None:
        serial = run_profile("reference", 1)
        parallel = run_profile("parallel", 2)

        self.assertEqual(serial["digest"], parallel["digest"])
        self.assertEqual(serial["cases"], "13")
        self.assertEqual(serial["overlap"], "false")
        self.assertEqual(parallel["overlap"], "true")
        self.assertGreaterEqual(int(parallel["max_active"]), 2)
        self.assertGreaterEqual(int(parallel["cpus"]), 2)

    def test_safe_rust_rejects_capability_forgery_and_competing_mutable_borrows(self) -> None:
        for source, diagnostic in [
            ("invalid_capability.rs", "is private"),
            ("invalid_mutable_share.rs", "cannot borrow"),
        ]:
            completed = subprocess.run(
                ["rustc", "--edition=2024", ROOT / source],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(completed.returncode, 0, source)
            self.assertIn(diagnostic, completed.stderr, source)

    def test_profile_has_atomic_publication_structured_cancel_and_task_bound(self) -> None:
        result = run_profile("reference", 1)

        self.assertEqual(result["atomic"], "accept")
        self.assertEqual(result["cancel"], "joined")
        self.assertEqual(result["task_quota"], "reject")


if __name__ == "__main__":
    unittest.main()
