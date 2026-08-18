#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS Stage 1 QEMU boot test — self-judging harness.
#
# Builds the capsule from system/boot/ sources, lays out an ESP (FAT32 via
# mtools), boots OVMF + QEMU, captures the serial boot-event log and the
# isa-debug-exit result code, then decides pass/fail itself.
#
# Result codes: RESULT_PORT = 0x501, QEMU exits with (code << 1) | 1, so
# HALT_OK 0x10 -> 33, CAPSULE_INVALID 0x21 -> 67, ABI_INVALID 0x22 -> 69.
#
# Exit status of THIS SCRIPT is a verdict, not QEMU's raw code:
#   0  expected result code observed and every required boot event present
#   1  wrong result code, missing/misordered event, or a forbidden event
#   2  environment problem (missing artifact, missing tool)
#   3  QEMU timed out
#
# Usage:
#   bash host-tools/qemu-test/run.sh [OUT_DIR] [CAPSULE_FILE]
#   bash host-tools/qemu-test/run.sh --out DIR [--capsule FILE] [--loader FILE] [--nucleus FILE]
#                                    [--runtime-image FILE] [--no-runtime-image]
#                                    [--expect N]
#                                    [--require "EV ..."] [--forbid "EV ..."]
#                                    [--timeout SECONDS] [--event-timestamps FILE] [--accel tcg|kvm]
#                                    [--interactive --display gtk|sdl] [--no-framebuffer]
#
# --expect defaults to 33 (HALT_OK). --require/--forbid default to the event
# set implied by --expect (see below) and may be overridden for a new scenario.
# Positional arguments are kept so the reproduction command recorded in
# interfaces/boot/STAGE1_REPORT.md keeps working.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# Repository root (two more levels up from source/): identity gate resolves
# repo-relative paths against the current git commit.
GITROOT="$(cd "$ROOT/.." && pwd)"

OUT=""
CAPSULE_IN=""
LOADER_IN=""
NUCLEUS_IN=""
EXPECT=33
REQUIRE=""
FORBID=""
QEMU_TIMEOUT=90
INTERACTIVE=0
DISPLAY_BACKEND=""
EVENT_TIMESTAMPS=""
QEMU_ACCEL=""
NO_FRAMEBUFFER=0

while [ $# -gt 0 ]; do
    case "$1" in
        --out)      OUT="$2"; shift 2 ;;
        --capsule)  CAPSULE_IN="$2"; shift 2 ;;
        --loader)   LOADER_IN="$2"; shift 2 ;;
        --nucleus)  NUCLEUS_IN="$2"; shift 2 ;;
        --runtime-image) RUNTIME_IMAGE_IN="$2"; shift 2 ;;
        --no-runtime-image) NO_RUNTIME_IMAGE=1; shift ;;
        --expect)   EXPECT="$2"; shift 2 ;;
        --require)  REQUIRE="$2"; shift 2 ;;
        --forbid)   FORBID="$2"; shift 2 ;;
        --timeout)  QEMU_TIMEOUT="$2"; shift 2 ;;
        --event-timestamps) EVENT_TIMESTAMPS="$2"; shift 2 ;;
        --accel)    QEMU_ACCEL="$2"; shift 2 ;;
        --no-framebuffer) NO_FRAMEBUFFER=1; shift ;;
        --interactive) INTERACTIVE=1; shift ;;
        --display)  DISPLAY_BACKEND="$2"; shift 2 ;;
        -h|--help)  sed -n '3,28p' "$0"; exit 0 ;;
        --*)        echo "unknown option: $1" >&2; exit 2 ;;
        *)
            # positional: OUT_DIR then CAPSULE_FILE
            if [ -z "$OUT" ]; then OUT="$1"; else CAPSULE_IN="$1"; fi
            shift ;;
    esac
done

if [ "$INTERACTIVE" -eq 1 ]; then
    case "$DISPLAY_BACKEND" in
        gtk|sdl) ;;
        *) echo "--interactive requires --display gtk or --display sdl" >&2; exit 2 ;;
    esac
    if [ -n "$EVENT_TIMESTAMPS" ]; then
        echo "--event-timestamps is not available with --interactive" >&2
        exit 2
    fi
