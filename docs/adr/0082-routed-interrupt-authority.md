<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0082: Where routed interrupt authority comes from, and how a textual driver waits for its device

- Status: **Accepted (Project Architect-approved, 2026-09-05)**
- Date: 2026-09-05
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-05, on the revised
  document — that is, this one, including the mandatory amendments of §5a–§5f
  which the ruling required before approval. **The decision was written before
  the mechanism and approved before the mechanism was built**, which is what
  ADR-0081 §0 recorded going wrong the previous time and is stated here because
  an order that is right is only visible if it is written down
- **What the approval covers**: §3–§12, including the ownership repairs in
  §5a–§5f. It does **not** cover DMA authority, device-visible addressing, the
  IOMMU, the MMIO↔DMA ordering contract, device reset, VirtIO feature
  negotiation, queues, or block I/O — see §12
- Decision level: **3** — it admits a second class of authority descending from
  a device assignment, moves interrupt-controller programming into the nucleus
  as a capability-gated mechanism, adds a blocking operation whose wake source
  is not a context, and **narrows already-accepted operations** because
  their current domains reach the hardware that decides where an interrupt goes
- Related: ADR-0049 (the interrupt baseline and what it withheld), ADR-0059
  (blocking and the liveness rule, and its §"Realisation"), ADR-0079 (hardware
  authority origin, §2.1's five conditions, §10's `PciFunction`), ADR-0081 §14
  (an assignment's descendants), ADR-0063 (an operation requiring two
  capabilities). `SYSTEM_ABI_V1` §2.1, §5, §6, `PLATFORM_INTERFACE_V1`,
  `CAPABILITY_V1` §2–§4, `docs/11` §Interrupts, `docs/34` S5/T5, `docs/35`
  §Stage 4

## 1. The smallest unreachable operation

> A canonical textual TOS Core process, holding authority over exactly one PCI
> function and nothing else, blocks; the real device that function names raises
> a real interrupt; the process resumes because of it; and the record shows the
> delivery arrived through the authority it holds rather than through anything
> the host supplied.

Nothing smaller is a routed interrupt at all. Everything Stage 4D needs after it
— a Virtqueue, a notification, a completion — is built on this one act.

## 2. What exists, and what it does not reach

`apic.rs` routes exactly two vectors: the timer (32) and spurious (255). The
8259 pair is masked entirely rather than programmed. There is no IOAPIC code, no
MSI code and no MSI-X code, and ADR-0049 §6 deliberately exposed the tick "only
as far as a scheduler and a bounded IPC timeout need". The IDT has 256 slots and
34 are claimed.

So there is no interrupt a device can raise that reaches a process, and there is
no authority that would say which process it should reach.

`docs/11` §Interrupts states the intended shape and is not an interface: "the
nucleus acknowledges and routes low-level interrupts to driver event endpoints".
`PLATFORM_INTERFACE_V1` §5 declares no interrupt interface and says why —
an interface arrives "when its mechanism is decided, not when a document first
shows its name". This ADR is that decision.

## 3. D1 — interrupt authority descends from the device assignment

**Accepted shape: a capability derived from a live `platform.pci.FunctionConfig`,
and from nothing else.**

```text
platform.pci.Bus                     platform root, minted at the launch boundary
    ↓  pci_function_claim
platform.pci.FunctionConfig          one exclusive assignment (ADR-0079 §10)
    ↓  pci_interrupt_claim, right `interrupt`
platform.irq.Source                  one interrupt of that function
    ↓  irq_wait, right `wait`
a blocked context, woken by the device
```

This is ADR-0079's authority model applied unchanged to a second mechanism, and
it is deliberately **not** a second model. In particular there is no
interrupt-controller capability, no "interrupt manager may route anything"
authority, and no module-name rule anywhere in ring 0. A driver that holds a
function holds the ability to ask for that function's interrupts; a process that
holds no function cannot ask for anybody's.

### A number is not authority, and here is the list

None of these is sufficient, and none of them is even expressible as an argument
to any operation this ADR adds:

- a CPU interrupt vector;
- a GSI;
- a legacy IRQ number;
- an MSI address/data pair;
- a PCI BDF.

