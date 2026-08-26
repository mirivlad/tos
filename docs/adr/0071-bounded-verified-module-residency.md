<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0071: Bounded verified-module residency and the module provider

- Status: **Proposed**
- Date: 2026-08-26
- Decision level: 2 — it fixes how many verified modules an execution may hold
  at once, what supplies the rest, and what survives a module image being
  released. It changes no TOS Core semantics, no ABI operation and no invariant
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Evidence: `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`, `docs/evidence/STAGE2_ARENA_BOUND.md`
- Related: ADR-0070 (Accepted) §5, which requires this decision and constrains
  its shape; ADR-0069 (Proposed) — the grant, still provisional pending this;
  ADR-0040 — the whole-machine budget both accounts are spent from
- Note: this ADR **designs only**. No engine change is proposed for
  implementation here, and `run_set` is not rebuilt by it

## The gap, stated once

ADR-0070 decided **encoded byte density** and said in as many words that it
promises nothing about a closure. The measurement is why:

| Quantity, one ceiling-sized module | Measured |
|---|---:|
| encoded image | 388 329 B (0.37 MiB) |
| **verifier working-set peak** | **29 697 360 B (28.32 MiB)** |
| live lowered `Module` retained by `run_set` | 12.52 MiB per module |

`run_set` is handed the whole set at once and every lowered `Module` stays alive
until the run ends. At the declared closure ceiling of 256 modules that is an
extrapolation past this machine, and no encoding fixes it: **making the artifact
smaller did not make the working set smaller.** What is missing is a decision
about *how many verified modules may be resident at one time, and what supplies
the ones that are not.*

The danger in answering it is specific and worth naming first. A provider that
can fetch a module is one short step from a module search path, and a cache that
can supply a receipt is one short step from a verifier that never ran. Both are
failures this project's architecture exists to prevent, and both look like
conveniences at the moment they are introduced.

## Decision

### 1. Verification is sequential, at launch, over the exact resolved closure

Before an execution begins, every module of the **exact resolved closure** — the
one module resolution produced, by identity, no more and no fewer — is verified
**one at a time**. Each module in turn is decoded from its image, traversed by
the verifier, reduced to the trusted record of §2, and its materialized `Module`
released before the next is decoded.

Two consequences, both intended:

- the **peak** cost of launch is one module's working set, not the closure's.
  With the measured figure that is `28.32 MiB` once, rather than `28.32 MiB`
  times the closure size;
- **verification is complete before execution starts.** No module is entered
  that has not been verified, and nothing is verified lazily on first call.
  Lazy verification would make an execution's trusted base depend on which
  branch it happened to take.

An execution whose closure cannot be verified in full does not start. There is
no partial launch, and no "verify the rest when we get there".

### 2. What survives: a small trusted verified-module record

When a module's materialized `Module` is released, what remains is a **trusted
verified-module record** — small, fixed-shape, held in trusted memory for the
lifetime of the execution:

- the **semantic module digest**, as the verifier computed it from the
  reconstructed module;
- the **artifact digest** of the exact bytes the verifier read;
- the **verifier identity** that produced the receipt;
- the module's **name, source set, content ID and dependency digest**;
- the **profile, resource envelope and capability-interface digest**;
- the **source-map digest**;
- the **export surface the closure needs** — the function identities other
  modules of this closure resolve to, and nothing else.

This is deliberately close to the existing `VerifiedModule` receipt, because
that is already exactly the shape "this module passed this verifier" takes. What
this ADR adds is a **lifetime**: the record outlives the image and the
materialized module, and is what everything later compares against.

The record is bounded and its size does not grow with the module's body. That is
the property that makes the whole design work: releasing a `Module` must free
`12–15 MiB` and retain kilobytes.

**The record lives in trusted memory and is never reloaded from a cache.** It is
produced by the verifier in this execution's own launch, and there is no path by
which a stored copy of one can be adopted (§9).

### 3. The module provider is an explicit argument, constrained to the closure

