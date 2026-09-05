#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The timed interval of one paired-measurement sample.

Both series start at ``TOS.NUCLEUS.ENTRY`` and end at the event that closes the
work each mode performs.

**The start event is the repair's second half.** The old metric timed its
numerator from ``TOS.BOOT.ENTRY`` — the whole boot, including the UEFI loader's
own capsule hashing, performed by a *different* binary — and its denominator
from ``TOS.TEST.CRYPTO.BASELINE.START``, a sub-interval of the nucleus alone.
Those two intervals do not begin at the same instant and do not cover the same
component, so their quotient was not a ratio of two comparable quantities even
before layout sensitivity was considered.

Starting both at the nucleus's own entry keeps the measured component the same
one whose two modes are being compared, and keeps the loader — which this metric
cannot vary and does not link — out of both sides.

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