The one number a caller does supply is an **MSI-X table entry index within the
function its capability already names**, which is the same class of argument as
the BAR index of ADR-0081 §13: it selects among things the capability covers and
cannot reach outside them. A fabricated index is `E_BAD_ARGUMENT`, and a
fabricated one that happens to be in range still names an entry of the caller's
own device.

## 4. D2 — the transport is MSI-X, and the other two are refused with reasons

The Stage 4 reference machine was measured rather than assumed. `q35`,
`virtio-blk-pci` at `00:04.0`, modern transport:

```text
vectors = 2                     MSI-X, two entries
BAR 1  = 4 KiB                  the MSI-X table and its pending-bit array
BAR 4  = 16 KiB                 the modern VirtIO structures
IRQ 0, pin A                    INTx is also present
```

**MSI-X is chosen.** It is edge-delivered and unshared, so acknowledgement is a
local-APIC EOI and nothing else — the nucleus needs to know no device register
to end an interrupt. It scales to Stage 4D's per-queue vectors without a new
mechanism. And its address/data pair never leaves ring 0.

**INTx is refused, and not because it is old.** It is level-triggered and
shared, so ending one requires the *device* to deassert the line, which for this
device means reading VirtIO's ISR status register. That is either VirtIO
knowledge in the nucleus — the boundary this stage exists to keep — or a line
that stays asserted until a CPL-3 driver gets round to it, which is an interrupt
storm a driver can cause by being slow. Deriving its GSI additionally needs the
q35 PIRQ routing or an ACPI `_PRT`, which is chipset policy in ring 0 with no
capability behind it.

**MSI is refused because this device does not offer it.** QEMU's modern
`virtio-pci` exposes the MSI-X capability and not the MSI one. MSI would
otherwise have been attractive — its address and data live in configuration
space, which the nucleus already reaches — and if a later device offers only
MSI, adding it is a second backend under the same capability model, exactly as
ECAM will be a second backend under ADR-0079 §7's.

**What this costs the nucleus.** MSI-X's table lives in a BAR, so ring 0 must be
able to reach a page of device memory that no process has mapped. The mechanism
already exists in the shape needed: `paging::fill` maps the local APIC into
every address space this nucleus builds, for exactly the same reason — a handler
must reach a device register whatever is running. An MSI-X table page is mapped
the same way, supervisor-only, uncacheable, bounded by `MAX_ASSIGNMENTS`.

## 5. D3 — three ambient paths to interrupt authority, closed

This is the part of the decision that touches already-accepted operations, and
it is stated first because it is the part a reviewer must weigh hardest.

MSI-X is programmed through three places a CPL-3 holder can currently reach:

| Place | Reached today by | What it would grant |
|---|---|---|
| the MSI-X table, in BAR 1 | `pci_bar_map_write` (op 27, right `map`) | an arbitrary message address and data — that is, an arbitrary interrupt vector delivered to the nucleus |
| the MSI-X capability's Message Control | `pci_config_write` (op 26, right `config_write`) | enable, disable or mask MSI-X behind the nucleus's back |
| the Command register's Bus Master Enable | `pci_config_write` | make the device able to write host memory at all |

**None of these is a defect anybody introduced.** ADR-0081 §13 lets a holder map
a page-aligned window of a BAR it names, and nothing said one BAR was different;
ADR-0079 §10 gave `config_write` the first 256 bytes. The hardware fact that the
thing which decides where an interrupt goes lives *inside* those ranges is what
makes them ambient paths, and it only becomes load-bearing at the moment
interrupt authority becomes a capability. That moment is this ADR.

So this decision **narrows** three domains, each a new refusal and none a
widening:

1. `pci_bar_map_read` and `pci_bar_map_write` refuse any window overlapping the
   function's MSI-X table or pending-bit array. The nucleus knows where those
   are from the MSI-X capability, which is generic PCI mechanism and not device
   knowledge. `E_NO_CAPABILITY`, by the same rule that refuses an I/O BAR.
2. `pci_config_write` refuses any access touching the function's MSI-X
   capability structure. Reads are unaffected: where a table lives is a fact
   about hardware, and ADR-0079 already says a fact is not authority.
