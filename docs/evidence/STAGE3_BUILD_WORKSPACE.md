<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 build workspace — what the build account costs, apart from what it hands over

Evidence level: **P1, locally measured on the host arena harness**, the same
instrument `STAGE2_ARENA_BOUND.md` and `STAGE3_PROCESS_GRANT.md` use: the
production path runs *through* `tos_runtime`'s bounded heap, so every figure is
the allocator's own accounting rather than a sum of requests.

Producer: `source/tests/arena-bound --build --modules N --shape S`, one count
per process, with `--lowering --modules 256 --shape S` for the attribution and
`--capsule` for the same measurement over a real capsule-backed source set. The
first calls `tos_pipeline::build_from_provider` and then `tos_pipeline::admit` —
the two calls ADR-0073 §1 separates — so the line between the build account and
the target account is a return, not an estimate.

**The current figure is the one measured with the products written outside the
workspace**, which is the arrangement ADR-0074 §1 records as decided:
`77.14 MiB` for the worst measured shape, against a bundle of `100.87 MiB`. The
sections up to "With the products written outside the workspace" measure the
earlier arrangement, in which the products accumulated inside the build's own
allocator; they are kept because the decision was taken from them.

Verdict for that earlier arrangement, at the docs/44 closure ceiling of
**256 ceiling-sized modules**:

- the build account's high-water mark is **`172.15` (chain), `170.25` (wide
  fan-in) and `176.86 MiB` (balanced)**;
- **`104.9 MiB` of that survives the workspace**: the image closure
  (`92.5–92.9 MiB`) and the declaration handed with it (`12.11 MiB`);
- the workspace's own transient state is one turn's scratch at `25.21 MiB`, the
  verification surfaces at `12.6 MiB`, the live lowering views (`0.06` to
  `14.78 MiB`, by shape) and the closure plan at `0.14 MiB`;
- the rest is the allocator's high-water mark above the live bytes, and it grows
  with the number of images retained beside the churn.

**These figures postdate a change this evidence caused.** The tables below were
measured at `4204f32`, when the build kept a `ModuleSummary` per module for the
whole build and the closure plan was `52.1 MiB` — its largest transient owner.
The set-wide check is the last reader of a summary's type surface, so the build
now consumes summaries into `ModulePlan` when that check returns, and the plan
costs `0.14 MiB` instead. The measured effect at the ceiling is a build account
`32.22`, `48.98` and `25.44 MiB` smaller for the three shapes. Everything below
is the earlier measurement, kept because the slopes and the attribution are what
the reduction was derived from; the last section reports the current numbers.

Nothing here chooses a `BuildWorkspace` size, a region, an owner or a lifetime.
Those are open decisions; this is the measurement they were waiting for.

## What the two accounts are, and where the line is

ADR-0073 §1 puts the frontend outside the target process and keeps the verifier
inside it. Since `3a7b70f` the code says the same thing in two calls:

```text
build_from_provider   read, parse, check, resolve, lower, encode
                      -> BuiltClosure: images, and a declaration about them
                      -> the build workspace ends here

admit                 verify every image, one at a time
                      -> records, membership, entry receipt
                      -> the process account begins here
```

**What is live when `build_from_provider` returns is the product, not the
workspace.** The summaries, the plan, the lowering views and the verification
surfaces are locals of that call and die with it, so the arena read at the
boundary shows exactly what a `BuiltClosure` holds. That is why the composition
of the workspace is attributed separately, below, from a harness that walks the
same phases and reads the arena between them.

**The source corpus is outside both accounts.** The fixture's text is copied to
host allocations and the generator's own copy is dropped before anything is
measured — in TOS a unit is a window into a mapped capsule, so source inside the
account being measured would be an artefact of the harness. Each run prints the
arena's state before the build, so this is visible rather than asserted.

| Reported as | What it is |
|---|---|
| build frontier | the highest address the arena was ever carried to during the build. A region must cover this |
| images | `BuiltClosure::image_bytes()` — the encoded closure the admission is handed |
| declaration | the rest of what is live at the boundary: the resolution snapshot the verifier holds every image to |
| frontier above what survives | the build's peak minus what it handed over: scratch, plan, views, and allocator headroom |

## The measurement, by shape

Ceiling-sized modules (`256 KiB`, docs/44 §2), one process per row, measured at
`4204f32` — before the plan reduction the last section reports.

**A — a chain, one import per module**

