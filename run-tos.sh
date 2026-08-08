#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Human-facing Stage 1 launcher. Boot preparation and QEMU configuration remain
# authoritative in source/host-tools/qemu-test/run.sh.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
SOURCE="$ROOT/source"
MODE=interactive
DISPLAY_BACKEND=

usage() {
    cat <<'EOF'
Usage: ./run-tos.sh [--check]

  (no option)  build and boot TOS interactively with a QEMU display and serial output
  --check      build and run the deterministic automated QEMU self-check
  -h, --help   show this help
EOF
}

case "${1-}" in
    "") ;;
    --check) MODE=check ;;
    -h|--help) usage; exit 0 ;;
    *) echo "run-tos: unknown option: $1" >&2; usage >&2; exit 2 ;;
esac
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

if [ "$MODE" = interactive ]; then
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

if [ "$MODE" = check ]; then
    exec bash host-tools/qemu-test/run.sh --out target/run-tos/check --expect 33
fi
exec bash host-tools/qemu-test/run.sh \
    --out target/run-tos/interactive --interactive --display "$DISPLAY_BACKEND"
