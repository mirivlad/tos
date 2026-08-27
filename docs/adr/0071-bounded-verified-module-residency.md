<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0071: Bounded verified-module residency and the module provider

- Status: **Accepted**
- Date: 2026-08-26 (accepted 2026-08-27)
- Decision level: 2 — it fixes how many verified modules an execution may hold
  at once, what supplies the rest, and what survives a module image being
  released. It changes no TOS Core semantics, no ABI operation and no invariant
- Project Architect approval: **given, 2026-08-27**
- Evidence: `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md` (this ADR's own
  evidence gate, measured), `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`, `docs/evidence/STAGE2_ARENA_BOUND.md`
- Related: ADR-0070 (Accepted) §5, which requires this decision and constrains
  its shape; ADR-0069 (Accepted) — the `54 MiB` grant this is measured against;
  ADR-0040 — the whole-machine budget both accounts are spent from
- Note: amended four times before acceptance — reload trust (§5), the
  record/manifest split (§2), the opaque provider key (§3), what the byte bound
  counts (§7), and finally the manifest's reduction to closure membership. The
  amendment history is kept in §2a and §11 because each shape was replaced for a
  measured reason, not a stylistic one

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
- **verification is complete before execution starts**, because two things must
  exist before the first instruction and neither can be built incrementally:
  the **complete trusted closure manifest** of §2 — the membership of the exact
  resolved closure, which is not membership until the last module of it has been
  verified — and the **exact executable closure and provider authority**, which
  is fixed at that moment and never widened afterwards. An execution that
  verified as it went would be deciding, mid-run, what it is allowed to reach.

An execution whose closure cannot be verified in full does not start. There is
no partial launch, and no "verify the rest when we get there".

### 2. What survives: a fixed-shape module record, and a closure manifest beside it

Two structures, and they are separate because one of them is fixed-shape and the
other cannot be.

**`VerifiedModuleRecord` — fixed shape, one per module.** When a module's
materialized `Module` is released, this is what remains in trusted memory for
the lifetime of the execution:

- the **semantic module digest**, as the verifier computed it from the
  reconstructed module;
- the **artifact digest** of the exact bytes the verifier read;
- the **verifier identity** that produced the receipt;
- the module's **name, source set, content ID and dependency digest**;
- the **profile, resource envelope and capability-interface digest**;
- the **source-map digest**.

**No export surface, and no variable-length field of any kind.** An export list
grows with the module, so a record carrying one would not be fixed-shape — and
"fixed-shape" would have been a word rather than a property. Every field above is
an identity, a digest or a bounded envelope, so the record's size is a constant
of the design rather than a function of the module. That is what makes the whole
scheme work: releasing a `Module` must free `12–15 MiB` and retain a known
handful of bytes.

This is deliberately close to the existing `VerifiedModule` receipt, because that
is already exactly the shape "this module passed this verifier" takes. What this
ADR adds is a **lifetime**: the record outlives the image and the materialized
module, and is what everything later compares against.

**`VerifiedClosureManifest` — built once, after the whole closure is verified.**
It holds the closure's **membership** and nothing else: one **exact resolved
module identity** per member, mapped to its opaque `ClosureModuleId`.

The identity is the pair the resolver contract uses — a declared module name and
the content identity that name resolved to. `V2012_IMPORT` checks an import
against both, so membership keys on both. A content ID alone is nowhere promised
to be the whole resolved identity, and this decision does not assume it is.

**Import slots and call sites are not in the manifest.** A caller's verified
artifact already states what its imports resolved to, and its call sites already
state which import and which export name they reach. Copying either into a
permanent structure would make the manifest grow with something it does not
decide. So:

- when a **caller** becomes resident, its `import slot -> ClosureModuleId`
  mapping is resolved against this membership;
- when a **callee** becomes resident, its `export name -> function index` index
  is built inside it.

Both are **resident module-derived state under §7** — inside the byte bound, and
released when the module is evicted. Neither is module search: membership is
fixed before the first instruction, a lookup can only answer with a member, and
the provider cannot widen it (§3).

The manifest is therefore bounded by the **closure ceiling and by nothing else**
— at most 256 members. Measured at that ceiling: `34 856 B` of manifest and
`151 552 B` of records, `182 KiB` of permanent launch state together, against a
whole-machine budget of `256 MiB`.

