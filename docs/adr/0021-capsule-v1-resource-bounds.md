<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0021: Capsule v1 resource bounds

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — adds bounded validation to the existing capsule
  v1 contract without changing its byte layout, source-identity semantics,
  trusted-base role or Tier 0 invariants
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

Capsule v1 parsing is total and uses checked layout arithmetic, but it had no
accepted resource maxima. A hostile capsule could therefore drive excessive
UEFI allocation, table traversal, path processing or hashing before its
structural invalidity was known. The Stage 1 performance contract also needs a
bounded accepted workload rather than only an informal fixture size.

The capsule remains a disposable, source-bearing transport and recovery seed;
it does not become canonical installed state. The parser remains `no_std`,
allocation-free and borrowed-slice based.

## Decision

Capsule v1 has these inclusive resource limits. `KiB = 1024` bytes and
`MiB = 1024 * 1024` bytes.

| Constant | Inclusive maximum |
|---|---:|
| `MAX_CAPSULE_BYTES` | 32 MiB |
| `MAX_FILE_COUNT` | 4096 |
| `MAX_PATH_BYTES` | 1024 |
| `MAX_NAME_ARENA_BYTES` | 1 MiB |
| `MAX_LICENCE_NOTICE_BYTES` | 64 KiB |

The limits apply jointly: satisfying one does not weaken another.

The UEFI loader MUST obtain the capsule file length from EFI metadata and
reject a value greater than `MAX_CAPSULE_BYTES` before allocating a pool buffer
or reading the complete capsule. The parser MUST apply gross input, declared
length, count and notice limits before identity validation, table walking or
payload hashing where the layout permits. It MUST check an individual path
length before UTF-8 or canonical-path processing. The builder MUST apply the
same limits with checked conversions and MUST NOT truncate fields silently.

The parser returns these stable structured errors for the corresponding limits:
`CapsuleTooLarge`, `FileCountTooLarge`, `PathTooLong`,
`NameArenaTooLarge` and `LicenceNoticeTooLarge`.

Validation precedence is deterministic:

1. too-short input is rejected before decoding;
2. physical input length greater than `MAX_CAPSULE_BYTES` is rejected before
   header decoding or any traversal;
3. after fixed header magic/UUID/version/size/alignment validation, declared
   total length, then table counts, then licence-notice length are checked in
   that order;
4. after checked table geometry establishes the name-arena bounds, the arena
   limit is checked before path-table iteration;
5. each path length is checked before decoding or canonical-path semantics.

An accepted capsule permits at most two linear hash traversals of its bytes:
one whole-capsule SHA-256 traversal and one cumulative traversal for per-file
content SHA-256 values. Detached source identity consumes already validated
paths and content digests; it MUST NOT hash file contents again.

## Consequences

- A currently accepted 1,000-file / 16 MiB performance workload remains below
  all maxima and remains required performance evidence.
- Resource-boundary QEMU inputs are generated deterministically below
  `target/`; no large tracked binary fixture is needed.
- Existing capsule bytes at or below all limits stay format-compatible. Bytes
  outside an accepted maximum are now rejected fail-closed.

## Architecture impact statement

- **Invariants and canonical representation:** I-01, I-02, I-09, I-10 and
  I-18 are preserved. Capsule v1 remains a bounded derived transport artifact;
  no canonical source or byte layout changes.
- **Trusted base and source-to-runtime:** loader and parser gain only
  fail-closed checks over already trusted-base inputs. No dependency, privilege
  or source identity enters the trusted base.
- **Recovery and rollback:** no selection, rollback or owner boot mechanism
  changes. An oversized recovery seed now fails explicitly rather than being
  materialized without a Stage 1 bound.
- **Threat and performance:** this addresses hostile size/count/path inputs
  before expensive processing and bounds accepted hashing to two linear
  traversals. It preserves the Stage 1 1,000-file / 16 MiB workload.
- **Compatibility, licence and patent:** the capsule format version and byte
  representation do not change; previously oversized development artifacts
  are not accepted v1 artifacts. No licence boundary, imported dependency or
  patent claim changes.
- **Tests:** parser and builder prove every maximum and maximum-plus-one
  boundary, deterministic error precedence and parity; deterministic fuzzing
  remains required; QEMU proves the loader rejects 32 MiB + 1 before handoff.
