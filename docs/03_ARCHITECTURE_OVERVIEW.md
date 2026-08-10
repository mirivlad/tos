<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture overview

## Layer model

```mermaid
flowchart TD
    FW[UEFI firmware] --> BL[Boot loader]
    BL --> CAP[Boot capsule]
    BL --> N[Nucleus]
    N --> REPO[Repository substrate]
    N --> RT[Text execution runtime]
    N --> IPC[Processes, IPC, capabilities]
    RT --> INIT[/system/boot/init.tos]
    INIT --> SVC[Textual system services]
    SVC --> DRV[Textual user-space drivers]
    SVC --> LANG[Language frontends]
    SVC --> UI[Shell and user interface]
    REPO --> SYS[Immutable /system commit]
    REPO --> OVL[Writable system overlay]
```

## Binary nucleus

The nucleus is a small `no_std` binary responsible for mechanism, not broad policy.

It owns:

- CPU mode and exception setup;
- physical and virtual memory management;
- scheduler primitives;
- process address spaces;
- capability handles;
- IPC transport;
- interrupt routing primitives;
- timekeeping primitives;
- boot capsule access;
- minimal repository object verification needed for boot;
- source-runtime bootstrap;
- structured panic and diagnostic output.

It does not own:

- a general network stack;
- a shell;
- package policy;
- graphical desktop policy;
- ordinary filesystems;
- full Git remote operations;
- most device drivers;
- language-specific standard libraries.

## Boot capsule

The loader places a deterministic, immutable capsule in memory before transferring control. It contains everything required before persistent storage becomes accessible:

- `/system/boot/init.tos`;
- the initial TOS Core runtime modules not compiled into the nucleus;
- boot-critical text drivers, such as VirtIO block and console support;
- schemas and manifests;
- a minimal recovery command set;
- expected object IDs and signatures.

The capsule is not the installed system. It is a transport and recovery seed. Once storage and the repository are available, the system resolves the selected commit and mounts its `/system` tree.

Every capsule carries provenance naming the canonical source commit, included source hashes, builder identity, target ABI, format version and whole-capsule digest. Its canonical inputs live in the repository; the capsule is reproducible and never becomes an independent hidden system.

## Text runtime

The runtime consumes source text, validates it, lowers it to a typed internal representation, and executes it. The initial language is TOS Core. Additional language frontends are modules that produce the same internal representation under a versioned frontend contract.

## Repository substrate

TOS uses Git semantics for durable system history:

- immutable content objects;
- trees;
- commits;
- references;
- branches;
- merge ancestry;
- remotes through a userspace service.

The boot path needs only read and verification operations. Full clone, fetch, pack, merge, signing, and transport logic belongs in textual privileged services.

## Filesystem view

The visible filesystem is assembled from distinct stores:

```text
/system   immutable tree of selected commit
/work     writable overlay for proposed system-source changes
/config   machine and deployment configuration, versioned separately or layered
/state    mutable durable service state
/home     user data
/secrets  encrypted capability-protected secrets
/cache    disposable generated data
/run      ephemeral runtime objects
/dev      capability-mediated device namespace
/vendor   external vendor-controlled opaque material, outside canonical source
```

The names are normative at the conceptual level; exact mount implementation may evolve through ADRs.

`/vendor` holds material TOS does not own or author, such as CPU microcode and
device firmware. It is not part of the canonical `/system` tree and is never
presented as TOS source. `/system` may declare that it requires a vendor object
by identity, version and hash; the opaque bytes themselves stay in `/vendor`.
See ADR-0030.

The internal structure of the runtime `/system` tree is defined by
`docs/45_SYSTEM_SOURCE_HIERARCHY.md`.

## Process model

Every service and driver is a process with:

- a source identity;
- a module identity;
- a system commit identity;
- a capability set;
- a declared IPC schema set;
- structured health state;
- restart policy;
- logs tied to source locations.

## Trust zones

1. **Firmware and loader** — external or minimally controlled trust.
2. **Nucleus** — highest TOS trust.
3. **Boot/repository authority services** — privileged but isolated.
4. **Drivers** — device-specific capabilities only.
5. **System services** — least authority required.
6. **Applications** — user-granted capabilities.
7. **Experimental branches** — explicitly marked trust state.
8. **External vendor material** — opaque bytes TOS identifies but does not
   inspect, verify or vouch for. TOS states their identity and version; it makes
   no claim about their behavior.

## Compatibility strategy

TOS does not initially emulate POSIX or Linux kernel APIs. Compatibility may later be provided as ordinary services or language runtimes. Native TOS contracts remain capability-oriented and repository-aware.

## Source-to-runtime identity plane

Alongside memory, IPC and repository layers, TOS maintains an identity plane. It answers:

- which commit supplied a module;
- which source bytes were validated;
- which frontend and IR schema were used;
- which derived cache is executing;
- which process instance and capability grants resulted;
- which health and activation transaction introduced it.

This plane is not optional debugging metadata. It is part of the operating-system model and is tested for conformance.

## Architecture preservation boundary

The project may substitute implementations while preserving contracts. It may not substitute away canonical text, commit-addressed system identity, owner-controlled boot, capability isolation or recoverable activation. External components are assigned explicit roles as reference, oracle, host tool, isolated service or trusted dependency.

See `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`, `docs/37_STAGE_IDENTITY_GATES.md` and ADR-0011.

Security assumptions are centralized in `docs/34_THREAT_MODEL.md`; performance-sensitive paths are governed by `docs/35_PERFORMANCE_CONTRACTS.md`; Git claims use `docs/36_GIT_COMPATIBILITY_PROFILES.md`.
