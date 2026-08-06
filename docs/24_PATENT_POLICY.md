<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Patent policy

## Scope

This policy manages patent risk without pretending that a volunteer project can prove worldwide freedom to operate.

## Default strategy

TOS uses:

- established licences with patent provisions;
- defensive publication of original architecture;
- a maintained landscape and design-around register;
- targeted review at high-risk architecture points;
- qualified legal review before material commercial distribution.

The project does not add a custom patent clause to GPL or Apache licences.

## Contributor duties

Contributors are not required to search patents. They must disclose a patent they actually know they or their employer control when the contribution is intentionally designed to practise a required claim. They must not offer code under a private patent arrangement that denies equivalent downstream rights.

A disclosure does not imply validity or infringement. It permits the project to investigate, redesign or reject the contribution.

## Review procedure

For a high-risk design:

1. define the proposed mechanism precisely;
2. search by concepts, classifications, assignees and citations;
3. identify patent families and jurisdictions;
4. read independent claims rather than relying on titles or abstracts;
5. create a non-legal engineering claim matrix;
6. document design differences;
7. request counsel when distribution risk justifies it;
8. preserve the decision in an ADR or legal review record.

## Design-around principle

Avoiding one optional claim element can be safer than debating a broad description. TOS should prefer its native commit graph, typed capabilities and source identity rather than copying a vendor’s exact link-switching, patch-memento, interrupt-stack or appliance-update mechanism.

## Freedom-to-operate gates

A professional FTO review is required before the project or an official distributor:

- sells a hardware appliance;
- signs a commercial indemnity;
- deploys a substantial paid hosted fleet service;
- distributes in a jurisdiction after a credible patent notice;
- deliberately implements a mechanism close to an active independent claim.

Research releases and source publication still require reasonable care, but they do not justify claims of complete patent clearance.

## Records

Public architecture risk belongs in `docs/research/PATENT_LANDSCAPE.md`. Privileged legal advice must not be committed publicly without counsel’s approval.
