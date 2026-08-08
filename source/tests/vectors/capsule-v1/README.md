<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Golden capsule vectors for format v1

The committed binaries are generated from canonical sources by
`source/tests/vectors/gen/gen.sh`. Every tracked `.bin` has an entry in
`provenance.json`, verified by `scripts/check-capsule-vector-provenance.py` and
the SPDX/provenance gate.

Each container is classified as `mixed-material-generated` under ADR-0019. This
is provenance status, not an SPDX licence expression: the manifest enumerates
the canonical materials, generator provenance and retained licence-notice role.

Detached vectors use ADR-0018's domain-separated canonical path/content-digest
identity; no fixture accepts a synthetic caller-selected `0x42` identity.
`vectors.tsv` remains the hand-maintained expected parse outcome table for the
integration and QEMU negative suites.
