<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0084: Where DMA authority comes from, and what a device-visible address is allowed to be

- Status: **Proposed — not Project Architect-approved. Revision 2.** Written
  before the mechanism, which is the order ADR-0081 §0 recorded going wrong once
  and ADR-0082 restored. Nothing in it is implemented.

  **Revision 2** answers three findings of the first review, and two of them
  changed a decision rather than a wording: `BME=0` is **not** a teardown proof
  and §5a replaces it with one; quarantined memory **stays charged** until the
  frames actually return (§5e); and a `DmaRegion` is **one contiguous
  device-visible extent** whose interior a driver can address (§6a, §6b). §9
  lists what moved
- Date: 2026-09-08
- Decision level: **3** — it admits a third class of authority descending from a
  device assignment, it is the first object with **two** ancestries at once, and
  it decides whether a number the hardware will accept as an address may leave
  ring 0 at all
- Related: ADR-0082 (§5's no-IOMMU wording, §5d's bus-mastering predicate and
  §12's handover), ADR-0081 (§2 `DmaRegion` access, §5 device memory, §11 the
  ordering it deliberately did not decide, §14 descendants), ADR-0076 (the one
  physical account), ADR-0075 (region lifecycle), ADR-0037 (`DmaRegion`
  transferability, already fixed), ADR-0063 (an operation requiring two
  capabilities), `docs/34` S5/T5, `docs/11` §DMA

## 1. The audit, before anything is proposed

**`DmaRegion<T>` is already in the accepted language surface and nothing
produces one.** `docs/40` §2 lists it as a parameterised V1 type; ADR-0037 §2
fixed its four facts — neither form shareable, neither transferable, both for
the reason that a `Shared<T>` is `Copy` and a copied DMA handle is exactly the
crossing the rule exists to forbid; ADR-0081 §2 implemented indexed access
through it. So the *type* is decided, the *access* is decided, and the
**origin** is not. This ADR is only about the origin and what comes with it.

**The reference machine has no IOMMU, and that is measured rather than
assumed.** The Stage 4 profile is `q35` with no `intel-iommu` and no `amd-iommu`
device, and the function is `virtio-blk-pci` at `00:04.0` with
`iommu_platform=off` — so the device does not negotiate
`VIRTIO_F_ACCESS_PLATFORM` and the addresses it is given are guest-physical.
Anything this ADR decides about device-visible addressing is therefore decided
on a platform where a device address **is** a physical address, and must be
written so that it stays true where it is not.

**What already exists and is reused rather than re-decided:**

| | |
|---|---|
| funding | ADR-0076's one physical account. A region is charged to a `MemoryAuthority` and returned to the pool when it dies |
| the region substrate | `region.rs`: allocation to a granule, a generation, counted names, per-process mapping lanes |
| the assignment | ADR-0079 §10, with ADR-0081 §14's descendant rule, which ADR-0082 §6 already used a second time |
| bus mastering | **ADR-0082 §5d, inherited whole.** A DMA mapping is a bus-mastering descendant, and Bus Master Enable is set if and only if at least one live bus-mastering descendant exists. This ADR adds a row to that table and restates nothing |

**What does not exist:** any operation producing a `DmaRegion`, any notion of a
device-visible address anywhere in any contract, and any rule about what happens
to pool memory a device may still be writing.

## 2. The problem this decision actually has

Every earlier hardware object was safe because **ring 0 could take it away**. A
mapped window is unmapped by writing a page table. An interrupt source is
silenced by masking a table entry and retiring its vector. In both cases the
nucleus ends the authority by acting on something it owns.

A DMA region is not like that, and the difference is the whole decision:

> The memory is **pool memory**. It goes back to the allocator and is handed to
> somebody else. The device's copy of the address is in a device register the
> nucleus does not know the meaning of, and on a platform with no IOMMU there is
> no mechanism that makes the hardware refuse the write.

So "release the region" cannot mean "the device stops writing it". Something
else has to mean that, and §5 is where this ADR proposes what.

## 3. D1 — DMA authority requires **two** capabilities, and neither is sufficient

**Proposed shape.**

```text
platform.pci.FunctionConfig   with right `dma`        the device that may reach it
        +
system.memory.Authority       with right `spend`      the memory that pays for it
        ↓  dma_region_allocate
DmaRegion<mut T>              one funded, device-visible region
```

This is ADR-0082 §5's sentence made mechanical rather than aspirational:

> sanctioned DMA authority and device-visible address issuance require *both*
> memory funding authority and the live device assignment, and that is
> mechanically enforced

