<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0084: Where DMA authority comes from, and what a device-visible address is allowed to be

- Status: **Proposed — not Project Architect-approved.** Written before the
  mechanism, which is the order ADR-0081 §0 recorded going wrong once and
  ADR-0082 restored. Nothing in it is implemented
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

## 5. D3 — the ordering that makes teardown safe, and the gap it leaves

**Proposed rule, in this order and stated as an invariant rather than as steps:**

```text
1  the region's last name goes
2  it stops being a bus-mastering descendant of the assignment
3  ADR-0082 §5d's predicate is re-evaluated
4  ... and only then may its frames return to the pool
```

Step 4 after step 3 is the whole of the safety argument on a no-IOMMU platform:
when the region being destroyed is the assignment's **last** bus-mastering
descendant, the predicate clears Bus Master Enable, and a function that is not a
bus master cannot write host memory whatever address it still holds. The frames
then go back to an allocator no device can reach.

### The gap, stated rather than discovered later

**It is not the last bus-mastering descendant in general.** An interrupt source
is bus-mastering too (ADR-0082 §5d), so a driver holding one keeps Bus Master
Enable set — and a DMA region released beside it would return its frames to the
pool while the device is still a master with the old address in a register.

That is a real hole and it needs a decision. Three candidates, and the
recommendation is the second:

| | | |
|---|---|---|
| **A. Retire the frames** | never return DMA frames to the pool within a boot, as §5f retires a vector | Provable, and consistent with an accepted precedent. But a vector is one of sixteen and a queue's buffers are megabytes: this makes every driver restart cost the machine its buffers permanently, which is not a bounded resource trade, it is a leak with a rationale |
| **B. Quarantine until the function stops mastering** *(recommended)* | the frames are held out of the pool until the assignment has **no live bus-mastering descendant**, and are returned as a batch when the predicate clears | Bounded by the driver's own lifetime rather than by the boot's; costs nothing while a driver runs, and returns everything when it stops. The quarantine is a nucleus-owned list, and process death runs the same path |
| **C. Reset the function** | clear BME regardless, or issue a Function Level Reset, when any DMA region dies | Changes the device's state for reasons another descendant did not ask for, and device reset is explicitly out of scope (ADR-0082 §12). It would also silence a live interrupt source belonging to the same driver |

**B is proposed** because it is the only one whose cost is proportional to the
risk: the memory is unavailable exactly while a device could still reach it, and
not one instant longer.

**The order this ADR does *not* decide** is the cross-domain visibility rule —
DMA writes visible before a notify, DMA completions visible after an interrupt.
ADR-0081 §11 deliberately left it, and it stays left: it is **Stage 4C-3**, and
deciding it inside an allocation decision would be deciding the portable memory
model by accident for the second time.

## 6. D4 — what a device-visible address is, and the rule it must not break

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
  must be given to reach this region"**. No contract says it is physical, no
  contract says it is an IOVA, and no arithmetic on it is meaningful;
- **no operation of any contract accepts one.** Not `dma_region_allocate`, not
  any mapping operation, not any future one. A device address is data a driver
  writes to its own device through its own window, and it is never presentable
  where authority is required — the same sentence that has governed a BAR value
  since ADR-0079 §10;
- an IOMMU backend later changes what the number *is* and changes no contract,
  which is precisely why it must not be described as physical now.

**And the honest half.** On the no-IOMMU reference profile this does disclose a
physical address to a process that holds both capabilities. ADR-0082 §5 already
governs that and this ADR does not soften it:

> **With no IOMMU, TOS cannot claim hardware-enforced confinement of a malicious
> bus-mastering device.** The capability model controls which sanctioned DMA
> objects and device-visible addresses software may **obtain**; it does not
> physically prevent a malicious driver from programming a bus-mastering device
> with some other address.

A process that can already program its device with any address it invents is not
made more dangerous by being told the one address it is entitled to. What would
be dishonest is a contract implying the opposite, and §6's wording is chosen so
that it cannot be read that way. ADR-0081 §2's "unobservable" sentence is
**narrowed rather than kept and quietly broken**: it remains exactly true of
`Region` and `MmioRegion`, and a `DmaRegion` is the one kind whose whole purpose
is to be reachable by something other than the CPU.

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
3. the region's frames return to the pool only after the function has stopped
   mastering, and the account is exactly where it started afterwards.

Negative, and a successful allocation alone is not sufficient:

4. a process holding only a `MemoryAuthority` cannot obtain one;
5. a process holding only a `PciFunction` cannot obtain one;
6. a function capability without `dma` is refused, with `map`, `config_write`
   and `interrupt` all present;
7. no operation anywhere accepts a device address — checked structurally over
   the accepted schema, as the "a BAR is data" property is;
8. a released DMA region's frames are **not** in the pool while an interrupt
   source keeps the function mastering, and **are** once it goes;
9. process death runs the same path, and leaves neither a charge nor a
   quarantined frame behind;
10. ring 0 still contains no device vocabulary.

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. ADR-0081 §2's
  physical-address sentence is **narrowed** by §6 and the narrowing is stated
  there rather than left to be inferred.
- **Trusted-base impact:** the nucleus gains a DMA region kind with two
  ancestries, a frame quarantine, and the issuing of one number per region.
- **Threat-model impact:** `docs/34` S5 already carries ADR-0082 §5's wording;
  this adds the exploitation path "frames returned to the pool while a device
  can still write them" and the control that closes it.
- **Compatibility profile:** `PLATFORM_INTERFACE_V1` at version 3, one new
  object kind, two new rights, and new `SYSTEM_ABI_V1` operations.
- **Bounded-resource impact:** quarantined frames are unavailable while a
  function masters the bus. Bounded by driver lifetime, not by the boot.
- **New dependencies:** none.
