#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""A green required CI on a commit means no less than a green `--full` on it.

ADR-0065 fixes that as the parity rule and this gate is where it holds. The rule
it enforces is deliberately not "the same list of gates appears in two places" —
there is one list, the inventory in `scripts/preflight.sh`, and CI names
**profiles**. A workflow cannot omit a gate, because a workflow never mentions
one; it can only fail to run a profile, and that is what is checked here.

Three assertions:

1. every profile the inventory declares is run by some repository-conformance
   job — otherwise the gates in it are proved locally and nowhere else;
2. every profile a workflow runs exists in the inventory — a typo names a
   profile that selects nothing, and a step that runs nothing passes;
3. every step of such a job is either a profile invocation or declares itself to
   be environment. Anything else is a repository check written in YAML, which is
   the second implementation ADR-0065 exists to prevent.

**A step declares itself environment in YAML, not in a comment**: `env:` with
`GATE_PARITY: environment` on the step. A comment is not part of the document a
YAML parser sees, and a marker a parser cannot see is a marker that means
whatever the next reader thinks it means.

Steps that only `uses:` an action (checkout, artifact upload) carry no command
and are environment by construction.

**Scope.** This proves parity against the repository-conformance jobs declared in
`.github/workflows`. Whether a hosting platform has been configured to *require*
those jobs is not visible from the repository, so this gate does not claim it
(ADR-0065 §"What this does not decide").
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover - environment defect, not a finding
    print(
        "check-gate-parity: PyYAML is required to read the workflows structurally",
        file=sys.stderr,
    )
    raise SystemExit(1)

WORKFLOWS = ".github/workflows"
PREFLIGHT = "scripts/preflight.sh"
MARKER_KEY = "GATE_PARITY"
MARKER_VALUE = "environment"


def inventory(root: Path) -> dict[str, int]:
    """Profiles and their gate counts, from the inventory itself.

    `--list` runs nothing, which is what makes it usable from inside a gate.
    """
    listing = subprocess.run(
        ["sh", str(root / PREFLIGHT), "--list"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    profiles: dict[str, int] = {}
    for line in listing.splitlines():
        if not line.strip():
            continue
        profile, scope, _label = line.split("\t", 2)
        if scope not in ("default", "full-only"):
            raise SystemExit(f"check-gate-parity: unknown local scope: {scope}")
        profiles[profile] = profiles.get(profile, 0) + 1
    return profiles


def profile_invoked(command: str) -> str | None:
    """The profile a step runs, or `None` when the step runs no profile."""
    words = command.split()
    for index, word in enumerate(words):
        if word == "--profile" and index + 1 < len(words):
            if any(part.endswith("preflight.sh") for part in words[:index]):
                return words[index + 1]
    return None


def is_environment(step: dict) -> bool:
    environment = step.get("env") or {}
    return str(environment.get(MARKER_KEY, "")) == MARKER_VALUE


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    failures: list[str] = []

    profiles = inventory(root)
    invoked: dict[str, list[str]] = {}

    for path in sorted((root / WORKFLOWS).glob("*.yml")):
        document = yaml.safe_load(path.read_text(encoding="utf-8"))
        for job_name, job in (document.get("jobs") or {}).items():
            where = f"{path.name}:{job_name}"
            steps = job.get("steps") or []
            runs_a_profile = any(
                profile_invoked(step["run"]) for step in steps if "run" in step
            )
            for step in steps:
                if "run" not in step:
                    continue  # `uses:` steps carry no command of their own
                profile = profile_invoked(step["run"])
                if profile is not None:
                    invoked.setdefault(profile, []).append(where)
                    continue
                if is_environment(step):
                    continue
                if not runs_a_profile:
                    # A job that runs no profile at all is not a
                    # repository-conformance job and is none of this gate's
                    # business.
                    continue
                name = step.get("name", "<unnamed step>")
                failures.append(
                    f"{where}: step '{name}' runs a command that is neither a "
                    f"profile nor declared `env: {MARKER_KEY}: {MARKER_VALUE}`"
                )

    for profile in sorted(profiles):
        if profile not in invoked:
            failures.append(
                f"profile '{profile}' ({profiles[profile]} gate(s)) is run by no "
                "workflow job: those gates are proved locally and nowhere else"
            )
    for profile in sorted(invoked):
        if profile not in profiles:
            failures.append(
                f"workflow runs profile '{profile}', which the inventory does not "
                f"declare (runs nothing): {', '.join(invoked[profile])}"
            )

    if failures:
        for failure in failures:
            print(f"check-gate-parity: {failure}", file=sys.stderr)
        return 1

    total = sum(profiles.values())
    print(
        f"check-gate-parity: PASS ({total} gate(s) in {len(profiles)} profile(s), "
        f"each profile run by a workflow job)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
