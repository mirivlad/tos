<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0074: The build workspace, the launch bundle, and who owns them

- Status: **Draft — not accepted, and nothing in it is implemented**
- Date: 2026-08-29
- Decision level: 2 — it fixes where a build's products live, what crosses the
  boundary between a build and a run, and which component owns that memory over
  time. It changes no TOS Core semantics, no accepted ceiling, no trust rule and
  no ABI operation
- Project Architect approval: **not given.** Two decisions inside it were taken
  by the Architect on 2026-08-29 and are recorded as such in §1 and §2; the
  rest is a proposal
- Evidence: `docs/evidence/STAGE3_BUILD_WORKSPACE.md`,
  `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`
- Related: ADR-0073 (Accepted) — the build leaves the target process and the
  verifier stays in it; this ADR says where its output goes. ADR-0072, ADR-0071,
  ADR-0070, ADR-0069, ADR-0040. `IPC_V1` §5 — regions. `SYSTEM_ABI_V1` §5 —
  operations 7, 8 and 15

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

`TOSBUNDLE/v1` (`source/crates/tos-bundle`) is that format, written and read as a
host-backed reference implementation, with a total bounded parser. No region, no
ABI operation and no freestanding lifecycle is implemented.

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
workspace the worst measured build fits in is `81 100 800 B` — one enforced
`40 960 B` below that, the same build fails to allocate.

The two paths are differentially equivalent: the same source through
`build_from_provider -> admit` and through
`build_into_bundle -> admit_bundle` produces the same receipt from the target's
own verifier, the same value and the same accounting.

## 4. Proposed: the lifecycle is a transaction, and the handoff is linear

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

## 5. Proposed: a fixed BuildWorkspace size

Derived, not chosen for its shape:

```text
enforced minimum, worst measured shape          81 100 800 B   (77.34 MiB)
+ 11 %, the spread between the measured shapes   8 921 088 B
+ a quarter of one turn's scratch                6 608 812 B
rounded up to a page                            96 632 832 B   (92.16 MiB)
```

The two margins are the two things that vary and that a fixed size cannot
measure in advance: **the shape of the graph**, which was `69.54` to
`77.14 MiB` across the three measured shapes and which is a property of the
program rather than of the build; and **the weight of one module's turn**, which
is `25.21 MiB` of the total for the fixture's ceiling-sized modules and would be
larger for a module whose body is heavier than records and small functions.

**It is a tight fit on the reference platform, and that has to be said.**
ADR-0040's machine leaves about `229.8 MiB` after the nucleus. At the docs/44
ceiling the build phase needs the capsule source mapped (`32 MiB` at the Capsule
v1 ceiling), the workspace, and the bundle it is filling:

| Phase | What is resident | At the docs/44 ceiling |
|---|---|---:|
| build | capsule + workspace + bundle | `32 + 92.16 + 100.87` = **225.03 MiB** |
| after handoff | bundle + target grant and its overhead | `100.87 + 56.08` = **156.95 MiB** |
| pool after the nucleus | | **~229.8 MiB** |

That leaves `4.77 MiB` at the peak, before the worker's own per-process overhead
(measured at `2.08 MiB` in `STAGE3_PROCESS_GRANT.md`) and before any page tables
for the mappings. **It does not fit with any margin at all**, and with the
measured minimum instead of the proposed size it fits by `19.8 MiB`.

So one of these is true, and choosing between them is not this ADR's to do:

- the reference platform does not build at the docs/44 ceiling, and the ceiling
  it does build at is smaller — at 128 ceiling-sized modules the same three
  numbers are `32 + 92.16 + 50.63` = `174.79 MiB`, which is comfortable;
- or the bundle does not stay whole in memory during the build, which is a
  different §2 than the one decided;
- or the reference platform is not where a ceiling build happens.

The measured slope makes the first concrete: the workspace is flat at
`36.4–36.5 MiB` from 8 to 64 modules, `42.98 MiB` at 128 and `77.14 MiB` at 256,
while the bundle grows at `0.39 MiB` a module.

## 6. Process creation: two shapes, and a recommendation

**Not implemented. No operation number is claimed by this draft.**

Both shapes must hold the same line: the nucleus does not know what a
`TOSIMAGE` is, a bundle is an opaque immutable range to it, a module is named by
path and never by an ordinal, the target parses and verifies the bundle itself,
operations 8 and 15 keep their meaning exactly, no reserved or magic binding
exists without a normative contract, restart-generation semantics survive,
capability authority stays explicit, the target cannot widen the closure it was
given, and nothing is looked up ambiently.

### A — a new operation that takes the bundle as a capability

`SYSTEM_ABI_V1` §3 already assigns a shape for an operation requiring two
capabilities (operation 13 is the precedent): they occupy `rdi` and `rsi` in §5
order, and the operation's own values start at `rdx`.

```text
16  process_create_from_bundle
    rdi  process-authority capability
    rsi  region handle over the bundle, with the right to be mapped read-only
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

**A**, with the two operation numbers, and with the entry path kept as a
declared input the runtime checks.

B is only cheaper until it is written down, and what it saves is an operation
number — which is the one thing this system has never been short of. What it
costs is a rule that a particular capability binding is special, which is the
kind of rule that is invisible in the ABI document and load-bearing in the
implementation.

## 7. What this draft does not decide

The Architect's six open questions are unchanged except where §1, §2 and the
measurement bear on them:

- **the origin of the BuildWorkspace** — §5 proposes a size, not a source;
- **the lifecycle of a Region object** — §4 proposes a shape and implements none
  of it;
- **the writable → sealed transition** — §4 proposes that it is the handoff, and
  no mechanism for it;
- **ownership and reclamation after the worker dies** — §4 proposes the
  transaction owner;
- **when a transferred region is mapped into the target** — §6 A places it at
  creation, in a proposal that is not implemented;
- **one bundle region or several** — decided in §2: one.
