#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# One-command local repository preflight. This script orchestrates the existing
# authoritative gates; it does not reimplement their checks.
set -u

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
MODE=default
PROFILE=
LIST=0

usage() {
    cat <<'EOF'
Usage: ./scripts/preflight.sh [--full | --profile NAME | --list]

  (no option)      every gate of local scope `default`
  --full           the whole inventory, every profile and both scopes
  --profile NAME   every gate of that profile, whatever its local scope
  --list           print the inventory and run nothing

The inventory below is the single declaration of what a gate is: its profile,
its local scope, its label and the function that proves it. `--list` is the only
source of that composition, and CI names **profiles** rather than gates so that
no second list of gates can exist to drift from this one (ADR-0065).

  profile  the environment class a gate needs, and the unit a CI job runs:
           docs (text only), provenance (full git history), source (the Rust
           toolchain), qemu (firmware and an emulator), selftest (fixtures —
           these gates test the gates rather than the repository).
  scope    `default` runs in a bare preflight; `full-only` needs --full.

Every gate runs even if an earlier gate fails. The final status is PASS only
when every selected authoritative command succeeds.
EOF
}

while [ "$#" -gt 0 ]; do
    case $1 in
        --full) MODE=full; shift ;;
        --list) LIST=1; shift ;;
        --profile)
            [ "$#" -ge 2 ] || { echo "--profile needs a name" >&2; exit 2; }
            MODE=profile; PROFILE=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2 ;;
    esac
done

failures=0
selected=0

# One line of the inventory: a profile, a local scope, a label and the function
# that proves it. Declaring and selecting are the same act on purpose — a gate
# that is declared is a gate that can be run, and there is nowhere to declare one
# that nothing runs.
gate() {
    gate_profile=$1
    gate_scope=$2
    gate_label=$3
    gate_function=$4
    if [ "$LIST" -eq 1 ]; then
        printf '%s\t%s\t%s\n' "$gate_profile" "$gate_scope" "$gate_label"
        return 0
    fi
    case $MODE in
        profile) [ "$gate_profile" = "$PROFILE" ] || return 0 ;;
        default) [ "$gate_scope" = default ] || return 0 ;;
        full) ;;
    esac
    run_gate "$gate_label" "$gate_function"
}

run_gate() {
    label=$1
    shift
    selected=$((selected + 1))
    printf '\n==> %s\n' "$label"
    if "$@"; then
        printf 'PASS: %s\n' "$label"
    else
        rc=$?
        failures=$((failures + 1))
        printf 'FAIL: %s (exit %s)\n' "$label" "$rc" >&2
    fi
}

