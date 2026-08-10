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

## Driver manifest example

```tos
module drivers.virtio.net

manifest driver {
    matches pci(vendor: 0x1af4, device: [0x1000, 0x1041])
    runtime_profile "bootstrap-capable"

    requires {
        capability pci.configure
        capability mmio.map
        capability irq.bind
        capability dma.allocate(max: 64 MiB)
        capability service.publish("net.adapter.v1")
    }

    provides "net.adapter.v1"
    state none
    restart restartable
}
```

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

## DMA

DMA regions are allocated through a trusted service or nucleus primitive. The driver receives a bounded region and device-visible address mapping. IOMMU support should later enforce hardware isolation without changing the driver contract.

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