Two earlier forms are recorded because the bound is the reason for the shape.
One link per **call site** came to `8 257 536` links (`378 MiB`) at the V1
ceilings — larger than the machine. One entry per **import slot** came to
`65 280`, which fits, but `resource imports` in docs/41 bounds transitive module
dependencies rather than the count of `import` declarations, so that bound was
not proved. Membership needs neither argument: the closure ceiling is a
published limit and it is the only one in play.

It can only be built after every module is verified, which is the other half of
why §1 is eager: membership is the set of identities the verifier itself
produced, and the executable closure is not fixed until the last of them is.

**Nothing crosses a module boundary during launch.** No export table, no pending
link, no name: each module is verified, reduced to its fixed-size record, and
released, and membership is assembled from the records afterwards.

The manifest is the **only** thing that mints a `ClosureModuleId` (§3).

**Neither structure is ever reloaded from a cache.** Both are produced by this
execution's own launch, and there is no path by which stored copies can be
adopted (§10).

### 3. The provider takes an opaque `ClosureModuleId`, so widening is not expressible

The engine is **handed** a provider. It never has one by default, never
constructs one, and never reaches for one.

- the provider is an **explicit argument** to the execution — not a global, not
  an ambient default, not a field the engine fills in when the caller left it
  empty;
- its key is an **opaque `ClosureModuleId`**, **minted only by the trusted
  closure manifest of §2**. Not a module name, not a path, not a content ID, not
  a semantic digest — nothing an attacker or a well-meaning caller can construct,
  parse, guess or derive from text;
- the provider **cannot enumerate**. There is no "list what you have", because
  a component that can enumerate can be asked what else exists, and what else
  exists is not this execution's business.

**Widening is refused structurally rather than by a check.** A request for a
module outside the closure is not rejected — it cannot be *written*, because
there is no `ClosureModuleId` for it and no way to make one. A design that took
a name and validated it against a list would be one forgotten call site away
from module search; this one has no failure mode of that shape, because the
dangerous request has no representation.

The identifier says only "the *n*-th module of this execution's verified
closure". It carries no meaning outside this execution and grants nothing outside
it.

What the provider cannot be prevented from doing is returning **wrong bytes** for
a right identity. That is a real and expected failure, and it is what §5's
artifact-digest check exists for.

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

### 5. Reload is byte identity against the trusted record, not re-verification

**Full semantic verification happens exactly once, at launch (§1).** The trusted
record holds the **exact artifact digest** of the image the verifier read. On
reload:

1. the provider returns an **immutable artifact snapshot** — a byte sequence
   that cannot change after it is handed over;
2. **SHA-256 of that exact snapshot** is computed and compared against the
   trusted artifact digest, **before any parsing**;
3. a mismatch fails the execution (§9). A match means these are the bytes the
   verifier already traversed, so they are reused as verified;
4. the semantic module digest is **not** recomputed and the full verifier does
   **not** run again. Neither is a source of trust here, because trust was
   established at launch and is carried by the artifact digest.

**Why byte identity is enough, and only here.** The artifact digest is a
commitment to one exact byte sequence. Second-preimage resistance is precisely
the property that a matching digest means the same bytes, and the same bytes
decode to the same module — the round-trip invariant ADR-0070 §6 measured. So
re-verifying would be re-deriving a conclusion this execution already reached
about this exact input. This reasoning holds **only** because the record was
produced by this execution's own verifier at its own launch; it does not extend
to a record from anywhere else, which is §10.

**The snapshot must be immutable, and this is not a detail.** The bytes that are
hashed and the bytes that are subsequently parsed, viewed and executed **must be
one immutable snapshot**. A provider that returned a mutable buffer — one it, or
anything else, could still write to — would create a time-of-check to
time-of-use window in which verified bytes are hashed and different bytes are
run. That is not a weakness of the digest; it is the digest being applied to a
different object than the one that gets used. A provider returning a mutable
buffer is a provider that does not satisfy this ADR.

**The parser stays total, and fails closed, regardless.** A hash match never
licenses a faster path through the reader: no bounds check is skipped, no length
is trusted, nothing is read past a slice. Two reasons, and either alone is
sufficient. At launch the same parser reads genuinely untrusted input, and a
reader with two modes would eventually be entered in the wrong one. And a parser
whose safety depended on the digest having matched would be unsafe in exactly the
case the digest is there to catch.

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

