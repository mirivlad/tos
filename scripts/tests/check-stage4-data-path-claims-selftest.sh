#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the Stage 4 data-path claim gate.
#
# **It exists because the gate's first version was wrong in a way only a truth
# table catches.** That version ORed its two conditions while its own comment
# claimed they were two independent proofs of one fact, so a schema declaration
# with no implementation — or an implementation with nothing accepted behind it —
# lifted the claim bound. It also counted `DmaRegion` and the *capability*
# transfer table, neither of which is the region payload transport `IPC_V1` §5
# and ADR-0037 describe. A gate that is only ever green is a gate nobody has
# tested.
#
# Each case builds a miniature repository with the shape the gate reads and whose
# README **does** carry the forbidden wording. That is the discriminator: while
# the bound holds the gate must go red, and it may only go green by lifting the
# bound. So "the bound held" and "the bound lifted" are distinguishable without
# the test having to read the gate's own message.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GATE="$ROOT/scripts/tests/check-stage4-data-path-claims.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

REPO="$TMP/repo"
NOTE="STAGE4_DATA_PATH_BOUNDARY.md"

fail() {
    echo "check-stage4-data-path-claims self-test: FAIL: $*" >&2
    exit 1
}

# --- the miniature repository ---------------------------------------------------
#
# Every file the gate reads, in the same shape. `$1` is the extra capability
# requirement of one `system.ipc.*` operation, `$2` the representation
# `system.memory.Region` declares, `$3` the runtime image's region-transport code,
# `$4` the inventory's extra lines and `$5` how the inventory describes
# `qemu_block_service` — the fragment the gate reads structurally rather than by
# grepping a file that documents a hundred gates.
#
# **The schema is written as whole interface blocks**, because the detector reads
# which interface carries the ordinary-region family and then which `system.ipc.*`
# operation requires one. A synthetic parameter list would be testing a shape the
# accepted surface does not have.
build() {
    rm -rf "$REPO"
    mkdir -p "$REPO/scripts" "$REPO/source/crates/tos-core/src" \
        "$REPO/source/runtime-image/src" "$REPO/source/host-tools/qemu-test" \
        "$REPO/source/tests/vectors/block-service"

    cat > "$REPO/source/crates/tos-core/src/interfaces.rs" <<EOF
pub const ACCEPTED: &[Interface] = &[
    Interface {
        path: "system.ipc.Endpoint",
        object: ObjectKind::Endpoint,
        representation: Representation::AsInterface,
        operations: &[
            Operation {
                name: "endpoint_send_something",
                capabilities: &[
                    Requirement::of("system.ipc.Endpoint", "send"),
$1
                ],
                parameters: &[],
                result: "i64",
            },
        ],
    },
    Interface {
        path: "platform.dma.Region",
        object: ObjectKind::DmaRegion,
        representation: Representation::DmaRegionFamily,
        operations: &[
            Operation {
                name: "dma_device_address",
                capabilities: &[Requirement::held("platform.dma.Region")],
                parameters: &[Parameter::fixed("size")],
                result: "Result<u64, i64>",
            },
        ],
    },
    Interface {
        path: "system.memory.Region",
        object: ObjectKind::Region,
        representation: Representation::$2,
        operations: &[
            Operation {
                name: "region_freeze",
                capabilities: &[Requirement::of("system.memory.Region", "write")],
                parameters: &[],
                result: "Result<Region<u8>, i64>",
            },
        ],
    },
];
EOF
    cat > "$REPO/source/runtime-image/src/main.rs" <<EOF
const PERFORMED: &[Performed] = &[
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endpoint_call_word",
        capabilities: &[Placed::Register(Reg::Rdi)],
    },
];

unsafe fn transferred(region: u64, index: usize) -> u64 {
    unsafe { word_at((region + tos_launch::MESSAGE_CAPABILITIES) as usize) }
}
$3
EOF

    cat > "$REPO/scripts/preflight.sh" <<EOF
#!/bin/sh
stage4_data_path_claims() {
    bash "\$ROOT/scripts/tests/check-stage4-data-path-claims.sh"
}
$4
${5:-$HONEST_DESCRIPTION}
qemu_block_service() {
    bash "\$ROOT/source/host-tools/qemu-test/block-service.sh"
}
# An unrelated gate whose own claim is legitimately end to end. It is present in
# every case, so no case can pass by the scoping being too wide.
qemu_something_else() {
    true
}
gate docs       default   "Stage 4 data-path claims"                   stage4_data_path_claims
gate qemu       full-only "QEMU a device answer reaches a bare client"  qemu_block_service
gate qemu       full-only "QEMU some other thing, end to end"           qemu_something_else
EOF

    # The present-tense section the gate reads out of the journal, and a README
    # that overstates the path. Both cite the note, so citation is never what
    # decides a case below.
    cat > "$REPO/PROGRESS.md" <<EOF