specification() { python3 "$ROOT/tools/build-specification.py" --check; }
# Reproducibility proves the generated view matches the inputs that are listed.
# Completeness — that everything required is listed — is a different statement
# and needs its own gate (docs/38 release check).
specification_manifest() {
    python3 "$ROOT/scripts/check-specification-manifest.py" --root "$ROOT"
}
# The journal's two present-tense lists against the ADR files themselves: which
# ADRs are not over, and which questions an ADR raised and did not answer.
# PROGRESS.md is chronological, so a summary sentence in it is the thing that
# goes stale; this is why both lists are checked rather than trusted. The second
# exists because a question outlives its decision — an ADR can close on the day
# it was raised and leave one standing.
open_decisions() {
    bash "$ROOT/scripts/check-open-decisions.sh"
}
# VIRTIO 1.4 §2.1.1: a driver adds status bits and never replaces the byte, so
# a bit the *device* set is not erased by a driver rebuilding the byte from the
# sequence it believes it performed. Structural rather than a spelling: it reads
# every DEVICE_STATUS write in every fixture that declares one.
device_status_additive() {
    bash "$ROOT/scripts/tests/check-device-status-additive.sh"
}
interface_contract_authority() {
    bash "$ROOT/scripts/tests/check-interface-contract-authority.sh"
}
interface_schema() {
    bash "$ROOT/scripts/tests/check-interface-schema.sh"
}
# What the Stage 4 client/service slice may be said to prove, against what an
# accepted schema row can actually carry. A scalar crossing IPC is not block data
# crossing IPC, and while no row places a region in a message, no document may say
# otherwise.
stage4_data_path_claims() {
    bash "$ROOT/scripts/tests/check-stage4-data-path-claims.sh"
}
abi_operations() {
    bash "$ROOT/scripts/tests/check-abi-operations.sh"
}
endowment_constants() {
    bash "$ROOT/scripts/tests/check-endowment-constants.sh"
}
closure_audit() {
    bash "$ROOT/scripts/tests/check-closure-audit.sh"
}
operator_journal() {
    bash "$ROOT/scripts/tests/check-operator-journal.sh"
}
boot_event_contract() {
    bash "$ROOT/scripts/tests/check-boot-event-contract.sh"
}
exception_foundation() {
    bash "$ROOT/scripts/tests/check-nucleus-exception-foundation.sh"
}
# The repository claim: every unsafe operation in the trusted base carries a
# local rationale.
unsafe_safety() {
    python3 "$ROOT/scripts/check-unsafe-safety.py" --root "$ROOT"
}
capsule_provenance() {
    bash "$ROOT/scripts/tests/check-capsule-provenance.sh"
}
embedded_artwork_provenance() {
    bash "$ROOT/scripts/tests/check-embedded-artwork-provenance.sh"
}
run_tos_launcher() {
    bash "$ROOT/scripts/tests/run-tos.sh"
}
qemu_interactive_mode() {
    bash "$ROOT/scripts/tests/qemu-interactive-mode.sh"
}
qemu_event_capture() {
    bash "$ROOT/scripts/tests/capture-qemu-events.sh"
}
qemu_timed_harness() {
    bash "$ROOT/scripts/tests/qemu-timed-harness.sh"
}
stage1_performance_workload() {
    bash "$ROOT/scripts/tests/stage1-performance-workload.sh"
}
stage1_native_validation_harness() {
    bash "$ROOT/scripts/tests/stage1-native-validation-harness.sh"
}
stage2_language_contract() {
    bash "$ROOT/scripts/tests/check-stage2-language-contract.sh"
}
freestanding_runtime_source() {
    python3 "$ROOT/scripts/check-freestanding-runtime.py" --root "$ROOT"
}
# The build is what proves the whole dependency closure is free of `std`; the
# source gate above only proves no module names a host facility.
build_freestanding_runtime() {
    (cd "$ROOT/source" && cargo build --release --target x86_64-unknown-none \
        -p tos-core -p tos-ir -p tos-verifier -p tos-image -p tos-residency \
        -p tos-engine -p tos-cache)
}
release_manifest() { python3 "$ROOT/tools/build-release-manifest.py" --check; }
spdx() { sh "$ROOT/scripts/check-spdx.sh"; }
dco() { sh "$ROOT/scripts/check-dco.sh"; }
# The x86-64 half of ADR-0086, read out of the built image rather than out of
# the source: exactly one LFENCE for `Consume`, no hardware fence for
# `Publish`, and no non-temporal store anywhere the store-store premise covers.
dma_ordering_backend() {
    bash "$ROOT/scripts/tests/check-dma-ordering-backend.sh"
}
fmt() { (cd "$ROOT/source" && cargo fmt --all -- --check); }
# `cargo test` covers the workspace default members. The UEFI loader is not one
# — it is a target-only crate — so its host unit tests were never being run by
# this gate despite existing. They are named explicitly.
tests() {
    (cd "$ROOT/source" && cargo test && cargo test -p tos-uefi-loader)
}
clippy_host() { (cd "$ROOT/source" && cargo clippy --all-targets -- -D warnings); }
clippy_uefi() {
    (cd "$ROOT/source" && cargo clippy -p tos-uefi-loader \
        --target x86_64-unknown-uefi -- -D warnings)
}
# Every declared feature of the freestanding binaries still type-checks. A
# feature nothing builds is a feature nothing checks, and Stage 4B lost a trait
# implementation behind one for exactly that reason.
feature_builds() {
    bash "$ROOT/scripts/tests/check-feature-builds.sh"
}
clippy_nucleus() {
    (cd "$ROOT/source" && cargo clippy -p tos-nucleus \
        --target x86_64-unknown-none -- -D warnings)
}
# **The ring-3 runtime image needs its own invocation and had none.** It is a
# separate freestanding binary on a separate target, so `clippy_host`'s workspace
# run does not reach it and `clippy_nucleus` names a different package — and the
# three clippy gates together looked like coverage of the freestanding tree while
# the image sat outside all of them. A live `doc_lazy_continuation` had been there
# since 2026-08 for that reason: a doc block was orphaned from the function it
# documents by a later insertion, and nothing local or in CI ever compiled the
# lint. Default features, exactly as the nucleus gate is; every declared feature
# of this binary is type-checked by `feature_builds` above.
clippy_runtime_image() {
    (cd "$ROOT/source" && cargo clippy -p tos-runtime-image \
        --target x86_64-unknown-none -- -D warnings)
}
fuzz() {
    (cd "$ROOT/source" && cargo run --release -p tos-tests-fuzz -- 200000)
}
build_capsule_tool() {
    (cd "$ROOT/source" && cargo build --release -p tos-capsule-tool)
}
build_uefi() {
    (cd "$ROOT/source" && cargo build --release -p tos-uefi-loader \
        --target x86_64-unknown-uefi)
}
build_nucleus() {
    (cd "$ROOT/source" && cargo build --release -p tos-nucleus \
        --target x86_64-unknown-none)
}
# The ring-3 runtime image is a boot artifact of its own (ADR-0053 option B):
# the machine does not boot without it, so it is built beside the nucleus rather
# than as part of it.
build_runtime_image() {
    (cd "$ROOT/source" && cargo build --release -p tos-runtime-image \
        --target x86_64-unknown-none)
}
qemu_success() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/run.sh \
        --out target/preflight-qemu/success --expect 33)
}
qemu_negative() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/negative-suite.sh \
        target/preflight-qemu/negative)
}
qemu_stage2_runtime() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/stage2-runtime.sh \
        target/preflight-qemu/stage2-runtime)
}
qemu_no_framebuffer() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/no-framebuffer.sh \
        target/preflight-qemu/no-framebuffer)
}
qemu_module_set() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/module-set.sh \
        target/preflight-qemu/module-set)
}
qemu_boot_module_failure() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/boot-module-failure.sh \
        target/preflight-qemu/boot-module-failure)
}
qemu_capsule_size_limit() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/capsule-size-limit.sh \
        target/preflight-qemu/capsule-size-limit)
}
qemu_memory_account() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/memory-account.sh \
        target/preflight-qemu/memory-account)
}
qemu_creation_rollback() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/creation-rollback.sh \
        target/preflight-qemu/creation-rollback)
}
qemu_memory_authority() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/memory-authority.sh \
        target/preflight-qemu/memory-authority)
}
qemu_region_transport() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/region-transport.sh \
        target/preflight-qemu/region-transport)
}
qemu_region_faults() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/region-faults.sh \
        target/preflight-qemu/region-faults)
}
qemu_bundle_launch() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/bundle-launch.sh \
        target/preflight-qemu/bundle-launch)
}
qemu_exception_ud2() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/exception-injection.sh ud2)
}
qemu_exception_gp() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/exception-injection.sh gp)
}
qemu_paging_unmapped() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/exception-injection.sh paging)
}
qemu_paging_readonly_text() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/exception-injection.sh readonly-text)
}
qemu_process_abi() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/process-isolation.sh abi)
}
qemu_process_privileged() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/process-isolation.sh privileged)
}
qemu_process_nucleus_memory() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/process-isolation.sh nucleus)
}
qemu_scheduler() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/scheduler.sh \
        target/preflight-qemu/scheduler)
}
qemu_capabilities() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/capabilities.sh \
        target/preflight-qemu/capabilities)
}
qemu_supervisor() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/supervisor.sh \
        target/preflight-qemu/supervisor)
}
qemu_blocking() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/blocking.sh \
        target/preflight-qemu/blocking)
}
qemu_request_reply() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/request-reply.sh \
        target/preflight-qemu/request-reply)
}
qemu_exchange_cost() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/exchange-cost.sh \
        target/preflight-qemu/exchange-cost)
}
qemu_deputy() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/deputy.sh \
        target/preflight-qemu/deputy)
}
qemu_second_receiver() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/second-receiver.sh \
        target/preflight-qemu/second-receiver)
}
qemu_module_operation() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/module-operation.sh \
        target/preflight-qemu/module-operation)
}
qemu_process_control() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/process-control.sh \
        target/preflight-qemu/process-control)
}
qemu_build_topology() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/build-topology.sh \
        target/preflight-qemu/build-topology)
}
qemu_supervision() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/supervision.sh \
        target/preflight-qemu/supervision)
}
qemu_runtime_authority() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/runtime-authority.sh \
        target/preflight-qemu/runtime-authority)
}
qemu_lifecycle() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/lifecycle.sh \
        target/preflight-qemu/lifecycle)
}
qemu_process_launch() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/process-launch.sh \
        target/preflight-qemu/process-launch)
}
# Stage 4A (ADR-0079): a textual module holds the platform root and takes an
# exclusive assignment of a real PCI function. `full-only` because it needs the
# Stage 4 device profile, which is an extension of the ADR-0040 base platform
# rather than part of it.
qemu_pci_discovery() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/pci-discovery.sh \
        target/preflight-qemu/pci-discovery)
}
# Stage 4B: canonical text discovers the real VirtIO PCI capability structures
# through configuration reads alone. `full-only` for the Stage 4 device profile.
qemu_virtio_caps() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/virtio-caps.sh \
        target/preflight-qemu/virtio-caps)
}
# Stage 4B: canonical text maps a BAR window and reads the real VirtIO common
# configuration. `full-only` for the Stage 4 device profile.
qemu_pci_placement() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/pci-placement.sh \
        target/preflight-qemu/pci-placement)
}
qemu_virtio_mmio() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/virtio-mmio.sh \
        target/preflight-qemu/virtio-mmio)
}
# Stage 4C-1b (ADR-0082): a textual driver holding one PCI function derives one
# routed interrupt of it, blocks, and is woken by a real MSI-X message from the
# real device. `full-only` for the Stage 4 device profile.
qemu_dma_region() {
    bash "$ROOT/source/host-tools/qemu-test/dma-region.sh"
}
# Stage 4D-1: one real split virtqueue on the reference endpoint, configured by
# canonical TOS text and left empty and enabled. It proves the device accepted
# the substrate, and deliberately not that the device performed DMA through it.
qemu_virtio_queue() {
    bash "$ROOT/source/host-tools/qemu-test/virtio-queue.sh"
}
# Stage 4D-2: one real VIRTIO_BLK_T_IN of sector 0 through that queue, proved by
# a sentinel the device had to replace rather than by a success code.
qemu_virtio_block_read() {
    bash "$ROOT/source/host-tools/qemu-test/virtio-block-read.sh"
}
# Stage 4D-3: two sequential VIRTIO_BLK_T_IN through **one** initialized queue,
# with descriptors reclaimed and reused between them and both ring indices
# carried across. It proves the queue is not a one-shot boot witness.
qemu_virtio_block_reuse() {
    bash "$ROOT/source/host-tools/qemu-test/virtio-block-reuse.sh"
}
# Stage 4D-4: two VIRTIO_BLK_T_IN outstanding **together** in that queue. Both
# chains are built out of a pool exactly two chains wide and exposed by one
# store that moves `avail.idx` by two, and the completions are associated by
# `used_elem.id` in whichever order the device produces them.
qemu_virtio_block_two_inflight() {
    bash "$ROOT/source/host-tools/qemu-test/virtio-block-two-inflight.sh"
}
# Stage 4D-5: the first real block write. One `VIRTIO_BLK_T_OUT` of 512 bytes
# the module composed, proved by an independent `VIRTIO_BLK_T_IN` of the same
# A capability crosses between two canonical textual processes in a message,
# which ADR-0093 P3 needs and nothing in the schema could express: a textual
# service could not answer a call, put a capability into a message, or take one
# out. The nucleus was not changed.
qemu_capability_transfer() {
    bash "$ROOT/source/host-tools/qemu-test/capability-transfer.sh"
}
# A separate textual client, holding no part of the machine and no name for the
# service until the ADR-0093 P3 registry sends it one, is answered by a textual
# block service that performs a real DMA/VirtIO/IRQ read — and the number it gets
# back originated in the device.
#
# **Not the Stage 4 data path.** The sector's 512 bytes do not cross IPC; one
# scalar computed from them does. `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1
# puts the boundary at client memory through IPC into the service's DMA memory,
# and no accepted schema row can carry a region in a message, so that boundary is
# not reached here. The claim gate reads this comment, which is why it says so.
#
# Publication authority *is* `CAPABILITY_V1` §6's, as ADR-0095 amended it: the
# publish endpoint's identity is the authority, and the negative — that a process
# holding no capability naming it cannot publish — is `qemu_publication_authority`.
qemu_block_service() {
    bash "$ROOT/source/host-tools/qemu-test/block-service.sh"
}
qemu_name_service() {
    bash "$ROOT/source/host-tools/qemu-test/name-service.sh"
}
# `CAPABILITY_V1` §6 as ADR-0095 amended it: the right to publish
# `block.device.v1` is a capability naming a dedicated publication endpoint, so
# possession of a name for that endpoint is the authority. Two boots of one
# capsule differing in exactly one endowment — the claimant's source, binding and
# registry are identical in both, and only the capability moves. ADR-0095 §5.2 is
# the negative and §5.3 is the mutation, which is performed rather than described.
qemu_publication_authority() {
    bash "$ROOT/source/host-tools/qemu-test/publication-authority.sh"
}
# ADR-0097's conformance gate: an ordinary immutable `Region<u8>` crosses IPC
# between two canonical textual processes and every byte of it arrives, counted in
# canonical text. **It is also the third condition `stage4_data_path_claims` reads**
# — the one that had to be a gate rather than a declaration, because evidence
# follows a passing gate here and never precedes one. Its three negatives are
# separate boots: a writable region refused by the transport, an ordinary region
# refused at a DMA capability position, and a 1.4 module refused against minor 5.
region_ipc_payload() {
    bash "$ROOT/source/host-tools/qemu-test/region-transfer-text.sh"
}
# **The Stage 4 data path, in the read direction.** All 512 bytes of one sector
# cross the client/service boundary as an ordinary immutable region: the client
# requests a sector and hands over a channel, the service performs the real
# VirtIO/DMA read, copies the bytes once out of device-visible memory — the copy
# ADR-0037 forces — freezes the region and sends it, and the client indexes every
# byte. `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1's boundary, reached.
#
# The client holds no hardware authority and cannot import `system.memory.Region`
# at all, so the region it reads can only have arrived in a message. The bytes are
# counted in canonical text and the gate reads two numbers; nothing on the host
# inspects the payload.
qemu_block_data_path() {
    bash "$ROOT/source/host-tools/qemu-test/block-data-path.sh"
}
# **The Stage 4 Branch-A persistence criterion, and ADR-0093 case C.** A block
# service serves a write and ends still holding the function, window, interrupt
# source and DMA region; the supervisor collects its ending, withdraws its
# publication from the registry and only then creates a successor; the successor
# claims the same function at a new assignment generation, drives VirtIO
# `DEVICE_STATUS` to 0 (ADR-0092 R1a's T1), initializes again and republishes; and
# a fresh lookup gives the client an endpoint through which all 512 bytes the first
# instance wrote come back.
#
# **The stale capability is the experiment.** The client still holds the name the
# first lookup gave it, and calls it while the successor is waiting for a request.
# It is not served — both waits are cancelled — so the name was never repaired
# (ADR-0093 §3a.5).
qemu_block_lifecycle() {
    bash "$ROOT/source/host-tools/qemu-test/block-lifecycle.sh"
}
# The accepted `block.device.v1` protocol (ADR-0098, `BLOCK_DEVICE_V1`), spoken by
# canonical text on both sides instead of by a fixture's own encoding.
#
# `word = sector * 4 + opcode`, all three operations of ADR-0093 §0's surface, and
# every refusal §7 states — each of them a reply. Two claims the gate is built
# around: a **write uses the region its own call carried**, proved by a decoy region
# sent just before it that a service of the old two-message shape would write
# instead; and a **read replies before it sends**, because the reverse leaves an
# orphan region queued on an endpoint that outlives its caller. `CAPACITY` answers
# the device's own count, proved by a second boot against a smaller device.
qemu_block_protocol() {
    bash "$ROOT/source/host-tools/qemu-test/block-protocol.sh"
}
# A process makes a **send-only** name for an endpoint it receives on (ADR-0100).
#
# `IPC_V1` §6 delegates at the rights the sender holds and §2 admits one
# receive-rights holder, so a channel handed over in a request has to be attenuated —
# and a delivery that would make a second receiver is refused *whole*, which from the
# waiter's side is a cancelled receive. The boot proves the original is neither
# consumed nor displaced, that the alias crosses and carries `send`, that it cannot
# receive, and that attenuation is an intersection rather than a validation: a
# send-only name asked for `send | receive` yields `send`.
qemu_endpoint_attenuation() {
    bash "$ROOT/source/host-tools/qemu-test/endpoint-attenuation.sh"
}
# A call that carries a capability reads the reply it is answered with (ADR-0101).
#
# `endpoint_call_word_carrying` was admitted producing `i64`, which left its caller
# able to learn that the call had been answered and nothing about the answer — so
# `BLOCK_DEVICE_V1` §5, which requires every reply to be read as
# `system.ipc.Answer{length, word}`, was unreadable by the §6a `READ` client bound to
# obey it. This boot proves the corrected result without a device: two distinct
# success words, one word in the refusal class above the top bit, and one reply with
# no payload at all, which is what makes the length the replier's rather than the
# row's.
qemu_carried_call_answer() {
    bash "$ROOT/source/host-tools/qemu-test/carried-call-answer.sh"
}
# A persistent object store, read back by a process that came after (ADR-0099).
#
# `state.store.v1` over `block.device.v1` over the reference VirtIO device, every layer
# canonical text. An initializer formats a zeroed device once and **refuses** the second
# time, because only an all-zero sector 0 is permission; a writer puts two objects and
# ends; the store's first generation ends, is retired and is collected **before** its
# successor exists; the successor re-reads and validates the header from the device; and a
# reader holding no way to address a sector gets object 2 and checks all 512 bytes in
# canonical text. `get(3)`, an id never created, is refused as absent and its sector is
# never read — presence is the occupancy bitmap and nothing else.
qemu_state_store() {
    bash "$ROOT/source/host-tools/qemu-test/state-store.sh"
}
qemu_repository_linkage() {
    bash "$ROOT/source/host-tools/qemu-test/repository-linkage.sh"
}
# sector into a different buffer — the status byte is not the evidence. The
# device's own `capacity` is read under §2.5.1's generation protocol first,
# because §5.2.6.1 forbids a request beyond it.
qemu_virtio_block_write() {
    bash "$ROOT/source/host-tools/qemu-test/virtio-block-write.sh"
}
qemu_irq_routed() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/irq-routed.sh \
        target/preflight-qemu/irq-routed)
}
qemu_supervisor_text() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/supervisor-text.sh \
        target/preflight-qemu/supervisor-text)
}
qemu_direction_flag() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/direction-flag.sh \
        target/preflight-qemu/direction-flag)
}
# Active Stage 1 validation-performance conformance (ADR-0083): one
# measurement-only image, two runtime-selected modes, and the p95 quotient of
# the complete logical validation workload over its own unavoidable
# cryptographic subset, bounded at 1.30.
qemu_paired_performance_conformance() {
    conformance_status=P1
    if [ "${GITHUB_ACTIONS:-}" = true ]; then
        conformance_status=P2
    fi
    (cd "$ROOT/source" && bash host-tools/qemu-test/stage1-paired-conformance.sh \
        --out target/preflight-qemu/performance-adr-0083 \
        --evidence-status "$conformance_status")
}
# ADR-0026's retained evidence set on the same harness: the native series, the
# mandatory functional profile whose absolute timing is a retained regression
# metric, and the separately linked crypto series. Its quotient is recorded and
# **not** asserted — ADR-0083 superseded that construction as conformance on
# 2026-09-06, and this gate keeps measuring it rather than deleting it. It still
# fails on evidence integrity: mismatched commits, workloads or accounting.
#
# The *evidence status* is the one environment-specific thing about either gate:
# ADR-0040 reserves P2 for the reference platform, and the scripts refuse to
# emit it anywhere else. The claim measured is the same either way.
qemu_performance_historical() {
    conformance_status=P1
    if [ "${GITHUB_ACTIONS:-}" = true ]; then
        conformance_status=P2
    fi
    (cd "$ROOT/source" && bash host-tools/qemu-test/stage1-performance-historical.sh \
        --out target/preflight-qemu/performance-adr-0026 \
        --evidence-status "$conformance_status")
}
qemu_stage3_observer_conformance() {
    conformance_status=P1
    if [ "${GITHUB_ACTIONS:-}" = true ]; then
        conformance_status=P2
    fi
    (cd "$ROOT/source" && \
        bash host-tools/qemu-test/stage3-observer-conformance.sh \
        --out target/preflight-qemu/performance-stage3-observer \
        --evidence-status "$conformance_status")
}
qemu_stage3_ipc_conformance() {
    conformance_status=P1
    if [ "${GITHUB_ACTIONS:-}" = true ]; then
        conformance_status=P2
    fi
    (cd "$ROOT/source" && \
        bash host-tools/qemu-test/stage3-ipc-conformance.sh \
        --out target/preflight-qemu/performance-stage3-ipc \
        --evidence-status "$conformance_status")
}
qemu_bootinfo_identity_mismatch() {
    bash "$ROOT/scripts/tests/qemu-bootinfo-identity-mismatch.sh"
}

