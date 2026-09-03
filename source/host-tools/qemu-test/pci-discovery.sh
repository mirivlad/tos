#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stage 4A: a textual module takes authority over a real PCI function.
#
# `docs/37`'s Stage 4 identity question is whether "a canonical textual
# user-space driver actually move[s] persistent data through final-style
# MMIO/interrupt/DMA/IPC boundaries". This is the first step of the chain that
# ends there, and it is deliberately the whole of what this gate claims: a
# platform root exists, a canonical textual module holds it, and it takes
# exclusive assignments out of its scope.
#
# **A claim does not touch the device.** It is an authority operation over an
# *address*; whether anything is behind that address is a question only a
# configuration read can ask, and no module can ask it yet (below). So this gate
# proves an authority boundary and not a hardware-facing act, and it would pass
# unchanged on a machine with no VirtIO device at all.
#
# **What this gate does not claim** is a configuration read from text. The
# textual side of that boundary is blocked on ADR-0078 §6 — see
# `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md` §7 — and a gate that asserted it
# would be asserting something no module can currently express.
#
# The evidence is a number no module contains. Four findings, two of which
# require a refusal the nucleus decides against the assignment table and one of
# which requires a device that is actually there:
#
#   1  a function was claimed out of the bus capability's scope
#   2  claiming it again was refused as E_LIMIT — the assignment is exclusive
#   4  device 32 was refused as E_BAD_ARGUMENT — outside the architectural range
#   8  a different function was claimed, so 2 was about the function
#
#   bash host-tools/qemu-test/pci-discovery.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-pci-discovery}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/pci"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-pci-discovery"

# All four findings.
EXPECTED_VALUE="i64:15"

fail() {
    echo "pci-discovery: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || {
    echo "missing production nucleus: $PRODUCTION" >&2
    exit 2
}
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-pci-discovery)
after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
[ "$before" = "$after" ] || {
    echo "production nucleus changed while building the isolated test artifact" >&2
    exit 1
}

printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE" > "$OUT/manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/pci.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/pci.bin" --manifest "$OUT/capsule.meta.json"

# --- with the device present --------------------------------------------------
bash "$HERE/run.sh" \
    --out "$OUT" \
    --capsule "$OUT/pci.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.REQUEST TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null

LOG="$OUT/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- the root is a mint, and it is named ---------------------------------------
# `CAPABILITY_V1` §2 requires a platform root's scope and identity to be nameable
# rather than assumed. A root nobody stated is a root nobody decided.
[ "$(count '^TOS\.RUN\.PCI_ROOT segment=0 first_bus=0 last_bus=255 rights=claim asserted_by=launcher$')" = 1 ] ||
    fail "the root bus authority was not minted and named by the launcher"

# --- the module asked for it by name and kind ----------------------------------
[ "$(count '^TOS\.RUN\.REQUEST binding=bus interface=platform\.pci\.Bus object=9 wanted=9$')" = 1 ] ||
    fail "the module's request for bus authority was not answered by name and kind"

# --- two assignments were made, and the nucleus named which functions -----------
# The BDF is the nucleus's, from the object it created: a module cannot put one
# in this record, and could not name a different function if it tried.
[ "$(count '^TOS\.RUN\.PCI_ASSIGNED process=0 segment=0 bus=0 device=4 function=0 generation=1 asserted_by=nucleus$')" = 1 ] ||
    fail "the function at 00:04.0 was not assigned to the textual process exactly once"
[ "$(count '^TOS\.RUN\.PCI_ASSIGNED process=0 segment=0 bus=0 device=5 function=0 ')" = 1 ] ||
    fail "the second, different function was not assigned"
[ "$(count '^TOS\.RUN\.PCI_ASSIGNED ')" = 2 ] ||
    fail "an assignment was made that the module did not obtain, or one was made twice"

# --- and the module's own findings ---------------------------------------------
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_VALUE\$")" = 1 ] ||
    fail "the module did not report all four findings (expected $EXPECTED_VALUE)"

# --- a process with no PCI authority cannot claim anything ---------------------
# The same module and the same machine, under the canonical launcher constant
# that endows nothing. Its request is unanswered, so it never reaches a call:
# `CapabilityDenied` before the first instruction, which is stronger than a
# refused call — there is no authority to refuse.
bash "$HERE/run.sh" \
    --out "$OUT/denied" \
    --capsule "$OUT/pci.bin" \
    --nucleus "$PRODUCTION" \
    --stage4-block-device \
    --expect 75 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REFUSED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.PCI_ASSIGNED TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED" \
    > /dev/null
grep -q 'capability-denied' "$OUT/denied/events.log" ||
    fail "a module with no PCI endowment was not refused at startup"
[ "$(grep -c '^TOS\.RUN\.PCI_ASSIGNED ' "$OUT/denied/events.log" || true)" = 0 ] ||
    fail "a process without bus authority reached an assignment"

echo "PCI-DISCOVERY PASS: a textual module holds a platform root and claims out of it"
echo "  the launcher minted one root bus authority, scope named in the record"
echo "  the module asked for it by name; the nucleus assigned 00:04.0 and 00:05.0 to it"
echo "  it reported $EXPECTED_VALUE — claim, exclusive refusal, range refusal, second claim"
echo "  the same module with no endowment is refused before its first instruction"
echo "  what is NOT proved here: that any device is present, and any configuration"
echo "  read from text. A claim is authority over an address and touches no hardware;"
echo "  see docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md §7 for the blocker"