3. `pci_config_write` refuses any access that would **change** the Command
   register's Bus Master Enable bit. That bit becomes **nucleus-owned**, under
   the predicate and the lifecycle of §5d.

   **One bit of one byte.** BME is bit 2 of the byte at `0x04`, and the refusal
   is judged on that bit alone: an access either contains that byte or does not,
   and a one-byte write at `0x05` — whose own bit 2 is INTx Disable — is
   ordinary business. The rest of the Command register is a driver's, save what
   §5b reserves.

Refusing rather than filtering is deliberate throughout: a write that silently
kept some bits and dropped others would leave a driver unable to tell what the
device now holds.

**Three is what the review found, and not what the hardware has.** §5a–§5f
extend this list, because a resource that can be relocated moves the MSI-X table
out from under refusal 1, and a bit nobody may write still has to hold the right
value. The reviewed and accepted set is those sections together with these
three; this section is the part that was visible first, not the part that is
sufficient.

### The consequence that must not be buried

**A bus-mastering device on a platform with no IOMMU can write anywhere in
physical memory.** An MSI-X message *is* a memory write, so bus mastering is not
optional for a routed interrupt — claiming one necessarily enables it.

`docs/34` S5 already governs this and is satisfied by saying it plainly rather
than by pretending otherwise: "IOMMU absence or limitations are reported as a
weaker security profile, not hidden", and T5 states that early TOS "does not
claim full protection" against DMA-capable hardware outside isolation. The
wording below is the accepted one and is to be carried unchanged into `docs/34`
and into the Stage 4C-2 DMA decision:

> **With no IOMMU, TOS cannot claim hardware-enforced confinement of a malicious
> bus-mastering device.** The capability model controls which sanctioned DMA
> objects and device-visible addresses software may **obtain**; it does not
> physically prevent a malicious driver from programming a bus-mastering device
> with some other address.

Two consequences, which must be stated together and never separately:

- **sanctioned DMA authority and device-visible address issuance** require
  *both* memory funding authority and the live device assignment, and that is
  mechanically enforced;
- **hardware DMA confinement** is not provided by the no-IOMMU reference
  profile, and no sentence of any contract may imply that it is.

Recorded so it cannot be written later by accident: **it is false, on this
profile, to say that possession of only a `PciFunction` makes arbitrary RAM
physically invisible to the device.** Once bus mastering is enabled — which a
routed interrupt requires — the device can address any physical memory a driver
programs into it. What the capability model bounds is which addresses a driver
can obtain *legitimately*, not which addresses the hardware will accept.

An IOMMU backend later strengthens confinement **without changing the public DMA
object model**, which is why that model must not be written in terms of
identity-mapped physical addresses. Closing the gap is not in this ADR's scope
and is not claimed by it.

## 5a. D3a — resource placement is static for an assignment

**Decided: a claimed function's resource placement is platform/nucleus-owned for
the whole assignment lifetime, and dynamic BAR relocation is rejected.**

This is not a precaution. It was measured on the reference function, and the
audit is `docs/evidence/STAGE4C1_REVIEW_FINDINGS.md` §2:

```text
BAR1  accepted a CPL-3 write   ← and BAR1 is the MSI-X table's own BAR
BAR4  accepted a CPL-3 write   ← the modern structures, low half
BAR5  accepted a CPL-3 write   ← the high half of the same 64-bit resource
and a window was still derived from BAR4 after BAR4 had been rewritten
```

ADR-0081 §13 measures each BAR **once, at claim time**, and every later mapping
derives its physical base from that measurement. §4 of this document adds a
nucleus mapping of the MSI-X table derived the same way, and §5's first refusal
computes the table's extent from a cached BIR. All three rest on the cached
layout still being the layout the live function decodes, and nothing kept that
true. Relocation is therefore **incompatible with the assignment model already
accepted by ADR-0081**, and the resolution is to make the model's assumption
hold rather than to teach three mechanisms to chase a moving resource.

`pci_config_write` refuses a write that would **change** a protected
resource-placement or routing field. Reads remain allowed. A write that puts
back the value already there remains allowed, exactly as for Bus Master Enable.
`E_NO_CAPABILITY`, because the argument is well formed and what the caller lacks
is the authority.

