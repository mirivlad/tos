<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0019: Capsule vector provenance and mixed-material containers

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — extends the versioned provenance/release contract
  and its gate without changing a runtime ABI, capsule byte format, trusted
  base or active invariant
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The committed capsule-v1 `.bin` fixtures are generated artifacts. The valid
fixture embeds official GPL-3.0-or-later boot source and a licence notice, while
the fixture set may also contain materials from other licence classes. The
existing SPDX gate exempts all `source/tests/vectors/capsule-v1/*.bin` files
because they are indexed by `vectors.tsv`; that proves neither a licence choice
nor provenance for the binary.

ADR-0010 and the release/provenance policy require a derived artifact to retain
its source relationship. `LICENSE.md` permits reusable test vectors under
Apache-2.0 only when explicitly marked, and requires generated artifacts to
record the licences of their canonical sources without removing notices. No
existing authority selects a single SPDX expression for a mixed-material binary
container. ADR-0018 also requires regenerated detached fixtures to wait for an
accepted F-22 provenance treatment.

## Decision

### Mixed-material generated artifact classification

A capsule-v1 binary fixture that contains materials from different licence
classes is classified as:

```text
mixed-material-generated
```

This is an artifact/provenance classification, **not** an SPDX licence
identifier or expression. Such a `.bin` MUST NOT be assigned one container-wide
SPDX expression merely from its path, generator, an extension exemption or the
licence of one embedded input. It does not assert that the container is an
Apache-2.0, GPL-3.0-or-later, or otherwise homogeneous work.

The authoritative machine-readable representation is:

```json
"container_licensing": {
  "status": "mixed-material-generated",
  "spdx_expression": null
}
```

`mixed-material-generated` MUST NOT appear in a field that expects a valid SPDX
expression. The absence of a container-level SPDX expression does not remove
any obligation attached to embedded materials.

### Required provenance record and gate

Every tracked capsule-v1 fixture MUST have a valid entry in one
machine-verifiable provenance manifest. For each canonical or generated input,
the entry MUST state its digest, role and applicable SPDX identifier. It MUST
also state the fixture filename/output digest, generated-artifact status,
generator identity/version and generator licence as generator provenance.
Generator licensing is not automatically inherited by the output artifact.

Embedded licence notices MUST be retained in the fixture and listed with a
separate notice role. A derived invalid fixture MUST additionally identify its
base vector, base digest and deterministic transformation recipe.

The SPDX/provenance gate MUST reject a tracked capsule-v1 `.bin` that lacks a
valid provenance entry. The existing broad `*.bin` exemption is therefore not
an adequate final gate and must be replaced by the manifest validation when the
record is introduced.

If exact historic provenance of an existing binary cannot be demonstrated, the
record MUST mark it with an explicitly defined `unverifiable-legacy` status; it
MUST NOT invent a source commit, material digest or generator claim. Such an
artifact must be replaced reproducibly from a known source commit before it is
used as current Stage 1 conformance evidence.

### Reusable synthetic vectors

An Apache-2.0 reusable synthetic conformance-vector class may be introduced in
the future only as a separate, explicitly designated class containing
Apache-eligible synthetic materials. That future class does not reclassify the
current boot-material fixtures and is not required by this ADR.

## Consequences

- The vector provenance manifest and its checker become required Stage 1
  evidence before tracked binary-vector regeneration, including ADR-0018's
  affected detached fixtures and the SHA-1-padding negative fixture.
- The project records source/material obligations truthfully without assigning
  a false blanket licence to a mixed-material container.
- Existing fixture documentation, outcome tables and gate comments must be
  reconciled with this decision; their own text-file SPDX identifiers remain
  independent of the container classification.
- A binary whose provenance is only historical inference is not silently
  upgraded to verified evidence.

## Architecture impact statement

- **Invariants and canonical representation:** no invariant changes. The
  canonical executable source remains textual; a fixture stays a disposable
  derivative with a mandatory source/material record.
- **Trusted base and source-to-runtime:** no runtime code, dependency, ABI or
  loader/nucleus trust boundary changes. The build/release evidence becomes
  more explicit.
- **Recovery and owner control:** no recovery, rollback or owner boot path
  changes; a fixture can be discarded and regenerated from recorded inputs.
- **Compatibility:** capsule v1 bytes and semantics are unchanged by this ADR.
  A later regeneration changes only those bytes required by independently
  accepted ADR-0018 or a documented vector recipe.
- **Threat, performance, licence and patent:** the gate prevents provenance
  concealment but adds no new runtime attack surface or measured path. It
  preserves licence notices and makes no patent-freedom claim.
- **Tests:** a deterministic manifest verifier must cover valid entries,
  missing entries, digest mismatches, missing input SPDX/notice roles,
  malformed mixed-material classification and derived-vector base/recipe
  requirements.
