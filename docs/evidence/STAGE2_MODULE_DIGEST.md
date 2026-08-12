<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# What the module digest costs, and what it is made of

Evidence level: **P1**, diagnostic (native medians, 21 samples).
Producer: `tos-core-performance --stages`.
Fixture: the 262 114-byte canonical module of the docs/35 frontend metric.

The frontend's largest stage is the independent verifier, and the verifier's
largest component is the module digest. This decomposes it, because "the digest
is slow" and "hashing is slow" and "the stream is too big" call for three
different pieces of work and only a measurement separates them.

## Measured

```text
canonical hash stream        5 975 526 bytes   for a 262 114-byte module  (22.8x)

verify                              48 246 us
  of which digest                   34 607 us   (72% of verify)
    stream build                     1 505 us   (4% of the digest)
    sha-256                         32 545 us   (94% of the digest)
    hash + hex render               32 547 us   (rendering is ~0)
```

The whole frontend is about 108 ms, so **the digest is roughly a third of it,
and SHA-256 over the stream is nearly all of the digest.**

## What that rules in and out

**Building the stream is not the problem.** 1.5 ms of 34.6 ms. Optimising the
writer — buffering strategy, allocation, copying — cannot recover more than that,
and an attempt to do so was already measured and rejected: hashing incrementally
instead of buffering was *slower*, because a compression function called with
small fragments pays its per-call cost repeatedly.

**Rendering is not the problem.** Hex formatting and the `sha256:` prefix are
below the noise.

**The stream's size is the problem.** 5.7 MB for a 262 KB module — 22.8x
expansion. SHA-256 runs at about 184 MB/s here, which is an ordinary software
rate, so the cost is the volume rather than the hash's implementation.

The expansion has one dominant cause: **every count and every length in the
canonical encoding is written as a 16-byte big-endian `u128`**, whatever its
magnitude. A module at the published ceiling has tens of thousands of table
lengths, string lengths, operand indices and block counts, and almost all of
them fit in one or two bytes. A variable-length encoding of the same values
would carry the same information injectively in a small fraction of the bytes.

## Why this is not simply fixed

The stream **is** the module's identity. Changing the encoding changes every
module digest, and a module digest is what a verifier receipt binds to, what an
engine re-derives before it will run anything, and what a cache entry is keyed
on. It is not an internal representation with a free hand over it.

docs/43 section 1 deliberately does not freeze an on-disk IR encoding, so this is
not a semantic `tos-ir/v1` change. But it is an identity change with consequences
for receipts and caches, and it is not made silently. **ADR-0044 (Proposed)**
states the question.

## The digest is computed more than once, by design

Three places derive it, and none of them is redundant:

- `tos-verifier` computes it to issue the receipt;
- `tos-engine` recomputes it from the module it was handed, and refuses to run
  unless it matches the receipt (docs/43 section 5) — the engine must not take
  the verifier's word for *which module* the receipt describes;
- `tos-cache` recomputes it before admitting an entry, for the same reason.

Caching the value inside `Module` would make each of those checks a comparison
of a number against itself. The repetition is the mechanism, not an oversight,
and it stays. It does mean the cost is paid twice on a full pipeline run — once
in `verify`, once in `run` — which is worth knowing when reading the totals.
