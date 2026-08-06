<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Compliance and release gates

No official release is produced solely because functional tests pass.

## Architecture and identity gate

- active invariants and ADRs listed;
- no canonical executable binary introduced for a textual component;
- source-to-runtime identity available;
- owner-modifiable boot path preserved;
- trusted-base changes reviewed;
- applicable `docs/37_STAGE_IDENTITY_GATES.md` report complete;
- commit/source identity is real, not placeholder metadata.

## Engineering gate

- unit, integration, QEMU, fuzz and conformance suites pass;
- failure and rollback paths tested;
- persistent/wire formats versioned;
- known limitations documented;
- no stage closure around mocks or known replacement architecture.

## Threat and security gate

- `docs/34_THREAT_MODEL.md` covers every new parser, boundary, privileged operation, DMA path and remote;
- required negative/fault tests pass;
- security claims carry evidence levels;
- recovery cannot be overwritten by candidate activation;
- owner experimental mode remains available and visible;
- no secret embedded in repository/image;
- vulnerability-reporting path exists before network release.

## Performance gate

- assigned metrics in `docs/35_PERFORMANCE_CONTRACTS.md` measured;
- environment and baseline recorded;
- hard topology/count budgets satisfied or amended by ADR;
- threshold failures are not hidden by moving work into nucleus;
- regression policy applied.

## Compatibility gate

- Git compatibility profile declared and passed;
- language compatibility described as exact profile/subset;
- hardware support claim tied to tested profile;
- no broad ecosystem claim from superficial syntax or loose-object parsing.

## Licence gate

- SPDX identifiers present;
- licence texts included;
- dependencies compatible;
- third-party notices complete;
- GPL source/installation obligations satisfied;
- no GPL-2.0-only code copied into GPLv3 components;
- generated artifacts retain provenance/notices.

## Patent and naming gate

- high-risk mechanisms checked against landscape;
- original architecture queued for defensive publication;
- credible notices reviewed;
- release name/marks pass clearance state;
- no unsupported patent-clearance claim.

## Provenance gate

- source commit and tag authenticated;
- DCO checks pass;
- SBOM/dependency inventory produced;
- artifact manifests and hashes generated;
- reproducibility grade recorded;
- stage identity report archived;
- source archives published immutably.

## Documentation gate

- normative hierarchy has no known conflict;
- `python3 tools/build-specification.py --check` passes;
- source manifest contains every accepted ADR and required normative file;
- README map and ADR statuses current;
- release notes state architecture, security, performance, compatibility and reproducibility limits.