**The rule is expressed from the function's reported header type**, because a
byte-offset rule would be wrong in both directions on a bridge — refusing
nothing that matters and permitting the registers that re-route whole buses.

| Header | Protected |
|---|---|
| **Type 0** | BAR0–BAR5; Expansion ROM BAR; Command Memory Space Enable, under §5b |
| **Type 1** | BAR0–BAR1; primary, secondary and subordinate bus numbers; I/O base/limit including the upper fields; memory base/limit; prefetchable memory base/limit including the upper fields; Expansion ROM BAR; Command Memory Space Enable; Command I/O Space Enable; and the Bridge Control fields that alter downstream address routing or downstream device state — ISA Enable, VGA Enable, VGA 16-bit Decode, Secondary Bus Reset |

**A 64-bit BAR pair is one resource placement, and both halves are protected.**
The audit's third line is why this is stated rather than implied: a driver that
could move only the high half would move the whole address while the low half
looked untouched.

**Status and error-reporting bits that merely share a register are not
reserved.** Bridge Control also carries parity-error response and SERR
forwarding, and those are a driver's ordinary business; reserving them because
of their neighbours would be the wide narrowing this decision exists to avoid.

## 5b. D3b — Memory Space Enable has a lifecycle of its own

Refusing CPL-3 writes to Command bit 1 is necessary and **not sufficient**: a
bit nobody may write still has to be *right*, and firmware decided its value
before TOS ran.

> **memory-decoding descendant** — an assignment descendant whose mechanism
> requires the function's PCI memory-space decoding to be enabled.

| Descendant | Memory-decoding? | Why |
|---|---|---|
| `MmioRegion` / `MmioRegionMut` | **yes** | the window is worthless if the function does not decode it |
| MSI-X `platform.irq.Source` | **yes** | its table is in a memory BAR |
| a DMA mapping alone | **no** | the device initiates those; decoding is governed independently by BME |

> **Memory Space Enable is set if and only if the assignment has at least one
> live memory-decoding descendant.**

**This is a second predicate and not a spelling of the first.** MSE and BME are
independent: an `MmioRegion` needs MSE and not BME; a DMA mapping needs BME and
not MSE; an MSI-X source happens to need both, which is exactly why keeping them
separate matters — a single predicate would be right about the source and wrong
about the other two.

The lifecycle is §5d's, applied to this predicate: a claim normalises MSE clear
before the first capability reaches CPL 3, the pre-claim firmware state is not
restored, the first such descendant sets it before that descendant becomes
usable, later ones do not change it, destruction of one leaves it set while
another remains, the last one clears it, and process death, failed creation and
re-claim all begin or end at the same defined state.

**One permitted exception, bounded and unobservable.** Sanitising the MSI-X
table at claim time is a memory access to a BAR, so the mechanism may enable
memory decoding inside a claim or source-initialisation critical section during
which no CPL-3 instruction runs. It must finish in the state the invariant
dictates. A window a process can observe is not such a section.

## 5c. D3c — conventional MSI is nucleus-owned interrupt routing too

§3's rule is about **authority**, not about a transport: an interrupt is reached
through the live assignment and through nothing else. A device that offered
conventional MSI would therefore offer an ambient route around
`platform.irq.Source` — the same escalation §5 closes for MSI-X, through a
different capability.

So the claim path audits the conventional capability list for **both** MSI
(id `0x05`) and MSI-X (id `0x11`).

For **MSI**:

- its extent is derived from its own Message Control — whether it carries a
  64-bit address, and whether it carries per-vector masking — because those two
  bits are what decide whether the structure is 10, 14 or 24 bytes. **A blanket
  maximum range is refused**: reserving bytes that belong to the *next*
  capability would refuse writes to a structure this decision says nothing
  about;
- `pci_config_write` refuses writes touching its routing, control, address, data
  and mask state; reads remain allowed;
- the claim normalises MSI Enable to **disabled** before CPL 3 can receive the
  assignment.

For **MSI-X**: the existing reservation is retained, and the claim normalises
MSI-X Enable **off** and Function Mask **on** before CPL 3 can receive the
assignment.

