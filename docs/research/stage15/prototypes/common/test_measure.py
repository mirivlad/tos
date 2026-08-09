#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression tests for the non-production Stage 1.5 measurement harness."""

from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import measure


ROOT = pathlib.Path(__file__).resolve().parent


class MeasureHarnessTests(unittest.TestCase):
    def test_validate_cases_accepts_the_committed_common_corpus(self) -> None:
        cases = measure.load_cases(ROOT / "cases.json")

        self.assertGreaterEqual(len(cases), 10)
        self.assertIn("multicore.partitioned-reduction", {case["id"] for case in cases})

    def test_measurement_records_exact_samples_and_command_result(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / "samples.json"
            records = measure.measure(
                command=[sys.executable, "-c", "print('digest=abc123 overlap=true')"],
                warmups=1,
                samples=2,
                label="unit",
                workers=2,
                output=output,
            )

            self.assertEqual(len(records["samples_ns"]), 2)
            self.assertEqual(records["result_digest"], "abc123")
            self.assertTrue(records["overlap_observed"])
            persisted = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(persisted["label"], "unit")
            self.assertEqual(persisted["record_spdx_license"], "GPL-3.0-or-later")
            self.assertEqual(output.read_text(encoding="utf-8").splitlines()[1], '  "record_spdx_license": "GPL-3.0-or-later",')


if __name__ == "__main__":
    unittest.main()
