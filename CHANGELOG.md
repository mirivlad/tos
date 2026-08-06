<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Changelog

## 0.2.1 — 2026-08-06

External architecture-review corrections.

### Added

- normative documentation hierarchy and deterministic consolidated-spec generator;
- CI guard preventing manual or stale `TOS_DEVELOPMENT_SPECIFICATION.md`;
- full threat model with adversary classes, assets, trust boundaries and stage mapping;
- quantitative performance contracts for runtime, IPC, drivers and repository operations;
- explicit Git compatibility profiles G0 through G6;
- TOS identity gates for every development stage;
- Stage 1.5 language-foundation decision gate and evaluation matrix;
- ADR-0015 establishing that no parser/runtime foundation is implemented before comparative architecture review.

### Changed

- the consolidated specification is now explicitly generated and non-normative;
- Stage 1 requires actual repository identity and source-commit provenance;
- Stage 2 no longer begins until the language foundation is selected by ADR;
- Stage 4 requires measured driver-path budgets rather than qualitative performance claims;
- Stage 5 requires a declared Git compatibility profile instead of an undefined promise of “Git support”;
- all stages now require identity evidence proving that TOS has not become a conventional microkernel with scripts;
- agent and pull-request instructions now require threat, performance, compatibility and identity impact review.

### Removed

- no project files removed; manual authority of the consolidated specification is explicitly rejected.

## 0.2.0 — 2026-08-05

Architecture and legal-governance baseline revision.

### Added

- GPLv3-or-later / Apache-2.0 / CC BY-SA 4.0 licence matrix and full licence texts;
- DCO 1.1 contribution model without copyright assignment;
- architecture-preservation policy and architect-led governance;
- patent policy, preliminary patent landscape and defensive-publication protocol;
- provisional-name, trademark and conformance policy;
- third-party dependency and external-oracle policy;
- release provenance and reproducibility grades;
- architecture conformance and legal release gates;
- source-to-runtime, owner-installability and derived-provenance invariants;
- GitHub pull-request architecture checklist and third-party inventory scaffold;
- ADR-0007 through ADR-0014.

### Changed

- boot capsule now has explicit source-commit and builder provenance;
- TOS Core and external language/runtime selection now require architecture review;
- Git implementations default to host tools/oracles rather than nucleus dependencies;
- driver porting policy distinguishes hardware knowledge from copyrighted implementation;
- Linux GPL-2.0-only code is explicitly excluded from direct copying into GPLv3 TOS components;
- Stage 0 now includes governance, licensing, defensive publication and naming readiness;
- Codex Stage 1 task includes SPDX, DCO, provenance and architecture-conformance requirements.

### Supersedes

This package supersedes architecture documentation version 0.1.0. It does not claim implementation completion or legal freedom to operate.