# --- gates that test the gates -------------------------------------------------
# These prove nothing about the repository. Each one runs a checker or a harness
# against a fixture and asserts that it still detects what it is for, which is a
# different claim from the one the checker makes and is kept in its own profile
# so the two are never read as one (ADR-0065).
selftest_unsafe_safety() {
    bash "$ROOT/scripts/tests/check-unsafe-safety.sh"
}
selftest_capsule_format_alignment() {
    bash "$ROOT/scripts/tests/check-capsule-format-alignment.sh"
}
selftest_capsule_vector_provenance() {
    bash "$ROOT/scripts/tests/check-capsule-vector-provenance.sh"
}
selftest_spdx_assembly() { bash "$ROOT/scripts/tests/check-spdx-assembly.sh"; }
selftest_spdx_assets() { sh "$ROOT/scripts/tests/check-spdx-assets.sh"; }
selftest_spdx_json() { sh "$ROOT/scripts/tests/check-spdx-json.sh"; }
selftest_gate_parity() {
    bash "$ROOT/scripts/tests/check-gate-parity.sh"
}
# The two enforcement gates added in the 2026-09-23 corrective round, each against
# the truth table it is supposed to have. Both were wrong on a state that never
# occurs today and would have occurred later: one lifted its bound on any single
# condition where its own comment claimed it needed all of them, and the other
# could not express "nothing is open" at all.
selftest_open_decisions() {
    bash "$ROOT/scripts/tests/check-open-decisions.sh"
}
selftest_stage4_data_path_claims() {
    bash "$ROOT/scripts/tests/check-stage4-data-path-claims-selftest.sh"
}
selftest_repository_extent() {
    bash "$ROOT/scripts/tests/check-repository-extent.sh"
}
selftest_measurement_observer() {
    python3 "$ROOT/source/host-tools/qemu-test/test-measure-channel.py"
    python3 "$ROOT/source/host-tools/qemu-test/test-qualify-observer.py"
    python3 "$ROOT/source/host-tools/qemu-test/test-qualify-ipc.py"
}

