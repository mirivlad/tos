<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Git compatibility profiles

## Purpose

“TOS supports Git” is too vague to be honest. Git contains object formats, refs, indexes, packfiles, transports, merge behavior, maintenance and a large compatibility surface. TOS declares a profile and passes its conformance suite.

The profiles are cumulative unless an ADR explicitly defines a specialized profile.

## G0 — Commit-addressed identity

TOS records an algorithm-qualified source commit/tree identity in boot and runtime provenance, but does not yet parse a persistent Git repository.

Required by: Stage 1.

Not a claim of Git repository compatibility.

## G1 — Bounded object reading

Capabilities:

- parse and verify the selected loose-object profile;
- read blob, tree and commit objects;
- traverse a tree through a bounded object-store interface;
- read explicitly supported refs;
- reject unsupported algorithms, malformed objects and ambiguous names;
- compare results against independent Git test oracles.

Excluded:

- object writing;
- packfiles;
- network protocols;
- merge/diff semantics;
- garbage collection.

## G2 — Deterministic local history

Adds:

- deterministic blob/tree/commit creation;
- branch and protected-ref operations;
- writable source overlay status;
- commit creation from TOS;
- candidate/current/last-known-good/recovery semantics;
- reflog or equivalent auditable ref-transition history;
- crash-safe local object publication.

Required by: Stage 5 exit gate.

Stage 5 may use loose objects and TOS-specific indexes. It must not claim full Git compatibility merely because ordinary Git can inspect the resulting history.

## G3 — Packed object interoperability

Adds:

- pack index reading;
- bounded pack and delta-chain validation;
- thin-pack policy if supported;
- resource quotas against decompression and delta bombs;
- compatibility tests with independent Git implementations.

Pack writing may be a separate subprofile.

## G4 — Remote interoperability

Adds:

- versioned fetch/push/clone protocol profile;
- authenticated transport;
- partial/interrupted transfer recovery;
- object and ref negotiation;
- credential isolation;
- malicious-remote tests.

Required by: Stage 7 remote-recovery exit gate.

The exact transport—SSH, HTTPS or another protocol—is declared separately.

## G5 — History manipulation

Adds specified subsets of:

- diff;
- three-way merge;
- conflict representation;
- bisect;
- revert;
- ancestry queries;
- signed metadata/notes policy.

Each command publishes semantic differences from command-line Git.

## G6 — Repository maintenance at scale

Adds:

- reachability and retention-root analysis;
- garbage collection;
- repacking;
- multi-pack indexes or equivalent;
- pruning safety;
- hash-family migration;
- corruption diagnosis and repair.

## Object-format declaration

Every repository and boot record names:

- object format/profile version;
- hash algorithm;
- object encoding rules;
- ref storage profile;
- normalization rules;
- supported pack/delta profile;
- extension requirements.

Unknown mandatory extensions fail closed.

## Nucleus boundary

The nucleus implements only the minimum bounded reading/verification mechanism justified by the active stage. Rich Git behavior belongs in isolated textual services.

A general Git library may be a host oracle or isolated service only under the external-implementation policy. It does not silently become the repository authority.

## Compatibility claims

Allowed examples:

- “TOS implements G1 for loose SHA-256 repositories.”
- “TOS histories are inspectable by Git version X under profile Y.”
- “Fetch is not implemented; remote compatibility is G2, not G4.”

Disallowed examples:

- “Git-compatible” without a profile;
- “supports Git” based only on hashes or a development repository;
- “full Git” without complete published conformance scope.
