<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0016: Capsule git identity carries the raw object id

- Status: Accepted
- Date: 2026-08-06

## Context

ADR-0010 requires every derived artifact to name its source commit. The
first capsule v1 draft stored `source_identity_digest = SHA-256(raw oid)`
for `SRC_KIND_GIT`. That binding is one-way: given only the digest, the
original git object id cannot be recovered, so the capsule cannot be
resolved back to a commit without external state. A capsule is meant to be
self-describing; a non-invertible commit reference defeats that.

## Decision

For `source_identity_kind = SRC_KIND_GIT (1)`, the 40-byte identity region
of the capsule header is:

| offset | size | field |
| ------ | ---- | ----- |
| 96     | 1    | `source_identity_kind` = 1 (git) |
| 97     | 1    | `source_oid_alg` = 1 (SHA-1) or 2 (SHA-256) |
| 98     | 1    | `source_oid_length` = 20 or 32 |
| 99     | 1    | reserved (zero) |
| 100    | 32   | `source_identity_value`: raw git object id (20 or 32 bytes, left-aligned, zero-padded) |
| 132    | 4    | reserved (zero) |

The raw object id is stored, not a digest of it. `source_oid_alg` and
`source_oid_length` make the value self-describing for both SHA-1 (20-byte)
and SHA-256 (32-byte) repositories. BootInfo mirrors the same triple
(`capsule_identity_kind` at 136, `capsule_oid_alg` at 137,
`capsule_oid_length` at 138, 5 reserved bytes, `capsule_source_identity`
32 bytes at 144).

For `source_identity_kind = SRC_KIND_DETACHED (2)` (a source set without a
repository), the same 40-byte region holds `alg = 0`, `length = 0`, and the
32-byte source-set digest in `source_identity_value`; no OID is present.

## Consequences

- A git-bound capsule can be resolved to its exact commit: read
  `source_oid_alg`/`source_oid_length`, then take the left-aligned bytes of
  `source_identity_value` as the object id and run `git show <oid>`.
- `capsule v1` is not yet declared final; this region is explicitly
  reserved for the raw OID before acceptance. The former
  `sha256(oid)` binding is not used anywhere in the released format.
- Parsers reject inconsistent triples (git kind without a valid
  alg/length pair, detached kind with a non-zero algorithm or length).
