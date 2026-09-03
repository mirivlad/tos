<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0074: The build workspace, the launch bundle, and who owns them

- Status: **Accepted** (Project Architect-approved)
- Date: 2026-08-29. Reconciled with the implemented system 2026-09-03
- Decision level: 2 — it fixes where a build's products live, what crosses the
  boundary between a build and a run, and which component owns that memory over
  time. It changes no TOS Core semantics, no accepted ceiling, no trust rule and
  no ABI operation
- Project Architect approval: **Vladimir Tomashevskiy, 2026-09-03**, for the
  reconciled form on closure commit `77970cb`. Two decisions inside it were
  taken by the Architect on 2026-08-29 and are recorded as such in §1 and §2;
  the 2026-09-03 ruling approves the surviving normative decision after
  reconciliation — build products outside the workspace, one immutable bundle
  region per exact closure, the performed T1 lifecycle of §4a, the funded
  build-worker role in place of a fixed `BuildWorkspace` allocation, the
  measured Capsule-v1 account of §5d, operation 20 as built in §6a, and the
  answers collected in §7a.

  **The historical and superseded sections stay historical and superseded.**
  Their presence in this document does not revive their old semantics, and the
  approval does not extend to them. The still-open installed-source backend
  (§5 C) and the absence of a fixed `BuildWorkspace` size (§5a, §5c) remain
  outside the Stage 3 closure claim, exactly as this document states
- Evidence: `docs/evidence/STAGE3_BUILD_WORKSPACE.md`,
  `docs/evidence/STAGE3_SUPERVISION.md` §7 — the performed T1 lifecycle,
  `docs/evidence/STAGE3_LAUNCH_PLANS.md`,
  `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`;
  `source/host-tools/qemu-test/build-topology.sh`,
  `source/host-tools/qemu-test/bundle-launch.sh`
- Related: ADR-0073 (Accepted) — the build leaves the target process and the
  verifier stays in it; this ADR says where its output goes. **ADR-0075
  (Accepted)** — the Region object model, which supersedes §4 and §5b.
  **ADR-0076 (Accepted)** — funded creation, which supersedes §6's endowment
  question and §7's workspace-origin question. ADR-0077, ADR-0072, ADR-0071,
  ADR-0070, ADR-0069, ADR-0040. `IPC_V1` §5 — regions. `SYSTEM_ABI_V1` §5 —
  operations 7, 17, 18, 19 and 20

## 0. What this document is now, section by section

This ADR was written as a draft before the Region contract, funded creation or
any freestanding bundle lifecycle existed. Every one of those now exists and is
evidenced, so most of what follows is either **implemented** or **superseded**.
The reconciliation is stated here, once, so a reader is not left to work out
which parts still decide anything.

| Section | Status now |
|---|---|
| §1 products outside the workspace | **Normative and implemented.** `build_into_bundle` writes into a caller-supplied backing |
| §2 one bundle region per exact closure, opaque to ring 0 | **Normative and implemented.** `SYSTEM_ABI_V1` operation 20; the nucleus reads no byte |
| §3 reference-implementation measurements | **Historical.** Superseded by its own §5 measurements, twice, and kept because §1 and §2 were taken from it |
| §4 the lifecycle as a transaction | **Superseded by ADR-0075 §4.** Replaced by §4a below, which is the lifecycle actually performed |
| §5 A/B physical accounts | **Historical measurement, standing.** §5d carries the current measured T1 figures |
| §5 C installed-source backend | **Open, and out of Stage 3** by ADR-0073's separation of claims |
| §5a a fixed BuildWorkspace size | **Still not proposed, and nothing depends on it.** See §5c |
| §5b region-authority gaps | **Superseded by ADR-0075**, which closed every gap it found |
| §6 process-creation shapes | **Superseded.** Shape A was chosen and is operation 20; the sketch's numbers, registers and entry-path rule are not what was built. See §6a |
| §7 open questions | **All six answered.** See §7a |

## 1. Decided: a build's products live outside the build workspace

`BuildWorkspace -> external output bundle`, and **not** a closure accumulating
inside the workspace's own allocator.

