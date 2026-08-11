<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0038: TOS Core V1 module-root precedence and the exact `E1605` condition

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — fixes a stable diagnostic condition and the resolution
  rule conformance evidence depends on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

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

Ordering settles roots, and only roots. It is not permission to paper over a
collision between *declared dependencies*: a name offered by more than one
reachable declared dependency source set is `E1605_AMBIGUOUS_IMPORT`, because
nothing orders dependencies against each other and choosing one would be an
implementation preference rather than a resolution rule.

The two sentences of docs/42 are reconciled this way. The order makes resolution
decidable for the ordinary case — a private root layered over a shared one — and
the code covers the case the order says nothing about.

### 2. The exact condition

`E1605_AMBIGUOUS_IMPORT` is reported when either holds:

1. the declared source set contains more than one module with the requested
   name inside one root, so nothing in the set orders them; or
2. more than one reachable declared dependency source set provides the
   requested name.

Otherwise the candidate in the earliest declared root resolves the name.

The diagnostic carries the requested import, the importer, the number of
candidates, and the identities that collided — the root for case 1, the
dependency source sets for case 2 — so the configuration mistake is nameable
without re-deriving it.

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
