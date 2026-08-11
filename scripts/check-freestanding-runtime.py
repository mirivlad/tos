#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prove the Stage 2 production runtime has no host dependency.

`docs/44` states the contract: Rust may implement these components, but rustc,
LLVM, libc, the C ABI and host threads are not recovery or runtime
dependencies. That is a claim about the *production* code, so this gate checks
the production code and leaves the test harness alone — a test harness is a host
program by construction.

Two things are checked, and they answer different questions:

  1. no production module names a host facility. This catches an import that
     compiles today because a host build happens to provide it.
  2. the crate declares `#![no_std]`. Without it, the first check is only a
     naming convention: `std` would still be linked and reachable.

The freestanding build itself is a separate preflight gate, because a build is
the only thing that proves the whole dependency closure is free of `std`.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# The five crates that form the Stage 2 production path.
RUNTIME_CRATES = ["tos-core", "tos-ir", "tos-verifier", "tos-engine", "tos-cache"]

# Facilities a freestanding runtime cannot have. `std::` as a path is the
# general case; the rest are named so a diagnostic says which contract they
# break rather than only that a rule fired.
FORBIDDEN = [
    (r"\bstd::fs\b", "host filesystem"),
    (r"\bstd::io\b", "host I/O"),
    (r"\bstd::env\b", "host environment"),
    (r"\bstd::net\b", "host network"),
    (r"\bstd::thread\b", "host threads"),
    (r"\bstd::time\b", "host clock"),
    (r"\bstd::process\b", "host process control"),
    (r"\bstd::sync\b", "host synchronization"),
    (r"\bstd::os\b", "host OS interface"),
    (r"\blibc::", "libc"),
    (r'extern\s+"C"', "C ABI"),
]


def production_source(path: Path) -> str:
    """The part of a file that is not its test module.

    A `#[cfg(test)]` module is a host program, so what it uses says nothing
    about the runtime. Everything before the marker is production code.
    """
    text = path.read_text(encoding="utf-8")
    marker = text.find("#[cfg(test)]\nmod tests")
    return text if marker == -1 else text[:marker]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    failures: list[str] = []

    for crate in RUNTIME_CRATES:
        source = root / "source" / "crates" / crate / "src"
        if not source.is_dir():
            failures.append(f"{crate}: no source directory")
            continue

        entry = source / "lib.rs"
        if "#![no_std]" not in entry.read_text(encoding="utf-8"):
            failures.append(
                f"{crate}: lib.rs does not declare #![no_std], so `std` is still linked"
            )

        for file in sorted(source.rglob("*.rs")):
            body = production_source(file)
            where = file.relative_to(root)
            for pattern, what in FORBIDDEN:
                for match in re.finditer(pattern, body):
                    line = body.count("\n", 0, match.start()) + 1
                    failures.append(f"{where}:{line}: production code reaches {what}")

    if failures:
        print("freestanding-runtime: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print(
        "freestanding-runtime: PASS "
        f"({len(RUNTIME_CRATES)} crates are no_std with no host facility)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
