<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0084: Where DMA authority comes from, and what a device-visible address is allowed to be

- Status: **Accepted (Project Architect-approved, 2026-09-08)**, at revision 4.
  Written before the mechanism, which is the order ADR-0081 §0 recorded going
  wrong once and ADR-0082 restored: at the moment of approval nothing in it was
  implemented, and Stage 4C-2 is the implementation of what is decided here.
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-08, on revision 4.
- Revision history, kept because a decision that was corrected twice is more
  useful with the corrections visible than without them:

  **Revision 2** answered three findings: `BME=0` is not a teardown proof;
  quarantined memory stays charged until the frames actually return (§5f); and a
  `DmaRegion` is one contiguous device-visible extent whose interior a driver can
  address (§6a, §6b). The second and third were accepted.

  **Revision 3** repaired the teardown proof itself: §5b states one reclaim
  condition discharging **both** obligations, §5c the profile it requires, §5d
  the fail-closed outcome. All of that was accepted.

  **Revision 4** is narrow. P5's argument was wrong — several Traffic Classes may
  share one VC, so the Virtual Channel configuration proves nothing about the TC a
  function labels its own requests with, and ECAM would not settle it either.
  §5c.1 replaces the derivation with a **profile requirement** and names the
  residual risk. And No Snoop is reclassified: it is a **coherency** condition,
  not an ordering relaxation, and is held off for the right reason. §9 lists what
  moved
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

**`dma` is a fifth right on the function**, separate from `config_read`,
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

### 5a. Two obligations, and why a flush alone discharges only one

Before the mechanism, the two things that have to be proved, stated apart
because a proof of one is not a proof of the other:

```text
(1)  no non-posted request the function issued is still outstanding
(2)  every posted write the function issued has reached its destination
```

**A configuration read discharges (2) and not (1).** The ordering rule is that a
**completion may not pass a previously issued posted request** travelling in the
same direction: a configuration read of the function produces a completion
travelling **upstream**, along the path the function's DMA writes travel, so
observing that read's value proves the earlier writes were accepted ahead of it.
That is the "a read flushes posted writes" rule every operating system stops DMA
with. It says nothing about the function's own outstanding reads, whose
completions travel the other way.

**PCIe provides an architected bit for (1), and it is exactly what it is for.**
The PCI Express Capability's Device Status register carries **Transactions
Pending**: set while the Function has issued Non-Posted Requests that have not
completed. It is the bit an FLR waits on, and this ADR uses it without entering
reset.

### 5b. The reclaim condition, as one condition

> **`DRAINED(assignment)`** — evaluated in this order, each step after the last:
>
> ```text
> 1  the assignment has no live bus-mastering descendant
>       ⇒ ADR-0082 §5d has cleared Bus Master Enable
>       ⇒ no further request of any kind can be issued
> 2  a configuration read of that function's PCI Express Capability
>    Device Status is performed, and returns Transactions Pending = 0
> ```

**Two obligations, one transaction, and that is not a coincidence.** The read in
step 2 is both the observation and the flush:

- its **value** discharges (1) — the function reports no outstanding non-posted
  requests;
- its **completion** discharges (2) — it could not pass posted writes issued
  before it, and step 1 guarantees none can be issued after it.

Step 1 before step 2 is load-bearing in both directions. Without `BME = 0`
first, a write could be posted behind the flush; and the read must reach a
function that still answers configuration cycles, which is why nothing in
teardown may power it down or remove it.

**The wait is bounded in reads, not in time.** If Transactions Pending is still
set, the condition is simply not met yet: it is re-evaluated at the next point
the predicate is evaluated. There is no timeout that converts "not proved" into
"proceed", because a timeout is a guess wearing a number.

**No device knowledge is involved.** The PCI Express Capability is capability id
`0x10` and its layout is in the PCI Express base specification; ring 0 reads two
architected registers of it and learns nothing about what the device is.

### 5c. What the proof requires of the function and the platform