The reason is measured. At the docs/44 closure ceiling of 256 ceiling-sized
modules, about `104.9 MiB` survives a build — `92.8 MiB` of images and
`12.1 MiB` of declaration — and that is a product rather than transient state.
Keeping it inside the allocator that also churns tens of megabytes per module
cost more than the product itself: with the products inside, the build account
peaked at `170–177 MiB`; with them written out as they are made, the workspace
peaks at `69.5–77.1 MiB` and holds `40 KB` when it returns.

## 2. Decided: one bundle region per exact closure

Not a region per module. One immutable launch bundle carries:

- a versioned header;
- the exact closure's membership and the declaration the target will check;
- a table of image offsets and lengths;
- every hostile `TOSIMAGE/v1` byte;
- the entry identity and path, and only those declared inputs a run needs.

It does **not** carry: verifier receipts, `VerifiedModuleRecord`s, a trusted
manifest, decoded modules, or any frontend or verifier conclusion.

**To the nucleus the contents are opaque.** The format is understood by the
runtime that will verify the closure, and by nothing in ring 0.

`TOSBUNDLE/v1` (`source/crates/tos-bundle`) is that format, with a total bounded
parser.

**Implemented, freestanding, and evidenced.** When this section was written none
of the region, the ABI operation or the lifecycle existed; all three now do. A
region carries the bundle (ADR-0075), operation 20 creates a process from one,
and the whole lifecycle runs on the reference platform — see §4a. `parse_prefix`
was added so a bundle can be read out of a region without changing the format: a
region is a container of whole frames and an artifact is a prefix of one.

## 3. What the reference implementation established

`build_into_bundle` writes each module's declaration and image into a
caller-supplied backing in the same step that produces them; `admit_bundle`
parses the bundle, rebuilds the declaration in the target, and verifies the
closure image by image. Measured with the backing outside the instrumented
arena, 256 ceiling-sized modules:

| Shape | Build workspace | Bundle | Both at once |
|---|---:|---:|---:|
| A chain | **77.14 MiB** | 100.87 MiB | 178.01 MiB |
| B wide fan-in | 69.54 MiB | 100.47 MiB | 170.01 MiB |
| C balanced DAG | 75.96 MiB | 100.90 MiB | 176.86 MiB |

The workspace holds `40 KB` when the build returns, and the smallest declared
workspace the worst measured **graph** fits in was `81 100 800 B` — one enforced
`40 960 B` below that, the same build failed to allocate.

**Superseded by measurement, twice.** Adversarial *bodies* then showed the graph
was not the worst case, and two changes since have moved every figure in this
section: the type surface became a byte slab (`2dff355`) and the source map
became steps (`cf7756f`, `a2c273e`). The current numbers are in §5; this section
is what the decisions in §1 and §2 were taken from and is kept for that.

The two paths are differentially equivalent: the same source through
`build_from_provider -> admit` and through
`build_into_bundle -> admit_bundle` produces the same receipt from the target's
own verifier, the same value and the same accounting.

## 4a. The lifecycle, as performed

> This replaces §4. ADR-0075 removed the need for a memory-owning transaction
> object — the lifetime is carried by the capability itself — and ADR-0074's T1
> topology has since been built and measured. What follows is not a proposal.

```text
supervisor resident, holding process authority and a MemoryAuthority
  -> creates a transient build worker (operation 19), funded from the authority
     it presents, at the worker role's own fixed policy grant (ADR-0069 §2a)
  -> the worker allocates a region from the authority it was endowed with
     (operation 17) and writes a TOSBUNDLE/v1 into it as it builds
  -> freeze (18): the region becomes permanently immutable in place, and the
     worker's writable window becomes read-only. There is no half-frozen state
  -> share (7): the immutable affine region becomes a shared one, which is what
     lets it be in two places at once
  -> the worker hands the shared region to the supervisor over an endpoint it
     was given (IPC_V1 §5), and exits
  -> the supervisor collects the worker's ending (operation 14) and the worker's
     frames and slot are reclaimed
  -> only then is the target created from the bundle (operation 20). It receives
     its own capability for the same region and its own read-only window
  -> the target parses, verifies and runs the closure itself. No receipt
     crosses, and no verdict but its own is trusted
```

**Every arrow is evidenced in that order**, by
`source/host-tools/qemu-test/build-topology.sh`: the gate requires the worker's
reclamation to precede the target's creation, and requires the ending collected
to be the worker's own instance rather than merely *an* ending.

