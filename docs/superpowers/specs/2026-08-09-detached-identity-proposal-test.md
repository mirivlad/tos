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
computed=b07b6e58e9e3aa9716d4ad779529a2e7be6522aef1f3e67a16230e04a55c8c05
RED: detached identity does not match ADR-0018 proposed path/digest encoding
```

The proposed calculation uses `DOMAIN = b"TOS.DSI.v1\0"` (hex
`544f532e4453492e763100`), then for each
canonical path/file-table index appends `u32_le(path_length)`, exact canonical
UTF-8 path bytes and the 32-byte `content_digest`, before applying SHA-256.
It did not write a vector or change builder or parser behavior.

## Proposed post-acceptance regression set

Only after ADR-0018 acceptance, add tests that prove all of the following:

1. the builder computes the fixed-domain, length-delimited canonical
   path/digest encoding and rejects an arbitrary detached identity input;
2. a digest-consistent capsule with one changed detached identity byte is
   rejected as `DetachedIdentityMismatch` by both host parser and real loader;
3. canonical path/file-table ordering and exact path bytes are material — a
   reordered entry or equal contents under a different canonical path produces
   a different identity and cannot validate the original capsule;
4. zero files remain rejected by the independent v1 zero-file rule, while the
   domain-separated formula itself has the unambiguous `SHA-256(DOMAIN)`
   mathematical value;
5. regenerated vectors reproduce deterministically and carry F-22-approved
   provenance records before becoming tracked binary fixtures; and
6. QEMU evidence reaches the existing fail-closed loader path with exit 67 and
   no nucleus entry for a malformed detached identity.

No executable failing test is committed with this proposal. It must remain a
review artifact until the architect accepts the Level 3 decision.
