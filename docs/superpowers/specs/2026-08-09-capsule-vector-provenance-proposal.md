<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Capsule v1 vector provenance and licensing proposal

Status: **proposal for Project Architect decision**. This document is
non-normative. It does not classify a binary container, designate the existing
set as reusable Apache material, alter the SPDX gate, or authorize tracked
binary-vector regeneration.

## Purpose and boundary

F-22 concerns the committed capsule-v1 `.bin` fixtures under
`source/tests/vectors/capsule-v1/`. They are generated artifacts, not source
files with a viable in-band SPDX comment convention. The accepted
ADR-0018 changes the valid detached fixture's identity, so its eventual
regeneration must wait until this set has a machine-verifiable provenance
record and an owner-approved treatment of its mixed-material binary container.

This proposal separates three distinct things which must not be conflated:

1. the format/conformance definition, expected outcomes and harness metadata;
2. a generated binary fixture; and
3. the canonical materials embedded in that fixture, including notices.

It covers the current 12 committed negative fixtures and `valid-001.bin`, as
well as a future derived SHA-1-padding fixture. It does not begin detached
builder/parser implementation, regenerate a vector, change a tracked binary,
or start Phase 2 or Stage 1.5.

## Authority and facts

| Requirement or evidence | Authority tier | Exact source | Consequence |
|---|---:|---|---|
| Operating-system implementation is GPL-3.0-or-later; reusable interface material is Apache-2.0 only when explicitly marked. | Tier 1 | `docs/adr/0007-licensing-model.md`, Decision | No existing directory location or fixture suffix can itself make a vector Apache. |
| Generated artifacts record verifiable source provenance. | Tier 1 | `docs/adr/0010-derived-artifact-provenance.md`, Decision | A committed fixture needs source relationship rather than an unexplained binary exemption. |
| A generated artifact's metadata links canonical source commit/hashes, builder/version, ABI/options, material/output digests; a capsule manifest includes included files and licence notices. | Tier 2 | `docs/28_RELEASE_PROVENANCE_AND_REPRODUCIBILITY.md`, “Artifact provenance” and “Boot capsule” | The vector record must carry those applicable fields. |
| Generated artifacts retain provenance/notices and CI must test that an artifact maps to canonical inputs. | Tier 2 | `docs/30_COMPLIANCE_AND_RELEASE_GATES.md`, “Licence gate”; `docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md`, “Licence and provenance tests” | An extension-wide `.bin` exemption is not sufficient evidence. |
| Test-vector parsers and independent inspection tools may be Apache only when explicitly designated. | Tier 2 | `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`, “Apache-2.0” | An Apache reusable boundary requires an explicit designation; it is not inferred. |
| Generated artifacts must carry provenance metadata naming licences of their canonical sources and must not remove notices. | Tier 3 operational policy | `LICENSE.md`, “File-level declarations” | Every embedded material and notice must be enumerated; a generated container cannot hide them. |
| Existing vector binaries are a recognised Stage 1 test set. | Tier 2 | `docs/15_TESTING_AND_VERIFICATION.md`, committed/versioned golden-vector requirement | The set needs a durable, versioned record, not an ad-hoc one-off exception. |
| The accepted detached identity change requires affected fixture bytes and evidence to be regenerated only after F-22 supplies an authoritative record. | Tier 1 | `docs/adr/0018-detached-capsule-source-identity.md`, “Compatibility and migration” and “Implementation boundary” | No detached fixture regeneration may precede the decision requested below. |

The authority hierarchy in `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md` controls
this reading. In particular, `LICENSE.md` is an operational Tier 3 licence map;
it does not silently amend ADR-0007 or Tier 2 requirements. The sources are
consistent: they require explicit designation for reusable Apache material and
complete provenance for a generated artifact, but none presently selects one
licence expression for a mixed-material capsule container.

### Current-set inventory

The following facts are directly observable in the tracked tree:

- `source/tests/vectors/gen/gen.sh` is GPL-3.0-or-later and creates
  `valid-001.bin` from `source/system/boot/init.tos`, a generated
  `/system/version` value, and `source/system/boot/NOTICES.txt` as the licence
  tail. The generator currently has no immutable input/provenance manifest.
- `source/system/boot/init.tos` and `source/system/boot/NOTICES.txt` are
  GPL-3.0-or-later. `valid-001.bin` therefore embeds GPL-class material and a
  notice set; it is not justified as a pure Apache fixture merely because it is
  a test vector.
- `source/tests/vectors/capsule-v1/README.md` is CC-BY-SA-4.0 prose. It says
  that binaries are generated from `system/boot/` and uses the superseded
  synthetic `0x42` detached identity. It neither designates the vector
  definition as Apache nor records material licences.
