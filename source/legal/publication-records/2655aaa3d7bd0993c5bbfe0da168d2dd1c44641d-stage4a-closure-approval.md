<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Project Architect approval — TOS Stage 4A closure

Project Architect Vladimir Tomashevskiy closed Stage 4A on 2026-09-04 for
evidence commit `2655aaa3d7bd0993c5bbfe0da168d2dd1c44641d`.

This record was archived after the Stage 4B closure record, and the ruling below
is the one that closes Stage 4A. Until it was archived, Stage 4A was the only
closed stage whose approval lived in the working log rather than here; that is
corrected by this file.

## Decision, as given

> Stage 4A — Hardware authority boundary and real textual PCI configuration
> access — is formally closed by Project Architect approval.
>
> Closure basis:
> `2655aaa3d7bd0993c5bbfe0da168d2dd1c44641d`
>
> The accepted evidence includes:
>
> * explicit root PCI Bus authority originates at the platform/launcher
>   boundary;
> * ordinary runtime code cannot mint that root;
> * `PciFunction` names exactly one live assigned PCI function;
> * BDF scalars are data, not authority;
> * assignment is exclusive and generation-bound;
> * configuration reads/writes require the exact live capability and rights;
> * canonical TOS Core 1.1 claims the real QEMU function and reads its actual
>   PCI configuration;
> * observed values are:
>    * vendor `0x1AF4`;
>    * device `0x1042`;
>    * class `0x01`;
>    * subclass `0x00`;
>    * capability pointer `0x98`;
> * those values are not embedded in the textual module;
> * removing the device makes the same probe report `vendor=0xFFFF`;
> * the authority/bounds/alignment/stale-generation/exclusivity negatives are
>   gated;
> * forged scalar capability use is refused by the independent verifier;
> * TOS Core 1.1 correctly separates startup capability requests from interface
>   effect declarations;
> * direct interface effects grant no authority by themselves;
> * no Stage 1–3 gate was weakened.
>
> The GitHub Actions workflows on the closure basis commit were all green:
>
> * Source CI — success;
> * generated specification — success;
> * provenance — success;
> * QEMU boot — success.
>
> The associated local full preflight was `83 of 85`; the two non-passing gates
> were the documented ADR-0066 host-side observer-QEMU prerequisites that exited
> before any TOS artifact was involved.
>
> This approval closes only:
>
> Stage 4A — Hardware authority boundary and real textual PCI configuration
> access.
>
> It does not approve or imply:
>
> * BAR/MMIO mapping;
> * device-memory semantics;
> * IRQ;
> * DMA;
> * IOMMU;
> * device reset;
> * VirtIO queue setup;
> * block I/O;
> * persistent storage;
> * repository handoff.
>
> The `SYSTEM_ABI_V1 §6` routed-interrupt liveness defect crosses this closure
> unchanged and remains a mandatory prerequisite before the first routed device
> interrupt.
>
> Project Architect:
> Vladimir Tomashevskiy
>
> Date:
> 2026-09-04

## The closure basis, as the ruling states it

- `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md` at the closure commit, and the
  two decisions it rests on: `docs/adr/0079-hardware-authority-origin.md`
  (**Accepted**, 2026-09-03) and
  `docs/adr/0080-capability-effects-name-interfaces.md` (**Accepted**,
  2026-09-04);
- the real read by canonical TOS Core 1.1 — vendor `0x1AF4`, device `0x1042`,
  class `0x01`, subclass `0x00`, capability pointer `0x98` — none of it embedded
  in the module, and the device-absent differential reporting `vendor=0xFFFF`
  through the same probe;
- the negative authority evidence: origin, bounds, alignment, stale generation
  and exclusivity, with a forged scalar refused by the independent verifier;
- all four GitHub Actions workflows green on `2655aaa`;
- local `preflight --full` **83 of 85**, the two non-passing gates being the
  documented host-side ADR-0066 observer-QEMU prerequisites, which report their
  own missing-prerequisite message and exit before any TOS artifact is involved;
- **no Stage 1–3 gate was weakened**, and the Stage 4 device profile is opt-in,
  reached only through `run.sh --stage4-block-device`.

## What this approval covers

The evidence tree is commit `2655aaa3d7bd0993c5bbfe0da168d2dd1c44641d`, and the
artifacts that commit's `MANIFEST.txt` and `SHA256SUMS` pin.

This record is written in a later commit because a record cannot contain its own
hash, and later still than the Stage 4B record for the separate reason that it
was missing. **The reviewed tree is the one named here.** The commit that adds
this record performs closure bookkeeping only — status lines, this approval
record and current-status prose — and changes no nucleus behaviour, no TOS Core
semantics, no IR or image format, no ABI operation, no interface, no test, no
threshold and no memory-account figure. Neither the Stage 4A evidence commit
`2655aaa` nor the Stage 4B basis `ec03210` nor the Stage 4B closure record is
altered.

## What this approval does not cover

It closes Stage 4A only, and the ruling names its exclusions itself: **BAR/MMIO
mapping, device-memory semantics, IRQ, DMA, IOMMU, device reset, VirtIO queue
setup, block I/O, persistent storage and repository handoff** are neither
approved nor implied by it.

BAR/MMIO mapping and device-memory semantics were decided later and separately,
by ADR-0081 under the Stage 4B closure
(`ec032105edb16d8559cd2177ca04a337854d12df-stage4b-closure-approval.md`). Nothing
in that later closure is enlarged by this one, and nothing in this one reaches
forward into it.

Two obligations cross this closure unchanged:

- the `SYSTEM_ABI_V1` §6 routed-interrupt liveness defect — the rule is
  implemented as its first half only, and remains a **mandatory prerequisite
  before the first routed device interrupt** (`STAGE4A_HARDWARE_BOUNDARY.md` §8);
- the **Stage 4 identity gate is not claimed.** Discovery is not persistent
  data, and Stage 4A does not assert it.
