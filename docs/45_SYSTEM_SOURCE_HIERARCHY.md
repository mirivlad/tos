<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Runtime system source hierarchy

- Status: **Accepted Tier 2 contract — implementation deferred to the stage that
  first needs each subsystem**
- Authority on acceptance: Tier 2 under
  `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`
- Governing Tier 1 decisions: ADR-0002, ADR-0031, ADR-0030
- Companion documents: `docs/03_ARCHITECTURE_OVERVIEW.md`,
  `docs/09_FILESYSTEM_AND_STATE.md`, `docs/17_REPOSITORY_LAYOUT.md`

## Status and boundary

This document describes the source hierarchy of a **running TOS installation**.
`docs/17_REPOSITORY_LAYOUT.md` describes the layout of the development
repository and remains authoritative for that purpose. Where the two overlap,
section 2 of this document defines the mapping.

This document defines placement and classification. It does not define module
resolution rules, manifest schema, capability grammar, activation mechanics or
storage format; those belong to `docs/05`, `docs/10`, `docs/12`, `docs/13` and
the versioned interface contracts. No directory described here is required to
exist before the stage that first implements the subsystem it serves.

## 1. Namespace classification

Every path visible to a running TOS installation belongs to exactly one class.
The class determines what deletion means, what rollback means and whether the
content is canonical.

| Class | Meaning | Deletion | Root namespaces |
|---|---|---|---|
| Canonical source | defines system behavior; commit-addressed and read-only | not possible while active; changes require commit and activation | `/system` |
| Source overlay | candidate canonical source, not yet trusted or activated | discards candidates only | `/work` |
| Configuration | machine and deployment configuration | changes machine behavior; versioning model is explicit | `/config` |
| Mutable state | durable data owned by services and users | loses data | `/state`, `/home`, `/secrets` |
| Derived cache | reproducible from canonical source and declared inputs | forces regeneration only | `/cache` |
| Ephemeral | recreated on boot | none | `/run` |
| Capability namespace | mediated handles, not stored bytes | not applicable | `/dev` |
| External material | vendor-controlled opaque material outside TOS ownership | requires reacquisition from the vendor | `/vendor` |

Consequences that follow from the table and are normative:

- `/system` **MUST NOT** contain derived executable artifacts, generated caches,
  mutable state, or vendor-controlled opaque material;
- `/cache` **MUST NOT** contain anything whose loss removes functionality;
- `/vendor` **MUST NOT** be presented as, mounted inside, or merged into
  `/system`, per ADR-0030;
- deleting `/cache` and rebooting **MUST** yield the same system behavior.

## 2. Repository-to-runtime mapping

The development repository subtree `source/system/` is the canonical input for
the runtime `/system` tree. A system commit's `system/` tree becomes the
installation's read-only `/system` when that commit is selected.

The mapping is direct and unrenamed: `source/system/boot/init.tos` in the
repository is `/system/boot/init.tos` in the running installation. A build step
that rewrites, relocates or generates entries between the two would break the
source-to-runtime chain required by I-16 and is not permitted.

Repository directories outside `source/system/` — `boot/`, `nucleus/`,
`crates/`, `interfaces/`, `host-tools/`, `tests/`, `docs/`, `legal/` — are
project development material. They produce the binary trusted base, derived
artifacts and evidence; they are not installed as `/system` content.

## 3. `/system` hierarchy

```text
/system/
    boot/           boot entry source, health requirements, boot policy
    services/       system service modules
    drivers/        user-space device driver modules
    languages/      language frontend modules
    lib/            shared textual modules used by other components
    apps/           applications delivered with the system commit
    shell/          command interpreter and console environment
    ui/             graphical environment source
    policy/         system policy source
    schemas/        versioned IPC, state and interface schema source
    machine/        machine-specific system source
    third-party/    imported textual source with provenance metadata
    lock/           resolved dependency, frontend, schema and vendor locks
```

Every entry is canonical source text. The names are normative at the conceptual
level; exact storage and mount implementation may evolve through ADRs.

### `boot/`

Contains `init.tos` and the boot health requirements referenced by
`docs/04_BOOT_AND_RECOVERY.md`. The capsule copy of `/system/boot/init.tos` and
the repository-backed copy are related through the handoff protocol defined
there; the capsule remains a transport and recovery seed, never a second
installed system.

### `services/`, `drivers/`, `languages/`