**Stage 4's positive backend remains MSI-X only.** No MSI delivery path is
built, and none is implied: what §5c decides is a *refusal*, and a refusal needs
no backend. Coverage for it is structural, over a controlled capability rather
than over a device the reference profile does not have — inventing an MSI
positive backend merely to exercise a refusal would be building a mechanism to
test the absence of a mechanism.

## 5d. D3d — bus mastering, with the lifecycle it needs

> **bus-mastering descendant** — an assignment descendant whose mechanism
> requires the function to issue its own memory transactions.

| Descendant | Bus-mastering? |
|---|---|
| `MmioRegion` / `MmioRegionMut` | no — the CPU initiates; the device does not |
| MSI / MSI-X `platform.irq.Source` | **yes** — the message *is* a memory write the device issues |
| a future DMA mapping | **yes** |
| a future INTx source | no — a line, not a transaction |

> **Bus Master Enable is set if and only if at least one live bus-mastering
> descendant exists.**

Both halves are load-bearing. *Only if* is what makes a claim clear the bit
**the firmware left set** — measured, not hypothetical: OVMF hands TOS a
`virtio-blk-pci` function that is already bus-mastering. *If* is what makes an
interrupt source work without a driver ever naming the bit.

| Moment | BME |
|---|---|
| immediately after `pci_function_claim` | **cleared** — whatever firmware left is discarded |
| before the first capability reaches CPL 3 | cleared; the clear is inside the claim |
| first bus-mastering descendant created | set, before that descendant becomes nameable |
| second and later such descendants | unchanged — a predicate over a set, not a counter |
| one of several destroyed | unchanged while another remains |
| the last one destroyed | **cleared**, including when it goes because its process died |
| an `MmioRegion` created or destroyed | unchanged |
| process death | the predicate is re-evaluated once after the sweep, not per release |
| assignment teardown | cleared explicitly, because the device outlives the assignment |
| failed claim or failed grant rollback | cleared |
| re-claim of the same BDF | cleared again, from the same defined state |

**The pre-claim state is deliberately not restored.** Restoring "set" would hand
the next claimant a bus-mastering function for a reason that is a fact about a
previous environment — the situation the measurement above shows is real — and
restoring "clear" where firmware left it clear is indistinguishable from not
restoring. One post-condition, and it is the safe one.

## 5e. D3e — an MSI-X table write is complete when the table says so

A configuration-space read is **not** a synonym for the completion of an MMIO
table write, and using one as a fence would be a claim about the interconnect
that nothing here proves. Completion is proved by **reading back the same MSI-X
table entry** — its Vector Control field — after the posted writes to it.

Creation, in order:

```text
1  the entry is masked
2  message address and data are programmed while it is masked
3  the same entry is read back, which proves the table writes have landed
4  BME satisfies §5d's invariant
5  MSI-X Enable and Function Mask reach their intended state
6  the entry is unmasked — only now can this source fire
```

with three properties preserved from §7 and §4.2: the **IDT gate is installed
before the message is programmed**, so a message that somehow arrives lands
somewhere defined; the **source object is published before the entry can fire**,
so a delivered message never finds a handler with no source to wake; and the
**capability reaches CPL 3 only after creation is complete**.

## 5f. D3f — a device vector is retired, never recycled, within a boot

The earlier proposal ended teardown with a "drain" step, and **no accepted
mechanism proves that a message emitted before masking cannot arrive after the
vector has been reused.** A stale MSI carries a vector; it does not carry the
source's generation. Every other stale-authority property in this system is
proved by a generation, and this one cannot be.

So Stage 4C takes the conservative rule rather than an unproved one:

> Once a CPU vector has been allocated to a routed device source, it is
> **retired** for the remainder of that boot and is never returned to the
> allocator.

Destruction, in order:

```text
1  mask the table entry
2  read back the same entry, so the mask has landed at the function
3  disable MSI-X or apply Function Mask, if this was the last source
4  apply the §5d and §5b transitions
5  cancel any waiter with E_CANCELLED
6  unpublish the source
7  keep an IDT handler for the retired vector
8  a late interrupt on it is acknowledged, counted as spurious, and wakes nobody
9  the allocator never hands that vector to another source this boot
```

