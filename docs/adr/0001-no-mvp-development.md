<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0001: No MVP or throwaway foundation

- Status: Accepted
- Date: 2026-08-05

## Context

TOS combines several difficult ideas. A conventional approach would build a minimal demonstration using shortcuts and replace it later. In operating-system projects, those shortcuts frequently become permanent dependencies or consume the energy required for the real architecture.

## Decision

TOS will not be developed as an MVP. Work is organized as coherent architectural stages. Platform breadth may be intentionally narrow, but interfaces, formats, and trust boundaries implemented within a closed stage are intended to survive.

A project pause is acceptable. A knowingly disposable foundation is not.

## Consequences

- More design work precedes visible demonstrations.
- Early milestones require format specifications, tests, and recovery behavior.
- Agents may not bypass intended subsystems to claim progress.
- Experimental code remains on explicit branches and is not treated as completed architecture.
