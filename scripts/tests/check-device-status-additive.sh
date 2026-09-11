#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A VirtIO driver adds status bits and never replaces the byte (§2.1.1).
#
#   "The driver MUST update device status, setting bits to indicate the
#    completed steps of the driver initialization sequence specified in 3.1.
#    The driver MUST NOT clear a device status bit."
#     — Virtual I/O Device (VIRTIO) Version 1.4, Committee Specification 01,
#       8 April 2026, §2.1.1
#
# **This is not a style rule.** The byte carries bits the *device* sets —
# `DEVICE_NEEDS_RESET` among them (§2.1.2) — so a driver that rebuilds it from
# the sequence it believes it performed erases a bit it never wrote and then
# goes on relying on a device that asked to be reset. Every canonical fixture in
# this tree had that defect until it was found, and a comment saying so is not
# what stops it coming back.
#
# **The check is structural rather than a spelling.** It does not look for a
# helper by name. It reads every write to the VirtIO `DEVICE_STATUS` register in
# every fixture that declares one, and requires each written value to be either
#
#   the literal `0u64`                      — explicit reset, the one legitimate
#                                             non-additive write (§2.1.1)
#   an expression containing `|` whose      — a read-modify-write of what the
#   function also reads DEVICE_STATUS         device currently reports
#
# and refuses any value naming a `STATUS_` constant directly, which is what a
# replacement write looks like in every form it has taken here — a bare bit, or
# a `+`-sum of the bits the driver expects to have set.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

fail() {
    echo "check-device-status-additive: FAIL: $*" >&2
    exit 1
}

mapfile -t FIXTURES < <(grep -rl '^const DEVICE_STATUS' "$ROOT/source/tests/vectors" \
    --include=init.tos | sort)
[ "${#FIXTURES[@]}" -gt 0 ] ||
    fail "no fixture declares a VirtIO DEVICE_STATUS register at all"

python3 - "${FIXTURES[@]}" <<'PY'
import pathlib
import re
import sys

WRITE = re.compile(
    r"mmio_write_\w+\s*\(\s*window\s*,\s*DEVICE_STATUS\s*,\s*(.*?)\)\s*;",
    re.S,
)
FUNCTION = re.compile(r"^fn |^pub fn ", re.M)

problems = []
checked = 0
resets = 0
additive = 0

for path in sys.argv[1:]:
    text = pathlib.Path(path).read_text()
    # Comments are stripped first: a comment quoting the rule documents the
    # boundary rather than crossing it.
    body = re.sub(r"//[^\n]*", "", text)
    # Function bodies, so "the same function reads the register" is a fact about
    # the code rather than about the file.
    bounds = [m.start() for m in FUNCTION.finditer(body)] + [len(body)]
    for write in WRITE.finditer(body):
        checked += 1
        value = " ".join(write.group(1).split())
        at = write.start()
        start = max(b for b in bounds if b <= at)
        end = min(b for b in bounds if b > at)
        enclosing = body[start:end]
        where = f"{pathlib.Path(path).parent.name}: mmio_write(DEVICE_STATUS, {value})"
        if value == "0u64":
            resets += 1
            continue
        if "STATUS_" in value:
            problems.append(
                f"{where} names a status constant directly, which replaces the"
                " byte instead of adding to it (VIRTIO 1.4 §2.1.1)"
            )
            continue
        if "|" not in value:
            problems.append(f"{where} is neither an explicit reset nor an OR of the current status")
            continue
        if "mmio_read_u8(window, DEVICE_STATUS)" not in enclosing:
            problems.append(
                f"{where} ORs something the enclosing function never read from"
                " DEVICE_STATUS, so it is not a read-modify-write"
            )
            continue
        additive += 1

if problems:
    for problem in problems:
        print(f"check-device-status-additive: FAIL: {problem}", file=sys.stderr)
    raise SystemExit(1)

print(
    f"check-device-status-additive: OK ({checked} DEVICE_STATUS write(s) in"
    f" {len(sys.argv) - 1} fixture(s): {resets} explicit reset(s), {additive} additive)"
)
PY
