#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# One-command local repository preflight. This script orchestrates the existing
# authoritative gates; it does not reimplement their checks.
set -u

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
MODE=default

case ${1-} in
    '') ;;
    --full) MODE=full ;;
    -h|--help)
        cat <<'EOF'
Usage: ./scripts/preflight.sh [--full]

Default: generated docs/release, SPDX, DCO, fmt, tests and clippy.
--full:  also run deterministic fuzzing and QEMU success/negative boot gates.

Every gate runs even if an earlier gate fails. The final status is PASS only
when every selected authoritative command succeeds.
EOF
        exit 0 ;;
    *)
        echo "unknown option: $1" >&2
        echo "usage: ./scripts/preflight.sh [--full]" >&2
        exit 2 ;;
esac
if [ "$#" -gt 1 ]; then
    echo "usage: ./scripts/preflight.sh [--full]" >&2
    exit 2
fi

failures=0
selected=0

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
unsafe_safety() {
    python3 "$ROOT/scripts/check-unsafe-safety.py" --root "$ROOT" || return
    bash "$ROOT/scripts/tests/check-unsafe-safety.sh"
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
        -p tos-core -p tos-ir -p tos-verifier -p tos-engine -p tos-cache)
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

run_gate "generated specification" specification
run_gate "interface-contract authority" interface_contract_authority
run_gate "accepted interface schema" interface_schema
run_gate "system ABI operation numbers" abi_operations
run_gate "Boot ABI event contract" boot_event_contract
run_gate "nucleus exception foundation" exception_foundation
run_gate "unsafe-code safety evidence" unsafe_safety
run_gate "capsule provenance sidecar" capsule_provenance
run_gate "embedded artwork provenance" embedded_artwork_provenance
run_gate "run-tos launcher" run_tos_launcher
run_gate "interactive QEMU mode" qemu_interactive_mode
run_gate "QEMU event timestamp capture" qemu_event_capture
run_gate "timed QEMU harness" qemu_timed_harness
run_gate "Stage 1 performance workload" stage1_performance_workload
run_gate "Stage 1 native validation harness" stage1_native_validation_harness
run_gate "Stage 2 language-contract consistency" stage2_language_contract
run_gate "freestanding runtime source" freestanding_runtime_source
run_gate "freestanding runtime build" build_freestanding_runtime
run_gate "release manifest and SHA256SUMS" release_manifest
run_gate "SPDX licence inventory" spdx
run_gate "DCO sign-off" dco
run_gate "cargo fmt" fmt
run_gate "cargo test" tests
run_gate "clippy host" clippy_host
run_gate "clippy UEFI loader" clippy_uefi
run_gate "clippy nucleus" clippy_nucleus

if [ "$MODE" = full ]; then
    run_gate "capsule parser fuzz" fuzz
    run_gate "build capsule tool" build_capsule_tool
    run_gate "build UEFI loader" build_uefi
    run_gate "build nucleus" build_nucleus
    run_gate "build runtime image" build_runtime_image
    run_gate "QEMU success boot" qemu_success
    run_gate "QEMU negative suite" qemu_negative
    run_gate "QEMU Stage 2 runtime path" qemu_stage2_runtime
    run_gate "QEMU boot without a framebuffer" qemu_no_framebuffer
    run_gate "QEMU multi-module capsule" qemu_module_set
    run_gate "QEMU boot-module failure code" qemu_boot_module_failure
    run_gate "QEMU capsule size limit" qemu_capsule_size_limit
    run_gate "QEMU exception #UD" qemu_exception_ud2
    run_gate "QEMU exception #GP" qemu_exception_gp
    run_gate "QEMU unmapped page faults" qemu_paging_unmapped
    run_gate "QEMU nucleus text is read-only" qemu_paging_readonly_text
    run_gate "QEMU system ABI at CPL 3" qemu_process_abi
    run_gate "QEMU privileged instruction at CPL 3" qemu_process_privileged
    run_gate "QEMU process cannot write nucleus memory" qemu_process_nucleus_memory
    run_gate "QEMU two processes are scheduled" qemu_scheduler
    run_gate "QEMU capabilities and IPC" qemu_capabilities
    run_gate "QEMU process authority" qemu_supervisor
    run_gate "QEMU blocking and the liveness rule" qemu_blocking
    run_gate "QEMU request and reply" qemu_request_reply
    run_gate "QEMU confused deputy" qemu_deputy
    run_gate "QEMU one endpoint has one receiver" qemu_second_receiver
    run_gate "QEMU a module performs an operation" qemu_module_operation
    run_gate "QEMU a module ends its own process" qemu_process_control
    run_gate "QEMU a module launches a process" qemu_process_launch
    run_gate "QEMU a textual supervisor starts services" qemu_supervisor_text
    run_gate "QEMU flags a process was holding" qemu_direction_flag
fi

printf '\n'
if [ "$failures" -eq 0 ]; then
    printf 'PREFLIGHT PASS: %s gate(s) passed\n' "$selected"
    exit 0
fi
printf 'PREFLIGHT FAIL: %s of %s gate(s) failed\n' "$failures" "$selected" >&2
exit 1
