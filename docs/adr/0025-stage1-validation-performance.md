<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0025: Stage 1 validation-performance conformance

- Status: Proposed
- Date: 2026-08-09
- Change level: **Level 2** — fixes the reference performance-evidence
  contract and authorizes only semantics-preserving implementation hardening;
  capsule v1, BootInfo v1 and the source-to-runtime trust boundary do not
  change
- Project Architect direction: preserve the qemu64/TCG profile, the approved
  end-to-end interval and independent loader/nucleus validation; do not weaken
  the 250 ms p95 budget

## Context

`docs/35_PERFORMANCE_CONTRACTS.md` already requires that a release capsule with
1,000 files and exactly 16 MiB of payload validates and locates
`/system/boot/init.tos` within 250 ms p95 on the declared QEMU CI profile.
ADR-0021 also permits no more than one whole-capsule SHA-256 traversal and one
cumulative per-file SHA-256 traversal for every accepted capsule validation.
Boot ABI v1 requires a loader `TOS.CAPSULE.OK` and a second, independently
validated nucleus `TOS.CAPSULE.OK`.

The approved F-18 measurement observes the host-monotonic serial interval from
`TOS.BOOT.ENTRY` through `TOS.BOOTTEXT.PATH` on the ordinary q35/qemu64/TCG,
256 MiB, OVMF, `isa-debug-exit` harness. It thereby includes both required
validations and canonical boot-text lookup without adding a guest clock/event
or a second boot path.

On source commit `58146971cc26a23b9b0bc1835f84b3e07299a759`, the deterministic
detached 1,000-file / exactly-16-MiB workload produced a complete P1 sample
set: median 2765.027 ms, p95 2842.450 ms, p99 2891.580 ms. Intermediate
timestamps show approximately 1372 ms for loader validation and 1189 ms for
nucleus validation; lookup after the latter takes less than 1 ms. The result
is a conformance failure, not a reason to move the timing boundary, discard a
validation, switch to KVM or silently loosen the budget.

## Decision

The Stage 1 performance conformance run is fixed as follows:

- QEMU machine profile is `q35`, `qemu64`, one vCPU, 256 MiB, ordinary TCG
  (no `-enable-kvm`), with the same OVMF discovery and ESP construction as
  `source/host-tools/qemu-test/run.sh`.
- The measured interval is host-monotonic serial-byte arrival from the sole
  `TOS.BOOT.ENTRY` to the sole `TOS.BOOTTEXT.PATH`. Existing intermediate
  `TOS.*` arrival timestamps are retained only as diagnostic trace evidence;
  they do not expand the Boot ABI.
- The fixture has exactly 1,000 canonical files and exactly 16 MiB
  (16 × 1024 × 1024 bytes) of file payload. It uses an ADR-0018 detached
  source-set identity, existing licence notice and a checked provenance
  sidecar; it is generated below ignored `source/target/`, not committed as a
  binary vector.
- The run records three warm-ups and 21 measured samples. Median is the 11th
  ascending sample; p95 is nearest-rank 20 and p99 nearest-rank 21. It records
  raw samples, exact source commit, QEMU/firmware identities, host CPU,
  virtualization mode, guest profile, Rust build identity and an explicit
  baseline. p95 MUST be no greater than 250 ms.
- A successful result must be P2: the normal QEMU CI workflow runs the exact
  harness and uploads fixture, sidecar, serial/event logs, raw samples and
  report. A local P1 run is diagnostic evidence only.

The implementation may optimize the existing dependency-free, no_std
`tos-hash` scalar SHA-256 and capsule validation flow only if all of the
following remain true:

1. the digest algorithm, capsule bytes, parser structured errors and
   deterministic validation precedence remain unchanged;
2. loader and nucleus each independently perform the accepted whole-capsule
   and cumulative per-file digest checks before their respective
   `TOS.CAPSULE.OK` event;
3. the two logical SHA-256 traversals remain bounded as ADR-0021 requires;
   a fused physical payload read is permitted only when it produces the exact
   same two logical digests and rejection outcomes;
4. no CPU feature requirement, QEMU CPU/profile change, external dependency,
   unsafe implementation, assembly backend or host service enters the trusted
   base; and
5. a failed optimization leaves Stage 1 open and returns to architectural
   review rather than changing the metric or hiding a regression.

## Architecture impact statement

- **Invariants/canonical representation:** I-01, I-02, I-09, I-10 and I-18
  remain intact. Canonical source stays textual and the capsule remains a
  disposable, byte-compatible derived transport.
- **Trusted base/dependencies:** the existing no_std SHA-256 and capsule code
  may be made faster in place. No new dependency, privileged service, CPU
  extension, unsafe block or assembly implementation is authorized.
- **Source-to-runtime/recovery:** Git/detached identity, provenance sidecars,
  owner boot control, recovery and rollback are unchanged. The fixture itself
  is reproducibly disposable.
- **Threat model:** hostile capsule bytes remain fully checked twice across the
  loader/nucleus boundary. No validation outcome, structured error or
  fail-closed path may be removed.
- **Performance/compatibility:** this fixes the Stage 1 evidence profile and
  retains the existing 250 ms reference-platform budget. The compatibility
  profile remains qemu64/TCG; KVM, `-cpu max`, SHA extensions and host-only
  measurements cannot satisfy this gate.
- **Licence/patent:** code remains GPL-3.0-or-later and documentation
  CC-BY-SA-4.0. No imported implementation or patent claim is introduced.
- **Evidence:** SHA known-answer/streaming tests; parser/vector/negative/fuzz
  tests; normal QEMU exit 33; exception paths exit 73; 3+21 raw P2 samples and
  a retained CI artifact with p95 ≤250 ms.

## Consequences

The existing F-18 result is an explicit failure baseline. A scalar-only,
semantics-preserving optimization may prove insufficient; if so, this ADR does
not authorize a shortcut. Any proposal to change the CPU profile, introduce
an accelerated/unsafe backend, reduce independent validation, alter capsule
format or revise the budget requires a separate architect-reviewed ADR.