Three properties §4 argued for survive, and are now facts rather than intentions:

- **the workspace and the target grant never coexist.** The worker's whole
  footprint is reclaimed before the target is created, which the account shows;
- **the worker cannot reach the bundle after the handoff.** Freeze is consuming
  and in place, so the writable window is gone rather than merely unused;
- **and even if something did rewrite it, the target verifies anyway.** ADR-0073
  owns that, and operation 20 keeps it: a corrupt bundle produces a process that
  is created *successfully* and then refuses itself.

**A partial bundle is still not a launchable artifact**, and still by a property
of the format rather than a rule anyone obeys: the header is written last, so a
build that did not finish does not parse.

## 4. Historical: the lifecycle as a transaction, and the handoff as linear

> **Superseded by ADR-0075 §4, and replaced by §4a above.** Under a consuming
> freeze transition the lifetime is carried by the capability itself — ownership
> is affinity, the handoff is a linear transfer, reclamation is the last handle
> — so no memory-owning transaction object is needed. What follows is **history**:
> the reasoning that led there, kept because the shape it argued for is the shape
> that was built, and none of it is normative.

**The bundle backing belongs to the system launch/recovery transaction, not to
the build worker.**

```text
transaction opens
  -> reserves a BuildWorkspace and a bundle backing
  -> build worker gets the workspace as its grant
     and writable access to the bundle backing
  -> worker builds, writing products as it makes them

handoff, on success
  -> the worker loses writable access
  -> the bundle is immutable from here
  -> the BuildWorkspace is released
  -> the worker is no longer needed
  -> only then is the target process created
  -> the target receives the bundle read-only
  -> the target verifies the exact closure itself, sequentially
  -> the bundle stays readable for the run and for every reload
  -> the target ending releases the bundle, unless a cache owner
     has been accepted separately

failure before the handoff
  -> no target is created
  -> the transaction owner reclaims the workspace and the backing
  -> a partial bundle is not a launchable artifact
```

The last line is a property of the format rather than a rule to be obeyed: the
header is written **last**, so a bundle whose build did not finish does not
parse — a reader is refused rather than handed a shorter closure than the one
that was asked for. The reference implementation proves it
(`a_bundle_that_does_not_fit_fails_closed`).

Two consequences worth stating because they are what make the arrangement
bounded: the workspace and the target grant **never coexist**, and the worker
has no way to reach the bundle after the handoff, so nothing can rewrite what
the target is about to verify — and even if something did, the target verifies
anyway.

## 5. The physical accounts, kept apart

**A correction, and it matters.** An earlier revision of this section added a
`32 MiB` capsule, a `92 MiB` workspace and a `100.9 MiB` bundle into one line
and concluded that a build at the docs/44 ceiling barely fits the ADR-0040
machine. That line describes no possible system: `§22`'s own measurement says a
Capsule v1 carries at most **127** units at the source ceiling, so a bundle over
`256 x 256 KiB` cannot be the product of a capsule at all. Claims A, B and C
have separate accounts and are kept apart here.

### A — the reference algorithm, no corpus resident anywhere

`256 x 256 KiB` through a **generative provider**: the catalog is paths, a unit
exists between the request that made it and the drop that ends it, and no corpus
is held inside the measured account or outside it.

| Shape | Workspace | Source at once | Bundle | Workspace + bundle |
|---|---:|---:|---:|---:|
| A chain | 74.61 MiB | 0.25 MiB | 100.87 MiB | 175.47 MiB |
| B wide fan-in | 70.71 MiB | 0.25 MiB | 100.47 MiB | 171.18 MiB |
| C balanced DAG | **76.45 MiB** | 0.25 MiB | 100.90 MiB | 177.35 MiB |

Enforced, on the worst of the three: the build completes in a declared workspace
of **`80 281 600 B`** (76.56 MiB) and fails to allocate at `79 298 560 B`.

**These three vary the graph and hold the module body constant, and the body was
the larger term.** Seven bodies at the same ceiling first put the workspace
between `43.30` and `221.04 MiB` and the bundle between `27.71` and
`179.41 MiB`, and two of those configurations exceeded the pool. Peak committed
was within `2 MiB` of the frontier in every one, so what they measured was live
data rather than fragmentation — a fact about the check phase's data structures
rather than about the ceiling or the machine.

