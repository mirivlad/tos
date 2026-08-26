<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Bounded verified-module residency — what it costs, measured

Evidence level: **P1, locally measured**, through the same instrumented bounded
heap every arena figure in this project is taken through
(`STAGE2_ARENA_BOUND.md`, `STAGE3_PROCESS_GRANT.md`,
`STAGE3_COMPACT_IMAGE_P1.md`).

Scope: the seven measurements **ADR-0071** requires before it can be accepted.
ADR-0071 remains **Proposed**. `RUNTIME_GRANT = 54 MiB` remains **provisional**.
**ADR-0070 §7's implementation gate stands**: nothing here authorizes production
engine integration, which waits on an image format covering 100 % of
`tos-ir/v1` and closing docs/43 §1 in full.

Verdict, stated once: **sequential launch is flat in the closure size — the peak
is one module's working set (`19.2 MiB`), not the closure's — and what survives
a 256-module closure is `0.16 MiB` of records and links against a live closure of
about `3.1 GiB`. Eviction under suspension works, the adversarial case costs
reloads rather than correctness, and every wrong image is refused. The number
that binds residency is decoded state, not image bytes: at a bound of one, a
`0.49 MiB` image carries `19.33 MiB` of decoded module behind it.**

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

The arena peak above the store rises `52.39 → 60.11 MiB` across an eightfold
closure: `+14.7 %` for `+700 %` of modules. **That residue is arena layout, not
retention**, and the proof is committed bytes rather than an argument:

```text
16-module closure, arena after launch:  committed 6 319 088 B
                          the store is  6 305 109 B
                                  left     13 979 B  = records (9 472) + manifest (768) + slack
```

Launch ends holding the store, the records and the manifest, and nothing else.
A retaining path would have ended holding sixteen materialized modules — about
`307 MiB` at the measured `19.2 MiB` each.

Launch time is linear in the closure, at about `70 ms` per ceiling-sized module
(decode plus full semantic verification). That is the cost §1 accepts in
exchange for the flat peak.

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
modules, are released when the manifest is built.

Extrapolated to the declared 256-module ceiling:

| | Bytes |
|---|---:|
| records, 256 × 592 B | 151 552 (148.0 KiB) |
| manifest, ~240 links | 11 568 (11.3 KiB) |
| **together** | **163 120 B (0.16 MiB)** |

Against a live closure of about `3.1 GiB` at the measured `12.52 MiB` per
retained lowered module. The links figure is an extrapolation of this fixture's
call pattern — one cross-module call per dependency — and is labelled as one; the
records figure is arithmetic on a fixed size and is not.

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
