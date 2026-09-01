#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The human-facing boot console changes nothing.
#
# The boot console is best-effort by contract: the framebuffer may be absent,
# unsupported or unusable, and the boot must be the same boot without it. That
# claim is only worth as much as the machine that tests it, so this boots the
# ordinary artifacts twice on the same profile — once with a display adapter and
# once with none at all, which is what makes the firmware hand over a BootInfo
# declaring no framebuffer — and requires that the two runs agree.
#
# Agreement is checked on the whole `TOS.*` event stream, not on the result code
# alone: identical events, in order, with identical values. Only the fields of
# TOS.BOOT.HANDOFF that describe the platform itself are masked — the runtime
# image's address among them, because where the firmware places an allocation is
# the platform's decision and not the boot's — together with every tick value,
# because a clock that read the same on two different runs would be a clock that
# had stopped. The memory account is masked for the same reason: a machine with
# no display adapter has no framebuffer to describe, so it admits different
# memory and needs fewer page tables to map it, and a reserve that came out the
# same size on both would mean the bound was ignoring the machine.
# A machine without a display adapter genuinely has a different
# framebuffer tuple and a different loader stack address; that difference is the
# input to this test, not a result of it.
#
# `quanta` is deliberately **not** masked. It is how many times a process was
# given the processor, and with one runnable process the answer is one on every
# platform: there is nobody to switch to. A boot where it were not would be a
# scheduler doing something neither run asked for.
#
#   bash host-tools/qemu-test/no-framebuffer.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-target/qemu-no-framebuffer}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

fail() { echo "no-framebuffer: FAIL: $*" >&2; exit 1; }

bash "$HERE/run.sh" --out "$OUT/with" --expect 33 >"$OUT/with.log" 2>&1 ||
    fail "the ordinary boot did not pass; see $OUT/with.log"
bash "$HERE/run.sh" --out "$OUT/without" --no-framebuffer --expect 33 \
    >"$OUT/without.log" 2>&1 ||
    fail "the boot without a framebuffer did not pass; see $OUT/without.log"

# The events, with the platform's own description of itself masked out.
events() {
    tr -d '\r' < "$1" | grep '^TOS\.' |
        sed -E 's/(fb_(format|width|height|pitch)|stack|runtime|available|begin|end|spin_begin|spin_end|ticks|first_tick|last_tick|admitted_frames|table_reserve_frames|table_reserve_free|pool_frames|root_frames|nucleus_space_actual_frames|process_region_mapping_frames|region_backing_frames|total_frames|process_identity_frames|runtime_baseline_frames|tables_free)=[^ ]*/\1=<platform>/g'
}

events "$OUT/with/serial.log" > "$OUT/with.events"
events "$OUT/without/serial.log" > "$OUT/without.events"

# Without this the test could pass on two identical runs that both had a
# framebuffer, and would be evidence for nothing.
grep -q 'TOS\.BOOT\.HANDOFF .*fb_format=0 ' <(tr -d '\r' < "$OUT/without/serial.log") ||
    fail "the second boot still had a framebuffer; the test proves nothing"
grep -q 'TOS\.BOOT\.HANDOFF .*fb_format=[1-9]' <(tr -d '\r' < "$OUT/with/serial.log") ||
    fail "the first boot had no framebuffer; the test proves nothing"

if ! diff -u "$OUT/with.events" "$OUT/without.events" > "$OUT/events.diff"; then
    cat "$OUT/events.diff" >&2
    fail "the boot event stream differs when there is no framebuffer"
fi

COUNT=$(wc -l < "$OUT/with.events")
echo "NO-FRAMEBUFFER PASS: $COUNT boot events identical with and without a framebuffer"
