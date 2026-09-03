#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The operator's important-error view of a boot (`RUNTIME_OBSERVABILITY_V1` §9).

This is a **reader**, not a component. It holds no state, produces no events,
and can be replaced by `grep` without losing anything — because the selection
rule is one segment of a name, which is what §9 fixed it to be. What it adds is
that a person does not have to know the rule.

    python3 scripts/tos-journal.py boot/serial.log
    python3 scripts/tos-journal.py --severity INFO boot/serial.log
    python3 scripts/tos-journal.py --check boot/serial.log

The view is the transport in the order it was produced, filtered to a severity.
Nothing is copied, summarised or reordered: an event appears once, where it
happened, and `--severity DEBUG` prints the whole transport.
"""

import argparse
import re
import sys

# §9.2. Severity is a property of the event kind, declared per identifier, so an
# emitter has no severity to choose and a reader applies this. Everything not
# listed is INFO.
#
# Boot ABI v1's own failure vocabulary is FATAL in its entirety, by that
# contract rather than by this table (BOOT_ABI_V1 §7).
BOOT_FAILURES = (
    "TOS.BOOT.FAILC",
    "TOS.BOOT.FAILI",
    "TOS.ABI.FAIL",
    "TOS.MEM.FAIL",
    "TOS.CAPSULE.FAIL",
    "TOS.IDENTITY.MISMATCH",
    "TOS.PANIC",
    "TOS.EXCEPTION",
    "TOS.BOOTMODULE.FAIL",
)

CLASSIFIED = {
    "TOS.NUCLEUS.INVARIANT": "FATAL",
    "TOS.RUN.UNSTARTABLE": "FATAL",
    "TOS.RUN.PROCESS_FAULT": "ERROR",
    "TOS.RUN.PROCESS_DEADLOCKED": "ERROR",
    "TOS.RUN.DEADLOCK": "ERROR",
    "TOS.RUN.BUNDLE.REFUSED": "ERROR",
    "TOS.RUN.PROCESS_REFUSED": "WARN",
    "TOS.RUN.BLOCK_CANCELLED": "WARN",
    "TOS.RUN.WAIT_CANCELLED": "WARN",
    "TOS.RUN.NOTICE_RELEASED": "WARN",
}
CLASSIFIED.update({identifier: "FATAL" for identifier in BOOT_FAILURES})

ORDER = ["DEBUG", "INFO", "WARN", "ERROR", "FATAL"]

# §9.3. A process's own record names its severity as its first dotted segment,
# because a process is the only thing that knows what its own decisions mean.
SAID = re.compile(r"^TOS\.RUN\.INTERFACE .*\bsaid=(\S+)")
IDENTIFIER = re.compile(r"^(TOS\.[A-Z0-9_.]+)(?:\s+(.*))?$")


def entry(line):
    """One transport line as (severity, producer, event, detail), or None."""
    line = line.strip()
    said = SAID.match(line)
    if said:
        record = said.group(1)
        parts = record.split(".")
        # A record whose first segment is not one of the five names is INFO:
        # the form is fixed, and a producer that does not use it has not
        # claimed a severity.
        severity = parts[0].upper() if parts[0].upper() in ORDER else "INFO"
        rest = parts[1:] if severity == parts[0].upper() else parts
        producer = rest[0] if rest else "process"
        return severity, producer, ".".join(rest[1:]) or record, ""
    found = IDENTIFIER.match(line)
    if not found:
        return None
    identifier, detail = found.group(1), found.group(2) or ""
    # Which component's statement it is, which an operator needs before the
    # event itself: a refusal asserted by the nucleus and one asserted by a
    # runtime are different facts about different things. Several `TOS.RUN.*`
    # events already carry `asserted_by=`, and where one does it is the answer —
    # a reader that guessed from the namespace would be overruling the emitter.
    asserted = re.search(r"\basserted_by=(\S+)", detail)
    if asserted:
        producer = asserted.group(1)
    elif identifier.startswith("TOS.BOOT."):
        producer = "loader"
    elif identifier.startswith("TOS.RUN."):
        producer = "runtime"
    else:
        producer = "nucleus"
    return CLASSIFIED.get(identifier, "INFO"), producer, identifier, detail


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", help="a captured diagnostic transport")
    parser.add_argument(
        "--severity",
        default="WARN",
        choices=ORDER,
        help="the lowest severity to show (default WARN: the important-error view)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit non-zero if anything at ERROR or above was produced",
    )
    arguments = parser.parse_args()

    floor = ORDER.index(arguments.severity)
    shown = 0
    worst = 0
    with open(arguments.log, "rb") as handle:
        text = handle.read().decode("utf-8", "replace").replace("\r", "")
    for line in text.splitlines():
        found = entry(line)
        if not found:
            continue
        severity, producer, event, detail = found
        worst = max(worst, ORDER.index(severity))
        if ORDER.index(severity) < floor:
            continue
        shown += 1
        print(f"{severity:<6} {producer:<10} {event:<34} {detail}".rstrip())
    if shown == 0:
        print(f"(nothing at {arguments.severity} or above)")
    if arguments.check and worst >= ORDER.index("ERROR"):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