- `source/tests/vectors/capsule-v1/vectors.tsv` is GPL-3.0-or-later expected
  outcome data. Its header classifies that text file, not any `.bin` container,
  and it contains no provenance record.
- `docs/08_GIT_NATIVE_SYSTEM.md` says test vectors *may* be Apache-2.0. It is a
  permissive Tier 2 statement, not an explicit designation of this fixture set
  and not a binary-container licence rule.
- `scripts/check-spdx.sh` currently exempts
  `source/tests/vectors/capsule-v1/*.bin` because they are versioned fixtures
  indexed by `vectors.tsv`. Its own comment says the gate deliberately does not
  choose a material's licence. This exemption is not a provenance or licensing
  decision and must not justify a new binary.

These are incomplete declarations, not a licence conflict that an
implementation may resolve by guessing. They establish F-22: no current
machine-readable record maps every tracked fixture to canonical inputs,
licences, generator, source revision and output digest.

## What follows without further owner policy

The following requirements are determined by the existing authority and do not
need a new architectural licence choice:

1. A newly committed or regenerated `.bin` must have a checked provenance
   record before it joins the vector set. The record must identify its output
   digest, generator, source revision or detached source declaration,
   canonical inputs/material digests, each material's SPDX identifier and
   generated-artifact status.
2. The record must preserve the role and digest of an embedded licence notice;
   it cannot replace, omit or obscure that notice with a container label.
3. A derived invalid vector must state both its base vector (including base
   digest) and an exact transformation recipe. Naming only the source inputs is
   insufficient for an invalid binary patched from a valid base.
4. The existing binary set is **not** explicitly designated as reusable
   Apache-2.0 material. An eventual reusable conformance definition/harness
   metadata boundary may be designated Apache only explicitly and separately.
   The current GPL generator need not be relabelled to create that boundary.
5. A standalone GPL-3.0-or-later or Apache-2.0 classification for the existing
   binary container is **not** implied. The binary must not receive a blanket
   SPDX identifier solely from its path, generator, or an extension exemption.

## Proposed machine-verifiable record

After the owner decision, add one tracked set-level manifest at
`source/tests/vectors/capsule-v1/provenance.json` and a focused verifier invoked
by the SPDX/provenance gate. The file name is a proposal, not an accepted
interface. The record is deliberately separate from the binary: JSON has no
reliable in-band SPDX comment syntax, and the manifest describes a generated
artifact rather than claiming that the container's bytes are homogeneous source
code.

The set-level object has `format: "tos-capsule-vector-provenance-v1"`, a
manifest-schema version, and one entry per tracked `.bin`. A conforming entry
has this shape (shown with placeholders, not assertions about current historic
generation):

```json
{
  "format": "tos-capsule-vector-provenance-v1",
  "vector": "valid-001.bin",
  "sha256": "<64 lowercase hex>",
  "generated_artifact": true,
  "generator": {
    "path": "source/tests/vectors/gen/gen.sh",
    "version": 1,
    "source_commit": "<full Git OID of generator source>",
    "sha256": "<generator content SHA-256>"
  },
  "source_commit": {
    "kind": "git",
    "algorithm": "sha1",
    "value": "<full Git OID of canonical source tree>"
  },
  "inputs": [
    {
      "repository_path": "source/system/boot/init.tos",
      "capsule_path": "/system/boot/init.tos",
      "sha256": "<64 lowercase hex>",
      "spdx": ["GPL-3.0-or-later"],
      "role": "embedded canonical boot source"
    },
    {
      "repository_path": "source/system/boot/NOTICES.txt",
      "capsule_path": null,
      "sha256": "<64 lowercase hex>",
      "spdx": ["GPL-3.0-or-later"],
      "role": "embedded licence notice tail"
    }
  ],
  "container_licensing": {
    "status": "<owner-approved option>",
    "spdx_expression": "<only if an accepted policy supplies one>",
    "rationale": "<required policy reference>"
  },
  "derivation": null
}
```

For a detached capsule, `source_commit` instead records
`kind: "detached"`, the accepted identity algorithm/value, and the Git OID of
the builder source separately in `generator.source_commit`. This avoids
pretending that a detached source identity is a Git commit while retaining
reproducible builder provenance.

The actual manifest schema must also require an entry for generator-created
material such as `/system/version`: it must name its recipe, literal/derived
input, digest, licence provenance and capsule path. It is not acceptable to
write `0.2.1` into a temporary file and omit its origin from the record.

For a derived invalid vector, `derivation` replaces `null`, for example:

```json
{
  "base_vector": "valid-001.bin",
  "base_sha256": "<64 lowercase hex>",
  "transformation_recipe": {
    "kind": "sha1-oid-padding",
    "header_identity_algorithm": "sha1",
    "unused_tail_offset": 120,
    "unused_tail_byte": 0,
    "replacement_byte_hex": "01",
    "whole_capsule_digest": "recomputed after the header rewrite"
  }
}
```