elif [ -n "$DISPLAY_BACKEND" ]; then
    echo "--display is valid only with --interactive" >&2
    exit 2
fi

case "$QEMU_ACCEL" in
    ""|tcg|kvm) ;;
    *) echo "--accel must be tcg or kvm" >&2; exit 2 ;;
esac

OUT="${OUT:-$ROOT/target/qemu-test}"
mkdir -p "$OUT"
# Absolutise: the capsule build runs with `cd "$GITROOT"` for the identity gate,
# so a relative --out would resolve against the repository root instead of the
# caller's directory and the build would fail on a missing directory.
OUT="$(cd "$OUT" && pwd)"

# Default event expectations per result code. The identifiers are the stable
# boot-event log contract (interfaces/boot/BOOT_ABI_V1.md §7); the harness
# checks presence AND relative order, so a boot that halts early cannot pass by
# printing the right final line.
if [ -z "$REQUIRE" ]; then
    case "$EXPECT" in
        33) REQUIRE="TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.CAPSULE.OK TOS.BOOTTEXT.PATH TOS.BOOTTEXT.DIGEST TOS.IDENTITY TOS.HALT" ;;
        67) REQUIRE="TOS.BOOT.ENTRY TOS.BOOT.FAILC" ;;
        *)  REQUIRE="TOS.BOOT.ENTRY" ;;
    esac
fi
if [ -z "$FORBID" ]; then
    case "$EXPECT" in
        # A rejected capsule must fail closed in the loader: control must never
        # reach the nucleus.
        67) FORBID="TOS.NUCLEUS.ENTRY" ;;
        *)  FORBID="TOS.PANIC" ;;
    esac
fi

TOOL="$ROOT/target/release/tos-capsule-tool"
DEFAULT_LOADER="$ROOT/target/x86_64-unknown-uefi/release/tos-uefi-loader.efi"
LOADER="${LOADER_IN:-$DEFAULT_LOADER}"
NUCLEUS="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
NUCLEUS="${NUCLEUS_IN:-$NUCLEUS}"
# The ring-3 runtime image (ADR-0053 option B). It rides the ESP beside the
# capsule and the nucleus, which is where the accepted architecture already
# keeps derived boot binaries.
RUNTIME_IMAGE="$ROOT/target/x86_64-unknown-none/release/tos-runtime-image"
RUNTIME_IMAGE="${RUNTIME_IMAGE_IN:-$RUNTIME_IMAGE}"
# Firmware discovery: the OVMF package installs its files under different names
# per distribution and release (Debian/Ubuntu split CODE/VARS into *_4M.fd only
# from the 2023 packages on). Search a candidate list of CODE/VARS *pairs* —
# mixing a 4M CODE with a 2M VARS gives a firmware that boots to nothing — and
# let the environment override it outright.
if [ -z "${OVMF_CODE:-}" ] || [ -z "${OVMF_VARS:-}" ]; then
    for cand in \
        "/usr/share/OVMF/OVMF_CODE_4M.fd:/usr/share/OVMF/OVMF_VARS_4M.fd" \
        "/usr/share/OVMF/OVMF_CODE.fd:/usr/share/OVMF/OVMF_VARS.fd" \
        "/usr/share/edk2/x64/OVMF_CODE.4m.fd:/usr/share/edk2/x64/OVMF_VARS.4m.fd" \
        "/usr/share/edk2/x64/OVMF_CODE.fd:/usr/share/edk2/x64/OVMF_VARS.fd" \
        "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd:/usr/share/edk2-ovmf/x64/OVMF_VARS.fd" \
        "/usr/share/qemu/OVMF_CODE.fd:/usr/share/qemu/OVMF_VARS.fd"
    do
        c="${cand%%:*}"; v="${cand##*:}"
        if [ -f "$c" ] && [ -f "$v" ]; then
            OVMF_CODE="${OVMF_CODE:-$c}"
            OVMF_VARS="${OVMF_VARS:-$v}"
            break
        fi
    done
fi
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS="${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
echo "firmware: $OVMF_CODE + $OVMF_VARS"

