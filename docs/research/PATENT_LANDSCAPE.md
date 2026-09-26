<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Preliminary patent landscape

**Status:** engineering research only, updated 2026-08-05; Stage 4 engineering review added 2026-09-26. This is not a legal opinion, exhaustive search or freedom-to-operate conclusion. Legal status shown by public aggregators must be verified in official registers for each jurisdiction.

## Search clusters

- uncompiled or interpreted device drivers;
- drivers stored in peripheral devices;
- user-mode interrupt delivery;
- non-native interrupt handlers;
- content-addressed software update and rollback;
- immutable system trees and activation;
- source-derived execution caches;
- capability microkernel mechanisms;
- remote recovery by repository identity.

## L-001 — Intel portable uncompiled peripheral driver

- Family/publication: `WO1997024656A1`, priority 1995-12-29.
- Public status indicator: PCT publication shown as ceased.
- Relevant concept: uncompiled source or interpretive driver code stored in memory of a peripheral device, read by a system and compiled or interpreted through an OS driver interface.
- TOS intersection: textual drivers.
- Important distinction: ordinary TOS drivers are repository objects, not necessarily stored in the peripheral itself. Device-carried TOS drivers would require renewed family and jurisdiction review.
- Research URL: `https://patents.google.com/patent/WO1997024656A1/en`

## L-002 — Microsoft user-mode interrupt delivery

- US patent: `US7581051B2`, priority 2005-05-16.
- Public US status indicator: expired/lapsed; international family status must be checked separately.
- Relevant concept: masking interrupts below CPU level through APIC, bus or device mechanisms while notifying a user-mode driver through a generic kernel service.
- TOS intersection: user-space drivers and interrupt broker.
- Design note: do not copy the exact mechanism without checking surviving family members. TOS should specify a general interrupt capability and platform-specific delivery backend.
- Research URL: `https://patents.google.com/patent/US7581051B2/en`

## L-003 — Non-native/Java interrupt handler stack

- US publication/grant family: `US20020049865A1` / `US7058929B2` among a large grouped disclosure.
- Public US status indicator: expired.
- Relevant concept: a prepared non-native thread stack switched to on interrupt, restrictions around blocking and garbage collection, Java/non-native bytecode at interrupt level.
- TOS intersection: interpreted driver interrupt handling.
- Design note: TOS currently prefers nucleus interrupt acknowledgement and user-space event delivery rather than running a rich GC language directly at hardware interrupt level.
- Research URL: `https://patents.google.com/patent/US20020049865A1/en`

## L-004 — Oracle CAS software-home patch and rollback

- US patent: `US10762059B2`, priority 2018-01-31.
- Public status indicator: active, adjusted expiration shown as 2038-12-19.
- Relevant claim concepts observed in the public record: content-derived filenames, links from a software-home directory to content-addressed objects, updating links, preserving former links in patch mementos and rollback by restoring those links.
- TOS intersection: content-addressed system activation and rollback.
- Design response: TOS uses a commit/tree/blob graph, immutable commit-addressed `/system`, candidate refs and boot records. Do not implement the Oracle-specific hard-link/filename/patch-memento structure without a claim review.
- Research URL: `https://patents.google.com/patent/US10762059B2/en`

## Stage 4 engineering review (2026-09-26)

`docs/16` §Cross-stage gates requires, before Stage 4 closes, a review of the
user-space interrupt, DMA and interpreted-driver mechanisms; `docs/24` §Review
procedure is how. This section is steps 1–6 of that procedure for the mechanisms
Stage 4 actually built. **It is engineering research, not a legal opinion, and it
claims no mechanism is free of patents.** Step 8 — preserving a decision — is the
Project Architect's, and nothing here is that decision. Statuses are what the
public aggregator showed on the date above and must be verified in the official
register of each jurisdiction before anyone relies on them.

### The mechanisms, stated precisely (step 1)

| | TOS mechanism | Where it is decided |
|---|---|---|
| M1 | a device driver is canonical TOS Core text held in the capsule (and, from Stage 5, the repository), checked, lowered and interpreted by the runtime inside an ordinary user process; nothing is read from the peripheral | ADR-0027, ADR-0048, ADR-0079 |
| M2 | the nucleus owns the function's MSI-X table; a driver holds a `platform.irq.Source` derived from its assignment and blocks in `irq_wait`; ring 0 acknowledges at the local APIC and wakes the waiter through a one-bit latch; no line is masked and re-enabled per delivery, and no driver code runs in interrupt context | ADR-0082 |
| M3 | a `DmaRegion` requires both a function capability with `dma` and a memory authority with `spend`; the device-visible address is data issued for a bounded offset; released backing is quarantined until bus mastering has stopped and Transactions Pending reads clear; no IOMMU on the reference profile | ADR-0084, ADR-0086 |

