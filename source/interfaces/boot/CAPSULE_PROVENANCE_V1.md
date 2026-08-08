<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Capsule Provenance Sidecar — Version 1

Status: **Accepted Tier 2 interface contract.**

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs,
including ADR-0010 and ADR-0024.

## 1. Role

`tos-capsule-provenance-v1` is deterministic release provenance for one
capsule.  It is produced by `tos-capsule-tool --meta` and is independently
validated by `scripts/check-capsule-provenance.py`.  It is a sidecar only: a
UEFI loader and nucleus MUST NOT consume it, and it does not alter capsule v1
or Boot ABI v1 bytes.

The capsule's `whole_capsule_digest`/`artifact.sha256` is the binding artifact
identity.  A sidecar with a digest mismatch is invalid provenance, never an
alternative capsule authority.

## 2. Canonical JSON document

The UTF-8 JSON document has these required members.  Producers MUST emit the
shown member order and arrays in ascending `capsule_path`/identifier byte order;
they MUST NOT emit timestamps, absolute host paths or environment-specific
fields.  Consumers validate field types, values and the relationships below.

```json
{
  "format": "tos-capsule-provenance-v1",
  "schema_version": 1,
  "artifact": {
    "sha256": "<64 lowercase hex>",
    "capsule_format": {
      "uuid": "2c4f78b3-9d1e-4b0a-9f2c-1a5c8e0d6f71",
      "version": 1
    },
    "architecture_spec_version": "0.2.1",
    "builder": { "implementation": "tos-capsule-tool", "version": 1 },
    "target": {
      "architecture": "x86_64",
      "loader_abi": "x86_64-unknown-uefi",
      "nucleus_boot_abi": {
        "minimum": { "major": 1, "minor": 0 },
        "maximum": { "major": 1, "minor": 0 }
      }
    }
  },
  "source_identity": {
    "kind": "git-commit",
    "source_commit": "<full Git OID>",
    "oid_algorithm": "sha1|sha256",
    "oid_length": 20,
    "raw_oid": "<lowercase hex OID>"
  },
  "materials": [
    {
      "role": "canonical-source",
      "capsule_path": "/system/boot/init.tos",
      "repository_path": "source/system/boot/init.tos",
      "content_sha256": "<64 lowercase hex>",
      "spdx_expression": "GPL-3.0-or-later"
    }
  ],
  "build": {
    "identity_mode": "git-commit",
    "licence_notice_included": true,
    "reproducibility_grade": "R0"
  },
  "licence_notice": {
    "sha256": "<64 lowercase hex>",
    "spdx_identifiers": ["GPL-3.0-or-later"]
  }
}
```

The formatting example is descriptive; stable field names, values and ordering
rules are normative.  Lower-case hexadecimal SHA-256 values are exactly 64
characters.  A Git OID is the full lower-case OID naming a local commit and is
the same identity represented in the capsule header.  `source_commit` is
explicit for a Git identity.

For `kind = "detached-source-set"`, `source_commit`, `oid_algorithm`,
`oid_length` and `raw_oid` are replaced by
`"digest_algorithm":"sha256"` and `"digest":"<64 lowercase hex>"`.
That digest is the accepted ADR-0018 identity; it is a publication identity,
not a fabricated Git commit.

## 3. Required relationships

- `artifact.sha256` equals SHA-256 of the exact capsule bytes.
- `capsule_format`, `architecture_spec_version` and `builder.version` equal
  the verified capsule header.  The target is exactly the Stage 1 x86_64 UEFI
  loader target and Boot ABI v1.0 range shown above.
- `materials` has one row for every capsule file, in canonical file-table
  order.  Its path and digest equal the parsed capsule path/content digest.
  A Git-mode `repository_path` names a blob in `source_commit` with the same
  bytes; detached mode omits that member.
- Each material is `canonical-source` and declares the exact SPDX expression
  found in its source bytes.  Its expression occurs in
  `licence_notice.spdx_identifiers`.
- `licence_notice.sha256` equals the embedded licence-notice tail.  The sorted,
  duplicate-free identifier list is extracted from its exact
  `SPDX-License-Identifier:` lines.  A Stage 1 provenance sidecar therefore
  requires a retained notice block.
- `build.identity_mode` and `licence_notice_included` equal the represented
  capsule/header facts.  `reproducibility_grade` is exactly `R0`; no higher
  grade is implied by deterministic local output.

## 4. Evidence and evolution

The checker MUST reject a missing required member, non-canonical ordering,
digest/header/source/notice mismatch, invented Git commit, or licence-set
mismatch.  QEMU's normal build path MUST run this checker before booting the
capsule.  A schema extension requires a new version or an accepted ADR; a
consumer MUST NOT silently reinterpret v1 fields.
