<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Boot Capsule Format — Version 1

Status: **Accepted Tier 2 interface contract.**

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, including
ADR-0016 through ADR-0021 where they decide capsule v1 semantics.

This document defines capsule v1 before any implementation exists. The parser is
total over arbitrary bytes: every rule below has an exact rejection behaviour.

## 1. Role

The capsule is a deterministic, immutable, read-only archive that transports the
first textual system content from the loader into the nucleus. It is **not** the
installed system: it is a transport and recovery seed. Deleting or replacing it
must not claim to be a system update.

## 2. Constants

| Name | Value | Meaning |
|---|---|---|
| `MAGIC` | bytes `54 4F 53 43 41 50 53 55` (`"TOSCAPSU"`) | format magic |
| `FORMAT_UUID` | `2c4f78b3-9d1e-4b0a-9f2c-1a5c8e0d6f71` (16 bytes, RFC order) | format identity |
| `FORMAT_VERSION` | 1 | version of this document |
| `HEADER_SIZE` | 184 | fixed header length |
| `ALIGNMENT` | 8 | structural alignment unit of the header and the fixed entry sizes (see §2.1) |
| `PATH_ENTRY_SIZE` | 16 | fixed path-table entry size |
| `FILE_ENTRY_SIZE` | 64 | fixed file-table entry size |
| `DIGEST_BYTES` | 32 | SHA-256 digest length |
| `ARCH_SPEC_VERSION` | `0x000201` | packed `0.2.1` (`major<<16|minor<<8|patch`) |
| `BUILDER_VERSION` | 1 | capsule builder contract version |

Byte order: all multi-byte integers are **little-endian**.

### 2.2 Resource bounds (ADR-0021)

All maxima are inclusive. `KiB = 1024` bytes and `MiB = 1024 * 1024` bytes.

| Constant | Value |
|---|---:|
| `MAX_CAPSULE_BYTES` | 32 MiB |
| `MAX_FILE_COUNT` | 4096 |
| `MAX_PATH_BYTES` | 1024 bytes per path |
| `MAX_NAME_ARENA_BYTES` | 1 MiB |
| `MAX_LICENCE_NOTICE_BYTES` | 64 KiB |

These limits apply jointly; satisfying one does not weaken any other limit.
The UEFI loader MUST reject an EFI capsule file larger than
`MAX_CAPSULE_BYTES` from its file-size metadata before allocating a pool buffer
or reading the complete file. The parser remains allocation-free and applies
gross limits before payload hashing or a full table walk where structurally
possible. The builder applies the same maxima with checked conversions and
MUST NOT silently truncate a field.

An accepted capsule permits at most two linear hash traversals of capsule or
payload bytes: one for `whole_capsule_digest` and one cumulative traversal for
per-file `content_digest` values. Detached source identity uses those validated
digest values and MUST NOT hash file contents again.

### 2.1 Alignment semantics (ADR-0017)

`ALIGNMENT` constrains the fixed structural sizes, not every offset:

- `HEADER_SIZE`, `PATH_ENTRY_SIZE` and `FILE_ENTRY_SIZE` are multiples of
  `ALIGNMENT`;
- `path_table_offset == HEADER_SIZE`, and is therefore `ALIGNMENT`-aligned;
- `file_table_offset`, `payload_offset` and `content_offset` are **not** required
  to be multiples of `ALIGNMENT`: the name arena and the file contents have
  arbitrary byte lengths, and padding them would change the bytes of every
  capsule.

Consequently an implementation must not assume aligned access anywhere in the
capsule. Every field is decoded byte-wise from the little-endian encoding above;
casting capsule bytes to a target struct is not a conforming implementation
technique.

