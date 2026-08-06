<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Boot and recovery model

## Goals

The boot path must be deterministic, inspectable, transactional, and recoverable. It must be possible to boot without trusting the current writable state of the machine.

## Boot stages

### Stage 0 — Firmware

UEFI locates and launches the TOS loader from a small boot partition or removable recovery medium.

### Stage 1 — Loader

The loader:

1. reads the boot-control record;
2. selects `candidate`, `current`, `last-known-good`, or an operator-selected commit;
3. loads the nucleus image;
4. loads the immutable boot capsule;
5. gathers memory-map, framebuffer, ACPI, and platform data;
6. constructs the versioned boot protocol block;
7. exits firmware boot services;
8. transfers control once.

The loader performs no policy-heavy update logic.

### Stage 2 — Nucleus bootstrap

The nucleus validates:

- boot protocol magic and version;
- memory ranges and alignment;
- capsule structure and digest;
- selected commit identity format;
- recovery policy flags.

It initializes memory isolation, exceptions, logging, scheduling, IPC, and the TOS Core bootstrap runtime.

### Stage 3 — Text init

The nucleus executes `/system/boot/init.tos` from the capsule. This component launches boot-critical driver services, discovers repository storage, verifies the selected commit, and transitions to the repository-backed system tree.

### Stage 4 — Repository system

The repository-backed `/system/boot/init.tos` takes over. It may differ from the capsule copy only through a defined handoff protocol. It launches normal services, health checks, login, shell, and UI.

## Boot control record

The control record is stored redundantly and updated atomically. It contains at least:

```text
format_version
sequence_number
current_commit
candidate_commit
last_known_good_commit
recovery_commit
candidate_attempts
candidate_max_attempts
boot_mode
required_signature_policy
record_digest
```

Two or more copies are written with sequence numbers. The loader selects the highest valid sequence. Partial writes must not destroy the previous valid record.

## Candidate activation

A system update never overwrites `current_commit` directly.

1. New commit is fetched and verified.
2. Required caches and any new nucleus boot artifact are prepared in an inactive slot.
3. `candidate_commit` is set with an attempt budget.
4. Machine boots candidate.
5. System runs declared health checks.
6. On success, candidate is promoted to `current` and `last-known-good`.
7. On repeated failure, loader clears candidate and returns to last-known-good.

## Health declaration

A commit may declare boot health requirements in `/system/boot/health.tos` or a versioned manifest, including:

- repository mounted;
- essential driver services healthy;
- writable state available or intentionally read-only;
- console available;
- scheduler watchdog alive;
- optional network target reachable;
- no fatal schema migrations pending.

Health checks have time limits and stable result codes.

## Recovery environment

Recovery consists of:

- a trusted nucleus image;
- a minimal immutable capsule;
- repository object inspection;
- local disk discovery;
- network configuration sufficient for clone/fetch;
- commit listing and verification;
- boot-control repair;
- state and secret volume discovery;
- an operator shell.

Recovery must work even when the active `/system`, caches, or normal services are corrupt.

## Restoration workflow

A blank machine can be restored by:

1. booting recovery media;
2. partitioning or selecting storage;
3. cloning the system repository;
4. selecting a commit and machine configuration branch;
5. restoring encrypted state and secrets separately;
6. generating derived caches and boot artifact slots;
7. committing the boot-control record;
8. rebooting.

## Nucleus changes

The nucleus is the binary exception. Its source belongs in the system repository, but the executable image is derived.

When a selected commit changes nucleus source:

- an approved builder creates a deterministic or reproducibly verifiable image;
- the image is associated with source commit, toolchain identity, and build manifest;
- it is installed into an inactive boot slot;
- the loader selects the slot only for the candidate commit;
- rollback preserves the previous slot.

The system must distinguish "commit containing nucleus source" from "verified boot artifact derived from that commit."

## Capsule provenance and licence inventory

The boot capsule header or signed manifest contains:

```text
capsule_format_version
source_commit
architecture_spec_version
nucleus_abi_range
builder_identity
material_digests
included_path_hashes
licence_notice_digest
whole_capsule_digest
```

The first implementation may omit cryptographic signatures if the stage has not yet introduced key policy, but it may not omit deterministic identity fields. Recovery can display the source relationship of every included component.

## Owner-authorized boot

Official developer and research profiles provide a documented path to boot an owner-modified commit. Secure defaults may require explicit physical or recovery action. Candidate state, warnings and signatures are recorded, but no vendor-only secret is a permanent prerequisite for owner control.
