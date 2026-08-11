<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0038: TOS Core V1 module-root precedence and the exact `E1605` condition

- Status: **Proposed** — needs Project Architect approval to become Accepted
- Date: 2026-08-11
- Decision level: 2 — fixes a stable diagnostic condition and the resolution
  rule conformance evidence depends on
- Project Architect approval: *(pending)*

## Context

`docs/42` section 1 gives module resolution "a declared ordered list of module
roots and dependency source-set identities", and says "a missing or ambiguous
import is `E1604_IMPORT_NOT_FOUND` or `E1605_AMBIGUOUS_IMPORT`".

Those two sentences disagree. If the root list is ordered and the first match
wins, no import matching under several roots is ambiguous — the order decides —
and `E1605` names a condition the rule prevents. If instead more than one
candidate is ambiguous regardless of order, the order is doing something else,
and the document does not say what.

The implementation stopped at that boundary: it reports `E1605` only for the one
case it can decide without choosing between the readings — a declared source set
holding the same module name twice, where nothing in the input decides at all.

## Decision

### 1. The order is a search order, and shadowing is not silent

The declared list of module roots is searched in order. The **first** root that
declares a module name resolves that name. That makes resolution deterministic
and total.

Ordering is not permission to shadow silently. A name declared by more than one
root is `E1605_AMBIGUOUS_IMPORT` **when more than one of the roots that declare
it is reachable from the importing module's declared dependency set**. A root
that the importer does not depend on is not a candidate and does not make
anything ambiguous.

The two sentences are reconciled this way: the order makes resolution decidable
for the ordinary case of a private root layered over a shared one, and the code
covers the case where two *declared dependencies* both offer the name, which is
a configuration mistake no ordering should paper over.

### 2. The exact condition

`E1605_AMBIGUOUS_IMPORT` is reported when either holds:

1. the declared source set contains more than one module with the requested
   name, and nothing in the set orders them; or
2. more than one declared module root reachable from the importer declares the
   requested name.

The diagnostic carries the requested import, the importer, the number of
candidates, and — when the roots are known — their ordered identities.

`E1604_IMPORT_NOT_FOUND` remains the case of no candidate at all. A missing
import takes precedence over an ambiguous one only when there is genuinely no
candidate; the two conditions are disjoint.

### 3. What resolution may read

Unchanged and restated because it bounds this rule: only the declared roots,
declared dependency source-set identities, the importer's own header, the
declared lock or manifest, and the effective import limit. Never an ambient
directory, the host filesystem outside those roots, the network, the clock, a
random source or an undeclared environment variable.

### 4. Conformance evidence

At least: a positive where a private root shadows a shared one and the first
root wins; a negative where two reachable roots declare the same name; and the
existing unit case where one source set holds a name twice. The first two need a
root-list input, so they are driver-level vectors rather than single files, and
the expectations table records them as such.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-15 is served by
  replacing two sentences that disagree with one rule.
- **Canonical representation:** unchanged.
- **Threat-model impact:** positive: `docs/44` section 3 requires an
  import-ambiguity negative, and this makes it precise.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the three cases above plus the mechanical gate binding the code to
  the registry.

## Consequences

`E1605` stops being a code whose condition the resolution rule prevents. A
layered root list — the ordinary way to override one module of a shared set —
keeps working, and a genuine collision between two declared dependencies is
named instead of silently decided.

## Alternatives considered

**First root always wins, `E1605` never fires.** Rejected: it makes an allocated
code unreachable and turns a dependency collision into a silent choice.

**Any multiple match is ambiguous, order is irrelevant.** Rejected: it breaks
layering, which is what an *ordered* list is for, and the document says ordered.

**Leave it to the compilation driver.** Rejected: resolution determinism is a
language property under `docs/42`, not a tool preference.
