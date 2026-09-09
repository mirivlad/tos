# SPDX-License-Identifier: GPL-3.0-or-later
# The Stage 4 reference profile's PCI topology, in one place.
#
# **Sourced, never executed.** Every Stage 4 gate needs the same four numbers,
# and before profile revision 2 each of them wrote `bus=0 device=4 function=0`
# out by hand — in eight scripts, fourteen source fixtures, the nucleus's
# profile qualification and the accepted evidence. Moving the endpoint proved
# what that costs: one topology decision, twenty-odd literals, and no way to
# tell which of them were about *this* function and which about some other.
#
# So the profile decides here, and `check-stage4-target.sh` holds the copies
# that cannot be centralised — a TOS Core fixture has no include, so its BDF is
# written in the source and checked against this.
#
#   bash host-tools/qemu-test/stage4-profile.sh   # prints what it declares

# **Revision 2** (ADR-0084 revision 5, 2026-09-10). Revision 1 attached the
# endpoint directly to `pcie.0`, q35's root-complex bus, where it has no PCI
# Express Capability at all — so ADR-0084 §5c's P1 could not hold and no DMA
# authority could be granted. The endpoint now sits behind an explicit root
# port, and P1 is unchanged.
STAGE4_PROFILE_REVISION=2

# The root port. It keeps the slot the endpoint used to occupy, so the topology
# reads from the outside the way revision 1's did.
STAGE4_PORT_SEGMENT=0
STAGE4_PORT_BUS=0
STAGE4_PORT_DEVICE=4
STAGE4_PORT_FUNCTION=0

# **The target endpoint: the assigned block device**, and the function every
# accepted Stage 4 invariant is about — its vendor and class, its BARs, its
# VirtIO capabilities, its MSI-X, its bus-mastering precision, and its DMA
# qualification.
#
# **Measured, not predicted.** The bus is what the firmware assigned when the
# topology was first booted, read from the machine rather than assumed from how
# QEMU is expected to number things.
STAGE4_TARGET_SEGMENT=0
STAGE4_TARGET_BUS=1
STAGE4_TARGET_DEVICE=0
STAGE4_TARGET_FUNCTION=0

# The fragment a `TOS.RUN.*` line carries for the target function, so a gate
# asserts against the profile rather than against four numbers it typed.
stage4_target_fields() {
    printf 'segment=%s bus=%s device=%s function=%s' \
        "$STAGE4_TARGET_SEGMENT" "$STAGE4_TARGET_BUS" \
        "$STAGE4_TARGET_DEVICE" "$STAGE4_TARGET_FUNCTION"
}

# The same for the root port, for the one gate that is about the topology.
stage4_port_fields() {
    printf 'segment=%s bus=%s device=%s function=%s' \
        "$STAGE4_PORT_SEGMENT" "$STAGE4_PORT_BUS" \
        "$STAGE4_PORT_DEVICE" "$STAGE4_PORT_FUNCTION"
}

# The arguments a TOS Core fixture passes to `pci_function_claim`, which takes
# the bus, the device and the function and never the segment — a capability's
# segment is part of what it was granted rather than part of what it asks for.
stage4_target_claim() {
    printf '%su64, %su64, %su64' \
        "$STAGE4_TARGET_BUS" "$STAGE4_TARGET_DEVICE" "$STAGE4_TARGET_FUNCTION"
}

if [ "${0##*/}" = "stage4-profile.sh" ]; then
    echo "stage4 profile revision $STAGE4_PROFILE_REVISION"
    echo "  root port: $(stage4_port_fields)"
    echo "  endpoint:  $(stage4_target_fields)"
    echo "  claim:     pci_function_claim(bus, $(stage4_target_claim))"
fi
