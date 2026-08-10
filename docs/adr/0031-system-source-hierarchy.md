<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0031: Runtime system source hierarchy

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 2 — extends existing namespace contracts with a normative
  runtime hierarchy without moving a trust boundary or changing an invariant
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

TOS documentation defines the root namespaces of a running system
(`docs/03_ARCHITECTURE_OVERVIEW.md`), their classes
(`docs/09_FILESYSTEM_AND_STATE.md`) and the layout of the development
repository (`docs/17_REPOSITORY_LAYOUT.md`). It does not define the inside of
`/system` on a running machine.

The gaps are concrete, and each one is already reachable from an accepted
contract:

- `docs/17` lists `system/{boot,services,drivers,languages,shell,ui,policy}` in
  the repository, but nothing states whether the runtime `/system` is that same
  tree, a transformation of it, or an unrelated structure. I-16 requires a
  running component to report its canonical source path, which is unanswerable
  while the mapping is undefined.
- `docs/04` names `/system/boot/init.tos` and `/system/boot/health.tos`,
  `docs/07` names `/system/languages/<name>/` and `docs/14` names
  `/system/drivers/virtio/block.tos`. These paths are used as facts by several
  documents without a document that defines them.
- `docs/13` requires that "the active system commit contains a lock manifest"
  without saying where it lives or whether it is canonical source or a cache.
- Shared libraries, applications, runtime-visible schemas, machine-specific
  source and imported third-party textual source have no stated location,
  although every one of them is implied by an accepted contract.
- `docs/09` classifies root namespaces informally. There is no single statement
  of which paths are canonical source, which are mutable state, which are
  derived cache and which are external material — the distinction that makes
  "deleting caches must not remove functionality" (I-01) mechanically testable.

Left undefined, each gap gets filled by whichever subsystem is implemented
first, and the resulting structure becomes architecture by accident.

## Decision

`docs/45_SYSTEM_SOURCE_HIERARCHY.md` becomes a Tier 2 normative contract
defining the runtime system source hierarchy. Its substance:

1. **Namespace classification.** Every runtime path belongs to exactly one of:
   canonical source, source overlay, configuration, mutable state, derived
   cache, ephemeral, capability namespace, external material. The class defines
   what deletion and rollback mean for that path.

2. **Repository-to-runtime mapping.** The repository subtree `source/system/` is
   the canonical input for the runtime `/system` tree, mapped directly and
   without renaming or generation. Repository directories outside
   `source/system/` are development material and are not installed as `/system`
   content.

3. **`/system` hierarchy.** Thirteen entries: `boot/`, `services/`, `drivers/`,
   `languages/`, `lib/`, `apps/`, `shell/`, `ui/`, `policy/`, `schemas/`,
   `machine/`, `third-party/`, `lock/`. Each is canonical source text. Seven of
   them are already named by `docs/17_REPOSITORY_LAYOUT.md`; the remaining six
   are the minimum needed to give an existing accepted requirement a defined
   location.

4. **Manifests stay in module source.** Component manifests are declared inside
   the module they describe, following `docs/11_DRIVER_MODEL.md`. No parallel
   manifest directory is introduced, because a separate manifest tree can drift
   from the code it describes.

5. **`/work` shape.** Overlays mirror `/system` paths, are never executed as
   system source without transactional activation, and are discardable.

6. **`/vendor` dependencies.** A component declares required vendor objects in
   its own manifest; `/system/lock/` aggregates the resolved set for the commit.
   Opaque bytes never appear in `/system`. Governed by ADR-0030.

7. **Lock manifests are canonical source, not cache.** They record resolution
   decisions that define the commit and cannot be regenerated identically at a
   later time, so they fail the derived-artifact test in I-01.

This ADR defines placement and classification only. Module resolution, manifest
schema, capability grammar, activation mechanics and storage format remain with
their existing owning contracts. No directory must exist before the stage that
implements the subsystem it serves.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-01 gains a mechanically testable
  boundary (canonical source versus derived cache per path); I-16 gains the
  mapping that makes a reported source path resolvable in the active commit;
  I-04 gains the explicit `/work`-to-`/system` relationship; I-09 gains a stated
  location for runtime-visible schema source.
- **Canonical representation after the change:** unchanged. `/system` remains
  canonical text; this decision says what is inside it.
- **Trusted-base impact:** none. No dependency enters the loader or nucleus and
  no trust boundary moves.
- **Source-to-runtime impact:** improved. The chain from reported source path to
  active-commit tree entry becomes resolvable rather than conventional.
- **Recovery and rollback impact:** unchanged mechanically. Classification makes
  rollback semantics explicit per class, and section 3 of docs/45 clarifies that
  `/system/lock/` rolls back with the commit while `/vendor` does not.
- **Stage identity gate:** no stage gate is claimed or closed.
- **Threat-model impact:** none directly. The classification supports existing
  properties S6 and S9 by making "derived" and "mutable" checkable per path
  rather than per subsystem convention.
- **Performance contract:** none applicable.
- **Compatibility profile:** none claimed.
- **New dependencies:** none. The decision is documentary.
- **Licence and patent impact:** none. `/system/third-party/` restates existing
  obligations from docs/22 and docs/27 rather than adding any.
- **Tests that enforce the decision:** deferred to the implementing stages, with
  required conformance expectations listed in docs/45 section 6 — no `/system`
  path resolving to cache, state or vendor content; `/cache` deletion behavior;
  reported source paths existing in the active commit; overlay paths unable to
  execute without activation; `/vendor` requirement sets enumerable from
  `/system/lock/`.

## Consequences

Subsystem work from Stage 3 onward has a defined place to put its source, and
the placement decisions are reviewable now rather than emerging from
implementation order. Architecture conformance tests gain a target they can
enforce mechanically.

The cost is that a hierarchy defined before most of its subsystems exist may
require revision. That is accepted: revising a stated contract through an ADR is
the visible path, whereas an unstated hierarchy is revised silently and without
review.

## Alternatives considered

**Extend `docs/09_FILESYSTEM_AND_STATE.md` instead of adding a document.**
Rejected: docs/09 is about why one Git repository cannot hold every changing
byte and how state is separated from source. The internal structure of the
canonical tree is a different subject and would dilute both.

**Extend `docs/17_REPOSITORY_LAYOUT.md`.** Rejected for the reason this ADR
exists: conflating the developer repository with the installed system is the
current source of ambiguity, and merging them into one document would preserve
it.

**Define nothing until Stage 3 needs it.** Rejected: the paths are already used
as facts by docs/04, docs/07, docs/13 and docs/14, so the hierarchy is being
relied upon before it is defined. Deferring means the first implementation
chooses for the architecture.

**Define a complete hierarchy including future subsystems.** Rejected: entries
would have no accepted contract behind them. Every entry in docs/45 section 3
traces to a requirement that already exists.