**A bounded number of vectors traded for a proof.** Exhaustion is `E_LIMIT` and
is neither hidden nor worked around by recycling. A later stage may introduce
reuse only with a separately proved hardware-quiescence mechanism.

This costs the steady-state request path nothing: vectors are allocated when a
source is created and never on a delivery.

## 6. D4 — what a `platform.irq.Source` is

Nucleus-owned state, unreachable except through a capability:

```text
the assignment it descends from, and that assignment's generation
the MSI-X table entry it occupies
the vector the nucleus allocated for it
the process waiting on it, if any
one pending bit
its own generation
```

| | |
|---|---|
| scope | exactly one interrupt of exactly one function |
| rights | `wait`. No mask right and no acknowledge right — see §7 |
| exclusivity | at most one live source per (assignment, entry); a second claim while the first lives is `E_LIMIT` |
| affinity | not affine. `capability_attenuate` makes another **name**, as it does for a function; the exclusivity is a property of the claim |
| lifetime | a **descendant of the assignment**, by ADR-0081 §14's existing rule |
| staleness | by its own generation, and by the assignment's |

**No vector, no message address and no message data appears in the object's
public description or in any operation's arguments or results.** They are
nucleus state in the same sense as a region's physical base.

### Lifetime is ADR-0081's, reused rather than reinvented

An assignment stays live while *either* a `FunctionConfig` capability names it
**or** any derived hardware object exists. ADR-0081 §14 wrote that rule
generically and said in as many words that "IRQ and DMA objects will need the
same invariant". They do, and this is the first of them.

So the stale-authority property is the one already proved for MMIO windows, with
nothing new to trust:

```text
release the function → re-claim the same BDF → the old IRQ reaches the new assignment
```

is impossible, because the assignment does not end while a source descends from
it, and the generation advances when it does.

## 7. D5 — delivery, exactly specified

Everything here is stated because leaving any of it to the implementation is how
a completion gets lost.

**One waiter.** At most one context may be inside `irq_wait` on a source. A
second concurrent wait is `E_LIMIT`. Several capabilities may name the source;
only one wait is outstanding.

**Edge, with a one-bit pending latch.** The nucleus keeps one `pending` bit per
source:

| Event | State | Result |
|---|---|---|
| interrupt arrives, a context is waiting | — | the context is woken, `pending` stays clear |
| interrupt arrives, nobody is waiting | — | `pending` is set |
| `irq_wait`, `pending` set | — | `pending` is cleared, the call returns `OK` **without blocking** |
| `irq_wait`, `pending` clear | — | the call blocks |
| second interrupt while `pending` is set | — | coalesced; `pending` was already set |

**The only completion event cannot be lost by racing the wait call**, which is
the property this shape exists to have: an interrupt that arrives before the
driver enters `irq_wait` sets the latch, and the next wait returns immediately.

**A bit and not a count, deliberately.** A count would invite a driver to pair
wakeups with completions, which is false of any device that coalesces — and
coalescing is not merely permitted here but *required* by `docs/35` §Stage 4:
"one interrupt wakeup may complete a batch of requests". A bit says "something
happened since you last looked", which is the only statement that stays true.
The driver's obligation is the one every real queue driver already has: after a
wake, drain until empty.

**Acknowledgement is nucleus-level and there is no operation for it.** An MSI-X
interrupt is ended by writing the local APIC's EOI register, which the handler
does before it returns. There is no transport-level acknowledgement, because
that is an INTx property and INTx was refused in §4. An operation that did
nothing would be a contract describing a system that does not exist.

**No mask or unmask operation in this version**, for the same reason. Edge
delivery to a latch needs neither for correctness. The nucleus masks a table
entry when its source is released, which is mechanism rather than an exposed
operation.

**A spurious interrupt is acknowledged and counted, and wakes nobody.** An
interrupt on a vector whose source is not live is recorded and dropped: waking
"whoever used to hold this" would be delivering an event to authority that has
ended. **This is also the whole of what a late message does to a retired
vector** (§5f): the vector keeps its handler for the rest of the boot, and the
handler's only possible answer is this one.

