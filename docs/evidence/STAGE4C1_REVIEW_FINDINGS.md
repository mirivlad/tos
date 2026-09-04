<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4C-1 — answers to the Project Architect's review of ADR-0082

- Status: **report, 2026-09-05.** ADR-0082 remains **Proposed — not Project
  Architect-approved**. Items 2, 3 and 4 below are **proposals only** and none of
  them is implemented
- Item 1 (the BME precision bug) is **fixed and gated**, because the review
  directed that it be repaired narrowly rather than proposed
- Stage 4C-1b — `platform.irq.Source`, vector allocation, MSI-X programming,
  `irq_wait` and the positive routed-interrupt proof — is **not begun**
- Audit fixtures: `source/tests/vectors/pci-bme-precision/init.tos`,
  `source/tests/vectors/pci-bar-relocation/init.tos`

## 1. The BME refusal is now one bit of one byte — fixed and gated

**The bug, as reported.** `write_is_permitted` compared `BUS_MASTER` against a
value shifted by `(COMMAND - offset) * 8`, saturating. For a one-byte write at
`0x05` the shift saturated to zero, so bit 2 of *that byte* — Command bit 10,
INTx Disable — was compared against the register's Bus Master Enable. A legal
write was refused for a bit it does not contain.

**The repair.** Bus Master Enable is bit 2 of the byte at `0x04`, and the rule
now says exactly that. `access_is_valid` already requires the offset to be a
multiple of the width, so an access either contains byte `0x04` whole or does not
touch it; the test is `offset <= COMMAND && COMMAND < offset + width`, the bit is
located at `(COMMAND - offset) * 8 + 2` inside the caller's value, and it is
compared against the same one bit read as a **single byte**. The refusal fires
only when the write would actually change it.

The mask constant was replaced by a bit *position* (`BUS_MASTER_BIT = 2`)
deliberately: a mask of a two-byte register is what invited comparing it against
whichever byte a caller happened to write.

**Evidence**, on the real device, in `virtio-mmio.sh` as the `bme-precision`
round. All seven cases the review enumerates, and an eighth that stops the other
seven passing on a nucleus that refused nothing and wrote nothing:

| # | Case | Expected | Result |
|---|---|---|---|
| 1 | width 1 at `0x04`, changing BME | `E_NO_CAPABILITY` | holds |
| 2 | width 1 at `0x04`, BME unchanged | allowed | holds |
| 3 | width 1 at `0x05`, toggling bit 2 of that byte | allowed | holds |
| 4 | width 2 at `0x04`, changing BME | refused | holds |
| 5 | width 2 at `0x04`, other Command bits only | allowed | holds |
| 6 | width 4 at `0x04`, changing BME / leaving it | refused / allowed | holds |
| 7 | reads unaffected | — | holds |
| 8 | the register really changed where allowed and really did not where refused | — | holds |

`TOS.RUN.COMPLETED value=i64:511` — 255 for the eight checks, plus bit 8, which
carries item 3's first measurement (§3.1).

The fixture is **polarity-agnostic**: it flips whichever way the bit currently
sits, so it tests the rule rather than the firmware. That turned out to matter.

## 2. BAR relocation — audited, and the problem is real

### 2.1 What the audit did

`source/tests/vectors/pci-bar-relocation/init.tos` claims the reference function
`00:04.0`, writes all-ones to each resource register, reads back, and restores —
the same probe the nucleus itself performs when sizing, so no register sees a
write it has not already seen. A bit is set only when the register **changed and
then restored**, so a register the device holds read-only is not counted as one
the nucleus protects.

Run against the real QEMU device: `TOS.RUN.COMPLETED value=i64:434`.

| Register | Offset | Result |
|---|---|---|
| BAR0 | `0x10` | unimplemented on this function — no change to observe |
| **BAR1** | `0x14` | **accepted the write** — and this is the **MSI-X table BAR** |
| BAR2 | `0x18` | unimplemented |
| BAR3 | `0x1C` | unimplemented |
| **BAR4** | `0x20` | **accepted the write** — the modern VirtIO structures, low half |
| **BAR5** | `0x24` | **accepted the write** — the *high half of the 64-bit BAR4 pair* |
| Expansion ROM BAR | `0x30` | absent on this function — no change to observe |
| header type | `0x0E` | Type-0 |
| **consequence** | — | **after BAR4 was rewritten, `pci_bar_map_read` still produced a window** |

