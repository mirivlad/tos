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

## Adversarial source shapes, and what they overturn

Every figure above varies the **graph** — chain, fan-in, DAG — and holds the
module body constant. A module body is what the frontend actually walks, so a
bound measured over graphs alone is a bound over one body. Seven bodies, each
filling the same `256 KiB` unit to the same ceiling, 256 modules, chain graph,
generative provider, products written outside:

| Body | Workspace frontier | Peak committed | Bundle | Workspace + bundle |
|---|---:|---:|---:|---:|
| mixed — records and functions | 74.61 MiB | 67.74 MiB | 100.87 MiB | 175.48 MiB |
| function-heavy | 82.59 | 56.25 | 107.58 | 190.17 |
| export-heavy | 43.30 | 43.14 | 107.79 | 151.09 |
| statement-heavy (source maps) | 90.76 | 90.76 | **179.41** | **270.17** |
| nested types | 120.55 | 114.83 | 27.71 | 148.26 |
| type-heavy | 155.79 | 153.78 | 40.22 | 196.01 |
| **maximum small declarations** | **221.04** | **218.92** | 42.63 | **263.67** |

Enforced hard minimums, bisected on the two worst bodies:

| Body | Smallest declared workspace that completes | Largest that fails |
|---|---:|---:|
| maximum small declarations | **231 997 440 B** (221.25 MiB) | 231 342 080 B |
| type-heavy | **163 840 000 B** (156.25 MiB) | 163 184 640 B |

Each is within `0.3 MiB` of that body's measured frontier, which is what a build
holding its peak rather than fragmenting looks like from the other side.

**The graph shapes were not the worst case, and not by a little.** The worst
body needs `221.04 MiB` of workspace against `77.14 MiB` for the worst graph —
`2.9x` — and a statement-heavy body produces a bundle of `179.41 MiB` against
`100.9 MiB`. Any workspace size derived from the graph tables alone would have
been wrong by a factor of three.

**Peak committed is the finding.** For every adversarial body the live peak is
within `2 MiB` of the frontier — `218.92` against `221.04`, `153.78` against
`155.79`, `90.76` against `90.76`. The build is not fragmenting an arena; it is
**holding that much at once**. Two consequences:

- the growth from `36 MiB` at 64 modules to `77 MiB` at 256 in the mixed body is
  mostly live data too (`67.74 MiB` peak committed), not allocator churn as the
  earlier section supposed;
- **a per-turn scratch arena would not help.** What is live at the peak is not
  one module's turn; it is the check phase's owned summaries, all 256 of them,
  and their largest field is the set of type names a module declares — which is
  exactly what the type-heavy and small-declaration bodies maximize.

The first crossing of `RuntimeMemoryGrantV1` tells the same story from the other
side. For the small-declaration body the arena passes `54 MiB` on a `3 145 728 B`
allocation with `50 789 216 B` already live, `511 865` blocks and a largest hole
of `1 048 720 B`: there was no hole to reuse because nothing had been freed.

### Can a build fit an ordinary process grant?

**No, and not for a reason an allocator change can fix.** Live bytes alone —
peak committed, with every product already outside the account — are
`43.14 MiB` for the friendliest body and `218.92 MiB` for the worst, against
`RuntimeMemoryGrantV1 = 54 MiB`. Even the friendliest leaves nothing for the
rest of the run.

What the measurement does identify is the single lever: the set-wide type check
(`check_module_summaries` → `check_qualified_types`) requires every module's
declared type names to be resident at the same time, because a qualified name in
one module is resolved against another module's set. That is what makes the
summaries the peak, and it is a data-structure question rather than an allocator
one. Nothing here proposes changing it: a semantics-preserving alternative would
have to be designed, measured and differentially checked first.

## What the check phase's peak is made of

The adversarial bodies put the workspace's peak in the check phase, holding 256
owned summaries at once. Decomposed per summary, with the source outside the
arena so only the summary is in the delta, and the total **measured** as the
arena's committed delta rather than summed over the fields this harness happens
to know about:

| Body | Declared types | Semantic payload | Measured summary | Ratio | At 256 modules |
|---|---:|---:|---:|---:|---|
| maximum small declarations | 9 390 | 64 716 B | 912 816 B | **14.10x** | 222.86 MiB over 15.80 MiB of payload |
| type-heavy | 6 413 | 75 937 B | 623 856 B | 8.22x | 152.31 MiB over 18.54 MiB |
| nested types | 5 003 | 49 013 B | 486 768 B | 9.93x | 118.84 MiB over 11.97 MiB |
| mixed | 2 266 | 26 173 B | 221 440 B | 8.46x | 54.06 MiB over 6.39 MiB |
| statement-heavy | 1 | 103 B | 960 B | 9.32x | 0.23 MiB over 0.03 MiB |

**Eight to fourteen times the payload.** The `String` capacities are exactly the
payload — no slack — so all of the difference is container and node overhead:
about `90 B` per declared type name, against a name that averages under
`7 B`. What the check phase peaks at is not information; it is the shape it is
held in.

