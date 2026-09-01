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

## 4a. The derivation, the collision, and what it actually was (2026-09-01)

The bound was derived and measured before being used, because a bound nobody
computed is not a bound. With `P` the memory the pool admitted:

```text
a = ceil(P / 512 GiB)    b = ceil(P / 1 GiB)    c = ceil(P / 2 MiB)

backing index   1 + (a + R - 1) + (b + R - 1) + (c + R - 1)
per process         (a + C - 1) + (b + C - 1) + (c + C - 1)
```

Spreading `P` over `slots` lanes costs at most one extra table per level per
lane after the first, because each lane starts on a fresh 512-GiB boundary; the
total backing alive at once cannot exceed `P` however it is divided. A process
is bounded by `C = MAX_CAPABILITIES`, not by `R = MAX_REGIONS`: it cannot reach
more distinct regions than it can hold handles for. On the reference machine —
`P = 58 909` frames, `a = 1`, `b = 1`, `c = 116` — that is `308` frames for the
backing index and `163` per process.

### What the first measurement showed, and why it was wrong

Added to the reserve as it then stood, this gave `1 477` frames and the fourth
process stopped being creatable: four grants of `14 356` frames need `57 424`,
and the root held `57 407`. Seventeen frames short. The lifecycle gate found it
on the first run, refusing the fourth creation for want of funding.

**That was not a ceiling problem. It was the nucleus's own address space
reserved twice.** The formula was

```text
identity + MAX_PROCESSES * (identity + windows)
```

and the leading `identity` was right for as long as every page-table tree came
from the reserve — it was the nucleus's own space. It stopped being right in the
same slice that moved that space out of the reserve and into the pool, built
before the reserve exists because until the nucleus owns its own map some of
what the memory map reports usable is still mapped read-only by the firmware.
From that moment those frames were already gone from `Frames`, reported as their
own line of the account, and reserved a second time for a tree that will never
be built again.

The corrected baseline drops the standalone term:

```text
backing + MAX_PROCESSES * (identity + windows + region mappings)
```

which recovers exactly the 25 frames the duplicate cost — more than the 17 the
lanes were short by.

### The reference machine, measured

| | frames |
|---|---:|
| admitted | 58 909 |
| the nucleus's own space, already spent | 23 |
| backing index | 308 |
| per process: identity 25 + windows 98 + region mappings 163 | 286 |
| **page-table reserve** | **1 452** |
| **root authority** | **57 434** |
| four ordinary processes at 14 356 | 57 424 |
| **margin** | **10 frames** |

No ceiling moved. `MAX_PROCESSES`, `MAX_CAPABILITIES`, `MAX_REGIONS`, the
reference platform's memory and the runtime grant are all unchanged, and the
conservative region derivation is the one being used rather than a tightened
one.

The failed measurement is kept here on purpose. It is the evidence that the
four-process lifecycle gate catches an accounting error the arithmetic gate
could not see at the time — and the arithmetic gate now checks the
decomposition, so a future change to boot ordering that reintroduces a
standalone identity term fails on the number rather than on a process that
will not start.

**Ten frames is not comfortable.** It is a true margin under a conservative
bound, and both the bound and the four-process figure are printed by the boot
and checked by the gate, so it cannot quietly become negative. If it needs to be
larger, the honest levers are the ones §4a of the previous revision listed, and
they are all Level-2.

## 5. What is not blocked by this

Operation 16 is done and green: it maps nothing. Everything below the mapping
contract — the authority tree, funding, the creation transaction, the
capability lifecycle — is in and proved. Operation 17 is the first thing that
needs an answer here, and nothing before it does.
