<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Bounded verified-module residency — what it costs, measured

Evidence level: **P1, locally measured**, through the same instrumented bounded
heap every arena figure in this project is taken through
(`STAGE2_ARENA_BOUND.md`, `STAGE3_PROCESS_GRANT.md`,
`STAGE3_COMPACT_IMAGE_P1.md`).

Scope: the seven measurements **ADR-0071** required before acceptance. **ADR-0071
and ADR-0069 were accepted on 2026-08-27** on this evidence, and
`RUNTIME_GRANT = 54 MiB` is ratified.
ADR-0070 §7's format condition is met by `tos-image`, so production engine
integration is authorized under the terms these three ADRs fix.

Verdict, stated once: **a launch of the exact resolved closure fits
`RUNTIME_GRANT = 54 MiB` — enforced against a bounded allocator whose whole
arena is that size, not estimated — for every conforming closure up to the
published 256-module ceiling (`27.60 MiB`) and for the worst declared resolution
the V1 ceilings admit (`42.42 MiB`), with no lower conformance profile and
nothing that consults free memory. Launch accumulates nothing across the
closure: `5 616 B` of live state carried across sixteen modules, and a frontier
that does not move from the first module's release to the last. What survives a
256-module closure is `182 KiB` — a manifest of closure membership and one
fixed-size record per module. Verification costs nothing above the module it is
verifying. Eviction under suspension works, the adversarial case costs reloads
rather than correctness, and every wrong image is refused. The number that binds
residency is decoded state, not image bytes: at a bound of one, a `0.49 MiB`
image carries `19.33 MiB` of decoded module behind it.**

**Every gate is closed.** The launch peak is enforced — not merely measured —
against a bounded allocator whose whole arena is exactly `RUNTIME_GRANT`, for
every conforming closure up to the published 256-module ceiling *and* for the
worst declared resolution the V1 ceilings admit (§12, §13).

## What was built

A measurement-only harness, `source/tests/residency/`:

- a **launch path** (`src/closure.rs`) that verifies the exact resolved closure
  sequentially, releases each materialized `Module` before decoding the next,
  and builds the trusted records and the closure manifest;
- a **bounded resident set and a measurement engine** (`src/engine.rs`) — not
  `tos-engine`, and nothing is switched onto it. It executes only what the
  fixture needs to cross module boundaries, suspend a caller, evict the module
  it is suspended in, reload it and return into it, and refuses everything else
  by name so a workload cannot quietly measure something other than what it
  claims.

The images are `TOSIMGx0`, experimental version `0`, whose payload coverage is
partial by declaration (`STAGE3_COMPACT_IMAGE_P1.md` §4). ADR-0071's evidence
section permits that, and the reason it is sound is narrow and worth stating:
what is measured here is **residency behaviour** — launch shape, working-set
peaks, eviction, reload, refusal — and none of it depends on which semantic
variants the payload encoder implements. It is not a claim that the format is
ready. It is not a promotion of `TOSIMGx0`.

**Launch is measured in its own process.** The arena's frontier never falls, so
a launch measured after the build phase that produced the images would inherit
that phase's high-water mark and report it as its own. `--prepare` writes the
closure to a directory; `--launch` reads it in a process the frontend never ran
in. The sidecar it reads carries the declared resolution, which is **launch
input** by design (docs/43 §5) — a real launch receives it from the resolution
that produced the closure, exactly as this one does.

## 1. Launch peak, and whether it is flat

Closures of ceiling-sized modules, each measured in a fresh process.

| Closure | Peak frontier | Store (machine) | **Above the store (grant)** | Largest module in flight | Launch |
|---:|---:|---:|---:|---:|---:|
| 2 | 55 841 920 B (53.26 MiB) | 0.86 MiB | **52.39 MiB** | **19.21 MiB** | 222.6 ms |
| 4 | 57 788 608 B (55.11 MiB) | 1.60 MiB | **53.51 MiB** | **19.21 MiB** | 325.4 ms |
| 8 | 61 676 592 B (58.82 MiB) | 3.08 MiB | **55.74 MiB** | **19.21 MiB** | 684.5 ms |
| 16 | 69 337 808 B (66.13 MiB) | 6.01 MiB | **60.11 MiB** | **19.20 MiB** | 1 126.0 ms |

**The largest single module in flight is flat to three digits** — `19.21 MiB` at
2 modules and `19.20 MiB` at 16 — which is the claim ADR-0071 §1 makes: one
module is materialized at a time and released before the next is decoded, so the
peak is a property of the largest module and not of the closure.

But the arena peak above the store rises `52.39 → 60.11 MiB` across an eightfold
closure, and **a flat live working set is not a flat launch bound**: ADR-0069
sizes a grant from the frontier. The first draft of this section explained the
`+7.72 MiB` as arena layout and offered committed-after-launch as the answer.
**Both were wrong**, and §8 replaces them: the growth had two named owners, it
was closure-wide temporary state, and committed-after-launch is not evidence
about a peak.

