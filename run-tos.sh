#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Human-facing launcher. Boot preparation and QEMU configuration remain
# authoritative in source/host-tools/qemu-test/run.sh, and the Stage 3 scenario
# remains the one source/host-tools/qemu-test/supervision.sh builds and boots —
# this runs those, it does not reimplement them.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
SOURCE="$ROOT/source"
MODE=interactive
DISPLAY_BACKEND=
WANT_DISPLAY=1

usage() {
    cat <<'EOF'
Usage: ./run-tos.sh [--check | --stage3 [--interactive]]

  (no option)   build and boot TOS interactively with a QEMU display and serial output
  --check       build and run the deterministic automated QEMU self-check
  --stage3      boot the Stage 3 system: a supervisor written in TOS Core,
                canonical policy from /system/policy/services.tos, real child
                processes, restarts, BLOCKED and terminal FAILED
  --interactive with --stage3, also open the QEMU boot display. The boot console
                shows the boot, not the supervision story; the story is printed
                on the terminal after you close QEMU or press Ctrl-C
  -h, --help    show this help
EOF
}

case "${1-}" in
    "") ;;
    --check) MODE=check; WANT_DISPLAY=0 ;;
    --stage3) MODE=stage3; WANT_DISPLAY=0 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "run-tos: unknown option: $1" >&2; usage >&2; exit 2 ;;
esac
if [ "$MODE" = stage3 ] && [ "${2-}" = --interactive ]; then
    WANT_DISPLAY=1
    shift
fi
[ "$#" -le 1 ] || { echo "run-tos: too many arguments" >&2; usage >&2; exit 2; }

command -v cargo >/dev/null 2>&1 || {
    echo "run-tos: missing tool: cargo" >&2
    exit 2
}
command -v rustup >/dev/null 2>&1 || {
    echo "run-tos: missing tool: rustup" >&2
    exit 2
}

cd "$SOURCE"
if ! installed_targets=$(rustup target list --installed 2>&1); then
    echo "run-tos: required Rust toolchain from source/rust-toolchain.toml is unavailable" >&2
    printf '%s\n' "$installed_targets" >&2
    exit 2
fi
for target in x86_64-unknown-uefi x86_64-unknown-none; do
    printf '%s\n' "$installed_targets" | grep -Fxq "$target" || {
        echo "run-tos: missing Rust target: $target" >&2
        echo "run-tos: install it explicitly with: rustup target add $target" >&2
        exit 2
    }
done

if [ "$WANT_DISPLAY" = 1 ]; then
    command -v qemu-system-x86_64 >/dev/null 2>&1 || {
        echo "run-tos: missing tool: qemu-system-x86_64" >&2
        exit 2
    }
    display_help=$(qemu-system-x86_64 -display help 2>&1) || {
        echo "run-tos: could not inspect QEMU display backends" >&2
        printf '%s\n' "$display_help" >&2
        exit 2
    }
    if printf '%s\n' "$display_help" | grep -Fxq gtk; then
        DISPLAY_BACKEND=gtk
    elif printf '%s\n' "$display_help" | grep -Fxq sdl; then
        DISPLAY_BACKEND=sdl
    else
        echo "run-tos: QEMU has no graphical display backend" >&2
        echo "run-tos: on Debian/MX install qemu-system-gui" >&2
        echo "run-tos: use ./run-tos.sh --check for headless verification" >&2
        exit 2
    fi
    if [ -z "${DISPLAY-}" ] && [ -z "${WAYLAND_DISPLAY-}" ]; then
        echo "run-tos: interactive mode requires a graphical session (DISPLAY or WAYLAND_DISPLAY)" >&2
        echo "run-tos: use ./run-tos.sh --check for the headless automated boot check" >&2
        exit 2
    fi
fi

cargo build --release -p tos-capsule-tool
cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi
cargo build --release -p tos-nucleus --target x86_64-unknown-none

# The Stage 3 scenario boots the **production** runtime image. It is built here
# rather than assumed, because the shared target directory is also where the
# evidence builds put their feature-gated images, and a demonstration that
# booted whichever one happened to be there last would be showing something
# other than the system. The gate profile builds it for the same reason.
if [ "$MODE" = stage3 ]; then
    cargo build --release -p tos-runtime-image --target x86_64-unknown-none
fi

if [ "$MODE" = check ]; then
    exec bash host-tools/qemu-test/run.sh --out target/run-tos/check --expect 33
fi

if [ "$MODE" = stage3 ]; then
    OUT="$SOURCE/target/run-tos/stage3"
    # Last run's evidence goes before this run starts. An interactive session
    # writes a serial log and no event log, so a stale one left in place would
    # be shown as though this boot had produced it.
    rm -f "$OUT/serial.log" "$OUT/events.log"
    echo
    echo "TOS Stage 3 — a supervisor written in TOS Core"
    echo
    echo "  The capsule carries three canonical TOS Core modules:"
    echo "    /system/policy/services.tos  the policy: which services, how hard"
    echo "                                 to try, and what depends on what"
    echo "    /system/boot/init.tos        the supervisor: the state machine"
    echo "    /system/boot/worker.tos      the service being supervised"
    echo
    echo "  Their source is in source/tests/vectors/supervision/."
    echo "  Building and booting; this takes a minute or two."
    echo
    if [ "$WANT_DISPLAY" = 1 ]; then
        echo "  The QEMU window stays open after the boot halts, as it does for"
        echo "  ./run-tos.sh — close it or press Ctrl-C to see the story below."
        echo
        bash host-tools/qemu-test/supervision.sh "$OUT" \
            --interactive --display "$DISPLAY_BACKEND"
    else
        bash host-tools/qemu-test/supervision.sh "$OUT"
    fi
    echo
    echo "What the supervisor did, from the boot's own diagnostic transport:"
    echo
    python3 "$ROOT/scripts/tos-journal.py" --story "$OUT/serial.log"
    echo
    echo "And what an operator would be shown — WARN and above, nothing else:"
    echo
    python3 "$ROOT/scripts/tos-journal.py" "$OUT/serial.log" | sed 's/^/  /'
    echo
    echo "Raw evidence, kept:"
    echo "  ${OUT#"$ROOT"/}/serial.log   every byte the machine emitted"
    if [ -f "$OUT/events.log" ]; then
        echo "  ${OUT#"$ROOT"/}/events.log   the TOS.* events, firmware chatter removed"
    fi
    echo
    echo "Edit source/tests/vectors/supervision/services.tos and run this again"
    echo "to see different behaviour; README.md has a worked example."
    exit 0
fi

exec bash host-tools/qemu-test/run.sh \
    --out target/run-tos/interactive --interactive --display "$DISPLAY_BACKEND"
