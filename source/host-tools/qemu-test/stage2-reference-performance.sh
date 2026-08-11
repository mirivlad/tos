#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The reference half of the docs/35 Stage 2 pair, taken on the real path.
#
# ADR-0040 section 1a requires the reference measurement to be taken through the
# Stage 2 runtime path on the declared platform, not through a host process
# wearing the platform's name. That is what this does: the workload is the
# capsule's canonical boot module, it goes through reader, parser, checker,
# resolution, lowering, the independent verifier and the engine inside QEMU on
# q35/qemu64/1 vCPU/256 MiB/TCG, and the time comes from host-monotonic
# timestamps of the `TOS.RUN.*` events the boot already emits.
#
# Two boundaries are measured, matching the two metrics docs/35 assigns to the
# bootstrap profile:
#
#   frontend   TOS.RUN.BEGIN            -> TOS.RUN.STAGE name=execute
#              (read, parse, check, resolve, lower, verify of a 256 KiB module)
#   execution  TOS.RUN.STAGE name=execute -> TOS.RUN.COMPLETED
#              (a one-million-operation integer/control-flow workload)
#
# The timestamps are taken on the host as bytes arrive on the serial line, so
# they include serial transport. That is stated rather than corrected for: the
# correction would be a number nobody measured.
#
#   bash host-tools/qemu-test/stage2-reference-performance.sh [OUT_DIR] [SAMPLES]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${1:-target/stage2-reference-performance}"
SAMPLES="${2:-5}"
TOOL="$ROOT/target/release/tos-capsule-tool"
NOTICES="$ROOT/system/boot/NOTICES.txt"

mkdir -p "$OUT"
[ -x "$TOOL" ] || { echo "missing $TOOL (cargo build --release -p tos-capsule-tool)" >&2; exit 2; }

# --- the two workload modules, generated deterministically -------------------
python3 - "$OUT" <<'PY'
import sys, pathlib
out = pathlib.Path(sys.argv[1])
spdx = "// SPDX-License-Identifier: GPL-3.0-or-later\n"

# The frontend workload: a canonical module at the published 256 KiB ceiling.
# It is the largest single source unit docs/44 admits, which is the case the
# docs/35 frontend budget is written against.
head = (spdx +
        "module system.boot.init version 1.0 profile bootstrap;\n\n"
        "resource [fuel: 4000000, stack: 64KiB, allocation: 64KiB, tasks: 1, workers: 1, "
        "sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0]\n\n"
        "pub fn main() -> i32 { return 0i32; }\n\n")
text = head
index = 0
while True:
    chunk = (f"pub record Point{index} [x: i32, y: i32]\n"
             f"pub fn total{index}(point: Point{index}) -> i32 {{ return point.x + point.y; }}\n")
    if len(text) + len(chunk) > 256 * 1024:
        break
    text += chunk
    index += 1
(out / "frontend.tos").write_text(text, encoding="utf-8")

# The execution workload: one million metered integer/control-flow operations.
# A `while` loop is the shape docs/35 names, and the total is returned so a
# wrong answer is a failed measurement rather than a fast one.
(out / "execute.tos").write_text(spdx +
    "module system.boot.init version 1.0 profile bootstrap;\n\n"
    "resource [fuel: 40000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, "
    "sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 0]\n\n"
    "// One million metered back edges. The engine charges fuel per operation,\n"
    "// so this is the docs/35 integer/control-flow workload on the real engine.\n"
    "pub fn main() -> i32 {\n"
    "    let mut total: i32 = 0i32;\n"
    "    let mut index: i32 = 0i32;\n"
    "    while (index < 1000000i32) {\n"
    "        total = total + 1i32;\n"
    "        index = index + 1i32;\n"
    "    }\n"
    "    return total;\n"
    "}\n", encoding="utf-8")
print(f"frontend fixture: {len((out / 'frontend.tos').read_bytes())} bytes")
PY

for workload in frontend execute; do
    printf '/system/boot/init.tos\t%s\n' "$OUT/$workload.tos" > "$OUT/$workload.manifest"
    "$TOOL" --detached --licence "$NOTICES" \
        --out "$OUT/$workload.bin" "$OUT/$workload.manifest" >/dev/null
done

# --- boot each workload SAMPLES times, timestamping its events ---------------
for workload in frontend execute; do
    for sample in $(seq 1 "$SAMPLES"); do
        # TCG is far slower than the host, and the 256 KiB workload is the
        # largest source unit the contract admits. The default 90 s is a Stage 1
        # timeout for a nucleus that only validated records.
        bash "$HERE/run.sh" --out "$OUT/run-$workload-$sample" \
            --capsule "$OUT/$workload.bin" --expect 33 --timeout 900 \
            --event-timestamps "$OUT/$workload-$sample.json" \
            --require "TOS.RUN.BEGIN TOS.RUN.COMPLETED" \
            --forbid "TOS.PANIC TOS.EXCEPTION TOS.RUN.REFUSED TOS.RUN.TRAP" >/dev/null
    done
done

# --- reduce the timestamps to the two boundaries -----------------------------
python3 - "$OUT" "$SAMPLES" <<'PY'
import json, pathlib, statistics, sys
out, samples = pathlib.Path(sys.argv[1]), int(sys.argv[2])

def spans(workload, first, last):
    values = []
    for sample in range(1, samples + 1):
        events = json.loads((out / f"{workload}-{sample}.json").read_text())
        marks = [e for e in events if e["event"] in (first, last)]
        # The stage events repeat the same identifier, so the boundary is the
        # first arrival of `first` and the first arrival of `last` after it.
        start = next(e for e in marks if e["event"] == first)
        end = next(e for e in marks
                   if e["event"] == last and e["monotonic_ns"] > start["monotonic_ns"])
        values.append((end["monotonic_ns"] - start["monotonic_ns"]) // 1000)
    return sorted(values)

report = []
for workload, first, last, label in (
    ("frontend", "TOS.RUN.BEGIN", "TOS.RUN.VERIFIED",
     "read + parse + check + resolve + lower + verify, 256 KiB module"),
    ("execute", "TOS.RUN.VERIFIED", "TOS.RUN.COMPLETED",
     "one-million-operation integer/control-flow benchmark"),
):
    values = spans(workload, first, last)
    report.append({
        "workload": workload,
        "label": label,
        "boundary": f"{first} -> {last}",
        "samples_us": values,
        "median_us": int(statistics.median(values)),
        "min_us": values[0],
        "max_us": values[-1],
    })

(out / "reference.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print("profile: reference (ADR-0040 q35/qemu64/1 vCPU/256 MiB/TCG, real Stage 2 path)")
print(f"sampling: {samples} boots per workload, host-monotonic serial event timestamps")
for entry in report:
    print(f"{entry['label']}")
    print(f"  boundary {entry['boundary']}")
    print(f"  median {entry['median_us']} us, min {entry['min_us']} us, max {entry['max_us']} us")
    print(f"  raw samples (us): {' '.join(str(v) for v in entry['samples_us'])}")
PY
echo "records: $OUT/reference.json"
