<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0003: Minimal nucleus and user-space textual drivers

- Status: Accepted
- Date: 2026-08-05

## Context

Executing text drivers directly inside a monolithic kernel would enlarge the trusted base and let ordinary driver bugs corrupt the whole machine.

## Decision

TOS uses a minimal microkernel-like nucleus. Drivers run as isolated textual services by default and receive device-specific capabilities. Boot-critical driver source is delivered through the boot capsule.

## Consequences

- IPC and capability performance are important.
- Driver restart and device reset become standard mechanisms.
- A small number of platform primitives remain in the nucleus.
- Ported driver logic must be adapted to TOS service contracts.