The engine is **handed** a provider. It never has one by default, never
constructs one, and never reaches for one.

- the provider is an **explicit argument** to the execution — not a global, not
  an ambient default, not a field the engine fills in when the caller left it
  empty;
- it is **constrained to the exact closure identities** established at launch.
  A request names a module by an identity in the trusted record set of §2; a
  request naming anything else is refused by the provider's own construction,
  not by a check the engine remembers to write;
- the provider **cannot enumerate**. There is no "list what you have", because
  a component that can enumerate can be asked what else exists, and what else
  exists is not this execution's business.

The provider's whole authority is: *given an identity this execution already
verified, return bytes that claim to be that module's image.* It cannot widen a
closure, and returning a module the resolution did not name is not a bug it
could have but a request it cannot express.

### 4. No ambient filesystem, network or module search

The engine has **no path it can walk**. Not a search path, not a module root,
not a fallback directory, not a network fetch, not an environment variable that
names any of those.

If the provider does not return an image for an identity the execution needs,
the execution fails (§9). It does not look elsewhere, because there is no
elsewhere: "look elsewhere" is the whole of module search, and module search is
how a program ends up running code nobody resolved.

This is stated as its own numbered decision rather than as a note on §3 because
it is the one a future convenience will try hardest to erode.

### 5. A reloaded image is checked against the trusted record, by artifact digest

When the provider returns bytes for a module that was evicted:

1. the **artifact digest** of the returned bytes is computed and compared
   against the artifact digest in the trusted record. A mismatch is refused
   (§9), before parsing;
2. the bytes are parsed by the verifier-owned parser as **untrusted input**,
   exactly as at launch;
3. the **semantic module digest is recomputed from the reconstructed module**
   and compared against the record's. A mismatch is refused.

Both checks, not one. The artifact digest is cheap and catches the ordinary
failures — a corrupted, stale or substituted image — before any parsing work is
done. The semantic digest is the one that matters: it is computed from meaning
rather than bytes, so it cannot be satisfied by anything that merely looks
right.

**Whether the full verifier re-runs on a reload is left open by this ADR**, and
deliberately. Two positions are defensible — that a matching semantic digest
plus this execution's own trusted record is exactly what a receipt asserts, or
that the verifier is cheap enough relative to correctness that it should simply
run again — and choosing between them without measuring reload frequency would
be choosing by taste. What is **not** open: the parse always happens as
untrusted input, and the semantic digest is always recomputed from the
reconstruction.

### 6. Eviction is safe because a continuation names identities, not addresses

A resident module image may be released while an execution is suspended inside
it. What makes that safe is that a continuation names **stable identities**:

- the **module identity** — the semantic module digest of the trusted record;
- the **function identity** — its index in the verified module's function table,
  which is part of what the digest covers;
- the **block identity** and the position within it.

Never a pointer into an image, never an offset into a decoded buffer, never a
reference whose meaning depends on a particular decoding being resident. A
continuation must survive the eviction and reload of the very module it is
suspended in, and it does so because nothing it holds refers to the image.

This is the load-bearing rule of the whole design. A residency scheme whose
continuations pointed into images would work perfectly until the first eviction
under memory pressure, which is to say it would work in every test and fail in
production.

The corollary is a constraint on what a resident image may be: the engine may
not hold derived pointers into it across a suspension point either. Whether the
engine's *execution* representation is a view over the image or a decoded
structure is an implementation question this ADR does not settle — but either
way, what crosses a suspension is identities.

### 7. Residency is bounded by count and by bytes, both

An execution declares, or is given, two bounds:

- a **maximum number of simultaneously resident module images**;
- a **maximum total bytes** of resident module images.

Both, because either alone is defeatable. A count alone lets a few large modules
exceed any byte budget; a byte budget alone permits an unbounded number of tiny
ones, and every resident image costs bookkeeping as well as bytes.