for f in "$TOOL" "$LOADER" "$NUCLEUS" "$OVMF_CODE" "$OVMF_VARS"; do
    [ -f "$f" ] || { echo "missing: $f" >&2; exit 2; }
done
if [ "${NO_RUNTIME_IMAGE:-0}" -eq 0 ] && [ ! -f "$RUNTIME_IMAGE" ]; then
    echo "missing: $RUNTIME_IMAGE" >&2
    exit 2
fi
for t in qemu-system-x86_64 mformat mcopy mmd python3; do
    command -v "$t" >/dev/null || { echo "missing tool: $t" >&2; exit 2; }
done

# --- 1. capsule ---
if [ -n "$CAPSULE_IN" ]; then
    cp "$CAPSULE_IN" "$OUT/capsule.bin"
    echo "capsule: $CAPSULE_IN (used as-is)"
else
    # Identity gate: the capsule is built from the current git commit (HEAD),
    # repo-relative paths, so tos-capsule-tool verifies that init.tos bytes
    # really are the bytes committed at HEAD (source->commit->digest link).
    printf '/system/boot/init.tos\tsource/system/boot/init.tos\n' > "$OUT/manifest.txt"
    ( cd "$GITROOT" && "$TOOL" --git-commit HEAD --licence source/system/boot/NOTICES.txt \
        --out "$OUT/capsule.bin" --meta "$OUT/capsule.meta.json" "$OUT/manifest.txt" )
    # Provenance is release evidence, not a loader input: independently bind
    # the exact capsule, Git blobs and retained notice before making the ESP.
    python3 "$GITROOT/scripts/check-capsule-provenance.py" --root "$GITROOT" \
        --capsule "$OUT/capsule.bin" --manifest "$OUT/capsule.meta.json"
fi

# --- 2. ESP image (FAT32, 64 MiB) ---
ESP="$OUT/esp.img"
rm -f "$ESP"
dd if=/dev/zero of="$ESP" bs=1M count=64 status=none
mformat -F -i "$ESP" ::
mmd -i "$ESP" ::/EFI
mmd -i "$ESP" ::/EFI/BOOT
mcopy -i "$ESP" "$LOADER" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$ESP" "$OUT/capsule.bin" ::/capsule.bin
mcopy -i "$ESP" "$NUCLEUS" ::/nucleus.bin
# Deliberately conditional: a machine with no runtime image is a case the
# nucleus has to report rather than paper over, and the gate for that needs an
# ESP without one.
if [ "${NO_RUNTIME_IMAGE:-0}" -eq 0 ]; then
    mcopy -i "$ESP" "$RUNTIME_IMAGE" ::/runtime.bin
fi

# --- 3. boot ---
cp "$OVMF_CODE" "$OUT/OVMF_CODE.fd"
cp "$OVMF_VARS" "$OUT/OVMF_VARS.fd"
chmod u+w "$OUT/OVMF_CODE.fd" "$OUT/OVMF_VARS.fd"
rm -f "$OUT/serial.log"
QEMU_ARGS=(
    -machine q35
    -cpu qemu64
    -m 256M
    -drive "if=pflash,format=raw,readonly=on,file=$OUT/OVMF_CODE.fd"
    -drive "if=pflash,format=raw,file=$OUT/OVMF_VARS.fd"
    -drive "if=none,id=esp0,format=raw,file=$ESP"
    -device ahci,id=ahci0
    -device ide-hd,bus=ahci0.0,drive=esp0
    -no-reboot
    -monitor none
)
# Acceleration is intentionally opt-in. The ordinary Stage 1 conformance
# profile invokes QEMU without this option and therefore remains qemu64/TCG.
# Research callers can select an alternate backend while retaining this exact
# preparation, firmware, device and event-capture path.
if [ -n "$QEMU_ACCEL" ]; then
    QEMU_ARGS+=( -accel "$QEMU_ACCEL" )
fi
# Without a display adapter the firmware has no GOP, so BootInfo declares the
# framebuffer absent. The machine is otherwise the same one, which is the point:
# the boot has to reach the same result with nothing to draw on.
if [ "$NO_FRAMEBUFFER" -eq 1 ]; then
    QEMU_ARGS+=( -vga none )
