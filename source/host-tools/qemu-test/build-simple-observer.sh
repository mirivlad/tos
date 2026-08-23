#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build the exact external observer selected for ADR-0066 from a pre-fetched,
# digest-pinned upstream source archive.  This script performs no network
# download.  QEMU's vendored Meson wheels are used; libfdt is disabled because
# the ADR-0040 x86_64 profile does not need it, avoiding the dtc subproject.
#
# Usage: build-simple-observer.sh QEMU-10.0.11.tar.xz OUTPUT-DIRECTORY
set -euo pipefail

QEMU_VERSION=10.0.11
QEMU_SOURCE_SHA256=22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871
QEMU_SOURCE_URL=https://download.qemu.org/qemu-10.0.11.tar.xz

fail() {
    echo "build-simple-observer: FAIL: $*" >&2
    exit 1
}

[ "$#" -eq 2 ] || fail "usage: $0 QEMU-10.0.11.tar.xz OUTPUT-DIRECTORY"
archive="$(realpath "$1")"
output="$2"
[ -f "$archive" ] || fail "source archive does not exist: $archive"
[ ! -e "$output" ] || fail "output already exists: $output"

actual_sha256="$(sha256sum "$archive" | cut -d' ' -f1)"
[ "$actual_sha256" = "$QEMU_SOURCE_SHA256" ] ||
    fail "source digest is $actual_sha256, expected $QEMU_SOURCE_SHA256"

output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
work="$(mktemp -d "$output_parent/.qemu-simple-observer.XXXXXX")"
cleanup() {
    rm -rf -- "$work"
}
trap cleanup EXIT

source_dir="$work/source"
build_dir="$work/build"
install_dir="$work/install"
mkdir -p "$source_dir" "$build_dir" "$install_dir"
tar -xJf "$archive" -C "$source_dir" --strip-components=1

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
    export CFLAGS="-O2 -ffile-prefix-map=$work=/usr/src/qemu-10.0.11 -fdebug-prefix-map=$work=/usr/src/qemu-10.0.11 -fmacro-prefix-map=$work=/usr/src/qemu-10.0.11"
    "$source_dir/configure" "${configure[@]}"
    ninja -j "${TOS_QEMU_BUILD_JOBS:-$(nproc)}" qemu-system-x86_64 trace/trace-events-all
)

# The measured q35/OVMF command names its firmware explicitly.  Its only QEMU
# data-file reads are the default VGA ROM and kvmvapic ROM, so retain and hash
# exactly those rather than an unrelated multi-architecture firmware bundle.
mkdir -p "$install_dir/bin" "$install_dir/share/qemu"
install -m 0755 "$build_dir/qemu-system-x86_64" \
    "$install_dir/bin/qemu-system-x86_64"
install -m 0644 "$source_dir/pc-bios/kvmvapic.bin" \
    "$source_dir/pc-bios/vgabios-stdvga.bin" \
    "$source_dir/pc-bios/efi-e1000e.rom" \
    "$install_dir/share/qemu/"

engine="$install_dir/bin/qemu-system-x86_64.real"
mv "$install_dir/bin/qemu-system-x86_64" "$engine"
[ -x "$engine" ] || fail "the build produced no qemu-system-x86_64"
qemu="$install_dir/bin/qemu-system-x86_64"
cat >"$qemu" <<'WRAPPER'
#!/bin/sh
# Keep the DESTDIR-installed observer bound to its retained QEMU data files.
set -eu
self_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$self_dir/qemu-system-x86_64.real" -L "$self_dir/../share/qemu" "$@"
WRAPPER
chmod +x "$qemu"
qemu_sha256="$(sha256sum "$qemu" | cut -d' ' -f1)"
engine_sha256="$(sha256sum "$engine" | cut -d' ' -f1)"
qemu_version_line="$($qemu --version | sed -n '1p')"
compiler="$(${CC:-cc} --version | sed -n '1p')"
python_version="$(python3 --version)"
meson_wheel_sha256="$(sha256sum "$source_dir/python/wheels/meson-1.5.0-py3-none-any.whl" | cut -d' ' -f1)"
pycotap_wheel_sha256="$(sha256sum "$source_dir/python/wheels/pycotap-1.3.1-py3-none-any.whl" | cut -d' ' -f1)"
kvmvapic_sha256="$(sha256sum "$install_dir/share/qemu/kvmvapic.bin" | cut -d' ' -f1)"
vgabios_sha256="$(sha256sum "$install_dir/share/qemu/vgabios-stdvga.bin" | cut -d' ' -f1)"
e1000e_rom_sha256="$(sha256sum "$install_dir/share/qemu/efi-e1000e.rom" | cut -d' ' -f1)"

python3 - \
    "$install_dir/bin/observer-build.json" \
    "$qemu_sha256" \
    "$engine_sha256" \
    "$qemu_version_line" \
    "$compiler" \
    "$python_version" \
    "$meson_wheel_sha256" \
    "$pycotap_wheel_sha256" \
    "$kvmvapic_sha256" \
    "$vgabios_sha256" \
    "$e1000e_rom_sha256" \
    "$engine" \
    "${configure[@]}" <<'PYTHON'
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ldd_output = subprocess.run(
    ["ldd", sys.argv[12]], check=True, text=True, stdout=subprocess.PIPE
).stdout
dynamic_dependencies = {}
for line in ldd_output.splitlines():
    for token in line.split():
        path = Path(token)
        if token.startswith("/") and path.is_file():
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            dynamic_dependencies[str(path)] = digest

report = {
    "record_spdx_license": "CC-BY-SA-4.0",
    "qemu_version": "10.0.11",
    "qemu_version_output": sys.argv[4],
    "qemu_source_url": "https://download.qemu.org/qemu-10.0.11.tar.xz",
    "qemu_source_sha256": "22e410fe784021c535756350a811ee78ae71356546ff90f5418493448a34b871",
    "qemu_sha256": sys.argv[2],
    "qemu_engine_relative_path": "qemu-system-x86_64.real",
    "qemu_engine_sha256": sys.argv[3],
    "trace_backends": ["simple"],
    "network_downloads": "disabled",
    "source_date_epoch": 1782452340,
    "build_path_remap": "/usr/src/qemu-10.0.11",
    "configure": sys.argv[13:],
    "cflags": "-O2 plus file, debug and macro prefix maps from the temporary build root to /usr/src/qemu-10.0.11",
    "compiler": sys.argv[5],
    "python": sys.argv[6],
    "vendored_wheels": {
        "meson-1.5.0-py3-none-any.whl": sys.argv[7],
        "pycotap-1.3.1-py3-none-any.whl": sys.argv[8],
    },
    "retained_data": {
        "../share/qemu/kvmvapic.bin": sys.argv[9],
        "../share/qemu/vgabios-stdvga.bin": sys.argv[10],
        "../share/qemu/efi-e1000e.rom": sys.argv[11],
    },
    "dynamic_dependencies": dynamic_dependencies,
    "libfdt": "disabled; unused by the measured x86_64 profile",
}
with open(sys.argv[1], "w", encoding="utf-8") as destination:
    json.dump(report, destination, indent=2)
    destination.write("\n")
PYTHON

mv "$install_dir" "$output"
trap - EXIT
echo "build-simple-observer: PASS"
echo "  output=$output/bin/qemu-system-x86_64"
echo "  qemu_sha256=$qemu_sha256"
echo "  source_sha256=$QEMU_SOURCE_SHA256"
echo "  network downloads disabled"
