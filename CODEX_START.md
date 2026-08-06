<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# First Codex task — Stage 1 trusted boot foundation

## Mission

Establish the real long-lived repository foundation and boot contract. Do not create a disposable toy kernel.

The resulting x86_64 UEFI image must:

1. start in QEMU under UEFI;
2. obtain and record firmware memory-map and framebuffer information;
3. load a deterministic immutable boot capsule before firmware boot services end;
4. transfer control through a versioned standalone boot ABI;
5. initialize structured serial diagnostics;
6. parse and validate capsule v1 using bounded borrowed slices;
7. locate UTF-8 canonical text `/system/boot/init.tos`;
8. print capsule source-commit identity, file hash and first logical source line;
9. halt cleanly with stable result codes;
10. emit a machine-readable Stage 1 identity record binding the boot result to the source commit and capsule digest.

This task does not yet implement the TOS language, Git traversal, persistent storage or a shell. It must establish final-quality boundaries for them.

## Before writing code

Read all required files in `AGENTS.md`, especially `docs/34_THREAT_MODEL.md`, `docs/37_STAGE_IDENTITY_GATES.md` and `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`. Then produce a written architecture impact statement. This task is expected to be Level 2 because it establishes versioned contracts but must not amend invariants.

Before an official build, confirm the directory is a real Git repository with a non-placeholder baseline commit. If `.git` is absent, initialize the repository and create a DCO-signed architecture-baseline commit before producing official source-commit provenance.

Do not start implementation if a required contract is missing. Add a proposed ADR or specification amendment first rather than inventing an undocumented format in code.

## Required repository structure

```text
boot/uefi-loader/
nucleus/
crates/boot-protocol/
crates/capsule/
crates/tos-hash/
host-tools/capsule/
host-tools/qemu-test/
system/boot/init.tos
interfaces/boot/
tests/vectors/capsule-v1/
tests/integration/
tests/architecture/
scripts/
legal/release-manifests/
tools/
```

Refine names only with a simultaneous update to `docs/17_REPOSITORY_LAYOUT.md`.

## Licensing from the first commit

- loader, nucleus, capsule implementation and official host implementation tools: `GPL-3.0-or-later`, unless an accepted ADR identifies a separable Apache interface library;
- versioned public ABI/schema definitions and reusable conformance vectors under `interfaces/`: `Apache-2.0`;
- documentation: `CC-BY-SA-4.0`;
- every source file has an SPDX identifier;
- commits intended for merge have DCO sign-off;
- add no third-party dependency without recording exact licence and role.

Do not copy Linux boot or driver implementation code.

## Boot protocol requirements

The standalone `no_std` boot-protocol crate defines:

- magic and protocol UUID;
- major/minor version;
- total structure size;
- architecture identifier;
- memory range descriptors with type and flags;
- framebuffer descriptor;
- capsule physical range and digest;
- capsule source-commit field or bounded identity descriptor;
- firmware/acpi pointers as explicitly typed optional records;
- checksum/digest policy;
- forward-compatible record iteration or reserved-field policy.

No Rust layout is treated as a stable ABI without explicit representation, endian, alignment and size rules.

## Capsule v1 requirements

Capsule v1 is a deterministic read-only archive with:

- magic, format UUID and version;
- explicit little-endian encoding;
- total length and bounded counts;
- path table and file metadata;
- normalized absolute UTF-8 paths;
- per-file digest;
- whole-capsule digest;
- source commit identity;
- architecture-spec version;
- builder identity field;
- licence-notice-set digest;
- exact alignment and maximum-size rules.

Reject duplicate paths, traversal, NUL bytes, invalid UTF-8, ambiguous normalization, integer overflow, overlap, unsorted canonical tables if sorting is required, truncated payloads and digest mismatch.

The format is specified before code and has golden valid/invalid vectors.

## Provenance requirement

The host capsule builder writes a manifest containing:

```text
source_commit
architecture_spec_version
builder_version
boot_protocol_version
capsule_format_version
material_path_and_hash_list
output_digest
licence_identifiers
```

A placeholder all-zero commit is forbidden for a claimed official artifact. Development builds may use an explicit `detached-source-set` identity with source hashes.

## Dependency constraints

Prefer zero external dependencies in the nucleus and boot ABI. Host-side crates may use reviewed dependencies, but the parser shared with target code must remain auditable and fuzzable.

Command-line Git, libgit2, Lua, Wasm runtimes and general filesystems are outside this task.

## Tests

Required:

- host unit tests for encoding/decoding;
- property tests for bounds and overflow;
- fuzz target for arbitrary capsule bytes;
- golden valid and invalid vectors;
- QEMU success test checking stable event IDs;
- QEMU corrupted-capsule tests;
- architecture test proving `init.tos` content printed by nucleus matches the canonical capsule input hash;
- Stage 1 identity test proving capsule `source_commit` exists in the repository and contains the declared source path;
- documentation-integrity test running `python3 tools/build-specification.py --check`;
- SPDX and DCO checks;
- deterministic builder test producing identical bytes twice.

## Stable diagnostic events

Define event identifiers such as:

```text
TOS.BOOT.ENTRY
TOS.BOOT.ABI_OK
TOS.CAPSULE.VALID
TOS.CAPSULE.INVALID
TOS.SOURCE.INIT_FOUND
TOS.BOOT.HALT_OK
TOS.PANIC
```

Human text may evolve; tests match identifiers and structured fields.

## Explicit non-goals

- no TOS Core interpreter;
- no fake command parser called a language;
- no Git object parser;
- no disk driver;
- no allocator unless design evidence requires it;
- no graphics UI;
- no network;
- no “temporary” kernel driver framework;
- no claim that Stage 1 is an MVP.

## Completion report

Report:

1. architecture impact statement;
2. repository tree;
3. formats and versions established;
4. dependencies and licences;
5. tests and exact commands;
6. QEMU logs for success and corruption cases;
7. provenance manifest example;
8. invariants exercised;
9. Stage 1 identity-gate report and artifact path;
10. threat-model entries exercised and evidence level;
11. remaining risks;
12. confirmation that no later-stage subsystem was mocked and called complete.