**§8 supersedes the table above.** It is kept because the phasing it describes
only means something against the numbers it changed.

Launch time is linear in the closure, at about `70 ms` per ceiling-sized module
(decode plus full semantic verification). That is the cost §1 accepts in
exchange for a peak that does not grow with the closure.

## 2. What survives: the record and the manifest, apart

| | Measured |
|---|---:|
| `VerifiedModuleRecord` | **592 B, fixed size, no heap** |
| closure of 16, cross-module links | 15 |
| closure of 16, manifest | 768 B |

The record holds seven 32-byte digests, two bounded names, a profile and ten
envelope limits. Every field is an identity or a bound; none of them is a list.
`size_of` is the whole cost, which is what "fixed shape" has to mean to be worth
saying: releasing a `19.2 MiB` materialized module retains `592 B`.

The manifest holds resolved links and nothing else —
`(caller, function, block, instruction) -> (callee ClosureModuleId, function
index)`, integers throughout. No names survive launch, so nothing has to stay
alive to resolve one, and execution follows links rather than performing lookups.
The launch-time export tables and pending links, `1.53 MiB` of them at 16
modules, were released when the manifest was built — and §8 removes them
altogether, leaving `816 B` of fixed-size pending links as the only thing that
crosses a module boundary during launch.

Extrapolated to the declared 256-module ceiling:

| | Bytes |
|---|---:|
| records, 256 × 592 B | 151 552 (148.0 KiB) |
| manifest, ~240 links | 11 568 (11.3 KiB) |
| **together** | **163 120 B (0.16 MiB)** |

Against a live closure of about `3.1 GiB` at the measured `12.52 MiB` per
retained lowered module.

**The records figure is arithmetic on a fixed size and stands. The links figure
does not.** It extrapolates this fixture's one-call-per-dependency shape, which
is a property of the fixture and not a bound on a conforming closure. §9 derives
the real one, and it is four hundred thousand times larger.

## 3. Steady-state residency at a bound

Eight ceiling-sized modules, each dependency called once, ADR-0071 §7's three
components reported separately.

| Bound | Image bytes | Decoded / view / index | Bookkeeping | **Total (peak)** | Loads | Evictions |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 514 498 B (0.49 MiB) | 20 269 408 B (19.33 MiB) | 2 784 B | **19.82 MiB** | 15 | 14 |
| 2 | 902 668 B (0.86 MiB) | 32 681 360 B (31.17 MiB) | 2 784 B | **32.03 MiB** | 8 | 6 |
| 4 | 1 679 008 B (1.60 MiB) | 57 477 568 B (54.81 MiB) | 2 784 B | **56.42 MiB** | 8 | 4 |

**This is the table ADR-0071 §7 exists for.** At a bound of one, the resident
image is `0.49 MiB` and the decoded module behind it is `20.27 MiB` — a factor of
**39**. A byte bound counted over image bytes alone would have called this
execution "half a megabyte resident" while it held twenty. Bookkeeping is
`2 784 B` and is noise at every bound, which is itself worth recording: the two
components that matter are the image and what was decoded from it.

**Two is the smallest bound that does not thrash.** At bound 1 a sweep of eight
modules costs 15 loads — every call evicts the caller and every return reloads
it. At bound 2 it costs 8, one per module, with no reloads at all, because the
working set of a call is exactly caller plus callee. Raising the bound to 4 buys
nothing on this workload and costs `24 MiB`.

**And a finding ADR-0069 will want.** Per resident ceiling-sized module the cost
is about `19–20 MiB` of decoded state. Against the provisional `54 MiB` grant
that is a bound of **two** comfortably, a bound of **four** not at all
(`56.42 MiB`). Whether the answer is a smaller decoded representation, a
bounded view, or a different grant is not settled here — but "how many modules
fit" now has a measured number behind it instead of an estimate.

### The two ledgers (§8)

At bound 1, eight modules:

| Ledger | Bytes |
|---|---:|
| **process grant** — decoded state, records, manifest, frames, bookkeeping | 22 344 056 B (21.31 MiB) |
| **machine residency** — the image store | 3 231 688 B (3.08 MiB), of which 514 498 B resident |

A resident image shares the store's allocation rather than copying it, which is
the right model — in TOS an image is mapped, not duplicated into the grant — so
the resident image bytes appear in the machine ledger and **not** in the grant.
The two are reported separately and never summed. Moving IR out of the arena
moved it to the other column; it did not make it free.

## 4. The adversarial case: A↔B at a bound of one

Two ceiling-sized modules called alternately, with room for one.

| | |
|---|---:|
| alternations | 8 (16 calls) |
| **loads** | **33** |
| evictions | 32 |
| **evictions of a suspended module** | **16** |
| **reloads of a suspended module** | **16** |
| bytes hashed | 14 952 001 B (14.26 MiB) |
| elapsed | 534.4 ms |
| answer | 24, expected 24 |