**Both terms have since been fixed and re-measured**
(`docs/evidence/STAGE3_BUILD_WORKSPACE.md`):

| Body | Workspace then | now | Bundle then | now |
|---|---:|---:|---:|---:|
| maximum small declarations | 221.04 MiB | **37.28** | 42.63 | 42.63 |
| type-heavy | 155.79 | **37.02** | 40.22 | 40.22 |
| statement-heavy | 90.76 | 90.77 | 179.41 | **122.90** |
| mixed | 74.61 | **37.06** | 100.87 | **87.90** |

The worst physical account is now the statement-heavy body at `216.2 MiB`
against a pool of `~229.8`, a margin of `13.6 MiB`. **Claim A at the docs/44
ceiling fits the reference platform**, and nothing measured asks for the ceiling
or the platform to move.

The source figure is measured rather than argued: every snapshot handed out is
watched weakly, and the most that was ever alive at once is **one unit**, over
`512` requests for a closure of 256. Residency-independence is a property of
this path, not an aspiration for it.

The physical account, worst shape, on the ADR-0040 machine:

| Line | Bytes |
|---|---:|
| BuildWorkspace (measured worst, no margin) | 76.45 MiB |
| launch bundle | 100.90 MiB |
| build-worker process overhead beyond its grant | 2.08 MiB |
| page tables for both mappings, `4 KiB` pages | ~0.4 MiB |
| **peak during the build** | **~179.8 MiB** |
| pool after the nucleus | ~229.8 MiB |
| **spare** | **~50 MiB** |

After the handoff the workspace is gone and the target replaces it:
`100.90 + 56.08 = 156.98 MiB`, which is smaller still. **A ceiling build fits
the reference platform**, and nothing measured asks for the docs/44 ceiling to
move.

### B — a real Capsule v1

The three configurations a capsule can actually hold, built through
`CapsuleSourceProvider -> build_into_bundle`, with the capsule read into memory
outside the measured arena as a boot maps one:

| Configuration | Capsule | Workspace | Hard minimum | Bundle | Worker + tables | Physical peak |
|---|---:|---:|---:|---:|---:|---:|
| 127 x 256 KiB | 31.75 MiB | 43.46 MiB | 45 875 200 B | 50.49 MiB | ~2.4 MiB | **~128.1 MiB** |
| 255 x 128 KiB | 31.88 MiB | 37.39 MiB | 39 321 600 B | 50.52 MiB | ~2.4 MiB | **~122.2 MiB** |
| 256 x 64 KiB | 16.01 MiB | 19.84 MiB | 20 971 520 B | 25.19 MiB | ~2.2 MiB | **~63.4 MiB** |

Each hard minimum is enforced the same way: the declared workspace one step
below it — `45 219 840`, `38 666 240` and `19 922 944 B` — fails to allocate.

Every one of them runs to its answer. The worst is `128.1 MiB` against a pool of
`229.8 MiB`: a capsule build on the reference platform has about `100 MiB` of
room to spare, and the binding constraint on a capsule is `MAX_CAPSULE_BYTES`,
not this machine.

### C — an installed-source backend

**Open, and nothing here describes it.** No residency may be attributed to a
backend that has not been chosen: not a whole corpus, not a capsule's `32 MiB`,
not anything. What its contract must permit is the shape A already measures —
one unit materialized at a time — which `SourceSnapshot::Owned` allows today
without the interface changing.

## 5a. A fixed BuildWorkspace size is not proposed yet

`80 281 600 B` is the smallest declared workspace the worst measured build fits
in — claim A, balanced, 256 ceiling-sized modules — enforced by refusing past
it. It is a measurement of
today's allocator behaviour and not yet a bound:

- **peak committed is within `2 MiB` of the frontier** for every adversarial
  body, so the account is holding that much at once rather than fragmenting.
  An allocator change, a per-turn scratch arena or any other churn remedy would
  move almost nothing;
- what is live at the peak is the check phase: 256 owned summaries, whose
  largest field is the set of type names each module declares, because the
  set-wide qualified-type check resolves a name in one module against another
  module's set;
- so the size question is downstream of a data-structure question, and fixing
  the size first would fix the wrong number;