## 3. Header layout (184 bytes)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 8 | `magic` | must equal `MAGIC` |
| 8 | 16 | `format_uuid` | must equal `FORMAT_UUID` |
| 24 | 2 | `format_version` | must equal `FORMAT_VERSION` |
| 26 | 2 | `header_size` | must equal `HEADER_SIZE` |
| 28 | 2 | `alignment` | must equal `ALIGNMENT` |
| 30 | 2 | `reserved` | must be zero |
| 32 | 8 | `total_length` | total capsule length in bytes; must equal input length |
| 40 | 8 | `path_table_offset` | absolute offset of path table |
| 48 | 4 | `path_table_count` | number of path entries |
| 52 | 4 | `path_entry_size` | must equal `PATH_ENTRY_SIZE` |
| 56 | 8 | `file_table_offset` | absolute offset of file table |
| 64 | 4 | `file_count` | number of file entries |
| 68 | 4 | `file_entry_size` | must equal `FILE_ENTRY_SIZE` |
| 72 | 8 | `payload_offset` | absolute offset of payload region |
| 80 | 8 | `payload_length` | length of payload region |
| 88 | 4 | `arch_spec_version` | must equal `ARCH_SPEC_VERSION` |
| 92 | 4 | `builder_version` | must equal `BUILDER_VERSION` |
| 96 | 1 | `source_identity_kind` | `0` none, `1` git commit, `2` detached source set |
| 97 | 1 | `source_oid_alg` | `0` none, `1` SHA-1, `2` SHA-256 (see §6) |
| 98 | 1 | `source_oid_length` | OID byte length: 20 (SHA-1) or 32 (SHA-256); 0 when no OID |
| 99 | 1 | `reserved` | must be zero |
| 100 | 32 | `source_identity_value` | raw git object id (left-aligned, zero-padded) or detached source-set digest (see §6) |
| 132 | 4 | `reserved` | must be zero |
| 136 | 8 | `licence_notice_offset` | absolute offset of licence-notice text; 0 if absent |
| 144 | 8 | `licence_notice_length` | length of licence-notice text; 0 if absent |
| 152 | 32 | `whole_capsule_digest` | SHA-256 over capsule with this field zeroed |
| 184 | — | — | end of header |

## 4. Tables

The capsule layout is strictly sequential:

```text
[header] [path table] [name arena] [file table] [payload] [licence notice]
```

The layout admits **no undescribed bytes**: every byte of the capsule belongs to
exactly one of the six regions above (ADR-0017). In particular
`path_table_offset == HEADER_SIZE` — the path table begins immediately after the
header, with no gap.

The name arena begins immediately after the path table and ends exactly at
`file_table_offset`. The file table ends exactly at `payload_offset`. The
licence notice, when present, is the exact tail of the capsule. Hence:

- `path_table_offset == HEADER_SIZE`;
- `file_table_offset == path_table_offset + path_table_count * PATH_ENTRY_SIZE + name_arena_length`;
- `payload_offset == file_table_offset + file_count * FILE_ENTRY_SIZE`;
- `payload_offset + payload_length + licence_notice_length == total_length`;
- when the licence notice is absent, both `licence_notice_offset` and
  `licence_notice_length` are zero;
- when present, `licence_notice_offset == payload_offset + payload_length` and
  `licence_notice_offset + licence_notice_length == total_length`.

### 4.1 Path entry (16 bytes)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 4 | `name_offset` | offset of UTF-8 name relative to name arena start |
| 4 | 4 | `name_length` | byte length of name; non-zero |
| 8 | 4 | `file_index` | index into the file table; must be `< file_count` |
| 12 | 4 | `flags` | bit 0: boot-canonical file; **only bit 0 is defined for path entries** — all other bits must be zero |

Path names must be **canonical absolute paths**:

- start with `/`;
- valid UTF-8; no NUL bytes; no control characters;
- no `.` or `..` components; no empty components (`//`), no trailing `/`;
- lexically sorted in ascending byte order over the whole table;
- distinct (no duplicate names).

**Packed name arena (ADR-0017).** The names tile the arena exactly, in path-table
order:

- `path_entry[0].name_offset == 0`;
- `path_entry[i].name_offset == path_entry[i-1].name_offset + path_entry[i-1].name_length`;
- the end of the last name equals `file_table_offset`.

No byte of the arena is outside a name; names neither overlap nor leave gaps.

**Canonical index mapping (ADR-0017).** The path table is a **bijection onto the
file table**, realised canonically:

- `path_table_count == file_count`;
- `path_entry[i].file_index == i`.

The file table and the payload therefore follow the same order as the
name-sorted path table. A non-canonical permutation of `file_index` — including
one that happens to be a valid bijection — is rejected, so that a given file set
has exactly one valid capsule encoding and the check costs a single O(n) pass.

### 4.2 File entry (64 bytes)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 8 | `content_offset` | offset of file content **relative to `payload_offset`** |
| 8 | 8 | `content_length` | byte length of content |
| 16 | 32 | `content_digest` | SHA-256 of content bytes |
| 48 | 4 | `file_flags` | bit 0: boot-canonical; bit 1: licence notice; **only bits 0-1 are defined** — reserved bits must be zero |
| 52 | 12 | `reserved` | must be zero |