The entry is a module too, so at a bound of one **every call evicts the caller
and every return reloads it**: 16 calls produced 33 loads, roughly two per call.
That is the worst case the design admits, measured rather than avoided, and its
cost is about `16 ms` per load — dominated by the parse, not by the hash (the
14.26 MiB of hashing is a few milliseconds of the 534).

The answer is checked. A run that had skipped a module could not have produced
it, so these are the costs of a workload that actually happened.

## 5. Eviction under suspension

The property ADR-0071 §6 rests on, proved by doing it:

```text
closure: entry + one dependency, bound = 1 resident module
entering the dependency evicts the entry, which is suspended in the call

evictions of a suspended module   1
reloads of a suspended module     1
answer                            1, expected 1
```

The caller was evicted while suspended, reloaded from the provider, checked
against its trusted artifact digest, and **returned into**. Nothing in a frame
pointed into the image: a continuation names a `ClosureModuleId`, a function
index, a block index, an instruction index, and its own values. That is what made
the eviction survivable, and it is why §6 refuses pointers into images — a design
with them would have worked in every test that never evicted.

The 8-module bound-1 run in §3 hits the same case seven more times, and the
adversarial run sixteen.

## 6. Negatives

Every one refused, naming the module and the check.

| Input | Refused with |
|---|---|
| no image for a needed identity | `Missing(0)` |
| **stale** — a different, well-formed image of the same module | `ArtifactDigest { module: 0 }` |
| **substituted** — another module's valid image | `ArtifactDigest { module: 0 }` |
| **truncated** image | `ArtifactDigest { module: 0 }` |
| **shifting** — a provider returning different bytes per request | `ArtifactDigest { module: 0 }` |
| **forged record** written beside a substituted image | `ArtifactDigest { module: 0 }` |

Three of these deserve a sentence each.

**Truncated is caught by the digest, not by the parser**, and that is the
intended order: §5 puts the artifact-digest check before parsing, so malformed
input never reaches the reader on a reload. The parser is still total — it has to
be, because at launch it reads genuinely untrusted bytes — but on this path the
cheap check fires first.

**The shifting provider** is the nearest thing this interface can express to a
mutable provider buffer, and the point is that it is not near enough to matter.
The loader requests once and hashes and parses **that** snapshot, so a provider
that changes its mind between requests changes nothing in use. A provider that
could write bytes it had already handed over would be a different story, and
`Snapshot = Arc<[u8]>` is why it cannot: the time-of-check to time-of-use window
§5 forbids is not representable in the type.

**The forged record was never consulted, and could not have been.** `Provider`
has one method and it returns bytes. There is no call by which a record could be
obtained, so ADR-0071 §10 is not a rule this harness follows — it is a sentence
it cannot say.

### Widening has no test, because it has no representation

A request for a module outside the closure is not refused here; it is
**unwritable**. `ClosureModuleId` is minted only by the trusted closure manifest,
its field is private, and the type exposes no constructor. The manifest of a
3-module closure mints three identities, and asking it for a fourth returns
`None`.

That is the difference ADR-0071 §3 is after. A provider keyed by name or digest
would have needed a validation at every call site that ever built one, and safety
would have rested on nobody ever forgetting it. This one has no such site.

## 8. The launch frontier, attributed

The first version of §1 reported a flat *live* working set and a **growing
frontier**: `52.39 → 60.11 MiB` above the store across 2, 4, 8 and 16 modules.
ADR-0069 sizes a grant from the frontier, so that was not a closed bound, and
committed-after-launch was not evidence of a peak. This section takes the same
launch apart at every phase boundary and says who owned the `+7.72 MiB`.

### It was closure-wide temporary state, not allocator residue

Reading committed **and** frontier at each phase, the growth from 2 to 16
modules splits exactly two ways:

| Owner | 2 modules | 16 modules | Growth |
|---|---:|---:|---:|
| export lookup tables (launch scaffolding) | 0.85 MiB | 4.91 MiB | **+4.06 MiB** |
| declared resolution snapshot (verifier input) | 0.77 MiB | 4.43 MiB | **+3.66 MiB** |
| records | 1 184 B | 9 472 B | +8 288 B |
| pending links | 496 B | 3 248 B | +2 752 B |
| **sum** | | | **8 106 030 B** |
| **measured growth above the store** | 52.39 MiB | 60.11 MiB | **8 093 054 B** |

The two agree to `12 976 B` — `0.16 %`. **None of it was allocator residue.** Both
owners hold export **names**, and both scaled with the closure.

### Both were phased out

**The export lookup tables were the harness's own**, kept so that a caller's
import could be resolved against a callee verified earlier. They are gone, by
two changes:

- **verification runs in reverse dependency order — callers before callees.**
  The image order is the topological order resolution produced, so reversing it
  puts every caller ahead of everything it calls. By the time a module is
  reached, every link that will ever name it is already pending, so its export
  table can be built, read and dropped inside its own turn. Nothing is retained
  on the chance that someone later asks;
- **pending links are fixed size.** A module and an export are named by a
  128-bit truncation of the sha-256 of their text, compared against the same
  digest computed from the callee's table while the callee is materialized. No
  string outlives the module it came from. 128 bits rather than 64, because a
  collision here would resolve a call to the wrong function.

**The declared resolution is now sliced per module.** The verifier consults only
the modules a module imports, so a closure-wide snapshot was holding the other
255 for nothing. The shape a launch receives its resolution in is an
input-format question — `tos_verifier::verify` is unchanged and takes the same
type — so the harness writes one slice per module and reads one at a time.

### Re-measured, 2 / 4 / 8 / 16

| Closure | Peak frontier | Store | **Above the store** | Live state accumulated across the closure | **Frontier carried across module releases** |
|---:|---:|---:|---:|---:|---:|
| 2 | 55 535 376 B | 0.86 MiB | **52.10 MiB** | 208 B | **0** |
| 4 | 56 868 320 B | 1.60 MiB | **52.63 MiB** | 240 B | **0** |
| 8 | 59 528 848 B | 3.08 MiB | **53.69 MiB** | 2 928 B | **0** |
| 16 | 64 780 176 B | 6.01 MiB | **55.76 MiB** | 5 616 B | **0** |

Accumulated live state across the closure fell from `4.85 MiB` to `5 616 B`, and
**the frontier does not move at all** from the first module's release to the
last. Launch now carries its high-water mark on the very first module and never
raises it again.

### What the launch bound is made of

| Term | Measured | Scales with |
|---|---:|---|
| one-module scratch — decode | 19.5 – 20.3 MiB | the largest module |
| one-module scratch — **verifier workspace** | **31.75 MiB**, flat at every closure size | the largest module |
| largest single module's import surface | 0.8 → 4.4 MiB | the widest *import list*, not the closure |
| records | 592 B × modules | the closure |
| pending links | ≤ 48 B × widest pending set | cross-module call sites |

```text
launch peak  =  one-module scratch  +  widest import surface  +  bounded closure metadata
```

That is the shape ADR-0071 §1 needs. Two things follow, and neither is
flattering.

**`54 MiB` does not hold launch, and raising the grant is not the answer.** At
two ceiling-sized modules the peak above the store is already `52.10 MiB`, and
at eight it is `53.69 MiB`. But the cause is no longer accumulation — that is
now provably zero — it is that **verifying one ceiling-sized module costs about
`52 MiB` in this implementation, of which `31.75 MiB` is the verifier's own
workspace** sitting on top of a `20 MiB` decoded module. The term to attack is
the verifier's working set, not the grant.

**The residual closure-scaled term is the widest import surface, not the
closure.** It is held only while its own module is verified. In this fixture the
entry imports every dependency, so its slice *is* the closure's export surface
and the term still grows — `0.8 → 4.4 MiB`. A closure in which no single module
imports everything would not show it. Stated rather than hidden, because the
fixture is the worst case for this term and a friendlier fixture would have made
the bound look closed when it is not.

## 9. The manifest holds closure membership, and nothing else

Two earlier forms of §2 were derived against the accepted V1 ceilings, and both
are kept below because the bound is the reason for the final shape.

- **One link per cross-module call site**: `32 256` sites in a conforming
  module, `8 257 536` links at the closure ceiling, **`378 MiB`** on a `256 MiB`
  machine. Raised as a Level-2 question.
- **One entry per declared import slot**: `65 280` edges, **`0.50 MiB`**. It
  fits — but the argument for `255` slots per module leaned on the closure
  ceiling and on `resource imports`, and docs/41's `imports` bounds *transitive
  module dependencies* rather than the count of `import` declarations. A bound
  that needs an argument that thin is not a proved bound.

**Membership needs neither argument.** The permanent manifest holds one **exact
resolved module identity** per closure member, mapped to its opaque
`ClosureModuleId`, and the closure ceiling is a published limit.

The identity is the pair the resolver contract uses — a declared module name and
the content identity it resolved to. `V2012_IMPORT` checks an import against
both: the snapshot must provide the name, and the module's claimed content ID
must agree with what that name resolved to. So membership keys on both, and
nothing here assumes a content ID alone is the whole resolved identity.

**Import slots and call sites are resident derived state.** A caller's verified
artifact already states what its imports resolved to; its call sites already
state which import and which export name they reach. So when a caller becomes
resident its `import slot -> ClosureModuleId` mapping is resolved against
membership, and when a callee becomes resident its `export name -> function
index` index is built inside it. Both live under §7 — inside the byte bound —
and both are released when the module is evicted. Neither is module search:
membership is fixed before the first instruction, a lookup can only answer with a
member, and the provider cannot widen it.