# journal

### Состояние на 2026-09-23 — настоящее время, по категориям

Клиент не читает сектор: см. docs/research/$NOTE.

## Checklist Stage 1
EOF
    cat > "$REPO/README.md" <<EOF
# readme

The client reads a real sector through the service, end to end.
The boundary is docs/research/$NOTE.
EOF
    for slice in "$REPO/source/host-tools/qemu-test/block-service.sh" \
        "$REPO/source/tests/vectors/block-service/client.tos" \
        "$REPO/source/tests/vectors/block-service/service.tos"; do
        printf '# a slice file citing docs/research/%s\n' "$NOTE" > "$slice"
    done
}

check() { bash "$GATE" --root "$REPO" >/dev/null 2>&1; }

# The three conditions, as the pieces a case switches on.
#
# An IPC operation's extra capability requirement, and the family
# `system.memory.Region` declares. Together they decide whether the schema carries
# an ordinary region across a message.
NO_EXTRA=''
NEEDS_ORDINARY='                    Requirement::held("system.memory.Region"),'
NEEDS_DMA='                    Requirement::held("platform.dma.Region"),'
ORDINARY_FAMILY='RegionFamily'
# A family this gate does not know: the detector reads the frontend's table rather
# than assuming a name, so a renamed member must read as absent rather than as
# present.
OTHER_FAMILY='SomeLaterFamily'

# How the inventory describes `qemu_block_service`, in the shapes that matter:
# what it says today, the sentence the 2026-09-23 round left behind, the same claim
# with an unrelated denial after it, and one that claims nothing improper and cites
# no boundary.
HONEST_DESCRIPTION="# A client holding no part of the machine is answered by a service that read the
# device. See docs/research/$NOTE."
STALE_DESCRIPTION="# The Stage 4 client/service data path end to end: a separate textual client,
# IPC, a block service, DMA, VirtIO and an answer. See docs/research/$NOTE."
DENIAL_ON_THE_SAME_LINE="# The Stage 4 client/service data path end to end. The 512 bytes do not cross
# IPC. See docs/research/$NOTE."
UNCITED_DESCRIPTION="# A client holding no part of the machine is answered by a service."

# The runtime image's side: whether `MESSAGE_REGIONS` is reached from
# always-compiled code.
NO_BRIDGE=''
BRIDGE='
unsafe fn place_region(area: u64, index: usize, handle: u64) {
    unsafe { region_slot((area + tos_launch::MESSAGE_REGIONS) as usize).add(index).write(handle) };
}'
# The same reference behind a feature gate, which is a Rust evidence workload and
# not the typed bridge — ADR-0094 §0 refuses one as a stand-in for canonical text.
GATED_BRIDGE='
#[cfg(any(feature = "test-region-transport", feature = "test-build-topology"))]
unsafe fn set_region_handle(area: u64, index: usize, handle: u64) {
    unsafe { region_slot((area + tos_launch::MESSAGE_REGIONS) as usize).add(index).write(handle) };
}'
# And the *capability* transfer table, which is a different area with a different
# count and a different bound. A capability crossing has been possible since
# ADR-0094 and is not a payload crossing.
DELEGATION_BRIDGE='
const CARRYING: &[Performed] = &[
    Performed {
        interface: "system.ipc.Endpoint",
        name: "endpoint_call_carrying",
        capabilities: &[Placed::Transfer(0), Placed::Register(Reg::Rdi)],
    },
];'

NO_GATE=''
CONFORMANCE_GATE='region_ipc_payload() {
    bash "$ROOT/scripts/tests/check-region-ipc-payload.sh"
}
gate qemu       full-only "QEMU a region payload crosses IPC"         region_ipc_payload'
DECLARED_ONLY='gate qemu       full-only "QEMU a region payload crosses IPC"         region_ipc_payload'

# --- 0. the bound holds, and an honest README passes ----------------------------
# First that the gate is not simply always red: with nothing overstated it is
# green while all three conditions are false.
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check || fail "an honest tree with no region path was rejected"

# --- 1. nothing at all: the bound holds ----------------------------------------
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE"
check && fail "an overstated claim passed with no region path at all"

# --- 2. a schema declaration alone is not enough --------------------------------
# The defect this self-test was written for, in its first form.
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE"
check && fail "a schema row with no implementation lifted the claim bound"

# --- 3. a runtime implementation alone is not enough ----------------------------
# And in its second form.
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$BRIDGE" "$NO_GATE"
check && fail "a bridge implementation with no accepted row lifted the claim bound"

# --- 4. a conformance gate alone is not enough ----------------------------------
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$CONFORMANCE_GATE"
check && fail "a conformance gate with neither a row nor an implementation lifted the bound"

