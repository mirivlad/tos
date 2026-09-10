#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The x86-64 DMA ordering asymmetry, read out of the built image (ADR-0086 §11).
#
# ADR-0086 accepts an implementation whose two directions are deliberately not
# symmetric, and both halves are claims about emitted code rather than about
# source:
#
#   Publish   compiler ordering barrier, no hardware fence
#   Consume   compiler ordering barrier and LFENCE
#
# It also imposes a standing obligation the Intel proof depends on (§8, §11):
# the store-store rule of SDM Vol. 3A §11.2.2 excepts non-temporal stores, so a
# backend that lowered a `DmaRegion` write to `MOVNT*` would step outside the
# premise the whole publication argument rests on.
#
# **A source grep cannot prove any of that**, which is the reason this gate
# disassembles. A grep would prove nobody *wrote* a non-temporal store; it would
# say nothing about what the compiler emitted, and "LLVM would never" is exactly
# the kind of plausible platform statement ADR-0084 revision 3 was wrong about.
# So the evidence here is the artifact that actually runs.
#
# **What this proves, and what it does not.** The image is a flat binary with no
# symbol table, so the disassembly is a linear sweep of its text range and the
# attribution of the single LFENCE is arithmetic rather than a symbol: there is
# exactly one in the whole image, and `dma_sync` is the only place that emits
# one. If a second legitimate LFENCE ever appears, this gate fails and the count
# must be re-justified deliberately — which is the point of pinning it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
IMAGE="$ROOT/source/target/x86_64-unknown-none/release/tos-runtime-image"

fail() {
    echo "check-dma-ordering-backend: FAIL: $*" >&2
    exit 1
}

command -v objdump >/dev/null 2>&1 ||
    fail "objdump is required to disassemble the runtime image"

(cd "$ROOT/source" && cargo build --release -p tos-runtime-image \
    --target x86_64-unknown-none >/dev/null 2>&1) ||
    fail "the freestanding runtime image does not build"

[ -f "$IMAGE" ] || fail "no runtime image at $IMAGE"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The image's own header says which of its bytes are text (runtime-image
# linker.ld): magic, entry offset, text end, file bytes, memory bytes, as five
# little-endian u64s. Disassembling only that range keeps the sweep away from
# rodata, where data would be decoded as instructions.
python3 - "$IMAGE" "$WORK/text.bin" <<'PY'
import struct
import sys

image = open(sys.argv[1], "rb").read()
magic, entry, text_end, _file_bytes, _memory = struct.unpack_from("<5Q", image, 0)
if magic != 0x534F54494D473100:
    raise SystemExit("the runtime image does not carry its header magic")
if not 0 < entry < text_end <= len(image):
    raise SystemExit("the runtime image header does not bound its own text")
open(sys.argv[2], "wb").write(image[entry:text_end])
PY

objdump -D -b binary -m i386:x86-64 "$WORK/text.bin" > "$WORK/text.asm" 2>/dev/null ||
    fail "the runtime image text could not be disassembled"

instructions="$(grep -c $'\t' "$WORK/text.asm" || true)"
[ "$instructions" -gt 10000 ] ||
    fail "only $instructions instructions disassembled; the text range is wrong"

# ADR-0086 §8. The SDM's store-store exceptions, by mnemonic: MOVNTI, MOVNTQ,
# MOVNTDQ, MOVNTPS, MOVNTPD, and the masked non-temporal stores.
nontemporal="$(grep -icE '\b(movnt[a-z]*|maskmov[a-z]*)\b' "$WORK/text.asm" || true)"
[ "$nontemporal" -eq 0 ] ||
    fail "$nontemporal non-temporal store(s) in the image; a DmaRegion write may not be one (ADR-0086 §8)"

# ADR-0086 §11: Consume emits an execution barrier.
lfence="$(grep -icE '\blfence\b' "$WORK/text.asm" || true)"
[ "$lfence" -eq 1 ] ||
    fail "expected exactly one LFENCE — Consume's — and found $lfence (ADR-0086 §11)"

# And Publish emits no hardware fence. Nothing in this image needs a store or a
# full fence, so any would be a second ordering mechanism nobody decided on.
fences="$(grep -icE '\b(sfence|mfence)\b' "$WORK/text.asm" || true)"
[ "$fences" -eq 0 ] ||
    fail "$fences store/full fence(s) in the image; Publish is compiler-only (ADR-0086 §11)"

echo "check-dma-ordering-backend: OK ($instructions instructions: 1 LFENCE, no store fence, no non-temporal store)"
