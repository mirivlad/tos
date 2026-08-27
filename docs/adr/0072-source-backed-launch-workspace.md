<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0072: The launch workspace, and what a process grant is for

- Status: **Accepted**
- Date: 2026-08-27
- Decision level: 2 — it separates the memory a system spends building an
  executable closure from the memory a process spends running one, gives each an
  owner and a lifetime, and fixes how canonical source reaches the frontend. It
  changes no TOS Core semantics, no ABI operation, no accepted ceiling and no
  trust boundary
- Project Architect approval: **given, 2026-08-27**
- Evidence: `docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`,
  `docs/evidence/STAGE3_PROCESS_GRANT.md`, `docs/evidence/STAGE2_ARENA_BOUND.md`,
  `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md`
- Related: ADR-0069 (Accepted) — the `54 MiB` grant this ADR says what is *for*;
  ADR-0071 (Accepted) — the bounded residency the grant holds; ADR-0070
  (Accepted) — the compact image this ADR keeps disposable; ADR-0040 — the
  whole-machine budget both accounts are spent from; ADR-0002 — recovery from
  canonical source, which point 5 below exists to preserve

## The measurement that forced this

The production path was measured end to end, through the production API, inside
a hard allocator whose whole arena is exactly `RuntimeMemoryGrantV1 = 54 MiB`.
Every representation cost that could be removed had already been removed: the
lowered closure is no longer retained (ADR-0071's phased lowering), a
dependency's lowering view is packed and dies when its last consumer does, and
normalized sources no longer accumulate.

**Source to running program, inside the grant:**

| Closure, ceiling-sized modules | Peak inside `54 MiB` | Margin |
|---:|---:|---:|
| 2 | 37.87 MiB | 16.13 MiB |
| **4** | **50.86 MiB** | **3.14 MiB** |
| 8 | **does not fit** | — |

**The same path at the declared closure ceiling of 256 modules**, measured on
three graph shapes with a large enough arena to complete:

| | A chain | B wide fan-in | C balanced DAG |
|---|---:|---:|---:|
| internal peak | 345.01 MiB | 346.41 MiB | 351.25 MiB |
| source backing beside it | 63.99 MiB | 63.75 MiB | 64.00 MiB |

Against ADR-0040's `256 MiB` machine, of which the measured pool after the
nucleus is about `229.8 MiB`, this misses by more than a factor of two.

Two accepted requirements were being asked to hold at once and could not:

- `docs/44` §2 declares a closure ceiling of **256 modules** of **256 KiB**;
- the freestanding reference path compiled that closure **from source, inside
  the process's own `54 MiB` grant**, at boot.

Nothing was wrong with either requirement. What was wrong was the assumption
joining them: that the memory a process is granted to *run* a program is also
the memory a system uses to *build* one.

### One correction to the evidence

The `63.99 MiB` of source beside those runs is a **counterfactual
physical-accounting fixture**, not a capsule. Capsule v1's accepted maximum is
`MAX_CAPSULE_BYTES = 32 MiB` (`source/crates/capsule/src/lib.rs`), so a source
set of that size **cannot be a valid Capsule v1** and no measurement here should
be read as one. It exists to answer one question honestly — *if that much
canonical source were physically resident, what would it cost* — because
caller-owned bytes are not free bytes. §6 below says where such a set would
actually come from, and §8 says that Capsule v1 is not it.

## 1. The grant is the process's, and only the process's

`RuntimeMemoryGrantV1 = 54 MiB` (ADR-0069) is the memory of a **running
process**: its bounded decoded residency (ADR-0071), its derived indexes, its
frame and value state, and whatever the program itself allocates.

It is **not** a workspace for the frontend, the checker, the resolver, the
lowerer or the verifier over a whole source closure. It never was, in intent;
it had merely never been said, and an implementation that ran the whole pipeline
inside the process made the omission look like a decision.

`54 MiB` does not change. What changes is what it is asked to hold.

## 2. Two owners, two lifetimes

```text
            SYSTEM LAUNCH / RECOVERY                  transient

canonical source
      -> explicit SourceProvider
      -> read / parse / check / resolve / lower
      -> untrusted TOSIMAGE/v1
      -> verifier-owned verify_image
      -> records + closure manifest + image store
      -> the launch workspace is released

            PROCESS                                   for the run

RuntimeMemoryGrantV1 = 54 MiB
      -> bounded Residency
      -> explicit-frame engine
```

**The launch workspace** belongs to the system launch and recovery path. It
holds source snapshots, parse trees, summaries, the closure plan, lowering
views, verification surfaces, one module's IR at a time and the verifier's own
scratch. Every one of those is transient: none of it may outlive the launch, and
the boundary is the point at which it is all released.

**The process grant** belongs to one process and lasts as long as it runs.

What crosses the boundary is exactly what ADR-0071 says survives verification:

```text
immutable image backing
VerifiedModuleRecord, one per module
VerifiedClosureManifest
the entry module's receipt
fixed execution metadata (the declared envelope)
```

Nothing else. A frontend result, a parse tree, a summary or a lowering view that
crossed this line would be a component of the build outliving the build.

## 3. The workspace bound is declared, never discovered

The launch workspace is bounded like everything else in TOS: by a **declared,
fixed** figure, proven on the ADR-0040 platform before it is accepted.

It must not be `available_memory()`, `free_frames_now()`, or whatever is left. A
launch whose success depended on how much memory happened to be free would be
reporting a fact about scheduling as a fact about the program — the same
argument ADR-0071 §7 makes about residency, one stage earlier.

The number is **not fixed by this ADR**. It is measured first; the measurement
is this ADR's evidence gate, and the figure is recorded there.

## 4. The pipeline, and where trust lives

```text
canonical source
  -> frontend
  -> lower
  -> untrusted TOSIMAGE/v1
  -> verifier
  -> records + manifest
  -> process
```

Unchanged from ADR-0070 and ADR-0071, and restated because moving the frontend
out of the process must not be read as moving trust with it:

- the **frontend and lowerer are not authority**. No receipt they produce is
  accepted by anything. What they emit is an *untrusted* image;
- the **verifier is the only semantic trust boundary**. It reads the image, is
  handed a declared resolution, and reaches its own conclusion;
- the **exact executable closure is fully verified before the first
  instruction**, one module at a time (ADR-0071 §1);
- a cache supplies **bytes, never conclusions** (ADR-0071 §3).

Splitting the lifecycle changes who pays for the build. It changes nothing about
who is believed.

## 5. Source is canonical; the image is disposable

`TOSIMAGE/v1` is a **derived, disposable artifact**. A prebuilt image may skip
the frontend and the lowerer for an ordinary launch:

```text
image present  ->  verify  ->  launch
```

But an image is an acceleration, not a state of record. Deleting every image on
a machine must never cost it the ability to reach the same executable state:

```text
image absent   ->  canonical source must regenerate it
```

This is ADR-0002's recovery property, and it is why §6's provider exists at all.
A system that could only run what it had already compiled would have made a
cache into the source of truth.

## 6. Source arrives through an explicit, closure-bounded provider

Source reaches the frontend through a `SourceProvider`, and the provider is
**not module search**.

```text
resolved exact source closure
      -> opaque SourceModuleId
      -> SourceProvider
      -> immutable canonical source snapshot
```

Two stages, and the order is the point.

**Resolution** may consult source-set metadata — that is what resolution is. Its
result is a closed membership: a `SourceClosureManifest` and the opaque
`SourceModuleId`s it mints, and nothing else can mint one.

**Materialization** then answers exactly one question: *given an identity this
resolution produced, return the canonical bytes.* On every read the content
identity is checked against the exact resolved closure. If the source is gone,
has changed, or the provider returns different content, **preparation fails**.
There is no search for an alternative.

The provider therefore:

- does not enumerate arbitrary modules;
- takes no path or name for ambient lookup;
- cannot widen the closure — there is no identifier for a module outside it;
- has no filesystem, network or environment fallback;
- returns source, never a receipt, a `Module`, an image conclusion or any other
  trusted object.

## 7. Both accounts are physical memory

The launch workspace and the process grant are different owners with different
lifetimes, and they spend from **the same physical machine**.

So does the source backing, and so does the image backing. Being outside
`RuntimeMemoryGrantV1` does not put bytes outside the ADR-0040 budget:

```text
caller-owned                 != free
outside the process grant    != outside the physical memory budget
```

The whole-machine ledger carries all of it: source backing, transient workspace
peak, persistent launch products (images, records, manifest), the process grant,
and the nucleus's own memory. The bound to satisfy is

```text
transient LaunchWorkspace peak
  + persistent launch products
  + process grants
  + nucleus and platform memory
<= the ADR-0040 machine
```

with the workspace's peak and the grants overlapping only where the lifecycle
actually requires them to.

## 8. Capsule v1 is unchanged, and is not a universal closure transport

Capsule v1 stays exactly what it is: a **source-bearing boot and recovery
transport**, `MAX_CAPSULE_BYTES = 32 MiB`, carrying canonical `.tos` text.

It is not extended, it does not gain an image payload, and it is **not declared
to be a container for every admissible TOS Core V1 closure**. The declared
ceiling of 256 modules at 256 KiB describes what the *language and the verifier*
admit; it was never a claim about what one boot transport carries. Conflating
the two is what produced a `64 MiB` "capsule" in a measurement, and §0 above
marks that fixture for what it is.

## 9. Where larger source sets come from is not decided here

A source set larger than a capsule needs a different `SourceProvider` and a
different persistent representation. **This ADR does not choose one.** It fixes
the interface, the closure-bounded discipline and the lifetime; the backend
behind it — an installed source tree, a content-addressed store, something else
— is a separate decision with its own evidence.

Deliberately so: the interface is what the rest of the system depends on, and
committing to a storage design in the same breath would tie a boundary that is
now provable to a backend that is not yet measured.

## 10. An image-bearing boot transport is a separate decision

If a machine one day wants to boot from prebuilt images rather than compile at
launch, that is a **new format and a new boot decision**, taken on its own
evidence. It is not introduced here.

Whatever such a transport looked like, it would be a derived acceleration
artifact under §5: it could not replace canonical source, and correctness could
never depend on it being present.

## 11. Nothing else moves

- `docs/44` ceilings are unchanged — not lowered, not raised;
- the ADR-0040 reference platform is unchanged;
- ADR-0069's `54 MiB` grant is unchanged;
- Capsule v1 is unchanged;
- TOS Core semantics, the ABI and the verifier's trust model are unchanged.

## What this ADR is answerable for

Three claims that this decision deliberately keeps apart, because the first does
not imply the third:

- **A — the language and reference implementation.** That the frontend, the
  lowerer and the launch workspace are bounded *independently of how much source
  the machine happens to hold resident*, at the declared ceilings.
- **B — the Capsule v1 boot path.** What source set can actually be delivered
  through a `32 MiB` capsule, and that it works within its own bound.
- **C — a full installed-source production path.** Whether a real production
  `SourceProvider` and backend exists that can deliver larger canonical closures
  on an ADR-0040 machine.

Evidence for A and B does not establish C, and no report under this ADR may
present it as if it did.
