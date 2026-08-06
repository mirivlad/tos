<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Project governance

## Governance phase A — architect-led

During foundational development, the Project Architect has final authority over invariants, architecture ADRs and stage closure. This is intended to protect a coherent uncommon design while agents and contributors naturally propose familiar substitutes.

Implementation review can be delegated. Architectural identity cannot be delegated implicitly.

## Roles

### Project Architect

- maintains the project thesis and invariant set;
- approves Level 3 and Level 4 architecture changes;
- decides whether a milestone is architecturally complete;
- appoints maintainers;
- approves defensive publications and official conformance use.

### Subsystem maintainer

- reviews code and tests within an accepted contract;
- maintains subsystem documentation;
- can reject quality, security or provenance failures;
- cannot waive project invariants.

### Release steward

- verifies legal, provenance, reproducibility and conformance gates;
- does not decide architecture alone.

### Contributor

- retains copyright;
- signs the DCO;
- follows licence, provenance and architecture policy.

## Decision process

Normal changes use public pull requests. Architectural proposals begin with an issue or design note and become ADRs before implementation commits depend on them.

The Project Architect may reject a technically sound proposal because it erodes TOS identity. The rejection should explain which invariant or project objective is affected.

## Disputes

Technical disputes are resolved by contract hierarchy and evidence. Licence or legal conflicts take priority over implementation preference. Personal conduct is handled separately from architecture review.

## Fork freedom

Governance controls the official project, not downstream rights. Anyone may fork under the applicable licences. A fork that rejects the invariants should use a distinct identity rather than pressuring the official project to become conventional.

## Future governance

After multiple independent maintainers demonstrate long-term understanding of TOS, governance may move to an architecture council. Such a change requires an ADR and an explicit mechanism preventing a simple majority from silently deleting core invariants.
