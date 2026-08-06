<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS architecture authority

The normative hierarchy is defined by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.

In summary, architecture authority descends through:

1. active system invariants;
2. accepted ADRs and architecture-preservation policy;
3. normative subsystem specifications;
4. stage, testing and release policies;
5. explanatory root documents;
6. generated convenience views.

`TOS_DEVELOPMENT_SPECIFICATION.md` is generated from normative sources and is never independently authoritative. If it differs from an individual source document, the source document governs and the generated file must be rebuilt.

The project does not use an MVP phase. A narrow implementation is valid only when it exercises the intended long-term contract. A quick demonstration that requires replacement of its trust boundaries, source model, driver placement, object identity or recovery path is not a TOS milestone.

Architecture-preserving substitutions are possible. For example, a different hash algorithm, scheduler or parser implementation may be accepted if it preserves the relevant contracts. Architecture-erasing substitutions—such as replacing canonical text with canonical binaries, embedding a general third-party runtime into the trusted nucleus for convenience, or making Git merely a developer tool—require rejection or an explicit identity-affecting ADR.

Every stage must satisfy `docs/37_STAGE_IDENTITY_GATES.md`; conventional feature completion alone is insufficient.