| Modules | Build frontier | Images | Declaration | Frontier above what survives |
|---:|---:|---:|---:|---:|
| 1 | 36.58 MiB | 0.37 MiB | 0.05 MiB | 36.16 MiB |
| 2 | 37.26 | 0.74 | 0.10 | 36.42 |
| 4 | 38.52 | 1.48 | 0.19 | 36.85 |
| 8 | 41.04 | 2.96 | 0.38 | 37.70 |
| 16 | 45.24 | 5.90 | 0.76 | 38.59 |
| 32 | 55.24 | 11.74 | 1.52 | 41.98 |
| 64 | 75.04 | 23.43 | 3.03 | 48.58 |
| 128 | 117.56 | 46.68 | 6.05 | 64.83 |
| **256** | **204.37** | **92.83** | **12.11** | **99.44** |

**B — a wide fan-in: one entry importing every dependency**

At 256 the entry names `255` dependencies in `14 601 B` of source, so the fan is
derived from byte accounting rather than assumed.

| Modules | Build frontier | Images | Declaration | Frontier above what survives |
|---:|---:|---:|---:|---:|
| 16 | 45.66 MiB | 5.53 MiB | 0.76 MiB | 39.37 MiB |
| 32 | 60.23 | 11.38 | 1.52 | 47.34 |
| 64 | 79.32 | 23.07 | 3.03 | 53.23 |
| 128 | 124.02 | 46.32 | 6.05 | 71.64 |
| **256** | **219.23** | **92.46** | **12.11** | **114.66** |

**C — a balanced DAG: each module importing the two below it**

| Modules | Build frontier | Images | Declaration | Frontier above what survives |
|---:|---:|---:|---:|---:|
| 16 | 45.31 MiB | 5.90 MiB | 0.76 MiB | 38.66 MiB |
| 32 | 56.23 | 11.75 | 1.52 | 42.97 |
| 64 | 76.79 | 23.44 | 3.03 | 50.32 |
| 128 | 115.83 | 46.70 | 6.05 | 63.08 |
| **256** | **202.30** | **92.86** | **12.11** | **97.34** |

Least squares over each series:

| Shape | Build frontier | Images | Declaration |
|---|---|---|---|
| A chain | `0.656 MiB/module` above `35.06 MiB` | `0.363 MiB/module` | `48.41 KiB/module` |
| B wide fan-in | `0.749 MiB/module` above `28.98 MiB` | `0.360 MiB/module` | `48.65 KiB/module` |
| C balanced | `0.646 MiB/module` above `35.54 MiB` | `0.363 MiB/module` | `48.42 KiB/module` |

The shape changes the slope by about `0.1 MiB` a module and nothing else. What
the graph decides is how many lowering views are alive at once, and that is one
term of several.

## What the account is made of

`--lowering --modules 256` walks the same phases on the same fixture and reads
the arena between them. It is a re-implementation of the build loop rather than
the production call, so it attributes rather than bounds:

| Owner | A chain | B wide fan-in | C balanced | Lives until |
|---|---:|---:|---:|---|
| closure plan (owned summaries) | 52.10 MiB | 51.89 MiB | 52.16 MiB | the build ends |
| one turn's source and parse tree | 25.21 | 25.21 | 25.21 | the end of that module's turn |
| live lowering views, maximum | 0.06 (1 module) | **14.78 (255)** | 0.12 (2) | their last consumer |
| verification surfaces | 12.66 | 12.61 | 12.66 | the declaration is built |
| accumulated images | 92.81 | 92.46 | 92.84 | **the admission** |
| **sum** | **182.84** | **196.95** | **182.99** | |
| measured build frontier | 204.37 | 219.23 | 202.30 | |
| allocator above the live sum | 21.53 | 22.28 | 19.31 | |

Three readings follow from that table.

**The closure plan is the largest thing the workspace keeps, and it is not the
product.** `52.1 MiB` — `208 KiB` per ceiling-sized module — is owned summaries,
held so that resolution needs no parse tree (`STAGE2_ARENA_BOUND.md` measures
why the alternative is worse by a factor of seventy). It is the same in all
three shapes, because a summary describes a module rather than a graph.

**One turn's scratch is a constant.** A ceiling-sized module's normalized
source, parse tree and lowered IR are alive together only while that module is
being lowered. `25.21 MiB`, identical across shapes, and it is what makes a
one-module build cost `36.58 MiB` at all.

**Keeping the products in the same allocator is not free.** The frontier stands
`19–22 MiB` above the live sum, and `frontier - images` grows by `0.28` to
`0.39 MiB` a module — about the size of an image. Each turn allocates and frees
tens of megabytes around images that stay, so what the allocator can reuse is
broken up by the products accumulating inside it. That is a fact about *where
the products live*, not about the build algorithm, and whether it survives a
decision to write them elsewhere is exactly what the open decision decides.