### 7. Residency is bounded by count and by bytes — and the bytes are all module-derived state

An execution declares, or is given, two bounds:

- a **maximum number of simultaneously resident modules**;
- a **maximum total bytes** of resident **module-derived state**.

Both, because either alone is defeatable. A count alone lets a few large modules
exceed any byte budget; a byte budget alone permits an unbounded number of tiny
ones, and every resident module costs bookkeeping as well as bytes.

**The byte bound counts everything a resident module keeps alive, not the image.**
Three components, measured and reported separately:

| Component | What it is |
|---|---|
| **image bytes** | the encoded artifact held in memory |
| **decoded / view / index state** | whatever the engine built from it to execute — a decoded structure, a view with its offset tables, any index or cache derived from the image |
| **bookkeeping** | the residency table's own per-resident-module cost |

A bound satisfied by the first column alone would be no bound at all. The
measurement that makes this concrete is already in hand: the image is `0.37 MiB`
and the materialized module is `12.19 MiB` — so "under the byte bound" while
retaining a decoded `Module` behind each image would be a factor of thirty-three
of self-deception. **Whatever the engine keeps alive on behalf of a resident
module is inside the bound**, and evicting a module means releasing all three
components, not dropping the image and keeping what was built from it.

Which of the three lands in which ledger is then §8's question, and the three are
reported separately precisely so that the split can be stated rather than
assumed.

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

Each of §7's three components is placed in one ledger or the other, explicitly.
Decoded and view state built inside the process's arena is grant; image bytes
mapped from a store outside it are machine residency; bookkeeping goes where it
is actually allocated. The placement is *reported*, never assumed, because it is
exactly the step at which a saving can be claimed twice.

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
| the provider returns nothing for a needed `ClosureModuleId` | execution fails, naming the identity |
| the returned snapshot's SHA-256 does not match the trusted artifact digest | execution fails as **stale, corrupted or substituted**, before parsing |
| the snapshot is mutable, or is not the object subsequently read | the provider does not satisfy §5; execution fails |
| the returned bytes fail the frame or parser checks | execution fails, naming the parser refusal |
| a module outside the closure is wanted | unrepresentable — there is no `ClosureModuleId` for it (§3) |
| **at launch**, a module's semantic digest or verification fails | execution never starts (§1) |

No retry against another source, because there is no other source (§4). No
degraded mode, no substitution of a "close enough" module, no continuing without
the module. A missing module is a failed execution, and failing is the correct
outcome: the alternative is an execution that continued by finding something
else, which is the failure mode module search produces.

Note where the two digest failures live. **Semantic** mismatch is a launch-time
condition, because launch is where semantic verification happens; **artifact**
mismatch is the reload-time condition, because reload asks only whether these
are the same bytes (§5). A design that reported semantic mismatch on reload would
be describing a check it had decided not to perform.

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
- **Not the engine's execution representation** — view over an image, or decoded
  structure (§6).
- **Not the grant size.** ADR-0069 stays Proposed and `54 MiB` stays
  provisional. It is re-measured once this is settled, not re-argued.
- **Not cross-boot receipt reuse or a trust anchor for one** (§10).
- **Not a lower conformance profile.** The declared closure ceiling is not
  lowered here, and choosing a cap so that it fits a budget would be choosing a
  conformance profile by its memory bill. §2a states the arithmetic that makes
  the question unavoidable and leaves the answer to a Level-2 decision.
- **Not the verifier's working set**, which is no longer a term at all: the
  nine verification steps allocate nothing above the module they check, and the
  digest buffer that used to dominate the launch peak is gone (§2c).

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
  every crossing is artifact-digest-checked against a record this execution's own
  verifier produced (§5). "Which source, which bytes, which verifier" stays
  answerable at every point, which is the property that must not be traded for
  residency.
- **Recovery and rollback impact:** deleting every image costs speed and no
  functionality — a closure is regenerable from source (AGENTS.md §9). An
  execution in progress whose provider goes away fails (§9); it does not
  continue in a degraded state.
