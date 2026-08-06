<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Third-party component policy

## Principle

TOS reuses knowledge aggressively and dependencies conservatively.

A mature external project can serve as:

- documentation source;
- behavior reference;
- test oracle;
- host-side build tool;
- isolated runtime service;
- trusted-base dependency.

These roles have radically different architectural and legal effects. Promotion to a more trusted role requires explicit review.

## Trusted-base admission

A dependency entering the loader, nucleus, bootstrap parser or verifier must satisfy:

- narrowly necessary function;
- maintainable source availability;
- compatible licence;
- bounded input behavior;
- no ambient system dependency;
- fuzzability and test vectors;
- documented unsafe code and transitive dependency tree;
- acceptable update and vulnerability process;
- ADR approval.

“Widely used” is not sufficient.

## Language runtimes

Lua, Scheme, Wasm and other runtimes are research candidates, not automatic bootstrap choices. Any candidate must be evaluated for:

- canonical source relationship;
- type and capability enforcement;
- deterministic parsing and validation;
- resource bounds;
- interrupt and driver suitability;
- trusted binary size;
- source-map and cache identity;
- licence and patent profile.

A language may be added as a frontend without replacing TOS Core.

## Git implementations

libgit2 and command-line Git are valuable host tools and behavior oracles. The nucleus initially needs only bounded object and tree verification. Clone, merge, pack, transport, authentication and garbage collection stay outside the nucleus.

## Driver sources

Porting means translating device knowledge into TOS contracts, not mechanically compiling a Linux source file. Record the source of register definitions and protocol behavior. Avoid incompatible code copying. Firmware redistribution is reviewed separately from driver source.

## Inventory

The implementation repository will maintain machine-readable dependency and licence inventories. Vendored source includes upstream metadata and patch series. Network downloads during reproducible release builds are prohibited unless content is cryptographically pinned and mirrored in the build manifest.
