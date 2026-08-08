<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0020: Versioned interface-contract authority and Boot ABI v1 events

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — accepts existing versioned public contracts and
  diagnostic vocabulary without changing a Tier 0 invariant, runtime trust
  boundary, persistent byte layout or implementation behavior
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The boot ABI and capsule format have implemented, versioned byte contracts and
conformance evidence, but their files live under `source/interfaces/`, outside
the classes assigned a tier by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.
Their old self-description as both “proposed” and “normative” was therefore not
an authority source (F-08). The same unassigned Boot ABI draft named diagnostic
events that the loader, nucleus and QEMU harness do not emit (F-13).

`docs/17_REPOSITORY_LAYOUT.md` already assigns independent interface
definitions and conformance vectors to `interfaces/`. The project's real
success and failure traces have been QEMU-checked since Stage 1 implementation;
changing them merely to preserve unused draft spellings would break that
evidence without strengthening an invariant.

## Decision

### Authority admission rule

`source/interfaces/**` gains Tier 2 authority only for a **versioned interface
contract** that satisfies every condition below:

1. it has the exact explicit status `Accepted Tier 2 interface contract`;
2. it is listed in `docs/SPECIFICATION_SOURCES.txt` and therefore carried in
   the generated review view;
3. it explicitly refers to `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; and
4. it states and observes its subordination to Tier 0 invariants and accepted
   Tier 1 ADRs.

Inclusion in `docs/SPECIFICATION_SOURCES.txt` alone does not assign Tier 2
authority to any other listed material. A new versioned source-interface
contract must meet all four conditions before it is normative; a prose report,
fixture, implementation note or other source-interface file is not promoted by
directory placement or generated-spec inclusion.

`BOOT_ABI_V1.md` and `CAPSULE_FORMAT_V1.md` meet this rule when this ADR is
implemented. They are Tier 2 contracts, subordinate to the invariant set and
to ADR-0016 through ADR-0019 where those ADRs decide their subject matter.
Future incompatible versions use a new versioned contract and the normal
Tier 1/2 conflict protocol; a contract cannot supersede an accepted ADR by
self-description.

### Boot ABI v1 event vocabulary

The current emitted identifiers are the canonical stable Boot ABI v1 serial
event vocabulary. The success order is exactly:

```text
TOS.BOOT.ENTRY
TOS.CAPSULE.OK          # loader validation
TOS.BOOT.HANDOFF
TOS.NUCLEUS.ENTRY
TOS.CAPSULE.OK          # nucleus independent validation
TOS.BOOTTEXT.PATH
[TOS.BOOTTEXT.LINE]     # optional
TOS.BOOTTEXT.DIGEST
TOS.IDENTITY
TOS.HALT
```

The stable failure vocabulary is:

```text
TOS.BOOT.FAILC
TOS.BOOT.FAILI
TOS.ABI.FAIL
TOS.MEM.FAIL
TOS.CAPSULE.FAIL
TOS.IDENTITY.MISMATCH
TOS.PANIC
```

`TOS.BOOT.FAILI` is itself stable. Existing reason tokens retain their defined
meaning; Boot ABI v1 may add a new reason token but must not repurpose an
existing one. Mandatory structured fields and raw payload grammars are fixed
in `BOOT_ABI_V1.md` §7. An implementation may append optional `key=value`
fields to an event only after its mandatory fields, so a parser that accepts
the v1 mandatory prefix remains compatible.

`TOS.IDENTITY` fixes the exact required field spellings and semantics:
`source_kind=`, `source_digest=`, `capsule_digest=`, `arch=`, and `builder=`.
The previous unimplemented draft names `TOS.CAPSULE.VALID`,
`TOS.SOURCE.INIT_FOUND` and `TOS.BOOT.HALT_OK` are not Boot ABI v1 events.

## Consequences

- The public ABI is now discovered through the same hierarchy as other Tier 2
  contracts, while retaining Tier 0 and Tier 1 precedence.
- The loader, nucleus and QEMU harness retain their emitted behavior; this
  ADR changes the contract to the verified implementation rather than creating
  a compatibility-breaking rename.
- A machine-checkable authority admission test and event-contract test guard
  both documents, required fields, success cardinality/order and QEMU gate
  wiring. Full preflight retains real QEMU success and negative execution.

## Architecture impact statement

- **Invariants and canonical representation:** I-09 versioned boundaries is
  made enforceable; no invariant or capsule/BootInfo byte representation
  changes.
- **Trusted base and source-to-runtime:** no loader, nucleus dependency,
  privilege or source-identity behavior changes.
- **Recovery, rollback and compatibility:** no boot-control or recovery path
  changes; existing Boot ABI v1 QEMU consumers retain their identifiers and
  mandatory field prefixes.
- **Threat and performance:** stable fail-closed identifiers improve audit
  evidence without adding an input path, runtime work or performance budget.
- **Licence and patent:** interface contracts remain Apache-2.0 as declared;
  no imported code, licence boundary or patent claim changes.
- **Tests:** authority admission, static event conformance, QEMU success and
  negative-suite evidence are required before Stage 1 closure can rely on the
  contracts.