### 2.2 The answer to the question asked

**Yes, the problem is real on the reference function, and it is worse than the
general statement of it.** Three findings, in order of severity:

1. **The MSI-X table BAR is relocatable.** ADR-0082 §5's first refusal computes
   the table's extent from `table_bar` and `table_offset` cached at claim time.
   A driver that moves BAR1 moves the table out from under that refusal — and,
   once 4C-1b exists, out from under the nucleus's own mapping of it. The
   nucleus would then be writing message addresses into a physical page the
   device no longer decodes, or into whatever else now lives there.
2. **Both halves of a 64-bit BAR are independently writable**, so the audit's
   answer is not "the low dword" but "the whole address", including the case
   where only the high half moves and the low half looks untouched.
3. **The cached layout is already being used after it can have gone stale.** The
   last row is the consequence rather than the capability: the nucleus handed
   out a window derived from a base the function had stopped decoding. Every
   live `MmioRegion` descendant has the same exposure — it keeps mapping a
   physical range its device has walked away from.

This directly contradicts the assumption ADR-0081 §13 rests on: *measured once,
at claim time, and cached in the assignment*. The measurement is sound; what is
missing is anything that keeps it true.

**STOP.** Per the review, this is reported and not implemented.

### 2.3 The exact writable registers today

`pci_config_write` refuses only what ADR-0082 §5 added: the MSI-X capability
structure, and a change to Bus Master Enable. Everything else in the first 256
bytes is writable at the caller's chosen width. Of that, the registers that
**place or enable a resource** are:

| Register | Type-0 offset | Type-1 offset | What it moves |
|---|---|---|---|
| Command, bit 0 | `0x04` | `0x04` | I/O space decode enable |
| Command, bit 1 | `0x04` | `0x04` | memory space decode enable |
| BAR0–BAR5 | `0x10`–`0x27` | BAR0–BAR1 only, `0x10`–`0x17` | where the function decodes |
| Expansion ROM BAR | `0x30` | `0x38` | where the option ROM decodes, and its enable bit |
| primary/secondary/subordinate bus | — | `0x18`–`0x1A` | which buses a bridge forwards |
| I/O base/limit | — | `0x1C`–`0x1D`, `0x30`–`0x33` | a bridge's I/O window |
| memory base/limit | — | `0x20`–`0x23` | a bridge's memory window |
| prefetchable base/limit | — | `0x24`–`0x27`, `0x28`–`0x2F` | a bridge's prefetchable window |

Command bit 1 is worth separating from the rest: clearing it does not *move* a
resource, it stops the function answering for it. That is mostly self-harm — the
driver's own window goes dead — but once the nucleus maps an MSI-X table it also
silently disables the nucleus's own access, so it belongs in the same
conversation even though it is not relocation.

### 2.4 Does the Type-0/Type-1 difference matter to the public rule?

**Yes, and a byte-offset rule that ignored it would be wrong in both
directions.** A Type-1 header has only two BARs; `0x18`–`0x27` are bus numbers
and bridge windows, and the Expansion ROM BAR is at `0x38` rather than `0x30`. A
rule written as "refuse `0x10`–`0x27` and `0x30`" would, on a bridge, refuse
nothing that matters and permit the registers that re-route whole buses.

`pci_function_claim` places no restriction on header type, so a bridge is
claimable today. The reference machine has Type-1 functions (the q35 root
complex), even though the Stage 4 profile does not claim one.

The honest public rule is therefore **stated over the header type the function
reports**, and the nucleus already reads the header type at claim time or can.
The two shapes are small and closed, so this is a table and not an analysis.

### 2.5 Does the Expansion ROM BAR belong in the same rule?

**Yes.** It is a base-address register with its own enable bit, it places a
decoded resource, and it is inside the range operation 26 reaches. It is absent
on the reference function, which is exactly why leaving it out would be a rule
that happened to be untestable rather than a rule that was right.