The future SHA-1 padding scenario therefore identifies a particular valid base,
the exact non-zero unused-tail mutation, and digest recomputation. Other
existing invalid vectors must likewise state whether they are a canonical build
from listed inputs or a precise byte/layout transformation from a base. The
verifier must reject an untracked `.bin`, missing entry, wrong output digest,
missing input SPDX, absent notice role, unsupported `source_commit` form, or a
derived record with no base digest/recipe.

Historical Git introduction commits are not substitute build attestations. If
the exact canonical source commit for a current binary cannot be demonstrated,
the future record must say so through an explicitly defined historical status
and the fixture must be rebuilt only after owner-approved policy; it must never
invent the current `HEAD` as historical provenance. ADR-0018 already requires
the valid detached fixture to be regenerated, so the accepted continuation can
establish an honest source revision for the replacement set.

## Reusable-vector boundary

The project should explicitly decide whether capsule-v1 conformance definitions
are official reusable TOS vectors. If yes, the reusable boundary should contain
only the format/conformance schema, expected outcomes and independent harness
metadata, each explicitly marked Apache-2.0. It must be distinct from:

- the GPL generator which builds a capsule containing official boot material;
- the mixed-material generated `.bin` container; and
- the embedded GPL source/notices, whose licence/provenance remain enumerated
  in the record.

This designation would make the reusable definition useful to independent
implementations without asserting that a fixture containing GPL material is an
Apache-only binary. It is a policy/documentation follow-up after the immediate
container decision; it is not made by this proposal.

## Owner decision: binary-container classification

No existing Tier 1–3 source selects one of the following for the current
mixed-material `.bin` containers. The Project Architect must choose an explicit
policy before tracked regeneration. The options are deliberately concrete:

### Option A — mixed-material generated container, no standalone SPDX expression (**recommended**)

Adopt a narrowly scoped policy that a generated capsule fixture containing
materials from more than one licence class is recorded as
`mixed-material-generated`; it has no single SPDX expression in the binary
entry. The provenance manifest enumerates every embedded material, SPDX
identifier, notice and derivation. The SPDX gate replaces its broad `.bin`
exemption with a check that every binary has a valid manifest entry; it does not
ask the binary itself to contain an SPDX comment.

This is the most faithful reading of the current policy: it preserves GPL
materials/notices and does not fabricate either Apache-only or GPL-only
homogeneity. It also supports an independent Apache conformance-metadata
boundary later. Cost: downstream tooling must understand this explicit
mixed-material status rather than treating the binary as a standalone reusable
source file.

### Option B — GPL-3.0-or-later generated container, with complete material inventory

Adopt an explicit policy that an official generated capsule fixture embedding
GPL-3.0-or-later official system material is classified as
GPL-3.0-or-later, while the manifest still records all material licences and
notice roles. The gate would validate both that classification and the complete
provenance record.

This gives conventional SPDX tooling a simple container expression and makes
the reciprocal status visible. Cost: the rule is a new owner policy not implied
by the current documents; it can obscure that an artifact also includes
non-GPL-class materials or metadata and is less suitable as a reusable external
conformance artifact.

### Option C — a separate Apache-only synthetic conformance fixture class

Create a future, explicitly Apache-2.0 reusable fixture class built only from
Apache-eligible synthetic inputs and independent conformance metadata. Keep the
current boot-material fixtures under either Option A or Option B with their
full provenance.

This is useful for external parser interoperability, but it cannot classify or
retroactively cure the existing `valid-001.bin`: that file embeds GPL-class
boot material. It therefore adds maintenance and does **not** close F-22 unless
paired with A or B for the current fixture set.

**Recommendation:** choose **Option A** now, and separately decide whether to
adopt the Apache reusable-metadata boundary described above. It is the narrowest
policy change, keeps every licence notice visible, satisfies the provenance
requirements without a false blanket label, and lets a later Option C coexist
without reclassifying existing boot-material fixtures.

## Acceptance boundary and next action

This proposal deliberately leaves the following unchanged until an owner choice
is recorded:

- all tracked capsule `.bin` bytes and their SHA-256 values;
- `vectors.tsv`, the vector README and the current SPDX exemption;
- detached builder/parser semantics and the post-ADR-0018 golden-vector
  regeneration; and
- Phase 2 and Stage 1.5 work.

After an explicit owner choice, a separately scoped continuation can make the
provenance schema/checker authoritative, reconcile vector documentation and
licence declarations, regenerate affected vectors under ADR-0018, and add the
committed SHA-1 malformed-padding fixture with its complete derivation record.
