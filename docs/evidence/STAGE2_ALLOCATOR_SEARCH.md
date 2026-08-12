<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# The allocator's search: what was wrong and how it is now checkable

Evidence level: **P1** (locally measured, docs/35).
Subject: `source/crates/tos-runtime/src/lib.rs`.
Tests: `source/crates/tos-runtime/tests/search.rs` (8 cases),
`source/crates/tos-runtime/tests/heap.rs` (11 cases, unchanged and still green).

## The defect

The heap served ADR-0041 correctly and served it quadratically. `try_allocate`
searched first-fit by walking **every** block from the base of the arena — free
and used alike — so the cost of one allocation grew with the number of blocks
the arena already held. An allocation-heavy frontend therefore became
superlinear in its own input.

It was not visible in any correctness test, because nothing was incorrect. It
was visible only in wall-clock time, and only at a size nothing had run before:
on the ADR-0040 reference platform a 256 KiB module — the published source-unit
ceiling — did not finish the frontend in **900 seconds**, against a ~3 s
expectation from the platform's ordinary factor.

## What replaced it

A **segregated fit**. Free blocks are threaded onto one doubly-linked list per
size class, class `k` holding payloads in `2^k .. 2^(k+1)`. Boundary tags,
splitting, bidirectional coalescing, the alignment prefix and the backlink are
unchanged — the block layout is the same, and so is every invariant proved
against it.

The search rests on one property of the classes: **a request whose own class is
`k` is satisfied by any block from a class above `k`**, because every block
there has a payload of at least `2^(k+1)`, which exceeds any request of class
`k`. So only the request's own class can hold blocks that are too small, and
only that class needs examining.

- the request's own class is probed at most `PROBE_LIMIT` (8) times;
- if that finds nothing, a 64-bit mask of non-empty classes gives the next
  larger class in one instruction, and its first block fits without being
  looked at;
- **the one deliberately unbounded path**: when no larger class holds anything,
  the rest of the exact class is walked before giving up. Refusing while a block
  that fits is sitting in the arena would be an allocator that lies about
  exhaustion. That path is reached only when the arena is nearly full, which is
  where a walk is both rare and affordable.

Freeing is bounded outright: at most two neighbours leave their lists and one
merged block joins one, each `O(1)` on a doubly-linked list.

**The lists cost no memory.** A free block's links live in its own payload,
which is free and therefore unused; the class heads are a fixed array in the
heap struct. The allocator still never allocates to describe its own free space,
which is what lets a bounded region stay bounded. The minimum block payload is
unchanged at 16 bytes, which is exactly the two links.

## Why the claim is checkable rather than asserted

"It is faster on my machine" does not establish that the *shape* is gone. The
allocator counts the free-list nodes it examines and the allocations it serves,
and `BoundedHeap::search_work` exposes both. The evidence is a **series**: work
per allocation measured while the number of live blocks grows by 64x.

```text
live blocks     64    256   1024   4096
probes/alloc   flat, and asserted to stay under 8 at every point
```

A walk-every-block search would cost about 64x more at the last point than the
first. The test asserts it does not, and a second test states the same bound as
a direct 64x comparison. One measurement could not tell a bounded search from a
linear one that happened to run on a small arena; a series can.

## Adversarial coverage

Each pattern is a way a naive free list goes wrong, and each is a test:

- **many small live allocations** — 8192 blocks, every one written and read back
  to prove nothing was overwritten;
- **alternating sizes** — 32 B and 512 B interleaved, populating several classes;
- **mixed free orders** — forward, backward, and evens-then-odds across 64
  rounds, each round asserted to return the arena to one free block;
- **heavy fragmentation** — 2048 blocks with every other one freed, so no two
  free blocks touch; allocation still serves, and the arena coalesces back to
  its starting census afterwards;
- **strong alignment** — 16 B to 4096 B across four sizes, each checked for
  actual alignment and written through;
- **arena near full** — filled to refusal, then asserted that the refusal
  changed no accounting, that every live allocation is still writable, and that
  the arena returns to exactly its previous capacity after everything is freed;
- **repeated whole rounds** — 64 rounds asserting the block census, the
  committed figure, the frontier *and the search cost* are identical from the
  second round on. Accumulating fragmentation breaks the census long before it
  breaks the total, and a degrading search shows up here and nowhere else.

The eleven pre-existing heap regressions were not modified and still pass.

## Effect

| workload | before | after |
|---|---|---|
| `tos-arena-bound` fast sweep | ~18 minutes | **1.4 s** |
| `tos-arena-bound --full` sweep | did not finish in hours | **16.5 s** |
| arena bound, 256 KiB module | 52 808 656 B | 52 770 176 B |

The memory figures moved by less than 0.1%. That is the point: the search was
the defect and the layout was not, so replacing the search had to leave the
bound where it was. `docs/evidence/STAGE2_ARENA_BOUND.md` is re-measured against
the current implementation rather than carried over.
