<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Decision packet — where a Region is mapped, and what that costs

- Status: **open — Project Architect decision required before operation 17**
- Date: 2026-09-01
- Why it is a STOP: operation 17 is the first operation that maps memory a
  caller asked for, and the page-table reserve is sized from a fixed list of
  regions that does not include one. Choosing a virtual-address policy while
  writing the operation would fix the reserve's bound as a side effect of an
  implementation detail
- Related: ADR-0075 §5a, ADR-0076 §5, §7; `SYSTEM_ABI_V1` §3; the reserve in
  `process::table_reserve`

## 1. What is already fixed, and is not being reopened

- **The nucleus chooses the virtual address.** A caller never supplies one and
  never supplies a pointer (ADR-0076 §5, `SYSTEM_ABI_V1` §3). A region is named
  by a handle; base and length come back from the operation that established the
  mapping.
- **Freezing downgrades in place.** `region_allocate` places a writable,
  non-executable window; `region_freeze` turns that same window read-only at the
  same address. There is no unmap, no remap and no `region_map` in V1.
- **A linear transfer takes the sender's mapping with the handle**, atomically,
  before ownership is anywhere else (ADR-0075 §5a); the receiver is given its
  own window and learns it the way any other capability arrives.
- **Physical backing is already bounded** by the authority tree: nothing can be
  mapped that was not charged to a `MemoryAuthority` first.

## 2. The problem this has to solve

`process::table_reserve` bounds the page tables one address space can need by
walking a fixed list — `IMAGE`, `SOURCE`, `RECORD`, `GRANT`, `STACK`, `REPORT`,
`ARGUMENTS` — each at the largest the accepted limits allow. On the reference
machine that is 123 frames per space and 517 in total.

A region window is not in that list, and the naive extension is ruinous. With
`MAX_REGIONS = 64` and a worst-case window each, a reserve sized for "every
region gets its own worst-case span" is tens of thousands of frames: the
reserve would be larger than the memory it exists to map. The reserve must stay
derived and it must stay small, so the virtual-address policy has to be part of
the same decision.

## 3. Recommendation — one aperture per address space

A single, finite, nucleus-chosen span in every process's address space, into
which every region that process holds is packed:

```text
REGION_APERTURE       one span, fixed base, fixed maximum length
  ├── region A        placed by the nucleus, frame-aligned
  ├── region B
  └── …               no two overlap; a hole is reusable
```

What makes this the right shape rather than the convenient one:

- the caller never chooses an address, so packing is the nucleus's business and
  no contract mentions it;
- total physical backing is already bounded by the tree, so an aperture at least
  as large as the largest fundable total is never the binding constraint;
- one span costs page tables for **its own extent**, once, not per region — the
  bound stops depending on `MAX_REGIONS` entirely;
- and a region's window can be freed and its span reused, because nothing
  outside the nucleus recorded where it was.

Sized, not invented: the aperture's maximum should be the largest region
backing one process could hold at once, which is bounded by what a single
`MemoryAuthority` can be endowed — and on the reference platform that is
bounded by the root, so the aperture is the root's budget rounded up. Its page
tables then cost `1 + spans/512 GiB + spans/GiB + spans/2 MiB`, the same
arithmetic every other window uses.

**What is explicitly not proposed:** a per-region window, a caller-chosen base,
and any aperture whose size depends on a count of regions rather than on an
amount of memory.

## 4. What the decision has to answer

1. **Base and maximum size.** One aperture, its fixed base in the process
   layout, and a maximum derived from the largest fundable backing rather than
   chosen.
2. **Alignment.** Frame alignment is the minimum; 2 MiB alignment for large
   regions would let the pager use huge leaves and cost no page table at all,
   at the price of internal fragmentation.
3. **Allocation and reuse within the aperture.** First fit, and whether a freed
   span is reusable immediately or the aperture is a bump allocator that a
   process can exhaust without exhausting its memory.
4. **The fragmentation rule.** What a process is told when its backing fits but
   its aperture does not — a refusal the memory account cannot explain is worse
   than a smaller aperture.
5. **The page-table bound this adds**, computed the way `table_reserve` computes
   the rest, and the new reserve figure for the reference machine.
6. **Several regions at once**, including the maximum a process may hold and
   whether that is a separate bound from `MAX_REGIONS`.
7. **A shared region in several address spaces.** Each holder's aperture is its
   own, so one region has a different address in each — which the contract
   already allows, since nobody outside the nucleus records an address.
8. **Invalidation on transfer, release and death.** A linear transfer must
   remove the sender's mapping before ownership moves; a release removes the
   holder's; a death removes all of that holder's. Each is a TLB invalidation
   the nucleus owes.
9. **Interaction with the table reserve.** Whether region page tables come from
   the same reserve — they must, since nothing else may take frames the tree has
   promised — and what that does to the bound.

## 4a. The derivation, and the collision it found (added 2026-09-01)

The bound was derived and measured before being deferred, because a bound
nobody computed is not a bound. With `P` the memory the pool admitted:

```text
a = ceil(P / 512 GiB)    b = ceil(P / 1 GiB)    c = ceil(P / 2 MiB)

backing index   1 + (a + R - 1) + (b + R - 1) + (c + R - 1)
per process         (a + C - 1) + (b + C - 1) + (c + C - 1)
```

Spreading `P` over `slots` lanes costs one extra table per level per lane after
the first, because each lane starts on a fresh 512-GiB boundary; the total
backing alive at once cannot exceed `P` however it is divided. A process is
bounded by `C = MAX_CAPABILITIES`, not by `R = MAX_REGIONS`: it cannot reach
more distinct regions than it can hold handles for.

On the reference machine — `P = 58 909` frames, `a = 1`, `b = 1`, `c = 116`:

| | frames |
|---|---:|
| existing reserve | 517 |
| backing index | 308 |
| region mappings, 4 processes | 652 |
| **reserve with lanes** | **1 477** |

**And that is 17 frames more than the platform has.** Four grants of `14 356`
frames need `57 424`. A root endowed over what is left after a 1 477-frame
reserve — and after the nucleus's own 23-frame address space — holds `57 407`.
Sixty-eight kilobytes short, and the fourth process stops being creatable; the
lifecycle gate found it on the first run.

So the lane bound is **not** in `table_reserve` today. Reserving for lanes that
nothing can yet allocate buys nothing and costs a process that works, so it is
added in the slice that starts using it. What it needs first is a decision, and
the honest options are all Level-2:

1. **Fewer simultaneous processes**, or a smaller `MAX_CAPABILITIES`, either of
   which lowers the bound directly — `C` appears three times in the per-process
   term.
2. **A smaller `MAX_REGIONS`**, which lowers the backing index by three frames
   per slot removed.
3. **A tighter derivation.** The `c` term assumes a process could map all of the
   machine's memory as regions. It is a true upper bound and may be loose for
   the reference platform, but nothing in the accepted contracts makes it
   looser.
4. **A larger reference platform**, which ADR-0040 fixes and this does not
   reopen.

The measurement is the deliverable here; the choice is not mine.

## 5. What is not blocked by this

Operation 16 is done and green: it maps nothing. Everything below the mapping
contract — the authority tree, funding, the creation transaction, the
capability lifecycle — is in and proved. Operation 17 is the first thing that
needs an answer here, and nothing before it does.
