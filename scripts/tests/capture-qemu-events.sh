#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for host-monotonic capture of existing QEMU serial events.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CAPTURE="$ROOT/source/host-tools/qemu-test/capture-events.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/fake-qemu" <<'EOF'
#!/usr/bin/env bash
printf 'firmware chatter\r\n'
printf 'TOS.BOOT.ENTRY\r\n'
sleep 0.01
printf 'TOS.CAPSULE.OK\r\n'
sleep 0.01
printf 'TOS.BOOTTEXT.PATH path=/system/boot/init.tos\r\n'
exit 33
EOF
chmod +x "$TMP/fake-qemu"

set +e
python3 "$CAPTURE" \
    --serial-log "$TMP/serial.log" \
    --stderr-log "$TMP/qemu.stderr" \
    --timestamps "$TMP/timestamps.jsonl" \
    --timeout 5 \
    -- "$TMP/fake-qemu"
RC=$?
set -e

[ "$RC" -eq 33 ] || {
    echo "FAIL: capture helper changed QEMU exit status to $RC" >&2
    exit 1
}

python3 - "$TMP/serial.log" "$TMP/timestamps.jsonl" <<'PY'
import json
import sys

serial = open(sys.argv[1], "rb").read()
if b"TOS.BOOT.ENTRY\r\n" not in serial:
    raise SystemExit("FAIL: raw serial log omitted TOS.BOOT.ENTRY")
if b"TOS.BOOTTEXT.PATH" not in serial:
    raise SystemExit("FAIL: raw serial log omitted TOS.BOOTTEXT.PATH")

records = [json.loads(line) for line in open(sys.argv[2], encoding="utf-8")]
events = [record["event"] for record in records]
if events != ["TOS.BOOT.ENTRY", "TOS.CAPSULE.OK", "TOS.BOOTTEXT.PATH"]:
    raise SystemExit(f"FAIL: unexpected timestamped events: {events!r}")
times = [record["monotonic_ns"] for record in records]
if not all(isinstance(value, int) and value > 0 for value in times):
    raise SystemExit("FAIL: timestamps are not positive integer nanoseconds")
if times != sorted(times) or len(set(times)) != len(times):
    raise SystemExit("FAIL: event timestamps are not strictly increasing")
PY

echo 'capture-qemu-events: PASS'
