<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0006: Rust for loader, nucleus, and host foundation

- Status: Accepted
- Date: 2026-08-05

## Context

The binary foundation requires low-level control, `no_std` support, strong tooling, and reduced accidental memory unsafety.

## Decision

The initial loader, nucleus, shared format libraries, and host tools use stable Rust pinned by repository configuration. The architecture remains language-neutral at external boundaries.

## Consequences

- Unsafe code is isolated and documented.
- Shared parsers can run on host and target.
- Toolchain pinning and reproducible-build work are mandatory.
- Changing the foundation language requires a superseding ADR.
