<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Preliminary patent landscape

**Status:** engineering research only, updated 2026-08-05. This is not a legal opinion, exhaustive search or freedom-to-operate conclusion. Legal status shown by public aggregators must be verified in official registers for each jurisdiction.

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

## Required follow-up searches

Before Stage 4:

- active international family claims around user-space interrupt/DMA delivery;
- interpreted or bytecode device-driver mechanisms;
- IOMMU capability allocation.

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