## Four ways to hold the same question

The set-wide check asks one thing of that data — *does module M declare name N* —
once per qualified use. Four representations, built from the production
summaries' own names, measured in the same arena. Every one is checked against
the others and against the fixture on `hits` and equal-sized `misses`, so a
smaller structure that answered differently would not count as smaller.

**256 modules, maximum small declarations — 2 269 744 names, `19 165 116 B` of
payload:**

| Representation | Size | Build | 454 240 probes |
|---|---:|---:|---:|
| A — `BTreeSet<String>` per module (today) | 206.45 MiB | 242.4 ms | 100.3 ms |
| B — sorted `Vec<String>` per module | 190.50 MiB | 114.5 ms | 145.6 ms |
| **C — byte slab + sorted offsets** | **40.04 MiB** | **36.6 ms** | **79.4 ms** |
| D — closure-wide interning + ids | 338.56 MiB | 1 592.0 ms | 157.2 ms |

The same ordering holds for the type-heavy body (143.51 / 132.38 / **27.79** /
243.58 MiB) and the mixed one (50.94 / 46.84 / **12.74** / 84.07 MiB).

**C wins on every axis measured**: `5.2x` smaller than what the build holds
today, `6.6x` faster to build, and faster to probe — a binary search over a
contiguous slab touches fewer cache lines than one over scattered `String`s. It
compares bytes exactly, so there is no collision question to answer.

**D is worth a second look despite its number.** Its steady state — one slab of
`19 165 116 B` and per-module id tables of `9 078 976 B` — is `28.25 MiB`,
*smaller* than C. What ruins it as written is the intern table: a
`BTreeMap<String, u32>` that holds every distinct name a second time while the
index is being built, which is the `338 MiB` peak and the `1.6 s`. An interning
pass whose own table is a slab rather than a map would plausibly land under C,
and that has not been measured.

Neither is implemented in production. What this establishes is the size of the
lever: replacing the representation, with no change to what the check computes,
would take the worst adversarial body's check-phase peak from `206 MiB` to about
`40 MiB`.

## After the compact representation: the adversarial rerun

`TypeNames` — a byte slab with a sorted offset table — replaced the
`BTreeSet<String>` a summary held. Same names, same answers, same diagnostics;
the change is what the bytes are stored in. Re-measured, every body at the
docs/44 ceiling, generative provider, products written outside:

| Body | Workspace before | Workspace after | Peak committed after | Bundle |
|---|---:|---:|---:|---:|
| maximum small declarations | 221.04 MiB | **37.28 MiB** | 36.82 MiB | 42.63 MiB |
| type-heavy | 155.79 | **37.02** | 36.82 | 40.22 |
| nested types | 120.55 | **37.02** | 36.82 | 27.71 |
| statement-heavy | 90.76 | 90.77 | 90.77 | 179.41 |
| function-heavy | 82.59 | 72.64 | 56.26 | 107.58 |
| mixed | 74.61 | **37.06** | 36.87 | 100.87 |
| export-heavy | 43.30 | 43.32 | 43.15 | 107.79 |

**The adversarial spread of `43–221 MiB` became `37–91 MiB`, and its shape
changed.** What is left does not grow with the closure: at `37 MiB` for four of
the seven bodies the workspace is one module's turn and nothing else, and the
three that are higher are higher for a reason that lives inside a single module
— a statement-heavy module's parse tree and IR, a function-heavy module's
tables, an export-heavy module's declaration. **The workspace is now bounded by
the worst single module the ceilings admit rather than by how many modules there
are**, which is the property claim A needed and did not have.

Enforced hard minimums, bisected after the change:

| Body | Smallest declared workspace that completes | Largest that fails |
|---|---:|---:|
| statement-heavy | **95 518 720 B** (91.10 MiB) | 94 699 520 B |
| function-heavy | 76 840 960 B (73.28 MiB) | 76 103 680 B |
| maximum small declarations | **39 321 600 B** (37.50 MiB) | 38 338 560 B |

The last row is the measurement this round was for: the body that needed
`231 997 440 B` before now needs `39 321 600 B`, a factor of `5.9`.

### The two-pass checker, measured rather than assumed

The phase split is implemented and proved to report exactly what the one-pass
check reports (`crates/tos-core/tests/two_pass_checker.rs`). Measured at the
ceiling it does not pay:

| Body | One pass | Two passes |
|---|---|---|
| mixed | 37.06 MiB, 31.2 s | 37.06 MiB, 34.7 s |
| type-heavy | 37.02 MiB, 16.7 s | 42.85 MiB, 18.6 s |
| small declarations | 37.28 MiB, 20.7 s | 43.63 MiB, 22.9 s |

