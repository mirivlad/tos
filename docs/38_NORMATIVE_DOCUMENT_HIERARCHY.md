<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Normative document hierarchy

## Purpose

TOS documentation is large enough that duplicated text can drift. This document defines authority, conflict resolution and generated views.

## Authority tiers

### Tier 0 — Project identity

- `docs/02_SYSTEM_INVARIANTS.md`

An active invariant overrides every lower-tier document. Changing an invariant requires the Level 4 process.

### Tier 1 — Accepted architectural decisions and preservation rules

- accepted files under `docs/adr/`;
- `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`;
- `docs/29_PROJECT_GOVERNANCE.md` for decision authority.

A later accepted ADR may supersede an earlier ADR explicitly. Silent contradiction is invalid.

### Tier 2 — Normative subsystem and policy specifications

Numbered documents under `docs/` that define boot, language, execution, repository, state, IPC, drivers, security, stages, testing, legal policy and release gates.

Accepted versioned interface contracts under `source/interfaces/` are also
Tier 2 only when all of the following are true:

- their status explicitly says `Accepted Tier 2 interface contract`;
- they are listed in `docs/SPECIFICATION_SOURCES.txt`;
- they explicitly reference this hierarchy; and
- they acknowledge Tier 0 invariant and accepted Tier 1 ADR precedence.

Listing a path in `docs/SPECIFICATION_SOURCES.txt` does not by itself grant
Tier 2 authority to any other listed material. Directory placement, generated
view inclusion and a contract's own “normative” claim are insufficient.

A subsystem document must conform to Tier 0 and Tier 1. Where two Tier 2 documents overlap, the more specific subsystem contract governs only if it cites the general document and does not violate higher tiers.

### Tier 3 — Root operational documents

`README.md`, `ARCHITECTURE.md`, `AGENTS.md`, `CODEX_START.md`, `CONTRIBUTING.md`, `SECURITY.md`, `GOVERNANCE.md`, `PATENTS.md`, `LICENSE.md` and similar entry points.

They summarize or operationalize normative sources. They do not silently amend them.

### Tier 4 — Research and explanatory material

Files under `docs/research/`, examples, evaluations and historical notes unless an ADR explicitly incorporates a result.

### Tier 5 — Generated views

`TOS_DEVELOPMENT_SPECIFICATION.md` and other generated bundles.

Generated views are never independent authority. Their purpose is transport, review and model ingestion.

## Consolidated specification rule

`TOS_DEVELOPMENT_SPECIFICATION.md`:

- is built only by `tools/build-specification.py`;
- takes ordered inputs from `docs/SPECIFICATION_SOURCES.txt`;
- contains a generated-file warning and source-manifest digest;
- must not be manually edited;
- is checked by CI for byte-for-byte reproducibility;
- is replaced whenever any listed source changes.

If a generated view differs from a source file, the source file governs and the generated view is stale.

## Conflict protocol

When a conflict is found:

1. stop implementation at the affected boundary;
2. identify the authority tier and exact passages;
3. open an architecture issue or ADR as required;
4. correct the lower-authority document or explicitly supersede the higher decision;
5. regenerate derived views;
6. add a test or lint rule if the conflict was mechanically detectable.

Agents must not resolve conflicts by choosing the easiest implementation.

## Amendment rules

- invariant changes: Level 4 identity amendment;
- accepted ADR changes: new superseding ADR, except spelling/link fixes;
- subsystem contract changes: Level 2 or 3 according to impact;
- generated view changes: never direct; regenerate from sources;
- research notes: may evolve but must not be cited as accepted architecture without promotion.

## Normative language

The terms **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **MAY** and **OPTIONAL** are normative when capitalized. Ordinary prose remains binding when clearly stated as an invariant, requirement, exit gate or accepted decision.

## Release check

A documentation release is invalid if:

- the generated consolidated specification is stale;
- a listed source is missing;
- an accepted ADR is absent from the source manifest;
- document version metadata disagrees;
- an unresolved normative conflict is known.