| | Measured |
|---|---:|
| `size_of::<Member>()` | **136 B** |
| `size_of::<VerifiedModuleRecord>()` | **592 B** |
| manifest at the 256-module ceiling | **34 856 B (34.0 KiB)** |
| records at the same ceiling | 151 552 B (148.0 KiB) |
| **permanent launch state, together** | **186 408 B (182.0 KiB)** |
| membership lookup — binary search over 256 fixed records | **134.8 ns** |
| resident `import slot -> id` at the widest importer (255) | 2 040 B, released with the module |

A right name with a wrong content identity does not resolve; a name outside the
closure does not resolve.

**The production crate commits rather than stores.** The harness held the module
name in a fixed-width field, which is a ceiling nothing accepted declares —
docs/44 §2 bounds an *identifier* at 128 bytes and a module name is
`identifier ("." identifier)*`. `crates/tos-residency` keys membership on a full
sha-256 of the exact `(module name, content identity)` pair, length-prefixed so
the boundary between them cannot move, and commits to the source-set identity
the same way. Its record is `464 B` and its member `36 B`, so the figures in
this table are the larger ones and every bound here holds with room to spare. The measured 16-module closure carries **16 members in
2 216 B**, where the call-site form carried 15 links in 816 B and the edge form
15 edges in 268 B — and, unlike either, this one does not grow with what the
modules contain.

**Launch no longer carries anything across a module boundary.** No export table,
no pending link, no name: each module is verified, reduced to its record, and
released; membership is assembled from the records afterwards.

## 9b. Retained: the import-edge and call-site bounds



An earlier version of this section derived the bound for a manifest holding
**one link per cross-module call site** and found it incompatible with the
platform: `32 256` sites in a conforming module, `8 257 536` links at the
closure ceiling, `378 MiB` of them on a `256 MiB` machine. That was raised as a
Level-2 question, and the answer was that the manifest was holding the wrong
thing.

**What the manifest is for is fixing the exact executable closure and the
provider's authority before the first instruction.** That needs one entry per
*declared import slot* — which module a caller's import names. It does not need
to know which function each call reaches: once the module is fixed and resident,
that lookup happens **inside** the module the manifest already chose, and cannot
widen anything. Any `(export name -> function index)` table built for it is
resident module-derived state under §7, inside the byte bound and evicted with
the module.

Derived at the V1 ceilings:

| | |
|---|---:|
| import declarations that fit in one conforming unit | 10 520 (source-derived) |
| the reference profile's cap on imports per module | 256 |
| **the binding constraint — a 256-module closure** | **255** |
| × 256 modules | **65 280 import edges** |
| stored as one `ClosureModuleId` per slot plus per-module offsets | **523 268 B (0.50 MiB)** |
| ADR-0040 whole-machine budget | 268 435 456 B (256 MiB) |

**`0.2 %` of the machine at the absolute worst case**, against `378 MiB` for the
form it replaces — a factor of `126`. The Level-2 question §9 used to raise is
closed by a change of shape rather than by a smaller cap, which is the outcome
that was worth waiting for: no conformance profile was lowered and no ceiling
was touched.

The measured closure of 16 modules now carries **15 import edges in 268 B**,
where the call-site form carried 15 links in 816 B — and, more to the point, a
closure with many calls across few edges no longer pays per call.

### Resolution is exact, not probabilistic

An intermediate version of the launch path named the callee by a **128-bit
truncation** of the sha-256 of its module name, on the argument that a collision
at this population is vanishingly unlikely. That argument is about probability
and what it buys is bytes; what it risks is resolving a call to a *different
function*, silently, in the trusted launch path. It is gone.

A pending edge now carries the **content ID the import itself declares** — an
exact identity the frontend already put in the IR — and resolution is a 32-byte
equality against the callee's own record. Not a name, not a hash of a name, not
a truncation of anything. A wrong match is not improbable; it is impossible.

## 10. Where the verifier's memory goes

§8 found the verifier's own workspace to be the largest single term in a launch
peak. A total cannot say whether that is one large structure or nine modest ones
in sequence, and those have different answers, so each of `VERIFY_STEPS` was
measured — sequentially in one process, and again **one step per process** so
that no step inherits another's high-water mark.

| Step | Survivors | Own frontier (isolated) |
|---|---:|---:|
| `limits` | 0 | **0** |
| `schema` | 0 | **0** |
| `source_identity` | 0 | **0** |
| `table_order` | 0 | **0** |
| `types_and_imports` | 0 | **0** |
| `control_flow` | 0 | **0** |
| `ownership_and_profile` | 0 | **0** |
| `tasks_sync_atomics_unsafe` | 0 | **0** |
| `source_maps` | 0 | **0** |
| **`module_digest`** (before the fix below) | 128 B | **16 515 360 B (15.75 MiB)** |
| `source_map_digest` (before the fix below) | 128 B | 3 809 472 B (3.63 MiB) |