About `11 %` more CPU and up to `6.4 MiB` more memory. The reason is that the
compact representation already removed the term the split was aimed at: what two
passes stop holding — the qualified uses — is small once the type surface is a
slab, while what they add is a second parse tree alive beside every summary and
a third materialization of every unit. Production keeps one pass; the phases
remain available, with their equivalence proof, for a corpus where uses
dominate.

### The physical account, after

Workspace + bundle + the worker's `2.08 MiB` of process overhead and about
`0.4 MiB` of page tables, against the ADR-0040 pool of `~229.8 MiB`:

| Body | Workspace | Bundle | Total | Fits |
|---|---:|---:|---:|---|
| maximum small declarations | 37.28 MiB | 42.63 MiB | 82.4 MiB | yes |
| type-heavy | 37.02 | 40.22 | 79.7 | yes |
| nested types | 37.02 | 27.71 | 67.2 | yes |
| mixed | 37.06 | 100.87 | 140.4 | yes |
| export-heavy | 43.32 | 107.79 | 153.6 | yes |
| function-heavy | 72.64 | 107.58 | 182.7 | yes |
| **statement-heavy** | 90.77 | **179.41** | **272.7** | **no** |

Six of seven now fit with room. The one that does not is over because of its
**bundle**, not its workspace: `179.41 MiB` of images for source that is
source-map heavy. That is an image-format question — ADR-0070's territory — and
it is the next thing in the way, not the build workspace.

## Where a statement-heavy image's bytes go

The workspace stopped being the obstacle and the bundle became it: `179.41 MiB`
of images for a closure that cost `90.77 MiB` to build. Decomposed from the
encoder's own layout, one standalone module filled to the source ceiling:

| Section | Statement-heavy | Mixed | Export-heavy |
|---|---:|---:|---:|
| function bodies | 360 555 B (49.0 %) | 192 313 B (49.3 %) | 152 085 B (41.0 %) |
| **source-map entries** | **375 092 B (51.0 %)** | 89 232 B (22.9 %) | 109 015 B (29.4 %) |
| string table | 267 B | 52 456 B | 59 153 B |
| types + exports | 11 B | 56 426 B | 50 644 B |
| source-map identities | 13 B | 13 B | 13 B |

**The identity fields were never the problem.** The encoder already collects the
seven per-entry identity fields into a table: `13 B` for the whole module,
against `1 823 970 B` if each entry wrote its own. What scales with statement
count is the entry *body* — `47 621` entries at `7.88 B`, two per instruction —
and it was half of a statement-heavy image.

An entry was an identity reference, an absolute `byte_start`, an absolute
`byte_end` and a parent tag. In a ceiling-sized module both offsets reach three
varint bytes, every time, for a map that walks forward in small steps.

### Two candidates, computed on the modules in hand

| | Statement-heavy | Mixed | Export-heavy |
|---|---:|---:|---:|
| as written | 7.88 B/entry | 7.87 | 7.87 |
| **spans as steps** | **4.00 B/entry** | **4.00** | **4.00** |
| distinct spans in a table | 10.53 B/entry | 9.86 | 9.86 |

A span table loses because there is nothing to share: `47 621` entries have
`47 621` distinct spans. Steps win because the numbers are small — the start as
a signed step from the previous entry's start, the end as a signed step from its
own start.

### What was changed, and what was not

`ENCODING_VERSION` is now `2`. ADR-0070 §3 versions the **storage encoding**
independently of the semantic digest scheme precisely so this can happen: the
fields, their meanings and the digest a module is identified by are untouched,
and a reader fails closed on a version it does not know. No stored image
predates the change — the golden vectors carry capsules, not images — so nothing
had to be migrated.

Both steps are zigzag, which makes the encoding **total**: a map that walks
backwards, an entry whose end precedes its start, and the longest step the
source ceiling admits all round-trip, and there is a test that says so rather
than a rule the writer has to obey.

The measured effect: a statement-heavy image falls from `736 026 B` to
`551 422 B`, `-25.1 %`, and its source map from `375 092` to `190 488 B`.

### The physical account, after both changes

256 ceiling-sized modules, workspace plus bundle plus the worker's `2.08 MiB`
and about `0.4 MiB` of page tables, against the ADR-0040 pool of `~229.8 MiB`:

| Body | Workspace | Bundle before | Bundle after | Total | Spare |
|---|---:|---:|---:|---:|---:|
| mixed | 37.06 MiB | 100.87 | **90.56** | 130.1 MiB | 99.7 MiB |
| export-heavy | 43.32 | 107.79 | **95.06** | 140.9 | 88.9 |
| function-heavy | 72.21 | 107.58 | **87.67** | 162.4 | 67.4 |
| **statement-heavy** | 90.77 | 179.41 | **134.49** | **227.8** | **2.0** |

**The statement-heavy case now fits, and only just.** Two megabytes of spare on
a `229.8 MiB` pool is a fit, not a margin, and the next lever is visible in the
same decomposition: function bodies are now `65 %` of a statement-heavy image at
`15.14 B` an instruction. Whether that is reducible without changing what an
image means has not been measured.

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