"A conforming hierarchy" is not enough. The ordering rule the flush rests on
holds **within one Traffic Class**, and Relaxed Ordering and ID-Based Ordering
exist precisely to relax transaction ordering. A fourth condition is not about
ordering at all and is listed here because it is required for the same frames to
be safe. So these are properties of a **DMA-capable function on a DMA-capable
platform**, and the table says for each one whether it is checked, enforced or
required by profile — the three are not the same thing:

| | Property | Kind | How it holds |
|---|---|---|---|
| **P1** | the function implements the **PCI Express Capability** (`0x10`) in conventional configuration space | **checked** | Read at claim. Without it there is no Transactions Pending bit and no architected way to prove obligation (1). A conventional PCI function gets **no DMA authority** — refused, not approximated |
| **P2** | **Relaxed Ordering is disabled** — Device Control bit 4 | **enforced** | *An ordering condition.* Cleared by the nucleus at claim, and the bit becomes nucleus-owned under ADR-0082 §5's rule so CPL 3 cannot re-enable it. Bit-precise, by §5a's rule that neighbours sharing a register are not reserved with it |
| **P3** | **ID-Based Ordering is disabled** — Device Control 2 bits 8 and 9, where the capability version has that register | **enforced** | *An ordering condition*, and the same treatment |
| **P4** | **No Snoop is disabled** — Device Control bit 11 | **enforced** | **Not an ordering relaxation, and calling it one would be wrong.** It is a *coherency* condition: our DMA frames are mapped write-back (§4), and a non-snooping device write may leave the CPU reading stale data — which would make "the write arrived" true and useless. It is held off for that reason, and the drain proof does not depend on it |
| **P5** | **TC0-only requester traffic** | **required by profile** | See below. Neither checked nor enforceable by this contract |

**P2–P4 are a narrowing of operation 26** in exactly the shape ADR-0082 §5
established, and are added to the reserved set for the same reason those were:
the hardware happens to keep the platform's ordering and coherency guarantees
inside a range a `config_write` holder reaches, and a driver that could
re-enable Relaxed Ordering could invalidate the teardown proof of a region it no
longer holds.

### 5c.1 P5 — what can honestly be said about Traffic Class

**An earlier revision argued this from the Virtual Channel configuration, and
that argument was wrong.** PCIe permits **several Traffic Classes on one VC**, so
"no additional Virtual Channel is enabled" does not mean only TC0 exists, and a
TC→VC map does not say which Traffic Class a function *chooses* for its own DMA
requests. The map describes where traffic goes, not what the requester labels it.

**Nor would ECAM settle it**, and the earlier revision was wrong about that too.
Reaching extended configuration space would let the nucleus read the Virtual
Channel capability and the mapping; it would still not read the Traffic Class a
device puts in the TLPs it originates. There is no architected register that
reports it.

So the honest boundary for Stage 4 is a requirement rather than a derivation:

> **DMA authority on the no-IOMMU reference profile is granted only to a function
> for which the compatibility profile separately establishes TC0-only requester
> traffic.**

On the current QEMU reference machine that is a **qualified property of the
platform and device model**, established once about the profile and recorded
with it. It is deliberately not established by anything in ring 0: a nucleus that
inferred a Traffic Class from what a device is would be holding device semantics,
which is the boundary Stage 4B and Stage 4C exist to keep.

**A physical-hardware profile will need more**, and this ADR does not design it.
Either an independent way to qualify the property of a given function, or a
platform mechanism that genuinely restricts or filters the Traffic Classes a
requester may use. Which of those is right is a question for the profile that
needs it, and inventing one here would be deciding it without the hardware in
front of us.

**What this costs, stated plainly.** P5 is the one condition of the five that is
neither checked nor enforced, and the drain proof is only as good as it. A
function that originated DMA writes in a Traffic Class other than TC0 would have
those writes unordered against the flush, and the reclaim condition would pass
without covering them. That is the residual risk of this profile, and it is
recorded here rather than dissolved into an argument that does not hold.

