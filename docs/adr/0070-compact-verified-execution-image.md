<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0070: A compact verified module image

- Status: **Accepted**
- Date: 2026-08-25 (accepted 2026-08-26, after the section 6 measurement)
- Decision level: 2 — it adds a bounded, versioned encoding of `tos-ir/v1` that
  the verifier reads and the engine executes. It changes no TOS Core semantics,
  no ABI operation and no invariant
- Project Architect approval: **given, 2026-08-26**, after the section 6
  prototype evidence. Acceptance carries the section 7 implementation gate:
  it does not authorize production engine integration
- Evidence: `docs/evidence/STAGE3_PROCESS_GRANT.md`,
  `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md` (the section 6 measurement)
- Related: ADR-0044 (Proposed) — digest scheme v2, which §3 leaves independently
  versioned from the storage encoding rather than merged into it; ADR-0071
  (Proposed) — the bounded residency decision §5 requires; ADR-0069 (Proposed) —
  the grant this measurement came from; docs/43 §1, whose obligations any
  persisted form must meet

## The gap, stated once

`execute_set` holds every lowered module alive for the whole run, and after the
frontend was phased (ADR-0069 §6) that is what remains of the arena's slope:
`12.52 MiB` per ceiling-sized module.

Measured, the live form is more than twice the same module's canonical stream:

| | Live `tos_ir::Module` | Canonical stream | Ratio |
|---|---:|---:|---:|
| ceiling-sized dependency | 12.10 MiB | 5.26 MiB | **2.3x** |
| entry module | 19.18 MiB | 8.41 MiB | **2.3x** |

The same ratio at 2, 4 and 8 modules. And inside it, `11 338` source-map entries
per module each own six `String`s of which five name the module: `1.59 MiB` of
text with **`147` distinct bytes**, the same identity written once per lowered
operation.

An earlier draft read the canonical stream as the module's semantic payload and
called the difference representation overhead. **The §6 prototype falsified
that.** The stream is itself a representation with its own costs — sixteen fixed
bytes for every number, enumerated values spelled as text with a sixteen-byte
length in front — and it is not a floor. What is measured, and all that is
claimed here, is this:

> The same semantic module — byte-identical `tos-ir/v1` content, confirmed by an
> unchanged semantic module digest after encode and verifier-owned parse — is
> `12 864 160 B` as a live `Module`, `5 561 951 B` as the current canonical
> stream, and `388 329 B` as the prototype image.

No theoretical minimum is asserted. Three representations of one module were
measured; a fourth might be smaller, and nothing here says otherwise.

## Decision

### 1. The encoding is untrusted input to the verifier, not a verified output

The trust chain is:

```text
source -> lower -> untrusted compact encoding -> verifier -> receipt -> engine
```

The compact form is produced **before** verification and is treated as hostile
bytes until the verifier has read it, exactly as any other external input is
(docs/34). The verifier checks **that representation** — not a `Module` some
other code decoded and vouched for — and issues a receipt bound to two things:
the **semantic module digest** and the **exact artifact identity** of the bytes
it read. The engine executes the verified image, or a view over it.

This replaces the shape the first draft of this ADR proposed, which produced an
image after verification and then had to choose between verifying it again and
trusting a binding. That choice is removed rather than answered: a cache that is
trusted because of who made it is the failure this decision exists to prevent,
and re-verifying an artifact the verifier had already approved is work done to
undo an ordering that was wrong.

### 2. A persisted form meets docs/43 §1, in full

Whatever bytes exist on the wire or on storage carry:

- a **magic** that identifies the format;
- an **encoding version**, independent of `tos-ir/v1`'s semantic version, so a
  reader knows how to interpret before it knows what it holds;
- **explicit length and table bounds**, checked before any allocation sized from
  them — the parser is total over arbitrary bytes and never sizes a read from a
  number it has not bounded;
- **canonical rules**: one encoding per value, so two encoders that agree on the
  meaning agree on the bytes;
- **unknown-field and unknown-version behaviour, failing closed** — a reader
  that meets something it does not know refuses rather than skipping;
- an **artifact digest** over the bytes themselves, distinct from the semantic
  digest of §3;
- **parser negative tests** as a condition of acceptance, not as follow-up work.

### 3. The storage encoding and the semantic digest scheme are versioned independently

They answer different questions and change for different reasons. **The image is
not required to be the digest scheme's byte stream.**

- the **storage encoding version** says how to read bytes;
- the **semantic digest scheme version** says how a module's identity is
  computed from its meaning.

The rule that keeps them from drifting is not that they share a byte layout. It
is this: **the verifier-owned parser reconstructs semantic `tos-ir/v1` from the
image and independently computes the versioned semantic digest from the
reconstructed module** — from the meaning, never from the bytes it was handed and
never from a value the image carried. The receipt then binds that semantic
digest to the **exact artifact digest** of the bytes the verifier read.

That is what makes the two versions safe to move apart. A new storage encoding
changes how the same module is spelled and the semantic digest does not move; a
new digest scheme changes how identity is computed and every reader recomputes
it the same way from the same reconstructed module. Neither has to be reissued
because the other changed, and no equivalence proof between two byte layouts is
owed to anyone — because identity was never defined by a byte layout.

