#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The source manifest is complete, and every entry in it exists.

`docs/38` §Release check: a documentation release is invalid if "an accepted ADR
is absent from the source manifest". That sentence was true and unenforced
locally, and the gap it left is a particular one worth naming, because the
repository already had a check that looked like it covered this:

    `tools/build-specification.py --check` proves
        generated output == output(listed inputs)

    it does not prove
        listed inputs == all required inputs

Reproducibility is a statement about a list; completeness is a statement about
the list itself. A decision the Project Architect accepted and nobody added
stayed outside the consolidated specification while every reproducibility check
passed, because the missing input was missing from both sides of the comparison.

**Status comes from each ADR's own `- Status:` line**, never from a number range
and never from a list kept here. A gate that knew which ADRs exist would need
editing for ADR-0065, and an edit that has to be remembered is the same failure
one level up.

Only *accepted* decisions are required, and that is the whole of the normative
fact. `docs/38` says an accepted ADR must be in the manifest; it says nothing
about any other status, so neither does this gate — it does not require a
`Proposed` ADR to be listed and does not forbid it either.

Being listed would not make a Proposed ADR authoritative in any case. `docs/38`
excludes that in two places: listing a path "does not by itself grant Tier 2
authority", and the generated bundle is Tier 5, "never independent authority".
Authority comes from a document's own status, which is also where this gate reads
it from. So the reason `Proposed` is not required here is simply that no accepted
document requires it — not that listing one would promote it.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

MANIFEST = "docs/SPECIFICATION_SOURCES.txt"
ADR_GLOB = "docs/adr/*.md"


def status_word(path: Path) -> str | None:
    """The first word of an ADR's own status line, lowercased.

    `- Status: **Accepted (option B)** — …` and
    `- Status: Accepted (Project Architect-approved), revision 4` are the same
    status written two ways, so emphasis, parentheses and trailing prose are
    stripped and what is compared is the word the line begins with. Returns
    `None` when the file has no status line at all, which is a defect of its own:
    a decision whose status cannot be read cannot be filed.
    """
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("- Status:"):
            continue
        rest = line.split(":", 1)[1]
        word = re.match(r"[\s*_]*([A-Za-z]+)", rest)
        return word.group(1).lower() if word else None
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    failures: list[str] = []

    entries: list[str] = []
    for line in (root / MANIFEST).read_text(encoding="utf-8").splitlines():
        entry = line.strip()
        if entry and not entry.startswith("#"):
            entries.append(entry)

    # Every entry names something. A manifest line pointing at a file that was
    # renamed or removed would make the generator fail with a path error rather
    # than a sentence about what is wrong.
    for entry in entries:
        if not (root / entry).is_file():
            failures.append(f"manifest entry names no file: {entry}")

    # Once each. A duplicate would put a document into the consolidated view
    # twice, which no reproducibility check would notice: the output would be
    # deterministic and wrong.
    for entry in sorted(set(entries)):
        if entries.count(entry) > 1:
            failures.append(f"manifest lists {entry} {entries.count(entry)} times")

    listed = set(entries)
    accepted: list[str] = []
    other: list[tuple[str, str]] = []
    for path in sorted(root.glob(ADR_GLOB)):
        relative = path.relative_to(root).as_posix()
        word = status_word(path)
        if word is None:
            failures.append(f"ADR has no `- Status:` line: {relative}")
            continue
        if word == "accepted":
            accepted.append(relative)
        else:
            other.append((relative, word))

    for relative in accepted:
        if relative not in listed:
            failures.append(
                f"accepted ADR absent from the source manifest (docs/38 release check): {relative}"
            )

    if failures:
        for failure in failures:
            print(f"specification-manifest: {failure}", file=sys.stderr)
        return 1

    print(
        f"specification-manifest: PASS ({len(entries)} manifest entries, "
        f"{len(accepted)} accepted ADR(s) all listed, "
        f"{len(other)} ADR(s) not accepted and not required)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