# --- 5. any two of the three are not enough ------------------------------------
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$BRIDGE" "$NO_GATE"
check && fail "a row and an implementation with no conformance gate lifted the bound"
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$CONFORMANCE_GATE"
check && fail "a row and a gate with no implementation lifted the bound"
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$BRIDGE" "$CONFORMANCE_GATE"
check && fail "an implementation and a gate with no accepted row lifted the bound"

# --- 6. a gate declared and not defined is not a gate ---------------------------
# `check-gate-parity.py` learned this the hard way: a declared-and-undefined gate
# keeps every count agreeing and proves nothing.
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$BRIDGE" "$DECLARED_ONLY"
check && fail "a gate declared in the inventory and never defined lifted the bound"

# --- 7. DmaRegion is not the client's payload -----------------------------------
# ADR-0037: neither shareable nor transferable in either mode. It is what the
# service fills, and its presence says nothing about this boundary.
build "$NEEDS_DMA" "$ORDINARY_FAMILY" "$BRIDGE" "$CONFORMANCE_GATE"
check && fail "a DMA region at an IPC capability position lifted the claim bound"

# --- 8. nor a family this gate was told about rather than read -------------------
# The detector reads which interface carries the ordinary-region family out of the
# frontend's own table. A member renamed there, and nowhere else, must read as
# absent — otherwise the gate would be trusting a name it hard-coded.
build "$NEEDS_ORDINARY" "$OTHER_FAMILY" "$BRIDGE" "$CONFORMANCE_GATE"
check && fail "a representation family the frontend does not declare lifted the claim bound"

# --- 9. nor is ordinary capability delegation ------------------------------------
# `Placed::Transfer` is `MESSAGE_CAPABILITIES` (ADR-0058, `IPC_V1` §6), a
# different area with a different count and a different bound from the region
# area. A capability crossing has been possible since ADR-0094 and is not a
# payload.
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$DELEGATION_BRIDGE" "$CONFORMANCE_GATE"
check && fail "the capability transfer table was accepted as region payload transport"

# --- 10. nor a Rust evidence workload --------------------------------------------
# ADR-0094 §0 refuses a Rust runtime stage as a stand-in for canonical text, and
# every `MESSAGE_REGIONS` reference in the tree today is inside one.
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$GATED_BRIDGE" "$CONFORMANCE_GATE"
check && fail "a feature-gated Rust workload was accepted as the canonical-text bridge"

# --- 11. the inventory's own description of this gate is in scope -----------------
# **The last place the stale claim survived the 2026-09-23 round.** `preflight.sh`
# documents a hundred gates, so the fragment is extracted structurally: the
# comment block above `qemu_block_service() {` and the row naming it. Putting the
# old sentence back there must redden the gate, with everything else honest.
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE" "$STALE_DESCRIPTION"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check && fail "the stale end-to-end claim over qemu_block_service was accepted"

# --- and a denial elsewhere in the sentence's line does not excuse it -------------
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE" "$DENIAL_ON_THE_SAME_LINE"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check && fail "an affirmative claim followed by an unrelated denial was accepted"

# --- and the scoping is minimal, which is the other half of that ------------------
# An unrelated gate in the same file whose claim is legitimately end to end is
# present in every case above and in this one, and an honest description of
# `qemu_block_service` must still pass. A checker that grepped `preflight.sh`
# would fail here, which is exactly why it does not.
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE" "$HONEST_DESCRIPTION"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check || fail "a legitimate end-to-end claim over another gate was treated as this one's"

# --- and the description must cite the boundary ----------------------------------
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE" "$UNCITED_DESCRIPTION"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check && fail "a gate description citing no boundary note was accepted"

# --- and a description that cannot be found at all is a failure ------------------
# Not a pass: a claim nobody can locate is a claim nobody is checking.
build "$NO_EXTRA" "$ORDINARY_FAMILY" "$NO_BRIDGE" "$NO_GATE" "$HONEST_DESCRIPTION"
sed -i '/^qemu_block_service() {$/,/^}$/d' "$REPO/scripts/preflight.sh"
sed -i '/ qemu_block_service$/d' "$REPO/scripts/preflight.sh"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check && fail "an inventory with no description of the gate at all was accepted"

# --- 12. and all three together lift it ------------------------------------------
# The one acceptance. The README still overstates the path and is no longer read,
# because by then the path is real.
build "$NEEDS_ORDINARY" "$ORDINARY_FAMILY" "$BRIDGE" "$CONFORMANCE_GATE"
check || fail "an accepted row, an ungated bridge path and a conformance gate did not lift the bound"

echo "check-stage4-data-path-claims self-test: PASS (fourteen refusals and three acceptances)"
