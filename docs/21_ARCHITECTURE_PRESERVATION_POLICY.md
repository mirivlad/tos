<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture preservation policy

## Purpose

TOS is unusually vulnerable to “reasonable” engineering decisions that solve local problems while erasing the reason the project exists. This policy makes architectural drift visible and reviewable.

## Architectural identity

TOS is defined by the conjunction of these properties:

1. human-readable source is the canonical installed form of non-nucleus executable components;
2. the active system has a commit identity and a visible source-to-runtime chain;
3. derived executable artifacts are disposable and verifiably tied to source and runtime versions;
4. the owner can inspect, branch, modify, validate and boot the system;
5. drivers and services are isolated by explicit capabilities;
6. activation and recovery are transactional and history-aware;
7. new languages extend the system through a stable frontend contract rather than expanding the nucleus without bound.

A project that keeps only some of these properties may be interesting, but it is not automatically TOS.

## Narrow scope versus architectural shortcut

The following are acceptable scope constraints:

- QEMU only;
- one CPU active while SMP-compatible interfaces are specified;
- VirtIO block before physical storage controllers;
- a small TOS Core bootstrap profile before the full language;
- serial shell before a graphical environment;
- one Git object format and one hash family initially.

The following are not acceptable milestone shortcuts:

- canonical binary modules with source kept “for later”;
- a Linux or BSD kernel hidden under a textual shell and presented as TOS;
- drivers moved into the nucleus because IPC is unfinished;
- Git used only for the development repository while runtime state has no commit identity;
- a general-purpose embedded runtime adopted before its trust, capability and source-identity semantics are accepted;
- a recovery flow that requires an undocumented host workstation;
- locked boot that allows source inspection but denies owner modification.

## Change classes

### Level 0 — editorial

No semantic effect. Normal review.

### Level 1 — implementation

Implements an existing contract without changing observable semantics. Requires tests.

### Level 2 — contract extension

Adds versioned behavior while preserving invariants. Requires a design note and generally an ADR.

### Level 3 — architectural

Moves trust boundaries, changes persistent formats, introduces a runtime dependency, changes source identity or modifies owner control. Requires an ADR and Project Architect approval.

### Level 4 — identity amendment

Changes or removes an invariant. Requires a dedicated identity-impact analysis, explicit approval and a major architecture version. It may result in a successor project rather than TOS.

## Architecture impact statement

Every Level 2 or higher change must answer:

- Which invariants are affected?
- What becomes canonical after the change?
- What enters or leaves the trusted base?
- Can the active runtime still identify its exact source?
- Can all derived artifacts be discarded and regenerated?
- Can the owner still recover and boot a previous commit?
- Does the change create a hidden host dependency?
- Does it alter licensing or patent exposure?
- How is the behavior tested?

## Substitution rule

A dependency or existing technology is not accepted merely because it is mature. It is evaluated in three roles:

- **runtime dependency** — becomes part of TOS operation and trust;
- **build dependency** — creates artifacts but is absent at runtime;
- **reference oracle** — used to compare behavior or generate test vectors.

The least invasive role that satisfies the requirement is preferred. libgit2, a Lua VM, Wasm engines, filesystem libraries and Linux driver code must not migrate from “oracle” or “research reference” to the trusted runtime without an ADR.

## Architecture debt

TOS does not normalize intentional architecture debt. Temporary diagnostics may exist on experimental branches, but a stage closes only when the real contract is implemented. Unfinished breadth is acceptable; falsified completion is not.

## Stage identity enforcement

Every stage is reviewed against `docs/37_STAGE_IDENTITY_GATES.md`. A conventional feature demonstration does not close a stage without the required TOS-specific evidence. The identity report is a release artifact.

## Documentation authority

Architecture decisions are interpreted through `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`. Generated summaries cannot amend architecture. A documentation conflict blocks implementation at the affected boundary until resolved.

## Enforcement

Architecture conformance is enforced through:

- ADR review;
- invariant references in pull requests;
- automated repository checks;
- source-to-runtime conformance tests;
- dependency and licence inventory;
- engineering and TOS identity stage gates;
- threat-model and performance-contract review;
- generated-document drift checks;
- refusal to merge identity-erasing shortcuts.
