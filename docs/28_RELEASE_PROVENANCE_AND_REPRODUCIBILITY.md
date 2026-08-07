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

## Integrity of the release package versus the source tree

TOS keeps exactly one source of truth for each kind of integrity, so that two
mechanisms can never disagree about what the project contains.

- **Git object identity is canonical for the source tree.** A commit names a
  tree, the capsule records that commit, and the boot chain re-verifies the
  capsule digest. `source/` is therefore not covered by a flat digest list: a
  second list would be a weaker duplicate of Git and a competing source of
  truth.
- **`SHA256SUMS` verifies the release-package files outside Git.** Its purpose
  is to let a recipient who received the documentation and governance package
  without a repository check that the files are intact. It covers exactly that
  package.
- **`MANIFEST.txt` describes the release baseline and is generated from its
  actual composition.** Aggregate values — file counts, invariant counts, the
  ADR list — are derived, never hand-maintained.

Both files are produced by `python3 tools/build-release-manifest.py` and
verified in CI with `--check`. A hand-edited count is not normative data; it is
a defect waiting to be discovered, as the manifest's own "15 accepted ADRs"
was while seventeen existed.
