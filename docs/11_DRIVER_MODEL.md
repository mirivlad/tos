<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Driver model

## Goal

Device drivers should be ordinary inspectable textual modules with narrowly granted hardware capabilities. A driver crash should normally terminate one process, not the entire operating system.

## Bootstrapping problem

A text driver stored on disk cannot be read until a disk driver exists. TOS solves this with the boot capsule:

1. UEFI loader reads the nucleus and capsule using firmware facilities.
2. Capsule contains boot-critical textual drivers.
3. Nucleus starts the TOS Core runtime from memory.
4. Text driver initializes persistent storage.
5. Repository-backed versions replace capsule versions through a versioned handoff.

Thus the disk driver remains text without placing a full disk stack in the binary nucleus.

## Driver process

A driver instance receives only capabilities for its assigned device and supporting resources, such as:

- PCI function configuration;
- MMIO regions;
- I/O port ranges;
- interrupt endpoint;
- DMA allocator with limits;
- clock or timer service;
- firmware data subset;
- publication rights for a device interface.

It does not receive arbitrary physical memory or unrelated devices.

## Driver manifest

A component declares what it needs in its own module source, using the accepted
TOS Core V1 forms. There is no `manifest` item in the V1 grammar, and ADR-0051
explains why one is not needed: everything a launcher must know before it starts
a component is already in the module header and already in the verified IR.

```tos
module drivers.virtio.net version 1.0 profile bootstrap;

resource [fuel: 4000000, stack: 128KiB, allocation: 64KiB, tasks: 4, workers: 1,
          sync: 2, shared: 0B, cleanup: 32, recursion: 16, imports: 4]

import capability platform.pci.FunctionConfig as pci;   // accepted
import capability platform.irq.Source as irq;           // accepted
import capability platform.mmio.RegionMap as mmio;      // ILLUSTRATIVE — not accepted
import capability platform.dma.Allocator as dma;        // ILLUSTRATIVE — not accepted
import capability net.adapter.V1Publisher as publisher; // ILLUSTRATIVE — not accepted
```

> **Only the paths marked `accepted` are interfaces.** The others are names for
> mechanisms that have not been decided, shown here because the shape of a
> driver's request is worth seeing whole. A module writing one today is rejected
> — `types.rs` resolves an interface path only against the accepted schema
> tables, and an `extern` reaching an undeclared interface is
> `E1801_FFI_NOT_AVAILABLE`.
>
> The accepted platform interfaces are exactly those in
> `source/interfaces/platform/PLATFORM_INTERFACE_V1.md`, which at version 2 is
> `platform.pci.Bus`, `platform.pci.FunctionConfig` and `platform.irq.Source`.
> They are added when their mechanisms are decided, not when this example first
> showed a plausible name — which is why the interrupt line above changed *name*
> as well as status: ADR-0082 decided a **source** derived from a function
> assignment, not the `Binding` this example had sketched, and the sketch is not
> what became real.
>
> **Device memory is no longer open**: ADR-0081 §13 decided it, and it arrived as
> two operations on `platform.pci.FunctionConfig` rather than as a
> `platform.mmio.RegionMap` interface, for the same reason. What remains open is
> **DMA** — ADR-0082 §12 leaves DMA authority, device-visible addressing, the
> IOMMU and the MMIO↔DMA ordering contract undecided — and the class publisher,
> which ADR-0051 leaves open.
>
> This warning exists because the previous revision of this passage was mistaken
> for a settled interface set during the Stage 4A audit: valid V1 syntax,
> plausible paths, a Tier 2 document, and nothing anywhere saying which of them
> a checker would accept. ADR-0051 §4 corrected an earlier version of the same
> passage for a related reason.

Three things are worth reading twice.

The right to publish `net.adapter.v1` is **requested**, not asserted: the
capability's nominal type is the interface it publishes, so the launcher decides
whether this component may offer it. docs/37 names "textual manifest grants
itself authority" as a Stage 3 failure condition, and a self-declared `provides`
line is exactly that.

Resource bounds are the module's declared envelope, which the verifier already
checks — not a second set of numbers beside it that could disagree.

Restart policy, health probes, state namespace and shutdown timeout are absent,
because they are decisions *about* this component rather than descriptions *of*
it. They belong to whoever has authority to launch it, and live in
`/system/policy/` as canonical source.

Device matching — which hardware this driver claims — is a Stage 4 question with
its own answer to find. It is not an authority a launcher grants but a query a
bus manager evaluates, and ADR-0051 deliberately leaves it open rather than
settling it a stage early.

## Driver interfaces

Drivers publish device-class interfaces rather than exposing hardware-specific details to applications. Examples:

- `block.device.v1`;
- `net.adapter.v1`;
- `input.keyboard.v1`;
- `display.scanout.v1`;
- `audio.stream.v1`.

Bus managers and class services may be separate processes.

## Interrupts

The nucleus acknowledges and routes low-level interrupts to driver event endpoints. Drivers must not block interrupt routing indefinitely. Shared interrupts are mediated by a bus or interrupt service with explicit acknowledgement semantics.

