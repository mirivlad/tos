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

# **The device-protocol vocabulary no production binary may contain.**
#
# ADR-0082 §9: the nucleus may know an MSI-X table entry's layout and how to
# acknowledge through the local APIC; it may not know what a queue is or that
# this is a block device. Stage 4B and Stage 4C-1 each check that inline with a
# token list frozen before Stage 4D existed, so the completed block-protocol
# vocabulary — rings, indices, request types, sectors — was never guarded. This
# is that list, and the Stage 4D-5 gate runs it, so a leak makes the ordinary
# `qemu` profile red rather than being found by reading.
#
# **Device semantics, not English nouns.** Bare `queue`, `descriptor`,
# `capacity` and `status` describe generic nucleus, x86 and runtime machinery
# and are deliberately absent; every token here names something only a VirtIO
# block driver has a reason to write.
#
# Scanned with comments stripped, over the three source trees the boot actually
# runs: ring 0, the ring-3 runtime binary, and the engine that binary links.
STAGE4_DEVICE_VOCABULARY='virtio|virtqueue|virtq_desc|virtq_avail|virtq_used|virtio_blk|blk_req|blk_t_in|blk_t_out|blk_s_ok|avail_idx|used_idx|avail_ring|used_ring|used_elem|queue_notify|queue_enable|queue_desc|queue_driver|queue_device|queue_select|queue_msix|notify_off|desc_f_next|desc_f_write|sector|device_status|driver_ok|feature_select'

# **One narrow, documented exception.** PCI Express has its own Device Status
# register, and ADR-0084 §5b makes its Transactions Pending bit ring 0's
# business: it is how the nucleus proves a function has no non-posted request
# outstanding before that function's memory returns to the pool. The token is
# blanked rather than the line dropped, so a genuine leak sharing a line with it
# still matches.
stage4_device_vocabulary_leak() {
    local root="$1"
    find "$root/nucleus/src" "$root/runtime-image/src" "$root/crates/tos-engine/src" \
        -name '*.rs' -print0 |
        xargs -0 sed -e 's://.*::' -e 's:/\*.*\*/::' -e 's:EXPRESS_DEVICE_STATUS::g' |
        grep -niE "$STAGE4_DEVICE_VOCABULARY" || true
}

if [ "${0##*/}" = "stage4-profile.sh" ]; then
    echo "stage4 profile revision $STAGE4_PROFILE_REVISION"
    echo "  root port: $(stage4_port_fields)"
    echo "  endpoint:  $(stage4_target_fields)"
    echo "  claim:     pci_function_claim(bus, $(stage4_target_claim))"
    echo "  device vocabulary guarded: $STAGE4_DEVICE_VOCABULARY"
fi
