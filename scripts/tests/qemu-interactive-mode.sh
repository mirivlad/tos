#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the human QEMU mode's shared-path isolation.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HARNESS="${HARNESS:-$ROOT/source/host-tools/qemu-test/run.sh}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/repo/source/host-tools/qemu-test" \
    "$TMP/repo/source/target/release" \
    "$TMP/repo/source/target/x86_64-unknown-uefi/release" \
    "$TMP/repo/source/target/x86_64-unknown-none/release" \
    "$TMP/bin"
cp "$HARNESS" "$TMP/repo/source/host-tools/qemu-test/run.sh"
printf x > "$TMP/repo/source/target/release/tos-capsule-tool"
printf x > "$TMP/repo/source/target/x86_64-unknown-uefi/release/tos-uefi-loader.efi"
printf x > "$TMP/repo/source/target/x86_64-unknown-none/release/tos-nucleus"
printf x > "$TMP/capsule.bin"
printf x > "$TMP/OVMF_CODE.fd"
printf x > "$TMP/OVMF_VARS.fd"

for tool in mformat mcopy mmd dd; do
    printf '#!/bin/sh\nexit 0\n' > "$TMP/bin/$tool"
    chmod +x "$TMP/bin/$tool"
done
cat > "$TMP/bin/qemu-system-x86_64" <<'EOF'
#!/bin/sh
printf 'qemu %s\n' "$*" >> "$QEMU_INTERACTIVE_LOG"
exit 0
EOF
cat > "$TMP/bin/timeout" <<'EOF'
#!/bin/sh
printf 'timeout %s\n' "$*" >> "$QEMU_INTERACTIVE_LOG"
exec "$@"
EOF
chmod +x "$TMP/bin/qemu-system-x86_64" "$TMP/bin/timeout"

export QEMU_INTERACTIVE_LOG="$TMP/qemu.log"
(cd "$TMP/repo/source" && PATH="$TMP/bin:$PATH" \
    OVMF_CODE="$TMP/OVMF_CODE.fd" OVMF_VARS="$TMP/OVMF_VARS.fd" \
    bash host-tools/qemu-test/run.sh --out "$TMP/out" --capsule "$TMP/capsule.bin" \
        --interactive --display gtk)

if grep -Fq 'timeout ' "$QEMU_INTERACTIVE_LOG"; then
    echo 'FAIL: interactive QEMU was wrapped in timeout' >&2
    cat "$QEMU_INTERACTIVE_LOG" >&2
    exit 1
fi
if grep -Fq 'isa-debug-exit' "$QEMU_INTERACTIVE_LOG"; then
    echo 'FAIL: interactive QEMU includes isa-debug-exit' >&2
    cat "$QEMU_INTERACTIVE_LOG" >&2
    exit 1
fi
grep -Fq -- '-display gtk' "$QEMU_INTERACTIVE_LOG" || {
    echo 'FAIL: interactive QEMU did not receive the selected GTK backend' >&2
    cat "$QEMU_INTERACTIVE_LOG" >&2
    exit 1
}
echo 'qemu-interactive-mode: PASS'
