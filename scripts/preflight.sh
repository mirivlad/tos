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
boot_event_contract() {
    bash "$ROOT/scripts/tests/check-boot-event-contract.sh"
}
exception_foundation() {
    bash "$ROOT/scripts/tests/check-nucleus-exception-foundation.sh"
}
release_manifest() { python3 "$ROOT/tools/build-release-manifest.py" --check; }
spdx() { sh "$ROOT/scripts/check-spdx.sh"; }
dco() { sh "$ROOT/scripts/check-dco.sh"; }
fmt() { (cd "$ROOT/source" && cargo fmt --all -- --check); }
tests() { (cd "$ROOT/source" && cargo test); }
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
qemu_success() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/run.sh \
        --out target/preflight-qemu/success --expect 33)
}
qemu_negative() {
    (cd "$ROOT/source" && bash host-tools/qemu-test/negative-suite.sh \
        target/preflight-qemu/negative)
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

run_gate "generated specification" specification
run_gate "interface-contract authority" interface_contract_authority
run_gate "Boot ABI event contract" boot_event_contract
run_gate "nucleus exception foundation" exception_foundation
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
    run_gate "QEMU success boot" qemu_success
    run_gate "QEMU negative suite" qemu_negative
    run_gate "QEMU capsule size limit" qemu_capsule_size_limit
    run_gate "QEMU exception #UD" qemu_exception_ud2
    run_gate "QEMU exception #GP" qemu_exception_gp
fi

printf '\n'
if [ "$failures" -eq 0 ]; then
    printf 'PREFLIGHT PASS: %s gate(s) passed\n' "$selected"
    exit 0
fi
printf 'PREFLIGHT FAIL: %s of %s gate(s) failed\n' "$failures" "$selected" >&2
exit 1