An image that carried a semantic digest as a field would be asking to be
believed. The parser is untrusted input, so nothing it says about identity is
input to identity.

**Interoperation with ADR-0044 stands, at the level of the idea rather than the
bytes.** Canonical varints and module-level identity referenced rather than
repeated are what the §6 prototype implemented and measured — `20.9x` on the
source-map section. That is now evidence available to ADR-0044, which **remains
Proposed** and is not advanced by this decision. If v2 is later accepted, the
storage encoding may adopt its spelling or not; what it may never do is take an
identity it did not compute.

Source-map identity interning is a property of the storage encoding. **Logically
every source-map entry still carries the docs/43 fields it carries today**;
physically the ones that name the module reference one module-identity record,
and the parser restores the full entries before anything computes a digest over
them. The contract's content does not change; its repetition does.

### 4. This ADR decides encoded byte density, and promises nothing about a closure

It decides the **encoded byte density of one verified module image** — how many
bytes the module's meaning occupies on storage and in transport. It does **not**
decide what that module costs to verify, and it does **not** propose that a whole
dependency closure be resident.

Those are three different quantities and this ADR settles one of them. Saying
"how much memory an image costs" would have collapsed the first into the second,
which is precisely the mistake the measurement caught.

The arithmetic that rules that out is already in hand: eight canonical streams
of ceiling-sized modules are `45.24 MiB`, so extrapolating today's density to
256 modules is on the order of **`1.35 GiB`**. That is not a bound on a future
encoding — a better encoding is exactly what §3 is for — but it is enough to say
that "make the modules smaller and keep them all" is not an answer to the grant
question, and this ADR does not offer it as one.

The §6 measurement has since made the point sharper rather than softer. Both
figures are recorded here, separately, because they are separately binding:

| Quantity | Measured, one ceiling-sized module |
|---|---:|
| **encoded byte density** — what this ADR decides | **388 329 B (0.37 MiB)** |
| **verifier working-set peak** — what it does not | **29 697 360 B (28.32 MiB)** |

The second is the cost of reading the image and traversing what came out, on the
prototype's materializing reader: `12.19 MiB` of reconstructed `Module` and the
verifier's own tables above it. **The quantity a residency decision must bound is
the working-set peak, not the artifact size.** A compact artifact is a statement
about storage and transport; it does not by itself make a closure resident, which
is why §5 is a requirement and not an alternative.

### 5. Bounded residency is a required follow-up, not an alternative

A separate ADR must define **bounded verified-module residency**: how many
verified modules an execution may hold at once, and what supplies the rest.
That decision is now drafted as **ADR-0071 (Proposed)**.

Its shape is constrained here so that the follow-up cannot quietly become an
ambient authority:

- the supplier is an **explicit argument to the engine**, never a global, a
  default, or something the engine reaches for on its own;
- it is **constrained to the exact resolved closure and identities** the run was
  given — a provider that could return a module the resolution did not name
  would be module search by another name;
- **no ambient filesystem, network or module search**, and no path the engine
  can walk to find something it was not handed.

This is listed as a requirement rather than an alternative because it answers a
different question. Density and residency multiply; neither substitutes for the
other.

### 6. What was measured before this was accepted

A prototype encoder, parser and verifier path, built for measurement and **not**
switched into the production engine, reporting for a ceiling-sized module:

1. image byte size;
2. reduction against the live `Module`;
3. reduction against the current canonical stream;
4. verifier peak memory while checking the image;
5. encode, decode and verify time;
6. the source-map identity contribution after interning;
7. negatives: malformed, truncated, oversized, non-canonical-varint,
   unknown-version and wrong-digest inputs, each refused.

The numbers live in the evidence, not here — they were produced before
acceptance rather than promised after it.

**What such a prototype may leave out, and what it may not.**

The prototype **may** cover only the semantic variants the ceiling fixture
requires. The **exact coverage must be recorded in the evidence** — the
supported tagged families and the unsupported ones, both listed — and **every
unsupported semantic tag must fail closed** on both sides: an encoder refuses to
write what it cannot round-trip, and a parser refuses a tag it does not know.
Partial coverage is safe only because refusal is the behaviour; a prototype that
skipped what it did not recognize would be measuring a format nobody could ship.

The **container and its security surface must be complete**: magic, encoding
version, canonical varints, section and table lengths, bounds checked before any
allocation sized from them, an artifact digest, fail-closed unknown version and
unknown tag, and negatives for malformed, truncated, oversized,
non-canonical-varint and wrong-digest inputs.

The prototype **must not use the production magic or encoding version.** It is
an explicitly experimental `v0`, and the engine never executes it.

The parser **belongs to the verifier path.** For the prototype it is acceptable
that this verifier-owned parser materializes an internal `Module` and then runs
the existing semantic verifier over it — provided the peak memory of that
materialization is measured honestly and reported as what it is. A production
zero-copy or bounded-view reader is **not** designed at this stage.

