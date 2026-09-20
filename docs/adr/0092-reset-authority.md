<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0092: Who may reset a PCI function, and what "reset" means

- Status: **Proposed**
- Date: 2026-09-21
- Decision level: **2** under `docs/21`. R2 and R3 each add a right to the
  `PciFunction` declared set and an operation to the closed `SYSTEM_ABI_V1` §5
  table; R3 additionally creates a power one process holds over hardware
  another process is driving, which is a threat-model addition in its own right
  (§9). R1 and R4 add no ABI surface but still decide §3's disposition, which is
  why all four are in one decision rather than three
- Related: **ADR-0079** §4 (reset listed open, no right allocated), §10 (the
  `PciFunction` rights table); **ADR-0081** §13 (BARs measured once at claim
  time), §14 (descendants keep an assignment alive); **ADR-0082** §5a (resource
  placement is static for an assignment; Secondary Bus Reset reserved on a
  bridge), §5b, §5d, §6, §9; **ADR-0084** §5b (the drain proof, and Transactions
  Pending as the bit an FLR waits on); `docs/11` §Crashes-and-restart;
  `docs/34`; `SYSTEM_ABI_V1` §5 operations 24–31;
  `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §6 and §11, which is the note
  this decision was raised from and is not authority for anything

## 0. What this decision is for

Stage 4's identity gate asks for "crash/restart and device-**reset** behavior"
(`docs/37`). The Stage 4 client/service slice cannot state its device-reset
acceptance criterion until the question below is fixed, and the Project
Architect has directed that the reachability recorded in §3 be dispositioned
deliberately rather than inherited.

**Nothing is chosen here.** Four options are set out with their consequences.
No option is marked preferred, and the ordering R1…R4 is the order they were
enumerated in, not a ranking.

**The persistent-data reading is already fixed and this ADR is written under
it.** The Project Architect selected Branch A on 2026-09-21: Stage 4 proves
that the write reached the device, that a later read sees the change, and that
the data remains available across a stop and restart of the block service.
Power-loss durability, `VIRTIO_BLK_F_FLUSH` and ungraceful-termination evidence
are **not** Stage 4's and are not arguments in this ADR.

## 1. The problem

A device whose driver has died or wedged is left in a device-defined state with
its queues configured and its rings holding whatever the dead driver published.
What the nucleus does today is drain the **assignment** — ADR-0082 §5d clears
Bus Master Enable, memory decoding goes off, and ADR-0084 §5b proves no
non-posted request is outstanding — and that is a statement about the *bus*,
not about the device. The device still believes it has a driver.

The nucleus cannot fix that, and must not: ADR-0082 §9 forbids ring 0 from
knowing what a queue is or that this function is a block device. So returning a
function to a known device-level state is necessarily somebody else's act, and
today no capability in this system carries that power on purpose.

## 2. Three things are called "reset", and they are not one decision

| | what it is | who could do it today |
|---|---|---|
| **T1 — VirtIO device reset by the successor** | writing `0` to the VirtIO `DEVICE_STATUS` register and re-running §3.1's initialization sequence | a new driver instance, through a window it holds. Needs nothing new |
| **T2 — PCI Function Level Reset by the function's own holder** | setting Initiate Function Level Reset, bit 15 of the PCI Express Capability's Device Control register | **apparently a holder of `config_write` today — see §3** |
| **T3 — reset of a function by a third party** | a supervisor or bus service resetting a function it does not hold, because the driver is dead or wedged | nobody. There is no right and no object |

Only T3 plainly needs the right ADR-0079 §10 says is unallocated. T1 needs
nothing. T2 is the one this ADR exists to disposition, because the system's
answer to it today appears to be "yes, by accident".

**T1 is already permitted by the tooling, which is worth recording.** The
`check-device-status-additive` gate enforces §2.1.1's "the driver MUST NOT clear
a device status bit" and admits exactly one non-additive write: the literal
`0u64`, named in the gate as "explicit reset, the one legitimate non-additive
write". So the mechanism T1 needs is not merely unforbidden, it is already
carved out.

## 3. The finding: FLR appears reachable through `config_write`

**Recorded as analysis of the source, not as a demonstrated defect.** It was
not attempted against the device, deliberately and on the Project Architect's
instruction; what follows is what the code says.

`write_is_permitted` in `nucleus/src/pci.rs` refuses a CPL-3 configuration
write that would change: the MSI and MSI-X capability structures; the
resource-placement registers of the reported header type; three Command
register bits; four ordering bits across PCI Express Device Control and Device
Control 2; and, on a Type 1 header, the four Bridge Control bits that alter
downstream routing or downstream device state. Everything else in conventional
configuration space is writable by a holder of `config_write`, at any offset
below 256 and any width of 1, 2 or 4 bytes.

The Device Control exclusion is bit-precise and its comment says why:

> **Four bits of two registers, and not the registers**: a driver has ordinary
> business in Device Control — maximum payload size, extended tags, maximum
> read request — and reserving all of it would be the wide narrowing ADR-0082
> §5a already refused for a Command register.

That reasoning is sound and this ADR does not reopen it. The observation is
narrower: **Initiate Function Level Reset is bit 15 of that same register and
is not among the four.** By the code as written, a write of Device Control with
bit 15 set is permitted.

**FLR is mentioned nowhere else in the repository** except in ADR-0084, twice,
and only as the thing Transactions Pending is normally read for — "the bit an
FLR waits on, used here without entering reset". No decision anywhere says
whether a function's holder may enter reset.

## 4. Why this is a conflict and not a curiosity: FLR against ADR-0081 §14

An FLR returns the function's configuration registers to their defaults. **That
includes the Base Address Registers.**

ADR-0081 §13 measures each BAR **once, at claim time**, and every later mapping
derives its physical base from that measurement. ADR-0082 §4 maps the MSI-X
table the same way, and §5's first refusal computes the table's extent from a
cached BIR. ADR-0081 §14 then keeps the assignment alive while any descendant
exists — a mapped window or, by ADR-0082 §6, an interrupt source — precisely so
that the cached layout stays the layout the live function decodes.

ADR-0082 §5a closed the direct route to breaking that, and it was measured
rather than supposed:

```text
BAR1  accepted a CPL-3 write   ← and BAR1 is the MSI-X table's own BAR
BAR4  accepted a CPL-3 write   ← the modern structures, low half
BAR5  accepted a CPL-3 write   ← the high half of the same 64-bit resource
and a window was still derived from BAR4 after BAR4 had been rewritten
```

Its conclusion was that relocation is "**incompatible with the assignment model
already accepted by ADR-0081**, and the resolution is to make the model's
assumption hold rather than to teach three mechanisms to chase a moving
resource."

**FLR reaches the same outcome by a different route.** §5a reserved the
registers; it did not reserve the operation that clears them wholesale. The
assignment would still be live, its descendants still mapped, the nucleus still
holding a measurement of a decode the function no longer performs.

**One route of the same family was reserved, which shows the rule's intent.**
ADR-0082 §5a's Type 1 protected set includes **Secondary Bus Reset** — a
Bridge Control bit — listed among "the Bridge Control fields that alter
downstream address routing or downstream device **state**". So reset was
recognised as a thing the rule must catch, on a bridge. A function's own FLR
is the Type 0 analogue and is absent from the Type 0 row.

Whether that absence is an oversight of §5a's rule or a deliberate allowance of
a holder's power over its own function is exactly what this ADR must settle. It
is not settled here.

## 5. Constraints any option must respect

1. **Ring 0 knows no device semantics** (ADR-0082 §9, gated since 2026-09-20 by
   the extended vocabulary guard in `stage4-profile.sh`). No option may put
   VirtIO initialization, queue knowledge or block semantics in the nucleus.
2. **A capability, never an ambient path** (ADR-0079 §5, ADR-0082 §5). Reset
   reached by holding a function rather than by being a driver.
3. **Descendants keep an assignment alive** (ADR-0081 §14, ADR-0082 §6). Any
   reset that changes decode must say what happens to live windows and sources,
   and it may not leave the nucleus holding a stale measurement.
4. **The drain proof must remain valid** (ADR-0084 §5b). It reads Transactions
   Pending *without entering reset*; an option that enters reset during or
   around teardown must say how the two interact.
5. **No right without an operation** (ADR-0079 §10's own reason for allocating
   none). An option that adds a right adds the operation in the same decision.
6. **Reads stay allowed and idempotent writes stay allowed** (ADR-0082 §5a).
   Whatever is reserved is reserved on the "would change" rule, not on the
   register.

## 6. The options

### R1 — no reset object; the successor resets at the VirtIO level

A restarted driver writes `0` to `DEVICE_STATUS` through its own window and
re-runs the initialization sequence. T1 only.

**Requires a disposition of T2 in the same breath**, and there are two
sub-forms that are genuinely different decisions:

- **R1a** — reserve Initiate FLR the way ADR-0082 §5a reserves the placement
  registers, on the same bit-precise "would change" rule. The Type 0 protected
  row gains one bit and the §4 conflict closes. No new right, no new operation.
- **R1b** — permit FLR as a holder's legitimate power over its own function,
  and state what happens to descendants. This is not free: §4's conflict has to
  be answered, and the only honest answers are that the assignment's
  descendants are invalidated, or that the nucleus re-measures, or that FLR is
  permitted only when no descendant exists.

### R2 — a `reset` right on the function capability, with an operation

The holder may reset the function it holds; a third party may not. Requires
deciding what the operation does at the PCI level (FLR, or something narrower),
what happens to live descendants, and whether the assignment's generation
advances — which is the same question §4 poses, now answered deliberately.

### R3 — a separate reset authority, held by a supervisor or bus service

A third party may reset a function it does not hold. This is T3, and it is what
a wedged driver actually requires, because in that case the holder is the
problem. Requires an object, a right, an operation, and a rule for what it does
to a live assignment, its descendants and any process still holding them.

### R4 — defer

State that Stage 4 proves recovery without device-level reset, and that reset
arrives with the bus/management separation. **This still requires disposing of
§3**: leaving the finding unstated is not deferral, it is silence about a
reachable power.

## 7. Consequences

| | R1a | R1b | R2 | R3 | R4 |
|---|---|---|---|---|---|
| new ABI surface | none | none | one right, one operation | one right, one operation, one object | none |
| reaches a **wedged** driver | no | no | no — the holder is the wedged one | **yes** | no |
| reaches a **dead** driver's device | yes, via the successor | yes | yes | yes | yes, via the successor |
| §4 conflict | **closed** | must be answered explicitly | must be answered explicitly | must be answered explicitly, and across processes | left standing unless §3 is dispositioned separately |
| trusted-base impact | one more reserved bit | none, and one fewer invariant | ring 0 gains an operation, no device semantics | ring 0 gains an operation usable across an ownership boundary | none |
| `docs/34` impact | none | a driver may invalidate its own mappings | as R1b | **new**: a denial-of-service primitive against a running driver | none |
| interacts with ADR-0084 §5b | no | during teardown, yes | yes | yes | no |

## 8. What each would let Stage 4 prove, and what it leaves outside

| | proves at Stage 4 | leaves outside Stage 4 |
|---|---|---|
| **R1a** | a successor brings the device back from whatever the dead instance left, by re-initialization alone; `docs/11` step 2 answered as "not needed at this stage", with a reason; the §4 route closed | third-party reset; any architected return to a known state that does not depend on the device's own initialization sequence |
| **R1b** | the same, plus a stated and bounded FLR power for a function's holder | third-party reset; a reset that is safe with descendants live, unless that is the stated answer |
| **R2** | the same as R1, plus an architected reset that does not rely on the device's initialization sequence being able to recover it | third-party reset — the case `docs/11` step 2 is actually written about |
| **R3** | `docs/11` step 2 as written: a supervisor resetting a device whose driver cannot | the bus/management service itself; R3 gives the right, not the service |
| **R4** | nothing about reset. The gate's "device-reset behavior" evidence line is **recorded as not met** rather than met by a narrower reading | all of it |

**What Stage 4 actually needs is narrow.** Under Branch A the lifecycle claims
are: a crash with a request in flight, the assignment teardown, a restart, and
re-initialization by the successor. Every one of those is reachable with T1
alone. T3 is what `docs/11` step 2 describes and what a wedged — as opposed to
dead — driver needs, and no Stage 4 acceptance criterion requires it.

**What can wait for the bus/management service**: T3 in full, shared-device
reset policy, reset of a function held by a process that is alive and
uncooperative, and any notion of a reset domain larger than one function.

## 9. Obligations each option creates for later stages

- **R1a** creates one more entry in the protected-register table that Stage 5's
  recovery paths and Stage 6's self-modification inherit. Cheapest.
- **R1b** creates a standing rule that a driver may invalidate its own
  mappings, which every later mapping consumer must be written against.
- **R2** creates a versioned operation in `SYSTEM_ABI_V1` that Stage 5 and
  Stage 6 must keep working, and a reset semantics that a future IOMMU decision
  must compose with.
- **R3** creates a cross-process power over hardware. It must enter `docs/34`
  as its own threat entry with negative tests: a right that can reset a device
  someone else is driving is a denial-of-service primitive, and it has to be
  introduced as one rather than discovered to be one. It also pre-commits the
  shape of the bus/management service that will hold it.
- **R4** creates the obligation to re-raise this before any stage claims
  device-reset behaviour, and to carry §3 as a known, stated reachability in
  the meantime.

## 10. What this ADR does not decide

- **IOMMU semantics.** Open in ADR-0079 §4, untouched here.
- **Whether the §3 reachability is a defect.** That is the disposition asked
  for, not a premise.
- **Any experiment.** No FLR is to be attempted against the device under this
  ADR; the Project Architect directed analysis by source-reading for this
  stage, and §3 is written to that standard.
- **The bus/management service.** R3 would allocate the right such a service
  needs; it does not design it.
- **Restart policy.** When to restart, how often and when to stop is canonical
  supervisor text (ADR-0077 §8), not this.

## 11. Conformance evidence this ADR would require once accepted

Listed so that acceptance carries its test obligations, and **not** to be
written into any evidence document before the tests exist and are gated.

1. Under R1a: a CPL-3 write setting Initiate FLR is refused with
   `E_NO_CAPABILITY`, and a write-back of the register's current value is
   permitted, on the same "would change" rule as the placement registers.
2. Under any option permitting a reset: a live window and a live interrupt
   source across that reset behave exactly as the option says they do —
   invalidated, re-measured, or the reset refused — proved by a negative that
   fails when the rule is removed.
3. Under any option: the nucleus still contains no device-protocol vocabulary,
   by the `stage4-profile.sh` guard.
4. Under R3: a process that holds no capability for a function cannot reset it,
   and the denial-of-service reachability is stated in `docs/34` with a test.
