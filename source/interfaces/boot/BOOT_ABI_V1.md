<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Boot ABI — Version 1

Status: **Accepted Tier 2 interface contract.**

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs.

## 1. Role

The loader constructs one `BootInfo` block in memory, then transfers control to
the nucleus entry point exactly once. The block is a versioned standalone ABI:
no Rust layout is authoritative; this byte layout is.

## 2. Constants

| Name | Value | Meaning |
|---|---|---|
| `MAGIC` | `u64::from_le_bytes(b"TOSBOOT1")` = `0x31544F4F42534F54` | ABI magic |
| `PROTOCOL_UUID` | `e2e8c15a-6c4b-4d11-9a2c-8f3b1a2c4d5e` (16 bytes, RFC order) | ABI identity |
| `MAJOR` / `MINOR` | 1 / 0 | ABI version |
| `STRUCT_SIZE` | 224 | size of BootInfo v1 |
| `ARCH_X86_64` | 1 | architecture id |
| `BOOT_MODE_NORMAL` | 0 | boot mode |
| `FB_FORMAT_NONE` | 0 | framebuffer absent |
| `MEM_DESC_SIZE` | 24 | memory-range descriptor size |
| `RESULT_PORT` | `0x501` | QEMU `isa-debug-exit` I/O port |
| `RESULT_HALT_OK` | `0x10` | clean halt |
| `RESULT_PANIC` | `0x20` | nucleus panic |
| `RESULT_CAPSULE_INVALID` | `0x21` | capsule rejected |
| `RESULT_ABI_INVALID` | `0x22` | boot ABI rejected |
| `RESULT_MEMORY_INVALID` | `0x23` | memory map rejected |

Result codes are written to `RESULT_PORT` as one `u8`; QEMU exits with
`(value << 1) | 1` when `isa-debug-exit` is configured.

## 3. Calling convention

- Entry: 64-bit long mode, paging enabled (identity mapping covering the kernel
  image, capsule, boot info and memory-map regions), GDT valid.
- Argument: `rdi = physical address of BootInfo`.
- `rsp`: valid stack configured by the loader.
- No other register contents are part of the ABI.
- The nucleus must not return; it either halts via `RESULT_PORT` or panics.

## 4. BootInfo layout (224 bytes, little-endian, 8-aligned)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 8 | `magic` | must equal `MAGIC` |
| 8 | 16 | `protocol_uuid` | must equal `PROTOCOL_UUID` |
| 24 | 2 | `major` | must be 1 |
| 26 | 2 | `minor` | 0; unknown minor with same major is rejected |
| 28 | 4 | `total_size` | `>= STRUCT_SIZE`; extra bytes are reserved, must be zero |
| 32 | 4 | `architecture_id` | must be `ARCH_X86_64` |
| 36 | 4 | `boot_mode` | `BOOT_MODE_NORMAL` |
| 40 | 8 | `memory_map_phys` | physical address of memory-range array |
| 48 | 8 | `memory_map_length` | byte length of the array |
| 56 | 8 | `memory_desc_size` | must equal `MEM_DESC_SIZE` |
| 64 | 8 | `framebuffer_phys` | 0 if absent |
| 72 | 4 | `framebuffer_width` | 0 if absent |
| 76 | 4 | `framebuffer_height` | 0 if absent |
| 80 | 4 | `framebuffer_pitch` | 0 if absent |
| 84 | 4 | `framebuffer_format` | `FB_FORMAT_NONE` if absent |
| 88 | 8 | `capsule_phys` | physical address of the capsule |
| 96 | 8 | `capsule_length` | byte length of the capsule |
| 104 | 32 | `capsule_digest` | SHA-256 of capsule bytes |
| 136 | 1 | `capsule_identity_kind` | mirrors capsule header field |
| 137 | 1 | `capsule_oid_alg` | mirrors capsule header field (0 none, 1 SHA-1, 2 SHA-256) |
| 138 | 1 | `capsule_oid_length` | mirrors capsule header field (20/32, or 0 when no OID) |
| 139 | 5 | `reserved` | zero |
| 144 | 32 | `capsule_source_identity` | mirrors capsule header field |
| 176 | 8 | `acpi_rsdp` | physical address of RSDP, 0 if absent |
| 184 | 8 | `smbios` | physical address of SMBIOS table, 0 if absent |
| 192 | 8 | `next` | 0; reserved extension pointer |
| 200 | 24 | `reserved` | zero |

## 5. Memory-range descriptor (24 bytes)

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `phys_start` |
| 8 | 8 | `phys_length` |
| 16 | 4 | `ty` (`1` usable, `2` reserved, `3` ACPI reclaimable, `4` ACPI NVS, `5` MMIO) |
| 20 | 4 | `flags` (`bit0` executable, `bit1` writable; reserved bits zero) |

