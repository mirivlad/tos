#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Enforce local safety rationales for unsafe Stage 1 Rust operations."""

import argparse
import re
from pathlib import Path


UNSAFE_BLOCK = re.compile(r"\bunsafe\s*\{")
UNSAFE_FN = re.compile(r"\bunsafe\s+fn\b")
UNSAFE_EXTERN = re.compile(r"\bunsafe\s+extern\b")


def without_literals(lines: list[str]) -> list[str]:
    """Blank the contents of Rust string literals, preserving line geometry.

    Rust tests embed TOS Core sample source as string literals, and a sample
    may legitimately contain `unsafe { ... }`. Matching the raw text would
    report that sample as an unsafe Rust block. Each removed character is
    replaced by a space so line and column positions still refer to the file.
    """

    stripped: list[str] = []
    in_string = False
    escaped = False
    for line in lines:
        result: list[str] = []
        index = 0
        while index < len(line):
            character = line[index]
            if in_string:
                if escaped:
                    escaped = False
                elif character == "\\":
                    escaped = True
                elif character == '"':
                    in_string = False
                result.append(" ")
                index += 1
                continue
            if character == '"':
                in_string = True
                result.append(" ")
                index += 1
                continue
            if character == "/" and line[index + 1 : index + 2] == "/":
                result.append(" " * (len(line) - index))
                break
            result.append(character)
            index += 1
        stripped.append("".join(result))
        # A backslash at end of line continues the literal onto the next line.
        if not in_string:
            escaped = False
    return stripped


def local_safety_comment(lines: list[str], line_number: int) -> bool:
    """Return whether the contiguous preceding comment block has SAFETY: text."""

    index = line_number - 2
    skipped_assignment_line = False
    while index >= 0:
        stripped = lines[index].strip()
        if not stripped:
            return False
        if stripped.startswith("#["):
            index -= 1
            continue
        if not skipped_assignment_line and stripped.endswith("="):
            # rustfmt may put `unsafe {` on the next line of an assignment;
            # keep the immediately preceding rationale local in that form.
            skipped_assignment_line = True
            index -= 1
            continue
        if not (
            stripped.startswith("//")
            or stripped.startswith("/*")
            or stripped.startswith("*")
            or stripped.endswith("*/")
        ):
            return False
        if "SAFETY:" in stripped:
            return True
        index -= 1
    return False


def rust_sources(root: Path) -> list[Path]:
    source_root = root / "source"
    if not source_root.is_dir():
        return []
    return sorted(
        path
        for path in source_root.rglob("*.rs")
        if "target" not in path.relative_to(source_root).parts
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    root = parser.parse_args().root.resolve()
    failures: list[str] = []
    checked = 0

    for path in rust_sources(root):
        relative = path.relative_to(root)
        lines = path.read_text(encoding="utf-8").splitlines()
        # Rationales are looked up in the original text; matches are taken from
        # code with literal contents removed.
        for number, line in enumerate(without_literals(lines), start=1):
            stripped = line.lstrip()
            if stripped.startswith("//"):
                continue
            operation = None
            if UNSAFE_BLOCK.search(line):
                operation = "unsafe block"
            elif UNSAFE_FN.search(line):
                operation = "unsafe function"
            elif UNSAFE_EXTERN.search(line):
                operation = "unsafe extern declaration"
            if operation is None:
                continue
            checked += 1
            if not local_safety_comment(lines, number):
                failures.append(
                    f"{relative}:{number}: missing local SAFETY comment for {operation}"
                )

    if failures:
        print("unsafe-safety: FAIL")
        print("\n".join(failures))
        return 1
    print(f"unsafe-safety: OK ({checked} unsafe operation(s) carry local SAFETY rationales)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
