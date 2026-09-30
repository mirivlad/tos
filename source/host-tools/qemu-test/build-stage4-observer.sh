#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Build the external observer ADR-0103 names for the Stage 4 reference
# measurement, from the same pre-fetched, digest-pinned upstream archive as the
# ADR-0066 observer.  This script performs no network download.
#
# **A second observer rather than a second event in the first.** The ADR-0066
# observer is the qualified instrument of a closed stage and its modified source
# is pinned by digest in `measure-channel.py`; adding an event to it would change
# that identity for a measurement it has nothing to do with.  This build leaves
# `build-simple-observer.sh` byte-for-byte alone and differs from it in exactly
# the two observer-only source changes below.
#
# **What it records, and where.** ADR-0103 fixes the clocks: elapsed I/O on
# `CLOCK_MONOTONIC_RAW`, CPU cost on `CLOCK_PROCESS_CPUTIME_ID` for the complete
# QEMU process — every vCPU and device-model thread.  Both are read in the one TCG
# vCPU thread at ADR-0066's points: after the UART has handled an `OPEN` marker
# and before it handles the matching `CLOSE`, so neither marker transport is
# inside an interval.  The reads nest: OPEN reads the CPU clock and then the raw
# clock, CLOSE reads the raw clock and then the CPU clock, so the elapsed interval
# contains neither CPU-clock system call.  All four raw timestamps are emitted in
# one simple-trace record only once the closing ones exist.  Nothing is estimated
# or subtracted, and a clock that cannot be read ends QEMU.
#
# The marker bytes are ADR-0066's (`OPEN = 0x80 | tag`, `CLOSE = 0xa0 | tag`); UART
# behaviour is unchanged.  QEMU's vendored Meson wheels are used; libfdt is
# disabled because the ADR-0040 x86_64 profile does not need it.
#
# Usage: build-stage4-observer.sh QEMU-10.0.11.tar.xz OUTPUT-DIRECTORY
set -euo pipefail

QEMU_VERSION=10.0.11
QEMU_SOURCE_SHA256=22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871

fail() {
    echo "build-stage4-observer: FAIL: $*" >&2
    exit 1
}

[ "$#" -eq 2 ] || fail "usage: $0 QEMU-$QEMU_VERSION.tar.xz OUTPUT-DIRECTORY"
archive="$(realpath "$1")"
output="$(realpath -m "$2")"
[ -f "$archive" ] || fail "source archive does not exist: $archive"
[ ! -e "$output" ] || fail "output already exists: $output"

actual_sha256="$(sha256sum "$archive" | cut -d' ' -f1)"
[ "$actual_sha256" = "$QEMU_SOURCE_SHA256" ] ||
    fail "source digest is $actual_sha256, expected $QEMU_SOURCE_SHA256"

output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
work="$(mktemp -d "$output_parent/.qemu-stage4-observer.XXXXXX")"
cleanup() {
    rm -rf -- "$work"
}
trap cleanup EXIT

source_dir="$work/source"
build_dir="$work/build"
install_dir="$work/install"
mkdir -p "$source_dir" "$build_dir" "$install_dir"
tar -xJf "$archive" -C "$source_dir" --strip-components=1

python3 "$(dirname "$0")/stage4-observer-patch.py" \
    "$source_dir/hw/char/serial.c" \
    "$source_dir/hw/char/trace-events" > "$work/modifications.json"

configure=(
    --prefix=/
    --target-list=x86_64-softmmu
    --enable-trace-backends=simple
    --enable-fdt=disabled
    --disable-download
    --disable-docs
    --disable-tools
    --disable-guest-agent
    --disable-slirp
    --disable-plugins
    --disable-vnc
    --disable-gtk
    --disable-sdl
    --disable-werror
    --disable-debug-info
)
(
    cd "$build_dir"
    export SOURCE_DATE_EPOCH=1782452340
    export CFLAGS="-O2 -ffile-prefix-map=$work=/usr/src/qemu-$QEMU_VERSION -fdebug-prefix-map=$work=/usr/src/qemu-$QEMU_VERSION -fmacro-prefix-map=$work=/usr/src/qemu-$QEMU_VERSION"
    "$source_dir/configure" "${configure[@]}"
    ninja -j "${TOS_QEMU_BUILD_JOBS:-$(nproc)}" qemu-system-x86_64 trace/trace-events-all
)

