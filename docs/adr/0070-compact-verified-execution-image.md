<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0070: A compact verified execution image, as a derived cache

- Status: **Proposed**
- Date: 2026-08-25
- Decision level: 2 — it adds a derived artifact between the verifier and the
  engine, and a rule about what may be trusted from it. It changes no TOS Core
  semantics, no ABI operation and no invariant
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Evidence: `docs/evidence/STAGE3_PROCESS_GRANT.md`
- Related: ADR-0069 (Proposed) measures the memory this is about; docs/43 fixes
  the IR contract and has deliberately **not** fixed an on-disk encoding

## The gap, stated once

`execute_set` holds every lowered module alive for the whole run, because
`run_set` is handed the whole set at once. After the frontend was phased
(ADR-0069 §6) that is what remains of the arena's slope: `12.52 MiB` per
ceiling-sized module.

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

`canonical_stream` is used above **as a density estimate only**. docs/43 has not
fixed an on-disk encoding, and this ADR does not fix one either.

## Decision

### 1. A verified execution image, produced after verification

The pipeline may produce, for a module that has been verified, a **compact
immutable execution image**: the same semantics in a representation built for
being held and executed rather than for being constructed and edited.

It is produced **after** the verifier, from what the verifier saw, and it
carries the receipt that was issued for it. Nothing about the frontend or the
verifier moves: the image is downstream of both.

### 2. It is a derived cache, and the source stays canonical

The image is a **derived artifact** in the sense AGENTS.md §9 already fixes for
this project: traceable to its source inputs, commit, builder version and output
digest; deletable at any time; and **fully regenerable from source**. Deleting
every image must not remove system functionality — only speed.

The canonical form remains the source text. An image is never a second source of
truth, is never edited, and is never the thing a module's identity is computed
from.

### 3. Digest-bound, and the verifier boundary is not bypassed

An image is named by the digests it is derived from — the module digest the IR
contract already defines, the verifier receipt's identity, and the builder's —
so an image that does not belong to the module being run cannot be mistaken for
one that does.

**Loading an image is not a way to skip verification.** Either the image is
verified on load with the same verifier, or it is bound to a receipt that was
issued for exactly these bytes and the binding is checked before use. Which of
those two is the mechanism is the substance of this decision and needs the
Project Architect's choice; what is not open is that an unverified image may
never execute, and that a cache may never become the reason something ran
unchecked.

### 4. What it is expected to buy, and what would prove it

The measured `2.3x` and the repeated identity text are the target: an image that
interned module identity once instead of once per operation, and held tables
without construction slack, would carry the same meaning in something near the
canonical stream's density.

**No number is claimed here.** The claim to make is a measurement of the same
fixture with the image in place, against the figures in
`docs/evidence/STAGE3_PROCESS_GRANT.md`, and it belongs in this ADR before it is
accepted rather than after.

## What this ADR does not decide

- **Not an on-disk encoding for docs/43.** `canonical_stream` was a measuring
  stick, not a proposal. A persisted format is its own decision.
- **Not streaming or lazy execution.** Whether `run_set` must hold every module
  at once is a separate question about the engine, and it is deliberately left
  where it is: the engine is not rebuilt as a side effect of a memory
  measurement.
- **Not a conformance cap.** Nothing here narrows what the implementation
  promises.
- **Not the grant size.** ADR-0069 stays Proposed and `54 MiB` stays
  provisional; if this ADR is accepted and implemented, the grant question is
  re-measured rather than re-argued.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. I-02's canonical
  source and the source-to-runtime chain are served only if §2 and §3 hold —
  which is why they are the decision rather than optimisation notes.
- **Canonical representation:** unchanged. The image is derived; the text is
  canonical.
- **Trusted-base impact:** an additional representation the engine can execute.
  Its safety rests entirely on §3: a binding that is checked, or a verification
  that is repeated. A third option — trusting the cache because it is ours — is
  the failure this ADR exists to forbid.
- **Source-to-runtime impact:** the chain gains a link, and the link is
  digest-bound in both directions. Provenance must be able to answer "which
  source, which verifier, which builder" for an image as it can for a capsule.
- **Recovery and rollback impact:** deleting the cache is always safe, by §2.
- **Stage identity gate:** none claimed.
- **Threat-model impact:** a cache is an attack surface — a poisoned or stale
  image that executed would be a verified-looking path to unverified code.
  Negative tests are required: an image whose digest does not match, one bound
  to another module's receipt, and one whose builder identity is unknown, must
  all be refused rather than regenerated silently.
- **Performance contract:** the point of the change is memory, and the measured
  claim of §4 is what would have to be produced. Any effect on the Stage 3 IPC
  path is not expected and would be measured, not assumed.
- **Dependencies, licence, patents:** none.

## Alternatives considered

**Intern the source-map strings and leave the representation otherwise alone.**
This is the cheap part of the win — `13–16 %` of a live module — and it needs no
new artifact at all. It should probably be done regardless; it is listed as an
alternative because it is *not* a substitute for §1, and because doing only it
would leave the `2.3x` in place.

**Make `run_set` stream modules instead of holding them.** A different answer to
the same measurement, and possibly a better one. It is an engine change, and the
data to choose between them is not in hand — which is why this ADR proposes the
representation and explicitly leaves the engine alone.

**Do nothing until the grant question forces it.** Rejected as a way of
deciding: the grant size would then be chosen around an implementation's
retention, which is what ADR-0069 §2 exists to prevent.