**Process death.** The waiter stops being blocked with everything else the
process held; the sources it named are released by the ordinary capability
sweep; the last name going releases the entry and the descendant, and **retires
the vector** — it does not return to the allocator. A dead process cannot leave
a routed interrupt pointing at a slot.

**Assignment teardown, and revocation.** When a source is destroyed while a
context waits on it, that wait is woken with `E_CANCELLED` **at that instant**.
This is `SYSTEM_ABI_V1` §6's cancellation path, and it is also what stops a
stranded wait from keeping the system falsely live — see §8.

## 8. D6 — what this does to the liveness rule

ADR-0059 §"Realisation" classified every blocking reason into a wake source and
found all of them **peer**. This ADR adds the first that is not:

| Reason | What could end it | Wake source |
|---|---|---|
| `Interrupt(source, generation)` | the device, with nothing running | **routed**, while the source is live |

The classification is per-wait and asks the source itself, so a wait whose source
has been released is no longer routed and falls back to the ordinary rule at the
next evaluation — and in practice never gets there, because revocation wakes it
immediately.

The limitation ADR-0059 §"Realisation" named becomes reachable here for the
first time, and this ADR does not soften it: while a routed source is live, a
peer deadlock beside it is not diagnosed. It is bounded by the source being an
authority that dies with its process, its assignment, or a revocation.

## 9. The boundary: what ring 0 learns, and what it must not

**The nucleus may know**: the MSI-X capability's layout, a table entry's layout,
which BAR the table is in and at what offset, how to allocate a CPU vector, how
to install an IDT gate, how to acknowledge through the local APIC, and how to
map one page of device memory into its own address spaces.

**The nucleus must not know**: that vector index 1 is queue 0's, that a
notification precedes a completion, what a Virtqueue is, what `device_status`
means, or that this is a block device. Which MSI-X entry serves which queue is
told to the *device* by the driver, through VirtIO's own registers in the
window the driver already maps. The nucleus's whole statement is "this source is
entry N of this function".

A gate asserts this mechanically over ring-0 source with comments stripped, as
Stage 4B's already does.

## 10. Performance

`docs/35` §Stage 4's budgets are per completed block request. This ADR
introduces no request path, and is checked against each anyway:

| Budget | This decision |
|---|---|
| zero dynamic allocation per steady-state request | a claim writes one table slot, once, at initialisation. A delivery allocates nothing |
| ≤1 payload copy | no payload |
| ≤4 handoffs per unbatched request | **one** crossing pair is added to the request path: `irq_wait` in, and out again when the device fires. That is the expected and unavoidable one, and it is counted in §11's Stage 4D projection |
| batching must stay possible | the latch is what makes it possible: one wake may cover any number of completions, and a driver that drains until empty needs no further wakeups |
| no global driver lock | a source is per-function state; two functions share nothing but the vector allocator, which is touched at claim time and never on a delivery |

## 11. What a Stage 4D request will cost, stated now

So that a later measurement has something to be checked against:

```text
driver writes descriptors into DMA memory   0 crossings   ordinary stores
driver notifies through MMIO                0 crossings   a mapped store
driver enters irq_wait                      1 crossing    in
device raises MSI-X, the nucleus wakes it   1 crossing    out
```

Two crossings per unbatched request on the driver's side, inside the budget of
four, with the remaining two available to whatever client exchange sits above
it. No syscall per descriptor field, none per MMIO access, and none per DMA
load or store.

## 12. What this ADR does not decide

DMA authority, DMA allocation, device-visible addressing, the IOMMU, the
MMIO↔DMA ordering contract, device reset, VirtIO feature negotiation, queues, or
block I/O. Bus mastering is decided here as far as §5d requires — the predicate,
the term, and the whole lifecycle — and the DMA slice **inherits** that rule
rather than restating it, adding a DMA mapping to the classification table and
changing nothing else.

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. I-07 is
  strengthened: interrupt routing gains a named origin and a traceable ancestry
  where it previously had neither, and three ambient paths to it are closed.
  I-08 is strengthened: a driver can wait for its device without a line of it
  being in ring 0.