**Decided, and not quite as this section anticipated** (ADR-0082). Interrupt
authority is a `platform.irq.Source` **derived from a live PCI function
assignment and from nothing else**: no interrupt-controller capability exists, and
a process holding no function cannot ask for anybody's interrupts. A driver waits
with `irq_wait` on a one-bit latch rather than receiving on an event endpoint, so
a completion cannot be lost by racing the wait, and one wake may cover any number
of completions.

Two sentences above are superseded by that decision and are kept because they
describe the shape a later transport may still need:

- *"driver event endpoints"* — delivery is a blocking operation on the source,
  not a message to an endpoint. An endpoint would put an IPC hop and a queue on
  the completion path, and the latch is what makes batching work instead;
- *"shared interrupts … with explicit acknowledgement semantics"* — the accepted
  transport is **MSI-X**, which is edge-delivered and unshared, so acknowledgement
  is a local-APIC EOI and there is no acknowledge operation at all. ADR-0082 §4
  records why INTx was refused: ending one requires reading the *device's* status
  register, which is device knowledge in ring 0 or an interrupt storm a slow
  driver can cause. A shared transport would need the mediation this section
  describes, and would need it decided rather than inherited.

## DMA

DMA regions are allocated through a trusted service or nucleus primitive. The driver receives a bounded region and device-visible address mapping. IOMMU support should later enforce hardware isolation without changing the driver contract.

**Not decided.** ADR-0082 §12 leaves DMA authority, DMA allocation,
device-visible addressing, the IOMMU and the MMIO↔DMA ordering contract open;
Stage 4C-2 is where they are decided. What is already settled and inherited
rather than restated there is the **bus-mastering predicate** of ADR-0082 §5d: a
DMA mapping is a bus-mastering descendant, and Bus Master Enable is set if and
only if at least one live bus-mastering descendant exists.

**And one thing must not be written by accident** (ADR-0082 §5). On the no-IOMMU
reference profile, TOS cannot claim hardware-enforced confinement of a malicious
bus-mastering device: the capability model controls which sanctioned DMA objects
and device-visible addresses software may *obtain*, and does not physically
prevent a malicious driver from programming a device with some other address. The
last sentence above therefore says "should later enforce" and means it.

## Crashes and restart

A restartable driver declares how it reconstructs state. The supervisor can:

1. revoke device mappings;
2. reset the device through a bus service;
3. start a new driver instance;
4. restore published interface endpoints;
5. notify clients of interruption.

Storage drivers require special care to avoid silent data corruption. A crash may force read-only mode or full device revalidation.

## Porting open drivers

TOS can reuse knowledge from open-source drivers, but most drivers cannot be mechanically copied because they are deeply tied to another kernel's APIs.

Portable knowledge includes:

- register definitions;
- initialization sequences;
- firmware formats;
- packet and descriptor layouts;
- quirks and revision tables;
- error recovery state machines.

The integration layer must be rewritten against TOS bus, DMA, IRQ, memory, and service interfaces. License compatibility and attribution remain mandatory.

## Driver language requirements

Boot-critical drivers use the TOS Core bootstrap profile. Later drivers may use other frontends only if those frontends and runtimes are available before the device is required.

## Physical hardware strategy

Physical hardware support begins only after the QEMU contracts are stable. Priority should go to devices with public specifications and simple reset behavior. GPU and Wi-Fi stacks are separate major programs, not early milestones.

## Devices requiring vendor firmware

Many real devices require a vendor firmware image before they operate. Under
ADR-0030 that image is vendor-controlled opaque material: it lives in
`/vendor`, it is not TOS source, and TOS makes no claim about its behavior.

The driver does not change class because of it. A TOS driver is canonical
readable source that the owner can inspect and modify, including when its
runtime job is to hand a firmware image to a device. Loading vendor firmware is
an action a textual component performs — never a reason for the component itself
to become opaque, and never grounds for shipping a binary driver in place of a
textual one.

A driver requiring vendor firmware declares it in its manifest alongside its
capability requirements: vendor, object identity, version, content hash and
behavior when the object is absent, mismatched or refused. Refusing to load
unavailable firmware and reporting the device as unavailable is a defined
outcome; operating in an undeclared degraded mode is not.

## Source reuse and legal provenance

Open driver source is not automatically reusable code. Porting separates:

- public hardware facts and register behavior;
- protocol sequencing and errata;
- operating-system integration structure;
- expressive source implementation.

The Linux kernel is generally GPL-2.0-only, which is not directly compatible with a GPL-3.0 combined work. TOS therefore prefers public hardware specifications, permissively licensed implementations, GPL-2.0-or-later files or documented clean-room translation of functional knowledge. Every imported table, firmware blob or source fragment receives provenance and licence review.

## Patent-sensitive mechanisms

Before finalizing interrupt delivery, DMA mapping or device-carried text drivers, maintainers review the patent landscape for surviving jurisdictional claims. The driver API should express general capabilities and leave platform mechanisms replaceable rather than copying a vendor’s exact patented sequence.
