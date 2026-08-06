<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# External implementation and oracle policy

## Motivation

TOS should benefit from decades of existing engineering without allowing mature external implementations to define its architecture accidentally.

## Roles

### Reference specification

A document or standard used to understand behavior. It contributes no code automatically.

### Test oracle

An implementation used to produce expected outputs or compare behavior. Disagreement is investigated; the oracle is not assumed correct for TOS semantics.

### Host tool

Runs on the developer OS to build, inspect or test artifacts. The release records it, but TOS restoration must not secretly depend on it forever.

### Isolated service

Runs outside the nucleus behind a versioned capability contract. Failure cannot compromise the nucleus directly.

### Trusted dependency

Runs in the loader, nucleus or verifier. This role is exceptional and requires ADR approval.

## Examples

- command-line Git: host tool and repository behavior oracle;
- libgit2: host library or oracle, not default nucleus dependency;
- Wasm engine: candidate execution backend or isolated service, not canonical source format by default;
- Lua: possible secondary language frontend, not automatic TOS Core replacement;
- QEMU: platform and test environment, not a runtime component;
- Linux drivers: research reference and source of specification links, subject to licence restrictions;
- seL4: capability and verification research reference, not a claim that TOS inherits seL4 proofs.

## Promotion procedure

Moving a dependency to a more trusted role requires:

- architecture impact statement;
- licence and patent review;
- transitive dependency analysis;
- resource and failure bounds;
- replacement and recovery plan;
- tests demonstrating conformance;
- accepted ADR.