**Nine verification steps out of nine allocate nothing measurable.** The entire
"verifier workspace" is one line: `tos_ir::module_digest`, which builds the whole
canonical byte stream in a `Vec` and then hashes it. The stream for this module
is `5.30 MiB`; the frontier it costs is `15.75 MiB`, because a growing `Vec`
reallocates.

So there was no cumulative scratch to pool and no reusable verifier arena worth
proposing. **The remedy was to hash the canonical stream incrementally instead
of materializing it**, and it is now done.

`tos-ir`'s canonical traversal writes into an abstract sink:

```text
canonical traversal
      |-- Bytes   -> canonical_stream()   (diagnostic, unchanged)
      '-- Digest  -> module_digest()      (feeds sha-256 as it goes)
```

`tag`, `number`, `signed`, `text`, `blob`, `count`, `flag` and the order they are
called in are untouched, and `canonical_stream` goes through the same traversal
so no second canonical encoder exists. A test hashes the diagnostic stream and
compares it against the streamed digest, so the two paths cannot drift.

`tos_verifier::source_map_digest` had the same shape and got the same fix. Once
`module_digest` stopped materializing, that buffer became the whole remaining
cost — `3.63 MiB` — which is exactly the kind of thing a total hides.

Re-measured on the same fixture:

| | Before | After |
|---|---:|---:|
| `module_digest` frontier | 16 515 360 B (15.75 MiB) | **0** |
| `source_map_digest` frontier | (hidden under the above) | **0** |
| **cumulative verifier frontier above the decoded module** | **15.75 MiB** | **0 B** |
| peak over decode + verify | 29 697 360 B (28.32 MiB) | **13 182 000 B (12.57 MiB)** |
| `module_digest` time | 50.75 ms | 45.34 ms |
| `source_map_digest` time | 13.44 ms | 15.47 ms |
| whole verification, nine steps | 2.94 ms | 2.77 ms |

**Verification now costs nothing above the decoded module.** The digest is
unchanged: the ceiling fixture is still `sha256:83954abd…` and its image is
byte-identical at `388 329 B`; every test and both negative suites pass. And the
streaming path is not slower — the earlier expectation that it might be was
about hashing, and what actually dominated was growing and copying a `Vec`.

## 11. The widest single-module import surface

§8's other residue: a module's declared resolution, held while that module is
verified. The fixture's entry imports every dependency, so the term was visible
at `0.8 → 4.4 MiB` — but a fixture is not a bound. Derived at the V1 ceilings:

| | |
|---|---:|
| exports in the densest conforming module | 6 745 (source-derived; the profile caps a table at 65 536) |
| that module's entry in a declared resolution | 644 896 B (0.62 MiB), measured |
| a module may import at most | 255 others |
| widest single-module import surface, as first measured | 164 448 480 B (156.83 MiB) |
| **the same, after §13** | **16 809 600 B (16.03 MiB)** |
| of which export-name text | 8 316 825 B (7.93 MiB) |
| the representation carrying it | **20x**, now **2x** |
| provisional `RUNTIME_GRANT` | 56 623 104 B (54 MiB) |
| ADR-0040 whole-machine budget | 268 435 456 B (256 MiB) |

**As first measured, the widest import surface alone was nearly three times the
provisional grant and 61 % of the whole machine.** §13 records what it is now —
`16.03 MiB` — and what the two representation changes were.

### Re-measured after the representation changed

docs/43 requires the **full** declared snapshot; it does not require a
particular Rust collection. `ResolutionSnapshot` now packs every name — module
names, content identities, export names, capability interfaces — end to end and
holds spans into that text: a sorted module table, a contiguous sorted export
range per module, sorted capability spans. Nothing was dropped. It is not "the
exports the caller happens to use", which would make the verifier's answer
depend on the question asked.

| | Before | After |
|---|---:|---:|
| one module's entry in a declared resolution | 644 896 B | **106 240 B** |
| **widest single-module import surface** | **164 448 480 B (156.83 MiB)** | **27 091 200 B (25.84 MiB)** |
| export-name text inside it | 7.93 MiB | 7.93 MiB |
| representation carrying that text | 20x | **3x** |
| against the provisional `RUNTIME_GRANT` | ~3x over | **fits** |

The verifier answers the same three questions and answers them the same way:
*what identity did this module name resolve to*, *does this exact module export
this exact name*, *is this capability interface declared*. `V2012_IMPORT` and
`V2013_CAPABILITY` are unchanged, and so is the distinction a missing map entry
used to carry — **a module declared without an export surface states nothing
about its exports, and a module declared with an empty one states that it
exports nothing.** That distinction is the single place this change could have
silently altered a verdict, so it is carried by an explicit flag and has its own
test.

### The two things this originally showed, retained