- **and the lever was measured and then taken.** A summary cost `8.2x` to
  `14.1x` its own semantic payload — about `90 B` per declared type name against
  a name averaging under `7 B`. Of four representations compared over the
  production summaries' own names, a byte slab with a sorted offset table was
  `5.2x` smaller, built `6.6x` faster, probed faster and answered identically.
  It is now what a summary holds (`tos_core::TypeNames`, `2dff355`), and the
  worst body's workspace fell from `221.04 MiB` to `37.28`.

Until then any margin is engineering judgement wearing a number, and the honest
statement is that the bound is **not yet known**. What is known, after both
changes, is that the workspace is bounded by the worst single module the
ceilings admit rather than by how many modules there are: `37 MiB` for four of
the seven bodies, `90.77 MiB` for the worst, with enforced hard minimums of
`39 321 600 B` and `95 518 720 B`.

## 5b. Region authority: what the accepted contracts already say, and what they do not

> **Superseded by ADR-0075.** This section's audit stands as the record of what
> was found, and two of its conclusions were corrected there: G1 and G2 are
> reconciliation with ADR-0037 rather than open decisions (ADR-0075 §1), and
> ADR-0055 does not forbid region creation outright (ADR-0075 §2). The origin
> model, the freeze transition, the lifecycle and the reclamation rules are
> ADR-0075's, not this document's.

The lifecycle in §4 uses the word *transaction* for a lifetime. Before any of it
can be implemented, that word has to name an object the system already has, or
an ADR has to make one. This is the audit, against `CAPABILITY_V1`,
`IPC_V1` §5–§6, `SYSTEM_ABI_V1` §5 and ADR-0055.

**What the contracts do give.**

- A capability is `object + rights + scope + lifetime + generation`, in a
  nucleus-owned table, process-local, unforgeable (`CAPABILITY_V1` §2–§3).
- A **region** is already one of the object kinds a capability may name
  (`CAPABILITY_V1` §3), and `IPC_V1` §5 says the nucleus maps and unmaps and
  does not copy, and that a shared region is mapped under "the access mode the
  grant declares".
- `SYSTEM_ABI_V1` operation 7 `region_share` exists and requires a region handle
  with `share`.
- **Attenuation** narrows rights, scope and lifetime, and widening is
  unexpressible (`CAPABILITY_V1` §4). Carving a smaller range out of a region a
  process already holds is therefore expressible **without any new operation**.
- **Delegation** is sending a capability over an endpoint; **transfer** consumes
  the sender's handle atomically, for capabilities an interface declares linear.
- **Revocation** is the owner invalidating derived capabilities by generation.
- ADR-0055: **no `SYSTEM_ABI_V1` operation produces a capability.** A table is
  written by the nucleus before the process is entered, from an endowment the
  launcher decided, and the recursion ends at the boot process's endowment,
  which is the launcher's stated constant (ADR-0051 §3).

That last point answers the Architect's instruction directly: **a `region_create`
operation is not merely undesirable, it is already ruled out.** Any output
backing must arrive as an endowment or as an attenuation of one.

**The six questions, answered as the contracts stand.**

1. *Which object owns the backing?* **None that exists.** The only accepted
   object that owns memory today is a process's `RuntimeMemoryGrantV1`, which is
   the process's own arena and dies with it. There is no accepted region object
   over pool memory that outlives its writer.
2. *Where does its identity and lifetime live?* A capability entry carries a
   lifetime and a generation, and ADR-0050 gives a grant an identity. **A region
   object's own identity and lifetime are not specified anywhere.**
3. *Who holds the original Region capability?* Necessarily the launch/recovery
   process, by endowment — there is no other lawful origin. Nothing accepted
   says it holds one.
4. *Where does that capability first come from?* The endowment chain ends at the
   boot process's constant endowment (ADR-0051 §3). **No accepted document puts
   a region over spare pool memory into it**, and the nucleus does not carve
   one: `memory::admit_memory` makes a pool, and process creation takes grants
   out of it.
5. *How is the backing reclaimed after the worker or target dies?* Process
   reclamation returns a process's frames (`TOS.RUN.PROCESS_RECLAIMED`). **A
   region that is not a grant has no reclamation rule at all.**
