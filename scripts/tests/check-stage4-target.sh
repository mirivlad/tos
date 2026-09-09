#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# One Stage 4 reference profile, and every copy of its target function held to it.
#
# The profile decides which PCI function the Stage 4 invariants are about, and
# it decides it in `host-tools/qemu-test/stage4-profile.sh`. Three kinds of
# consumer need the same four numbers and cannot all read them from there:
#
#   the harness      sources the profile, so it needs no checking
#   the nucleus      a `cfg`-gated constant beside the qualification it feeds
#   TOS Core         a fixture writes the BDF in its source; the language has
#                    no include, so the literal is the only form available
#
# **This exists because the copies were found the hard way.** Profile revision 2
# moved the endpoint behind a PCIe root port, and the one topology decision
# turned out to be written out by hand in the nucleus, in fourteen fixtures and
# in eight gates — with nothing saying which of those numbers were about the
# target function and which about something else. A gate is what makes the next
# move one decision instead of an archaeology exercise.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PROFILE="$ROOT/source/host-tools/qemu-test/stage4-profile.sh"
NUCLEUS="$ROOT/source/nucleus/src/pci.rs"

fail() {
    echo "check-stage4-target: FAIL: $*" >&2
    exit 1
}

[ -f "$PROFILE" ] || fail "the Stage 4 profile declares nothing"
# shellcheck source=/dev/null
. "$PROFILE"

# --- the nucleus's own constant ------------------------------------------------
declared=$(sed -n 's/^pub const STAGE4_TARGET: (u16, u8, u8, u8) = (\(.*\));$/\1/p' "$NUCLEUS" |
    tr -d ' ')
[ -n "$declared" ] || fail "the nucleus declares no Stage 4 target constant"
wanted="$STAGE4_TARGET_SEGMENT,$STAGE4_TARGET_BUS,$STAGE4_TARGET_DEVICE,$STAGE4_TARGET_FUNCTION"
[ "$declared" = "$wanted" ] ||
    fail "the nucleus qualifies ($declared) and the profile targets ($wanted)"

# --- every fixture that claims a function --------------------------------------
#
# A TOS Core fixture takes the bus, the device and the function; the segment is
# part of what the bus capability *was granted* rather than part of what it asks
# for, which is why no fixture names one.
claim="$(stage4_target_claim)"
# What profile revision 1 targeted. This is the hazard the gate is for: a
# fixture left behind by the move looks exactly like one that always meant some
# other function, and only the number tells them apart.
stale_claim="${STAGE4_PORT_BUS}u64, ${STAGE4_PORT_DEVICE}u64, ${STAGE4_PORT_FUNCTION}u64"
stale_spelled=$(printf '%02x:%02x.%x' \
    "$STAGE4_PORT_BUS" "$STAGE4_PORT_DEVICE" "$STAGE4_PORT_FUNCTION")
targeted=0
while IFS= read -r fixture; do
    while IFS= read -r written; do
        [ -n "$written" ] || continue
        if [ "$written" = "$claim" ]; then
            targeted=$((targeted + 1))
            continue
        fi
        [ "$written" = "$stale_claim" ] || continue
        # **The one address that must be argued for.** Since revision 2 it is
        # the root port rather than the endpoint, so a fixture claiming it is
        # either about the topology — and says so, by naming it — or is a
        # fixture the move forgot.
        grep -Fq "$stale_spelled" "$fixture" || {
            echo "$fixture claims: $written" >&2
            echo "the profile targets: $claim" >&2
            fail "a fixture claims $stale_spelled, which profile revision 2 makes the root
       port rather than the endpoint. If that is deliberate, name
       $stale_spelled in the fixture's own text; if it is not, it is a claim
       revision 2 left behind."
        }
    done < <(sed -n 's/.*pci_function_claim([a-z]*, \([0-9]*u64, [0-9]*u64, [0-9]*u64\)).*/\1/p' \
        "$fixture")
done < <(grep -rl "pci_function_claim(" "$ROOT/source/tests/vectors" --include=init.tos)

[ "$targeted" -gt 0 ] || fail "no fixture claims the Stage 4 target function at all"

# --- and no gate writes the old location out by hand ---------------------------
#
# Scoped to the Stage 4 gates, and to the *event* fields rather than to any
# occurrence of the digits: `addr=0x4` is the root port's slot and is the
# profile's own topology, while `bus=0 device=4` inside a `TOS.RUN.PCI_*`
# assertion is a claim about the target function that the profile no longer
# makes.
stale=$(grep -rn "TOS\\\\\?\.RUN\\\\\?\.PCI[A-Z_]* .*bus=$STAGE4_PORT_BUS device=$STAGE4_PORT_DEVICE function=0" \
    "$ROOT/source/host-tools/qemu-test" --include='*.sh' || true)
[ -z "$stale" ] || {
    echo "$stale" >&2
    fail "a Stage 4 gate asserts the target function is where profile revision 1 put it"
}

echo "check-stage4-target: PASS (profile revision $STAGE4_PROFILE_REVISION," \
     "$(stage4_target_fields), $targeted fixture claim(s) of it, and the nucleus agree)"