The loader converts the UEFI memory map to this descriptor set: contiguous
usable ranges are merged; the array is sorted by `phys_start`; entries do not
overlap; `phys_length` is non-zero.

## 6. Capsule identity binding

`capsule_digest`, `capsule_identity_kind`, `capsule_oid_alg`,
`capsule_oid_length` and `capsule_source_identity` are copied from the
verified capsule header (`whole_capsule_digest`, `source_identity_kind`,
`source_oid_alg`, `source_oid_length`, `source_identity_value`). The nucleus
re-verifies the capsule digest against the bytes at `capsule_phys`.

## 7. Stable diagnostic events

Every serial event line begins with one stable identifier matching
`^TOS\.[A-Z0-9_.]+`. Human-facing console text is outside this ABI.

### Success order

An ordinary successful boot emits the following identifiers in this exact
order. `TOS.CAPSULE.OK` occurs exactly twice: first after loader validation,
then after the nucleus independently validates the capsule.

```text
TOS.BOOT.ENTRY
TOS.CAPSULE.OK files=<decimal>
TOS.BOOT.HANDOFF nucleus=0x<hex> stack=0x<hex> bootinfo=0x<hex>
TOS.NUCLEUS.ENTRY
TOS.CAPSULE.OK files=<decimal>
TOS.BOOTTEXT.PATH <canonical-absolute-path>
[TOS.BOOTTEXT.LINE <first-logical-source-line>]
TOS.BOOTTEXT.DIGEST <64-lowercase-hex-digits>
TOS.IDENTITY source_kind=<git|detached> source_digest=<64-lowercase-hex-digits> capsule_digest=<64-lowercase-hex-digits> arch=0.2.1 builder=1
TOS.HALT ok=0x10
```

`TOS.BOOTTEXT.LINE is optional`: it is emitted only when the canonical boot
text has a first logical line. The other success identifiers and the shown
fields are mandatory. A consumer may rely on the listed identifier order,
including both `TOS.CAPSULE.OK` events.

### Failure vocabulary and extension rule

The following identifiers are stable Boot ABI v1 failures:

| Identifier | Required payload / fields | Meaning |
|---|---|---|
| `TOS.BOOT.FAILC` | `capsule_err=<CapsError>` | Loader rejected capsule bytes before handoff. |
| `TOS.BOOT.FAILI` | `<reason-token>` | Loader infrastructure failure before handoff. |
| `TOS.ABI.FAIL` | none | Nucleus rejected BootInfo ABI bytes. |
| `TOS.MEM.FAIL` | none | Nucleus rejected memory-map data. |
| `TOS.CAPSULE.FAIL` | none | Nucleus rejected capsule data after handoff. |
| `TOS.IDENTITY.MISMATCH` | `bootinfo-vs-capsule-header` | Nucleus rejected the mirrored identity. |
| `TOS.PANIC` | `<component>` | Trusted component stopped by panic. |

`TOS.BOOT.FAILI` is a stable identifier. Existing reason tokens retain their
meaning: `no-boot-services`, `no-loaded-image`, `no-fs`, `no-volume`,
`no-capsule`, `no-nucleus`, `alloc-nucleus`, `alloc-stack`, `memmap-probe`,
`memmap-descsize`, `alloc-map`, `alloc-ranges`, `alloc-bootinfo`,
`memmap-fill`, `memmap-toomany`, `map-overflow`, `unsorted-map`, and `exit-bs`.
An implementation MAY add a reason token in Boot ABI v1, but it MUST NOT change
the meaning of an existing token.

Mandatory fields and raw payloads above are a stable prefix. An implementation
MAY append optional fields in `key=value` form after that prefix; optional
fields must not alter, remove or reinterpret mandatory fields, so parsers that
consume the v1 prefix remain compatible.

### Identity record

`TOS.IDENTITY` carries the machine-readable Stage 1 identity record. Its
required fields are, in this order:

```text
source_kind=
source_digest=
capsule_digest=
arch=
builder=
```

`source_kind` is `git` or `detached`; `source_digest` is the exact 32-byte
header identity rendered in lower-case hexadecimal (with zero padding for a
raw SHA-1 OID); `capsule_digest` is the validated whole-capsule SHA-256;
`arch` is the capsule architecture-spec version; and `builder` is the capsule
builder version. Their spelling and semantics are part of Boot ABI v1.

## 8. Validation summary (reject conditions)

1. magic mismatch;
2. protocol UUID mismatch;
3. unsupported major version;
4. `total_size < STRUCT_SIZE` or reserved trailing bytes non-zero;
5. wrong architecture id;
6. memory map region outside addressable bounds, unsorted, overlapping or with
   zero-length entries;
7. `memory_desc_size != MEM_DESC_SIZE`;
8. capsule region outside memory-map bounds or digest mismatch.

The nucleus halts with the corresponding `RESULT_*` code instead of continuing
when validation fails.