**The invariant the whole measurement rests on:** after `encode` followed by the
verifier-owned parse, the **semantic module digest must equal the digest of the
`Module` that was encoded**. Without it every byte figure is a measurement of
something else.

Canonical varints and module-level source-map identity are used as an
**experimental candidate** for ADR-0044's digest scheme v2. Measuring them here
does not advance that ADR's status.

**Such a prototype is evidence for the density and architecture decision. It is
not a production format, and it does not close the completeness obligations of
docs/43 §1** — which §2 above states, and which a production encoder must meet
in full.

Measured: `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md`. The prototype lives in
`source/tests/image-prototype/` and is built and linted with the workspace so it
cannot rot unnoticed, while being reachable only by running it.

### 7. The implementation gate

**Accepting this ADR does not authorize production engine integration.** The
decision is the shape; the permission to build on it is separate, and it opens
only when both of these hold:

1. the production format **covers 100 % of `tos-ir/v1`** — every tagged family,
   every variant, with no declared-coverage exception and nothing left to fail
   closed for want of an implementation;
2. it **closes the docs/43 §1 parser and conformance requirements in full** —
   the obligations §2 lists, met and demonstrated, not scheduled.

Until then the engine reads what it reads today, and an image is a measured
artifact rather than an execution path.

`TOSIMGx0` is **not** a candidate for promotion. It is version `0` of an
experiment, its coverage is partial by declaration, and a production format
starts with its own magic and its own version rather than by graduating this
one. Nothing in this acceptance is an instruction to finish the prototype's
payload.

The gate exists because a decision and a permission look alike from a distance.
An accepted ADR that quietly licensed a partial parser into the trusted base
would be worse than no ADR: it would be this project's own review process
producing the failure the design was written to prevent.

## What this ADR does not decide

- **Not the engine.** `run_set` is not rebuilt here, and §7 gates any
  integration on complete coverage and complete conformance. Acceptance is the
  shape of the decision, never a licence to start.
- **Not residency.** §5 is a requirement on a later decision, not a design.
- **Not the grant size.** ADR-0069 stays Proposed and `54 MiB` stays
  provisional; the grant is re-measured once this and residency are settled,
  not re-argued.
- **Not ADR-0044's acceptance.** That decision remains its own and stays
  Proposed. §3 makes the storage encoding and the digest scheme independently
  versioned, so neither waits on the other.
- **Not a theoretical minimum.** Three representations of one module were
  measured. Nothing here claims a floor.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. The canonical form
  remains the source text; an image is derived from it and is regenerable.
- **Canonical representation:** unchanged. A module's identity is still computed
  from its source, and an image never becomes a second source of truth.
- **Trusted-base impact:** a new parser that reads untrusted bytes — which is
  why §2's obligations are the decision rather than notes attached to it. The
  verifier's position in the chain does not move; what it reads does.
- **Source-to-runtime impact:** the chain gains an artifact, and both digests —
  semantic and artifact — travel with the receipt, so "which source, which
  bytes, which verifier" stays answerable.
- **Recovery and rollback impact:** an image is derived and deletable; deleting
  every image costs speed and no functionality (AGENTS.md §9).
- **Stage identity gate:** none claimed.
- **Threat-model impact:** the parser is a new attack surface and is treated as
  one: total over arbitrary bytes, bounds before allocation, fail-closed on
  unknown versions, and negative tests required before acceptance. A poisoned,
  stale or foreign image must be refused by the verifier, not by convention.
- **Performance contract:** the change is about memory; encode/decode/verify
  time is measured in §6 rather than assumed, because a smaller image bought
  with a slower verifier is a trade this project states rather than takes.
- **Dependencies, licence, patents:** none.

## Alternatives considered

**Intern the source-map identity and change nothing else.** The cheap part of
the win — `13–16 %` of a live module — and it needs no new artifact. It is not a
substitute: it leaves the live representation where it is, and the §6
measurement puts the whole module at `388 329 B` where interning alone would
have removed about `1.7 MiB` from a `12.86 MiB` value.

**Keep the current in-memory form and accept the cost.** Understood and
measured, and it makes the grant question harder every time the closure grows.
Rejected as a default rather than as a possibility: it is what happens if
nothing is decided.

**Make the image byte-for-byte the digest scheme's stream.** Considered and
**not** taken; §3 says why. It sounds like the safe choice — one byte layout,
nothing to prove equivalent — but it welds two versions together that change for
different reasons, so a storage format could not improve without reissuing every
module's identity. The property actually needed is weaker and stronger at once:
identity is computed from the *reconstructed meaning*, by the verifier, from
whatever encoding it read. Two encodings then need no equivalence proof, because
neither of them defines identity.

**Let the image carry its semantic digest as a field.** Refused. The image is
untrusted input; a digest it supplied would be a claim about identity made by
the thing whose identity is in question.

**Accept this ADR and integrate it.** Refused by §7. The decision is the shape,
not the permission: production integration waits on complete `tos-ir/v1`
coverage and complete docs/43 §1 conformance.