- **Canonical representation:** unchanged.
- **Trusted-base impact:** the nucleus gains MSI and MSI-X capability parsing, a
  **retiring** vector allocator, an IDT gate range, one interrupt handler, a
  source table and its lifecycle, and ownership of two independent
  device-enable predicates. **No VirtIO knowledge**, which a gate enforces.
- **Source-to-runtime impact:** none. No new artifact class, no new digest.
- **Threat-model impact:** a new delivery path into a process, and — stated
  rather than implied — bus mastering on a platform with no IOMMU. `docs/34` S5
  and X4 §Drivers gain the entry §5 requires.
- **Compatibility profile:** `PLATFORM_INTERFACE_V1` at version 2, one new
  `OBJECT_*` kind, two new rights, two new `SYSTEM_ABI_V1` operations, and the
  narrowed domains of operations 26 and 27 — each a public boundary versioned
  from this commit under I-11.
- **Bounded-resource impact:** CPU vectors for device sources are **retired
  rather than recycled** within a boot (§5f), so their supply is finite and
  exhaustion is `E_LIMIT`. This is a deliberate trade of capacity for a
  stale-authority proof the generation mechanism cannot provide.
- **New dependencies:** none.

## 13. Conformance evidence

Positive, on the real device and from canonical text:

1. a textual module holding one `platform.pci.FunctionConfig` and nothing else
   derives a `platform.irq.Source` through it;
2. it blocks in `irq_wait`;
3. the real QEMU device raises a real MSI-X interrupt;
4. the module resumes because of that interrupt and says so;
5. the record shows the delivery arrived through the source it holds — no
   host-injected boolean, and no fixture supplying the answer;
6. ring-0 source contains no VirtIO vocabulary, checked mechanically.

Negative, and a successful wake alone is not sufficient evidence:

1. a process holding no `PciFunction` cannot obtain a device interrupt;
2. authority for function A cannot obtain function B's interrupt;
3. a numeric vector, GSI or MSI address/data tuple is not authority — there is
   no parameter for one;
4. a function capability without `interrupt` is refused;
5. a source capability without `wait` cannot wait;
6. a stale or released source refuses;
7. a re-claimed same BDF cannot be reached by a source of the previous
   assignment;
8. process death removes the waiter and releases the entry and the vector;
9. assignment teardown revokes descendants and cancels their waits;
10. a wait with no live source does not keep the system alive forever
    (`TOS.RUN.LIVENESS` shows `routed=0` after revocation);
11. a forged scalar in the capability position is refused independently by the
    verifier;
12. **`pci_bar_map_write` over the MSI-X table is refused**, and so is a window
    that merely overlaps it;
13. **`pci_config_write` over the MSI-X capability is refused**, and reads of it
    still work;
14. **`pci_config_write` that would change Bus Master Enable is refused**, and
    the refusal is one bit of one byte: every width and offset that includes
    `0x04` is judged on that bit alone, a one-byte write at `0x05` is not, and a
    write that leaves the bit as it found it proceeds.

The ownership repairs of §5a–§5f carry their own evidence, and it is separate
from the mechanism's because it exists before the mechanism does:

15. **BAR1 cannot move**, nor BAR4's low half, nor BAR4's high half;
16. a **readback of an unchanged protected value is still permitted**;
17. **`pci_bar_map` after a refused relocation still derives the original
    measured window** — which is the property §5a exists to preserve, and is not
    implied by the refusal alone;
18. **ordinary unrelated writable configuration fields remain writable**, so the
    narrowing is shown to be a narrowing and not a closure;
19. a claim leaves **BME clear, MSE clear, MSI-X Enable off, Function Mask on
    and MSI Enable off**, whatever the firmware left — proved against a machine
    whose firmware leaves BME *set*;
20. the **MSI capability's extent is derived from its own Message Control**, not
    from a blanket maximum, so a device with a capability after it has that
    capability still writable;
21. **MSE follows memory-decoding descendants and BME follows bus-mastering
    descendants, independently** — an `MmioRegion` moves one and not the other,
    and a source moves both;
22. a **retired vector is never handed to a second source** in the same boot,
    and a late interrupt on one is counted spurious and wakes nobody.