## What this says about sizing a workspace

Two readings, matched to two shapes the decision could take. Neither is
recommended here.

| If the build's output … | The workspace must hold | At 256 ceiling-sized modules |
|---|---|---|
| accumulates inside the workspace | everything measured | **`170–177 MiB`** after the plan reduction; `204–219 MiB` before it |
| is written out as it is produced | plan, scratch, views, surfaces, headroom | **`78–84 MiB`**, from `frontier - images` after the reduction |

The second row is `frontier - images` after the reduction, and it is an *upper*
bound on that arrangement: part of the allocator headroom exists because the
images are there. Its lower bound is the live sum without images — one turn's
scratch, the surfaces, the views and the plan, so `38.0 MiB` (chain),
`52.7 MiB` (wide) and `38.1 MiB` (balanced) — plus whatever headroom remains.
Neither end is a measurement of a build that writes its images somewhere else,
because no such code path exists: where they would go has not been decided.

For smaller closures the pre-reduction tables read directly and are now upper
bounds: `41 MiB` at 8 modules, `55 MiB` at 32, `75 MiB` at 64, `118 MiB` at 128,
with the products inside.

## Over a capsule-backed source set

`--capsule` repeats the measurement over the source backend a boot actually
has: `tos_capsule_source::CapsuleSourceProvider` reading a Capsule v1's own path
table and handing out windows into its payload. How many units fit is derived
rather than assumed — the fixture is built at a count estimated from
`MAX_CAPSULE_BYTES` and reduced until the builder produces a capsule that is
within the ceiling and parses.

| Unit size | Units carried | Capsule | Spare | Build frontier | Images | Declaration | Run |
|---:|---:|---:|---:|---:|---:|---:|---|
| 256 KiB | **127** | 33 294 052 B | 260 380 B | 115.76 MiB | 46.58 MiB | 6.04 MiB | `Int(I32, 1)`, 253 fuel |
| 128 KiB | **255** | 33 433 738 B | 120 694 B | 102.02 MiB | 46.59 MiB | 6.07 MiB | `Int(I32, 1)`, 509 fuel |
| 64 KiB | **256** | 16 786 942 B | 16 767 490 B | 51.43 MiB | 23.22 MiB | 3.07 MiB | `Int(I32, 1)`, 511 fuel |

Each row is a chain that deep: the entry at `/system/boot/init.tos` calling
through every dependency under `/system/lib/`, so no module is carried without
being reached, and the answer is checked.

**Which ceiling binds is a function of the unit size.** At the source ceiling a
capsule holds `127` modules — half of the docs/44 closure ceiling — and the
`32 MiB` capsule limit is what stops it. At `128 KiB` the two nearly meet:
`255` modules with `120 694 B` of capsule left. At `64 KiB` the closure ceiling
binds first and `16.7 MiB` of capsule goes unused.

So a capsule can carry a conforming closure at the docs/44 ceiling only if the
average unit is at or below about `128 KiB`. That is a statement about Capsule
v1's `32 MiB`, not about the language: **claim B's provider and algorithm hold**
— a real capsule-backed set builds, verifies and runs at every size measured —
and a source tree whose modules are ceiling-sized needs a backend a capsule is
not, which is claim C and stays open.

The build accounts agree with the ones measured through the slice provider: the
`127`-module capsule build peaks at `115.76 MiB` against `117.56 MiB` for a
`128`-module chain. The provider changes where source comes from and not what a
build costs.

**Caveat.** These runs build the capsule in the measured arena before dropping
it, so the arena's frontier is already at about `100 MiB` when the build starts;
each run prints that line. The build's own high-water is still the reported
figure — it exceeded the fixture's — but the small-closure end of this table
would be dominated by the fixture and is not reported here for that reason.

## The target account, in the same process

Each run continues past the boundary: it admits the built closure and runs the
entry.

**Verifying the whole closure never went above the build's high-water mark.** At
every count and every shape, the admission — sequential `verify_image` over the
closure, records, membership, entry receipt — rose `0 B` above the frontier the
build had already reached.

The *run* did, at the middle counts: up to `77.06 MiB` above it at 16 modules of
the wide fixture, falling to `0 B` at 256 where the build's own peak is higher.
That is the bounded resident set decoding modules under `HOST_RESIDENCY`, and it
is not the process grant's number: what a runtime process must hold is measured
under the grant itself in `STAGE3_MODULE_RESIDENCY_P1.md` and
`STAGE3_PROCESS_GRANT.md`. A harness that builds and runs in one arena cannot
separate the two by frontier, because a frontier never falls.

