<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS project charter

## Purpose

TOS explores a different relationship between an operating system, its source code, its installed state, and its history.

The project exists to build an operating system where:

- the installed system is inspectable source text;
- changing source text changes the system without a separate package-build-install cycle;
- executable caches are derived and disposable;
- the system is identified by a repository commit;
- rollback, branching, merging, cloning, and bisecting are ordinary system operations;
- device support can be delivered as textual driver modules;
- multiple programming languages can be added as textual frontend modules targeting one common execution model;
- the owner retains the right to inspect, modify, replace, and recover every non-firmware component.

## Success definition

TOS succeeds when a user can perform the following sequence on a supported machine:

1. boot a trusted nucleus and select a system commit;
2. run the system whose canonical components are text files from that commit;
3. inspect the exact source currently responsible for a service or driver;
4. modify the source in the running system;
5. validate and activate the new module transactionally;
6. commit the system change with tests and metadata;
7. push the history to a standard remote;
8. restore another machine from the nucleus plus that repository;
9. boot an earlier commit after a regression;
10. use automated bisect and health checks to locate the first bad system commit.

## Development stance

TOS does not pursue an MVP. The term obscures an important distinction:

- **Limited platform support** is acceptable.
- **Limited architectural integrity** is not.

A milestone may implement only one architecture and a small device set, but the interfaces created at that milestone must be intended to survive.

The project may be paused when time, energy, or resources run out. A paused coherent system is preferable to a quickly demonstrated pile of shortcuts that must later be discarded.

## Initial supported use

The first target is a research and enthusiast operating system running under QEMU. The initial goal is not desktop replacement, application compatibility, or mass adoption. The goal is to prove and mature the TOS model under controlled hardware while retaining a path to physical machines.

## Governance

Governance is architect-led during the foundational phases. The decision hierarchy is:

1. applicable law and third-party licence obligations;
2. system invariants;
3. accepted Architecture Decision Records;
4. normative subsystem specifications;
5. conformance tests and implementation;
6. informal design discussion.

The initial Project Architect is Vladimir Tomashevskiy. The role protects the project thesis, accepts identity-affecting ADRs and decides whether a stage has reached a coherent architectural boundary. Subsystem maintainers may decide implementation details inside accepted contracts, but they cannot waive invariants silently.

This authority applies to the official project only. The open-source licences preserve the right to fork. Copyright remains with contributors under the DCO model.

See `GOVERNANCE.md`, `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`, and `docs/29_PROJECT_GOVERNANCE.md`.

## Licensing

TOS uses a component-based established licence model accepted by ADR-0007:

- operating-system implementation: `GPL-3.0-or-later`;
- SDK, ABI, schemas and designated independent interface libraries: `Apache-2.0`;
- documentation: `CC-BY-SA-4.0`;
- documentation code samples: `GPL-3.0-or-later OR Apache-2.0`;
- a future official hosted network service may use `AGPL-3.0-or-later` only through its own ADR.

Contributors retain copyright and certify contributions through Developer Certificate of Origin 1.1 sign-off. No mandatory assignment of copyright is required.

The licensing strategy is part of the architecture: official TOS should not be distributable as a source-visible but owner-locked appliance. See `LICENSE.md` and `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`.

## Public-interest and patent stance

TOS is intended to remain a public technical commons. The project does not plan to seek patents on its core architecture by default. Original enabling architecture is prepared for defensive publication. Patent risk is tracked honestly, and a professional freedom-to-operate review is required before material commercial distribution.

The project does not claim that publication or independent invention prevents infringement of earlier valid claims.
