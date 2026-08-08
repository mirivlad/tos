#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the opt-in timed path in the single QEMU harness.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HARNESS="$ROOT/source/host-tools/qemu-test/run.sh"
CAPTURE="$ROOT/source/host-tools/qemu-test/capture-events.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/repo/source/host-tools/qemu-test" \
    "$TMP/repo/source/target/release" \
    "$TMP/repo/source/target/x86_64-unknown-uefi/release" \
    "$TMP/repo/source/target/x86_64-unknown-none/release" \
    "$TMP/bin"
cp "$HARNESS" "$TMP/repo/source/host-tools/qemu-test/run.sh"
cp "$CAPTURE" "$TMP/repo/source/host-tools/qemu-test/capture-events.py"
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
#!/usr/bin/env bash
printf '%s\n' "$*" > "$QEMU_TIMED_ARGS"
printf 'TOS.BOOT.ENTRY\r\n'
sleep 0.01
printf 'TOS.CAPSULE.OK\r\n'
printf 'TOS.BOOT.HANDOFF\r\n'
printf 'TOS.NUCLEUS.ENTRY\r\n'
printf 'TOS.CAPSULE.OK\r\n'
printf 'TOS.BOOTTEXT.PATH path=/system/boot/init.tos\r\n'
printf 'TOS.BOOTTEXT.DIGEST sha256=00\r\n'
printf 'TOS.IDENTITY source_kind=detached source_digest=00 capsule_digest=00 arch=x86_64 builder=test\r\n'
printf 'TOS.HALT ok=0x10\r\n'
exit 33
EOF
chmod +x "$TMP/bin/qemu-system-x86_64"

export QEMU_TIMED_ARGS="$TMP/qemu.args"
(cd "$TMP/repo/source" && PATH="$TMP/bin:$PATH" \
    OVMF_CODE="$TMP/OVMF_CODE.fd" OVMF_VARS="$TMP/OVMF_VARS.fd" \
    bash host-tools/qemu-test/run.sh --out "$TMP/out" --capsule "$TMP/capsule.bin" \
        --event-timestamps "$TMP/timestamps.jsonl" --expect 33)

grep -Fq -- '-serial stdio' "$QEMU_TIMED_ARGS" || {
    echo 'FAIL: timed run did not route serial through the shared capture path' >&2
    exit 1
}
grep -Fq 'isa-debug-exit' "$QEMU_TIMED_ARGS" || {
    echo 'FAIL: timed run changed automated isa-debug-exit semantics' >&2
    exit 1
}
python3 - "$TMP/timestamps.jsonl" <<'PY'
import json
import sys

records = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
events = [record["event"] for record in records]
if events[0] != "TOS.BOOT.ENTRY" or "TOS.BOOTTEXT.PATH" not in events:
    raise SystemExit("FAIL: timed harness did not retain the measurement boundaries")
if "TOS.CAPSULE.OK" not in events:
    raise SystemExit("FAIL: timed harness omitted intermediate diagnostic timestamps")
PY

echo 'qemu-timed-harness: PASS'
