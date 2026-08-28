<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 build workspace — what the build account costs, apart from what it hands over

Evidence level: **P1, locally measured on the host arena harness**, the same
instrument `STAGE2_ARENA_BOUND.md` and `STAGE3_PROCESS_GRANT.md` use: the
production path runs *through* `tos_runtime`'s bounded heap, so every figure is
the allocator's own accounting rather than a sum of requests.

Producer: `source/tests/arena-bound --build --modules N --shape S`, one count
per process, with `--lowering --modules 256 --shape S` for the attribution. The
first calls `tos_pipeline::build_from_provider` and then `tos_pipeline::admit` —
the two calls ADR-0073 §1 separates — so the line between the build account and
the target account is a return, not an estimate.

Verdict, at the docs/44 closure ceiling of **256 ceiling-sized modules**:

- the build account's high-water mark is **`204.37` (chain), `219.23` (wide
  fan-in) and `202.30 MiB` (balanced)**;
- **`104.9 MiB` of that survives the workspace**: the image closure
  (`92.5–92.9 MiB`) and the declaration handed with it (`12.11 MiB`);
- the workspace's own transient state is **`90–105 MiB`**, of which the largest
  owner is the closure plan at **`52.1 MiB`** — `208 KiB` per module — followed
  by one turn's scratch at `25.21 MiB`;
- the remaining `19–22 MiB` is the allocator's high-water mark above the live
  bytes, and it grows with the number of images retained beside the churn.

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

Ceiling-sized modules (`256 KiB`, docs/44 §2), one process per row.

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
| accumulates inside the workspace | everything measured | **`204–219 MiB`** |
| is written out as it is produced | plan, scratch, views, surfaces, headroom | **`110–127 MiB`** |

The second row is `frontier - images` from the tables, and it is an *upper*
bound on that arrangement: part of the allocator headroom exists because the
images are there. Its lower bound is the live sum without images —
`90.03 MiB` (chain), `104.49` (wide), `90.15` (balanced) — plus whatever
headroom remains. Neither end is a measurement of a build that writes its images
somewhere else, because no such code path exists: where they would go has not
been decided.

For smaller closures the same tables read directly: `41 MiB` at 8 modules,
`55 MiB` at 32, `75 MiB` at 64, `118 MiB` at 128, with the products inside.

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

## What this is not

- **Not a freestanding measurement.** A host process with a bounded heap. No
  process boundary was crossed, no region was transferred, and no build worker
  exists.
- **Not evidence for claim C.** The source came from a `SliceSourceProvider`
  over host allocations. An interface is not a backend (ADR-0072 §9).
- **Not a decision.** Where a build workspace comes from, how large it is, what
  its output is, whether that output is one region or several, when it becomes
  read-only, who reclaims it and when a target maps it all remain open.
- **Not P2 or P3.** One machine, one build, no CI reproduction and no
  independent reproduction (docs/35).
