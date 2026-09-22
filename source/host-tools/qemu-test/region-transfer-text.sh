#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# An ordinary immutable region crosses IPC between two canonical textual
# processes, and every byte of it arrives (ADR-0097 §9).
#
# **This is the conformance gate ADR-0097 requires**, and it is the third of the
# three conditions `check-stage4-data-path-claims.sh` needs before it stops
# holding the Stage 4 claim bound. The other two are an accepted schema row with
# an ordinary `Region<...>` parameter and a `MESSAGE_REGIONS` reference from
# always-compiled bridge code; this is the one that had to be a gate rather than
# a declaration, because in this project evidence follows a passing gate.
#
#   sender: region_allocate -> write a pattern -> region_freeze
#             -> endpoint_send_region  (MESSAGE_REGIONS[0], count in r8)
#   reader: endpoint_receive_region -> index every byte
#
# **The pattern is `i * 7 + 11` modulo 256**, checked byte by byte in canonical
# text. A reader that compared one byte, or that was off by one in either
# direction, would not find it; the number it reports is how many of 512 bytes
# agreed, and this gate requires all 512. Nothing outside the textual modules
# inspects the bytes.
#
# **The negatives are separate boots and each refuses for its own reason**, which
# is what keeps them from being one test wearing three hats.
#
#   bash host-tools/qemu-test/region-transfer-text.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-region-transfer-text}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/region-transfer-text"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-region-transfer-text"
WORK="$OUT/source"

# 1 allocated, 2 pattern written and read back, 4 frozen, 8 child created,
# 16 sent, 32 and the handle no longer resolves.
EXPECTED_SENDER="i64:63"
# Every byte of the pattern.
EXPECTED_READER="i64:512"
PATTERN_BYTES=512
# A boot in which a module is refused halts with its own code rather than the
# success one, which is what the harness reads. `module-operation.sh` names it the
# same way and for the same reason: a refusal is the expected outcome of these two
# boots, so expecting the success code would be expecting the refusal not to
# happen.
REFUSED_EXIT=$(( (0x25 << 1) | 1 ))

