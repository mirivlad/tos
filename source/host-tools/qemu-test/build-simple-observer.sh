#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Build the exact external observer selected for ADR-0066 from a pre-fetched,
# digest-pinned upstream source archive.  This script performs no network
# download.  It applies two hash-bound observer-only changes: the UART captures
# the emitting vCPU thread's physical CPU time after OPEN has been handled and
# before CLOSE is handled, then emits both raw timestamps in one simple-trace
# record.  This excludes the two marker transports without subtracting any
# measured work.  QEMU's vendored Meson wheels are used; libfdt is disabled
# because the ADR-0040 x86_64 profile does not need it, avoiding the dtc
# subproject.
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
output="$(realpath -m "$2")"
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

python3 - \
    "$source_dir/hw/char/serial.c" \
    "$source_dir/hw/char/trace-events" <<'PYTHON'
import hashlib
import sys
from pathlib import Path

serial_path = Path(sys.argv[1])
events_path = Path(sys.argv[2])

serial = serial_path.read_bytes()
serial_upstream_sha256 = "46548454bc48e12b430795fc69cb19f0349bbef3a63ee37c23aa365713978b91"
if hashlib.sha256(serial).hexdigest() != serial_upstream_sha256:
    raise SystemExit("hw/char/serial.c does not match the pinned upstream source")

function = b"static void serial_ioport_write(void *opaque, hwaddr addr, uint64_t val,\n"
helper = b"""static bool tos_measurement_open_valid;
static uint8_t tos_measurement_open;
static uint64_t tos_measurement_open_ns;

static uint64_t tos_measurement_clock(void)
{
    struct timespec timestamp;

    if (clock_gettime(CLOCK_THREAD_CPUTIME_ID, &timestamp) != 0) {
        error_report("TOS measurement observer cannot read the thread CPU clock");
        exit(EXIT_FAILURE);
    }
    return timestamp.tv_sec * 1000000000ULL + timestamp.tv_nsec;
}

"""
if serial.count(function) != 1:
    raise SystemExit("the serial write function is not unique")
serial = serial.replace(function, helper + function)

start = b"""    assert(size == 1 && addr < 8);
    trace_serial_write(addr, val);
    switch(addr) {
"""
start_replacement = b"""    assert(size == 1 && addr < 8);
    trace_serial_write(addr, val);
    if (addr == 0 && (val & 0xe0) == 0xa0 &&
        trace_event_get_state_backends(TRACE_TOS_MEASUREMENT_PAIR)) {
        uint64_t close_ns = tos_measurement_clock();

        trace_tos_measurement_pair(
            tos_measurement_open_valid ? tos_measurement_open : 0,
            val,
            tos_measurement_open_valid ? tos_measurement_open_ns : UINT64_MAX,
            close_ns);
        tos_measurement_open_valid = false;
    }
    switch(addr) {
"""
if serial.count(start) != 1:
    raise SystemExit("the serial write entry is not exact")
serial = serial.replace(start, start_replacement)

end = b"""    case 7:
        s->scr = val;
        break;
    }
}

static uint64_t serial_ioport_read(void *opaque, hwaddr addr, unsigned size)
"""
end_replacement = b"""    case 7:
        s->scr = val;
        break;
    }
    if (addr == 0 && (val & 0xe0) == 0x80 &&
        trace_event_get_state_backends(TRACE_TOS_MEASUREMENT_PAIR)) {
        if (tos_measurement_open_valid) {
            tos_measurement_open_ns = UINT64_MAX;
        } else {
            tos_measurement_open_ns = tos_measurement_clock();
        }
        tos_measurement_open = val;
        tos_measurement_open_valid = true;
    }
}

static uint64_t serial_ioport_read(void *opaque, hwaddr addr, unsigned size)
"""
if serial.count(end) != 1:
    raise SystemExit("the serial write exit is not exact")
serial = serial.replace(end, end_replacement)

serial_modified_sha256 = "5fb72ef50b75f630e68260c487760d5ad99f4fba28ba1bf573439abc4fe7a876"
serial_digest = hashlib.sha256(serial).hexdigest()
if serial_digest != serial_modified_sha256:
    raise SystemExit("the observer-only serial modification is not exact")
serial_path.write_bytes(serial)

events = events_path.read_bytes()
events_upstream_sha256 = "64f70f77897a5e52957f12d55dcb5b0d09f692a56ed70afb757f5f8f5d16e364"
if hashlib.sha256(events).hexdigest() != events_upstream_sha256:
    raise SystemExit("hw/char/trace-events does not match the pinned upstream source")
anchor = b'serial_write(uint16_t addr, uint8_t value) "write addr 0x%02x val 0x%02x"\n'
addition = (
    anchor
    + b'tos_measurement_pair(uint8_t open, uint8_t close, uint64_t open_ns, '
      b'uint64_t close_ns) "open 0x%02x close 0x%02x open_ns %" PRIu64 '
      b'" close_ns %" PRIu64\n'
)
if events.count(anchor) != 1:
    raise SystemExit("the serial trace-event anchor is not unique")
events = events.replace(anchor, addition)
events_modified_sha256 = "7828c2cf29a8ecbc9da05210a29b6132efdc3215d9a72df21cae4841fdb0d466"
events_digest = hashlib.sha256(events).hexdigest()
if events_digest != events_modified_sha256:
    raise SystemExit("the observer-only trace-event modification is not exact")
events_path.write_bytes(events)
PYTHON

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
    "trace_clock": "CLOCK_THREAD_CPUTIME_ID pair on the one TCG vCPU thread",
    "observer_modifications": [
        {
            "path": "hw/char/serial.c",
            "upstream_sha256": "46548454bc48e12b430795fc69cb19f0349bbef3a63ee37c23aa365713978b91",
            "modified_sha256": "5fb72ef50b75f630e68260c487760d5ad99f4fba28ba1bf573439abc4fe7a876",
            "scope": "capture after OPEN and before CLOSE; UART behavior unchanged",
        },
        {
            "path": "hw/char/trace-events",
            "upstream_sha256": "64f70f77897a5e52957f12d55dcb5b0d09f692a56ed70afb757f5f8f5d16e364",
            "modified_sha256": "7828c2cf29a8ecbc9da05210a29b6132efdc3215d9a72df21cae4841fdb0d466",
            "scope": "one measurement-pair event carrying both raw timestamps",
        },
    ],
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