fi
if [ "$INTERACTIVE" -eq 0 ]; then
    QEMU_ARGS+=( -device isa-debug-exit )
fi
set +e
if [ "$INTERACTIVE" -eq 1 ]; then
    # Preparation and machine profile are identical to the self-judging path.
    # Deliberately omit isa-debug-exit and timeout: after the production nucleus
    # writes RESULT_HALT_OK it remains in its HLT loop, so the human may inspect
    # the screen and serial output until closing QEMU or pressing Ctrl-C.
    qemu-system-x86_64 "${QEMU_ARGS[@]}" -display "$DISPLAY_BACKEND" \
        -chardev "stdio,id=tosserial,signal=off,logfile=$OUT/serial.log" \
        -serial chardev:tosserial
    RC=$?
else
    if [ -n "$EVENT_TIMESTAMPS" ]; then
        # This opt-in path retains the exact normal QEMU profile and verdict.
        # The helper only observes serial-byte arrival times for existing
        # events; it does not create a guest timing interface.
        python3 "$ROOT/host-tools/qemu-test/capture-events.py" \
            --serial-log "$OUT/serial.log" \
            --stderr-log "$OUT/qemu.stderr" \
            --timestamps "$EVENT_TIMESTAMPS" \
            --timeout "$QEMU_TIMEOUT" \
            -- qemu-system-x86_64 "${QEMU_ARGS[@]}" \
            -serial stdio -display none
    else
        timeout "$QEMU_TIMEOUT" qemu-system-x86_64 "${QEMU_ARGS[@]}" \
            -serial file:"$OUT/serial.log" -display none \
            > "$OUT/qemu.stdout" 2> "$OUT/qemu.stderr"
    fi
    RC=$?
fi
set -e

if [ "$INTERACTIVE" -eq 1 ]; then
    echo "interactive QEMU session ended (status $RC); serial log: $OUT/serial.log"
    exit "$RC"
fi

# --- 4. boot-event log ---
# Strip terminal escape sequences written by the firmware, then keep only the
# TOS event lines. Firmware chatter (BdsDxe:) and the UEFI console string the
# firmware mirrors to serial are deliberately excluded: they do not match
# `^TOS\.`, which is the contract.
EVENTS="$OUT/events.log"
# `tr -d '\r'`: the boot-event log is CRLF-terminated (16550 output), and a
# trailing CR would silently defeat every exact event comparison below.
sed 's/\x1b\[[0-9;=?]*[A-Za-z]//g' "$OUT/serial.log" 2>/dev/null \
    | tr -d '\r' \
    | grep -a '^TOS\.[A-Z0-9_.]*' > "$EVENTS" || true

echo "--- serial boot-event log ---"
cat "$EVENTS" 2>/dev/null || echo "(no TOS events captured)"
echo "-----------------------------"

fail() { echo "QEMU-TEST FAIL: $*" >&2; exit 1; }

if [ "$RC" -eq 124 ]; then
    echo "QEMU-TEST FAIL: timed out after ${QEMU_TIMEOUT}s (no result code written)" >&2
    exit 3
fi

# --- 5. verdict ---
if [ "$RC" -ne "$EXPECT" ]; then
    fail "expected exit code $EXPECT, got $RC"
fi

# Required events must appear in the given order (a sequential scan, so a
# repeated identifier such as TOS.CAPSULE.OK is matched once per requirement).
line_no=0
for ev in $REQUIRE; do
    found=0
    n=0
    while IFS= read -r line; do
        n=$((n + 1))
        [ "$n" -le "$line_no" ] && continue
        case "$line" in
            "$ev"|"$ev "*) line_no=$n; found=1; break ;;
        esac
    done < "$EVENTS"
    [ "$found" -eq 1 ] || fail "missing or out-of-order boot event: $ev"
done

for ev in $FORBID; do
    if grep -q "^$ev\([ ]\|$\)" "$EVENTS"; then
        fail "forbidden boot event present: $ev"
    fi
done

echo "QEMU-TEST PASS: exit $RC as expected; $(wc -w <<<"$REQUIRE") required events in order, $(wc -w <<<"$FORBID") forbidden absent"
exit 0
