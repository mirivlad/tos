<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Contributing to TOS

TOS welcomes contributions that preserve its architectural identity. The project deliberately accepts narrow progress more readily than broad shortcuts.

## Before contributing

Read, in order:

1. `README.md`;
2. `docs/02_SYSTEM_INVARIANTS.md`;
3. `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`;
4. `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`;
5. `docs/23_CONTRIBUTION_PROVENANCE.md`;
6. the subsystem specification and accepted ADRs.

Automated contributors must also follow `AGENTS.md`.

## Contribution classes

### Class A — implementation-preserving

Bug fixes, tests, diagnostics, documentation corrections and implementations that clearly follow accepted contracts. These may be reviewed normally.

### Class B — contract-extending

New public API fields, wire-format versions, capability types, on-disk structures, language semantics or new top-level components. These require a design note and normally an ADR.

### Class C — identity-affecting

Any proposal that changes canonical-source semantics, nucleus boundaries, owner boot control, Git-native identity, licensing boundaries, driver placement, architecture governance or an invariant. These require an ADR and explicit approval by the Project Architect. A pull request may not smuggle a Class C decision inside implementation code.

## Required commit sign-off

All commits must include:

```text
Signed-off-by: Real Name <email@example.com>
```

The sign-off certifies the Developer Certificate of Origin 1.1 in `DCO`. It is not a transfer of copyright.

## AI-assisted contributions

AI tools may be used, but the human submitter remains responsible for:

- the origin and license compatibility of the contribution;
- reviewing every material change;
- ensuring no third-party code was reproduced without permission;
- disclosing any known generated-code provenance concern;
- running the required tests;
- signing the DCO personally.

An AI system cannot provide a DCO sign-off and cannot be listed as the legal author.

## Third-party code

Do not paste code merely because it is publicly visible. Record source, exact license, version or commit, modifications and compatibility in `THIRD_PARTY.toml` or the future equivalent inventory.

In particular, the Linux kernel is generally GPL-2.0-only. GPL-2.0-only code cannot simply be copied into a GPL-3.0-or-later TOS component. Linux drivers are valuable sources of hardware knowledge, register behavior and references to specifications, but direct copying requires file-level license review and may be prohibited. Prefer public hardware specifications, permissively licensed code, GPL-2.0-or-later code, or a documented clean-room reimplementation.

## Completion standard

A contribution is complete only when:

- implementation and tests agree with the specification;
- changed contracts are documented in the same change;
- formats have versions and stable test vectors;
- failure behavior is tested;
- required license and provenance metadata is present;
- the architecture conformance checklist passes;
- no known limitation is hidden.
