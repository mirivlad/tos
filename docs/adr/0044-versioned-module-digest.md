<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0044: A versioned module-digest scheme

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-12
- Decision level: 2 — the identity a verifier receipt binds to, an engine
  re-derives, and a cache entry is keyed on
- Project Architect approval: *(none — this ADR is not accepted)*

## Context

The module digest is the largest single cost in the Stage 2 frontend, and its
shape is now measured (`docs/evidence/STAGE2_MODULE_DIGEST.md`):

```text
canonical hash stream   5 975 526 bytes for a 262 114-byte module  (22.8x)
digest total                   34 607 us   — about a third of the frontend
  stream build                  1 505 us   (4%)
  sha-256                      32 545 us   (94%)
  hex render                        ~0
```

Building the stream is cheap and hashing it is not, because the stream is 22.8
times the size of the module it describes. The expansion has one dominant cause:
every count and every length is encoded as a **16-byte big-endian `u128`**,
whatever its magnitude. A module at the published ceiling has tens of thousands
of such values and nearly all of them fit in one or two bytes.

An attempt to recover the cost without touching the encoding was measured and
failed: hashing the stream incrementally rather than buffering it was *slower*.
There is no meaningful saving on the 4% and no defect in the hash's ~184 MB/s.
The only lever with real leverage is the volume of bytes hashed.

## What makes this an Architect question

docs/43 section 1 deliberately declines to freeze an on-disk IR encoding, so a
change here is **not** a semantic `tos-ir/v1` change: no program means anything
different, no rule is relaxed, and the verifier checks exactly what it checked.

But the stream is the module's **identity**. Every module digest would change,
and a module digest is:

- what a verifier receipt binds to;
- what an engine re-derives from the module it was handed before it will run
  anything (docs/43 section 5);
- what a cache entry is keyed on (`tos-cache`).

That is a versioned interface between components, and it is not changed quietly.

## The questions this ADR asks

1. **Is the digest scheme versioned independently of `tos-ir/v1`?** They answer
   different questions — one is what a module *means*, the other is how a module
   is *named* — and binding them together means a future identity change forces a
   language version it has nothing to do with.
2. **How does the version travel?** A receipt already carries `schema_id`; a
   digest scheme identifier could sit beside it, or be folded into the digest
   string itself (`sha256-v2:...`) so a consumer cannot mistake one for another.
3. **How do old receipts and caches fail?** They must fail **closed**: a receipt
   whose digest scheme a component does not recognise is not a receipt it may
   act on. Silent recomputation under a new scheme would let a stale receipt
   authorise a module it never described.
4. **What must the encoding guarantee?** Canonical and injective: two distinct
   modules must not produce the same stream, and one module must produce exactly
   one stream on every implementation. A variable-length integer encoding
   satisfies this if the encoding is itself canonical — one shortest form per
   value, no redundant representations — which is a requirement to state, not an
   assumption to make.
5. **What is invalidated?** Every existing cache entry and every retained
   receipt. Both are derived artifacts that regenerate from canonical source, so
   the consequence is time rather than loss — but the invalidation is total and
   should be stated as such.

## Options

1. **Leave the encoding alone.** The digest stays about a third of the frontend.
   No identity changes, no migration, and the docs/35 frontend budget carries the
   cost permanently.
2. **Canonical variable-length integers, digest scheme v2.** Counts and lengths
   in a canonical varint form; the scheme is versioned and carried in the digest
   string; unknown schemes fail closed. Expected to cut the stream by most of the
   22.8x expansion and the digest cost with it, though the exact figure needs
   measuring rather than predicting.
3. **Hash the structure without materialising a stream.** A tree hash over the
   module's shape. Larger change, and it makes the canonical form harder to state
   and to reimplement independently, which is a cost paid by every future
   implementation.

## Recommendation

**Option 2**, with the digest scheme versioned **independently** of
`tos-ir/v1` (question 1) and the version carried in the digest string
(question 2), because a consumer that reads a digest should not need a side
channel to know how to interpret it.

The measurement supports acting rather than accepting the cost: a third of the
frontend, in a component whose expansion is 22.8x and whose cause is a fixed
16-byte encoding of values that are almost always small.

It does **not** support acting hastily. Nothing here is on the critical path of
correctness, the current scheme is correct, and the migration invalidates every
receipt and cache in existence. This is a decision to take deliberately.

## Consequences

If accepted, the frontend's largest single cost falls substantially and the
docs/35 500 ms budget becomes materially more reachable — but this ADR is not a
performance-budget argument and should not be read as one. If rejected, the cost
is understood, documented, and permanent, and ADR-0043's frontend part should
say so.

## Alternatives considered

**Cache the digest inside `Module`.** Rejected. Three components derive it
independently precisely so that none of them takes another's word for which
module a receipt describes. A cached field would turn each of those checks into
a comparison of a value against itself.

**Hash less of the module.** Rejected outright: the receipt binds to the
*complete* module (docs/43 section 5). A digest over part of a module names
something that is not the module.
