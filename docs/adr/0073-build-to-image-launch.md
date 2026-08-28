<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0073: Build-to-image launch and verifier-owned process admission

- Status: **Accepted**
- Date: 2026-08-28
- Decision level: 2 — it fixes where a source closure is turned into images,
  what a runtime process is handed, and which component decides that what it was
  handed may run. It changes no TOS Core semantics, no ABI operation, no
  accepted ceiling and no trust rule
- Project Architect approval: **given, 2026-08-28**
- Evidence: `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`, `docs/evidence/STAGE2_ARENA_BOUND.md`
- Related: ADR-0072 (Accepted) — separated the build account from the process
  grant; this ADR moves the boundary to where the freestanding lifecycle put it
  and amends ADR-0072's §2 diagram accordingly. ADR-0070, ADR-0071 (Accepted) —
  the image and the sequential verification this preserves exactly. ADR-0002 —
  recovery from canonical source. ADR-0069, ADR-0040 — the two budgets

## What ADR-0072 got right, and what it left in the wrong place

ADR-0072 separated *the memory a system spends building an executable closure*
from *the memory a process spends running one*, and that separation holds. What
it left unsaid is **where the build runs**, and the implementation answered by
running the whole pipeline — frontend, checker, resolver, lowerer, encoder *and*
verifier — inside the target runtime process, under its own `54 MiB` grant.

That is the shape this ADR replaces. The build side moves out of the target
process. The **verifier stays in it**.

```text
                 BUILD / LAUNCH SIDE            untrusted

canonical source
      -> explicit SourceProvider
      -> read, parse, check, resolve, lower
      -> TOSIMAGE/v1
      -> persistent explicit image store
      -> the build worker ends

                 TARGET RUNTIME PROCESS         trusted, by its own work

exact TOSIMAGE closure
      -> verifier-owned verify_image
      -> VerifiedModuleRecord, VerifiedClosureManifest
      -> bounded Residency
      -> explicit-frame engine
```

## 1. The build side is not a semantic authority

A build worker may read source, parse it, check it, resolve a set, lower it and
encode `TOSIMAGE/v1`. **Its output is hostile bytes.**

No receipt it produces is handed to the runtime process, and none would be
accepted. The rule ADR-0071 §3 states about caches is the rule here, unchanged:

```text
a cache or a provider supplies bytes, never conclusions
```

Only the target process's own verifier decides whether an image is semantically
valid and whether it belongs to the declared exact closure. Nothing about a
correct build makes a run admissible; nothing about a hostile build makes it
unsafe.

## 2. Why the verifier stays in the target process

This is the part of the decision that is not an optimization.

Three alternatives were available and all three are rejected:

- **Put the language stack back in the nucleus.** It would return a parser, a
  checker, a lowerer and a verifier to ring 0, which is the opposite of every
  reason they were taken out.
- **A trusted compiler daemon.** It would make one process's verdict binding on
  another, which is a cross-process trust anchor TOS does not have and this ADR
  does not introduce.
- **Trust a receipt from the build side.** It would make the receipt the thing
  that admits execution, rather than the module the receipt is about — and the
  receipt would have been produced by a component with no authority to produce
  it.

So the target process does it itself:

```text
receive the exact image closure
  -> verify the whole closure, one image at a time
  -> build its own trusted records and membership
  -> only then execute the first instruction
```

Which is ADR-0071 §1 word for word. Nothing about the trust boundary moves; what
moves is the frontend, which never had any.

## 3. Source stays canonical; the image stays disposable

`TOSIMAGE` does **not** become installed state.

```text
the source tree is the installed system
TOSIMAGE is a disposable derived cache
```

The property that has to keep holding:

```text
delete every image
  -> canonical source still regenerates them
  -> the same executable semantics is reached again
```

An image-only installation is not a TOS installation. A prebuilt image is an
acceleration: with the source unchanged and a valid cached image present, the
build stage may do no source work at all — and the runtime still verifies the
image for this launch, because the verification is about *this run*, not about
how the bytes were obtained.

## 4. What Launch v5 carries

The boundary between the nucleus and a runtime process is versioned and
fail-closed. Launch v5 describes an **exact image closure** instead of a source
set.

What a member carries is the minimum needed to materialize the exact image and
to place it in the closure. What it must **not** carry:

```text
no verifier receipt
no VerifiedModuleRecord
no trusted manifest from the build side
no decoded Module
no frontend verdict
```

A process receives untrusted bytes and the declared inputs of its run. The fact
that bytes arrived through `Launch` is not evidence about them. If one image of
the closure fails verification, the process never reaches its first TOS
instruction.

The runtime does not search for modules: the closure it is given is already
resolved. That is a statement about *which* modules, not about whether they are
valid.

## 5. Reload is unchanged

After launch, ADR-0071's reload discipline is exactly as it was:

```text
immutable snapshot
  -> SHA-256 over those exact bytes
  -> compared against the trusted record this launch produced
  -> parsed by a total parser
  -> no second semantic verification
```

## 6. Amendment to ADR-0072

ADR-0072 remains **Accepted**. Its §2 lifecycle is amended: what it calls the
launch workspace is, after this ADR, a **build workspace** that runs outside the
target process and does not contain the verifier; the target process's grant
covers image verification, the trusted records and membership, bounded decoded
residency and execution.

The original shape is not erased. It was replaced for a measured reason: the
real freestanding lifecycle showed the verifier belongs to *target-process
admission*, because that is the only place it can be without either a
cross-process receipt trust, a language stack in ring 0, or a change to
ADR-0071's sequential verification model. Keeping the verifier in the target
process is what let all three be avoided at once.

## 7. What is bounded, and where

Two accounts, two owners, two lifetimes, one physical machine:

```text
BuildWorkspace                     transient, outside the target process
  one source snapshot at a time
  the parse tree and check scratch of one module
  the closure plan
  the live lowering-view frontier
  one Module
  the image encoder's scratch

persistent products of a build      outlive the workspace
  the image payload
  the metadata needed to launch a target process

RuntimeMemoryGrantV1 = 54 MiB       the target process
  sequential image verification
  VerifiedModuleRecord, VerifiedClosureManifest
  bounded decoded residency and its indexes
  frames, values, and what the program allocates
```

Both spend from the same ADR-0040 machine, and both are accounted. Image backing
outside the grant is still physical memory (ADR-0072 §7).

## 8. Nothing else moves

`docs/44` ceilings, the ADR-0040 reference platform, `RuntimeMemoryGrantV1`,
Capsule v1, TOS Core semantics, `SYSTEM_ABI_V1` and the verifier's trust model
are all unchanged by this decision.

## What this ADR is answerable for

The same three claims ADR-0072 keeps apart, restated against this lifecycle:

- **A — the reference algorithm.** That the source-to-image build is bounded at
  the `docs/44` ceilings *independently of how much source is resident*, proven
  with a provider that materializes one unit at a time.
- **B — the Capsule v1 boot path.** What source set a real
  `CapsuleSourceProvider` can actually build within `MAX_CAPSULE_BYTES = 32 MiB`.
- **C — an installed-source backend.** Whether a production provider exists for
  canonical source larger than a capsule.

A does not establish C. An interface is not a backend, a host generator is not a
backend, and a capsule is not a universal one.