# The parity between this inventory and what CI runs (ADR-0065). It reads the
# inventory from `--list` and the workflows structurally; it is a gate like any
# other, and its own regression test is in the selftest profile beside it.
gate_parity() {
    python3 "$ROOT/scripts/check-gate-parity.py" --root "$ROOT"
}

# --- the inventory ------------------------------------------------------------
# Declared once, here. `--list` prints it; `--profile` selects by the first
# column; a bare run selects by the second. CI names a profile and never a gate.

gate docs       default   "generated specification"                    specification
gate docs       default   "specification source manifest"              specification_manifest
gate docs       default   "open decisions and questions match the ADRs" open_decisions
gate docs       default   "release manifest and SHA256SUMS"            release_manifest
gate docs       default   "interface-contract authority"               interface_contract_authority
gate docs       default   "accepted interface schema"                  interface_schema
gate docs       default   "Stage 4 data-path claims"                   stage4_data_path_claims
gate docs       default   "VirtIO device status is additive"           device_status_additive
gate docs       default   "system ABI operation numbers"               abi_operations
gate docs       default   "launcher endowment constants"               endowment_constants
gate docs       default   "Boot ABI event contract"                    boot_event_contract
gate docs       default   "operator important-error view"              operator_journal
gate docs       default   "Stage 3 closure audit"                      closure_audit
gate docs       default   "nucleus exception foundation"               exception_foundation
gate docs       default   "Stage 2 language-contract consistency"      stage2_language_contract
gate docs       default   "CI and preflight prove the same gates"      gate_parity

