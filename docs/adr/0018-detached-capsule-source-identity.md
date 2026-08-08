<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0018: Detached capsule source identity

- Status: Proposed — not an authority for implementation
- Date: 2026-08-09
- Change level: **Level 3** — source-identity semantics and existing detached
  capsule bytes change
- Requires: Project Architect acceptance before any builder, parser, vector or
  evidence change

## Context

ADR-0016 defines the header shape for a detached source set but deliberately
does not define the derivation of its 32-byte value. The lower-tier interface
draft says the value is derived from file content digests, while the current
official detached golden fixture and builder instead use caller-supplied
synthetic bytes (`0x42` repeated). This means a currently accepted detached
capsule does not prove a source-set identity.

`source/interfaces/boot/CAPSULE_FORMAT_V1.md` remains unassigned in the
authority hierarchy (F-08). It is evidence of the intended contract, not an
independent authority for this decision. If accepted, this ADR becomes the
Tier 1 decision for detached-source-set identity.

## Proposed decision

### Canonical detached identity

For a capsule whose `source_identity_kind = SRC_KIND_DETACHED (2)`, let
`d_i` be the exact 32-byte `content_digest` from file-table entry `i`, for
`i = 0 .. file_count - 1`. The identity is exactly:

```text
source_identity_value = SHA-256(d_0 || d_1 || ... || d_(file_count - 1))
```

The file-table order is material. It is not replaced by path sorting,
concatenation of file contents, filename hashing, a Merkle tree, a length
prefix, or an alternative digest encoding. Capsule v1 already constrains that
order through its canonical file layout; the identity formula binds to that
canonical representation.

The mathematical empty input is `SHA-256(empty)`. Capsule v1 nevertheless
continues to reject `file_count = 0`, and the official builder continues to
reject an empty file set. Thus no valid v1 capsule emits or accepts an
empty-input detached identity; this statement defines the function without
relaxing the independent zero-file rule.

### Builder and parser obligations

After acceptance, the builder MUST compute the detached identity itself from
the canonical file-table digest sequence. It MUST NOT accept a caller-selected
detached value, including an all-zero, synthetic, precomputed or environment
injected substitute. This does not change ADR-0016 Git input: a Git-bound
capsule still accepts its explicit raw object identifier.

After it has validated every file's content digest and the canonical file-table
layout, the parser MUST independently recompute the same detached identity and
reject a disagreement with a dedicated structured
`CapsError::DetachedIdentityMismatch`. The loader must serialize that error and
fail closed with `RESULT_CAPSULE_INVALID` before it can hand an invalid capsule
to the nucleus. The nucleus retains its existing BootInfo-to-header mirror
check; it is not a substitute for parser validation.

### Compatibility and migration

This is intentionally not byte-compatible for existing detached artifacts.
The current `valid-001.bin` contains the synthetic `0x42` value whereas the
proposal-only calculation for its two canonical file digests is:

```text
56daf5dbc0865b626200a1284100b7c4642f686b6d23978dc1050dfe8bc0b7ce
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
  of the ordered 32-byte content-digest sequence, never an externally supplied
  label.
- **Trusted base:** the existing capsule builder/parser gain only a streaming
  SHA-256 calculation using the in-tree hash crate; the loader/nucleus gain no
  dependency or trust boundary.
- **Source-to-runtime:** the capsule identity now commits to exactly the
  canonical file content digests that the parser validates, and the existing
  header-to-BootInfo-to-nucleus evidence remains the runtime mirror.
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
- **Performance:** one additional 32-byte update per file is required. The
  accepted implementation must measure the existing Stage 1 parser workload
  and show the `docs/35` p95 contract remains satisfied; no performance claim
  is made by this proposal.
- **Compatibility profile:** Stage 1/G0 scope remains unchanged. Only the
  untrusted synthetic detached-fixture convention is intentionally retired;
  no Git compatibility profile changes.
- **Dependencies:** none; `tos_hash::Sha256` is already an in-tree dependency.
- **Licence and patent:** no imported code or new licence class is proposed.
  Fixture/container provenance is deliberately deferred to F-22. This ADR
  makes no patent-freedom claim.
- **Tests after acceptance:** RED/GREEN builder-computation and parser-mismatch
  tests; zero-file rejection remains; deterministic rebuild with regenerated
  vectors; host/integration/QEMU negative evidence; source-to-runtime identity
  report; performance measurement; and provenance verification for every
  regenerated fixture.

## Rejected alternatives

- Preserve arbitrary caller-provided detached values: rejected because the
  header then does not identify its source set.
- Hash file contents directly: rejected because the format already commits to
  per-file digests and this would define a different canonical representation.
- Sort or hash filenames again: rejected because file-table order is already
  canonical and material.
- Use a Merkle tree or add a version field: rejected because neither is needed
  to resolve this existing Stage 1 identity defect and either changes more of
  capsule v1 than the proposed formula.
- Accept an empty capsule with `SHA-256(empty)`: rejected because it weakens
  the existing zero-file validation rule.

## Acceptance boundary

Until this ADR is explicitly accepted by the Project Architect, it authorizes
no production code, parser semantics, builder API change, golden-vector
regeneration, generated-binary provenance declaration, Stage 1 closure claim,
Phase 2 work or Stage 1.5 work.
