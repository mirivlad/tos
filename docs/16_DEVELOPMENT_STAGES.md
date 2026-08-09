<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Development stages

## No MVP interpretation

These stages are not a sequence from disposable prototype to real system. Each stage closes a coherent layer using intended long-term contracts. The system may be paused after any stage without invalidating prior work.

Every stage must pass both its engineering exit gate and the corresponding identity gate in `docs/37_STAGE_IDENTITY_GATES.md`.

## Stage 0 — Architecture, governance and legal baseline

Deliverables:

- charter, manifesto and invariants;
- accepted foundational ADRs;
- boot/capsule requirements;
- repository layout;
- toolchain and CI policy;
- normative threat model;
- documentation hierarchy and deterministic consolidated-spec generation;
- TOS Core language requirements;
- licensing, provenance, patent and naming policy.

Engineering exit: no implementation begins on an undefined boundary.

Identity exit: the normative documents distinguish TOS from a conventional microkernel with scripts and expose known contradictions rather than hiding them.

## Stage 1 — Trusted boot foundation

Deliverables:

- UEFI loader and x86_64 nucleus entry;
- memory map and exception setup;
- structured serial diagnostics;
- deterministic capsule v1;
- immutable source lookup;
- real source-commit or detached-source-set identity;
- QEMU harness and corruption tests;
- versioned boot protocol.

Engineering exit: clean checkout boots and validates canonical text from a real capsule.

Identity exit: capsule and nucleus prove exact source provenance; anonymous/hard-coded text is rejected as official evidence.

## Stage 1.5 — Language foundation decision

Deliverables:

- completed language evaluation matrix;
- comparable prototypes or formal evidence;
- trusted-base/dependency/performance analysis;
- canonical-source and verifier-boundary analysis;
- accepted language-foundation ADR.

Engineering exit: grammar/runtime implementation can begin against an accepted contract.

Identity exit: selected foundation preserves canonical text, capability semantics, bounded bootstrap and source observability without a hidden host ABI.

## Stage 2 — Native textual reference runtime

Stage 2 begins with a complete proposed semantic/IR specification and the
single Project Architect acceptance checkpoint for that contract. No production
parser, checker, IR verifier, interpreter, cache, or runtime begins before that
acceptance. Programmer documentation and canonical language examples evolve
with the specification and implementation; they are not end-stage cleanup.

Deliverables:

- normative lexical, syntax and semantic specification;
- parser and complete diagnostics;
- bootstrap-profile type checker;
- TOS IR schema and independent verifier;
- reference interpreter;
- source maps and resource limits;
- conformance, performance and fuzz tests.

Engineering exit: `/system/boot/init.tos` executes real language semantics.

Identity exit: runtime behavior maps to canonical source and disposable caches can be deleted/regenerated.

## Stage 3 — Process, IPC and capability substrate

Deliverables:

- isolated address spaces and scheduler;
- capability handles;
- typed IPC;
- supervisors and service manifests;
- process source identity;
- failure/restart and authority-denial tests;
- IPC performance report.

Engineering exit: textual services communicate through final-style interfaces with enforced authority boundaries.

Identity exit: privileged behavior is exercised by source-identified textual processes, not hidden binary policy services.

## Stage 4 — Textual boot drivers and storage

Deliverables:

- PCI discovery service;
- interrupt/MMIO/DMA contracts;
- VirtIO block textual driver;
- persistent object/state storage;
- capsule-to-repository handoff;
- crash/reset and adversarial-device tests;
- Stage 4 performance contract report.

Engineering exit: persistent storage works through a textual user-space driver.

Identity exit: the textual driver performs actual I/O from canonical source; no binary shadow driver or hidden host path exists.

## Stage 5 — Git-native system tree

Deliverables:

- declared compatibility profile at least G2;
- bounded object store and commit/tree/blob traversal;
- immutable `/system` mount by commit;
- writable source overlay;
- status, diff, commit and branch services;
- protected refs and transition audit;
- candidate/last-known-good activation and rollback;
- repository performance/fault-injection reports.

Engineering exit: running system is identified by a commit and can return from a failed candidate.

Identity exit: commit tree is the installed `/system`, not metadata around another package/image authority.

## Stage 6 — Native shell and self-editing workflow

Deliverables:

- textual shell and editor/protocol;
- source inspection and module validation;
- transactional service replacement;
- commit creation inside TOS;
- documentation browser;
- recovery-shell parity for core operations.

Engineering exit: TOS modifies, validates, commits and activates its own services without the host OS.

Identity exit: owner-visible source is the actual installed system and changes flow through repository transactions.

## Stage 7 — Network and remotes

Deliverables:

- VirtIO network driver;
- network service architecture;
- transport-specific threat model and performance contracts;
- secure time policy;
- declared G4 fetch/push/clone profile;
- authenticated remotes and remote recovery.

Exit: recovery media plus credentials can restore `/system` from a remote while verifying identity and preserving owner trust choice.

## Stage 8 — Extensible languages

Deliverables:

- frontend ABI and sandbox;
- frontend registry;
- one second language frontend written through accepted TOS mechanisms;
- cache/source-map integration;
- honest compatibility declaration.

Exit: a language is added without nucleus modification or loss of source/runtime identity.

## Stage 9 — Broader device and UI platform

Potential deliverables include VirtIO input/GPU, compositor, shell/UI, USB, audio and a physical x86_64 profile. Each subsystem has its own threat, performance and identity gates.

## Stage 10 — Self-hosted nucleus toolchain

Long-term goal:

- build necessary nucleus artifacts within TOS or a reproducibly equivalent trusted service;
- record full provenance;
- verify candidate artifacts;
- preserve independent recovery-builder capability.

The nucleus source remains canonical; the boot image remains a necessary derived binary.

## Cross-stage gates

Every stage closes only after architecture, identity, engineering, threat/security, performance, compatibility, licence, provenance, patent/naming and documentation gates appropriate to it pass.

Before Stage 4 closes, review user-space interrupt, DMA and interpreted-driver patent/security mechanisms. Before Stage 5 closes, review content-addressed activation claims. Before commercial distribution, obtain jurisdiction-specific legal review.
