<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0011: External implementations default to references or test oracles

- Status: Accepted
- Date: 2026-08-05

## Context

Existing Git libraries, language runtimes and driver frameworks can accelerate work but may import incompatible trust, source and capability models.

## Decision

External implementations default to specification references, host tools or test oracles. Admission as an isolated runtime service or trusted-base dependency requires a separate ADR, licence review, transitive-dependency audit and architecture impact statement.

## Consequences

libgit2, command-line Git, Lua, Wasm engines and Linux driver implementations are not silently adopted into the nucleus or canonical runtime.
