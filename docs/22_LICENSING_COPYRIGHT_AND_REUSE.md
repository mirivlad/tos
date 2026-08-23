<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Licensing, copyright and reuse

## Goals

The legal structure should preserve the freedoms that TOS provides architecturally while allowing independent applications, tools and compatible implementations.

## Component policy

### GPL-3.0-or-later

The following are part of the reciprocal operating system:

- bootloader and nucleus;
- TOS Core reference runtime and verifier;
- official recovery environment;
- official system services and drivers;
- repository activation, rollback and boot-control implementation;
- official shell, editor and source inspection services;
- generated target code when it forms a covered derived work of these components.

### Apache-2.0

The following may be used independently:

- ABI structures and bindings;
- IPC and file-format schemas;
- SDK libraries;
- conformance client libraries;
- test-vector parsers and independent inspection tools explicitly designated Apache;
- language-frontend SDK surfaces.

An implementation is not reclassified as an SDK simply to avoid copyleft.

### CC-BY-SA-4.0

This applies to prose specifications, diagrams, governance and documentation. Code snippets embedded in documentation are dual licensed under `GPL-3.0-or-later OR Apache-2.0` unless marked otherwise.

### AGPL-3.0-or-later

This is reserved for a future official network service whose value would otherwise be delivered as a modified hosted service without source reciprocity. AGPL adoption is component-specific and requires an ADR.

### MIT

MIT is reserved for external observer build or patch material that must be
combined with a GPL-2.0-only test instrument. ADR-0066 applies it to the QEMU
observer builder so the inserted UART observation code is compatible with both
the MIT-licensed source file and the GPLv2 QEMU executable. This exception does
not apply to production TOS implementation or general host tooling.

## GPL installation freedom and TOS

Official distributions must not satisfy source obligations while blocking the owner from installing modified covered software. Where GPLv3 Installation Information obligations apply, the distributor must provide the necessary methods, procedures or authorization material. Independently of the narrow legal trigger, owner-controlled boot remains a TOS conformance requirement for official builds.

## Copyright

Contributors retain copyright. The project records authorship through Git and DCO sign-offs. There is no default copyright assignment and no broad contributor licence agreement granting relicensing power to one party.

A future foundation may hold project assets or receive voluntary assignments, but existing contributors are not retroactively required to assign rights.

## Relicensing

Changing the licence of existing files requires permission from all relevant copyright holders unless the existing licence already permits the change. The Project Architect cannot unilaterally convert community-owned GPL code to a proprietary licence.

New major components may choose a compatible licence through ADR, but the central system licence matrix remains an architectural decision.

## Third-party compatibility

Every imported component is evaluated by exact file licence, not project reputation. Important examples:

- Apache-2.0 code can generally be incorporated into a GPLv3 combined work under GPLv3 terms;
- MIT/BSD code is usually compatible when notices are preserved;
- GPL-2.0-only code is not compatible with GPLv3 in one combined work;
- GPL-2.0-or-later code may be used under GPLv3;
- proprietary firmware or tools may be distributed separately only after legal and architectural review.

Because the Linux kernel as a whole is GPL-2.0-only, TOS must not assume that Linux driver source can be copied into GPLv3 code. Hardware facts are not copyright, but expressive implementation is. Prefer specifications, compatible files and documented clean-room reimplementation.

## Compliance artifacts

Each release must contain:

- full licence texts;
- source offer or source distribution as required;
- copyright and notice inventory;
- dependency and generated-code inventory;
- installation information when applicable;
- build and source provenance;
- machine-readable SPDX or equivalent SBOM when tooling exists.

## References

- GNU GPLv3: `https://www.gnu.org/licenses/gpl-3.0.html`
- GNU GPL FAQ: `https://www.gnu.org/licenses/gpl-faq.html`
- Apache License 2.0: `https://www.apache.org/licenses/LICENSE-2.0`
- Creative Commons BY-SA 4.0: `https://creativecommons.org/licenses/by-sa/4.0/`
- SPDX licence list: `https://spdx.org/licenses/`
