<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0014: Driver knowledge reuse without incompatible source copying

- Status: Accepted
- Date: 2026-08-05

## Context

TOS intends to learn from open drivers. The Linux kernel is generally GPL-2.0-only, while official TOS implementation is GPL-3.0-or-later. The licences are not compatible for combining copied implementation code in one work. In addition, an existing driver contains substantial operating-system integration that TOS should not inherit architecturally.

## Decision

TOS driver work distinguishes public hardware facts from expressive implementation. Preferred sources are public specifications, permissively licensed code, GPL-2.0-or-later files and independently written functional descriptions. GPL-2.0-only source may be studied for behavior and specification references, but code is not copied into GPLv3 TOS components without a specific legal basis.

Where necessary, use a documented clean-room functional reimplementation process.

## Consequences

Porting is slower than mechanical translation but avoids licence conflict and Linux-specific architecture leakage. Provenance records are mandatory for register tables, firmware and adapted source.
