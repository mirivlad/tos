<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 Phase 1 capsule identity design

Status: owner-approved architecture scope on 2026-08-08. This design does not
accept the Level 3 detached-identity decision; implementation of that part
waits for a separately accepted ADR.

## Goal

Close the Stage 1 capsule-identity findings that have accepted authority while
preserving the single loader/nucleus/QEMU path. The work is limited to capsule
identity and its evidence. It does not start Stage 1.5 or later closure queues.

## Authority and change classification

`docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md` does not give
`source/interfaces/boot/*.md` an independent authority tier. Their statement
that they are normative does not resolve F-08. Phase 1 therefore uses only the
following accepted authority at the affected boundaries:

- ADR-0017 is Tier 1 and controls alignment. Its Decision 1 permits unaligned
  `content_offset` and requires byte-wise decoding. Removing “misaligned” from
  `CAPSULE_FORMAT_V1.md` §9 rule 16 is a Level 0 reconciliation of a lower
  interface draft with that decision, not a parser semantic change.
- ADR-0016 is Tier 1 for raw Git OID representation. Its left-aligned,
  zero-padded SHA-1 OID representation makes rejection of a non-zero unused
  12-byte tail a Level 1 conformance fix.
- The detached formula currently written in `CAPSULE_FORMAT_V1.md` §6 has no
  independent tier. Adopting it changes source-identity semantics and fixture
  bytes, so it is Level 3 and waits for an accepted ADR plus Project Architect
  approval.
- A build-gated BootInfo corruption scenario is Level 1 test evidence only. It
  has no production/default-loader behavior, ABI, capsule format or second boot
  path.

No invariant is amended. No dependency enters the trusted base. Canonical
installed text, G0 scope, recovery/rollback and owner control remain unchanged.

## Required evidence order

1. Add RED conformance tests without altering production behavior:
   - an explicit canonical unaligned-content assertion and a documentation
     consistency test that initially finds rule 16's contradiction;
   - a SHA-1 identity whose byte 20 through 31 tail contains a non-zero byte;
   - detached identity computation and a corrupted detached identity;
   - a real QEMU scenario for a corrupted BootInfo identity mirror.
2. Reconcile the alignment wording under ADR-0017.
3. Add the SHA-1 structured error and parser rejection, then an invalid vector
   and existing negative-QEMU evidence (exit 67 before `TOS.NUCLEUS.ENTRY`).
4. Draft the detached identity ADR and stop for architect acceptance before its
   implementation or vector regeneration.
5. Prepare a provenance/licensing proposal for the vector set and stop rather
   than assigning a blanket binary licence without existing-policy authority.

## SHA-1 padding contract

For `SRC_KIND_GIT`, SHA-1 uses bytes 100 through 119 for the raw 20-byte OID.
Bytes 120 through 131 are the unused portion of the 32-byte identity region and
MUST be zero. The parser returns a dedicated stable `CapsError` when any such
byte is non-zero. SHA-256 OIDs occupy all 32 bytes and have no tail.

The committed malformed-padding vector recomputes `whole_capsule_digest` so the
identity rule, rather than an incidental whole-digest mismatch, is proved. The
existing negative suite must report the same structured error in the loader and
must exit 67 without reaching the nucleus.

## Proposed detached ADR scope (not yet accepted)

The ADR proposal will define, exactly:

- `detached-source-set identity = SHA-256(concat(content_digest_i))` for file
  entries in canonical file-table order;
- zero files are invalid for capsule v1, so no empty-input detached identity is
  emitted or accepted; an empty byte sequence is not a capsule identity case;
- the builder computes the value and does not accept a caller-selected detached
  value; Git raw OID input remains explicit under ADR-0016;
- the parser independently recomputes the value after validating every file
  digest and rejects disagreement with a structured error;
- existing detached golden vectors using synthetic `0x42` identity are replaced
  only after acceptance, with vector digest, provenance and Stage 1 evidence
  regenerated; no Git-bound capsule compatibility is changed;
- deterministic output, source-to-runtime provenance, test evidence,
  invariants, trusted base, recovery, compatibility, dependencies, licensing,
  patent scope and rollback answers required by `docs/21`.

## BootInfo mismatch e2e evidence

The UEFI loader receives an explicit test-only Cargo feature. Only that feature
flips one byte in the BootInfo mirror after the loader copied the verified
capsule header; it never changes capsule bytes, digest or ABI fields needed to
validate BootInfo itself. The test builds a separately named test artifact and
boots it with the same ESP construction, OVMF pair, q35/qemu64 profile and
nucleus as the production harness.

The expected trace reaches `TOS.NUCLEUS.ENTRY`, emits
`TOS.IDENTITY.MISMATCH bootinfo-vs-capsule-header`, never emits `TOS.HALT`, and
exits 67. A production build assertion/CLI regression proves that the ordinary
loader invocation supplies no corruption feature.

## Vector provenance and licensing

F-22 remains open. The set is separated conceptually into:

1. vector format, expected outcomes, conformance definition and generator;
2. generated binary fixtures; and
3. embedded canonical source materials and their notices.

`LICENSE.md` permits Apache-2.0 for reusable test vectors only when explicitly
designated, while generated artifacts must retain provenance of every canonical
input. The valid capsule embeds GPL-class `init.tos` and a licence notice;
therefore neither an Apache-only nor GPL-only container label is inferred here.

Before committing regenerated binaries, Phase 1 prepares one machine-verifiable
manifest for the vector set with filename, SHA-256, generator identity/version,
source commit, canonical input paths, every embedded SPDX identifier and
generated-artifact status. README, `vectors.tsv` and the architecture statement
will be reconciled only after the policy analysis identifies an existing
authority for the binary container classification. If that authority is absent,
the work stops for owner direction rather than inventing a rule.

## Tests and completion boundary

The normal loader/capsule success path must remain byte/behavior compatible for
the Level 0/1 work. Each security rule receives a RED-to-GREEN focused test,
then integration and QEMU evidence where stated. No detached builder/parser
change, detached vector regeneration, Stage 1.5 work or next-phase closure work
occurs before the detached ADR is accepted.
