<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Risks and open questions

This document prevents unresolved issues from becoming accidental implementation decisions.

## R1 — Language design scope

Creating a systems language, runtime, OS, driver model, and Git-native platform is an enormous combined effort.

Mitigation:

- keep TOS Core deliberately small;
- specify bootstrap and full profiles;
- complete Stage 1.5 comparative language-foundation review before implementation;
- build a reference interpreter only after the foundation is selected;
- avoid advanced syntax before capability, error, and memory semantics are complete;
- consider adapting a rigorously specified existing core language if a later ADR proves it better satisfies invariants.

## R2 — Performance of text drivers

Interpreted drivers may struggle with high-throughput devices.

Mitigation:

- canonical text with verified bytecode/native caches;
- zero-copy shared memory and DMA;
- batch IPC;
- quantitative contracts from `docs/35_PERFORMANCE_CONTRACTS.md` measured on VirtIO;
- optimize execution engine without changing source model.

## R3 — Git repository scale

Using Git semantics for an entire system may create object-count, checkout, merge, and garbage-collection challenges.

Mitigation:

- immutable object store and virtual tree access rather than eager checkout;
- pack and index services outside nucleus;
- explicit retention policy;
- explicit G0–G6 compatibility profiles rather than an all-or-nothing promise;
- compatibility tests and possible repository extensions that preserve ordinary Git visibility.

Open question: exact initial object/hash/ref profile and the evidence required to promote from G1 to G2/G3.

## R4 — Kernel/repository chicken-and-egg

The selected commit may require a newer nucleus than the currently booted image.

Mitigation:

- versioned minimum nucleus ABI in commit metadata;
- inactive boot slots;
- source-to-artifact attestations;
- recovery nucleus capable of fetching compatible artifacts;
- never destroy previous boot slot during candidate activation.

## R5 — Driver ecosystem effort

Modern GPU, Wi-Fi, USB, and audio support is vast.

Mitigation:

- QEMU and VirtIO first;
- public-specification hardware next;
- explicit non-goal of broad hardware support in early stages;
- tool-assisted porting of device knowledge from open drivers;
- compatibility services may later host existing user-space driver frameworks.

## R6 — Security of textual extensibility

Readable source may create false confidence.

Mitigation:

- capabilities, signatures, isolation, provenance, and transactional activation;
- source review tooling;
- no ambient access for language frontends;
- protected recovery.

## R7 — State rollback incompatibility

Rolling source back does not automatically roll mutable state back safely.

Mitigation:

- state schema declarations;
- linked snapshots;
- reversible migrations or explicit no-downgrade markers;
- candidate namespaces;
- recovery UI warning before incompatible rollback.

## R8 — Bootstrap trust size

A parser and interpreter in the nucleus could become large and dangerous.

Mitigation:

- strict bootstrap profile;
- small reference parser;
- move rich standard library to capsule text;
- fuzz every parser;
- consider a minimal verified bytecode loader only after preserving source-based recovery semantics.

Open question: exact boundary between binary parser, IR verifier, and textual frontend modules.

## R9 — Reproducible nucleus builds

Perfect bit-for-bit reproducibility may be difficult across toolchain and firmware changes.

Mitigation:

- pinned toolchain;
- hermetic build manifests;
- multiple independent builders;
- source and artifact signatures;
- reproducibility treated as a measured property, not assumed.

## R10 — Project size and abandonment

The project may be paused before becoming a daily-use system.

Mitigation:

- coherent stage gates;
- complete documents and tests at each stage;
- no throwaway architecture;
- the research results remain valuable even if implementation stops.

## Decisions still requiring ADRs

- exact initial bootloader strategy;
- exact Git object-format compatibility target;
- nucleus allocator policy;
- TOS Core language foundation selection under ADR-0015;
- selected language grammar, semantics and memory model;
- IPC schema language;
- first persistent object/state filesystem;
- cryptographic algorithms and key-management policy;
- SMP activation stage;
- state snapshot mechanism;
- exact official project name after trademark clearance;
- first professional patent/FTO review scope;
- future architecture-council succession model.

## R11 — Architectural erosion by mature substitutes

A familiar library or runtime may solve a local problem while converting TOS into a conventional microkernel with scripts.

Mitigation:

- architecture preservation policy;
- external implementations default to oracle/host roles;
- dependency promotion ADRs;
- identity conformance tests.

## R12 — Licence incompatibility during driver reuse

Useful Linux driver implementations are commonly GPL-2.0-only, while TOS core is GPL-3.0-or-later.

Mitigation:

- exact file-level licence review;
- public hardware specifications;
- permissive or GPL-2.0-or-later sources;
- clean-room functional reimplementation;
- third-party inventory.

## R13 — Patent exposure in update and driver mechanisms

Individual pieces of TOS have been subjects of patents, and status differs by jurisdiction.

Mitigation:

- maintained landscape;
- claim-focused design review;
- design-around;
- defensive publication;
- professional FTO review before commercial distribution.

## R14 — Project name conflict

`TOS` is historically associated with Atari and is widely used as an abbreviation in other industries.

Mitigation:

- provisional combined name `TOS — TextOS`;
- rename-ready namespaces;
- formal trademark clearance before broad public branding;
- no copied Atari or military visual identity.

## R15 — Copyleft compliance and locked appliances

A distributor may misunderstand source, installation-information or notice duties.

Mitigation:

- release compliance gates;
- full source and provenance package;
- owner-installable conformance requirement;
- legal review for commercial User Products.

## R16 — Normative documentation drift

A consolidated specification or copied requirement may diverge from its source and mislead agents.

Mitigation:

- hierarchy in `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`;
- deterministic generator and source manifest;
- CI byte-for-byte drift check;
- generated file marked non-normative and read-only by policy.

## R17 — Incomplete threat reasoning

Capabilities and provenance may create false confidence if adversary powers and accepted non-goals are not explicit.

Mitigation:

- normative `docs/34_THREAT_MODEL.md`;
- stage-specific negative tests and evidence levels;
- mandatory update for new parsers, trust boundaries, DMA paths, remotes and protected-state mutation.

## R18 — Stage-order identity erosion

Years of ordinary boot, scheduler, PCI and driver work could produce a conventional microkernel with scripts before Git-native identity becomes visible.

Mitigation:

- identity gate for every stage;
- commit/source provenance begins in Stage 1;
- runtime source identity begins in Stage 2;
- actual textual authority and driver evidence in Stages 3–4;
- Stage 5 cannot close unless the commit tree is the installed `/system`.

## R19 — Benchmark-induced architecture substitution

A faster conventional reference driver/runtime may be promoted into production simply because it wins benchmarks.

Mitigation:

- reference implementations remain oracles under ADR-0011;
- performance failure triggers profiling or explicit ADR, not hidden relocation into nucleus;
- identity gate and performance report are reviewed together.