gate provenance default   "SPDX licence inventory"                     spdx
gate provenance default   "DCO sign-off"                               dco
gate provenance default   "embedded artwork provenance"                embedded_artwork_provenance

gate source     default   "unsafe-code safety evidence"                unsafe_safety
gate source     default   "capsule provenance sidecar"                 capsule_provenance
gate source     default   "freestanding runtime source"                freestanding_runtime_source
gate source     default   "freestanding runtime build"                 build_freestanding_runtime
gate source     default   "DMA ordering backend asymmetry"             dma_ordering_backend
gate source     default   "cargo fmt"                                  fmt
gate source     default   "cargo test"                                 tests
gate source     default   "clippy host"                                clippy_host
gate source     default   "clippy UEFI loader"                         clippy_uefi
gate source     full-only "feature configurations type-check"        feature_builds
gate source     default   "clippy nucleus"                             clippy_nucleus
gate source     default   "clippy runtime image"                       clippy_runtime_image
gate source     full-only "capsule parser fuzz"                        fuzz

gate selftest   default   "unsafe-safety checker self-test"            selftest_unsafe_safety
gate selftest   default   "capsule format alignment self-test"         selftest_capsule_format_alignment
gate selftest   default   "capsule vector provenance self-test"        selftest_capsule_vector_provenance
gate selftest   default   "SPDX assembly classification self-test"     selftest_spdx_assembly
gate selftest   default   "SPDX asset classification self-test"        selftest_spdx_assets
gate selftest   default   "SPDX JSON classification self-test"         selftest_spdx_json
gate selftest   default   "gate parity self-test"                      selftest_gate_parity
gate selftest   default   "open-decision gate self-test"               selftest_open_decisions
gate selftest   default   "Stage 4 data-path claim gate self-test"     selftest_stage4_data_path_claims
gate selftest   default   "repository extent tool self-test"           selftest_repository_extent
gate selftest   default   "measurement observer self-test"             selftest_measurement_observer
gate selftest   default   "run-tos launcher self-test"                 run_tos_launcher
gate selftest   default   "interactive QEMU mode self-test"            qemu_interactive_mode
gate selftest   default   "QEMU event capture self-test"               qemu_event_capture
gate selftest   default   "timed QEMU harness self-test"               qemu_timed_harness
gate selftest   default   "Stage 1 performance workload self-test"     stage1_performance_workload
gate selftest   default   "Stage 1 native validation harness self-test" stage1_native_validation_harness

