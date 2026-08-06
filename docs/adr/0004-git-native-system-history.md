<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0004: Git-native system history

- Status: Accepted
- Date: 2026-08-05

## Context

A text-centric system naturally benefits from source version control. Treating Git only as an external developer tool would miss recovery, audit, branching, merging, and reproducibility benefits during normal operation.

## Decision

The durable `/system` source tree is identified by a repository commit. Boot control selects commits. Updates produce candidate commits. Rollback, merge, clone, push, and bisect are system operations.

The nucleus implements only boot-critical repository verification and traversal. Full Git behavior and remote protocols live in textual services.

## Consequences

- Mutable state must be separated from `/system`.
- Protected refs and retention rules become security mechanisms.
- Commit metadata carries system-specific compatibility and health data.
- Exact Git interoperability requires a dedicated format ADR and conformance suite.