### What was found (steps 2–4)

- **L-001** (`WO1997024656A1`): the national-phase family the aggregator lists is
  `US5835772A` (expired, fee-related), `AU1568497A` (abandoned), `TW318227B`
  (right ceased). Every independent claim read requires the uncompiled driver code
  to be **stored in the peripheral device's own memory** and read from it.
- **L-002** (`US7581051B2`): US, EP (`EP1889165B1`), JP, CN, BR and RU members are
  shown expired, lapsed or ceased; `CA` withdrawn, `KR` abandoned. **`MX2007014338A`
  is shown as an active grant and `ATE538436T1` as active** — the latter is the
  Austrian validation of an EP patent the same page shows expired at the end of its
  lifetime, so the indicator is doubtful, and both need the register. The
  independent claims read combine a registered user-mode driver, a
  device-independent interrupt interface, masking the interrupt **below processor
  level (APIC or bus controller)** while the user-mode handler runs, a generic
  kernel-mode routine run after kernel drivers' routines, and re-enabling the line.
- **L-003** (`US20020049865A1` / `US7058929B2`): the granted US claims read are
  about fragment-based compilation of dominant execution paths, and the grant is
  shown expired (2020). **The landscape's association of this family with
  interrupt-level non-native stacks is imprecise** and is corrected here: that
  subject is in the grouped disclosure, not in the claims this grant carries.
- **L-005, new — IBM, `US8806511B2`** (priority 2010), executing a kernel device
  driver as a user-space process: shown expired (fee-related). Its independent
  claims read intercept kernel API calls from a user-space driver through a library
  and convey privileged ones to a kernel module through a file descriptor. **One US
  continuation is shown active (granted 2018-11-06); its claims have not been
  read.**
- **L-006, new — Apple, `US11829303B2`** (priority 2019), device driver operation
  in non-kernel space: **shown active, expiring 2041-04-09.** Independent claims
  read concern a non-kernel entity given access to a hardware component and a
  memory allocation while other entities' allocations are excluded, configuration
  from granted resources, deallocation on termination, and — in the method claim —
  an IOMMU used for the allocation. Claim 1 is written about a network interface.
- A concept search for interpreted or bytecode device drivers returned bytecode
  interpreter and VM-acceleration patents and nothing claiming a driver executed
  as interpreted source in user space.

### Engineering claim matrix and design differences (steps 5–6)

| Family | Element the claims read require | M1 | M2 | M3 |
|---|---|---|---|---|
| L-001 | driver code stored in and read from the peripheral | not practised: the driver is a capsule/repository object | — | — |
| L-002 | interrupt masked at APIC/bus level while a user-mode handler runs, then re-enabled | — | **different by design**: edge MSI-X, unshared, one latch; no per-delivery mask/unmask of a line | — |
| L-003 | fragment compilation of dominant paths | TOS has no JIT; interpretation only | — | — |
| L-005 | kernel-API emulation library forwarding privileged calls to a kernel module | — | no emulation layer; operations are capability ABI rows | no emulation layer |
| L-006 | non-kernel hardware access, allocation isolation, deallocation on termination; IOMMU in the method claim | — | overlaps in **general concept** (a user-space driver granted a device and memory, released at termination) | overlaps in general concept; **no IOMMU** on the reference profile |

**Flagged for the Project Architect, not resolved here:** L-006 is active and its
general concept overlaps M2/M3; L-005 has an active continuation whose claims were
not read; L-002 has two family members shown active by the aggregator. A general
overlap is not a finding of practice — every independent claim is a combination,
and none was charted element by element against TOS at claim-construction depth —
but `docs/24` step 7 (counsel) and step 8 (the recorded decision) are the Project
Architect's to take or to decline, and this review does not take them.

## Required follow-up searches

Before Stage 4 — **performed 2026-09-26** as the engineering review above, with
three items flagged for decision:

- active international family claims around user-space interrupt/DMA delivery —
  L-002's remaining members, L-005's continuation and L-006;
- interpreted or bytecode device-driver mechanisms — no specific family found;
- IOMMU capability allocation — **not reachable yet**: the Stage 4 reference
  profile has no IOMMU backend, and the search is owed again when one is built.

Before Stage 5:

- content-addressed OS deployment;
- immutable tree activation and rollback;
- Git-like boot and system-version selection;
- software-home snapshot patents.

Before Stage 7:

- remote recovery, signed fleet activation and repository-based appliance restore.

Before commercial release:

- professional search in intended jurisdictions using final implementation claim charts.

## Recording rule

A patent is not labelled “safe” because it appears old, expired in one country or conceptually similar. Record exact jurisdiction and independent claims. A design difference is an engineering hypothesis until reviewed by qualified counsel.
