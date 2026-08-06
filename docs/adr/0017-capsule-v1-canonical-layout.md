<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0017: Capsule v1 canonical layout — packed arena, canonical index mapping, byte compatibility preserved

- Status: Accepted (owner-approved)
- Date: 2026-08-06
- Change level: **Level 2** (contract extension under
  `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`) — adds reject conditions to an
  existing versioned format without amending an invariant. Explicitly **not**
  Level 3: no capsule byte changes.

## Context

An implementation audit of capsule v1 found three defects in the contract, none
of which the reference builder actually exercises:

1. **Alignment claim without an implementation.** §2 declared `ALIGNMENT = 8`
   the "required alignment of offsets and entry sizes", but a real capsule does
   not satisfy it: the name arena has an arbitrary byte length, so in
   `valid-001.bin` `file_table_offset` is 252. Either the parser had to start
   rejecting every capsule ever built, or the builder had to pad — and §4.2
   already states that `content_offset` need not be aligned, contradicting the
   §2 wording.

2. **Undescribed bytes were accepted.** The parser bounded each name inside the
   arena but never required the names to cover it, and required only
   `path_table_offset >= HEADER_SIZE`. A capsule carrying 64 arbitrary bytes in
   the arena (or between the header and the path table) parsed as valid. That
   contradicts §1 ("deterministic, immutable, read-only archive") and I-10
   (deterministic identity): for one file set, many distinct "valid" capsules
   existed, and the extra bytes travelled to the nucleus unvalidated.

3. **Quadratic bijection check on the boot path.** §4.1 requires the path table
   to be a bijection onto `[0, file_count)`. It was verified by counting
   references for every file — O(n²). Measured in a release build: 20 001 files
   (1.7 MB) took 3.55 s, against the `docs/35_PERFORMANCE_CONTRACTS.md` Stage 1
   budget of 250 ms p95 for 1 000 files / 16 MiB. A 16 MiB capsule admits
   ~200 000 entries.

## Decision

### 1. Alignment: clarify the contract, do not pad (byte compatibility preserved)

`ALIGNMENT = 8` is normative for the header and the fixed entry sizes only:

- `HEADER_SIZE` (184), `PATH_ENTRY_SIZE` (16) and `FILE_ENTRY_SIZE` (64) are
  multiples of 8;
- `path_table_offset == HEADER_SIZE`, and is therefore 8-aligned;
- `file_table_offset`, `payload_offset` and `content_offset` are **not** required
  to be multiples of 8;
- implementations must not assume aligned struct access anywhere in the capsule
  and must decode every field byte-wise (little-endian), as the reference parser
  already does.

The alternative — making the builder pad the name arena so every table starts on
an 8-byte boundary — is **rejected**. It is a Level 3 change to a persistent
format: it would alter the bytes of every capsule v1, invalidate every
`capsule_sha256`, every committed golden vector and every digest recorded in
archived Stage 1 evidence. The gain is unproven: the parser decodes fields
byte-wise, so aligned tables buy nothing today, and a future implementation that
wants aligned access can request it in a future format version.

### 2. The name arena is strictly packed

For a path table of `n` entries in table order:

- `path_entry[0].name_offset == 0`;
- `path_entry[i].name_offset == path_entry[i-1].name_offset + path_entry[i-1].name_length`;
- the end of the last name equals `file_table_offset`.

No undescribed byte may exist between the header and the path table, between
names, or between the last name and the file table.

### 3. Canonical index mapping replaces the reference count

The bijection required by §4.1 is realised canonically:

- `path_table_count == file_count`;
- `path_entry[i].file_index == i` for every `i`;
- the file table and the payload therefore follow the same order as the
  name-sorted path table.

The official builder already emits exactly this layout, so no existing correct
capsule needs its data moved; the rule only forbids non-canonical permutations
of the index field. The check becomes a single O(n) pass and stays
allocation-free and `no_std`.

A structured error distinguishes a non-canonical mapping, and a negative golden
vector with permuted/repeated `file_index` values pins the behaviour.

## Consequences

- Capsule v1 stays byte-compatible: every committed vector, every recorded
  `capsule_sha256` and the archived QEMU evidence remain valid. The format
  version is not incremented.
- Capsules that were only accepted because of undescribed bytes or a
  non-canonical index permutation are now rejected. No such capsule was ever
  produced by an official builder.
- The deterministic-archive property becomes real: for a given file set, licence
  notice and identity there is exactly one valid capsule v1 byte string.
- The O(n²) validation path disappears without introducing a maximum-size rule.
  A maximum-size bound remains an open question (CODEX_START asks for one) and
  is deliberately **not** decided here.
- Third-party builders must emit the canonical mapping; they could previously
  emit any permutation.

## Compliance

- Invariants: none amended. Strengthens I-10 (deterministic identity) and I-18
  (derived-artifact provenance).
- Trusted base: unchanged; no dependency added. The parser loses code rather
  than gaining it.
- Recovery/rollback: unaffected — no capsule is invalidated.
- Threat model: closes an undescribed-bytes channel into the nucleus
  (`docs/34_THREAT_MODEL.md`: all external bytes are hostile until validated).
- Performance: removes the quadratic term measured against
  `docs/35_PERFORMANCE_CONTRACTS.md` §Stage 1.
- Tests: parser unit tests per rule, negative golden vectors, and the existing
  tamper/truncation suites.