### 2.6 The minimum proposed narrowing (proposal only)

> For the lifetime of an assignment, the **resource-placement registers** of the
> function's reported header type are nucleus-owned, and `pci_config_write`
> refuses any access that would change one. Reads are unaffected.

Where *resource-placement registers* means, precisely:

- Type-0: BAR0–BAR5 (`0x10`–`0x27`) and the Expansion ROM BAR (`0x30`–`0x33`);
- Type-1: BAR0–BAR1 (`0x10`–`0x17`), the Expansion ROM BAR (`0x38`–`0x3B`), and
  the bus-number and window registers (`0x18`–`0x2F`);
- and, in both, **Command bit 1** (memory space decode), for the reason in §2.3.

The same "would change it" discipline as item 1 applies: a write that puts back
what it found proceeds, so read-modify-write of a neighbouring field is not
collateral damage. And the same refusal status: `E_NO_CAPABILITY`, because the
argument is well formed and what the caller lacks is the authority.

Command bit 0 (I/O space decode) is deliberately **excluded**: this stage refuses
I/O BARs outright, so nothing the contract grants depends on it.

**Why this is the minimum.** It is exactly the set of registers whose value the
assignment has already cached and already depends on. It adds no operation, no
right, no object and no lifecycle. And it makes true, mechanically, the sentence
ADR-0081 §13 already relies on.

**What it costs.** A driver can no longer place its own resources. On this
architecture it never should have been able to: `pci_function_claim` is the
platform mechanism, firmware has already assigned the BARs before TOS runs, and
ADR-0079 §5 put resource policy with the platform rather than with a driver.
The review's preferred direction and this analysis agree.

### 2.7 The alternative that keeps relocation, and what it costs

For completeness, since the review asked. A design that permitted BAR writes
would have to, on every accepted write to a placement register:

1. re-derive the function's whole BAR layout, because a 64-bit pair is two
   registers and a partial write leaves a half-moved address;
2. walk every live `MmioRegion` descendant of the assignment, and for each one
   either re-map its pages at the new physical base in **every** address space
   that holds it, or revoke it;
3. do the same for the nucleus's own MSI-X table mapping, and re-program every
   live table entry, since the entries move with the BAR;
4. serialise all of that against an interrupt that may arrive from the old
   address mid-update;
5. define what a driver observes when its window is revoked or silently
   re-pointed — neither of which any accepted contract currently describes.

Step 4 is the one that makes this more than bookkeeping: there is a window in
which the device has been told to decode a new address and the table has not yet
been rewritten, and a message delivered in it is a message from a masked-off
past. Getting that right needs the mask/unmask ordering of item 4 applied to a
case nothing requires.

**Recommendation: do not build it.** The review's instruction not to implement a
dynamic remap scheme merely to preserve unnecessary BAR-write freedom is, on this
evidence, the right call — nothing in Stage 4 needs a driver to place its own
resources, and the cost is a re-entrant update path guarding a race that would
otherwise not exist.

## 3. Bus Master Enable ownership, from the moment of claim (proposal only)

### 3.1 The initial state, measured

**The reference machine hands TOS a function that is already bus-mastering.**
This is not a hypothetical: the `bme-precision` fixture reports bit 8 set, which
is `has_bit(Command, BME) == 1` read immediately after `pci_function_claim`.
OVMF enables bus mastering during its own PCI enumeration and does not clear it
before handing over.

So the contract must not be written as though a claim starts from a clear bit,
and a lifecycle that only ever *sets* BME would leave every claimed function
bus-mastering for reasons that predate TOS entirely.

### 3.2 The precise term

`device-visible descendant` is ambiguous, as the review says — an `MmioRegion` is
device memory and is a descendant, and it needs no bus mastering at all. The
proposed term is:

> **bus-mastering descendant** — a descendant of an assignment whose *mechanism*
> requires the function to issue its own memory transactions.

By that definition, and stated as a rule rather than a list so the next
mechanism is judged rather than compared to a precedent:

