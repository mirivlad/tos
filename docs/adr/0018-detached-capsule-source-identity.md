<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0018: Detached capsule source identity

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 3** — source-identity semantics and existing detached
  capsule bytes change
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

ADR-0016 defines the header shape for a detached source set but deliberately
does not define the derivation of its 32-byte value. The lower-tier interface
draft says the value is derived from file content digests, but a content-only
sequence does not bind canonical source paths. The current official detached
golden fixture and builder instead use caller-supplied synthetic bytes (`0x42`
repeated). This means a currently accepted detached capsule does not prove a
source-set identity.

`source/interfaces/boot/CAPSULE_FORMAT_V1.md` remains unassigned in the
authority hierarchy (F-08). It is evidence of the intended contract, not an
independent authority for this decision. This accepted ADR is the Tier 1
decision for detached-source-set identity.

## Proposed decision

### Canonical detached identity

For a capsule whose `source_identity_kind = SRC_KIND_DETACHED (2)`, let
`p_i` be the exact canonical UTF-8 path bytes from path-table entry `i`, and
let `d_i` be the exact 32-byte `content_digest` from file-table entry `i`, for
`i = 0 .. file_count - 1`. ADR-0017's canonical index mapping makes each
path-table entry `i` refer to file-table entry `i`; that shared canonical
path/file-table order is material.

The fixed versioned domain separator is the 11-byte sequence:

```text
DOMAIN = b"TOS.DSI.v1\0"
       = 54 4f 53 2e 44 53 49 2e 76 31 00
```

The identity is exactly:

```text
source_identity_value = SHA-256(
    DOMAIN ||
    for i = 0 .. file_count - 1:
        u32_le(len(p_i)) || p_i || d_i
)
```

The domain separator prevents an implicit claim of compatibility with another
SHA-256 construction over coincidentally similar bytes. Each path length has a
fixed four-byte little-endian encoding; each `d_i` is fixed at 32 bytes. These
length-delimited entries, consumed to the end of the encoded input, already
give a unique sequence boundary, so `file_count` is not included redundantly.
The parser already knows the count from the validated header and iterates that
many entries; adding it to the digest input would add no source-set binding.

The identity is not replaced by file-table ordering alone, raw file-content
concatenation, a Merkle tree, a different path encoding, or an
implementation-defined domain string. Capsule v1 already constrains the
path/file-table order and canonical paths; this formula binds both paths and
their validated contents to that representation.

For zero entries, the mathematical value of this domain-separated formula is
`SHA-256(DOMAIN)`. Capsule v1 nevertheless continues to reject `file_count =
0`, and the official builder continues to reject an empty file set. Thus no
valid v1 capsule emits or accepts a zero-entry detached identity; this
statement defines the function without relaxing the independent zero-file rule.

### Builder and parser obligations

After acceptance, the builder MUST compute the detached identity itself from
the specified canonical path/digest encoding. It MUST NOT accept a
caller-selected detached value, including an all-zero, synthetic, precomputed
or environment-injected substitute. This does not change ADR-0016 Git input: a
Git-bound capsule still accepts its explicit raw object identifier.

After it has validated every canonical path, file content digest and canonical
path/file-table layout, the parser MUST independently recompute the same
detached identity and reject a disagreement with a dedicated structured
`CapsError::DetachedIdentityMismatch`. The loader must serialize that error and
fail closed with `RESULT_CAPSULE_INVALID` before it can hand an invalid capsule
to the nucleus. The nucleus retains its existing BootInfo-to-header mirror
check; it is not a substitute for parser validation.

### Compatibility and migration

This is intentionally not byte-compatible for existing detached artifacts.
The current `valid-001.bin` contains the synthetic `0x42` value whereas the
proposal-only calculation for its two canonical paths and file digests is:

```text
b07b6e58e9e3aa9716d4ad779529a2e7be6522aef1f3e67a16230e04a55c8c05
```