**Two capabilities and not one, in ADR-0063's shape.** `endpoint_reply_receive`
is the precedent: both are resolved before either is used, and a half-performed
operation is impossible. Here the reason is stronger than symmetry — a process
holding only memory authority must not be able to make any memory reachable by
any device, and a process holding only a function must not be able to spend
somebody's memory to do it.

**`dma` is a fourth right on the function**, separate from `config_read`,
`config_write`, `map` and `interrupt`, by the rule that already separates those:
a holder that may map a device's registers is not thereby a holder that may put
host memory where that device can write it.

**Nothing here takes an address.** The caller says how many elements of what
type it wants. It never presents a physical address, never presents a frame
number, and never presents a device address — the same rule that makes a BAR
data rather than authority.

## 4. D2 — a `DmaRegion` is the first object with two ancestries

It is **both**:

- a region in the memory account (ADR-0076) — funded, charged, reclaimed;
- a descendant of the assignment (ADR-0081 §14) — bus-mastering (ADR-0082 §5d).

Neither ancestry is decorative and neither can be dropped:

| If it were only a region | If it were only a descendant |
|---|---|
| the assignment could end while the device still had the address | nothing would fund it, and ADR-0076's one account would have a hole in it the size of every driver's buffers |

So the assignment stays live while a DMA region descends from it, exactly as it
does for a window and for an interrupt source; and the memory is charged,
accounted and returned exactly as any other region's is. **The novelty is that
one object's death has to satisfy two lifecycles**, and §5 is about the order.

**Mapping attributes are not MMIO's.** A device window is `UC` because a device
register is not memory (ADR-0081 §10). DMA memory *is* memory: on x86-64 the
platform is DMA-coherent for write-back memory, so a DMA region is mapped
**write-back**, user-accessible and not executable, like any other region. A
contract that made it uncacheable would be paying for a coherence problem this
architecture does not have — and would be wrong on the architecture where it
does, because there the answer is explicit synchronisation rather than a
mapping attribute.

## 5. D3 — teardown, and why `BME=0` is not the proof

**The first draft of this section was wrong, and the review was right.** It said:

```text
last bus-mastering descendant → BME clear → frames safe to return
```

Clearing Bus Master Enable stops the function **initiating** new requests. It
says nothing about transactions already issued, and PCIe keeps those three
things apart on purpose:

| | |
|---|---|
| blocking new requests | what `BME=0` does |
| completion of outstanding **non-posted** requests | the function's own reads, whose data returns *to the device* |
| arrival of previously issued **posted writes** at their destination | the ones that land in the frames about to be freed |

Only the third threatens the pool, and `BME=0` does not address it. A posted
write is fire-and-forget: the function considers it done when it is handed to
the fabric, and the fabric may still be carrying it.

### 5a. The generic mechanism that does prove it

**It exists, it is ordinary PCI, and the nucleus already performs it.**

> A **completion may not pass a previously issued posted request** travelling in
> the same direction. A configuration read of the function produces a completion
> that travels **upstream**, along the path the function's DMA writes travel. So
> when the CPU observes the value of that read, every posted write the function
> issued earlier has already been accepted ahead of it.

This is the "a read flushes posted writes" rule every operating system relies on
to stop DMA, expressed over the one transaction type this nucleus can already
issue against any function: `pci_config_read`. So the proposed drain is

```text
1  the region's last name goes; its mapping is removed
2  it stops being a bus-mastering descendant; ADR-0082 §5d re-evaluates
3  if the function is now not a bus master:
      a  a configuration read of **that function** is performed and observed
      b  by the ordering rule, its earlier posted writes are at their destination
      c  only now may the quarantine for that assignment be drained
4  if the function is still a bus master, nothing is drained
```

**Order matters in both directions.** The read must come *after* `BME=0`, or new
writes could be issued behind the flush; and the read must reach the function
while it still answers configuration cycles, which is why nothing about
teardown may power it down or remove it.

**No device knowledge is involved.** A configuration read of an arbitrary
architected offset — the vendor/device word is the obvious choice, because every
function has one and its value means nothing to the nucleus. Ring 0 learns
nothing about what the device is.

### 5b. What this proof does *not* cover, stated rather than assumed

- **Non-posted reads the function had already issued.** Their data goes to the
  device, so they cannot corrupt a freed frame — but they can *read* one, which
  is a disclosure to the device rather than a corruption of the pool. The
  quarantine covers the window: frames are out of the pool for the whole of it,
  and the flush above orders the completions of those reads behind the same
  boundary.
- **A function that has stopped answering configuration cycles.** The proof is a
  read that completes; a function in D3cold, hot-removed, or behind a link that
  is down cannot produce one. This ADR does not enter device reset or power
  management (ADR-0082 §12), so the honest statement is the compatibility
  restriction below rather than a fallback that would be a reset by another
  name.