The **content** is `7.93 MiB` of export-name text; the `156.83 MiB` is the
reference `ResolutionSnapshot` — `BTreeMap<String, BTreeSet<String>>` with an
owned `String` per name — carrying it at `20x`. That ratio is the same shape as
the `2.3x` ADR-0070 measured on live IR, and it says the same thing: the number
is a property of a representation, not of the contract.

But the contract is what it is: `tos_verifier::verify` takes a declared
resolution containing every imported module's export names, and this harness
does not change the verifier. **So the remedy is a decision, not a measurement**
— a more compact declared-resolution representation carrying the same
information (export identities rather than owned name text) would be a change to
an accepted verifier interface, and choosing it is not this document's to make.

Both were made, without narrowing what the snapshot says: §10 removed the digest
buffer and §13 separated the export index. `54 MiB` answers for neither now.

## 9a. The manifest's earlier bound, retained



`~11 KiB at 256 modules` was an extrapolation of this fixture's
one-link-per-dependency shape. It is not a V1 bound. A conforming closure may
pack cross-module call sites as densely as its source allows, and the manifest
holds one link per site.

**The IR side supplies no finite bound.** docs/44 §2 bounds "IR
tables/blocks/instructions" by the **declared module resource envelope**, whose
ten fields are `u128` and self-declared. What bounds call sites is the source
unit: every one of them has to be written down.

Measured — the densest cross-module call packing the reference frontend accepts
inside one conforming source unit:

```text
260 134 B of source            32 256 written cross-module call sites
lowered                        32 256 Op::Call{Imported}, 64 449 instructions,
                               64 575 source-map entries
```

| | |
|---|---:|
| cross-module call sites in one conforming module | 32 256 |
| × 256-module closure ceiling | **8 257 536 links** |
| × `size_of::<Link>()` = 48 B | **396 361 728 B (378 MiB)** |
| ADR-0040 whole-machine budget | 268 435 456 B (256 MiB) |

**A conforming closure may require a manifest larger than the reference
platform's entire memory.** Compaction does not rescue it: an all-`u32` link is
24 bytes and still `189 MiB`, on a machine that must also hold a nucleus and
four processes. Fitting `8 257 536` links into any plausible budget needs about
two bytes each, which no encoding of five indices provides.

**This is brought as a Level-2 question and not answered here.** docs/44 §2
permits a lower cap only in a declared conformance profile, and choosing a
conformance profile by its memory bill is a decision, not a measurement. The
options are visibly different in kind — bound call sites per module, bound links
per closure, make the manifest partially resident under the same discipline as
module images, or declare a lower profile — and they are not interchangeable.

### A defect found on the way

The first attempt packed all 32 738 call sites into one expression, which is
inside every published limit: under 256 KiB, delimiter nesting of one,
identifiers far under 128 bytes. **It aborted the process with a stack
overflow** in `Parser::parse_schema`. Measured threshold on this host: a chain of
about 14 000 terms parses, 15 000 does not.

docs/44 §2 says these limits "prevent attacker-controlled recursion" and that
gross count and depth limits are checked before expensive work. A crash is not a
rejection. The harness routes around it by spreading call sites across many
functions — which costs almost nothing in density, `8.4` bytes per site against a
theoretical `9` — and the defect is recorded here rather than worked around
silently. It is not fixed in this change.

## 12. Launch, re-measured — and enforced

After §9's manifest, §10's streaming digest and §11's packed declared
resolution, launch was measured again in fresh processes, and then **run against
a bounded allocator whose whole arena is exactly `RUNTIME_GRANT = 54 MiB`**. A
measured bound and an enforced one are different claims; a `--features grant`
build makes the second, and a launch that does not fit fails to allocate rather
than reporting a number.

**Image bytes are outside the measured arena**, on the host allocator. That is
ADR-0071 §8's accounting, not a convenience: in TOS an image is capsule or cache
storage mapped into the address space, not memory taken from
`RuntimeMemoryGrantV1`. Leaving them inside would have made a grant bound
untestable.

| Closure | Grant frontier | Largest module in flight | Records | Manifest | Machine residency (store) | Launch |
|---:|---:|---:|---:|---:|---:|---:|
| 2 | **19.68 MiB** | 19.21 MiB | 1 184 B | 312 B | 0.86 MiB | 0.18 s |
| 16 | **20.10 MiB** | 19.20 MiB | 9 472 B | 2 216 B | 6.01 MiB | 1.09 s |
| 64 | **21.57 MiB** | 19.13 MiB | 37 888 B | 8 744 B | 23.54 MiB | 4.17 s |
| **256** | **27.60 MiB** | 18.84 MiB | 151 552 B | 34 856 B | 92.88 MiB | 16.24 s |
| **256, worst declared resolution** | **42.42 MiB** | 18.84 MiB | 151 552 B | 34 856 B | 92.88 MiB | 17.29 s |

