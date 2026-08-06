<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0010: Derived artifacts carry verifiable source provenance

- Status: Accepted
- Date: 2026-08-05

## Context

TOS permits boot images, bytecode and native caches, but they must not become anonymous canonical programs.

## Decision

Every executable derivative records canonical source hashes, source commit, runtime/compiler identity, target ABI, build/material digests and artifact digest. Boot capsules are reproducibly derived from repository inputs and name their source commit.

## Consequences

Artifact formats include provenance from version 1. Cache validation is part of runtime correctness. A binary without a valid source relationship is not an official TOS textual component.