6. *Can the owner still write to the pages after the handoff?* **Yes, and
   nothing in the accepted contracts prevents it.** This is the critical gap:
   - delegation leaves the sender holding what it had (`IPC_V1` §6);
   - `IPC_V1` §6 states that **no Stage 3 object type is declared linear**, so a
     region capability is not consumed by being handed on;
   - there is no sealing operation and no "no writable aliases" property
     anywhere in the contracts;
   - revocation invalidates *derived* capabilities by generation — it does not
     touch the owner's own, and nothing says what happens to a **mapping** that
     an invalidated capability authorized.

   So "the worker lost its writable mapping" is not immutability. A supervisor
   or transaction owner retaining a writable capability could rewrite the bundle
   while the target verifies it. The target verifies anyway, which is why this is
   not a soundness hole in ADR-0073's sense — but §4's sentence "the bundle is
   immutable from here" is **not currently provable**, and an ADR must not claim
   it.

**Superseded in part by ADR-0075 (Draft).** Two corrections belong here rather
than only there: G1 and G2 are **not** free decisions — ADR-0037 revision 3 is
Accepted and already fixes the access modes, the shareability and the
transferability of a region at the type level, so the system contract must
implement that table rather than choose another (ADR-0075 §1). And the sentence
above that `region_create` is "already ruled out" by ADR-0055 is too strong:
ADR-0055 rejects *ambient* creation and explicitly leaves its Option B — a
bounded, self-only creation — as a later decision (ADR-0075 §2). What ADR-0075
recommends instead is narrower still: a region authority whose scope is a frame
range, carved by the attenuation operation that already exists.

**STOP — the normative gaps, exactly.** None of these can be closed by
implementation; each needs an accepted contract change:

| # | Gap | Where it belongs |
|---|---|---|
| G1 | The region object type has no declared rights set. `region_share` requires `share`, which no contract defines, and `CAPABILITY_V1` §3 enumerates rights for endpoints and processes only | `CAPABILITY_V1` §3 |
| G2 | No access mode — read-only against writable — is expressible as a right, so "writable → sealed" cannot be an attenuation | `CAPABILITY_V1` §3–§4 |
| G3 | No sealing, and no linear region transfer: `IPC_V1` §6 declares no Stage 3 type linear, so handing a region on cannot consume the sender's authority | `IPC_V1` §5–§6 |
| G4 | No rule for what happens to an existing **mapping** when the capability that authorized it is released, revoked or invalidated by generation | `IPC_V1` §5 |
| G5 | No accepted origin for a region object over pool memory that is not a process grant, and ADR-0055 forbids creating one with an operation | a new ADR + `CAPABILITY_V1` §2 |
| G6 | No reclamation contract for a region object independent of the death of a process | a new ADR |
| G7 | **No mutable-to-immutable transition exists at all.** `Region<mut T>` and `Region<T>` are both accepted types and nothing in the corpus turns the first into the second; `share` presupposes immutability rather than producing it | ADR-0075 §3 |

Until G1–G6 are closed there is no Region contract to implement, and §6's ABI
shape cannot be fixed: it would have to name a right (`read-only map`) that no
accepted contract defines.

## 5c. The workspace size question, and why Stage 3 does not turn on it

§5a says the smallest declared workspace the worst measured build fits in is a
measurement rather than a bound, and that remains true. **Nothing in the
implemented system depends on it.** A build worker's arena is a *funded role
grant* under ADR-0069 §2a and ADR-0076 §3: a fixed policy figure its creator
names and a presented `MemoryAuthority` pays for, refused with `E_LIMIT` if it
cannot be. The system therefore needs no fixed `BuildWorkspace` constant to be
correct, and this ADR does not introduce one.

## 5d. The measured T1 account

Measured on the reference platform with **both** roles resident, from the
nucleus's own account rather than from arithmetic:

| | frames | |
|---|---:|---|
| supervisor, at the ordinary runtime grant | 14 357 | 56.08 MiB |
| build worker, at the worker role's grant | 25 109 | 98.08 MiB |
| both resident | 39 466 | 154.16 MiB |
| the root authority | 57 410 | 224.26 MiB |
| **left for bundle backing** | **17 944** | **70.09 MiB** |

Held against the largest bundle any Capsule v1 configuration produces — `50.52
MiB`, at 255 modules of 128 KiB, from §5 B — that leaves **19.57 MiB** spare.
The gate asserts it against that figure and deliberately not against the
1147-byte artifact a test boot happens to build.