**Every one of these launches inside a hard `54 MiB` arena**, at the published
closure ceiling, with no lower conformance profile and no memory-dependent
behaviour anywhere: the grant is a constant, evictions are driven by declared
bounds, and nothing consults free memory.

Against §8's first measurement of the same shape — `52.10 MiB` above the store at
two modules, `55.76 MiB` at sixteen — the three changes together take a
16-module launch from `55.76 MiB` to `20.69 MiB`, and buy the whole range from
32 to 256 modules that could not be measured before.

Launch time is linear at about `64 ms` per ceiling-sized module.

## 13. The worst-case declared resolution, and what it took to fit

A module may import 255 others, and each of those may export as much as a
conforming source unit allows — measured, `6 745` exports in `32 615 B` of names.
A resolver over such a closure hands the launch exactly that, and docs/43
requires the **whole** surface, not the part the caller uses.

Measured by widening the 256-module fixture's declared resolution to that
surface, at that density:

| | First measurement | After the export index was separated |
|---|---:|---:|
| the widest importer's slice, live | 38 584 176 B (36.79 MiB) | **24 258 144 B (23.13 MiB)** |
| grant frontier | 58 804 784 B (56.08 MiB) | **44 478 752 B (42.42 MiB)** |
| under the hard `54 MiB` build | **failed to allocate** | **passes** |

Three things account for the difference, and none of them touched what the
snapshot says.

**Export names moved out of the shared string table into their own.** An export
now costs **one `u32`** and nothing else: names are packed in order, so a name
runs from its own offset to the next one and no length is stored. A
`{start, length}` pair is eight bytes — `size_of` says so, and narrowing the
second field would not have helped, because alignment pads it back. Removing the
field is what a narrower field could not do. Measured: **`4.00 B` of metadata per
export**, `32 615 B` of names and `26 984 B` of metadata for the densest module,
`65 847 B` of `heap_bytes` in all.

**A sorted surface is accepted as it arrives.** `build` checks whether each
module's exports are already in order and only rewrites the export text when
they are not. A stored declared resolution carries them sorted, so the worst
case costs no rewrite — which is what had been doubling the transient peak.

**Exact reservation.** A reader knows the sizes before it starts, so the snapshot
is built without a reallocation.

Nothing about the *facts* changed: the full export surface of every declared
module, the `surface absent` / `surface present but empty` distinction, exact
byte equality, sorted lookup, and identical `V2012_IMPORT` and `V2013_CAPABILITY`
verdicts. Build is `0.22 ms` and lookup `297.9 ns` over 6 745 sorted names for
the densest module.

The widest single-module import surface is now **16 809 600 B (16.03 MiB)** at
`2x` over the `7.93 MiB` of name text it carries — from `156.83 MiB` at `20x`
where this started.

## 7. What this does not settle

- **Not production integration.** ADR-0070 §7's gate stands. The images are
  `TOSIMGx0`, coverage is partial by declaration, and none of this is a claim
  that the format is ready.
- **Not the engine.** The interpreter here executes what the fixture needs and
  refuses the rest by name. It is not `tos-engine` and is not proposed as one.
- **Not eviction policy.** Least-recently-used above the one-resident minimum,
  because something had to be chosen to measure; ADR-0071 leaves the policy open
  and this does not close it.
- **Not the decoded representation.** The `39x` between image and decoded state
  is this materializing reader's, and a bounded view would change it. What is
  measured is what this reader costs.
- **Not the grant size.** `54 MiB` stays provisional. §3 gives it a number to be
  decided against, not a decision.

## Reproduction

From `source/`, on the host. Launch is prepared and measured in separate
processes on purpose:

```sh
for n in 2 4 8 16; do cargo run --release -p tos-residency-prototype -- --prepare --modules $n --dir target/res$n; cargo run --release -p tos-residency-prototype -- --launch --dir target/res$n; done
```

```sh
cargo run --release -p tos-residency-prototype -- --sizes --modules 16
```

```sh
for b in 1 2 4; do cargo run --release -p tos-residency-prototype -- --residency --modules 8 --bound-count $b; done
```

```sh
cargo run --release -p tos-residency-prototype -- --adversarial --repeats 8
```

```sh
cargo run --release -p tos-residency-prototype -- --eviction
```

```sh
cargo run --release -p tos-residency-prototype -- --negatives
```

§9's import-edge bound, §10's verifier breakdown and §11's import surface:

```sh
cargo run --release -p tos-residency-prototype -- --manifest-bound
```

```sh
cargo run --release -p tos-image-prototype -- --verify-steps target/ceiling.tosimg0
```

```sh
cargo run --release -p tos-residency-prototype -- --import-surface
```

The frontier attribution of §8, per closure size:

```sh
for n in 2 4 8 16; do cargo run --release -p tos-residency-prototype -- --attribute --dir target/res$n; done
```