# The measured q35/OVMF command names its firmware explicitly. Its only QEMU
# data-file reads are the default VGA ROM and kvmvapic ROM (and the option ROM
# the reference NIC would ask for), so retain and hash exactly those.
mkdir -p "$install_dir/bin" "$install_dir/share/qemu"
install -m 0755 "$build_dir/qemu-system-x86_64" "$install_dir/bin/qemu-system-x86_64.real"
install -m 0644 "$source_dir/pc-bios/kvmvapic.bin" \
    "$source_dir/pc-bios/vgabios-stdvga.bin" \
    "$source_dir/pc-bios/efi-e1000e.rom" \
    "$install_dir/share/qemu/"

engine="$install_dir/bin/qemu-system-x86_64.real"
qemu="$install_dir/bin/qemu-system-x86_64"
cat >"$qemu" <<'WRAPPER'
#!/bin/sh
# Keep the installed observer bound to its retained QEMU data files.
set -eu
self_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$self_dir/qemu-system-x86_64.real" -L "$self_dir/../share/qemu" "$@"
WRAPPER
chmod +x "$qemu"

python3 - "$install_dir" "$source_dir" "$work/modifications.json" "${CC:-cc}" \
    "${configure[@]}" <<'PYTHON'
import hashlib
import json
import subprocess
import sys
from pathlib import Path

install, source = Path(sys.argv[1]), Path(sys.argv[2])
modifications = json.loads(Path(sys.argv[3]).read_text(encoding="utf-8"))
compiler, configure = sys.argv[4], sys.argv[5:]
bin_dir = install / "bin"
engine = bin_dir / "qemu-system-x86_64.real"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command):
    return subprocess.run(command, check=True, text=True, stdout=subprocess.PIPE).stdout


dynamic_dependencies = {}
for line in run(["ldd", str(engine)]).splitlines():
    for token in line.split():
        path = Path(token)
        if token.startswith("/") and path.is_file():
            dynamic_dependencies[str(path)] = digest(path)

record = {
    "record_spdx_license": "CC-BY-SA-4.0",
    "observer": "ADR-0103 Stage 4 reference observer",
    "qemu_version": "10.0.11",
    "qemu_version_output": run([str(bin_dir / "qemu-system-x86_64"), "--version"]).splitlines()[0],
    "qemu_source_url": "https://download.qemu.org/qemu-10.0.11.tar.xz",
    "qemu_source_sha256": "22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871",
    "qemu_sha256": digest(bin_dir / "qemu-system-x86_64"),
    "qemu_engine_relative_path": "qemu-system-x86_64.real",
    "qemu_engine_sha256": digest(engine),
    "trace_backends": ["simple"],
    "trace_event": "tos_block_window",
    "elapsed_clock": "CLOCK_MONOTONIC_RAW",
    "cpu_clock": "CLOCK_PROCESS_CPUTIME_ID of the complete QEMU process",
    "timestamp_point": "after OPEN handling and before CLOSE handling in the one TCG vCPU "
                       "thread; OPEN reads CPU then raw, CLOSE reads raw then CPU",
    "observer_modifications": modifications,
    "network_downloads": "disabled",
    "source_date_epoch": 1782452340,
    "build_path_remap": "/usr/src/qemu-10.0.11",
    "configure": configure,
    "cflags": "-O2 plus file, debug and macro prefix maps from the temporary build root "
              "to /usr/src/qemu-10.0.11",
    "compiler": run([compiler, "--version"]).splitlines()[0],
    "python": run([sys.executable, "--version"]).strip(),
    "vendored_wheels": {
        name: digest(source / "python" / "wheels" / name)
        for name in ("meson-1.5.0-py3-none-any.whl", "pycotap-1.3.1-py3-none-any.whl")
    },
    "retained_data": {
        f"../share/qemu/{name}": digest(install / "share" / "qemu" / name)
        for name in ("kvmvapic.bin", "vgabios-stdvga.bin", "efi-e1000e.rom")
    },
    "dynamic_dependencies": dynamic_dependencies,
    "libfdt": "disabled; unused by the measured x86_64 profile",
}
(bin_dir / "observer-build.json").write_text(json.dumps(record, indent=2) + "\n",
                                             encoding="utf-8")
PYTHON

mv "$install_dir" "$output"
# The source and build trees are a gigabyte and nothing refers to them now.
cleanup
trap - EXIT
echo "build-stage4-observer: PASS"
echo "  output=$output/bin/qemu-system-x86_64"
echo "  source_sha256=$QEMU_SOURCE_SHA256"
echo "  network downloads disabled"
