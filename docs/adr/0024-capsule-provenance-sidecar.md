<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0024: Capsule provenance sidecar manifest v1

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — adds a versioned artifact-provenance contract;
  capsule v1 bytes, Boot ABI v1 and the runtime trust boundary do not change
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The existing host builder emits a convenient JSON summary, but it is not a
versioned provenance manifest.  It lacks the full artifact format/ABI/target
description, material licence relationship, declared build options and a
reproducibility grade required by `docs/28_RELEASE_PROVENANCE_AND_REPRODUCIBILITY.md`.
Consequently it cannot by itself prove the complete Stage 1 source-to-capsule
relationship required by I-18 and the Stage 1 identity gate.

## Decision

The host builder emits the deterministic `tos-capsule-provenance-v1` JSON
sidecar defined by the accepted `source/interfaces/boot/CAPSULE_PROVENANCE_V1.md`
whenever `--meta` is requested.  The sidecar names the capsule SHA-256, capsule
format/version, architecture specification, builder, x86_64 loader/nucleus ABI
range, source commit or detached publication identity, every canonical source
material, build options, R0 reproducibility grade and retained licence-notice
set.

The whole-capsule digest remains the artifact anchor.  The sidecar is release
evidence and is not loaded, parsed or trusted by the UEFI loader or nucleus.
A host checker independently compares it with the capsule bytes, the committed
Git blobs in Git mode and the embedded notice tail.  Its deterministic JSON has
no timestamp, host path or environment-specific field.

`R0` means described provenance only; this decision makes no R1/R2/R3 claim.
The ordinary Stage 1 build uses Git-commit identity and a licence notice.  A
detached capsule names its ADR-0018 detached-source-set digest instead of
inventing a source commit.

## Architecture impact statement

- **Invariants/canonical representation:** I-01, I-10 and I-18 are strengthened.
  Canonical source remains repository text; the capsule remains a disposable
  transport/recovery seed, not installed system state.
- **Trusted base/dependencies:** only host builder/checker code changes.  The
  no_std loader, nucleus and parser gain no dependency or new input.
- **Source-to-runtime/recovery:** a Git capsule records its exact source commit
  and material blobs; detached mode records the accepted ADR-0018 publication
  identity.  Recovery/rollback selection is unchanged.
- **Threat/performance:** malformed or mismatched release evidence is rejected
  by the host gate.  No boot-time parser, hash traversal, firmware boundary or
  performance path is added.
- **Compatibility/licence/patent:** v1 capsule and BootInfo bytes remain
  compatible.  The notice set and each source SPDX expression are recorded,
  not inferred as a blanket artefact licence.  No imported code or patent claim
  is introduced.
- **Evidence:** deterministic builder sidecar regression, independent manifest
  checker/tamper rejection, Git-blob/material verification, QEMU success-path
  provenance check and full preflight.
