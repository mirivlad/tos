<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Testing and verification strategy

## Principle

TOS changes the boot, execution, driver and update models simultaneously. Tests are part of architecture, not a finishing phase.

Every stage requires:

- functional evidence;
- negative/adversarial evidence from the threat model;
- TOS identity evidence;
- performance evidence assigned to that stage;
- compatibility-profile evidence where relevant.

## Test layers

### Host unit tests

Pure libraries for capsule parsing, object identity, repository traversal, schemas, parsers, IR verification and merge logic run as ordinary host tests.

### Property and fuzz tests

Required for all parsers and untrusted inputs:

- boot protocol and capsule;
- Git objects, indexes and packs in the selected profile;
- IPC messages;
- language source and IR;
- filesystem/state metadata;
- device descriptors and queue data.

Properties include no panic, bounded resource use, deterministic output, quota enforcement and round-trip stability where applicable.

### Golden vectors

Versioned formats include committed vectors with:

- valid minimal object;
- valid complex object;
- each invalid boundary class;
- digest values;
- expected decoded representation;
- resource-limit expectations.

### QEMU integration tests

Automated tests verify, by stage:

- clean boot and corrupted-capsule rejection;
- source identity and text init execution;
- process isolation and capability denial;
- textual driver startup and failure containment;
- repository mount by commit;
- candidate promotion and failed-candidate rollback;
- Git restoration and bisect workflows.

### Runtime conformance

Reference interpreter, bytecode engine and future native backends run the same language/IR suite. Source maps, errors and resource limits are compared, not merely final output.

### Driver simulation

VirtIO and later device tests include malformed descriptors, reset, interrupt loss/storm, timeout, stale completion, DMA-boundary violation and device removal.

## Threat-model tests

Each threat introduced by a stage maps to at least one of:

- parser rejection test;
- capability-denial test;
- fault injection;
- fuzz target;
- recovery test;
- audit/provenance assertion;
- explicit accepted non-goal.

A change adding a new boundary without a negative test leaves the stage open.

## Stage identity tests

`docs/37_STAGE_IDENTITY_GATES.md` defines required evidence. Examples:

- Stage 1 capsule source identity matches the real repository commit;
- Stage 2 cache deletion regenerates executable state from text;
- Stage 3 textual service holds only declared capabilities;
- Stage 4 no hidden binary driver performs I/O;
- Stage 5 process identities and `/system` agree on active commit;
- Stage 6 edit/commit/activate occurs without undocumented host tooling.

## Reproducibility

A clean checkout builds and tests using documented commands. Toolchains are pinned. CI does not depend on developer home directories, undeclared networks or secret local caches.

The consolidated specification is deterministically regenerated and checked for drift.

## Performance tests

`docs/35_PERFORMANCE_CONTRACTS.md` is normative. Benchmarks begin before optimization and include environment, baseline, percentiles and source identity.

A stage cannot close on unmeasured qualitative performance. A benchmark oracle does not become accepted runtime architecture.

## Git compatibility tests

Claims follow `docs/36_GIT_COMPATIBILITY_PROFILES.md`. Independent Git implementations provide cross-checks at the declared profile. Pack, transport, merge and maintenance tests are not implied by loose-object reading.

## Architecture tests

Examples:

- no canonical executable cache committed under `/system`;
- driver implementation does not link into nucleus;
- protected refs require dedicated capability;
- active `/system` is immutable;
- cache deletion preserves functional recovery;
- process reports include complete source identity;
- boot/IPC schemas reject incompatible versions;
- language foundation has an accepted selection ADR before parser code is normative;
- generated consolidated specification matches sources.

## Fault injection

The harness injects:

- partial writes and power loss at every activation phase;
- corrupt and adversarial object graphs;
- driver crash/hang and interrupt anomalies;
- out-of-memory and quota exhaustion;
- invalid commits/signatures;
- state migration failure;
- stale or forged derived caches;
- recovery-media mismatch.

## Milestone gates

A milestone cannot close until:

- specified behavior has automated tests;
- relevant threat-model paths are exercised;
- identity gate evidence exists;
- performance contract is measured;
- claimed compatibility profile passes;
- documentation matches implementation;
- no placeholder remains in claimed scope;
- recovery from introduced failure modes is tested.

## Legal and provenance conformance

CI and release process enforce:

- SPDX identifiers;
- DCO sign-offs;
- dependency/third-party inventory;
- prohibited licence combinations;
- source-to-artifact manifest completeness;
- source/cache/runtime introspection;
- owner-authorized boot workflow;
- architecture change-level declaration.

See `docs/30_COMPLIANCE_AND_RELEASE_GATES.md` and `docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md`.
