<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0005: QEMU x86_64 UEFI is the first platform

- Status: Accepted
- Date: 2026-08-05

## Context

Supporting arbitrary physical hardware would consume the project in driver work before the TOS model is validated.

## Decision

The first platform is x86_64 under QEMU with UEFI, framebuffer/serial diagnostics, and VirtIO devices. Platform interfaces must remain architecture-neutral where feasible, but no unsupported hardware is simulated through fake success paths.

## Consequences

- Tests can be automated deterministically.
- Textual driver architecture is exercised with real virtual devices.
- Physical-hardware support is deferred without weakening core contracts.
