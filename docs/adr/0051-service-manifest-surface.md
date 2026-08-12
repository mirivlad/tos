<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0051: What a service manifest is, and where it lives

- Status: **Proposed**
- Date: 2026-08-12
- Decision level: 2 — resolves a contradiction between accepted Tier 2 documents
  and fixes the Stage 3 manifest surface without changing TOS Core V1 or
  `tos-ir/v1`
- Project Architect approval: *(pending)*

## The contradiction, reported rather than resolved quietly

Three accepted documents disagree, and AGENTS.md §2 requires that be said out
loud rather than settled by picking the convenient one.

`docs/11_DRIVER_MODEL.md` shows a manifest as a syntactic block in TOS source:

```tos
manifest driver {
    matches pci(vendor: 0x1af4, device: [0x1000, 0x1041])
    requires { capability pci.configure ... }
    provides "net.adapter.v1"
    state none
    restart restartable
}
```

`docs/45_SYSTEM_SOURCE_HIERARCHY.md` points at that example as the normative
pattern: "Each component's manifest is declared inside its own module source, as
shown in `docs/11_DRIVER_MODEL.md`; TOS does not keep a parallel manifest
directory that could drift from the code it describes."

`docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`, accepted under ADR-0028, admits six
item forms — `resource`, `record`, `enum`, `const`, `fn`, `extern` — and no
`manifest`. The word is not reserved and the form does not parse. **No accepted
grammar admits the source docs/11 shows.**

So docs/11's block is not currently TOS Core. It is a sketch of a surface that
was never specified, and docs/45 elevated the sketch to a pattern.

## What the accepted contracts already carry

Before inventing a surface, it is worth reading what a verified module already
tells a launcher without being executed. `tos-ir/v1` puts all of this in the
module image:

| Manifest concern (docs/10) | Already in the accepted contract |
|---|---|
| module entry point | `Header.module_name`, exported `Signature`s |
| requested capabilities | `CapabilityImport { interface, binding, ty }`, plus `Header.capability_interface_digest` over the ordered list |
| required interfaces / startup dependencies | `Import`s and `Header.dependency_digest` |
| resource limits | `Header.resource_envelope` — the ten declared limits |
| declared effects per entry point | `Signature.effects`, "by interface path" |
| source identity | `Header.content_id`, `source_set`, `path`, `frontend_identity`, `profile` |

That is most of a manifest, it is verified rather than asserted, it is covered by
the module digest, and it cannot drift from the code because it *is* the code's
header. The residue is: what the component **offers**, and how the system
**supervises** it — `provides`, `restart`, `state`, health probes, shutdown
timeout, and (for Stage 4) device matching.

## Decision

**The Stage 3 service manifest is not a new syntactic object. The residue is
split by who has the authority to assert it.**

### 1. What a module needs stays in the module, in accepted V1 form

Capability requests, resources, imports, exports and declared effects are
already V1 source and already in the verified IR. They are never duplicated into
a manifest block; a second declaration of the same fact inside one file is the
drift docs/45 warns about, moved indoors.

### 2. What a module offers is a capability request, not a claim

A service does not declare `provides "net.adapter.v1"`. It requests the
authority to publish that interface, using the accepted capability-import form,
where the nominal capability type **is** the interface being published:

```tos
import capability net.adapter.V1Publisher as publisher;
```

The launcher reads `capability_imports` from the verified IR, sees exactly which
interface the module intends to publish, and grants or denies it under policy.
Denial produces the typed `CapabilityDenied` that docs/42 §2 already specifies.

This is not a workaround for a missing syntax; it is the stronger form.
docs/37's Stage 3 failure conditions include "textual manifest grants itself
authority", and a `provides` line that no one mediates is precisely that. Under
this decision, publishing an interface is an authority the system grants, and
the request for it is the declaration of intent — one fact, one place, one
mediator.

### 3. How a component is supervised is the supervisor's canonical text

Restart policy, health-probe schedule, shutdown timeout, state namespace and
restart-loop bounds are decisions *about* a component, not descriptions *of* it,
and the entity with authority to make them is the one with the capability to
launch it. They live in `/system/policy/` as canonical source keyed by module
name, exactly where docs/45 already places policy — "canonical text like any
other component; not a binary configuration database".

This is not the "parallel manifest directory" docs/45 forbids. That prohibition
is about a description of the code drifting from the code. Supervision policy is
not a description of the code: it is the system's decision, it is reviewed and
committed like any other source, and if it names a module that does not exist
the supervisor says so at activation.

### 4. docs/11 is corrected, not preserved as aspiration

`docs/11_DRIVER_MODEL.md`'s manifest block is re-marked as illustrating a
possible future surface that no accepted grammar admits, and its example is
restated in accepted V1 form for the parts Stage 3 fixes. docs/45's sentence is
narrowed to what remains true: what a component needs is declared inside its own
module source.

## What this deliberately leaves open

Device matching — docs/11's `matches pci(vendor:…, device:[…])` — is a Stage 4
question, and this ADR does not pre-decide it. It is a different problem:
matching is a query evaluated by a bus manager against hardware, not an
authority a launcher grants. Stage 4 may find it needs a surface; if so it will
be argued on Stage 4's evidence rather than smuggled in now.

Sugar remains available later. If, after real services exist, a `manifest` block
proves worth a grammar change, it can be added to a future language version as
sugar over the same accepted facts. Sugar over an established semantics is a
small decision; a semantics invented in grammar first is not.

## Consequence, and one measured finding it does not depend on

A launcher must be able to read a module's header, capability imports and
exports from the verified IR **without executing it**, because the capability
decision happens before the process starts. `tos-ir/v1` already supports this;
no schema change is required, which is the property that makes this decision
cheap.

A separate gap was measured while evaluating the alternatives and is recorded
here so it is not mistaken for a consequence of this decision. A module-level
`const` — an accepted V1 item form — is parsed and type-checked today and then
dropped: reading one from a function refuses at lowering with
`construct=unbound place`, and `tos-ir/v1`'s `Constant` is scalar-only with no
named module-constant table. An earlier candidate design put the manifest in a
record-valued `pub const`; it was rejected because making it work would have
required changing `tos-ir/v1`, which is a closed Stage 2 contract. The lowering
gap is real and belongs in Phase 1 as implementation of an accepted contract —
but this decision does not stand on it.

## Evidence required

- A launcher decides the full capability grant of a service from its verified
  module image alone, before that service's first instruction runs.
- A service whose publish capability is denied fails to start with
  `CapabilityDenied`, does not appear in the interface registry, and says so in
  the audit record.
- No component can publish an interface it did not request, and no request
  grants itself.
- A supervision policy naming an unknown module is refused at activation rather
  than at first failure.
- The corrected docs/11 example parses under the accepted V1 grammar. The
  current one does not, which is how this contradiction became visible.