**The claim, stated precisely.** Stage 3 proves the canonical Capsule-v1
freestanding build/launch path under this topology, with the worst measured
Capsule-v1 bundle fitting the measured headroom. It does **not** claim that a
future installed-source backend can freestanding-build every generative docs/44
ceiling corpus with the same supervisor resident. ADR-0073's separation of the
reference algorithm, the Capsule-v1 boot path and a future installed-source
backend is not collapsed, and no docs/44 ceiling moves.

## 6a. Process creation, as built

> This supersedes §6. Shape A was chosen and is `SYSTEM_ABI_V1` operation 20.
> §6 below is **history**: the comparison that led to the choice, kept for the
> argument against shape B, which is still the reason a capability binding is
> never special. None of its numbers, registers or rules is normative.

What was built, and where it differs from the sketch:

| §6 A sketch | Operation 20 as accepted |
|---|---|
| operation numbers 16 and 17 | **20**; 16 and 17 became `capability_attenuate_scoped` and `region_allocate` |
| a second operation for the restart generation | one operation, with the generation in `CreateFundedRecord` under a flag — because absence and a zero must stay apart (ADR-0067, ADR-0076) |
| `rsi` = the bundle region | `rdx` = the bundle, a **shared** region with `read`. `rsi` is the `MemoryAuthority` that pays, which shape A predates (ADR-0076) |
| the endowment in `CREATE_ENDOWMENT` | a **sealed launch plan** (ADR-0077); the raw table is removed |
| "the entry path is a declared input, checked by the runtime" | **no entry, path or ordinal at all.** The bundle declares its own entry, and a caller-supplied one would be a second truth about which program this is |

The last row is the one substantive reversal, and it is a strengthening rather
than a weakening: the sketch wanted a supervisor to name what it meant to run so
a mismatch could be caught, and what replaced it removes the possibility of a
mismatch. Everything §6 required of *both* shapes holds — the nucleus does not
know what a `TOSIMAGE` is, a bundle is an opaque immutable range, the target
parses and verifies it itself, restart-generation semantics survive, capability
authority stays explicit, the target cannot widen the closure it was given, and
nothing is looked up ambiently.

**The affine form is refused, which the sketch could not have said.** Operation
20 requires the *shared* region: a target gets a window of its own while its
creator keeps one, which is two holders, and `share` is the operation that makes
that possible.

## 6. Process creation: two shapes, and a recommendation

**Historical. Superseded by §6a.**

Both shapes must hold the same line: the nucleus does not know what a
`TOSIMAGE` is, a bundle is an opaque immutable range to it, a module is named by
path and never by an ordinal, the target parses and verifies the bundle itself,
operations 8 and 15 keep their meaning exactly, no reserved or magic binding
exists without a normative contract, restart-generation semantics survive,
capability authority stays explicit, the target cannot widen the closure it was
given, and nothing is looked up ambiently.

### A — a new operation that takes the bundle as a capability

**The Architect chose A as the direction on 2026-08-29, and the numbers,
registers and rights below are not accepted.** They cannot be: the shape names a
region right that §5b shows no contract defines. What follows is a sketch to be
returned to *after* the Region contract exists, and operations 8 and 15 stay
exactly as they are either way.

`SYSTEM_ABI_V1` §3 already assigns a shape for an operation requiring two
capabilities (operation 13 is the precedent): they occupy `rdi` and `rsi` in §5
order, and the operation's own values start at `rdx`.

```text
(numbers illustrative; not claimed, not accepted, not implemented)
16  process_create_from_bundle
    rdi  process-authority capability
    rsi  region handle over the bundle, with whatever right the Region
         contract ends up defining for "may be mapped, not written"

    rdx  how many capabilities the child is endowed with
    r10  the rights the child holds over itself
    r8   the length of the expected entry path
    r9   flags, zero in v1
    argument region: CREATE_MODULE   = the expected entry path
                     CREATE_ENDOWMENT = the endowment, as for operation 8
    returns: rax = status, rdx = the child's capability handle

17  process_create_from_bundle_with_generation
    16, plus r9 = the supervisor-asserted restart generation
```

- **The nucleus reads none of the bundle.** It maps the region read-only into
  the new address space and records its base and length in the launch record.
  What it validates is what it already validates about a region: that the handle
  is one the caller holds, with the right named, over a range this boot admits.