- **A platform whose root complex reorders across the boundary.** The ordering
  rule is a requirement on conforming hierarchies, and a platform that violates
  it is outside the profile.

### 5c. The compatibility restriction, fixed honestly

> **The DMA teardown proof of §5a is valid on a conforming PCI/PCIe hierarchy in
> which the function is still reachable by configuration access at the moment of
> teardown.** A profile that cannot meet both conditions does not get a weaker
> proof; it gets no DMA authority, and this contract says so rather than
> lowering what "safe to return" means.

That is a real narrowing and it is the right one: the alternative is either a
timeout — which is a guess wearing a number — or a function reset, which
ADR-0082 §12 puts out of scope and which would silence a live interrupt source
belonging to the same driver.

**On the reference machine the proof is honest but is not stressed.** QEMU's
`q35` model completes device writes synchronously, so the drain will pass there
whatever the fabric would have done on hardware. The evidence therefore proves
that the *mechanism is performed in the right order*, and does not claim to have
observed a real posted write in flight. Saying so is the difference between
evidence and decoration.

### 5d. The gap the review found, and the sibling case

**`BME` does not clear when another bus-mastering descendant lives.** An
interrupt source is bus-mastering too (ADR-0082 §5d), so a driver holding one
keeps the function a master, and a DMA region released beside it must not return
its frames.

So the quarantine's release condition is **the predicate, not the region**: an
assignment's quarantined frames are returned when that assignment has no live
bus-mastering descendant *and* the §5a flush has been performed for it. Until
then they stay out of the pool — and, by §5e, stay charged.

Three candidates were weighed and quarantine remains the proposal:

| | | |
|---|---|---|
| **A. Retire the frames** | never return them within a boot, as §5f retires a vector | A vector is one of sixteen and a queue's buffers are megabytes: a leak with a rationale, not a bounded trade |
| **B. Quarantine until the predicate clears and the flush completes** *(proposed)* | frames held out of the pool, charge outstanding | Cost proportional to the risk: unavailable exactly while a device could still reach them |
| **C. Reset the function** | clear mastering regardless | Out of scope, and it would silence a sibling interrupt source nobody asked to silence |

## 5e. D3a — quarantined memory stays charged

**The review's second point, and it is a correctness bug rather than a
refinement.** If destroying a `DmaRegion` refunded the funding lineage while the
frames were still quarantined, a holder could:

```text
allocate → release into quarantine → allocate again from the refunded budget
```

and accumulate physically occupied frames without bound, above the
`MemoryAuthority` that was supposed to be the bound. A budget that can be spent
twice is not a budget.

> **Invariant.** Destroying a source-visible `DmaRegion` removes its name and its
> mapping and moves its backing into quarantine. **Its allocation charge stays
> outstanding.** The funding lineage is refunded at the same moment the frames
> actually return to the physical pool, and never earlier.

**This is ADR-0075's rule and not a new one.** That ADR already fixes *physical
reclamation before accounting refund*, and `region.rs` already implements it as
two steps with a receipt between them: `take_reclaim` hands out a ticket, the
caller returns the backing, and only the ticket can then credit the authority —
once, to the right lineage, with the slot unreusable in between. **A quarantine
is that interval made longer**, not a second mechanism, and
`allocated + reserved + free == budget` holds throughout because a quarantined
region is still `allocated`.

The consequence is the honest one: a driver that releases a DMA region while
holding an interrupt source **does not get its budget back yet**. That is not a
penalty, it is the accounting telling the truth about memory the device can
still reach.

## 6. D4 — a device-visible address, its extent, and how a driver reaches a part of one

A driver has to put a number in a device register, and the number has to be one
the device will accept. This is the part of the decision that is genuinely new,
because **ADR-0081 §2 says the physical backing address remains unobservable**
and on this platform a device address *is* the physical address.

**Proposed: a device address is issued, is opaque in the contract, and is never
accepted back.**

- it is obtained from the `DmaRegion` itself, by an operation on that
  capability — so possession of the region is what issues it, and nothing else
  can;
- its type is a **scalar the contract describes only as "the number this device
  must be given to reach this region"**. No contract says it is physical and no
  contract says it is an IOVA;
- **no operation of any contract accepts one.** Not the allocation, not any
  mapping operation, not any future one. A device address is data a driver
  writes to its own device through its own window, and is never presentable
  where authority is required — the rule that has made a BAR data since
  ADR-0079 §10;
- an IOMMU backend later changes what the number *is* and changes no contract,
  which is why it must not be described as physical now.

