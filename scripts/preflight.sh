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
interface_contract_authority() {
    bash "$ROOT/scripts/tests/check-interface-contract-authority.sh"
}
interface_schema() {
    bash "$ROOT/scripts/tests/check-interface-schema.sh"
}
abi_operations() {
    bash "$ROOT/scripts/tests/check-abi-operations.sh"
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
clippy_nucleus() {
    (cd "$ROOT/source" && cargo clippy -p tos-nucleus \
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
qemu_lifecycle() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/lifecycle.sh \
        target/preflight-qemu/lifecycle)
}
qemu_process_launch() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/process-launch.sh \
        target/preflight-qemu/process-launch)
}
qemu_supervisor_text() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/supervisor-text.sh \
        target/preflight-qemu/supervisor-text)
}
qemu_direction_flag() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/direction-flag.sh \
        target/preflight-qemu/direction-flag)
}
# ADR-0026's mandatory functional profile and its p95 ratio, on the boot harness
# this repository ships. One gate, and the *evidence status* is the one thing
# about it that is environment-specific: ADR-0040 reserves P2 for the reference
# platform, and the script refuses to emit it anywhere else. The claim measured
# is the same either way.
qemu_performance_conformance() {
    conformance_status=P1
    if [ "${GITHUB_ACTIONS:-}" = true ]; then
        conformance_status=P2
    fi
    (cd "$ROOT/source" && bash host-tools/qemu-test/stage1-performance-conformance.sh \
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
gate docs       default   "release manifest and SHA256SUMS"            release_manifest
gate docs       default   "interface-contract authority"               interface_contract_authority
gate docs       default   "accepted interface schema"                  interface_schema
gate docs       default   "system ABI operation numbers"               abi_operations
gate docs       default   "Boot ABI event contract"                    boot_event_contract
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
gate source     default   "cargo fmt"                                  fmt
gate source     default   "cargo test"                                 tests
gate source     default   "clippy host"                                clippy_host
gate source     default   "clippy UEFI loader"                         clippy_uefi
gate source     default   "clippy nucleus"                             clippy_nucleus
gate source     full-only "capsule parser fuzz"                        fuzz

gate selftest   default   "unsafe-safety checker self-test"            selftest_unsafe_safety
gate selftest   default   "capsule format alignment self-test"         selftest_capsule_format_alignment
gate selftest   default   "capsule vector provenance self-test"        selftest_capsule_vector_provenance
gate selftest   default   "SPDX assembly classification self-test"     selftest_spdx_assembly
gate selftest   default   "SPDX asset classification self-test"        selftest_spdx_assets
gate selftest   default   "SPDX JSON classification self-test"         selftest_spdx_json
gate selftest   default   "gate parity self-test"                      selftest_gate_parity
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
gate qemu       full-only "QEMU a textual supervisor starts services"  qemu_supervisor_text
gate qemu       full-only "QEMU flags a process was holding"           qemu_direction_flag
gate qemu       full-only "QEMU BootInfo identity mismatch self-test"  qemu_bootinfo_identity_mismatch
gate qemu       full-only "Stage 1 ADR-0026 performance conformance"   qemu_performance_conformance
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
