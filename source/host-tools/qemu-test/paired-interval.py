#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The timed interval of one paired-measurement sample.

Both series start at ``TOS.TEST.PAIRED.START`` and end at the event that closes
the logical workload each mode performs.

**One explicit boundary, not a boot milestone.** The old metric timed its
numerator from ``TOS.BOOT.ENTRY`` — the whole boot, including the UEFI loader's
own capsule hashing, performed by a *different* binary — and its denominator
from ``TOS.TEST.CRYPTO.BASELINE.START``, a sub-interval of the nucleus alone.
Those intervals neither began at the same instant nor covered the same
component.

``TOS.TEST.PAIRED.START`` is emitted at the same point in both modes, after an
identical untimed prefix that includes the common setup parse. So the ordinary
boot, the loader and the setup are outside both intervals rather than inside
one, and whatever they did to the emulator's translation and cache state is
common to both.

The clock is the existing host monotonic serial-byte arrival clock; this adds no
guest timing interface.
"""
import argparse
import json
import sys
from pathlib import Path

START = "TOS.NUCLEUS.ENTRY"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timestamps", required=True, type=Path)
    parser.add_argument("--end", required=True)
    args = parser.parse_args()

    start = end = None
    for line in args.timestamps.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        event = record.get("event")
        if event == START and start is None:
            start = record["monotonic_ns"]
        elif event == args.end and start is not None and end is None:
            end = record["monotonic_ns"]
    if start is None or end is None:
        print(f"missing boundary: start={start} end={end}", file=sys.stderr)
        return 1
    print(end - start)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
