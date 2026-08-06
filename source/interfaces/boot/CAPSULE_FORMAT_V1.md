<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Boot Capsule Format — Version 1

Status: **proposed specification amendment for Stage 1** (accepted by code review; a
format ADR may supersede it). Normative for all Stage 1 capsule implementations.

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
| `ALIGNMENT` | 8 | required alignment of offsets and entry sizes |
| `PATH_ENTRY_SIZE` | 16 | fixed path-table entry size |
| `FILE_ENTRY_SIZE` | 64 | fixed file-table entry size |
| `DIGEST_BYTES` | 32 | SHA-256 digest length |
| `ARCH_SPEC_VERSION` | `0x000201` | packed `0.2.1` (`major<<16|minor<<8|patch`) |
| `BUILDER_VERSION` | 1 | capsule builder contract version |

Byte order: all multi-byte integers are **little-endian**.

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
| 97 | 7 | `reserved` | must be zero |
| 104 | 32 | `source_identity_digest` | SHA-256 binding (see §6) |
| 136 | 8 | `licence_notice_offset` | absolute offset of licence-notice text; 0 if absent |
| 144 | 8 | `licence_notice_length` | length of licence-notice text; 0 if absent |
| 152 | 32 | `whole_capsule_digest` | SHA-256 over capsule with this field zeroed |
| 184 | — | — | end of header |

## 4. Tables

The capsule layout is strictly sequential:

```text
[header] [path table] [name arena] [file table] [payload] [licence notice]
```

The name arena begins immediately after the path table and ends exactly at
`file_table_offset`. The file table ends exactly at `payload_offset`. The
licence notice, when present, is the exact tail of the capsule. Hence:

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
- distinct (no duplicate names);
- the path table is a **bijection onto the file table**: each name is
  referenced exactly once, and every `file_index` in `[0, file_count)` is
  referenced by exactly one path entry (no duplicate references, no orphan
  files).

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

- `source_identity_kind = 2` (detached source set): `source_identity_digest` is
  SHA-256 of the concatenation of per-file `content_digest` values in file-table
  order. This binds the capsule to the exact byte content of its source material
  without a repository.
- `source_identity_kind = 1` (git commit): `source_identity_digest` is SHA-256 of
  the raw commit object id (binary, 20 or 32 bytes) of the source commit.
- Kind `0` is forbidden for any capsule produced by an official builder; it is
  rejected by the parser for boot-canonical capsules. Development fixtures may
  use kind 2 with an explicit `detached-source-set` label in the manifest.

## 7. Licence notices

`licence_notice_offset/length` point at a UTF-8 text block listing the SPDX
identifiers of all materials inside the capsule, one per line, sorted. If the
capsule carries no non-canonical material, the block contains the single line
`GPL-3.0-or-later` (the canonical boot text licence). The block must be within
bounds and, when present, its SHA-256 is recorded in the header field
`licence_notice_digest` — this document's first version stores the digest of the
notice block in `whole_capsule_digest` coverage only; the dedicated
`licence_notice_digest` field is reserved in the header reserved region until
capsule v2. Parsers reject an out-of-bounds notice block.

## 8. Whole-capsule digest

`whole_capsule_digest = SHA-256(capsule_bytes with bytes [152, 184) zeroed)`.
The digest is verified over the exact bytes passed to the parser.

## 9. Validation summary (reject conditions)

1. magic mismatch;
2. format UUID mismatch;
3. format version unsupported;
4. header size, alignment or entry-size fields inconsistent;
5. `total_length` mismatch with actual input length;
6. any integer overflow in offset/length arithmetic (checked);
7. table offsets/counts imply regions outside `[0, total_length)`;
8. name arena does not end exactly at `payload_offset`;
9. payload region exceeds capsule bounds;
10. invalid UTF-8, NUL or control bytes in any path name;
11. path not canonical (see §4.1);
12. duplicate path names;
13. path table not sorted ascending;
14. `file_index` out of range, or the path table not being a bijection
    (duplicate references to one file, or an orphan file);
15. file table not sorted by content offset;
16. file content out of payload bounds or misaligned;
17. overlapping or non-covering payload content;
18. per-file digest mismatch;
19. whole-capsule digest mismatch;
20. source identity kind unsupported, or kind 0 with boot-canonical flag;
21. licence notice block out of bounds, not the exact capsule tail, absent
    fields inconsistent (offset non-zero with zero length), or not valid UTF-8;
22. reserved fields non-zero (header, path-entry, file-entry 12-byte block);
23. boot-canonical flag inconsistency between path entry and file entry.

A parser must return a structured error naming the rule violated; it must never
panic on malformed input.

## 10. Golden vectors

`tests/vectors/capsule-v1/` contains binary fixtures:

- `valid-cap.bin` — a valid capsule (built by the reference builder);
- `invalid-magic.bin`, `invalid-truncated.bin`, `invalid-traversal-path.bin`,
  `invalid-dup-path.bin`, `invalid-unsorted-path.bin`, `invalid-bad-digest.bin`,
  `invalid-overlap.bin`, `invalid-overflow-count.bin` — each violating exactly
  one rule from §9.

Every fixture records its expected parse outcome (accept/reject + rule id) in
`vectors.tsv`.