- **The entry path is a declared input, checked by the runtime.** The target
  compares it against the entry path inside the bundle and refuses a mismatch,
  so a supervisor still names what it meant to run, and the check happens in the
  component that can read the bundle.
- **The closure cannot be widened**: there is no second bundle argument and no
  path lookup, so what the target may reach is exactly what it was handed.
- **Restart generation survives** as the 8/15 pair does, by being a separate
  operation whose only difference is a supervisor-asserted number that is
  recorded and never computed.
- **Compatibility.** Operations 8 and 15 are untouched. An older nucleus answers
  16 and 17 with `E_NOT_SUPPORTED`, which §7 already requires of an unknown
  operation number, so a newer runtime discovers the absence rather than
  misbehaving. A newer nucleus with an older runtime is unaffected: nothing
  calls the new numbers.

The cost is honest and small: two operation numbers, and one more thing a
supervisor can hold authority to do.

### B — the existing `process_create`, with the bundle in the endowment

The bundle would travel as an ordinary endowed capability, and operation 8 would
be unchanged.

It does not work, for a reason that is structural rather than awkward. The
nucleus builds the launch record **before** the child runs, and that record has
to say where the child's source or bundle is. If the bundle arrives as one entry
of an endowment, the nucleus must know *which* entry it is in order to build the
record — and a nucleus that treats the entry named `launch` (or at a fixed
index, or of a distinguished type) as the thing to launch from has exactly the
reserved binding this project has refused everywhere else. That is a second
launch ABI, undeclared, expressed through a capability name.

The alternative inside B is worse: leave the launch record naming a module from
the boot template, let the child start, and have the child then discover a
bundle among its capabilities and re-launch itself from it. That makes every
process start twice, gives the child a launch decision the supervisor was
supposed to make, and leaves the nucleus unable to say what a process is running
— `PROCESS_IDENTITY_V1` is a record made by whoever held `process_create`, and
under this variant it would describe something the process no longer is.

### Recommendation

**A** as the direction — which the Architect has taken — and **no ABI shape
until the Region contract exists.** The entry path stays a declared input the
runtime checks, whatever the eventual registers are.

B is only cheaper until it is written down, and what it saves is an operation
number — which is the one thing this system has never been short of. What it
costs is a rule that a particular capability binding is special, which is the
kind of rule that is invisible in the ABI document and load-bearing in the
implementation.

## 7a. The six open questions, answered

Every question §7 left open has since been decided by an accepted contract and
implemented. This is the reconciliation; §7 below is history.

| Question | Answer, and where |
|---|---|
| the origin of the `BuildWorkspace` | a **funded role grant**: a fixed policy figure the creator names, paid for by a presented `MemoryAuthority` (ADR-0069 §2a, ADR-0076 §3). No fixed constant is needed — §5c |
| the lifecycle of a Region object | **ADR-0075** in full: three states, structural affinity, per-process mapping ownership, reclamation on the last of capability references, mappings and internal references |
| the writable → sealed transition | `region_freeze`, `SYSTEM_ABI_V1` operation 18: consuming, in place, with no half-frozen state (ADR-0075 §3) |
| ownership and reclamation after the worker dies | the **capability** owns it. The worker's names go with the worker; the supervisor holds one and the target holds its own; the backing returns when all three counts reach zero |
| when a transferred region is mapped into the target | **at creation**, by operation 20, which gives the target its own capability and its own read-only window |
| one bundle region or several | **one**, decided in §2 and unchanged |

## 7. What this draft does not decide

**Historical. Superseded by §7a.**

The Architect's six open questions are unchanged except where §1, §2 and the
measurement bear on them:

- **the origin of the BuildWorkspace** — §5 proposes a size, not a source;
- **the lifecycle of a Region object** — §4 proposes a shape, §5b shows the six
  contract gaps that have to close before any of it can be written;
- **the writable → sealed transition** — §4 proposes that it is the handoff;
  §5b G2 and G3 show that no accepted contract can express it, and that §4's
  claim of immutability is not provable today;
- **ownership and reclamation after the worker dies** — §4 proposes the
  transaction owner;
- **when a transferred region is mapped into the target** — §6 A places it at
  creation, in a sketch that cannot be fixed until §5b's gaps close;
- **one bundle region or several** — decided in §2: one.
