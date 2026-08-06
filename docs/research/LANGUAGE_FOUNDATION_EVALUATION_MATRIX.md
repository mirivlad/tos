<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Language foundation evaluation matrix

**Status:** non-normative research template required by ADR-0015.

## Decision to be made

Choose the foundation that will implement the TOS bootstrap profile and support the long-term TOS Core role without making a hidden binary/runtime ecosystem the true operating-system contract.

Candidate classes:

- **A — Bespoke TOS Core:** grammar, type system, IR lowering and reference runtime designed for TOS.
- **B — TOS surface over an existing formal core:** TOS source remains canonical while a rigorously specified lower core provides execution semantics.
- **C — Adapted existing language:** an existing language is restricted or extended to satisfy TOS contracts.
- **D — Existing language unchanged:** accepted only if it satisfies all blocking requirements without semantic fiction.

## Blocking requirements

A candidate is rejected if it cannot demonstrate:

1. canonical human-readable source remains authoritative;
2. deterministic parse and lowering from declared inputs;
3. bounded bootstrap implementation and resource accounting;
4. explicit capability imports that cannot be forged by ordinary code;
5. typed memory/region model suitable for services and drivers;
6. source maps through every derived stage;
7. independent verification before execution;
8. no ambient host filesystem/network/time access during lowering;
9. no undocumented C/host ABI becoming the real system ABI;
10. compatible licence and acceptable patent/dependency profile;
11. recovery implementation small enough to audit and fuzz;
12. multiple execution backends cannot disagree silently on semantics.

## Comparative criteria

For each candidate record evidence, not adjectives:

- normative specification size and maturity;
- trusted implementation size and transitive dependencies;
- parser/type-checker/verifier complexity;
- memory safety and unsafe boundary;
- concurrency semantics;
- deterministic behavior;
- interrupt/IPC/DMA expression;
- resource metering/preemption;
- diagnostics and source maps;
- boot-profile reducibility;
- frontend extensibility;
- performance profile;
- self-hosting path;
- tool support value versus architectural cost;
- licensing and contribution compatibility;
- implementation and maintenance effort.

## Required prototype exercises

Each serious candidate must implement or model the same exercises:

1. parse a malformed module corpus with stable diagnostics;
2. declare and enforce a PCI/MMIO/IRQ/DMA capability set;
3. lower a small block-driver state machine into typed IR;
4. reject an undeclared privileged operation;
5. enforce a bounded loop/fuel policy in bootstrap mode;
6. produce source maps through an optimized execution path;
7. invalidate a cache after one source/dependency change;
8. run the same semantic conformance vectors in two engines or interpreter modes;
9. build in a documented recovery-sized configuration;
10. report trusted-base and dependency inventory.

## Decision output

The Stage 1.5 report must contain:

- candidates evaluated;
- evidence repository/commits;
- blocking failures;
- measured results;
- trusted-base comparison;
- language and IR boundary;
- selected option;
- rejected alternatives;
- migration consequences;
- accepted selection ADR.

This matrix does not presuppose that a bespoke language wins. It prevents convenience from masquerading as architecture.