### 6a. The extent, which the first draft left undefined

The review is right that "a device address" is meaningless without saying what
it is the address *of*.

> **A `DmaRegion` is one contiguous device-visible extent.** Its issued address
> names the first byte, and every byte of the region is reachable by the device
> at a fixed offset from it, for the whole life of the region.

That sentence is the public contract, and it is deliberately written in terms of
**device-visible** contiguity rather than physical contiguity, because that is
what stays true across both backends:

| backend | what makes the extent contiguous |
|---|---|
| **no-IOMMU** *(the reference profile)* | the backing must be a **physically contiguous** run. There is nothing between the address and the memory, so one base plus an offset must be one address |
| **IOMMU** *(later, unchanged contract)* | a contiguous **IOVA** range, which the mapping may back with any physical frames. The public sentence above is unchanged, and no driver written against it needs revisiting |

**An implementation obligation this creates, recorded because it is real.**
`tos-frames` can carve a physically contiguous run today, and its released-frame
list is deliberately never re-carved — so a returned DMA run cannot currently
satisfy a later contiguous allocation. Stage 4C-2's implementation must make
returned runs re-carvable, or DMA memory is consumed permanently from the
contiguous frontier and the quarantine's whole point is lost. That is allocator
work, not an architectural contradiction, and it is named here so it is not
discovered as a leak later.

### 6b. Addressing a part of one region

**The first draft said arithmetic on a device address is meaningless, and also
left a driver with no way to address part of an allocation. Both cannot stand.**
A Virtqueue is three structures at three offsets inside one allocation, so
Stage 4D needs this and it must be settled here rather than improvised there.

> **Required.** A driver must be able to obtain the device-visible address of a
> **bounded offset inside a `DmaRegion` it holds**, checked against that region's
> extent, without supplying, computing or presenting any address.

The resolution of the apparent contradiction is that the *nucleus* does the
arithmetic and the *caller* does not:

- the contract defines the region as one contiguous extent, so an offset within
  it is meaningful — that is what §6a buys;
- the offset is checked against the extent and fails closed, exactly as an MMIO
  offset is (ADR-0081 §12);
- the caller presents a **region capability and an offset**, never an address.
  The input side is unchanged: no physical or device address is accepted by any
  operation as authority or as anything else.

**The API form is deliberately not fixed here.** `base + checked offset` as one
operation, a distinct operation per subobject, or a record of offsets resolved
in one call are all admissible; what the contract fixes is the property above.
What is *not* admissible is a form that takes an address as an argument, or one
that requires the driver to add numbers itself and thereby makes address
arithmetic part of the public contract.

### 6c. And the honest half

On the no-IOMMU reference profile this does disclose a physical address to a
process holding both capabilities. ADR-0082 §5 already governs that and this ADR
does not soften it:

> **With no IOMMU, TOS cannot claim hardware-enforced confinement of a malicious
> bus-mastering device.** The capability model controls which sanctioned DMA
> objects and device-visible addresses software may **obtain**; it does not
> physically prevent a malicious driver from programming a bus-mastering device
> with some other address.

A process that can already program its device with any address it invents is not
made more dangerous by being told the one address it is entitled to. What would
be dishonest is a contract implying the opposite. ADR-0081 §2's "unobservable"
sentence is **narrowed rather than kept and quietly broken**: it remains exactly
true of `Region` and `MmioRegion`, and a `DmaRegion` is the one kind whose whole
purpose is to be reachable by something other than the CPU.

## 7. What this ADR does not decide

The MMIO↔DMA ordering contract (Stage 4C-3), device reset, VirtIO feature
negotiation, queues, block I/O, scatter-gather beyond one contiguous region,
IOMMU domain management, and any device-matching policy. A second DMA backend
under an IOMMU is anticipated by §6's wording and is not designed here.

## 8. Conformance evidence this ADR will require

Positive, from canonical text on the real device:

1. a module holding a function **and** a memory authority obtains a
   `DmaRegion<mut T>`, and the memory account moves by exactly the charge;
2. the assignment becomes a bus master when it does, and not before;
3. the backing is **one contiguous device-visible extent**, and the address of a
   bounded offset inside it is obtainable and is that base plus that offset;
4. teardown performs §5a in order: mastering stops, **then** the configuration
   read of that function is performed, **then** the frames return — and the
   record shows the three in that order rather than merely all three present;
5. the account is exactly where it started once the quarantine has drained.

Negative, and a successful allocation alone is not sufficient:

6. a process holding only a `MemoryAuthority` cannot obtain one;
7. a process holding only a `PciFunction` cannot obtain one;
8. a function capability without `dma` is refused, with `map`, `config_write`
   and `interrupt` all present;
