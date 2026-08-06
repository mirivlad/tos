<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0002: Text source is canonical

- Status: Accepted
- Date: 2026-08-05

## Context

The defining idea of TOS is that programs should remain visible as source rather than being replaced by opaque installed binaries.

## Decision

All non-nucleus executable components are canonically stored as human-readable source text. The runtime may generate IR, bytecode, native code, indexes, and snapshots, but these are disposable caches keyed to source identity and runtime inputs.

The nucleus source is also canonical, although a derived binary image is unavoidable for boot.

## Consequences

- Deleting caches must preserve functionality.
- System updates operate on source history.
- Running processes expose source identity.
- Binary-only native packages are not native TOS components.
