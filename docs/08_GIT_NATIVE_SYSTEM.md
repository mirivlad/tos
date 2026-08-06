<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Git-native system model

## Principle

The durable identity of a TOS installation is a commit graph. The operating system is not merely stored in a Git repository for development; repository semantics are part of normal operation.

## Canonical scope

The system repository contains:

- nucleus and loader source;
- textual system components and drivers;
- language frontends;
- schemas and policies;
- tests and documentation;
- machine-profile templates;
- build manifests and source-to-artifact attestations.

Derived nucleus images, IR caches, JIT output, logs, mutable databases and secrets are not canonical repository content.

## Compatibility is profiled

TOS does not make an undifferentiated promise of “Git support.” `docs/36_GIT_COMPATIBILITY_PROFILES.md` defines G0 through G6.

Stage 1 requires G0 provenance identity. Stage 5 requires at least G2 deterministic local history. Stage 7 remote recovery requires the declared G4 transport profile.

A release states its object format, hash family, ref profile, pack support and transport support explicitly.

## Nucleus versus userspace responsibility

The nucleus implements only mechanisms required for trusted boot and immutable mounting at the selected profile:

- algorithm-qualified content-ID parsing;
- object-integrity verification;
- bounded commit/tree traversal through a narrow object-store interface;
- reference selection from boot control;
- immutable tree exposure;
- protected transactional ref primitives.

Textual privileged services implement:

- working-overlay status;
- diff;
- object and commit creation;
- branch management;
- merge and conflict handling;
- fetch, push and clone;
- pack/index optimization;
- signature-policy UI;
- retention and garbage collection.

No stage is required to implement every item at once. Its profile states the exact subset.

## Active tree

At boot, the selected commit tree is mounted read-only as `/system`.

A writable overlay at `/work/system` records proposed source changes. A union view may expose them to development tools, but running services report whether they originated from committed or overlay source.

The active commit is not decorative metadata around an independently installed binary tree.

## Commit creation

A system commit operation performs:

1. source validation;
2. module dependency resolution;
3. required tests;
4. capability-manifest validation;
5. schema compatibility checks;
6. performance/identity checks required by the stage;
7. optional reproducibility checks for nucleus changes;
8. object and commit creation;
9. optional signing;
10. update of a non-active branch or candidate reference.

Committing does not automatically activate the commit.

## Commit metadata

In addition to ordinary Git commit data, TOS records structured versioned metadata in an ordinary-tree-visible form or signed note:

- TOS schema version;
- parent system commit;
- machine/hardware profiles tested;
- test and performance results;
- threat-model/security evidence changes;
- required state migrations;
- nucleus artifact attestations;
- capability-policy changes;
- loader/nucleus compatibility;
- human-readable rationale.

Metadata remains inspectable by ordinary Git clients even if they do not interpret it.

## Branch and protected-ref model

Suggested names:

```text
refs/heads/main                    upstream system
refs/heads/machines/<machine-id>  machine customization
refs/heads/users/<name>           optional user environment
refs/heads/experiments/<topic>    experimental work
refs/tos/current                  active commit
refs/tos/candidate                next boot candidate
refs/tos/last-known-good          successful fallback
refs/tos/recovery                 protected recovery commit
```

Protected semantics may be implemented in boot-control storage rather than ordinary mutable files, but every transition remains explicit and auditable.

## Updates as merges

An update is a merge or fast-forward between histories, not replacement of opaque packages.

The update service shows:

- upstream and local changes;
- conflicts;
- capability changes;
- driver, language and schema impacts;
- state migrations;
- tests and performance evidence;
- candidate rollback plan.

## Bisect

TOS integrates automated bisect with boot and service health probes. A bisect session records tested commits and outcomes separately from immutable source commits.

## Retention and garbage collection

Garbage collection protects:

- current, candidate, last-known-good and recovery commits;
- signed release refs;
- commits required by retained state snapshots;
- commits not yet pushed to configured remotes;
- operator-pinned branches;
- objects needed by an in-progress activation or recovery transaction.

An object is not deleted merely because it is unreachable from `main`.

## Remote recovery

A recovery environment can clone or fetch a declared G4 profile and recreate system refs. Secrets and mutable state are restored through separate encrypted mechanisms.

## Work decomposition

Repository implementation is intentionally staged:

1. G0 source identity in capsule/runtime provenance;
2. G1 bounded loose-object reading;
3. G2 deterministic local object writing and protected refs;
4. G3 packed-object reading;
5. G4 remote transport;
6. G5 history manipulation;
7. G6 maintenance and scale.

This prevents packfiles, merge and networking from being hidden inside the phrase “Git-native.”

## Performance and threat requirements

Repository parsers and activation paths follow:

- `docs/34_THREAT_MODEL.md` for malicious object graphs, rollback, ref mutation and retention threats;
- `docs/35_PERFORMANCE_CONTRACTS.md` for lazy mounting, lookup, activation and scale fixtures;
- `docs/37_STAGE_IDENTITY_GATES.md` for proof that commit identity is the actual installed system.

## External implementations and patents

Command-line Git and libgit2 may be host-side oracles and tooling. They are not hidden runtime foundations without ADR review.

Before Stage 5 closes, active patent claims around content-addressed deployment, link-switching, patch mementos and rollback are reviewed. TOS activation remains commit/tree based rather than copying a patented claim combination for convenience.

## Licence boundaries

Repository schemas, independent interoperability readers and test vectors may be Apache-2.0. Official activation, boot-control and recovery services remain GPL-3.0-or-later.
