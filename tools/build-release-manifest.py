#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate MANIFEST.txt and SHA256SUMS for the TOS release package.

Two rules decided by the project owner (see docs/28):

* ``SHA256SUMS`` verifies the integrity of the release-package files **outside
  Git**. Git object identity is the canonical integrity of the source tree, so
  ``source/`` is deliberately not hashed here: a flat digest list over the
  source tree would be a second, weaker Git and a competing source of truth.
* ``MANIFEST.txt`` describes the release baseline and is generated from its
  actual composition. Aggregate numbers are derived, never hand-maintained -
  the file previously claimed "15 accepted ADRs" while there were 17.
"""

from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "MANIFEST.txt"
SUMS = ROOT / "SHA256SUMS"
VERSION = ROOT / "VERSION"
INVARIANTS = ROOT / "docs" / "02_SYSTEM_INVARIANTS.md"
ADR_DIR = ROOT / "docs" / "adr"

# Excluded from the release package, each for a stated reason.
EXCLUDED_PREFIXES = ("source/",)          # Git tree/commit identity is canonical there
EXCLUDED_FILES = {
    "SHA256SUMS",                          # cannot contain its own digest
    "PROGRESS.md",                         # non-normative working log
    "WORKLOG_STAGE1_HARDENING.md",         # non-normative working log
}


def tracked_files() -> list[str]:
    """Tracked files only.

    A file that has just been created is not tracked yet, so regenerate after
    staging it - otherwise the count is one short and ``--check`` fails on the
    very commit that adds it (which is exactly how this note came to exist).
    """
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    files = []
    for line in out.splitlines():
        rel = line.strip()
        if not rel or rel in EXCLUDED_FILES:
            continue
        if any(rel.startswith(p) for p in EXCLUDED_PREFIXES):
            continue
        files.append(rel)
    return sorted(files)


def adrs() -> list[tuple[str, str, str]]:
    """(file, title, status) for every ADR, ordered by number."""
    rows = []
    for p in sorted(ADR_DIR.glob("*.md")):
        text = p.read_text(encoding="utf-8")
        title = next(
            (l[2:].strip() for l in text.splitlines() if l.startswith("# ")),
            p.stem,
        )
        status = next(
            (
                l.split(":", 1)[1].strip()
                for l in text.splitlines()
                if l.startswith("- Status:")
            ),
            "unknown",
        )
        rows.append((p.relative_to(ROOT).as_posix(), title, status))
    return rows


def invariant_count() -> int:
    return len(re.findall(r"^## I-\d+", INVARIANTS.read_text(encoding="utf-8"), re.M))


def render_manifest(files: list[str]) -> bytes:
    version = VERSION.read_text(encoding="utf-8").strip()
    adr_rows = adrs()
    accepted = [r for r in adr_rows if r[2].lower().startswith("accepted")]
    lines = [
        "TOS — TextOS Development Documentation",
        f"Version: {version}",
        "Status: Accepted architecture, governance, licensing, security-model and",
        "implementation-gate baseline for Stage 1",
        "",
        "GENERATED FILE — DO NOT EDIT.",
        "Regenerate with `python3 tools/build-release-manifest.py`; the release date",
        "and per-revision narrative belong to CHANGELOG.md.",
        "",
        "Scope of the release package",
        "----------------------------",
        "This manifest and SHA256SUMS describe the files distributed as the",
        "documentation and governance release package, and let a recipient verify",
        "them without Git.",
        "",
        "`source/` is intentionally excluded: the Git tree and commit identity, the",
        "capsule provenance record and the boot chain already establish a stronger",
        "identity for it. A second flat digest list over the source tree would be a",
        "weaker duplicate of Git and a competing source of truth.",
        "",
        "Canonical entry points",
        "----------------------",
        "- README.md",
        "- ARCHITECTURE.md",
        "- docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md",
        "- docs/02_SYSTEM_INVARIANTS.md",
        "- docs/21_ARCHITECTURE_PRESERVATION_POLICY.md",
        "- docs/37_STAGE_IDENTITY_GATES.md",
        "- AGENTS.md",
        "- CODEX_START.md",
        "- LICENSE.md",
        "- GOVERNANCE.md",
        "- PATENTS.md",
        "",
        "Generated convenience view",
        "--------------------------",
        "- TOS_DEVELOPMENT_SPECIFICATION.md (non-normative; do not edit)",
        "",
        "Composition (derived, not hand-maintained)",
        "------------------------------------------",
        f"- files in the release package: {len(files)}",
        f"- active system invariants: {invariant_count()}",
        f"- architecture decision records: {len(adr_rows)} ({len(accepted)} accepted)",
        "",
        "Architecture decision records",
        "-----------------------------",
    ]
    for rel, title, status in adr_rows:
        lines.append(f"- {rel} — {title} [{status}]")
    lines += [
        "",
        "Legal note: project policy is not jurisdiction-specific legal advice or a",
        "freedom-to-operate opinion.",
        "",
    ]
    return "\n".join(lines).encode("utf-8")


def render_sums(files: list[str]) -> bytes:
    out = []
    for rel in files:
        digest = hashlib.sha256((ROOT / rel).read_bytes()).hexdigest()
        out.append(f"{digest}  {rel}")
    return ("\n".join(out) + "\n").encode("utf-8")


def main() -> int:
    check = "--check" in sys.argv[1:]
    files = tracked_files()
    # MANIFEST.txt is part of the package, so its own digest depends on its
    # contents: render it first, write it, then hash the package.
    manifest = render_manifest(files)
    if check:
        stale = []
        if not MANIFEST.exists() or MANIFEST.read_bytes() != manifest:
            stale.append("MANIFEST.txt")
        expected_sums = None
        if not stale:
            expected_sums = render_sums(files)
            if not SUMS.exists() or SUMS.read_bytes() != expected_sums:
                stale.append("SHA256SUMS")
        if stale:
            print(
                f"stale: {', '.join(stale)}; run tools/build-release-manifest.py",
                file=sys.stderr,
            )
            return 1
        print(f"release manifest is current ({len(files)} files)")
        return 0

    MANIFEST.write_bytes(manifest)
    SUMS.write_bytes(render_sums(files))
    print(f"wrote MANIFEST.txt and SHA256SUMS for {len(files)} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
