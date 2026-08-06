#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs" / "SPECIFICATION_SOURCES.txt"
OUTPUT = ROOT / "TOS_DEVELOPMENT_SPECIFICATION.md"
VERSION = ROOT / "VERSION"


def load_sources() -> list[str]:
    paths: list[str] = []
    seen: set[str] = set()
    for raw in MANIFEST.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line in seen:
            raise SystemExit(f"duplicate specification source: {line}")
        if line == "TOS_DEVELOPMENT_SPECIFICATION.md":
            raise SystemExit("generated specification cannot include itself")
        path = ROOT / line
        if not path.is_file():
            raise SystemExit(f"missing specification source: {line}")
        seen.add(line)
        paths.append(line)
    if not paths:
        raise SystemExit("empty specification source manifest")
    return paths


def source_digest(paths: list[str]) -> str:
    h = hashlib.sha256()
    for rel in paths:
        data = (ROOT / rel).read_bytes()
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(len(data).to_bytes(8, "big"))
        h.update(data)
    return h.hexdigest()


def render(paths: list[str]) -> bytes:
    version = VERSION.read_text(encoding="utf-8").strip()
    digest = source_digest(paths)
    parts = [
        "<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->\n\n",
        "# TOS — consolidated development specification\n\n",
        "> **GENERATED FILE — DO NOT EDIT.**  \n",
        "> This file is a non-normative convenience view. Individual source documents and accepted ADRs govern according to `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.\n\n",
        f"Version: {version}  \n",
        f"Source-manifest SHA-256: `{digest}`  \n",
        "Generator: `tools/build-specification.py`\n\n",
        "---\n\n",
    ]
    for rel in paths:
        text = (ROOT / rel).read_text(encoding="utf-8").rstrip()
        parts.extend([
            f"<!-- BEGIN {rel} -->\n\n",
            text,
            "\n\n",
            f"<!-- END {rel} -->\n\n",
            "---\n\n",
        ])
    return "".join(parts).encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if the committed generated file is stale")
    args = parser.parse_args()
    paths = load_sources()
    expected = render(paths)
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_bytes() != expected:
            print("TOS_DEVELOPMENT_SPECIFICATION.md is stale; run tools/build-specification.py", file=sys.stderr)
            return 1
        print("generated specification is current")
        return 0
    OUTPUT.write_bytes(expected)
    print(f"wrote {OUTPUT.relative_to(ROOT)} from {len(paths)} sources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
