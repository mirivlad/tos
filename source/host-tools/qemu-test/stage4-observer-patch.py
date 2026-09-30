#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Apply the two observer-only source changes of the ADR-0103 Stage 4 observer.

Hash-bound on both sides: the upstream file must be the pinned one, and the
modified file must be exactly the pinned result, so neither a different QEMU nor
an edit to this script can produce an observer that still calls itself this one.
Prints the modification record `build-stage4-observer.sh` binds into
`observer-build.json`.

Usage: stage4-observer-patch.py hw/char/serial.c hw/char/trace-events
"""

import hashlib
import json
import sys
from pathlib import Path

SERIAL_UPSTREAM = "46548454bc48e12b430795fc69cb19f0349bbef3a63ee37c23aa365713978b91"
SERIAL_MODIFIED = "f1cd58e8a88d34c7660aeba96c034a316d85319a289912dd0d27e8ef0182a56f"
EVENTS_UPSTREAM = "64f70f77897a5e52957f12d55dcb5b0d09f692a56ed70afb757f5f8f5d16e364"
EVENTS_MODIFIED = "9dff0dc68434dd3a295e41a443f0c698e79250b1be9009612dd816f49485d179"

HELPER = b"""static bool tos_block_open_valid;
static uint8_t tos_block_open;
static uint64_t tos_block_open_raw_ns;
static uint64_t tos_block_open_cpu_ns;

static uint64_t tos_block_clock(clockid_t clock, const char *name)
{
    struct timespec timestamp;

    if (clock_gettime(clock, &timestamp) != 0) {
        error_report("TOS block observer cannot read %s", name);
        exit(EXIT_FAILURE);
    }
    return timestamp.tv_sec * 1000000000ULL + timestamp.tv_nsec;
}

"""

# CLOSE: before the UART handles the byte. Raw first, then CPU, so the elapsed
# interval ends before the CPU-clock system call.
START = b"""    assert(size == 1 && addr < 8);
    trace_serial_write(addr, val);
    switch(addr) {
"""
START_REPLACEMENT = b"""    assert(size == 1 && addr < 8);
    trace_serial_write(addr, val);
    if (addr == 0 && (val & 0xe0) == 0xa0 &&
        trace_event_get_state_backends(TRACE_TOS_BLOCK_WINDOW)) {
        uint64_t close_raw_ns = tos_block_clock(CLOCK_MONOTONIC_RAW,
                                                "CLOCK_MONOTONIC_RAW");
        uint64_t close_cpu_ns = tos_block_clock(CLOCK_PROCESS_CPUTIME_ID,
                                                "CLOCK_PROCESS_CPUTIME_ID");

        trace_tos_block_window(
            tos_block_open_valid ? tos_block_open : 0,
            val,
            tos_block_open_valid ? tos_block_open_raw_ns : UINT64_MAX,
            close_raw_ns,
            tos_block_open_valid ? tos_block_open_cpu_ns : UINT64_MAX,
            close_cpu_ns);
        tos_block_open_valid = false;
    }
    switch(addr) {
"""

# OPEN: after the UART has handled the byte. CPU first, then raw, so the elapsed
# interval begins after the CPU-clock system call. A second OPEN before a CLOSE
# poisons the pair rather than silently restarting it.
END = b"""    case 7:
        s->scr = val;
        break;
    }
}

static uint64_t serial_ioport_read(void *opaque, hwaddr addr, unsigned size)
"""
END_REPLACEMENT = b"""    case 7:
        s->scr = val;
        break;
    }
    if (addr == 0 && (val & 0xe0) == 0x80 &&
        trace_event_get_state_backends(TRACE_TOS_BLOCK_WINDOW)) {
        if (tos_block_open_valid) {
            tos_block_open_cpu_ns = UINT64_MAX;
            tos_block_open_raw_ns = UINT64_MAX;
        } else {
            tos_block_open_cpu_ns = tos_block_clock(CLOCK_PROCESS_CPUTIME_ID,
                                                    "CLOCK_PROCESS_CPUTIME_ID");
            tos_block_open_raw_ns = tos_block_clock(CLOCK_MONOTONIC_RAW,
                                                    "CLOCK_MONOTONIC_RAW");
        }
        tos_block_open = val;
        tos_block_open_valid = true;
    }
}

static uint64_t serial_ioport_read(void *opaque, hwaddr addr, unsigned size)
"""

FUNCTION = b"static void serial_ioport_write(void *opaque, hwaddr addr, uint64_t val,\n"
ANCHOR = b'serial_write(uint16_t addr, uint8_t value) "write addr 0x%02x val 0x%02x"\n'
EVENT = (
    b'tos_block_window(uint8_t open, uint8_t close, uint64_t open_raw_ns, '
    b'uint64_t close_raw_ns, uint64_t open_cpu_ns, uint64_t close_cpu_ns) '
    b'"open 0x%02x close 0x%02x open_raw_ns %" PRIu64 " close_raw_ns %" PRIu64 '
    b'" open_cpu_ns %" PRIu64 " close_cpu_ns %" PRIu64\n'
)


def fail(message):
    raise SystemExit(f"stage4-observer-patch: {message}")


def replace_once(text, old, new, what):
    if text.count(old) != 1:
        fail(f"the {what} is not unique in the pinned source")
    return text.replace(old, new)


def patched(path, upstream, modified, transform, scope):
    """Both files are checked before either is written."""
    original = path.read_bytes()
    if hashlib.sha256(original).hexdigest() != upstream:
        fail(f"{path.name} does not match the pinned upstream source")
    result = transform(original)
    digest = hashlib.sha256(result).hexdigest()
    if digest != modified:
        fail(f"the observer-only {path.name} modification is {digest}, expected {modified}")
    return path, result, {"path": f"hw/char/{path.name}", "upstream_sha256": upstream,
                          "modified_sha256": modified, "scope": scope}


def serial(text):
    text = replace_once(text, FUNCTION, HELPER + FUNCTION, "serial write function")
    text = replace_once(text, START, START_REPLACEMENT, "serial write entry")
    return replace_once(text, END, END_REPLACEMENT, "serial write exit")


def events(text):
    return replace_once(text, ANCHOR, ANCHOR + EVENT, "serial trace-event anchor")


def main():
    if len(sys.argv) != 3:
        fail("usage: stage4-observer-patch.py hw/char/serial.c hw/char/trace-events")
    changes = [
        patched(Path(sys.argv[1]), SERIAL_UPSTREAM, SERIAL_MODIFIED, serial,
                "CLOCK_MONOTONIC_RAW and process CPU time after OPEN and before CLOSE; "
                "UART behavior unchanged"),
        patched(Path(sys.argv[2]), EVENTS_UPSTREAM, EVENTS_MODIFIED, events,
                "one window event carrying all four raw timestamps"),
    ]
    for path, result, _ in changes:
        path.write_bytes(result)
    json.dump([record for _, _, record in changes], sys.stdout, indent=2)
    sys.stdout.write("\n")


main()
