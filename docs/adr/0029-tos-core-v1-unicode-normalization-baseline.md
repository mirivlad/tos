<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0029: TOS Core V1 Unicode normalization baseline

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Decision level: 2 — fixes the versioned TOS Core 1.0 source-identity and
  normalization contract within ADR-0028's accepted language boundary
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

ADR-0028 and docs/39 require canonical `.tos` source to be UTF-8, LF, and
Unicode NFC. Unicode is deliberately permitted in comments and string data,
although identifiers remain ASCII-only. The earlier wording did not pin the
Unicode data against which NFC is determined. Letting the host OS, libc, ICU,
Rust, Python, locale, or a newer Unicode release choose that data would make
canonical-source acceptance and `E1004_NOT_NFC` implementation-dependent.

Because normalized source bytes participate in source-content identity, module
resolution, source maps, IR receipts, and cache keys, this is a Level 2
determinism gap rather than a library-selection detail.

## Decision

TOS Core 1.0 uses exactly this Unicode normalization baseline:

```text
Unicode Standard:                 17.0.0
Unicode Character Database:       17.0.0
Normalization specification:      UAX #15, Revision 57
Normalization form:               NFC
```

After the existing CRLF-to-LF transport normalization, canonical `.tos` input
MUST be valid NFC under that exact baseline. This preserves the existing
ASCII-only identifier grammar, Unicode-permitted comments/string data, and
`E1004_NOT_NFC` diagnostic. It does not normalize runtime `string` values.

The reference frontend MUST derive its normalization data reproducibly from
the accepted UCD release. It MUST NOT take host Unicode tables, locale, ICU,
libc, Rust/Python library release, or a newer Unicode release as semantic
authority. A host tool may assist generation only when the generated result is
independently pinned to this baseline.

Before the frontend enables the generated tables, its checked-in provenance
record MUST state the Unicode/UCD/UAX versions, exact upstream UCD files,
their integrity hashes, and the generator identity/version. The required
inputs are the minimum applicable subset of `UnicodeData.txt`,
`CompositionExclusions.txt`, `DerivedNormalizationProps.txt`, and
`NormalizationTest.txt`. Any imported or generated material follows the
third-party licence, notice, provenance, and reproducible-build requirements.
No runtime Unicode-library dependency is admitted by this ADR.

The conformance corpus MUST cover NFC acceptance, decomposed and
combining-order rejection in comments and strings, ASCII byte identity, UTF-8
precedence before normalization, and sufficient NormalizationTest.txt-derived
positive/negative cases to prove the fixed baseline. The same normalized source
bytes MUST result in the same source-content identity independently of host
Unicode version.

Unicode normalization data is part of the TOS Core 1.0 language contract. A
future Unicode/UCD baseline cannot silently alter V1 acceptance or identity;
it requires an explicit versioned language and compatibility decision. An
implementation supporting multiple language versions selects the normalization
baseline from the declared TOS language version, never from the host.

## Architecture impact statement

- **Invariants/canonical representation:** canonical human-readable source and
  its source-content identity remain unchanged; this fixes which NFC predicate
  determines those existing bytes.
- **Trusted base/dependencies:** no runtime dependency or host Unicode service
  enters the frontend, verifier, recovery path, or TOS semantic contract.
- **Source-to-runtime/recovery:** source maps, IR receipts, module closure and
  disposable-cache identities now bind an explicit normalization baseline;
  deleting caches still regenerates from canonical source.
- **Threat model:** hostile source cannot select a locale or host Unicode table
  to change validation; malformed UTF-8 precedes normalization; bounded input
  limits remain in docs/44.
- **Performance/compatibility:** the accepted 256-KiB source bound remains;
  normalization data and conformance costs are measured as part of Stage 2,
  not delegated to a host library. TOS Core 1.0 is permanently compatible with
  Unicode 17.0.0/UAX #15 Revision 57 only.
- **Licence/patent:** no material is imported by this documentation decision.
  Future UCD inputs/tables require exact licence/notice/provenance records
  under docs/22, docs/27, and docs/28 before use.
- **Evidence:** deterministic Unicode-17 normalization vectors, generated-data
  provenance/hash checks, source-identity equality across host environments,
  and malformed-UTF-8 precedence tests are required before frontend closure.

## Consequences

The first Stage 2 source reader/lexer must implement or use only reproducible,
version-pinned Unicode 17.0.0 NFC data. It may not claim a partial ASCII-only
normalizer as TOS Core V1 conformance. ADR-0028 remains accepted and its
language foundation, grammar, ownership, IR, verifier, and runtime decisions
are not reopened.
