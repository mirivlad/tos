<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0022: BootInfo v1 platform handoff

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — defines existing BootInfo v1 fields without
  changing its layout, offsets, major/minor version or source identity
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Decision

`FB_FORMAT_NONE=0`, `FB_FORMAT_RGBX8=1` and `FB_FORMAT_BGRX8=2`. RGBX8 and
BGRX8 name increasing framebuffer-memory byte order; X is ignored. GOP
`PixelRedGreenBlueReserved8BitPerColor` maps to RGBX8 and
`PixelBlueGreenRedReserved8BitPerColor` maps to BGRX8. PixelBitMask and
PixelBltOnly fail closed. Pitch is bytes per scanline, exactly checked
`PixelsPerScanLine * 4` for the supported formats.

Framebuffer is absent only when GOP is absent, in which case its complete tuple
is zero/`NONE`. A present but malformed, unbacked, unsupported, zero-sized or
overflowing GOP mode fails closed. The loader validates base, geometry, minimum
pitch, required bytes and base-plus-size; it reserves the handed-off range from
ordinary usable memory.

`acpi_rsdp` is the selected physical RSDP: ACPI 2.0+ configuration table is
preferred, ACPI 1.0 is used only when it is absent. `smbios` is the selected
physical SMBIOS entry point: SMBIOS 3 is preferred, SMBIOS 2 is used only when
it is absent. A malformed preferred entry fails closed rather than falling back.
The loader checks anchors, declared lengths and checksums (including ACPI v1
and extended v2 checksums and SMBIOS 2 intermediate anchor/checksum). Zero
means absent configuration table, not an ignored malformed one.

The pointers remain physical and valid at handoff after ExitBootServices.
Consumers validate firmware-owned structures before use and do not reclaim them
until copied/read. This does not claim protection from malicious firmware (T7).

## Architecture impact statement

No Tier 0 invariant, trusted dependency, capsule format, recovery path or
BootInfo byte layout changes. This makes the existing platform fields concrete,
fail-closed and observable through optional `TOS.BOOT.HANDOFF` fields. Tests
cover format/geometry and configuration-table selection; QEMU proves real
OVMF values reach the existing handoff path.
