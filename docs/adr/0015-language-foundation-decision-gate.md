<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0015: Require a language-foundation decision gate before Stage 2

- Status: Accepted
- Date: 2026-08-06
- Decision level: 3 — architectural process and trusted-runtime boundary

## Context

TOS depends on a language/runtime relationship that ordinary embedded scripting languages do not automatically provide: canonical text, deterministic lowering, capability-aware types, bounded bootstrap execution, source maps, independent IR verification and suitability for user-space drivers.

The current documents name this role “TOS Core” and show illustrative syntax, but they do not yet define a normative grammar, semantics or memory model. Beginning parser implementation immediately would turn accidental early choices into architecture. Conversely, embedding a mature language for convenience could erase TOS identity while appearing pragmatic.

## Decision

A mandatory **Stage 1.5 — Language foundation decision** occurs after the trusted boot boundary is established and before Stage 2 parser/runtime implementation begins.

Stage 1.5 must:

1. use `docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md`;
2. compare at least a bespoke TOS Core option, a TOS surface over an existing formal core, and one adapted existing-language option;
3. produce executable or formal evidence for the required prototype exercises;
4. measure trusted-base, dependency, performance and recovery impact;
5. identify the canonical source, verifier boundary and host ABI exposure for every candidate;
6. end in a separate accepted ADR selecting the language foundation.

Until that selection ADR is accepted:

- `.tos` syntax remains illustrative;
- no parser implementation may be declared normative;
- no existing runtime may enter the trusted base as a temporary shortcut;
- Wasm or another bytecode may be researched as a backend, but cannot become canonical source by convenience.

## Consequences

Positive:

- the largest conceptual dependency is decided with evidence;
- a bespoke language is not assumed merely for originality;
- mature runtimes are evaluated without allowing architectural capture;
- Stage 2 begins with a stable contract rather than syntax experimentation.

Negative:

- Stage 2 starts later;
- comparison prototypes create work that may be discarded;
- the decision may reveal that earlier IR assumptions need revision.

The additional work is accepted because language-foundation mistakes would contaminate every later subsystem.