### 5d. Fail-closed, and what happens to memory that cannot be proved safe

> **If `DRAINED` cannot be established, the frames do not return and the charge
> is not refunded.** There is no path from "not proved" to "proceed".

That covers every way it can fail, and each is the same outcome:

- Transactions Pending never clears;
- the configuration read does not complete — a function that has stopped
  answering returns all-ones, which is not an observation of a zero bit and is
  not treated as one;
- the function or platform does not satisfy P1–P5, in which case DMA authority
  was refused at claim and there are no frames to return.

**And the quarantine cannot be orphaned.** Quarantined frames attach to the
assignment, and — by ADR-0081 §14's existing rule, reused rather than
reinvented — **an assignment does not end while something reaches it**. So a
function whose drain has not been proved stays claimed: its BDF cannot be
claimed again, no second driver can be handed a device that was never shown to
be quiescent, and the frames keep an owner that can retry. Bus Master Enable is
already clear, so a pinned assignment is inert rather than dangerous.

The condition is re-evaluated whenever the assignment's descendants change, so a
device that becomes quiescent later does return its memory. If it never does,
the frames are lost for the boot and **the account says so** — outstanding
charge, reduced pool, an assignment that will not release. That is the correct
outcome rather than a regrettable one: memory the system cannot prove is
unreachable by a device is memory it must not hand to anybody.

### 5e. The sibling case, and what the quarantine is released by

**`BME` does not clear when another bus-mastering descendant lives.** An
interrupt source is bus-mastering too (ADR-0082 §5d), so a driver holding one
keeps the function a master, and a DMA region released beside it must not return
its frames.

So the quarantine's release condition is **`DRAINED(assignment)` of §5b, not the
region**: an assignment's quarantined frames return when that assignment has no
live bus-mastering descendant *and* the read of §5b has observed Transactions
Pending clear. Until then they stay out of the pool — and, by §5f, stay charged.

Three candidates were weighed and quarantine remains the proposal:

| | | |
|---|---|---|
| **A. Retire the frames** | never return them within a boot, as ADR-0082 §5f retires a vector | A vector is one of sixteen and a queue's buffers are megabytes: a leak with a rationale, not a bounded trade |
| **B. Quarantine until `DRAINED`** *(proposed)* | frames held out of the pool, charge outstanding | Cost proportional to the risk: unavailable exactly while a device could still reach them, and released the moment it provably cannot |
| **C. Reset the function** | clear mastering regardless | Out of scope, and it would silence a sibling interrupt source nobody asked to silence |

## 5f. D3a — quarantined memory stays charged

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
4. teardown establishes `DRAINED` in §5b's order: mastering stops, **then** the
   Device Status read observes Transactions Pending clear, **then** the frames
   return — and the record shows the three in that order rather than merely all
   three present;
5. the account is exactly where it started once the quarantine has drained.

Negative, and a successful allocation alone is not sufficient:

6. a process holding only a `MemoryAuthority` cannot obtain one;
7. a process holding only a `PciFunction` cannot obtain one;
8. a function capability without `dma` is refused, with `map`, `config_write`
   and `interrupt` all present;
9. **no operation anywhere accepts a device address** — checked structurally
   over the accepted schema, as "a BAR is data" is;
9a. a claim of a function with **no PCI Express Capability** yields no DMA
    authority — P1 refused rather than approximated, on a real function of the
    reference machine that lacks one;
9b. a `config_write` that would set Relaxed Ordering, No Snoop or an IDO enable
    is refused, and its **neighbours in the same register remain writable** —
    the narrowing shown to be a narrowing, as ADR-0082 §5's was;
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
    quarantined frame behind once `DRAINED` holds;
13a. **fail-closed, exercised rather than described.** With `DRAINED`
    unestablished, the frames are not in the pool, the charge is outstanding, the
    assignment does **not** end, and a second claim of that BDF is refused — so
    a quarantine can never lose the owner that would retry it;