- **Stage identity gate:** none claimed.
- **Threat-model impact:** three new surfaces, each answered above — a provider
  that could widen a closure (§3, §4), a substituted or stale image (§5, §9), and
  a forged receipt from an untrusted cache (§10), and a mutable provider buffer
  opening a time-of-check to time-of-use window between the hash and the
  execution (§5). The parser reading untrusted bytes is ADR-0070's and unchanged:
  it is total and fails closed on every path, and a matching artifact digest
  never licenses a faster one.
- **Performance contract:** launch becomes sequential verification of the whole
  closure, which is work that previously happened once per module anyway; what
  changes is that peak memory is one module's rather than the closure's. Reload
  cost is a new term and must be measured, not assumed — a residency scheme that
  thrashes has traded a memory bound for a latency one, and that trade must be
  stated.
- **Compatibility profile:** ADR-0040's machine profile is unchanged.
- **Dependencies, licence, patents:** none.

## Evidence required before this is accepted

Measured on ceiling-sized modules, with §7's three components reported
separately and §8's two ledgers reported separately:

1. **launch peak** for closures of **2, 4, 8 and 16** modules under sequential
   verification — the claim being that it is flat in the closure size, and the
   measurement being what actually happens;
2. **`VerifiedModuleRecord` size** per module and **`VerifiedClosureManifest`
   size** for the closure, both extrapolated to **256 modules**, reported apart
   from each other — the record is fixed-shape and the manifest is not, so one
   number covering both would hide which one grows;
3. **steady-state residency** during execution at bounds of **1, 2 and 4**
   resident modules, with image bytes, decoded/view/index state and bookkeeping
   each shown, and each placed in a ledger;
4. **reload frequency and cost** on a workload that crosses module boundaries,
   **including the adversarial A↔B case at bound = 1** — two modules calling each
   other with room for one, so the worst case is measured rather than avoided;
5. **eviction under suspension**: a caller suspended inside module A, A evicted,
   A reloaded, and the call **returned into** — proving §6 rather than asserting
   it;
6. **negatives**: a missing snapshot, a stale one, a substituted one, a truncated
   one, and a mutable-buffer provider, each failing the execution with the
   identity and the check named;
7. **the receipt-forgery negative**: a record or receipt written into the cache
   beside a substituted image is not accepted (§10).

### What the evidence harness may be

The measurements may use a **measurement-only provider and engine harness** over
the subset `TOSIMGx0` covers. That is a deliberate and bounded concession: what
is being measured here is residency behaviour — launch shape, working-set peaks,
eviction, reload, refusal — and none of those depend on which semantic variants
the payload encoder happens to implement.

It changes nothing about `TOSIMGx0`. It remains an experimental version `0` with
partial coverage, it is not promoted, and **ADR-0070 §7's implementation gate
stands**: production engine integration waits on a format that covers 100 % of
`tos-ir/v1` and closes docs/43 §1 in full. Evidence taken on a subset is evidence
about residency, never a claim that the format is ready.

No number is claimed in this ADR. They are in
`docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`, measured against this list, and
the status stays **Proposed** until the Project Architect has read them.

**All seven are met, and the launch bound is enforced rather than estimated.**
A bounded allocator whose whole arena is exactly `RUNTIME_GRANT = 54 MiB` runs
every conforming closure to the published 256-module ceiling and the worst
declared resolution the V1 ceilings admit:

| | Grant frontier under a hard `54 MiB` arena |
|---|---:|
| closure of 2 ceiling-sized modules | 19.68 MiB |
| closure of 16 | 20.10 MiB |
| closure of 64 | 21.57 MiB |
| **closure of 256 — the published ceiling** | **27.60 MiB** |
| **256, with the worst declared resolution admissible** | **42.42 MiB** |

Launch accumulates nothing across a closure — `5 616 B` of live state at sixteen
modules, and a frontier that does not move from the first module's release to the
last. What survives a 256-module closure is `182 KiB`. Verification costs nothing
above the module it is verifying.

No lower conformance profile was introduced, no ceiling was changed, and nothing
in the path consults free memory.

All seven are closed, and the launch bound is enforced rather than estimated:
the table above is a bounded allocator whose whole arena is exactly the grant,
not a measurement taken beside one.

### 2a. The manifest's upper bound, derived, and the two shapes it replaced

