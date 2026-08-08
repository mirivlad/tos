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

Create commits with Git's sign-off option so the trailer is not forgotten:

```sh
git commit -s
```

Before pushing, run the local repository gates from the repository root:

```sh
./scripts/preflight.sh
```

Use `./scripts/preflight.sh --full` when the change touches boot, capsule parsing
or QEMU-visible behavior; it additionally runs fuzzing and both QEMU suites.
Preflight reports all selected gate results and does not install missing tools.

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

## Repository assets

Text assets carry an SPDX identifier in the first five lines, using the
existing `LICENSE.md` component class that applies to the material. Do not
select a new licence merely because a file extension is new.

Binary artwork cannot use a normal source comment. Its directory therefore
contains a tracked `README.md` that lists each binary path, its licence under
the existing matrix, its origin and the Git contribution that introduced it.
The SPDX gate checks the record path-by-path. Adding a blanket extension
exemption is not an acceptable substitute for provenance.

Imported or adapted assets additionally follow
`docs/23_CONTRIBUTION_PROVENANCE.md` and are recorded in `THIRD_PARTY.toml` when
applicable. If origin or licensing cannot be established, the asset is blocked.

## Completion standard

A contribution is complete only when:

- implementation and tests agree with the specification;
- changed contracts are documented in the same change;
- formats have versions and stable test vectors;
- failure behavior is tested;
- required license and provenance metadata is present;
- the architecture conformance checklist passes;
- no known limitation is hidden.
