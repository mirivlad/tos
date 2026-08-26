<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0070: A compact verified module image

- Status: **Proposed**
- Date: 2026-08-25
- Decision level: 2 — it adds a bounded, versioned encoding of `tos-ir/v1` that
  the verifier reads and the engine executes. It changes no TOS Core semantics,
  no ABI operation and no invariant
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Evidence: `docs/evidence/STAGE3_PROCESS_GRANT.md`,
  `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md` (the section 6 measurement)
- Related: ADR-0044 (Proposed) — digest scheme v2, whose stated operational
  reason has now arrived; ADR-0069 (Proposed) — the grant this measurement came
  from; docs/43 §1, whose obligations any persisted form must meet

## The gap, stated once

`execute_set` holds every lowered module alive for the whole run, and after the
frontend was phased (ADR-0069 §6) that is what remains of the arena's slope:
`12.52 MiB` per ceiling-sized module.

Measured, that live cost is mostly not the module's meaning:

| | Live `tos_ir::Module` | Canonical stream | Ratio |
|---|---:|---:|---:|
| ceiling-sized dependency | 12.10 MiB | 5.26 MiB | **2.3x** |
| entry module | 19.18 MiB | 8.41 MiB | **2.3x** |

The same ratio at 2, 4 and 8 modules. And inside it, `11 338` source-map entries
per module each own six `String`s of which five name the module: `1.59 MiB` of
text with **`147` distinct bytes**, the same identity written once per lowered
operation.

So of `~15 MiB` live, about `6.5 MiB` is semantic payload and about `8.5 MiB` is
the in-memory representation carrying it.

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

### 3. One canonical semantic representation, shared with ADR-0044

ADR-0044 proposes digest scheme v2 — canonical varints, and **module-level
identity referenced rather than repeated** — and says in as many words that it
is "waiting on an operational reason, such as receipt or cache persistence,
boot-time pressure, many modules, or transport over storage or a network". Three
of those four have now arrived, measured.

**Two encodings must not be invented.** Either the image *is* ADR-0044's v2
canonical semantic representation inside the frame of §2, or the image's
verifier independently recomputes the same versioned semantic digest from what
it read. Anything else leaves two canonical forms that must be proved equivalent
to each other forever, by every future implementation.

Source-map identity interning belongs to that shared representation. **Logically
every source-map entry still carries the docs/43 fields it carries today**;
physically the five that name the module reference one module-identity record.
The contract's content does not change; its repetition does.

### 4. This ADR is about the density of one module, and promises nothing about a closure

It decides how much memory **one verified module image** costs. It does **not**
propose that a whole dependency closure be resident.

The arithmetic that rules that out is already in hand: eight canonical streams
of ceiling-sized modules are `45.24 MiB`, so extrapolating today's density to
256 modules is on the order of **`1.35 GiB`**. That is not a bound on a future
encoding — a better encoding is exactly what §3 is for — but it is enough to say
that "make the modules smaller and keep them all" is not an answer to the grant
question, and this ADR does not offer it as one.

The §6 measurement has since made the point sharper rather than softer. One
ceiling-sized module encodes to `0.37 MiB`, `14.32x` below its canonical stream —
and **verifying it still costs `28.32 MiB` of peak memory**, because the reader
materializes a module before the verifier traverses it. The quantity a residency
decision must bound is the second number, not the first. A compact artifact is a
statement about storage and transport; it does not by itself make a closure
resident, which is why §5 is a requirement and not an alternative.

### 5. Bounded residency is a required follow-up, not an alternative

A separate ADR must define **bounded verified-module residency**: how many
verified modules an execution may hold at once, and what supplies the rest.

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

### 6. What must be measured before this is accepted

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

No number is claimed in this ADR. The claim belongs in the evidence, before
acceptance rather than after it.

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

## What this ADR does not decide

- **Not the engine.** `run_set` is not rebuilt here, and no integration happens
  before §6's evidence exists.
- **Not residency.** §5 is a requirement on a later decision, not a design.
- **Not the grant size.** ADR-0069 stays Proposed and `54 MiB` stays
  provisional; the grant is re-measured once this and residency are settled,
  not re-argued.
- **Not ADR-0044's acceptance.** That decision remains its own; this ADR states
  that the two must share one representation, not that either is approved.

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
substitute: it leaves the `2.3x` in place, and under §3 the interning is part of
the shared representation anyway rather than a separate change to argue about.

**Keep the current in-memory form and accept the cost.** Understood and
measured, and it makes the grant question harder every time the closure grows.
Rejected as a default rather than as a possibility: it is what happens if
nothing is decided.

**Invent a compact encoding independent of the digest scheme.** Rejected in §3.
Two canonical forms would have to be proved equivalent by every implementation
that ever reads either.