**Historical.** The bound below is current; the two forms after it were replaced
and are kept because a bound is only meaningful against what it improved on.



| | Measured |
|---|---:|
| `size_of::<Member>()` | 136 B |
| `size_of::<VerifiedModuleRecord>()` | 592 B |
| manifest at the 256-module ceiling | **34 856 B (34.0 KiB)** |
| records at the same ceiling | 151 552 B (148.0 KiB) |
| **permanent launch state, together** | **186 408 B (182.0 KiB)** |
| membership lookup, binary search over 256 | 134.8 ns |
| resident `import slot -> id` at the widest importer | 2 040 B, released with the module |

Bounded by the closure ceiling and by nothing else.

#### Superseded: the import-edge form

One entry per declared import slot came to `65 280` edges, `0.50 MiB`. It fit,
but the argument for 255 slots per module leaned on docs/41's `resource imports`,
which bounds transitive module dependencies rather than the count of `import`
declarations. Membership needs no such argument: the closure ceiling is a
published limit and is the only one in play.

#### Superseded: the call-site form

docs/44 §2 bounds "IR tables/blocks/instructions" by the **declared module
resource envelope**, whose ten fields are `u128` and self-declared, so the IR
side gives no finite bound. What bounds cross-module call sites is the source
unit, because each one has to be written down. Measured, the densest packing the
reference frontend accepts inside one conforming `256 KiB` unit is **32 256 call
sites**, so:

| | |
|---|---:|
| links in one conforming module | 32 256 |
| × the 256-module closure ceiling | **8 257 536** |
| × `size_of::<Link>()` = 48 B | **378 MiB** |
| ADR-0040 whole-machine budget | **256 MiB** |

**A conforming closure would have required a manifest larger than the whole
machine.** An all-`u32` link is 24 bytes and still `189 MiB`. That was raised as
a Level-2 question about which contract had to give — and the answer was that
neither did: the manifest was holding the wrong thing. Which function a call
reaches is not part of fixing a closure's identity or a provider's authority,
which is what a manifest is for.

### 2c. Two launch-memory owners, both closed

Also historical, and recorded because each was a real bound before it was one no
longer. `module_digest` materialized the canonical stream before hashing it —
`15.75 MiB` on a ceiling-sized module, and the whole of the verifier's workspace,
since the other nine verification steps allocate nothing; it now feeds the digest
incrementally through the same traversal, with the digest unchanged byte for
byte. And the widest single-module declared-resolution surface was `156.83 MiB`,
of which `7.93 MiB` was export-name text; separating the export index from the
shared string table brought it to `16.03 MiB` without narrowing what the snapshot
states. Both are in `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md` §10 and §13.

## Alternatives considered

**Keep the whole closure resident.** What the implementation does today. Rejected
by measurement: `12.52 MiB` of retained lowered IR per module, against a
whole-machine budget of `256 MiB` shared by four processes. It is not a design;
it is what happens when residency is never decided.

**Verify lazily, on first entry into a module.** Cheaper at launch and refused
by §1: the closure manifest cannot be built from modules not yet verified, and
the executable closure and the provider's authority would then be settled
progressively, by the run, rather than fixed before it starts. An authority that
grows as a program executes is not a bounded authority.

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

**Count the byte bound over image bytes alone.** Rejected in §7, and it is the
easiest mistake in the design to make, because the image is the thing with an
obvious size. It would let an execution report itself inside a byte bound while
holding a `12.19 MiB` decoded module behind each `0.37 MiB` image — a bound
satisfied by measuring the wrong column.

**Re-run the full verifier on every reload.** Considered and not taken (§5). It
costs a re-derivation of a conclusion this execution already reached about this
exact byte sequence, which a matching artifact digest already establishes. What
is *not* traded away for that saving: the parser stays total, the snapshot must
be immutable, and the reasoning holds only for a record this execution's own
verifier produced.

**Key the provider by module name or digest.** Refused in §3. Both are
constructible from text, so a widened request becomes expressible and safety
reduces to a validation call at every site that ever builds one. An opaque
identifier minted only by the trusted manifest has no such site.

**Name continuations by pointer into the image.** Faster, and it is the one
choice that would make eviction unimplementable — correct until the first
eviction, which is the worst possible failure schedule. §6 refuses it.
