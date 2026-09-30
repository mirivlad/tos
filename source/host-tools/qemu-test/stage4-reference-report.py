#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check both ADR-0103 reference boots, then compute R1-R3 and judge them.

Everything that makes the two boots comparable is checked before any ratio is
formed: the same measured sector sequence (equal digests), the same disk bytes,
the TOS client completing with that digest and every one of its READs on the
audit record, and the oracle's interrupt account balancing. The thresholds are
ADR-0103's and are constants here, not options.

Exit status: 0 every budget met, 4 a valid measurement with a budget missed,
1 the pair is not evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import sys
from pathlib import Path

R1_MINIMUM = 0.35
R2_MAXIMUM = 5.0
R3_MAXIMUM = 8.0
TOTAL_READS = 128 + 3 * 2048 + 303 * 8
MEASURED_READS = 3 * 2048 + 303 * 8


def fail(message: str) -> None:
    raise SystemExit(f"stage4-reference: FAIL: {message}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command: list[str], cwd: Path | None = None) -> str:
    return subprocess.run(command, cwd=cwd, check=True, text=True,
                          stdout=subprocess.PIPE).stdout.strip()


def measurement(boot: Path) -> dict:
    path = boot / "measurement.json"
    if not path.is_file():
        fail(f"{boot.name}: no measurement record")
    record = json.loads(path.read_text())
    if record["qemu_exit"] != 33:
        fail(f"{boot.name}: QEMU exited {record['qemu_exit']}, not 33")
    return record


def lines(boot: Path) -> list[str]:
    return [line.rstrip("\r") for line in
            (boot / "serial.log").read_bytes().decode("utf-8", "replace").split("\n")]


def oracle_account(boot: Path) -> dict[str, int]:
    results = [line for line in lines(boot) if line.startswith("ORACLE.RESULT ")]
    if len(results) != 1:
        fail("the oracle did not report exactly one result")
    fields = {key: int(value) for key, value in re.findall(r"(\w+)=(\d+)", results[0])}
    expected = {"warmup_verified": 128, "measured_reads": MEASURED_READS,
                "deliveries": TOTAL_READS, "unexpected": 0}
    for key, value in expected.items():
        if fields.get(key) != value:
            fail(f"oracle {key}={fields.get(key)}, expected {value}")
    return fields


def tos_account(boot: Path, digest: int) -> dict[str, object]:
    text = lines(boot)
    for forbidden in ("TOS.EXCEPTION", "TOS.PANIC", "TOS.RUN.TRAP", "TOS.RUN.REFUSED",
                      "TOS.RUN.UNSTARTABLE"):
        if any(line.startswith(forbidden) for line in text):
            fail(f"the TOS boot reported {forbidden}")
    completed = [line for line in text if line == f"TOS.RUN.COMPLETED value=i64:{digest}"]
    if len(completed) != 1:
        fail("the TOS client did not complete with the oracle's digest")
    calls = sum(1 for line in text if line ==
                "TOS.RUN.INTERFACE operation=endpoint_call_word_carrying status=0")
    if calls != TOTAL_READS:
        fail(f"{calls} successful READ calls on the audit record, the plan makes {TOTAL_READS}")
    accounts = [line for line in text if line.startswith("TOS.RUN.ACCOUNTING ")]
    if len(accounts) != 3:
        fail(f"{len(accounts)} process account(s) on the record, expected three")
    engines = sorted({m.group(1) for line in text
                      for m in [re.search(r"runtime_engine=sha256:([0-9a-f]{64})", line)] if m})
    modules = dict(re.findall(r"^TOS\.RUN\.VERIFIED module=(\S+) digest=sha256:([0-9a-f]{64})",
                              "\n".join(text), re.M))
    identity = [line for line in text if line.startswith("TOS.IDENTITY ")]
    return {"audit_interface_lines": sum(1 for line in text
                                         if line.startswith("TOS.RUN.INTERFACE ")),
            "read_calls_on_record": calls, "accounts": accounts,
            "runtime_engine_sha256": engines, "module_digests": modules,
            "identity": identity[0] if identity else None}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("repository", "out", "tos", "oracle", "capsule-meta", "pattern", "nucleus",
                 "runtime-image", "oracle-efi", "quantum-source"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    parser.add_argument("--production-nucleus-sha256", required=True)
    parser.add_argument("--production-runtime-image-sha256", required=True)
    args = parser.parse_args()

    tos, oracle = measurement(args.tos), measurement(args.oracle)
    if tos["observer_build_sha256"] != oracle["observer_build_sha256"]:
        fail("the two boots were read by different observer builds")
    account = oracle_account(args.oracle)
    tos_record = tos_account(args.tos, account["digest"])
    pattern = sha256(args.pattern)
    images = {side: sha256(boot / "stage4-block.img") for side, boot in
              (("tos", args.tos), ("oracle", args.oracle))}
    if set(images.values()) != {pattern}:
        fail(f"the disk images differ from the pattern after the boots: {images}")

    r1 = tos["r1"]["throughput_bytes_per_s"] / oracle["r1"]["throughput_bytes_per_s"]
    r2 = tos["r2"]["p99_elapsed_ns"] / oracle["r2"]["p99_elapsed_ns"]
    r3 = tos["r3"]["cpu_ns_per_mib"] / oracle["r3"]["cpu_ns_per_mib"]
    verdict = {"R1": {"ratio": r1, "threshold": f">= {R1_MINIMUM}", "met": r1 >= R1_MINIMUM},
               "R2": {"ratio": r2, "threshold": f"<= {R2_MAXIMUM}", "met": r2 <= R2_MAXIMUM},
               "R3": {"ratio": r3, "threshold": f"<= {R3_MAXIMUM}", "met": r3 <= R3_MAXIMUM}}

    status = run(["git", "status", "--porcelain"], cwd=args.repository)
    quantum = re.findall(r"^\s*const\s+QUANTUM\s*:\s*u32\s*=\s*([0-9_]+)\s*;",
                         args.quantum_source.read_text(), re.M)
    cpu = [line.split(":", 1)[1].strip() for line in
           Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")]
    report = {
        "record_spdx_license": "CC-BY-SA-4.0",
        "decision": "ADR-0103",
        "evidence_status": "P1" if not status else "exploratory",
        "commit": run(["git", "rev-parse", "HEAD"], cwd=args.repository),
        "dirty": bool(status),
        "host": {"platform": platform.platform(), "cpu": cpu[0] if cpu else None,
                 "logical_cpus": len(cpu)},
        "profile": "QEMU q35, qemu64, one vCPU, 256 MiB, TCG; Stage 4 VirtIO-block profile "
                   "revision 2; retained windows run one boot at a time",
        "cache_state": "cold: no derived executable cache exists; every module is checked, "
                       "lowered and verified in the boot that runs it",
        "scheduler_quantum": int(quantum[0].replace("_", "")) if len(quantum) == 1 else None,
        "disk_image_sha256": pattern,
        "capsule": json.loads(args.capsule_meta.read_text()),
        "artifacts": {"measurement_nucleus_sha256": sha256(args.nucleus),
                      "measurement_runtime_image_sha256": sha256(args.runtime_image),
                      "oracle_efi_sha256": sha256(args.oracle_efi),
                      "production_nucleus_sha256": args.production_nucleus_sha256,
                      "production_runtime_image_sha256": args.production_runtime_image_sha256},
        "observer_build": tos["observer_build"],
        "qemu_command": {"tos": tos["qemu_command"], "oracle": oracle["qemu_command"]},
        "workload_digest": account["digest"],
        "oracle_account": account,
        "tos_account": tos_record,
        "verdict": verdict,
        "tos": {key: tos[key] for key in ("floor_elapsed_ns", "r1", "r2", "r3", "raw")},
        "oracle": {key: oracle[key] for key in ("floor_elapsed_ns", "r1", "r2", "r3", "raw")},
    }
    (args.out / "stage4-reference-report.json").write_text(json.dumps(report, indent=2) + "\n")

    def side(name: str, record: dict) -> str:
        return (f"  {name:6} R1 {record['r1']['throughput_bytes_per_s'] / 1024:10.1f} KiB/s   "
                f"R2 p99 {record['r2']['p99_elapsed_ns'] / 1e6:10.3f} ms   "
                f"R3 {record['r3']['cpu_ns_per_mib'] / 1e9:9.3f} s CPU/MiB   "
                f"floor median {record['floor_elapsed_ns']['median']} ns")
    print(f"stage4-reference: evidence={report['evidence_status']} digest={account['digest']}")
    print(side("tos", tos))
    print(side("oracle", oracle))
    for name, entry in verdict.items():
        print(f"  {name}: {entry['ratio']:.6g} (ADR-0103 {entry['threshold']}) "
              f"{'met' if entry['met'] else 'MISSED'}")
    return 0 if all(entry["met"] for entry in verdict.values()) else 4


if __name__ == "__main__":
    sys.exit(main())
