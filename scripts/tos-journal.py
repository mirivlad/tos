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


# The Stage 3 supervision story, as labels over the same transport.
#
# **Not a second vocabulary.** Every key below is an identifier or a record the
# accepted contracts already define; this maps them to words a person reads
# quickly. Anything not listed simply does not appear in the story view, which
# is what makes it a story rather than a second log.
STORY_RECORDS = {
    "policy.start-permitted": ("start", "policy permits starting"),
    "action.create": ("create", "creating a process"),
    "result.created": ("created", "process created"),
    "result.refused": ("refused", "creation refused"),
    "policy.dependency-unavailable": ("blocked", "a dependency is not running"),
    "state.blocked": ("blocked", "BLOCKED"),
    "observed.ending": ("ended", "an ending was observed"),
    "observed.no-ending": ("idle", "nothing left to wait for"),
    "inferred.own-failure": ("failure", "the service itself failed"),
    "policy.restart-permitted": ("restart", "inside the window, restart permitted"),
    "policy.budget-exhausted": ("failed", "restart budget exhausted"),
    "state.failed": ("failed", "FAILED, and it latches"),
    "policy.latched-no-start": ("latched", "not started: already FAILED"),
    "report": ("report", "the supervisor is done"),
}

STORY_EVENTS = {
    "TOS.RUN.PROCESS_ENDOWED": ("process", "a process was endowed and started"),
    "TOS.RUN.PROCESS_EXIT": ("exit", "a process reached its own end"),
    "TOS.RUN.PROCESS_TERMINATED": ("exit", "a process was ended by authority"),
    "TOS.RUN.PROCESS_RECLAIMED": ("reclaim", "its memory came back"),
    "TOS.RUN.PROCESS_REFUSED": ("refused", "the nucleus refused a creation"),
    "TOS.RUN.VERIFIED": ("verify", "a module verified itself before running"),
    "TOS.RUN.COMPLETED": ("done", "an entry returned"),
    "TOS.RUN.REQUEST": ("granted", "a capability request was answered"),
    "TOS.RUN.BEGIN": ("capsule", "the source set this boot runs"),
}

# Which fields of an event are worth showing beside its label.
STORY_FIELDS = {
    "TOS.RUN.PROCESS_ENDOWED": ("process", "capabilities"),
    "TOS.RUN.PROCESS_EXIT": ("process", "self_reported_status"),
    "TOS.RUN.PROCESS_TERMINATED": ("process", "by"),
    "TOS.RUN.PROCESS_RECLAIMED": ("process", "frames"),
    "TOS.RUN.PROCESS_REFUSED": ("reason",),
    "TOS.RUN.VERIFIED": ("module",),
    "TOS.RUN.COMPLETED": ("value",),
    "TOS.RUN.REQUEST": ("binding", "interface"),
    "TOS.RUN.BEGIN": ("path", "modules"),
}


def fields_of(detail):
    return dict(re.findall(r"(\w+)=(\S+)", detail))


def story(lines):
    """The supervision narrative, as (label, subject, note) in transport order."""
    told = []
    # Where the most recent *process-written* record landed. A module path
    # follows its decision in the supervisor's own record stream, but the
    # transport interleaves that stream with the nucleus's — a report region is
    # drained at system calls, so an event from ring 0 can arrive between a
    # decision and the service it was about. Attaching the path to whatever came
    # last would put it on that event instead.
    last_record = None
    for line in lines:
        # A record naming a module path is read before `entry` sees it, because
        # a path is not a dotted severity form and splitting it as one would
        # take it apart.
        said = SAID.match(line.strip())
        if said and "/" in said.group(1):
            if last_record is not None and not told[last_record][1]:
                told[last_record][1] = said.group(1)
            continue
        found = entry(line)
        if not found:
            continue
        severity, producer, event, detail = found
        # A process's own record: `<severity>.<producer>.<kind>.<what>`, of
        # which `entry` has already stripped the first two segments.
        if producer not in ("nucleus", "runtime", "loader"):
            if event in STORY_RECORDS:
                label, note = STORY_RECORDS[event]
                told.append([label, "", note])
                last_record = len(told) - 1
            continue
        if event in STORY_EVENTS:
            label, note = STORY_EVENTS[event]
            values = fields_of(detail)
            subject = " ".join(
                f"{name}={values[name]}"
                for name in STORY_FIELDS.get(event, ())
                if name in values
            )
            told.append([label, subject, note])
    return told


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
        "--story",
        action="store_true",
        help="the Stage 3 supervision narrative instead of the severity view",
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
    if arguments.story:
        for label, subject, note in story(text.splitlines()):
            print(f"  [{label:<8}] {subject:<34} {note}".rstrip())
        return 0
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
