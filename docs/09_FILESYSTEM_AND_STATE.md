<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Filesystem and state separation

## Why one Git repository cannot contain every changing byte

The system source changes relatively rarely. Logs, caches, databases, leases, queues, and runtime state may change thousands of times per second. Mixing them would destroy meaningful history and make rollback unsafe.

TOS therefore presents one namespace backed by several stores.

## Namespace classes

### `/system`

Immutable source tree of the selected system commit.

Properties:

- read-only;
- content-addressed;
- reproducible;
- visible source identity for every file;
- executable modules load from here by default.

### `/work`

Explicit writable overlays for source development.

Properties:

- changes are visible through status and diff;
- not automatically trusted or activated;
- may contain multiple named workspaces or branches;
- can be discarded without altering current system.

### `/config`

Deployment and machine configuration.

Configuration may be:

- part of a machine branch;
- a separate signed repository;
- a layered commit;
- local uncommitted configuration in research mode.

The selected model must remain explicit. Secrets never appear here in plaintext.

### `/state`

Mutable durable state owned by services.

Examples:

- databases;
- message queues;
- network leases;
- service indexes;
- update transaction records;
- user session metadata.

State paths are namespaced by service identity and protected by capabilities.

### `/home`

User-owned data. Users may independently choose versioning, snapshots, or ordinary storage.

### `/secrets`

Encrypted secrets and keys. Access is capability-mediated and audit logged. Secret material is not exposed through ordinary recursive repository operations.

### `/cache`

Disposable generated data:

- parsed modules;
- IR;
- native-code caches;
- thumbnails;
- package indexes;
- downloaded but verified objects that can be reacquired.

Deleting `/cache` cannot destroy canonical functionality.

### `/run`

Ephemeral handles, sockets, service discovery entries, locks, and runtime metadata. Recreated on boot.

### `/dev`

A logical device namespace exposing service endpoints and capability-safe handles, not necessarily raw device files with ambient access.

### `/vendor`

External vendor-controlled opaque material: CPU microcode, GPU and peripheral
firmware, and comparable bytes produced outside the project that TOS cannot
express as editable source.

Properties:

- not canonical TOS source and never presented as source;
- not a derived cache — deletion requires reacquisition from the vendor, not
  regeneration from `/system`;
- not part of the system commit, so `/system` rollback does not roll it back;
- identified by vendor, object identity, version and content hash;
- never merged into or mounted inside `/system`.

`/system` may declare a requirement on a vendor object as canonical source text.
The opaque bytes stay here. Firmware is one class inside `/vendor`; there is no
separate `/firmware` root.

This namespace is defined by ADR-0030. No implementation is required
before the stage that first needs physical-hardware firmware.

## State schema versions

Every service with durable state declares:

- state schema identifier and version;
- compatible source-module versions;
- migration functions;
- downgrade policy;
- snapshot requirements;
- maximum supported migration chain.

A system commit requiring migration cannot become current until the migration plan is validated.

## Snapshot linkage

State snapshots may record the system commit with which they were consistent. Rollback tooling can warn when a state snapshot is newer or incompatible with the selected commit.

## Transaction boundaries

System-source commits and mutable-state transactions are separate. A coordinated update record ties them together:

1. snapshot state;
2. stage candidate commit;
3. run forward migration in candidate namespace;
4. boot candidate;
5. promote on health success;
6. retain reverse path or snapshot until policy allows cleanup.

## Filesystem implementations

The first implementation may use a simple native object store and state filesystem under QEMU. The VFS and capability contracts must not assume a particular disk format. Support for conventional filesystems may later be implemented as user-space services.
