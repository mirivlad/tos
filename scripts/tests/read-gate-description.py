#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""How `scripts/preflight.sh` describes one gate, and nothing else of that file.

Two pieces, because a reader meets the gate through either of them: the
contiguous comment block immediately above `<gate>() {`, which is what somebody
reading the source sees, and the one inventory row naming `<gate>`, which is what
`--list` and a CI log print.

**Extracted structurally rather than by searching the file.** The inventory
documents a hundred gates and a later one may legitimately claim something this
one may not, so a checker that grepped `preflight.sh` would be policing a phrase
instead of checking a claim. Prints nothing when neither piece is found, and the
caller treats that as a failure: a description that cannot be located is a
description nobody is checking.
"""

from __future__ import annotations

import pathlib
import re
import sys


def description(inventory: pathlib.Path, gate: str) -> list[str]:
    lines = inventory.read_text().splitlines()
    row = re.compile(rf'^gate +\S+ +\S+ +".*" +{re.escape(gate)}$')
    definition = f"{gate}() "
    out: list[str] = []
    for index, line in enumerate(lines):
        if line.startswith(definition):
            block: list[str] = []
            probe = index - 1
            while probe >= 0 and lines[probe].startswith("#"):
                block.append(lines[probe])
                probe -= 1
            out.extend(reversed(block))
        if row.match(line):
            out.append(line)
    return out


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: read-gate-description.py INVENTORY GATE", file=sys.stderr)
        return 2
    for line in description(pathlib.Path(sys.argv[1]), sys.argv[2]):
        print(line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
