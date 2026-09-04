<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0082: Where routed interrupt authority comes from, and how a textual driver waits for its device

- Status: **Proposed** — not approved, and must not be recorded as approved
  until the Project Architect has reviewed it. ADR-0081 §0 records what
  happened the last time an implementation ran ahead of a decision; this file
  exists before the mechanism it decides
- Date: 2026-09-05
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
3. `pci_config_write` refuses any access that would change the Command
   register's Bus Master Enable bit. Bus mastering becomes **nucleus-owned**:
   it is set while the function has at least one live device-visible descendant
   and cleared when it has none.

Refusing rather than filtering is deliberate throughout: a write that silently
kept some bits and dropped others would leave a driver unable to tell what the
device now holds.

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
ended.

**Process death.** The waiter stops being blocked with everything else the
process held; the sources it named are released by the ordinary capability
sweep; the last name going releases the entry, the vector and the descendant.
A dead process cannot leave a routed interrupt pointing at a slot.

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

## 11a. Review findings, and what is still open in this decision

The Project Architect's review of this document was positive in direction and
withheld approval pending six findings. They are answered in
`docs/evidence/STAGE4C1_REVIEW_FINDINGS.md`, and three of them change what this
ADR must say before it can be approved:

- **the Bus Master refusal above is now bit-precise** and gated, which is the one
  finding the review directed be repaired rather than proposed;
- **resource-placement registers are relocatable today, and the reference
  function proves it.** BAR1 — the MSI-X table's own BAR — accepts a CPL-3 write,
  as do both halves of the 64-bit BAR4 pair, and the nucleus still derives a
  window from the base it cached before the move. §5's first refusal and §4's
  nucleus mapping both rest on a cached layout that nothing currently keeps
  true. A narrowing is proposed in the findings report §2.6 and **is not
  implemented**;
- **bus-master ownership cannot start from a clear bit.** The reference
  machine's firmware hands TOS a function that is *already* bus-mastering, so
  §5's "nucleus-owned" needs the explicit lifecycle proposed in the findings
  report §3, including the claim-time clear. `device-visible descendant` is
  replaced by **bus-mastering descendant**, which excludes `MmioRegion` and
  includes an MSI/MSI-X source and a future DMA mapping.

The initial-state and teardown ordering §7 leaves implicit is set out in the
findings report §4, and is also a proposal.

## 12. What this ADR does not decide

DMA authority, DMA allocation, device-visible addressing, the IOMMU, the
MMIO↔DMA ordering contract, device reset, VirtIO feature negotiation, queues, or
block I/O. Bus-master enable is decided here only as far as §5's third refusal
requires — that it is nucleus-owned and follows the existence of device-visible
descendants — and the DMA slice inherits that rule rather than restating it.

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. I-07 is
  strengthened: interrupt routing gains a named origin and a traceable ancestry
  where it previously had neither, and three ambient paths to it are closed.
  I-08 is strengthened: a driver can wait for its device without a line of it
  being in ring 0.
- **Canonical representation:** unchanged.
- **Trusted-base impact:** the nucleus gains MSI-X capability parsing, a vector
  allocator, an IDT gate range, one interrupt handler, a source table and its
  lifecycle. **No VirtIO knowledge**, which a gate enforces.
- **Source-to-runtime impact:** none. No new artifact class, no new digest.
- **Threat-model impact:** a new delivery path into a process, and — stated
  rather than implied — bus mastering on a platform with no IOMMU. `docs/34` S5
  and X4 §Drivers gain the entry §5 requires.
- **Compatibility profile:** `PLATFORM_INTERFACE_V1` at version 2, one new
  `OBJECT_*` kind, two new rights, two new `SYSTEM_ABI_V1` operations, and two
  narrowed operation domains — each a public boundary versioned from this
  commit under I-11.
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
14. **`pci_config_write` that would change Bus Master Enable is refused.**
