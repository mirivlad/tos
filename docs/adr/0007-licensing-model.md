<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0007: Multi-license model aligned with owner freedom

- Status: Accepted
- Date: 2026-08-05
- Classification: Identity-affecting

## Context

TOS aims to be open in the running machine, not only in an upstream repository. A permissive licence for the whole system would allow a distributor to close modifications and lock the owner out. A custom licence would reduce compatibility and may cease to be accepted open source.

## Decision

- TOS operating-system implementation: `GPL-3.0-or-later`.
- SDK, ABI and reusable interface material explicitly marked: `Apache-2.0`.
- Documentation: `CC-BY-SA-4.0`.
- Documentation code samples: `GPL-3.0-or-later OR Apache-2.0`.
- AGPL may be selected for a future network service only through a component ADR.
- Contributions use DCO 1.1 without mandatory copyright assignment.

## Consequences

Official appliance distributors must evaluate GPLv3 source and Installation Information duties. External applications can use Apache SDK material. Linux GPL-2.0-only source cannot be copied casually into GPLv3 TOS components.