The chain and balanced fixtures declare `recursion: 8`, so from 16 modules up
their runs verify, execute and then trap on their own declared bound. The wide
fan-in completes at every count, reaching `Int(I32, 32385)` at 256 on `1 022`
fuel of `10 000 000`. Every row's closure was verified image by image before
anything ran.

## What the plan costs after the set has been checked

The attribution above put the closure plan at `52.1 MiB` — `208 KiB` a module,
larger than one turn's scratch and larger than every image the build had encoded
by the halfway mark. What is in it is mostly one thing: every type name a module
declares, kept so that another module's qualified use can be resolved against it
(`check_qualified_types`). That question is asked once, by
`check_module_summaries`, and never again.

So the build now consumes its summaries into `ModulePlan` — path, name, content
id, imported module names — the moment the set-wide check returns. Measured on
the same fixture, beside the summaries rather than instead of them, so that what
is reported is the reduced form's own size:

| Modules | As summaries | As plans | |
|---:|---:|---:|---|
| 32 | 6 983 936 B (6.66 MiB) | 17 952 B (0.02 MiB) | `389x` |
| 256 | 54 635 392 B (52.10 MiB) | 144 832 B (0.14 MiB) | `377x` |

And the build account at the closure ceiling, before and after:

| Shape | Build frontier at `4204f32` | After the reduction | Difference |
|---|---:|---:|---:|
| A chain | 204.37 MiB | **172.15 MiB** | `-32.22` |
| B wide fan-in | 219.23 MiB | **170.25 MiB** | `-48.98` |
| C balanced | 202.30 MiB | **176.86 MiB** | `-25.44` |

The drop is smaller than the `52.1 MiB` the plan used to be, and that is the
fragmentation effect again: what the allocator no longer has to carry is not
only what is no longer live. The images, the declaration and every outcome are
byte-identical across the change — the wide fan-in still completes with
`Int(I32, 32385)` on `1 022` fuel, the chain and balanced fixtures still trap on
their own `recursion: 8`.

## With the products written outside the workspace

The section above inferred what a workspace would cost if its products lived
somewhere else. It no longer has to: `build_into_bundle` writes each module's
declaration and image into a backing the build does not own, in the same step
that produces them, and `--external` measures the workspace with that backing
allocated outside the instrumented arena.

The fixture is streamed too — each unit is generated and copied out before the
next is made — because a corpus built inside the account being measured leaves
its own high-water mark under every figure after it. Until that was fixed, the
chain and balanced numbers below were the *generator's* peak and not the
build's.

**At the docs/44 closure ceiling, 256 ceiling-sized modules:**

| Shape | Build workspace | Live when it returns | Bundle | Both at once |
|---|---:|---:|---:|---:|
| A chain | **77.14 MiB** | 0.04 MiB | 100.87 MiB | 178.01 MiB |
| B wide fan-in | **69.54 MiB** | 0.04 MiB | 100.47 MiB | 170.01 MiB |
| C balanced DAG | **75.96 MiB** | 0.04 MiB | 100.90 MiB | 176.86 MiB |

Two things change with the products gone, and both matter:

- the workspace is **`93–100 MiB` smaller** than the same build with its
  products inside (`170–177 MiB`), which is more than the products weigh;
- what it holds when it returns is `40 KB`. The whole of a build's memory is
  transient, and the account can be released whole.

**The workspace against the closure size** (chain, the worst measured shape):

| Modules | Build workspace | Bundle | Both at once |
|---:|---:|---:|---:|
| 8 | 36.43 MiB | 3.20 MiB | 39.63 MiB |
| 32 | 36.46 | 12.71 | 49.17 |
| 64 | 36.51 | 25.38 | 61.90 |
| 128 | 42.98 | 50.63 | 93.61 |
| 256 | 77.14 | 100.87 | 178.01 |

The workspace is **flat to three digits from 8 to 64 modules** — `36.4` to
`36.5 MiB`, one turn's scratch and nothing else — and then climbs. Nothing
semantic accumulates to explain it: what is live at the boundary is `40 KB` at
every count. It is the allocator's high-water mark under 256 turns of churn, so
it is a property of this bounded heap and not of the build algorithm, and it is
the first thing to look at if the workspace ever needs to be smaller.

The bundle grows at `0.39 MiB` a module and is the same to within `0.5 MiB`
across the three shapes, which it should be: it holds the same images.

**The smallest workspace the build fits in, enforced.** With `--workspace-cap N`
the measured allocator refuses past `N` bytes, so the question is answered by
running the build rather than by arithmetic. Bisected on the chain at 256:

| | |
|---|---:|
| smallest declared workspace that completes | **81 100 800 B** (77.34 MiB) |
| largest that does not | **81 059 840 B** (77.30 MiB) |

Below the line the build **fails closed**: the allocation is refused, the build
does not finish, and the partial bundle is not launchable — its header is
written last, so a reader is refused rather than handed a shorter closure than
the one that was asked for.

**The two paths agree.** `crates/tos-pipeline/tests/bundle_path.rs` runs the
same source through `build_from_provider -> admit` and through
`build_into_bundle -> admit_bundle` and compares what a run can observe: the
same receipt from the target's own verifier, the same value, the same fuel used
and the same declared limit. A storage arrangement that changed a result would
be a semantic input, and a build's output has no business being one.

## The three claims have three accounts

**A ledger that mixes them describes no possible system.** An earlier revision of
this file's companion ADR added a `32 MiB` capsule to a build of
`256 x 256 KiB`; the capsule measurement above says a Capsule v1 carries at most
`127` units at that size, so the two lines cannot be about the same build. What
follows keeps them apart.

### A — the reference algorithm, with no corpus resident anywhere

Measured through a **generative provider**: the catalog is paths, each unit is
made when it is asked for and dropped when the caller is done with it, and no
corpus exists inside the measured account or outside it. Every snapshot handed
out is watched weakly, so what is reported is what the caller had not yet
dropped.

| Shape | Build workspace | Source at once | Bundle | Workspace + bundle |
|---|---:|---:|---:|---:|
| A chain | 74.61 MiB | 262 116 B | 100.87 MiB | 175.47 MiB |
| B wide fan-in | 70.71 MiB | 262 116 B | 100.47 MiB | 171.18 MiB |
| C balanced DAG | **76.45 MiB** | 262 142 B | 100.90 MiB | 177.35 MiB |

**One unit, ever**, over `512` requests for a closure of 256 — two per module,
one for the check pass and one for lowering. That is claim A's residency
independence, measured rather than asserted.

Enforced hard minimum on the worst shape: the build completes in a declared
workspace of **`80 281 600 B`** and fails to allocate at `79 298 560 B`.

On the ADR-0040 machine, worst shape, with no margin on the workspace:

| Line | |
|---|---:|
| BuildWorkspace, measured worst | 76.45 MiB |
| launch bundle | 100.90 MiB |
| build-worker process overhead beyond its grant | 2.08 MiB |
| page tables for both mappings, `4 KiB` pages | ~0.4 MiB |
| **peak during the build** | **~179.8 MiB** |
| pool after the nucleus | ~229.8 MiB |
| **spare** | **~50 MiB** |

### B — a real Capsule v1

The three configurations a capsule can hold, through `CapsuleSourceProvider` into
an external bundle. The capsule is assembled by a separate process and read into
memory outside the measured arena, as a boot maps one the loader placed — a
capsule assembled in the same process leaves its own high-water mark under every
figure after it.

| Configuration | Capsule | Workspace | Hard minimum | Bundle | Physical peak |
|---|---:|---:|---:|---:|---:|
| 127 × 256 KiB | 31.75 MiB | 43.46 MiB | 45 875 200 B | 50.49 MiB | ~128.1 MiB |
| 255 × 128 KiB | 31.88 MiB | 37.39 MiB | 39 321 600 B | 50.52 MiB | ~122.2 MiB |
| 256 × 64 KiB | 16.01 MiB | 19.84 MiB | 20 971 520 B | 25.19 MiB | ~63.4 MiB |

Every one runs to its answer (`Int(I32, 1)`), and the physical peak includes the
capsule, the workspace, the bundle and the worker's overhead. Against a pool of
`229.8 MiB` the worst leaves about `100 MiB` spare.

### C — an installed-source backend

**Open.** No residency is attributed to it here: not a corpus, not a capsule's
`32 MiB`, nothing. What its contract has to permit is what A measures — one unit
materialized at a time — which `SourceSnapshot::Owned` already allows.

## What this is not

- **Not a freestanding measurement.** A host process with a bounded heap. No
  process boundary was crossed, no region was transferred, and no build worker
  exists.
- **Not evidence for claim C.** The shape tables came from a
  `SliceSourceProvider` over host allocations, and the capsule table from a
  capsule, which is a backend for what fits in `32 MiB` and for nothing above it
  (ADR-0072 §9).
- **Not a decision.** Where a build workspace comes from, how large it is, what
  its output is, whether that output is one region or several, when it becomes
  read-only, who reclaims it and when a target maps it all remain open.
- **Not P2 or P3.** One machine, one build, no CI reproduction and no
  independent reproduction (docs/35).