The bounds are **fixed properties of the execution**, in the sense ADR-0069 §2
fixes the grant size: not a function of free memory, not adaptive, not a share
divided among whatever is running. An execution either fits its bounds or fails
in a way that names the bound it hit. A program whose success depended on how
much memory happened to be free would be reporting a fact about scheduling as a
fact about itself.

The minimum admissible bound is **one**: an execution must be able to make
progress with a single resident image, or the design has not bounded anything.
Eviction policy above that minimum is an implementation matter and is not fixed
here.

### 8. Process-grant peak and whole-machine residency are accounted separately

Two accounts, reported separately, spent from one budget:

- **process-grant peak** — the arena high-water mark inside one process's
  `RuntimeMemoryGrantV1` region, which is what ADR-0069 sizes;
- **whole-machine physical residency** — the frames held by module images and
  any cache backing them, wherever they live.

They have different owners and different lifetimes, so summing them into one
number would hide which one binds. But both are spent from the **ADR-0040
whole-machine budget**, so reporting only one is reporting half a ledger — which
is exactly what ADR-0069 §7 already records.

The practical consequence, stated so it cannot be forgotten: **moving IR out of
the arena is not by itself a saving.** It moves the cost to the other account.
A residency proposal that showed a smaller grant and did not show what the image
account grew by would not have shown anything.

Any evidence taken under this ADR must report both.

### 9. Missing, stale and wrong images fail the execution

There is one behaviour and it is refusal.

| Condition | Result |
|---|---|
| the provider returns nothing for a needed identity | execution fails, naming the identity |
| the returned bytes fail the frame or parser checks | execution fails, naming the parser refusal |
| the artifact digest does not match the record | execution fails as **stale or substituted** |
| the semantic digest does not match the record | execution fails as **wrong module** |
| the identity requested is not in the closure | the provider cannot express it (§3) |

No retry against another source, because there is no other source (§4). No
degraded mode, no substitution of a "close enough" module, no continuing without
the module. A missing module is a failed execution, and failing is the correct
outcome: the alternative is an execution that continued by finding something
else, which is the failure mode module search produces.

The failure must name **which identity** and **which check**, because a
residency failure that says only "could not load module" is indistinguishable
from a bug in the provider, and an operator cannot tell a corrupted cache from a
missing one.

### 10. A receipt loaded from the same untrusted cache is not a receipt

**A verified-module receipt read out of the image cache is worth nothing.**

If images and receipts come from the same untrusted store, an attacker who can
write an image can write its receipt, and the receipt asserts exactly what the
attacker wants asserted. Accepting one would reduce the verifier to a component
that runs only when the cache misses — which is to say, only when it does not
matter.

So: a receipt may be trusted only when it is anchored by something the untrusted
cache cannot produce. Either

- it is the record this execution's own verifier produced at launch (§2) — the
  case this ADR designs for; or
- it carries a **separate trust anchor** — an independent chain rooted outside
  the cache, verified before the receipt is believed.

This ADR proposes **no such anchor and no cross-boot receipt reuse.** Every
execution verifies its own closure at launch. Cross-boot receipt persistence is
a distinct decision with its own threat model, and it must not be arrived at by
noticing that the record and the receipt happen to have the same fields.

The general rule, which outlives this ADR: **a cache may supply bytes, never
conclusions.**

## What this ADR does not decide

- **Not the engine.** This is a design. No implementation is proposed here, and
  `run_set` is not rebuilt by it.
- **Not the eviction policy** above the one-resident minimum (§7).
- **Not whether a reload re-runs the full verifier** (§5), which needs reload
  frequency measured first.
- **Not the engine's execution representation** — view over an image, or decoded
  structure (§6).
- **Not the grant size.** ADR-0069 stays Proposed and `54 MiB` stays
  provisional. It is re-measured once this is settled, not re-argued.
- **Not cross-boot receipt reuse or a trust anchor for one** (§10).
- **Not a lower conformance profile.** The declared closure ceiling is not
  lowered here, and choosing a cap so that it fits a budget would be choosing a
  conformance profile by its memory bill.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. The canonical form
  remains the source text; an image stays derived, deletable and regenerable.
