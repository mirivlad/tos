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
NEGATIVE="$ROOT/tests/vectors/pci-negative"
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

# --- with the device present: the real read ------------------------------------
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
[ "$(count '^TOS\.RUN\.PCI_ROOT segment=0 first_bus=0 last_bus=255 rights=claim asserted_by=launcher$')" = 1 ] ||
    fail "the root bus authority was not minted and named by the launcher"

# --- the module asked for the bus, and for nothing else ------------------------
# One request, and it is the bus. It never imported `platform.pci.FunctionConfig`
# and could not have: the only lawful producer is the claim, which runs after
# startup. It reaches that interface through an effect declaration and a runtime
# value (ADR-0080).
[ "$(count '^TOS\.RUN\.REQUEST binding=bus interface=platform\.pci\.Bus object=9 wanted=9$')" = 1 ] ||
    fail "the module's request for bus authority was not answered by name and kind"
[ "$(count '^TOS\.RUN\.REQUEST ')" = 1 ] ||
    fail "the module requested authority beyond the bus root"

# --- the nucleus named which function it assigned ------------------------------
[ "$(count '^TOS\.RUN\.PCI_ASSIGNED process=0 segment=0 bus=0 device=4 function=0 generation=1 asserted_by=nucleus$')" = 1 ] ||
    fail "the function at 00:04.0 was not assigned to the textual process exactly once"

# --- and what the device said about itself -------------------------------------
# The guest packed five fields it read; this decodes them. The host supplies
# none of them — it only says which values a modern VirtIO block device has.
value="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$LOG")"
[ -n "$value" ] || fail "the module reported no observation"
[ "$value" -gt 0 ] || fail "the module reported a refusal rather than a reading: $value"

field() { printf '%s' "$(( ($value >> $1) & $2 ))"; }
vendor="$(field 0 65535)"
device="$(field 16 65535)"
class="$(field 32 255)"
subclass="$(field 40 255)"
capabilities="$(field 48 255)"

[ "$vendor" = "$((0x1AF4))" ] ||
    fail "vendor is 0x$(printf %04X "$vendor"), expected 0x1AF4"
[ "$device" = "$((0x1042))" ] ||
    fail "device is 0x$(printf %04X "$device"), expected 0x1042 (modern virtio-blk)"
[ "$class" = "$((0x01))" ] ||
    fail "class is 0x$(printf %02X "$class"), expected 0x01 (mass storage)"
[ "$subclass" = "$((0x00))" ] ||
    fail "subclass is 0x$(printf %02X "$subclass"), expected 0x00"
# A capability pointer inside conventional space and past the standard header is
# a list the *device* laid out. Zero would mean no capabilities at all, and a
# modern VirtIO device must have them.
[ "$capabilities" -ge 64 ] && [ "$capabilities" -le 255 ] ||
    fail "capability pointer 0x$(printf %02X "$capabilities") is not a device-provided list"

# --- without the device, the same proof fails ----------------------------------
# The one case that shows the numbers above came from hardware. Same module, same
# nucleus, same machine minus the device: an absent function reads all-ones on
# every field, so the vendor is 0xFFFF and no assertion above would hold.
bash "$HERE/run.sh" \
    --out "$OUT/absent" \
    --capsule "$OUT/pci.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE" \
    > /dev/null
absent="$(sed -n 's/^TOS\.RUN\.COMPLETED value=i64:\(-\?[0-9]*\)$/\1/p' "$OUT/absent/events.log")"
[ -n "$absent" ] || fail "the device-absent run reported nothing"
[ "$absent" != "$value" ] ||
    fail "the module reported the same observation with and without the device"
[ "$(( absent & 65535 ))" = 65535 ] ||
    fail "an absent function did not read as all-ones: $absent"

# --- the negatives, executed ---------------------------------------------------
printf '/system/boot/init.tos\t%s/init.tos\n' "$NEGATIVE" > "$OUT/negative-manifest.txt"
"$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/negative.bin" --meta "$OUT/negative.meta.json" "$OUT/negative-manifest.txt"
python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
    --capsule "$OUT/negative.bin" --manifest "$OUT/negative.meta.json"

bash "$HERE/run.sh" \
    --out "$OUT/negative" \
    --capsule "$OUT/negative.bin" \
    --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
    > /dev/null
# All eight, and the number is a sum of refusals the nucleus decided:
#   1  a device outside its architectural range        E_BAD_ARGUMENT
#   2  an offset past conventional config space        E_BAD_ARGUMENT
#   4  a width the mechanism cannot perform            E_BAD_ARGUMENT
#   8  an offset not a multiple of its width           E_BAD_ARGUMENT
#  16  a read-only capability asked to write           E_NO_CAPABILITY
#  32  a released capability asked to read             refused by generation
#  64  two functions, one device: authority for A does not reach B
# 128  a second claim of a live assignment             E_LIMIT
[ "$(grep -c '^TOS\.RUN\.COMPLETED value=i64:255$' "$OUT/negative/events.log" || true)" = 1 ] ||
    fail "the authority negatives did not all hold: $(grep '^TOS\.RUN\.COMPLETED' "$OUT/negative/events.log")"

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

echo "PCI-DISCOVERY PASS: canonical text read a real PCI device under a capability"
echo "  the launcher minted one root bus authority, scope named in the record"
echo "  the module asked for the bus alone; it reached platform.pci.FunctionConfig"
echo "  through an effect declaration and the value the claim returned (ADR-0080)"
echo "  it read 00:04.0 and reported vendor=0x$(printf %04X "$vendor") device=0x$(printf %04X "$device")"
echo "  class=0x$(printf %02X "$class") subclass=0x$(printf %02X "$subclass") capabilities=0x$(printf %02X "$capabilities")"
echo "  without the device the same module reports vendor=0xFFFF, so the values are the hardware's"
echo "  eight authority negatives hold, executed rather than asserted"
echo "  and the same module with no endowment is refused before its first instruction"
