<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS manifesto

## The installed program should not hide from its owner

Most operating systems separate the code a person may inspect from the artifact the computer actually executes. Even open-source systems commonly install compiled binaries whose relationship to available source code depends on external build infrastructure, package metadata, and trust.

TOS treats this separation as optional rather than fundamental.

The authoritative form of a service, driver, shell, application, language frontend, and system policy is human-readable text in the active system commit. The runtime may parse, lower, optimize, cache, or translate that text, but those products do not replace the source as the definition of the program.

## A machine should have history, not merely state

A conventional machine accumulates state through installations, upgrades, scripts, package hooks, manual edits, and undocumented recovery actions. The final disk contents often do not explain how the machine arrived there.

TOS makes the system tree a commit graph. Every durable change can have:

- a parent state;
- an author;
- a reason;
- a diff;
- test results;
- hardware context;
- signatures;
- boot-health records.

A machine can therefore answer not only "what is installed?" but "which change made it so?"

## Recovery is a first-class operation

Reinstallation should not be the standard answer to uncertainty. A TOS machine must always know:

- its current commit;
- the previous commit;
- the last commit that passed boot health checks;
- a protected recovery commit or recovery source;
- whether the working overlay contains uncommitted changes.

Restoring a machine is repository reconstruction, not archaeological package management.

## Text does not mean slow by decree

TOS does not fetishize character-by-character interpretation. The source is canonical; execution strategy is an implementation detail.

The runtime may create:

- abstract syntax trees;
- typed intermediate representations;
- bytecode;
- native code;
- persistent caches keyed by source hash and runtime version.

All such artifacts must be reproducible, invalidatable, and deletable without losing the program.

## Extensibility must not enlarge the trusted core without limit

New languages, drivers, services, and applications should normally be installed as textual modules outside the binary nucleus. The nucleus provides mechanisms: memory isolation, scheduling, capabilities, IPC, object access, and boot selection. Policy belongs in textual system components.

## The owner is allowed to break the machine

TOS should provide safe defaults, signatures, capability boundaries, transactional activation, and recovery. It must not convert those protections into ownership denial.

An explicit research mode may permit unsigned commits, experimental drivers, and unrestricted system branches. The system should make risk visible and recovery easy rather than pretending the owner is an attacker.

## Open here, not somewhere else

A system is not fully open merely because a source archive exists on a developer server. TOS should expose the exact source identity of what is running on the owner’s machine, the commit that selected it, the derived artifact that executes it, the capabilities it holds and the history that introduced it.

The desired chain is visible:

```text
source path and content hash
    -> system commit
    -> validated IR/cache identity
    -> running process
    -> granted capabilities
```

The owner should be able to branch that chain and boot the result. Open source that cannot be installed by the owner is incomplete for TOS.

## Openness is architectural and legal

Architecture provides inspectability, source identity, modification and recovery. Copyleft provides downstream legal continuity. Neither substitutes for the other. A readable system under a closing licence can be enclosed; an open licence over an opaque installed binary can remain practically inaccessible. TOS requires both layers.
