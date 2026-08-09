#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify the version-pinned UCD input set required by ADR-0029."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    root = parser.parse_args().root.resolve()
    data_dir = root / "source/crates/tos-core/unicode/ucd-17.0.0"
    record = json.loads((data_dir.parent / "PROVENANCE.json").read_text(encoding="utf-8"))
    required = {
        "unicode_version": "17.0.0", "uax15_revision": 57,
        "normalization_form": "NFC", "license_expression": "Unicode-3.0",
        "license_notice": "LICENSES/Unicode-3.0.txt",
    }
    for key, expected in required.items():
        if record.get(key) != expected:
            print(f"unicode-provenance: FAIL: {key}", file=sys.stderr)
            return 1
    inputs = record.get("inputs")
    if not isinstance(inputs, list) or len(inputs) != 4:
        print("unicode-provenance: FAIL: input set", file=sys.stderr)
        return 1
    listed = set()
    for item in inputs:
        name, digest = item.get("file"), item.get("sha256")
        path = data_dir / name if isinstance(name, str) else None
        if path is None or not isinstance(digest, str) or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            print(f"unicode-provenance: FAIL: {name}", file=sys.stderr)
            return 1
        listed.add(name)
    if listed != {path.name for path in data_dir.glob("*.txt")} or not (root / required["license_notice"]).is_file():
        print("unicode-provenance: FAIL: input/notice set", file=sys.stderr)
        return 1
    print("unicode-provenance: PASS (Unicode 17.0.0 / UAX #15 Revision 57)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