14. ring 0 still contains no device vocabulary.

## 9. What revision 2 changed

Recorded separately, because a revised decision a reader has to diff is a
revised decision nobody can review.

| | Revision 1 | Revision 2 |
|---|---|---|
| **teardown proof** | `BME=0` was treated as sufficient | **Changed decision.** `BME=0` blocks new requests and proves nothing about posted writes already issued. §5a adds an explicit drain: after mastering stops, a **configuration read of that function** is performed, and by PCIe's rule that a completion may not pass a previously issued posted request, observing its value proves those writes reached their destination. §5b names what the proof does not cover; §5c fixes the compatibility restriction it costs |
| **quarantine accounting** | left unstated, and the natural reading was refund-on-release | **Changed decision.** §5f: the charge stays outstanding until the frames actually return, and the refund happens at that same moment. This is ADR-0075's existing two-step reclaim with a longer interval, not a new mechanism, so `allocated + reserved + free == budget` holds throughout |
| **quarantine release condition** | "when the last bus-mastering descendant goes" | Sharpened to **the predicate plus the flush**, per assignment — a live interrupt source keeps the frames quarantined and charged (§5d) |
| **region extent** | undefined | **New decision.** §6a: a `DmaRegion` is **one contiguous device-visible extent**. On the no-IOMMU backend that requires a physically contiguous run; under an IOMMU it is a contiguous IOVA range, and the public sentence is unchanged |
| **addressing part of a region** | "no arithmetic on it is meaningful", with no way to address a subobject | **Resolved contradiction.** §6b: the driver presents a **region capability and a bounded offset**, checked against the extent; the nucleus does the arithmetic. The API form is deliberately left open, and the input rule is unchanged — no address is accepted anywhere |
| **allocator obligation** | absent | **New.** §6a records that `tos-frames` cannot re-carve a released run today, so Stage 4C-2 must make returned runs re-carvable or the quarantine achieves nothing |
| **evidence** | 10 items | 14, including the **churn case** (§8.12), which a refund-on-release implementation fails and every other item passes, and the **ordering** of the teardown steps (§8.4) rather than their mere presence |

### Revision 3

| | Revision 2 | Revision 3 |
|---|---|---|
| **what the flush proves** | a configuration read was treated as the whole drain | **Corrected.** It discharges only "posted writes have arrived", and only **within TC0**. It says nothing about the function's own outstanding non-posted requests, whose completions travel the other way |
| **outstanding non-posted requests** | not addressed | **New.** PCIe's architected **Transactions Pending** bit, in the PCI Express Capability's Device Status, is exactly the proof of their absence — the bit an FLR waits on, used here without entering reset |
| **the reclaim condition** | two informal steps | **One condition, `DRAINED`** (§5b): `BME = 0` first, then a read of Device Status returning Transactions Pending clear. One transaction discharges both obligations — its *value* proves the first, its *completion* proves the second — and that is stated rather than left to be noticed |
| **ordering profile** | "a conforming hierarchy", which the review correctly called insufficient | **Five properties of a DMA-capable function/platform** (§5c): the PCI Express Capability **checked at claim**; Relaxed Ordering, No Snoop and IDO **cleared and made nucleus-owned**, bit-precisely, so a `config_write` holder cannot invalidate the proof; and TC0-only traffic separated out — revision 4 corrects how that last one is justified |
| **operation 26** | unchanged | Narrowed again, in ADR-0082 §5's shape: four ordering bits added to the set it refuses, with their register-neighbours left writable |
| **fail-closed** | implied | **Explicit** (§5d): no path from "not proved" to "proceed"; an all-ones read is not an observation of a zero bit; there is no timeout, only re-evaluation. Frames stay out, the charge stays outstanding, and — reusing ADR-0081 §14 rather than inventing anything — **the assignment does not end while quarantined frames attach to it**, so the BDF cannot be re-claimed and a quarantine can never lose the owner that would retry it |
| **evidence** | 14 items | 17: a function with no PCI Express Capability gets no DMA authority, the four ordering bits are refused while their neighbours stay writable, and the fail-closed state is **exercised** — frames out, charge outstanding, assignment pinned, second claim refused |