gate qemu       full-only "build capsule tool"                         build_capsule_tool
gate qemu       full-only "build UEFI loader"                          build_uefi
gate qemu       full-only "build nucleus"                              build_nucleus
gate qemu       full-only "build runtime image"                        build_runtime_image
gate qemu       full-only "QEMU success boot"                          qemu_success
gate qemu       full-only "QEMU negative suite"                        qemu_negative
gate qemu       full-only "QEMU Stage 2 runtime path"                  qemu_stage2_runtime
gate qemu       full-only "QEMU boot without a framebuffer"            qemu_no_framebuffer
gate qemu       full-only "QEMU multi-module capsule"                  qemu_module_set
gate qemu       full-only "QEMU boot-module failure code"              qemu_boot_module_failure
gate qemu       full-only "QEMU capsule size limit"                    qemu_capsule_size_limit
gate qemu       full-only "QEMU unified memory account"                 qemu_memory_account
gate qemu       full-only "QEMU creation rollback"                     qemu_creation_rollback
gate qemu       full-only "QEMU memory authority at CPL 3"              qemu_memory_authority
gate qemu       full-only "QEMU a region crosses between processes"     qemu_region_transport
gate qemu       full-only "QEMU a region is data, and a released one is nothing" qemu_region_faults
gate qemu       full-only "QEMU a process is created from a bundle"     qemu_bundle_launch
gate qemu       full-only "QEMU exception #UD"                         qemu_exception_ud2
gate qemu       full-only "QEMU exception #GP"                         qemu_exception_gp
gate qemu       full-only "QEMU unmapped page faults"                  qemu_paging_unmapped
gate qemu       full-only "QEMU nucleus text is read-only"             qemu_paging_readonly_text
gate qemu       full-only "QEMU system ABI at CPL 3"                   qemu_process_abi
gate qemu       full-only "QEMU privileged instruction at CPL 3"       qemu_process_privileged
gate qemu       full-only "QEMU process cannot write nucleus memory"   qemu_process_nucleus_memory
gate qemu       full-only "QEMU two processes are scheduled"           qemu_scheduler
gate qemu       full-only "QEMU capabilities and IPC"                  qemu_capabilities
gate qemu       full-only "QEMU process authority"                     qemu_supervisor
gate qemu       full-only "QEMU blocking and the liveness rule"        qemu_blocking
gate qemu       full-only "QEMU request and reply"                     qemu_request_reply
gate qemu       full-only "QEMU what one request/reply costs"          qemu_exchange_cost
gate qemu       full-only "QEMU confused deputy"                       qemu_deputy
gate qemu       full-only "QEMU one endpoint has one receiver"         qemu_second_receiver
gate qemu       full-only "QEMU a module performs an operation"        qemu_module_operation
gate qemu       full-only "QEMU a module ends its own process"         qemu_process_control
gate qemu       full-only "QEMU a module launches a process"           qemu_process_launch
gate qemu       full-only "QEMU a supervisor collects endings"          qemu_lifecycle
gate qemu       full-only "QEMU runtime-obtained authority"             qemu_runtime_authority
gate qemu       full-only "QEMU textual service supervision"            qemu_supervision
gate qemu       full-only "QEMU T1 build topology"                     qemu_build_topology
gate qemu       full-only "QEMU a textual supervisor starts services"  qemu_supervisor_text
gate qemu       full-only "QEMU textual PCI function claim"            qemu_pci_discovery
gate qemu       full-only "QEMU textual VirtIO capability discovery"    qemu_virtio_caps
gate qemu       full-only "QEMU textual VirtIO register read"           qemu_virtio_mmio
gate qemu       full-only "QEMU a claimed function cannot be relocated"   qemu_pci_placement
gate qemu       full-only "QEMU a device interrupt wakes its driver"    qemu_irq_routed
gate qemu       full-only "QEMU a textual driver makes and frees a DMA region" qemu_dma_region
gate qemu       full-only "QEMU a textual driver configures one virtqueue"  qemu_virtio_queue
gate qemu       full-only "QEMU a textual driver reads one real sector"    qemu_virtio_block_read
gate qemu       full-only "QEMU one queue serves more than one request"    qemu_virtio_block_reuse
gate qemu       full-only "QEMU two requests outstanding together"         qemu_virtio_block_two_inflight
gate qemu       full-only "QEMU a textual driver writes a real sector"     qemu_virtio_block_write
gate qemu       full-only "QEMU a capability crosses in a message"      qemu_capability_transfer
gate qemu       full-only "QEMU a client looks a service up"            qemu_name_service
gate qemu       full-only "QEMU publishing needs the publish endpoint" qemu_publication_authority
gate qemu       full-only "QEMU a region payload crosses IPC"    region_ipc_payload
gate qemu       full-only "QEMU a sector crosses IPC as a region"  qemu_block_data_path
gate qemu       full-only "QEMU a successor restarts the device path" qemu_block_lifecycle
gate qemu       full-only "QEMU the accepted block.device.v1 protocol" qemu_block_protocol
gate qemu       full-only "QEMU a send-only name for an endpoint"    qemu_endpoint_attenuation
gate qemu       full-only "QEMU a carried call reads its reply"       qemu_carried_call_answer
gate qemu       full-only "QEMU a persistent object store"             qemu_state_store
gate qemu       full-only "QEMU capsule-to-repository linkage"         qemu_repository_linkage
gate qemu       full-only "QEMU a device answer reaches a bare client"  qemu_block_service
gate qemu       full-only "QEMU flags a process was holding"           qemu_direction_flag
gate qemu       full-only "QEMU BootInfo identity mismatch self-test"  qemu_bootinfo_identity_mismatch
gate qemu       full-only "Stage 1 ADR-0083 paired validation performance" qemu_paired_performance_conformance
gate qemu       full-only "Stage 1 ADR-0026 retained historical evidence" qemu_performance_historical
gate qemu       full-only "Stage 3 ADR-0066 observer conformance"     qemu_stage3_observer_conformance
gate qemu       full-only "Stage 3 IPC latency conformance"          qemu_stage3_ipc_conformance

if [ "$LIST" -eq 1 ]; then
    exit 0
fi

printf '\n'
if [ "$failures" -eq 0 ]; then
    printf 'PREFLIGHT PASS: %s gate(s) passed\n' "$selected"
    exit 0
fi
printf 'PREFLIGHT FAIL: %s of %s gate(s) failed\n' "$failures" "$selected" >&2
exit 1
