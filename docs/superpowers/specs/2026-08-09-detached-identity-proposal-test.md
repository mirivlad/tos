<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Detached capsule identity — proposal-only regression evidence

Status: evidence for ADR-0018 review only. This document is non-normative and
does not make the detached formula an implementation contract.

## Local RED observation

On 2026-08-09, an uncommitted local probe read the committed detached
`source/tests/vectors/capsule-v1/valid-001.bin` and calculated:

```text
file_count=2
stored=4242424242424242424242424242424242424242424242424242424242424242
computed=56daf5dbc0865b626200a1284100b7c4642f686b6d23978dc1050dfe8bc0b7ce
RED: detached identity does not match SHA-256(concat(content_digest_i))
```

The calculation read each 32-byte `content_digest` at file-table offsets
`file_table_offset + i * 64 + 16`, concatenated them in increasing file-table
index order, and applied SHA-256. It did not write a vector or change builder
or parser behavior.

## Proposed post-acceptance regression set

Only after ADR-0018 acceptance, add tests that prove all of the following:

1. the builder computes the ordered digest concatenation and rejects an
   arbitrary detached identity input;
2. a digest-consistent capsule with one changed detached identity byte is
   rejected as `DetachedIdentityMismatch` by both host parser and real loader;
3. canonical file-table ordering is material — reordering digest input gives a
   different identity and cannot validate the original capsule;
4. zero files remain rejected by the independent v1 zero-file rule, while the
   formula itself has the unambiguous `SHA-256(empty)` mathematical value;
5. regenerated vectors reproduce deterministically and carry F-22-approved
   provenance records before becoming tracked binary fixtures; and
6. QEMU evidence reaches the existing fail-closed loader path with exit 67 and
   no nucleus entry for a malformed detached identity.

No executable failing test is committed with this proposal. It must remain a
review artifact until the architect accepts the Level 3 decision.