Textual components launched as isolated processes under
`docs/10_PROCESS_SERVICE_IPC.md`, `docs/11_DRIVER_MODEL.md` and
`docs/07_LANGUAGE_FRONTENDS.md`. What a component **needs** — its capability
requests, resource envelope, imports and exports — is declared inside its own
module source in accepted TOS Core V1 form, as shown in
`docs/11_DRIVER_MODEL.md`; TOS does not keep a parallel description of the code
that could drift from the code it describes.

How a component is **supervised** — restart policy, health probes, state
namespace, shutdown timeout — is not a description of the code but a decision
about it, made by the authority that launches it, and lives in `policy/` below.
ADR-0051 fixes that split.

### `lib/`

Shared textual modules imported by other `/system` components. A module is
placed here when more than one component depends on it. Placement grants no
authority: a library module holds no capabilities of its own and receives only
what its caller passes.

### `apps/`

Applications delivered as part of the system commit. Applications installed and
owned by a user are not `/system` content and do not appear here.

### `shell/`, `ui/`, `policy/`

Console environment, graphical environment and system policy source. Policy is
canonical text like any other component; it is not a binary configuration
database.

### `schemas/`

Source of record for the versioned boundaries required by I-09: IPC message
schemas, durable state schemas and interface contracts as consumed by the
running system. Schema version identity is part of activation validation under
`docs/13_UPDATE_MERGE_PACKAGE_MODEL.md`.

### `machine/`

System source that applies to a specific machine or hardware profile — for
example a board-specific driver set or platform quirk module. This is source and
therefore lives in the system commit.

Machine *configuration* is not source and remains in `/config` under
`docs/09_FILESYSTEM_AND_STATE.md`. The distinction is: if changing it requires a
source change, commit and activation, it belongs in `/system/machine/`; if it is
deployment data consumed by a component, it belongs in `/config`.

### `third-party/`

Textual source imported from outside the project, retaining upstream metadata,
patch series, provenance and licence records as required by
`docs/27_THIRD_PARTY_COMPONENT_POLICY.md` and
`docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`.

Material here is canonical source: readable, modifiable and rebuildable by the
owner. Material that cannot satisfy that description is not third-party textual
source — it is vendor-controlled opaque material and belongs in `/vendor` under
ADR-0030.

### `lock/`

The resolved lock manifests required by `docs/13_UPDATE_MERGE_PACKAGE_MODEL.md`:
exact dependency identities, frontend versions, schema versions, required
runtime ABI and the identity/version/hash of every required `/vendor` object.

Lock content is generated during update resolution but is **not** a derived
cache: it is committed canonical source, because it records the decisions that
define the system commit. Regenerating it may produce a different result at a
different time, so it cannot be discarded and rebuilt.

## 4. Relationship between `/work` and `/system`

`/work` holds writable overlays with the same shape as `/system`. An overlay is
a proposal, not an installation.

- an overlay path corresponds to the `/system` path it proposes to change;
- overlay content is never executed as system source without explicit
  validation and transactional activation under I-05;
- an overlay may be discarded without affecting the active commit;
- multiple named overlays or branches may exist simultaneously;
- status and diff against the active commit are always available.

Editing source in a running system means editing an overlay and then committing
and activating it. It does not mean mutating `/system`, which is read-only by
class.

## 5. Dependencies on `/vendor`

A `/system` component that requires vendor-controlled opaque material declares
that requirement in its own manifest, alongside its capability requirements, in
the same way `docs/11_DRIVER_MODEL.md` declares device and capability needs. The
declaration names vendor, object identity, version, content hash, expected
`/vendor` placement, compatibility constraints and behavior when the object is
absent, mismatched or refused.

`/system/lock/` aggregates the resolved set for the commit so that the required
external material of a system commit can be listed without traversing every
component.

The opaque bytes never appear in `/system`. The declaration is a reference.
Full rules are in ADR-0030.

## 6. Conformance expectations

When this hierarchy is implemented, architecture conformance tests under
`docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md` must enforce that:

- no path under `/system` resolves to derived-cache, mutable-state or
  vendor-material content;
- deleting `/cache` and rebooting reproduces identical system behavior;
- every running non-nucleus component reports a `/system` source path that
  exists in the active commit;
- an overlay path in `/work` cannot execute as system source without passing
  activation;
- the set of required `/vendor` objects for the active commit is enumerable from
  `/system/lock/` and matches the per-component declarations.