| Descendant | Bus-mastering? | Why |
|---|---|---|
| `MmioRegion` / `MmioRegionMut` | **no** | the CPU reads and writes the device; the device initiates nothing |
| `platform.irq.Source` (MSI/MSI-X) | **yes** | the message *is* a memory write the device issues — verified in the emulator's own source, where `pci_msi_trigger` writes through `dev->bus_master_as` |
| a future INTx source | no | a line, not a transaction |
| a Stage 4C-2 DMA mapping | **yes** | the device reads and writes host memory directly |

### 3.3 The invariant

> Bus Master Enable is set **if and only if** the live assignment has at least
> one live bus-mastering descendant.

Both halves are load-bearing. "Only if" is what makes a claim clear the bit the
firmware left. "If" is what makes an interrupt source work without a driver ever
naming the bit.

### 3.4 The lifecycle, state by state

| Moment | BME | Note |
|---|---|---|
| immediately after `pci_function_claim` | **cleared** | whatever the firmware left is discarded. This is the state the audit shows cannot be assumed |
| before the first capability reaches CPL 3 | cleared | the clear happens inside the claim, so no CPL-3 instruction runs against a bus-mastering function it did not ask for |
| first bus-mastering descendant created | set | as part of creating it, before the descendant becomes nameable |
| second and later such descendants | unchanged | it is a predicate over the set, not a counter written per creation |
| one of several destroyed, others remain | unchanged | still at least one |
| the last such descendant destroyed | **cleared** | including when the last one goes because its *process died* |
| an `MmioRegion` created or destroyed | unchanged | not a bus-mastering descendant |
| process death | recomputed | the sweep releases that process's descendants; the predicate is re-evaluated once afterwards, not per release |
| assignment teardown | cleared | the assignment ends only when no name and no descendant remains, so the predicate is false by then; the clear is explicit rather than implied, because the device outlives the assignment |
| failed claim rollback | cleared | a claim that does not complete leaves the function as the claim found it *except* for BME, which is cleared — see below |
| failed capability grant after a successful claim | recomputed | the descendant is abandoned, so the predicate falls back to false and the bit is cleared |
| re-claim of the same BDF | cleared again | a new assignment starts from the same defined state as any other |

### 3.5 Is the pre-claim state restored?

**Deliberately not.** A release leaves BME clear rather than restoring what the
firmware left, and the reason is that the alternative is worse in both
directions:

- restoring "set" would hand the next claimant a bus-mastering function for a
  reason that is a fact about a previous environment, which is the exact
  situation §3.1 shows is real;
- restoring "clear" when the firmware had left it clear is indistinguishable
  from not restoring at all.

So there is one post-condition rather than two, and it is the safe one. This
means TOS does not hand a function back to firmware in the state firmware left
it. Nothing in the accepted corpus requires it to, and no path returns a claimed
function to firmware within a boot.

**This is a proposal.** It is not implemented, and §3.4's claim-time clear in
particular is a change to `pci_function_claim`'s observable effect on the device.

## 4. MSI-X initial state and teardown ordering (proposal only)

### 4.1 What must not be assumed

The audit's lesson generalises: firmware ran before TOS. The design must not
assume MSI-X arrives disabled, that Function Mask is set, that table entries are
masked, or that entry contents are zero. The reference machine's firmware does
not use MSI-X on this function, but "this firmware does not" is not a contract.

### 4.2 Creation order

Every step is a precondition of the next, and the order is what makes it
impossible for a message to be delivered to a vector that does not yet mean what
the message says:

```text
 1  clear MSI-X Enable in Message Control        the mechanism is off while it is being built
 2  set Function Mask                            belt and braces: an enable that races is still masked
 3  mask every table entry                       the state firmware left is discarded, not inherited
 4  clear Bus Master Enable                      §3.4: the function issues nothing yet
    -- the function is now in a defined state, whatever it arrived in --
 5  allocate a CPU vector                        nucleus state only; nothing is programmed
 6  install the IDT gate for that vector          a message that somehow arrived now lands somewhere defined
 7  publish the source object, pending clear      the waiter registry can name it before it can fire
 8  program the entry's message address and data  the message now means this source
 9  set Bus Master Enable                        §3.3's predicate becomes true
10  set MSI-X Enable, clear Function Mask        the mechanism is on
11  unmask the entry                             this source can now fire
```

