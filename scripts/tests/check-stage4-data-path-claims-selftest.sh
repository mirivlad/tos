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
# Every file the gate reads, in the same shape. `$1` is the schema's parameter
# list for one IPC operation, `$2` the runtime image's region-transport code and
# `$3` the inventory's extra lines.
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
        operations: &[
            Operation {
                name: "endpoint_call_word",
                capabilities: &[Requirement::of("system.ipc.Endpoint", "call")],
                parameters: &[$1],
                result: "Result<system.ipc.Answer, i64>",
            },
        ],
    },
    Interface {
        path: "platform.dma.Region",
        object: ObjectKind::DmaRegion,
        operations: &[
            Operation {
                name: "dma_device_address",
                capabilities: &[Requirement::held("platform.dma.Region")],
                parameters: &[Parameter::fixed("size")],
                result: "Result<u64, i64>",
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
$2
EOF

    cat > "$REPO/scripts/preflight.sh" <<EOF
#!/bin/sh
stage4_data_path_claims() {
    bash "\$ROOT/scripts/tests/check-stage4-data-path-claims.sh"
}
$3
gate docs       default   "Stage 4 data-path claims"                   stage4_data_path_claims
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
PLAIN='Parameter::fixed("u64")'
REGION='Parameter::fixed("u64"), Parameter::fixed("Region<u8>")'
REGION_MUT='Parameter::fixed("u64"), Parameter::fixed("Region<mut u8>")'
DMA_REGION='Parameter::fixed("u64"), Parameter::fixed("DmaRegion<u8>")'

NO_BRIDGE=''
BRIDGE='
unsafe fn place_region(area: u64, index: usize, handle: u64) {
    unsafe { region_slot((area + tos_launch::MESSAGE_REGIONS) as usize).add(index).write(handle) };
}'
GATED_BRIDGE='
#[cfg(any(feature = "test-region-transport", feature = "test-build-topology"))]
unsafe fn set_region_handle(area: u64, index: usize, handle: u64) {
    unsafe { region_slot((area + tos_launch::MESSAGE_REGIONS) as usize).add(index).write(handle) };
}'
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
build "$PLAIN" "$NO_BRIDGE" "$NO_GATE"
cat > "$REPO/README.md" <<EOF
# readme

The client receives a number that originated in a device read. It does not read a
sector. The boundary is docs/research/$NOTE.
EOF
check || fail "an honest tree with no region path was rejected"

# --- 1. nothing at all: the bound holds ----------------------------------------
build "$PLAIN" "$NO_BRIDGE" "$NO_GATE"
check && fail "an overstated claim passed with no region path at all"

# --- 2. a schema declaration alone is not enough --------------------------------
# The defect this self-test was written for, in its first form.
build "$REGION" "$NO_BRIDGE" "$NO_GATE"
check && fail "a schema row with no implementation lifted the claim bound"

# --- 3. a runtime implementation alone is not enough ----------------------------
# And in its second form.
build "$PLAIN" "$BRIDGE" "$NO_GATE"
check && fail "a bridge implementation with no accepted row lifted the claim bound"

# --- 4. a conformance gate alone is not enough ----------------------------------
build "$PLAIN" "$NO_BRIDGE" "$CONFORMANCE_GATE"
check && fail "a conformance gate with neither a row nor an implementation lifted the bound"

# --- 5. any two of the three are not enough ------------------------------------
build "$REGION" "$BRIDGE" "$NO_GATE"
check && fail "a row and an implementation with no conformance gate lifted the bound"
build "$REGION" "$NO_BRIDGE" "$CONFORMANCE_GATE"
check && fail "a row and a gate with no implementation lifted the bound"
build "$PLAIN" "$BRIDGE" "$CONFORMANCE_GATE"
check && fail "an implementation and a gate with no accepted row lifted the bound"

# --- 6. a gate declared and not defined is not a gate ---------------------------
# `check-gate-parity.py` learned this the hard way: a declared-and-undefined gate
# keeps every count agreeing and proves nothing.
build "$REGION" "$BRIDGE" "$DECLARED_ONLY"
check && fail "a gate declared in the inventory and never defined lifted the bound"

# --- 7. DmaRegion is not the client's payload -----------------------------------
# ADR-0037: neither shareable nor transferable in either mode. It is what the
# service fills, and its presence says nothing about this boundary.
build "$DMA_REGION" "$BRIDGE" "$CONFORMANCE_GATE"
check && fail "a DmaRegion parameter lifted the claim bound"

# --- 8. nor is a writable region -------------------------------------------------
# `IPC_V1` §5: a writable region handle may not be delegated or sent at all.
build "$REGION_MUT" "$BRIDGE" "$CONFORMANCE_GATE"
check && fail "a Region<mut T> parameter lifted the claim bound"

# --- 9. nor is ordinary capability delegation ------------------------------------
# `Placed::Transfer` is `MESSAGE_CAPABILITIES` (ADR-0058, `IPC_V1` §6), a
# different area with a different count and a different bound from the region
# area. A capability crossing has been possible since ADR-0094 and is not a
# payload.
build "$REGION" "$DELEGATION_BRIDGE" "$CONFORMANCE_GATE"
check && fail "the capability transfer table was accepted as region payload transport"

# --- 10. nor a Rust evidence workload --------------------------------------------
# ADR-0094 §0 refuses a Rust runtime stage as a stand-in for canonical text, and
# every `MESSAGE_REGIONS` reference in the tree today is inside one.
build "$REGION" "$GATED_BRIDGE" "$CONFORMANCE_GATE"
check && fail "a feature-gated Rust workload was accepted as the canonical-text bridge"

# --- 11. and all three together lift it ------------------------------------------
# The one acceptance. The README still overstates the path and is no longer read,
# because by then the path is real.
build "$REGION" "$BRIDGE" "$CONFORMANCE_GATE"
check || fail "an accepted row, an ungated bridge path and a conformance gate did not lift the bound"

echo "check-stage4-data-path-claims self-test: PASS (ten refusals and two acceptances)"