Once accepted and implemented, detached capsules produced under the synthetic
convention are invalid and must be regenerated from their canonical inputs.
Git-bound capsules and their ADR-0016 raw-OID representation are unchanged.
Migration must regenerate all affected fixture bytes, SHA-256 records,
capsule metadata and Stage 1 evidence only after the separate F-22 vector
provenance/licensing decision supplies an authoritative record for every
generated binary. There is no silent compatibility fallback and no v1 version
bump in this proposal: acceptance explicitly chooses the Level 3 source
identity correction rather than treating invented fixture bytes as a compatible
identity.

## Architecture impact statement

- **Invariants:** I-10 deterministic identity and I-18 derived-artifact
  provenance are strengthened; no invariant is amended.
- **Canonical representation:** a detached header value is the stated SHA-256
  of the domain-separated, length-delimited canonical path plus 32-byte
  content-digest sequence, never an externally supplied label.
- **Trusted base:** the existing capsule builder/parser gain only a streaming
  SHA-256 calculation using the in-tree hash crate; the loader/nucleus gain no
  dependency or trust boundary.
- **Source-to-runtime:** the capsule identity now commits to exactly the
  canonical paths and file content digests that the parser validates, and the
  existing header-to-BootInfo-to-nucleus evidence remains the runtime mirror.
- **Derived artifacts:** a detached capsule remains disposable and can be
  regenerated from canonical source inputs, file-table order, builder version
  and the declared identity formula. The generated-artifact provenance record
  remains required by `docs/28` and F-22.
- **Recovery and rollback:** no Git, ESP, loader or recovery path changes.
  A previous Git-bound commit remains bootable as before; a detached recovery
  artifact must retain enough inputs and provenance to regenerate its identity.
- **Hidden host dependency:** none is introduced. SHA-256 is already in the
  TOS trusted code; no host Git command, network service or external runtime is
  consulted by the parser.
- **Threat model:** a malicious detached header can no longer claim arbitrary
  source-set provenance after valid file digests have been checked. Parsing
  stays total, bounded and fail-closed over hostile bytes.
- **Performance:** one additional four-byte length, path-byte and 32-byte
  digest update per file is required. The accepted implementation must measure
  the existing Stage 1 parser workload and show the `docs/35` p95 contract
  remains satisfied; no performance claim is made by this proposal.
- **Compatibility profile:** Stage 1/G0 scope remains unchanged. Only the
  untrusted synthetic detached-fixture convention is intentionally retired;
  no Git compatibility profile changes.
- **Dependencies:** none; `tos_hash::Sha256` is already an in-tree dependency.
- **Licence and patent:** no imported code or new licence class is proposed.
  Fixture/container provenance is deliberately deferred to F-22. This ADR
  makes no patent-freedom claim.
- **Tests after acceptance:** RED/GREEN builder-computation and parser-mismatch
  tests, including equal-content/different-path rejection; zero-file rejection
  remains; deterministic rebuild with regenerated vectors; host/integration/
  QEMU negative evidence; source-to-runtime identity report; performance
  measurement; and provenance verification for every regenerated fixture.

## Rejected alternatives

- Preserve arbitrary caller-provided detached values: rejected because the
  header then does not identify its source set.
- Hash file contents directly: rejected because the format already commits to
  per-file digests and this would define a different canonical representation.
- Treat canonical ordering as a substitute for path binding: rejected because
  two distinct path sets can preserve lexical order while carrying identical
  ordered contents. Exact canonical path bytes are therefore encoded above.
- Use a Merkle tree or add a version field: rejected because neither is needed
  to resolve this existing Stage 1 identity defect and either changes more of
  capsule v1 than the proposed formula.
- Accept an empty capsule with `SHA-256(DOMAIN)`: rejected because it weakens
  the existing zero-file validation rule.

## Implementation boundary

This accepted ADR authorizes the corresponding detached builder/parser work and
ephemeral test evidence. It does not authorize tracked binary-vector
regeneration or a generated-binary container classification before F-22 has an
accepted provenance/licensing decision. It also does not close Stage 1, start
Phase 2 or start Stage 1.5.