Steps 1–4 run once per assignment, at claim; 5–11 run per source. **The IDT gate
precedes the message programming** (6 before 8), because the reverse leaves a
window in which a programmed message has no handler. **The source is published
before the entry is programmed** (7 before 8), because the reverse leaves a
window in which a delivered message has a handler that can find no source and
must treat it as spurious — losing an event the latch exists to keep.

### 4.3 Destruction order

The reverse, and the reason is the one the review names — an old message must not
be delivered to a newly reused vector:

```text
 1  mask the table entry                         the device is told to stop first
 2  read back Message Control                    a posted write is not a completed one; the read is the fence
 3  if this was the last source: clear MSI-X Enable
 4  if this was the last bus-mastering descendant: clear Bus Master Enable
 5  wake any waiter with E_CANCELLED             §7's revocation, before the object goes
 6  unpublish the source object                  a message arriving now is spurious, and is counted as such
 7  drain: acknowledge and discard any message already in flight
 8  release the CPU vector back to the allocator  only now can it be reused
 9  remove the IDT gate if no source uses it
```

**Step 2 is the one that is easy to omit and fatal to omit.** Masking is a
configuration write, and a write that has not been read back may not have taken
effect at the device; releasing the vector before that read is what makes an old
message land on a new source. **Step 8 after step 7**, likewise: a vector is
reusable only once nothing can still be in flight against it.

Between steps 6 and 8 a message may still arrive. It finds no live source, is
acknowledged, counted as spurious, and wakes nobody — which is §7's spurious rule
doing exactly the job it was written for rather than a gap.

### 4.4 What is not in this sequence

No device register outside the PCI-defined MSI-X capability and table. No VirtIO
status, no ISR, no queue register, no notion of what the device does. The
sequence is the same for any MSI-X function.

## 5. The no-IOMMU security profile, worded as accepted

The following is the wording to be carried into ADR-0082, `docs/34` and — when
it exists — the Stage 4C-2 DMA decision. It replaces the looser sentence
currently in ADR-0082 §5.

> **With no IOMMU, TOS cannot claim hardware-enforced confinement of a malicious
> bus-mastering device.** The capability model controls which sanctioned DMA
> objects and device-visible addresses software may **obtain**; it does not
> physically prevent a malicious driver from programming a bus-mastering device
> with some other address.

Two consequences that must be stated together and never separately:

- **sanctioned DMA authority and device-visible address issuance** require
  *both* memory funding authority and the live device assignment, and that is
  mechanically enforced;
- **hardware DMA confinement** is not provided by the no-IOMMU reference
  profile, and no contract sentence may imply it is.

In particular, and recorded here so it cannot be written later by accident:
**it is false, on this profile, to say that possession of only a `PciFunction`
makes arbitrary RAM physically invisible to the device.** Once BME is set — which
a routed interrupt requires — the device can address any physical memory the
driver programs into it. What the capability model bounds is which addresses a
driver can obtain *legitimately*, not which addresses the hardware will accept.

An IOMMU backend later strengthens confinement without changing the public DMA
object model, which is why the object model must not be written in terms of
identity-mapped physical addresses.

`docs/34` S5 already governs this — "IOMMU absence or limitations are reported as
a weaker security profile, not hidden" — and T5 already declines to claim full
protection against DMA-capable hardware outside isolation. This wording is what
satisfies them rather than a new concession.

## 6. Editorial

ADR-0082 §5's heading and body said "two" while enumerating three; both now say
three, and the two places that described the configuration-space refusals as
"two structures" now say "two of the function's registers", which is what they
are. `SYSTEM_ABI_V1` operation 26's row and `pci.rs` carry the same correction.

## 7. Status

| Item | State |
|---|---|
| 1 — BME precision | **fixed, gated, green** |
| 2 — BAR relocation | **audited; problem confirmed real; narrowing proposed, not implemented** |
| 3 — BME lifecycle | **proposed, not implemented** |
| 4 — MSI-X ordering | **proposed, not implemented** |
| 5 — no-IOMMU wording | **written as accepted** |
| 6 — editorial | **done** |
| Stage 4C-1b | **not begun** |

ADR-0082 remains `Proposed — not Project Architect-approved`.