### Revision 4

| | Revision 3 | Revision 4 |
|---|---|---|
| **P5's argument** | derived from Virtual Channel configuration: "no additional VC is enabled, so TC0→VC0 is the only mapping, so a function emitting another TC is emitting one nothing mapped" | **Withdrawn as wrong.** PCIe permits several Traffic Classes on one VC, so the VC configuration does not say which TC a function labels its own DMA requests with. A map describes where traffic goes, not how a requester labels it |
| **P5's future** | "ECAM makes this checkable, and it should be checked then" | **Withdrawn as wrong.** ECAM would read the VC capability and the mapping; no architected register reports the Traffic Class a device originates. A physical-hardware profile needs an independent way to qualify the function, or a platform mechanism that genuinely restricts requester Traffic Classes — and this ADR deliberately designs neither |
| **P5's status** | "declared" | **A profile requirement** (§5c.1): DMA authority is granted only to a function for which the compatibility profile *separately establishes* TC0-only requester traffic. On the QEMU reference machine that is a qualified property of the platform and device model, established about the profile and **not** inferred in ring 0 — a nucleus deriving a Traffic Class from what a device is would be holding device semantics |
| **residual risk** | implicit in "declared" | **Named** (§5c.1): P5 is the one condition of the five neither checked nor enforced, and the drain proof is only as good as it. Writes originated in another TC would be unordered against the flush and the reclaim condition would pass without covering them |
| **No Snoop** | listed among the ordering conditions | **Reclassified.** It is a **coherency** condition, not an ordering relaxation: the frames are mapped write-back (§4), and a non-snooping write may leave the CPU reading stale data — which makes "the write arrived" true and useless. Still held off, now for the stated reason, and the drain proof does not depend on it |
| **the table** | "checked or declared" | Three kinds, because they are three different things: **checked** (P1), **enforced** (P2–P4), **required by profile** (P5) |

**Audit for revision 3**, because the profile is only honest if the reference
machine meets it: the Stage 4 function is a PCI Express endpoint —
`x-disable-pcie = false` on `virtio-blk-pci` at `00:04.0` under `q35`, so QEMU
initialises the PCI Express Capability on it and Device Status carries
Transactions Pending. **P1 is satisfiable on the reference machine**, which is
what makes §5c a restriction rather than a refusal of the platform this project
runs on.

**This remains a generic PCI/PCIe mechanism.** Capability id `0x10`, two
architected registers, one configuration read. No device-specific cooperation, no
reset, no VirtIO. **Not an architecture STOP.**

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
  inside one. It also gains the PCI Express Capability to its capability walk,
  and four ordering bits to the set it owns. The drain proof itself is **one
  configuration read** the nucleus already knows how to perform.
- **Threat-model impact:** `docs/34` S5 already carries ADR-0082 §5's wording;
  this adds two exploitation paths — "frames returned to the pool while a
  posted write is still in flight" and "one budget spent twice through
  quarantine churn" — and the control that closes each.
- **Compatibility profile:** `PLATFORM_INTERFACE_V1` at version 3, one new
  object kind, two new rights, and new `SYSTEM_ABI_V1` operations. **And two
  narrowings**: §5c makes DMA authority conditional on a PCI Express function
  under a TC0-only, Relaxed-Ordering-free, IDO-free ordering profile, and adds
  the bits that enforce it to the set operation 26 refuses.
- **Bounded-resource impact:** quarantined frames are unavailable **and remain
  charged** while a function masters the bus. Bounded by driver lifetime rather
  than by the boot, and visible in the account rather than silent.
- **Implementation obligation:** `tos-frames` must make a returned contiguous run
  re-carvable (§6a), or DMA memory is consumed permanently from the contiguous
  frontier and the quarantine achieves nothing.
- **New dependencies:** none.
