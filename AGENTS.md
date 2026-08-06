<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Mandatory instructions for agents working on TOS

These instructions apply to Codex, Claude Code, Qwen-based agents, local models and any other automated contributor.

## 1. Understand the project before optimizing it

TOS is not a conventional kernel plus scripts. Its identity is the conjunction of:

- canonical human-readable installed source;
- commit-addressed `/system` identity;
- verifiable source-to-runtime provenance;
- disposable derived executable artifacts;
- explicit capabilities and user-space drivers;
- transactional activation and repository recovery;
- an owner-controlled path to boot modified source.

Do not “simplify” TOS into a familiar architecture.

## 2. Documentation authority

Read `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md` before relying on documentation.

`TOS_DEVELOPMENT_SPECIFICATION.md` is generated and non-normative. Never edit it directly. Change the individual source file, run `python3 tools/build-specification.py`, and include the regenerated file in the same change.

If two normative documents conflict, stop and report the conflict. Do not silently choose the more convenient text.

## 3. Required reading

Before planning or modifying code, read:

1. `README.md`;
2. `docs/02_SYSTEM_INVARIANTS.md`;
3. `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`;
4. `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`;
5. `docs/37_STAGE_IDENTITY_GATES.md`;
6. `docs/34_THREAT_MODEL.md`;
7. `docs/35_PERFORMANCE_CONTRACTS.md` when the task touches a measured path;
8. `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`;
9. `docs/27_THIRD_PARTY_COMPONENT_POLICY.md`;
10. the relevant subsystem document;
11. every accepted ADR touching the task.

For the first implementation task, read `CODEX_START.md` completely.

## 4. No MVP or disguised throwaway architecture

Do not:

- replace canonical text with canonical binaries “temporarily”;
- embed Linux/BSD as the real OS and place a TOS shell above it;
- put ordinary drivers in the nucleus because IPC is unfinished;
- make Git only a host-side development tool;
- adopt Lua, Wasm, libgit2 or another mature component as a runtime foundation without an ADR;
- implement a TOS Core parser before the Stage 1.5 language-foundation decision is accepted;
- design a recovery path requiring undocumented host tools;
- claim language or Git compatibility from superficial parsing;
- create a vendor-locked boot path in an official TOS profile;
- mark a stage complete around mocks, hard-coded success or unimplemented failure paths.

Narrow scope is allowed. False completion is not.

## 5. Architecture impact statement

Before any Level 2 or higher change, state:

- change level under `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`;
- invariants affected;
- canonical representation after the change;
- trusted-base impact;
- source-to-runtime impact;
- recovery and rollback impact;
- applicable stage identity gate and evidence;
- threat-model impact;
- applicable performance contract;
- declared compatibility profile;
- new dependencies and their role;
- licence and patent impact;
- tests that enforce the decision.

Do not implement a Level 3 or Level 4 decision before an ADR is accepted.

## 6. Licensing and provenance

- Use the exact SPDX identifier required by `LICENSE.md`.
- Do not copy publicly visible code without identifying its licence.
- Do not copy Linux GPL-2.0-only implementation code into GPL-3.0 TOS components.
- Hardware facts may be reimplemented from specifications; expressive code requires licence permission.
- Record imported dependencies and modifications.
- Preserve copyright and notice files.
- Generated and AI-assisted code has the same provenance requirements as human code.
- Every human-approved commit requires a DCO `Signed-off-by` trailer.

## 7. Patent discipline

Do not claim a mechanism is patent-free. For high-risk mechanisms listed in `PATENTS.md`, update the landscape or flag review before finalizing architecture. Do not deliberately reproduce a patent’s described claim combination because it seems convenient.

Do not publish confidential legal advice or respond substantively to a patent demand without maintainer direction.

## 8. Binary trusted base

The loader and nucleus remain `no_std`, narrowly scoped and dependency-conscious. A dependency entering them requires explicit justification. Parsers must be total over arbitrary bytes, return structured errors and never rely on panics for normal invalid input.

Unsafe Rust requires a local safety statement and focused tests. Public boundaries are versioned from their first commit.

## 9. Canonical source and artifacts

Every derived artifact must be traceable to source inputs, commit, builder/runtime version, target ABI and output digest. Cache invalidation is correctness, not optimization. Deleting caches must not delete system functionality.

The boot capsule is a reproducible transport and recovery seed, not a second hidden installed system.

## 10. Threat-model discipline

Treat all external bytes and all textual source outside a trusted commit as hostile until validated. A readable module is not a safe module.

Any new parser, DMA mapping path, protected-ref mutation, executable cache, remote transport, frontend or secret-bearing service requires explicit threat-model coverage and negative tests.

Do not treat owner-authorized code as vendor-trusted, but do not deny the owner the ability to run it through the documented research path.

## 11. Performance discipline

Do not replace measurements with adjectives such as “fast,” “zero-cost” or “line-rate.” Identify the metric, workload, baseline, environment and percentile.

A stage touching IPC, drivers, parsing, repository traversal or activation must produce the evidence required by `docs/35_PERFORMANCE_CONTRACTS.md`. A benchmark-only conventional implementation may be an oracle; it must not become the accepted runtime by convenience.

## 12. Tests and completion

A completed task report includes:

- files changed;
- tests added and exact commands run;
- architecture level and invariants satisfied;
- stage identity evidence;
- threat-model changes and negative tests;
- performance measurements where applicable;
- compatibility profile actually reached;
- dependency/licence/provenance changes;
- patent-risk notes where applicable;
- documentation and ADR changes;
- remaining limitations and risks;
- reproduction commands.

No `TODO`, fake return value, silent fallback, ignored error or hard-coded happy path may remain in code claimed complete for the stage.