9. **no operation anywhere accepts a device address** — checked structurally
   over the accepted schema, as "a BAR is data" is;
10. an offset outside the region's extent is refused and yields no address;
11. a released DMA region's frames are **not** in the pool while an interrupt
    source keeps the function mastering, and **are** once it goes;
12. **the churn case.** With a live bus-mastering sibling, repeated
    allocate/release must not let one budget be spent twice: after *n* cycles the
    authority's remainder reflects *n* outstanding charges, the (*n*+1)-th
    allocation is refused when the budget is exhausted, and the pool has lost
    exactly the quarantined frames. A refund-on-release implementation passes
    every other item on this list and fails this one, which is why it is here;
13. process death runs the same path, and leaves neither a charge nor a
    quarantined frame behind once the predicate clears;
14. ring 0 still contains no device vocabulary.

## 9. What revision 2 changed

Recorded separately, because a revised decision a reader has to diff is a
revised decision nobody can review.

| | Revision 1 | Revision 2 |
|---|---|---|
| **teardown proof** | `BME=0` was treated as sufficient | **Changed decision.** `BME=0` blocks new requests and proves nothing about posted writes already issued. §5a adds an explicit drain: after mastering stops, a **configuration read of that function** is performed, and by PCIe's rule that a completion may not pass a previously issued posted request, observing its value proves those writes reached their destination. §5b names what the proof does not cover; §5c fixes the compatibility restriction it costs |
| **quarantine accounting** | left unstated, and the natural reading was refund-on-release | **Changed decision.** §5e: the charge stays outstanding until the frames actually return, and the refund happens at that same moment. This is ADR-0075's existing two-step reclaim with a longer interval, not a new mechanism, so `allocated + reserved + free == budget` holds throughout |
| **quarantine release condition** | "when the last bus-mastering descendant goes" | Sharpened to **the predicate plus the flush**, per assignment — a live interrupt source keeps the frames quarantined and charged (§5d) |
| **region extent** | undefined | **New decision.** §6a: a `DmaRegion` is **one contiguous device-visible extent**. On the no-IOMMU backend that requires a physically contiguous run; under an IOMMU it is a contiguous IOVA range, and the public sentence is unchanged |
| **addressing part of a region** | "no arithmetic on it is meaningful", with no way to address a subobject | **Resolved contradiction.** §6b: the driver presents a **region capability and a bounded offset**, checked against the extent; the nucleus does the arithmetic. The API form is deliberately left open, and the input rule is unchanged — no address is accepted anywhere |
| **allocator obligation** | absent | **New.** §6a records that `tos-frames` cannot re-carve a released run today, so Stage 4C-2 must make returned runs re-carvable or the quarantine achieves nothing |
| **evidence** | 10 items | 14, including the **churn case** (§8.12), which a refund-on-release implementation fails and every other item passes, and the **ordering** of the teardown steps (§8.4) rather than their mere presence |

Unchanged, and not re-argued: two capabilities with a separate `dma` right; two
ancestries; ADR-0082 §5d inherited whole; quarantine preferred to permanent
retirement; a device address is not a capability and no operation accepts one;
the no-IOMMU confinement statement; and ordering left to Stage 4C-3.

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. ADR-0081 §2's
  physical-address sentence is **narrowed** by §6c and the narrowing is stated
  there rather than left to be inferred.
- **Trusted-base impact:** the nucleus gains a DMA region kind with two
  ancestries, a per-assignment frame quarantine with its drain proof, the
  issuing of one number per region and the checked resolution of an offset
  inside one. The drain proof is a configuration read the nucleus already knows
  how to perform.
- **Threat-model impact:** `docs/34` S5 already carries ADR-0082 §5's wording;
  this adds two exploitation paths — "frames returned to the pool while a
  posted write is still in flight" and "one budget spent twice through
  quarantine churn" — and the control that closes each.
- **Compatibility profile:** `PLATFORM_INTERFACE_V1` at version 3, one new
  object kind, two new rights, and new `SYSTEM_ABI_V1` operations. **And a
  narrowing**: §5c restricts DMA authority to a conforming hierarchy in which
  the function still answers configuration cycles at teardown.
- **Bounded-resource impact:** quarantined frames are unavailable **and remain
  charged** while a function masters the bus. Bounded by driver lifetime rather
  than by the boot, and visible in the account rather than silent.
- **Implementation obligation:** `tos-frames` must make a returned contiguous run
  re-carvable (§6a), or DMA memory is consumed permanently from the contiguous
  frontier and the quarantine achieves nothing.
- **New dependencies:** none.
