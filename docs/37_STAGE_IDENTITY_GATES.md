<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage identity gates

## Purpose

Early TOS development necessarily contains familiar OS work: UEFI, memory, IPC, PCI and VirtIO. Conventional feature success does not prove that the project is still TOS.

Every stage therefore has an identity question, required evidence and explicit failure conditions. The evidence is archived under `legal/release-manifests/` or a later versioned conformance store.

## Stage 0 — Architecture identity

Question: is TOS distinguishable in normative contracts before implementation?

Evidence:

- active invariants and ADR set;
- threat model;
- documentation authority map;
- architecture-preservation policy;
- licensing/provenance baseline;
- no unresolved contradiction at an implementation boundary.

Failure conditions:

- generated summary treated as independent authority;
- core concepts described only by slogans;
- undefined trust or ownership boundary.

## Stage 1 — Source-bearing boot identity

Question: does the first boot artifact prove that it carries canonical source from an identified repository state rather than anonymous embedded text?

Evidence:

- real Git repository or explicit detached-source-set identity;
- capsule manifest binds source commit/tree, paths, hashes, builder, ABI and output digest;
- nucleus reports structured source identity for `/system/boot/init.tos`;
- corruption and identity-mismatch tests fail closed;
- generated documentation is in sync at the source commit.

Failure conditions:

- all-zero or invented official commit;
- hard-coded text with no source object provenance;
- capsule treated as canonical installed system.

## Stage 1.5 — Language-foundation identity

Question: does the selected language/runtime foundation preserve canonical text, capability semantics, deterministic lowering, bounded bootstrap and source observability?

Evidence:

- completed evaluation matrix;
- comparative prototypes/test vectors;
- accepted selection ADR;
- rejected alternatives and reasons;
- explicit trusted-base and licence analysis.

Failure conditions:

- choosing a language solely because an interpreter is available;
- making Wasm/bytecode the canonical source;
- undocumented host/C ABI becoming the true system contract.

## Stage 2 — Executed-source identity

Question: is actual language semantics executing from canonical text with a verifiable mapping to runtime behavior?

Evidence:

- normative grammar and semantics;
- source -> AST -> typed IR -> execution trace;
- independent verifier;
- cache deletion/regeneration test;
- source mutation invalidates old cache;
- runtime introspection reports source and engine identity.

Failure conditions:

- command dispatcher presented as a language;
- executable derivative accepted without source binding;
- diagnostics cannot identify source spans.

## Stage 3 — Authority-bearing textual service identity

Question: do textual processes exercise real capability/IPC contracts rather than running as decorative scripts around privileged binary services?

Evidence:

- process source identity bound to commit/blob;
- explicit granted capability set;
- denial and confused-deputy tests;
- privileged policy remains outside ordinary module code;
- service restart preserves identity/audit records.

Failure conditions:

- ambient root-equivalent authority;
- ordinary service logic moved into nucleus for convenience;
- textual manifest grants itself authority.

## Stage 4 — Textual driver identity

Question: does a canonical textual user-space driver actually move persistent data through final-style MMIO/interrupt/DMA/IPC boundaries?

Evidence:

- driver loaded from identified commit/blob or Stage-compatible source set;
- device capabilities only;
- DMA and interrupt threat tests;
- performance contract report;
- crash/restart and device-reset behavior;
- no binary shadow driver performs the real I/O.

Failure conditions:

- text merely configures an in-kernel driver;
- hidden host I/O path;
- performance is unmeasured or achieved by bypassing isolation.

## Stage 5 — Commit-as-system identity

Question: is the running `/system` genuinely the selected commit tree, with transactional history operations as runtime behavior?

Evidence:

- declared Git compatibility profile at least G2;
- immutable `/system` mounted by commit;
- writable overlay is distinct;
- candidate/current/last-known-good/recovery transitions survive fault injection;
- failed candidate returns to previous commit;
- process source identities agree with active commit;
- no eager binary package installation is the hidden authority.

Failure conditions:

- Git only tracks development sources;
- commit is metadata around a separately installed binary tree;
- rollback copies files ad hoc without protected history semantics.

## Stage 6 — Self-modifying open-system identity

Question: can TOS inspect, modify, validate, commit and activate its own canonical textual system without an undocumented host workstation?

Evidence:

- in-system edit and diff;
- validation and tests;
- commit creation;
- candidate activation and rollback;
- documentation/source browser tied to active commit;
- recovery shell can inspect and select commits.

Failure conditions:

- host compiler required for ordinary textual services;
- edit affects a shadow copy but not installed identity;
- activation bypasses repository transaction.

## Stage 7 — Remote recovery identity

Question: can a recovery environment reconstruct the same system identity from a remote without trusting the failed active system?

Evidence:

- declared G4 transport profile;
- authenticated remote and malicious-server tests;
- separate secret restoration;
- selected commit and artifact provenance verified;
- owner chooses trust policy.

## Stage 8 — Extensible-language identity

Question: can a second language become a first-class textual source without nucleus modification or loss of provenance?

Evidence:

- frontend ABI conformance;
- deterministic lowering and source maps;
- capability import enforcement;
- cache identity and runtime introspection;
- honest compatibility profile.

## Stage 9 — Platform expansion identity

Question: do UI and physical-device additions remain textual, capability-confined and commit-addressed rather than forcing a second hidden OS layer?

Evidence is subsystem-specific and must include source identity, authority, recovery and performance reports.

## Stage 10 — Toolchain identity

Question: can necessary binary nucleus artifacts be reproduced and verified without becoming the canonical installed truth?

Evidence:

- source commit and complete build provenance;
- independent reproducibility;
- recovery builder path;
- owner-installable artifact authorization;
- canonical-source rule unchanged.

## Gate report format

Each stage report contains:

```text
stage
source_commit
architecture_version
identity_question
required_evidence[]
produced_artifacts[]
tests[]
performance_report
threat_model_coverage
compatibility_profiles
known_failures[]
architect_approval
```

A stage may remain open indefinitely. It must not be declared complete with missing identity evidence.