Content constraints:

- the payload is the exact byte-to-byte concatenation of file contents in
  file-table order: consecutive files are adjacent, so the union of
  `[content_offset, content_offset + content_length)` over all files equals
  `[0, payload_length)`, and `content_offset` need not be aligned;
- content regions are pairwise disjoint (no overlap) — enforced by requiring
  that the cumulate of guarded payload covers `[0, payload_length)` exactly
  and every byte belongs to exactly one file;
- `content_digest` equals SHA-256 of the exact content bytes (no padding).

## 5. Canonical ordering

- Path table: sorted by name bytes (ascending). Unsorted table is rejected.
- File table: sorted by `content_offset` (ascending). Unsorted table is rejected.
- `file_flags` boot-canonical bit (bit 0) is set for exactly one file, the
  system boot text at `/system/boot/init.tos`.
- Boot-canonical **consistency**: the path entry carrying bit 0 must reference
  the file entry that carries bit 0, and vice versa. A canonical path pointing
  at a non-canonical file, a non-canonical path pointing at the canonical
  file, or a canonical flag on any other file is rejected.

## 6. Identity fields

- `source_identity_kind = 2` (detached source set): `source_oid_alg = 0`,
  `source_oid_length = 0`, and `source_identity_value` is exactly the
  ADR-0018 value:

  ```text
  SHA-256(
      b"TOS.DSI.v1\0" ||
      for each canonical path/file-table entry i:
          u32_le(path_length_i) || path_bytes_i || content_digest_i
  )
  ```

  Entries use the shared canonical path/file-table order. `path_bytes` are the
  exact validated canonical UTF-8 path bytes; `content_digest` is the exact
  validated 32-byte SHA-256 file digest. The fixed domain separator bytes are
  `54 4f 53 2e 44 53 49 2e 76 31 00`. `file_count` is not additionally encoded:
  the length-delimited path and fixed-size digest sequence is unambiguous.
  Capsule v1 still rejects zero files, although the mathematical zero-entry
  value is `SHA-256(b"TOS.DSI.v1\0")`. Builder and parser compute this value
  independently; a caller-selected detached value or a mismatch is rejected.
- `source_identity_kind = 1` (git commit): `source_oid_alg` names the OID
  algorithm (`1` = SHA-1, `2` = SHA-256) and `source_oid_length` its byte
  length (20 or 32). `source_identity_value` holds the **raw commit object
  id**, left-aligned and zero-padded to 32 bytes. The id is stored directly
  (not hashed) so a capsule can be resolved back to its commit with
  `git show <oid>`; see ADR-0016. A SHA-1 identity therefore has a 20-byte raw
  id followed by a 12-byte all-zero unused tail; any non-zero tail byte is
  rejected.
- The pair `(source_oid_alg, source_oid_length)` must be consistent with the
  kind: git kind requires `(1, 20)` or `(2, 32)`; detached kind requires
  `(0, 0)`. Anything else is rejected by the parser.
- Kind `0` is forbidden for any capsule produced by an official builder; it is
  rejected by the parser for boot-canonical capsules. Development fixtures may
  use kind 2 with an explicit `detached-source-set` label in the manifest.

## 7. Licence notices

`licence_notice_offset/length` point at a UTF-8 text block naming the SPDX
identifiers of all materials inside the capsule. If the capsule carries no
non-canonical material, the block names `GPL-3.0-or-later` (the canonical boot
text licence).

There is **no** `licence_notice_digest` field in the v1 header: the header
layout in §3 has none, and the notice block is covered by
`whole_capsule_digest` (§8) like every other byte of the capsule. A dedicated
digest field, if it is ever needed, belongs to a future format version and
requires an ADR. (An earlier revision of this section referred to such a field
and then denied its existence in the same sentence; the layout in §3 has always
been the authority.)

**Builder obligation vs parser obligation.** Producing a notice block that
actually names every SPDX identifier in the capsule is a builder obligation; a
v1 parser cannot verify it, because SPDX identifiers are not derivable from the
capsule bytes. A v1 parser validates only what is checkable: the block is in
bounds, is the exact tail of the capsule, has consistent offset/length fields
and is valid UTF-8 (§9 rule 21).

## 8. Whole-capsule digest

`whole_capsule_digest = SHA-256(capsule_bytes with bytes [152, 184) zeroed)`.
The digest is verified over the exact bytes passed to the parser.

