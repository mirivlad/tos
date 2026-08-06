<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS governance

TOS begins as an architect-led open project.

## Project Architect

The initial Project Architect is Vladimir Tomashevskiy. The role owns the architectural intent, accepts or rejects invariant-changing ADRs, defines coherent stage boundaries and may refuse changes that make TOS more conventional at the cost of its identity.

This role does not own contributor copyrights and cannot revoke open-source rights already granted.

## Maintainers

Subsystem maintainers may accept implementation-preserving changes inside accepted contracts. They may not redefine project invariants, licensing boundaries or the owner-control model without an accepted ADR.

## Decision hierarchy

1. applicable law and third-party licence obligations;
2. accepted system invariants;
3. accepted ADRs;
4. normative subsystem specifications;
5. implementation and tests;
6. informal discussion.

A lower level cannot silently override a higher level.

## Architecture amendments

An invariant amendment requires:

- a dedicated ADR labelled `Identity-affecting`;
- an explicit explanation of why TOS remains TOS after the change;
- rejected alternatives;
- migration and rollback consequences;
- approval by the Project Architect;
- a release-note entry.

## Succession

If the Project Architect formally steps down, a signed governance commit may appoint a successor or architecture council. Mere inactivity does not authorize rewriting the invariants on the official branch; forks remain free to choose different rules under the software licences.

See `docs/29_PROJECT_GOVERNANCE.md`.
