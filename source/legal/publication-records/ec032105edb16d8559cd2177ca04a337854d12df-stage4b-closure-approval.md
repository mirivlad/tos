<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Project Architect approval — TOS Stage 4B closure

Project Architect Vladimir Tomashevskiy closed Stage 4B on 2026-09-04 for
evidence commit `ec032105edb16d8559cd2177ca04a337854d12df`.

## Decision, as given

> Project Architect closure ruling.
>
> I have reviewed the final Stage 4B closure evidence at:
> `ec032105edb16d8559cd2177ca04a337854d12df`
>
> The repair is accepted.
>
> The following are accepted as the final Stage 4B evidence basis:
>
> * canonical TOS Core 1.2 discovers the real VirtIO PCI capabilities itself;
> * it derives a bounded MMIO window from a live `PciFunction`;
> * the caller never supplies a physical address;
> * BAR numeric values remain data, not authority;
> * `MmioRegion` is distinct from ordinary `Region` and `DmaRegion`;
> * device memory is not funded by `MemoryAuthority`;
> * MMIO descendants remain bound to the PCI assignment lifecycle;
> * stale assignments cannot reach later assignments of the same BDF;
> * process death drains mappings and restores the accounted reserves;
> * real VirtIO registers are read from the QEMU device by canonical textual
>   TOS Core;
> * the nucleus contains mechanism only and no VirtIO interpretation;
> * the negative authority/bounds/lifetime cases are gated;
> * all 48 declared freestanding feature configurations are now type-checked;
> * ADR-0081 records the real implementation-before-approval chronology without
>   rewriting history;
> * the narrowed approval scope of ADR-0081 is accepted;
> * ordinary preflight is 38/38;
> * local full preflight differs only by the two documented host-side ADR-0066
>   observer-QEMU prerequisites;
> * GitHub Actions on `ec03210` are all green:
>    * Source CI;
>    * generated specification / documentation integrity;
>    * provenance;
>    * QEMU boot.
>
> The previously red QEMU workflow was correctly treated as a real independent
> failure and fixed at its cause rather than being misclassified as the local
> observer-QEMU limitation.
>
> No Stage 1–4A gate was weakened.
>
> No IRQ, DMA, Virtqueue, block-I/O or reset semantics are implied by this
> approval.
>
> Therefore:
>
> Stage 4B — BAR/MMIO and real textual VirtIO PCI capability discovery — is
> formally closed by Project Architect approval.
>
> Closure basis:
> `ec032105edb16d8559cd2177ca04a337854d12df`

## The closure basis, as the ruling states it

- `docs/evidence/STAGE4B_MMIO_BOUNDARY.md` at the closure commit, and the
  decision it rests on, `docs/adr/0081-region-access-and-device-memory.md`
  (**Accepted**, 2026-09-04), including its §0 chronology;
- canonical TOS Core 1.2 reads the real device: `num_queues=1`,
  `device_status=0x00`, `config_generation=0`, `queue0_size=256`,
  `features=0x0101`, obtained through a window the module derived itself from a
  live `PciFunction`;
- the negative gates: eight mapping refusals, bounds refused before the device
  is touched, the no-device probe, the stale-assignment cases, and the
  process-death drain checked against the memory account;
- `scripts/tests/check-feature-builds.sh`: all **48** declared freestanding
  feature configurations of `tos-runtime-image` and `tos-nucleus` type-checked;
- ordinary preflight **38/38**;
- local `preflight --full` **86 of 88**, the two failures being the documented
  host-side ADR-0066 observer-QEMU prerequisites, which report their own
  missing-prerequisite message and exit before any build step;
- all four GitHub Actions workflows green on `ec03210` — Source CI,
  Documentation integrity, Provenance gates, QEMU boot gate — with the QEMU
  workflow's own profile reporting `PREFLIGHT PASS: 48 gate(s) passed`;
- **no conformance threshold, gate or accepted memory figure was changed to
  obtain closure.** The memory-account equation was reconciled by naming the
  device-mapping reserve term, not by adjusting the accepted total.

## What this approval covers

The evidence tree is commit `ec032105edb16d8559cd2177ca04a337854d12df`, and the
artifacts that commit's `MANIFEST.txt` and `SHA256SUMS` pin.

This record is written in a later commit because a record cannot contain its own
hash. **The reviewed tree is the one named here.** The commit that adds this
record performs closure bookkeeping only — status lines, this approval record
and current-status prose — and changes no nucleus behaviour, no TOS Core
semantics, no IR or image format, no ABI operation, no supervisor behaviour, no
restart policy, no memory figure or ceiling, and no performance threshold. The
technical evidence commit is not altered.

The ruling also records two governance findings as accepted rather than
forgiven: ADR-0081 was implemented before it was approved, and the previously
red QEMU workflow was a real independent failure. Both stay in the record —
ADR-0081 §0 and `STAGE4B_MMIO_BOUNDARY.md` §3a — as written.

## What this approval does not cover

It closes Stage 4B only. It authorizes no part of Stage 4C, and the ruling names
the exclusions itself: **no IRQ, DMA, Virtqueue, block-I/O or reset semantics
are implied by this approval.** The narrowed scope ADR-0081 states for itself is
accepted as stated, so this closure also decides none of:

- DMA ordering, DMA address semantics, coherency or IOMMU;
- interrupt semantics or routing;
- device reset or feature negotiation beyond read-only observation;
- Virtqueue semantics;
- volatile ordinary RAM, arbitrary physical mappings, pointer arithmetic,
  unsafe pointer access, or generic native-memory FFI.

Two obligations carried into this closure stay open, unchanged by it:

- the `SYSTEM_ABI_V1` §6 liveness rule is implemented as its first half only,
  and remains a **mandatory prerequisite to the first routed device interrupt**
  (`STAGE4A_HARDWARE_BOUNDARY.md` §8, `STAGE4B_MMIO_BOUNDARY.md` §12);
- the **Stage 4 identity gate is not claimed**. No persistent data has moved,
  and Stage 4B does not assert it.

ADR-0075 and ADR-0076 are **not** widened to cover device memory by this
closure, and neither is `MemoryAuthority` funding.
