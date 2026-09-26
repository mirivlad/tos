#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A `block.device.v1` service ends in the middle of a request, and what its caller
# can know about it.
#
# **ADR-0093 case D and `BLOCK_DEVICE_V1` §6a's incomplete READ, on the accepted
# protocol.** The same canonical supervisor, service and client in four boots; the
# only difference is `/system/policy/fault.tos`, which says where the service
# instance ends:
#
#   none            every request is served                        (the control)
#   before-device   a WRITE's bytes are in device-visible memory; the device has
#                   not been told
#   after-device    the device has performed and completed the WRITE; no reply
#   before-region   a READ is answered with success; the region it owes is not
#                   sent
#
# and two more boots of the control policy against a **device that fails**: QEMU's
# `blkdebug` layer makes the reference endpoint complete the first WRITE, or the
# first READ, of the target sector with a non-OK status — the only way a conforming
# reference machine produces `BLOCK_DEVICE_V1` §7's `BLK_DEVICE` at all; and four
# boots against a **device that lies**, where a test nucleus writes into the
# driver's DMA memory what a bus master could, before the driver reads it.
#
# **What the pair of case-D boots proves is a boundary, not a guarantee.** The
# client's observation is `E_CANCELLED` in both, bit for bit, and the device
# afterwards differs: the nucleus counted no completion interrupt in one and one
# in the other, and the sector the harness reads out of the image once QEMU has
# exited is untouched in one and the client's pattern in the other. A caller whose
# `WRITE` is cancelled cannot tell which happened, and Stage 4 says so rather than
# adding a transaction id, a journal, a retry rule or exactly-once semantics to hide
# it (ADR-0093 §5a, ADR-0098 §3).
#
# **The incomplete READ is observable, and is observed.** The client is answered
# with success — exactly one region is owed — and the service ends before sending
# it. The client's receive is cancelled by the liveness rule rather than satisfied,
# and **no region send happens anywhere in the boot**: nothing was queued on the
# answer endpoint that a later request could take for its own answer.
#
# **How the caller is released is itself the contract** (`SYSTEM_ABI_V1` §6). A
# service that ends holding a call it never answered leaves its caller blocked;
# when nothing is runnable and nothing routed can change that, the nucleus cancels
# every block. The journal must show that census: `routed=0 verdict=stalled`, the
# client's call or receive cancelled, and the supervisor's wait cancelled with it.
#
# **The sector read after the boot is the judge's, not the system's.** It happens
# after QEMU has exited, on the image this harness made, and nothing inside TOS
# reads or is told what it finds.
#
# **Not claimed:** power-loss durability, `VIRTIO_BLK_F_FLUSH`, crash consistency,
# exactly-once writes, a successor after a case-D death (a successor started beside
# the blocked caller would be cancelled at the same instant — `block-lifecycle`
# ends its first instance only after it has answered, and is case C's evidence).
#
#   bash host-tools/qemu-test/block-fault.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-block-fault}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
FIXTURE="$ROOT/tests/vectors/block-fault"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-block-fault"

fail() { echo "block-fault: FAIL: $*" >&2; exit 1; }

cleanup() { rm -rf "$TARGET"; }
trap cleanup EXIT

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)
[ -f "$PRODUCTION" ] || { echo "missing production nucleus: $PRODUCTION" >&2; exit 2; }

# **The accepted protocol boot's endowment, unchanged.** A launcher, a service with
# the PCI root, a client with no hardware authority: exactly what `block-protocol`
# gives, so the only thing these boots add is where the service ends.
before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-block-protocol >/dev/null 2>&1) ||
    fail "the nucleus does not build with test-block-protocol"
[ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
    fail "the production nucleus changed while building the isolated test artifact"
NUCLEUS="$TARGET/x86_64-unknown-none/release/tos-nucleus"

# **The service is the accepted one plus three exits.** Everything a client can
# reach is `block-protocol`'s text; checked here rather than trusted, by removing
# the lines this fixture adds and comparing what is left.
python3 - "$ROOT/tests/vectors/block-protocol/service.tos" "$FIXTURE/service.tos" <<'CHECK' ||
import re, sys
accepted = open(sys.argv[1], encoding="utf-8").read()
fixture = open(sys.argv[2], encoding="utf-8").read()
def body(text):
    # From the module line on, comments and blank lines aside: what executes.
    text = text[text.index("module system.service.block"):]
    lines = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//"):
            continue
        lines.append(stripped)
    return lines
accepted, fixture = body(accepted), body(fixture)
extra = [line for line in fixture if line not in accepted]
missing = [line for line in accepted if line not in fixture]
allowed_missing = {"const CONFORMANCE_REQUESTS: u64 = 10u64;",
                   "const REPEATED_READS: u64 = 12u64;",
                   "const REQUESTS: u64 = 22u64;"}
if set(missing) - allowed_missing:
    sys.exit("the fixture lost accepted lines: %r" % sorted(set(missing) - allowed_missing))
expected_extra = {
    "import system.policy.fault as fault;",
    "const REQUESTS: u64 = 2u64;",
    "const FAULT_BEFORE_DEVICE: u64 = 1u64;",
    "const FAULT_AFTER_DEVICE: u64 = 2u64;",
    "const FAULT_BEFORE_REGION: u64 = 3u64;",
    "const ENDED_BY_POLICY: i64 = -999999i64;",
    "const PROVED_ENDED_BY_POLICY: i64 = 4294967296i64;",
    "if (answered == ENDED_BY_POLICY) {",
    "return proved | PROVED_ENDED_BY_POLICY;",
    "if (writing && fault.point() == FAULT_BEFORE_DEVICE) {",
    "return ENDED_BY_POLICY;",
    "if (fault.point() == FAULT_AFTER_DEVICE) {",
    "if (fault.point() == FAULT_BEFORE_REGION) {",
}
if set(extra) - expected_extra:
    sys.exit("the fixture adds lines beyond its exits: %r" % sorted(set(extra) - expected_extra))
CHECK
    fail "the fault service is not the accepted service plus its three exits"

# The client's pattern, byte for byte, as `client.tos` computes it.
pattern_hex() {
    python3 -c 'print(bytes(((i * 61 + 29) % 251 + 1) for i in range(512)).hex())'
}
PATTERN="$(pattern_hex)"
ZEROS="$(python3 -c 'print(bytes(512).hex())')"

# What sector 8 of the image holds once the machine has stopped.
sector8() {
    python3 - "$OUT/$1/stage4-block.img" <<'READ'
import sys
with open(sys.argv[1], "rb") as image:
    image.seek(8 * 512)
    print(image.read(512).hex())
READ
}

boot() {
    local policy="$1" name="${2:-$1}"
    shift $(( $# < 2 ? $# : 2 ))
    {
        printf '/system/boot/init.tos\t%s/init.tos\n' "$FIXTURE"
        printf '/system/service/block.tos\t%s/service.tos\n' "$FIXTURE"
        printf '/system/client/block.tos\t%s/client.tos\n' "$FIXTURE"
        printf '/system/policy/fault.tos\t%s/fault-%s.tos\n' "$FIXTURE" "$policy"
    } > "$OUT/$name.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$OUT/$name.bin" --meta "$OUT/$name.meta.json" "$OUT/$name.txt" >/dev/null
    python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
        --capsule "$OUT/$name.bin" --manifest "$OUT/$name.meta.json" >/dev/null
    bash "$HERE/run.sh" \
        --out "$OUT/$name" \
        --capsule "$OUT/$name.bin" \
        --nucleus "$NUCLEUS" \
        --stage4-block-device "$@" \
        --expect 33 \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
        --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED TOS.RUN.PROCESS_DEADLOCKED" \
        > /dev/null || fail "the $name boot did not complete"
    grep -q '^TOS\.RUN\.BEGIN path=system/service/block\.tos .* modules=4$' "$OUT/$name/events.log" ||
        fail "$name: the service did not run as one of a set of four modules"
}

# The three accounts of one boot, by module: the client's, the supervisor's and
# the service's. **Told apart by what each can say**, because the nucleus prints
# every process's `COMPLETED` in the order they finish and not beside anything
# naming the process: the service's account carries its device facts and is at
# least 2^20, or is the negative step it refused at; the supervisor's is one of 15
# and 31; and no combination of the client's bits is either. Three values, each claimed by exactly one reader,
# or the boot is not the one this gate knows how to read.
accounts() {
    python3 - "$OUT/$1/events.log" <<'READ'
import re, sys
values = [int(m.group(1)) for l in open(sys.argv[1], encoding="utf-8", errors="replace")
          for m in [re.match(r"^TOS\.RUN\.COMPLETED value=i64:(-?\d+)\s*$", l)] if m]
if len(values) != 3:
    sys.exit("%d account(s); three processes end" % len(values))
service = [v for v in values if v >= 1 << 20 or v < 0]
supervisor = [v for v in values if v in (15, 31)]
client = [v for v in values if v not in service and v not in supervisor]
if len(service) != 1 or len(supervisor) != 1 or len(client) != 1:
    sys.exit("cannot tell the accounts apart: %r" % values)
print(client[0], supervisor[0], service[0])
READ
}

# How many completion interrupts the nucleus delivered to the service's source.
deliveries() {
    sed -n 's/^TOS\.RUN\.IRQ_RELEASED .* deliveries=\([0-9]*\) .*/\1/p' "$OUT/$1/events.log"
}

region_sends() {
    grep -c '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$' "$OUT/$1/events.log" || true
}

# The service's account carries 2^32 exactly when it ended by policy, and 2^31
# when the device itself failed a request.
ENDED_BY_POLICY=4294967296
PROVED_REFUSED_DEVICE=2147483648

# --- the control ----------------------------------------------------------------
boot none
read -r client supervisor service <<< "$(accounts none)"
[ "$client" = 53 ] || fail "none: the client observed $client; a served WRITE, a served READ, its region and every byte is 53"
[ "$supervisor" = 15 ] || fail "none: the supervisor observed $supervisor; two creations and two endings with no stall is 15"
[ $((service & ENDED_BY_POLICY)) = 0 ] || fail "none: the service ended by policy with no policy to end by"
[ "$(deliveries none)" = 2 ] || fail "none: $(deliveries none) completion(s); a write and a read are two"
[ "$(region_sends none)" = 1 ] || fail "none: $(region_sends none) region send(s); one READ owes one"
! grep -q '^TOS\.RUN\.BLOCK_CANCELLED ' "$OUT/none/events.log" ||
    fail "none: a block was cancelled in a boot where every request was answered"
[ "$(sector8 none)" = "$PATTERN" ] || fail "none: sector 8 does not hold the client's pattern"
echo "block-fault: control — WRITE and READ served, one region, every byte, no stall"

# --- case D, from both sides ----------------------------------------------------
stalled_call() {
    local policy="$1" operation="$2"
    grep -q '^TOS\.RUN\.LIVENESS blocked=2 routed=0 verdict=stalled asserted_by=nucleus$' "$OUT/$policy/events.log" ||
        fail "$policy: the liveness census did not find two blocks and nothing routed"
    grep -q "^TOS\\.RUN\\.BLOCK_CANCELLED process=[0-9]* operation=$operation endpoint=[0-9]* reason=no-runnable-context asserted_by=nucleus\$" "$OUT/$policy/events.log" ||
        fail "$policy: the client's operation $operation was not cancelled by the liveness rule"
    grep -q '^TOS\.RUN\.BLOCK_CANCELLED process=[0-9]* operation=14 endpoint=0 reason=no-runnable-context asserted_by=nucleus$' "$OUT/$policy/events.log" ||
        fail "$policy: the supervisor's wait was not cancelled with it"
}

observations=()
for policy in before-device after-device; do
    boot "$policy"
    read -r client supervisor service <<< "$(accounts "$policy")"
    [ "$client" = 2 ] || fail "$policy: the client observed $client; a cancelled WRITE and nothing else is 2"
    [ "$supervisor" = 31 ] || fail "$policy: the supervisor observed $supervisor; both endings and one stall is 31"
    [ $((service & ENDED_BY_POLICY)) != 0 ] || fail "$policy: the service did not end at its policy's point"
    stalled_call "$policy" 3
    [ "$(region_sends "$policy")" = 0 ] || fail "$policy: a region was sent in a boot with no READ"
    observations+=("$client")
done
[ "$(deliveries before-device)" = 0 ] ||
    fail "before-device: the nucleus delivered $(deliveries before-device) completion(s); the device was never told"
[ "$(sector8 before-device)" = "$ZEROS" ] ||
    fail "before-device: sector 8 changed although the device was never told to write it"
[ "$(deliveries after-device)" = 1 ] ||
    fail "after-device: the nucleus delivered $(deliveries after-device) completion(s); the write completed once"
[ "$(sector8 after-device)" = "$PATTERN" ] ||
    fail "after-device: sector 8 does not hold the pattern the completed write carried"
# **The boundary itself.** Two device states, one observation.
[ "${observations[0]}" = "${observations[1]}" ] ||
    fail "the two case-D boots gave the caller different observations (${observations[*]}); case D would then be decidable"
echo "block-fault: case D — the caller observed ${observations[0]} both times; the device" \
     "was untouched once (0 completions, sector zero) and written once (1 completion," \
     "the pattern on the image). The caller cannot tell them apart, and nothing claims it can"

# --- the incomplete READ ----------------------------------------------------------
boot before-region
read -r client supervisor service <<< "$(accounts before-region)"
[ "$client" = 69 ] ||
    fail "before-region: the client observed $client; a served WRITE, a READ answered with success and its receive cancelled is 69"
[ "$supervisor" = 31 ] || fail "before-region: the supervisor observed $supervisor; both endings and one stall is 31"
[ $((service & ENDED_BY_POLICY)) != 0 ] || fail "before-region: the service did not end at its policy's point"
stalled_call before-region 2
[ "$(deliveries before-region)" = 2 ] ||
    fail "before-region: $(deliveries before-region) completion(s); the READ reached the device"
[ "$(region_sends before-region)" = 0 ] ||
    fail "before-region: a region was sent although the service ended before sending the one it owed"
grep -q '^TOS\.RUN\.INTERFACE operation=endpoint_receive_region status=-5$' "$OUT/before-region/events.log" ||
    fail "before-region: the client's receive for the owed region did not end in E_CANCELLED"
echo "block-fault: incomplete READ — answered with success, the owed region never sent," \
     "the receive cancelled by the liveness rule, and nothing left queued"

# --- the device fails, and the service does not -----------------------------------
#
# **A real device failure, through the real ring.** QEMU's `blkdebug` layer fails
# the first WRITE of sector 8 in one boot and the first READ of it in the other, and
# the reference endpoint completes that request with a non-OK status. Until these
# boots `BLOCK_DEVICE_V1` §7's `BLK_DEVICE` was implemented and never reached — and
# reaching it found that the service advanced its ring counters only for requests
# that *succeeded*, so the request after a device failure was published into the
# same slot at the same index and waited forever for a completion the device had no
# reason to send. The count now includes a request the device failed.
#
# What must hold: the failure is a **refusal**, answered with `BLK_DEVICE`; the
# service goes on serving; a refused READ owes no region and none is sent; and the
# image agrees with what the client was told.
boot none device-fails-write --stage4-block-fault "$FIXTURE/device-fails-write.txt"
read -r client supervisor service <<< "$(accounts device-fails-write)"
[ "$client" = 660 ] ||
    fail "device-fails-write: the client observed $client; a WRITE refused by the device, then a READ served with a region of zeros, is 660"
[ "$supervisor" = 15 ] || fail "device-fails-write: the supervisor observed $supervisor; nothing stalled, so 15"
[ $((service & PROVED_REFUSED_DEVICE)) != 0 ] || fail "device-fails-write: the service did not report a device refusal"
[ "$(deliveries device-fails-write)" = 2 ] ||
    fail "device-fails-write: $(deliveries device-fails-write) completion(s); the failed write and the read are two"
[ "$(region_sends device-fails-write)" = 1 ] || fail "device-fails-write: the READ after the failure was not answered with its region"
[ "$(sector8 device-fails-write)" = "$ZEROS" ] || fail "device-fails-write: the refused write reached the image"

boot none device-fails-read --stage4-block-fault "$FIXTURE/device-fails-read.txt"
read -r client supervisor service <<< "$(accounts device-fails-read)"
[ "$client" = 257 ] ||
    fail "device-fails-read: the client observed $client; a WRITE served, then a READ refused by the device, is 257"
[ "$supervisor" = 15 ] || fail "device-fails-read: the supervisor observed $supervisor; nothing stalled, so 15"
[ $((service & PROVED_REFUSED_DEVICE)) != 0 ] || fail "device-fails-read: the service did not report a device refusal"
[ "$(region_sends device-fails-read)" = 0 ] || fail "device-fails-read: a region followed a refused READ"
[ "$(sector8 device-fails-read)" = "$PATTERN" ] || fail "device-fails-read: the served write is not on the image"
echo "block-fault: device failures — a failed WRITE and a failed READ are each refused with" \
     "BLK_DEVICE, the service serves the next request, a refused READ owes and sends no" \
     "region, and the image agrees with what the client was told"

# --- the device lies, and the driver does not believe it ----------------------------
#
# **`docs/34` T7, for one build at a time.** `test-hostile-device` makes the nucleus
# write bytes the harness chose into the DMA region of the delivering function's
# assignment at a named delivery, before the driver is woken — what a bus master of
# that function could do, and the one thing the reference endpoint never does. The
# nucleus knows an offset and a byte; which offset is a used-ring length is computed
# here, from the service's own constants and the layout rule its text states, and a
# wrong offset would show as a refusal code other than the one asserted.
#
# Each lie must be refused **at the step that checks it**, by the code the service's
# text gives that step, with nothing handed to the client: the service gives the
# device up and ends, and the client's pending call is released by the liveness rule.
python3 - "$FIXTURE/service.tos" > "$OUT/layout.txt" <<'LAYOUT' || fail "the service's queue layout could not be read"
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
def const(name, suffix):
    m = re.search(r"^const %s: \w+ = (\d+)%s;$" % (name, suffix), text, re.M)
    if not m:
        sys.exit("no constant " + name)
    return int(m.group(1))
cap = const("QUEUE_CAP", "u64")
# The reference endpoint's queue 0 offers 256 entries (`STAGE4B_MMIO_BOUNDARY.md`,
# `queue0_size=256`), and the service takes the smaller of that and its own cap.
chosen = min(cap, 256)
align = lambda value, to: (value + to - 1) // to * to
avail = align(16 * chosen, 2)
used = align(avail + 6 + 2 * chosen, 4)
queue_end = used + 6 + 8 * chosen
header = align(queue_end, 16)
data = align(header + const("HEADER_BYTES", "B"), 16)
# A used element is `le32 id, le32 len`, after `le16 flags, le16 idx`.
print("USED_ID_SLOT0=%d" % (used + 4 + 8 * 0))
print("USED_LEN_SLOT1=%d" % (used + 8 + 8 * 1))
print("DATA=%d" % data)
print("SENTINEL=%d" % const("DATA_SENTINEL", "u64"))
LAYOUT
# shellcheck source=/dev/null
. "$OUT/layout.txt"

hostile() {
    local name="$1" rule="$2"
    (cd "$ROOT" && TOS_HOSTILE_DEVICE="$rule" CARGO_TARGET_DIR="$HOSTILE" cargo build --release \
        -p tos-nucleus --target x86_64-unknown-none \
        --features test-block-protocol,test-hostile-device >/dev/null 2>&1) ||
        fail "the nucleus does not build with test-hostile-device"
    [ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
        fail "the production nucleus changed while building the hostile-device artifact"
    NUCLEUS="$HOSTILE/x86_64-unknown-none/release/tos-nucleus" boot none "$name"
    grep -q "^TOS\.RUN\.HOSTILE_DEVICE .* written=1 asserted_by=test-adversary\$" "$OUT/$name/events.log" ||
        fail "$name: the adversary wrote nothing"
}
HOSTILE="$ROOT/target/test-block-fault-hostile"
trap 'rm -rf "$TARGET" "$HOSTILE"' EXIT

# name                  rule (delivery:offset:byte[:count])   refused at   client
for lie in \
    "hostile-id:1:$USED_ID_SLOT0:7:-53:2" \
    "hostile-long:2:$USED_LEN_SLOT1:255:-59:9" \
    "hostile-short:2:$USED_LEN_SLOT1:0:-54:9" \
    "hostile-nothing:2:$DATA:$SENTINEL:512:-56:9"
do
    IFS=: read -r name delivery offset byte rest <<< "$lie"
    case "$name" in
        hostile-nothing) IFS=: read -r count refused expected <<< "$rest"; rule="$delivery:$offset:$byte:$count" ;;
        *) IFS=: read -r refused expected <<< "$rest"; rule="$delivery:$offset:$byte" ;;
    esac
    hostile "$name" "$rule"
    read -r client supervisor service <<< "$(accounts "$name")"
    [ "$service" = "$refused" ] ||
        fail "$name: the service reported $service; the step that checks this lie refuses with $refused"
    [ "$client" = "$expected" ] ||
        fail "$name: the client observed $client; with the device given up it is $expected"
    [ "$supervisor" = 31 ] || fail "$name: the supervisor observed $supervisor, not both endings and one stall"
    [ "$(region_sends "$name")" = 0 ] || fail "$name: a region was sent after the device lied"
done
echo "block-fault: hostile device — a completion naming a chain the driver never made (-53)," \
     "a length past the buffers (-59) and short of them (-54), and a read that wrote nothing" \
     "(-56) are each refused at the step that checks them, and no byte reaches the client"

echo "block-fault: PASS (ADR-0093 case D as a stated boundary; BLOCK_DEVICE_V1 §6a's incomplete READ; BLK_DEVICE from a failing device; a lying device refused)"
