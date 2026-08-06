<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Release provenance and reproducibility

## Source release identity

Every release names:

- architecture specification version;
- source commit;
- active invariant set;
- accepted ADR set;
- compiler and toolchain identities;
- dependency lock state;
- generated schema/compiler versions;
- licence inventory;
- build command and environment description.

## Artifact provenance

A boot artifact, capsule or native cache is never anonymous. Its metadata links to:

- canonical source commit and source hashes;
- builder implementation and version;
- target architecture and ABI;
- build options;
- dependency/material digests;
- output digest;
- signature or attestation where available.

The artifact is still derived and disposable. Provenance does not elevate it above source.

## Boot capsule

The capsule manifest includes at minimum:

- format version;
- source commit or publication identity;
- nucleus ABI minimum and maximum;
- included canonical files with hashes;
- builder identity;
- whole-capsule digest;
- reproducibility status;
- licence notice set.

The capsule may be reconstructed from the commit plus documented tools. The recovery image may carry a copy, but the canonical capsule inputs remain in the repository.

## Reproducibility grades

- **R0 — described:** build steps and materials recorded;
- **R1 — repeatable:** same controlled environment reproduces output;
- **R2 — independently reproducible:** a second environment produces identical output;
- **R3 — multi-builder attested:** multiple independent builders publish matching results.

A release states its achieved grade rather than claiming reproducibility by aspiration.

## Documentation provenance

The consolidated specification records the ordered source manifest digest and is reproducibly generated. Official documentation releases include the generator, source manifest and drift-check result. The generated file is not a normative source.

Stage identity reports record their source commit, architecture version, compatibility profiles, threat evidence and performance artifacts.

## Archive retention

Official source archives, manifests, signatures, SBOMs and release notes are retained permanently. Derived convenience images may be mirrored or regenerated, but at least one verified recovery artifact for every supported architecture generation is retained.