## 9. Validation summary (reject conditions)

### 9.1 Resource-limit precedence

The parser returns stable structured errors in this deterministic order for
the five limits: `CapsuleTooLarge`, `FileCountTooLarge`, `PathTooLong`,
`NameArenaTooLarge` and `LicenceNoticeTooLarge`.

1. An input shorter than `HEADER_SIZE` is rejected before header decoding.
2. A physical input longer than `MAX_CAPSULE_BYTES` is rejected before header
   decoding, hashing or table traversal (`CapsuleTooLarge`).
3. After magic, UUID, format version, header size and alignment are checked,
   the declared total length, path/file counts and licence-notice length are
   checked in that order.
4. After checked table geometry establishes the name-arena bounds, its length
   is checked before path-table iteration (`NameArenaTooLarge`).
5. Each `name_length` is checked against `MAX_PATH_BYTES` before UTF-8 or
   canonical-path processing (`PathTooLong`).

### 9.2 Other reject conditions

1. magic mismatch;
2. format UUID mismatch;
3. format version unsupported;
4. header size, alignment or entry-size fields inconsistent;
5. `total_length` mismatch with actual input length;
6. any integer overflow in offset/length arithmetic (checked);
7. table offsets/counts imply regions outside `[0, total_length)`;
8. name arena does not end exactly at `file_table_offset` (§4);
9. payload region exceeds capsule bounds;
10. invalid UTF-8, NUL or control bytes in any path name;
11. path not canonical (see §4.1);
12. duplicate path names;
13. path table not sorted ascending;
14. `file_index` out of range, or the path table not being a bijection
    (duplicate references to one file, or an orphan file). Under §4.1 this is
    decided canonically: `path_table_count != file_count`, or any
    `path_entry[i].file_index != i`;
15. file table not sorted by content offset;
16. file content out of payload bounds;
17. overlapping or non-covering payload content;
18. per-file digest mismatch;
19. whole-capsule digest mismatch;
20. source identity kind unsupported, or kind 0 with boot-canonical flag;
21. licence notice block out of bounds, not the exact capsule tail, absent
    fields inconsistent (offset non-zero with zero length), or not valid UTF-8;
22. reserved fields non-zero (header, path-entry, file-entry 12-byte block);
23. boot-canonical flag inconsistency between path entry and file entry;
24. `path_table_offset != HEADER_SIZE`, i.e. a gap between the header and the
    path table (§4, ADR-0017);
25. the name arena is not packed: `path_entry[0].name_offset != 0`, a name that
    does not start where the previous one ends, or a last name that does not end
    exactly at `file_table_offset` (§4.1, ADR-0017);
26. `path_entry[i].file_index != i` — a non-canonical index mapping (§4.1,
    ADR-0017).
27. detached source identity differs from the ADR-0018 canonical
    path/digest encoding in §6.

A parser must return a structured error naming the rule violated; it must never
panic on malformed input.

## 10. Golden vectors

`tests/vectors/capsule-v1/` contains committed binary fixtures, regenerated by
`tests/vectors/gen/gen.sh`:

- `valid-001.bin` — a valid capsule built by the reference builder from the real
  `system/boot/init.tos` plus `system/version`, with the real `NOTICES.txt` as
  the licence tail;
- `invalid-badmagic.bin`, `invalid-truncated.bin`, `invalid-kind-none.bin`,
  `invalid-missing-boot.bin`, `invalid-bootcanon-mismatch.bin`,
  `invalid-licence-tail.bin`, `invalid-traversal.bin`, `invalid-dup.bin`,
  `invalid-dup-file-index.bin`, `invalid-unreferenced-file.bin`,
  `invalid-path-flag.bin`, `invalid-file-reserved.bin`,
  `invalid-sha1-oid-padding.bin` — each targeting one rule from §9.

A fixture targets one rule, which is the rule its expected error names. Fixtures
produced by patching a valid capsule in place also break the whole-capsule
digest (§8); the parser reports the targeted rule because it is checked before
the digest. A fixture must therefore be read as "rejected, and rejected for this
reason", not as "violates exactly one rule".

Every fixture records its expected parse outcome (accept, or reject with the
error name) in `vectors.tsv`, which is the input of the vector-driven
integration test. ADR-0019 requires every tracked binary fixture to have a
machine-verifiable `provenance.json` record; its
`mixed-material-generated` container status is not an SPDX expression.
