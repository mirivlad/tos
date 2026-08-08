#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression checks for the explicit, isolated BootInfo identity-corruption
# QEMU test path. This is intentionally a CLI/configuration test: the real
# feature artifact and nucleus-visible failure are exercised separately.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HARNESS="$ROOT/source/host-tools/qemu-test/run.sh"
LOADER_MANIFEST="$ROOT/source/boot/uefi-loader/Cargo.toml"
MISMATCH="$ROOT/source/host-tools/qemu-test/bootinfo-identity-mismatch.sh"

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

[[ -f "$HARNESS" ]] || fail "missing QEMU harness"
[[ -f "$LOADER_MANIFEST" ]] || fail "missing loader manifest"
[[ -f "$MISMATCH" ]] || fail "missing BootInfo mismatch scenario"

bash "$HARNESS" --help | grep -F -- '--loader FILE' >/dev/null \
    || fail "harness does not document --loader FILE"
rg -Fx 'test-corrupt-bootinfo-identity = []' "$LOADER_MANIFEST" >/dev/null \
    || fail "loader feature is missing or carries implicit dependencies"
if rg -q '^default\s*=.*test-corrupt-bootinfo-identity' "$LOADER_MANIFEST"; then
    fail "corruption feature is enabled by default"
fi
rg -F 'target/test-corrupt-bootinfo' "$MISMATCH" >/dev/null \
    || fail "mismatch scenario does not name the isolated target directory"
rg -F -- '--loader' "$MISMATCH" >/dev/null \
    || fail "mismatch scenario does not pass an explicit loader"

# Parsing a missing explicit loader must reach the artifact check and name that
# path. Before --loader exists, the harness instead exits at option parsing.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/source/host-tools/qemu-test" \
    "$TMP/source/target/release" \
    "$TMP/source/target/x86_64-unknown-none/release"
cp "$HARNESS" "$TMP/source/host-tools/qemu-test/run.sh"
touch "$TMP/source/target/release/tos-capsule-tool" \
    "$TMP/source/target/x86_64-unknown-none/release/tos-nucleus" \
    "$TMP/OVMF_CODE.fd" "$TMP/OVMF_VARS.fd"
missing_loader="$TMP/test-corrupt-loader.efi"
if OVMF_CODE="$TMP/OVMF_CODE.fd" OVMF_VARS="$TMP/OVMF_VARS.fd" \
    bash "$TMP/source/host-tools/qemu-test/run.sh" --out "$TMP/out" \
        --loader "$missing_loader" >"$TMP/harness.out" 2>&1; then
    fail "harness accepted a missing explicit loader"
fi
grep -Fx "missing: $missing_loader" "$TMP/harness.out" >/dev/null \
    || fail "harness did not select the explicit loader path"

echo "qemu-bootinfo-identity-mismatch: PASS"