fail() {
    echo "region-transfer-text: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-region-transfer-text)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

# Each case is the same two modules with one edit, applied to a copy. Written as
# edits rather than as a fixture per case for the reason the publication gate
# gives: separate copies drift, and then the cases differ in more than the one
# thing under test.
prepare() {
    rm -rf "$WORK"
    mkdir -p "$WORK"
    cp "$FIXTURE"/*.tos "$WORK/"
}

# Applied to the copy, and required to change something: an edit whose anchor has
# moved would otherwise leave the case booting the unmutated fixture and passing
# for the wrong reason, which is how the first version of this gate reported a
# green negative.
edit() {
    python3 - "$WORK/$1" || fail "the edit for this case matched nothing in $1"
}

build_capsule() {
    printf '/system/boot/init.tos\t%s/init.tos\n' "$WORK" > "$OUT/manifest-$1.txt"
    printf '/system/client/reader.tos\t%s/reader.tos\n' "$WORK" >> "$OUT/manifest-$1.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$OUT/$1.bin" --meta "$OUT/$1.meta.json" "$OUT/manifest-$1.txt" > /dev/null
}

boot() {
    # $1 = name, $2 = extra --forbid terms, $3 = extra --require terms,
    # $4 = the exit code this boot is expected to halt with, $5 = the event it
    # ends on — a boot whose entry module is refused ends `TOS.BOOTMODULE.FAIL`
    # rather than `TOS.HALT`, and requiring the wrong one would be requiring the
    # refusal not to have happened
    mkdir -p "$OUT/$1"
    bash "$HERE/run.sh" \
        --out "$OUT/$1" \
        --capsule "$OUT/$1.bin" \
        --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
        --expect "${4:-33}" \
        --require "TOS.NUCLEUS.ENTRY $3 ${5:-TOS.HALT}" \
        --forbid "TOS.EXCEPTION TOS.PANIC $2" \
        > /dev/null
}

# --- the positive ---------------------------------------------------------------
prepare
build_capsule positive
boot positive "TOS.RUN.TRAP TOS.RUN.REFUSED TOS.RUN.UNSTARTABLE" "TOS.RUN.COMPLETED"
LOG="$OUT/positive/events.log"
count() { grep -c "$1" "$LOG" || true; }

grep -q '^TOS\.RUN\.BEGIN .* modules=2$' "$LOG" ||
    fail "the positive boot did not run a set of two modules"

# The four new operations, each once, and each answering OK.
for operation in region_allocate region_freeze endpoint_send_region endpoint_receive_region; do
    [ "$(count "^TOS\.RUN\.INTERFACE operation=$operation status=0\$")" = 1 ] ||
        fail "$operation did not succeed exactly once: $(grep "operation=$operation" "$LOG" || echo absent)"
done

# **Every byte.** The reader counted them in canonical text and this reads its
# account; a single corrupted byte lowers the number and fails here.
[ "$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_READER\$")" = 1 ] ||
    fail "the reader did not report all $PATTERN_BYTES bytes of the pattern: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_SENDER\$")" = 1 ] ||
    fail "the sender did not report every step including losing the region: $(grep COMPLETED "$LOG")"

# And the sender's own release of the transferred handle was refused, which is the
# nucleus saying the region is not its any more. Two releases appear in this boot —
# the endpoint's receiving name, which succeeds, and the region's, which must not.
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')" = 1 ] ||
    fail "the endpoint's receiving name was not released exactly once"
[ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release status=0$')" = 1 ] &&
    [ "$(count '^TOS\.RUN\.INTERFACE operation=capability_release ')" = 2 ] ||
    fail "the transferred region's release was not attempted and refused"

# --- negative 1: a writable region cannot cross IPC -----------------------------
# `IPC_V1` §5: "a writable region handle may not be delegated or sent at all, and
# a send that names one is refused whole". The edit **bypasses the freeze** rather
# than following it — a region consumed by `region_freeze` is gone, and using it
# afterwards would prove affinity instead of this. So a helper hands the mutable
# region straight to the send.
#
# **And the refusal is the nucleus's, not the frontend's**, which is where §5 puts
# the rule and is what this case therefore asserts. `RegionFamily` admits both
# members at a capability position — that is what a family is — so the module
# compiles and `send_transaction` answers `E_NO_CAPABILITY`. A frontend refusal
# would have been a weaker result: it would prove this schema has no row for it
# rather than that the transport refuses it.
prepare
edit init.tos <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
anchor = "            match (region_freeze(region)) {"
if anchor not in text:
    raise SystemExit(1)
text = text.replace(anchor, "            match (do_not_freeze(region)) {", 1)
text += '''
// The mutation's helper: the region, unfrozen, at the position a frozen one
// belongs. Nothing about it is unusual — it is an ordinary function returning an
// affine value — and that is the point: the refusal has to come from the send.
fn do_not_freeze(region: Region<mut u8>) -> Result<Region<mut u8>, i64> {
    return Ok(region);
}
'''
path.write_text(text)
PY
build_capsule writable
boot writable "" "TOS.RUN.COMPLETED"
LOG="$OUT/writable/events.log"
# The send was made and refused `E_NO_CAPABILITY`, and nothing else was.
[ "$(grep -c '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=-1$' "$LOG" || true)" = 1 ] ||
    fail "sending a writable region was not refused E_NO_CAPABILITY: $(grep endpoint_send_region "$LOG" || echo 'the send was never attempted')"
[ "$(grep -c '^TOS\.RUN\.INTERFACE operation=endpoint_send_region status=0$' "$LOG" || true)" = 0 ] ||
    fail "a writable region crossed IPC"
# The sender stopped at its own send-failed code, so nothing downstream ran.
[ "$(grep -c '^TOS\.RUN\.COMPLETED value=i64:-40$' "$LOG" || true)" = 1 ] ||
    fail "the sender did not report the send it made being refused: $(grep COMPLETED "$LOG")"
# And **no region arrived**: the reader's wait was ended by the liveness rule
# rather than by a message, which is `E_CANCELLED` through its own refusal path.
[ "$(grep -c '^TOS\.RUN\.COMPLETED value=i64:-1005$' "$LOG" || true)" = 1 ] ||
    fail "the reader did not report an empty wait: $(grep COMPLETED "$LOG")"
[ "$(grep -c "^TOS\.RUN\.COMPLETED value=$EXPECTED_READER\$" "$LOG" || true)" = 0 ] ||
    fail "the reader read a pattern out of a region that was never sent"

# --- negative 2: an ordinary region is refused where a DMA region belongs -------
# The two families are disjoint and each has one interface (§4.3 rule 1), so a
# capability position of `platform.dma.Region` does not take a `Region<u8>` — and
# the reader has no DMA region to offer it, which is exactly the point: a client
# holding no device authority cannot reach a device operation by holding a region.
prepare
edit reader.tos <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
anchor = "            match (to_i64(agreeing(borrow region))) {"
if anchor not in text:
    raise SystemExit(1)
text = text.replace(
    "extern fn endpoint_receive_region(",
    "extern fn dma_device_address(\n"
    "    cap: platform.dma.Region, at: size\n"
    ") -> Result<u64, i64> uses [platform.dma.Region];\n\n"
    "extern fn endpoint_receive_region(",
    1,
)
text = text.replace(
    anchor,
    "            match (dma_device_address(region, 0B)) {\n"
    "                Ok(address) => { return -2i64; }\n"
    "                Err(refused) => { return -3i64; }\n"
    "            }\n" + anchor,
    1,
)
path.write_text(text)
PY
build_capsule wrong_family
boot wrong_family "" ""
LOG="$OUT/wrong_family/events.log"
grep -q '^TOS\.RUN\.REFUSED stage=' "$LOG" ||
    fail "an ordinary region at a DMA capability position was not refused: $(grep -E 'DIAGNOSTIC|COMPLETED' "$LOG" | head -3)"
# And the reader never ran, so it reported neither of its own answers.
for reported in -2 -3; do
    [ "$(grep -c "^TOS\.RUN\.COMPLETED value=i64:$reported\$" "$LOG" || true)" = 0 ] ||
        fail "the reader reached a DMA operation it holds no DMA region for"
done

# --- negative 3: an old language minor does not receive the new rule ------------
# ADR-0085 §13's rule, applied to ADR-0097's member: a 1.4 module naming
# `system.memory.Region` is refused against the minor *this* feature needs, so it
# is told 5 rather than 3. A module told 3 would be told to declare a version
# under which the form is still invalid.
prepare
edit init.tos <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
if "version 1.5 profile full" not in text:
    raise SystemExit(1)
path.write_text(text.replace("version 1.5 profile full", "version 1.4 profile full", 1))
PY
build_capsule old_minor
boot old_minor "" "" "$REFUSED_EXIT" "TOS.BOOTMODULE.FAIL"
LOG="$OUT/old_minor/events.log"
grep -q '^TOS\.RUN\.DIAGNOSTIC E1608_FEATURE_REQUIRES_LANGUAGE_MINOR .*requires=5' "$LOG" ||
    fail "a 1.4 module naming system.memory.Region was not refused against minor 5: $(grep DIAGNOSTIC "$LOG" | head -3)"
if grep -q 'requires=3' "$LOG"; then
    fail "the refusal named the DMA family's minor rather than this one's"
fi
# And the sender did not run: a 1.4 module naming this family is refused whole.
[ "$(grep -c "^TOS\.RUN\.COMPLETED value=$EXPECTED_SENDER\$" "$LOG" || true)" = 0 ] ||
    fail "a 1.4 module naming system.memory.Region ran anyway"

echo "REGION-TRANSFER-TEXT PASS: an ordinary region crossed IPC from canonical text"
echo "  sender: region_allocate -> a pattern -> region_freeze ->"
echo "  endpoint_send_region, through MESSAGE_REGIONS and its own count register"
echo "  reader: endpoint_receive_region, holding one receiving name and no memory"
echo "  authority — it could not have made a region — and it indexed the bytes"
echo "  at an address the nucleus chose and this module never learns"
echo "  all $PATTERN_BYTES bytes of \`i * 7 + 11 mod 256\` agreed, counted in canonical"
echo "  text: one corrupted byte lowers the count and fails this gate"
echo "  the transfer is linear: the sender's release of the handle it sent was"
echo "  refused, because a successful send took it (IPC_V1 §5, ADR-0075 §5a)"
echo "  negatives, each its own boot and each refusing for its own reason:"
echo "    a writable Region<mut u8> cannot be sent at all"
echo "    an ordinary region is refused at a platform.dma.Region position"
echo "    a 1.4 module naming system.memory.Region is refused against minor 5"
echo "  NOT claimed: a second region in one message, a region of another element"
echo "  type, zero-copy, or anything about a device"