- **Canonical representation:** unchanged. A module's identity is still computed
  from its meaning by the verifier, never adopted from a stored field.
- **Trusted-base impact:** the trusted record of §2 and the launch-time
  verification loop of §1 are trusted; the provider and everything it returns are
  **not**. The boundary is where it already was — the verifier — and this ADR
  moves what crosses it rather than where it sits.
- **Source-to-runtime impact:** the chain gains an eviction and reload step whose
  every crossing is digest-checked (§5). "Which source, which bytes, which
  verifier" stays answerable at every point, which is the property that must not
  be traded for residency.
- **Recovery and rollback impact:** deleting every image costs speed and no
  functionality — a closure is regenerable from source (AGENTS.md §9). An
  execution in progress whose provider goes away fails (§9); it does not
  continue in a degraded state.
- **Stage identity gate:** none claimed.
- **Threat-model impact:** three new surfaces, each answered above — a provider
  that could widen a closure (§3, §4), a substituted or stale image (§5, §9), and
  a forged receipt from an untrusted cache (§10). The fourth, a parser reading
  untrusted bytes, is ADR-0070's and unchanged: reload parses as untrusted input
  every time.
- **Performance contract:** launch becomes sequential verification of the whole
  closure, which is work that previously happened once per module anyway; what
  changes is that peak memory is one module's rather than the closure's. Reload
  cost is a new term and must be measured, not assumed — a residency scheme that
  thrashes has traded a memory bound for a latency one, and that trade must be
  stated.
- **Compatibility profile:** ADR-0040's machine profile is unchanged.
- **Dependencies, licence, patents:** none.

## Evidence required before this is accepted

Measured on the ADR-0040 reference profile, on ceiling-sized modules, with both
accounts of §8 reported:

1. **launch peak** for closures of 2, 4, 8 and 16 modules under sequential
   verification — the claim being that it is flat in the closure size, and the
   measurement being what actually happens;
2. **trusted record size** per module, and the total for a 256-module closure;
3. **steady-state residency** during execution at bounds of 1, 2 and 4 resident
   images, in both accounts;
4. **reload frequency and cost** on a workload that crosses module boundaries —
   including a deliberately adversarial one that alternates between two modules
   at a bound of one, so the worst case is measured rather than avoided;
5. **eviction under suspension**: a continuation resumed after the module it was
   suspended in has been evicted and reloaded, proving §6 rather than asserting
   it;
6. **negatives**: missing, stale (wrong artifact digest), substituted (wrong
   semantic digest), truncated and out-of-closure requests, each failing the
   execution with the identity and the check named;
7. **the receipt-forgery negative**: a receipt written into the cache alongside a
   substituted image is not accepted (§10).

No number is claimed in this ADR.

## Alternatives considered

**Keep the whole closure resident.** What the implementation does today. Rejected
by measurement: `12.52 MiB` of retained lowered IR per module, against a
whole-machine budget of `256 MiB` shared by four processes. It is not a design;
it is what happens when residency is never decided.

**Verify lazily, on first entry into a module.** Cheaper at launch and refused:
it makes an execution's trusted base depend on the path it took, so two runs of
the same program would have verified different amounts of it. "This program was
verified" must not be a statement about one input.

**Let the provider search a path when it does not have an image.** The
convenience §4 exists to refuse. A provider that can search is a module search
path with a different name, and the closure stops being the closure that was
resolved.

**Trust a receipt found beside the image.** Refused in §10. If the same store
holds both, an attacker who can write one can write the other, and the verifier
becomes a component that runs only when the cache misses.

**Bound residency by bytes only.** Simpler and insufficient: an unbounded number
of small images is unbounded bookkeeping. §7 takes both bounds because either
alone has a defeat.

**Name continuations by pointer into the image.** Faster, and it is the one
choice that would make eviction unimplementable — correct until the first
eviction, which is the worst possible failure schedule. §6 refuses it.
