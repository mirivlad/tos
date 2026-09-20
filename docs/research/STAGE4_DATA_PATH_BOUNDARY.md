<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — the client/device data path: an architectural note before the decision

- Status: **note, not a decision.** Tier 4 under `docs/38`: research and
  explanatory material, incorporated by no ADR.
- Date: 2026-09-20
- Audience: the Project Architect, before the Stage 4C/4D closure ruling and
  before any of the work below is begun.

## 0. What this is, and what it is not

**It is a boundary, not a plan.** It states what the next slice would have to
prove, where each contract already decides the answer, and where a decision
does not exist yet. It deliberately stops short of a design: no module layout,
no operation numbers for anything new, no schedule.

**It accepts nothing.** Two new ADRs are sketched in §11 and §12 with their
options and consequences and **no recommendation**. Both would be `Proposed`.

**It is placed in `docs/research/` on purpose.** The one previous
pre-decision document of this kind, `STAGE4C1_REVIEW_FINDINGS.md`, was filed
under `docs/evidence/`, and a later reader took it for evidence of the system
as built — which cost a whole corrective document
(`STAGE4C1_ROUTED_INTERRUPT.md` §0). A note written before the thing exists
does not belong beside proof that it does.

**It is deliberately not listed in `docs/SPECIFICATION_SOURCES.txt`.** The
manifest gate requires accepted ADRs and neither requires nor forbids anything
else; shipping a document whose whole content is "nothing is decided yet"
inside the consolidated specification bundle would invite exactly the reading
the previous paragraph describes. It is added to the manifest if and when it
becomes something other than a note.

## 1. The minimal end-to-end data path

The Stage 4 identity gate asks one question (`docs/37`):

> does a canonical textual user-space driver actually move persistent data
> through final-style MMIO/interrupt/DMA/**IPC** boundaries?

IPC is inside the question. Stage 4D-1…4D-5 proved the right-hand half of that
path — queue, MMIO, DMA, interrupt, a real write and an independent read-back —
with the driver as **its own client**. Nothing has ever crossed a process
boundary to reach the device.

The minimal path that answers the question whole:

```text
client process                    block service / driver process     device
--------------                    ------------------------------     ------
compose payload in Region<mut T>
region_freeze          (op 18)
endpoint_call          (op 3) ------> endpoint_receive   (op 2)
  carrying Region<T>                    copy Region -> DmaRegion<mut T>
  + sector + direction                  build descriptor chain
                                        dma_publish
                                        MMIO notify
                                        irq_wait          (op 29) ---> MSI-X
                                        dma_consume
                                        read used ring, check status
                                      <- endpoint_reply   (op 4)
                                         or reply_receive (op 13)
receives the answer
```

**The copy is forced, not chosen.** ADR-0037's table makes `DmaRegion<T>` and
`DmaRegion<mut T>` neither shareable nor transferable in either mode, so a
client cannot be handed device-visible memory and a driver cannot be handed the
client's. Exactly one copy therefore exists between client memory and
device-visible memory — which is precisely the ceiling `docs/35` sets.

**One consequence must be recorded rather than discovered later.** `docs/35`
also says "zero-copy is preferred where the DMA contract permits it". Under
ADR-0037 the DMA contract permits it nowhere, so that preference is currently
unreachable **by decision**, not by omission. If zero-copy is wanted at Stage 4,
ADR-0037's table is what would have to change, and that is a separate Level 2
decision this note does not open.

**What is genuinely new here** is the left-hand half and the lifecycle. The
VirtIO work is nearly all done: queue initialization (4D-1), a real read (4D-2),
queue reuse (4D-3), two requests outstanding (4D-4), a write proved by
read-back (4D-5). Scoping the slice as "another piece of device work" would
overstate it by a wide margin.

## 2. The minimal IPC contract

Nothing new in the ABI is needed for the data path itself. What exists:

| need | already decided |
|---|---|
| request/reply | `IPC_V1` §4; ops 3, 4, and 13 `endpoint_reply_receive` (ADR-0063) so a server answers and re-waits in one crossing |
| payload as a capability, not a copy through the nucleus | `IPC_V1` §5: a region is transferred as a capability; the nucleus maps and unmaps and does not copy the payload through itself |
| linear transfer semantics | `IPC_V1` §5 / `CAPABILITY_V1` §4: `Region<T>` is Transferable into exactly one task; `Region<mut T>` may not be sent at all |
| where handles travel | ADR-0058: `MESSAGE_CAPABILITIES` and `MESSAGE_REGIONS` in the argument region, not in registers |
| backpressure | `IPC_V1` §7 |
| the counting budgets | `IPC_V1` §8 — unchanged by ADR-0068, which withdrew only the relative latency bound |

So the minimal IPC contract for this slice is a **message shape**, not a
mechanism: what a block request and its answer contain. That is interface
design inside an accepted contract, and it does not need an ADR unless the
shape wants something `IPC_V1` does not offer.

**One real constraint to design against.** A call carries at most three
capabilities of its own, because one capability place is spoken for by the
answer (`SYSTEM_ABI_V1` op 3). A request carrying one payload region is well
inside that; a request wanting to carry several is not obviously so.

## 3. The minimal `block.device.v1`

`docs/11` §Driver interfaces names `block.device.v1` as the class interface a
storage driver publishes. ADR-0079 §4 lists its publication **open**, while
noting its shape "needs nothing new" (ADR-0051 §2, `CAPABILITY_V1` §6).

What is already decided: **the right to publish an interface is itself a
capability whose nominal type is the interface** (`CAPABILITY_V1` §6). There is
no self-declared `provides`, and the registry never holds an entry nobody
granted. So a publishing driver must be *given* the right by whoever launches
it — which is ADR-0077's launch plan, and needs no new mechanism.

What is **not** decided is everything about the registry as an object: whether
one exists at Stage 4 at all, who holds it, how a client obtains a capability
naming a published interface, and what happens to an entry when its publisher
dies. §12 is about exactly that.

A minimal V1 surface for the gate would be small: read a sector range into a
supplied region; write a sector range from a supplied region; report capacity.
Anything beyond — partitions, naming, enumeration, multiple devices, queueing
policy — is not what the gate asks.

## 4. Where the boundaries are in this path

| boundary | who holds it | contract |
|---|---|---|
| **capability** — client → service | the client holds an endpoint capability with `call` and nothing else; it holds no function, no window, no source, no DMA region, and cannot name the device in any way | `CAPABILITY_V1` §3, `IPC_V1` §4 |
| **capability** — launcher → service | the service is endowed by a sealed launch plan with: a PCI function capability, memory authority, and (if §12 goes that way) a publication right | ADR-0077 §3–§5, op 19/20 |
| **MMIO** | `pci_bar_map` (op 27); physical base from the assignment's own measured BAR state, never a caller argument; a window overlapping the MSI-X table or PBA is refused | ADR-0081 §13, ADR-0082 §5 |
| **DMA** | `dma_region_allocate` (op 30) needs **two** capabilities — the function with `dma` and memory authority with `spend`; neither alone can make any memory reachable by any device | ADR-0084 §4–§5 |
| **device addresses** | `dma_device_address` (op 31): a capability and an offset in, an address out; no operation anywhere accepts one back | ADR-0084 §6b |
| **ordering** | `dma_publish` / `dma_consume` as ring-3 barriers, not syscalls | ADR-0086 §6, §11 |
| **interrupt** | `pci_interrupt_claim` (op 28) derives a source from the function's own MSI-X entry index; no vector, GSI, MSI pair or BDF is a parameter | ADR-0082 §3 |
| **nucleus ignorance** | ring 0, the runtime binary and the engine contain no VirtIO or block-protocol vocabulary, checked mechanically with comments stripped | ADR-0082 §9; gated since 2026-09-20 in `stage4-profile.sh` |

**The client side of this table is the point.** Every row above the first is
already proved at Stage 4C/4D. The first row is what the slice adds, and it is
the row the identity gate's word "IPC" refers to.

## 5. Crash of the device's owner

**Partly proved already, and the part that is proved must not be reused as
proof of the rest.** As of 2026-09-20 the Stage 4D-5 gate asserts, in lifecycle
order, that a process dying while holding a function, a window, a source and a
DMA region leads to: quarantine of the live DMA backing rather than return to
the pool (`kept=1`), drain of the assignment (memory decoding and bus mastering
off), reclaim with the run proved safe and the charge refunded, retirement of
the vector with its source, and `dma_quarantined=0` at reclaim.

That is `docs/11` §Crashes-and-restart **step 1** — revoke device mappings —
and nothing else. It says nothing about steps 2–5.

What the slice would add, and what is genuinely new:

- a crash **with a request in flight**: the device holds descriptors the dying
  process published, and the drain must be sound while the device may still be
  writing. ADR-0084 §5b's Transactions Pending proof is the mechanism; it has
  never been exercised against an actually outstanding block request.
- a crash **with a client blocked in `endpoint_call`**: what the client sees.
  `IPC_V1` §4 and the liveness rule decide this; whether the answer they give
  is the one wanted is a question the slice must ask out loud, not assume.

## 6. Device reset

**There is no reset authority, and the note must say so plainly.** ADR-0079 §10
in the `PciFunction` table:

> reset | **no right allocated.** A right with no operation would be a contract
> describing a system that does not exist

and `docs/11` says a supervisor resets "through a **bus service**", which does
not exist either.

**Three different things are being called "reset", and they are not one
decision.**

1. **VirtIO device reset by the successor** — writing 0 to `device_status` and
   re-running the initialization sequence. This is a *driver* action in ring 3,
   through a window the successor holds. It appears to need no new right.
2. **PCI Function Level Reset by the function's own holder** — see the finding
   below.
3. **Reset of a device whose driver is dead or wedged, by a third party** — a
   supervisor or bus service acting on a function it does not hold. This needs
   a right that does not exist, and an object to hold it.

**A finding that belongs in the reset ADR rather than in a bug report.**
`write_is_permitted` in `nucleus/src/pci.rs` reserves four ordering bits of the
PCI Express Device Control and Device Control 2 registers, and explicitly
leaves the rest of Device Control to the driver as "ordinary business —
maximum payload size, extended tags, maximum read request". **Bit 15 of Device
Control is Initiate Function Level Reset**, and it is not in the reserved set.
A holder of `config_write` therefore appears able to FLR its own function
today.

If that is so, it is not merely a nominal contradiction with "no right
allocated". An FLR returns the function's configuration registers to defaults,
**BARs included** — and ADR-0081 §14 keeps an assignment alive while a mapped
window descends from it, on the premise that the function's decode does not
move. A driver could clear the BARs under a live mapping the nucleus still
believes in.

**Stated as reachability by inspection, not as a demonstrated defect.** This
note did not attempt it on the device and does not propose to as part of
itself. Whether it is a hole, or a capability holder's legitimate power over
its own function, is exactly what the reset ADR must decide deliberately
instead of inheriting by accident.

## 7. Service restart

Available without anything new: `process_create_funded` (op 19) with the
supervisor-asserted restart generation in `CREATE_FUNDED_RECORD`, and
`process_wait_child` (op 14) for the supervisor to learn the ending — ADR-0067,
accepted and closed 10/10; ADR-0076 §3 for funding.

The restart question at Stage 4 is not "can a process be started again". It is
**what the new instance inherits and what it must rebuild**: the old assignment
generation is gone, so the function must be re-claimed, the window re-mapped,
the source re-derived, the DMA region re-allocated, and the device
re-initialized. Every one of those already has a contract. What has no contract
is the **published interface**: whether the client's endpoint capability
survives the publisher's death, which is §12's question and not this one's.

**This is the criterion that must not be merged with §5.** "The service came
back" and "the device state the dead owner left behind was correctly recovered"
are separate obligations with separate evidence: the first is a supervisor
fact, the second a nucleus fact, and a run can show either without the other.

## 8. Stale client

`docs/11` §Crashes-and-restart step 5 — "notify clients of interruption" — is
the normative source; this is not an invented requirement.

The cases, which are not one case:

- a client blocked in `endpoint_call` when the service dies;
- a client holding an endpoint capability naming an endpoint whose receiver is
  gone, that has not yet called;
- a client that calls **after** a restart, holding a capability minted before
  it;
- a request the old instance accepted and never answered — whether the client
  can tell "not done" from "done, answer lost" at all.

The last one is the architecturally interesting one and the slice should say
what it proves rather than what it hopes. A block write whose answer was lost
is exactly the case where "retry" and "do not retry" differ.

## 9. Evidence artifacts

Following the established Stage 4 pattern — one gate per claim, one evidence
document per slice, no claim without a gate that fails when it is false:

| artifact | what it would hold |
|---|---|
| a new QEMU gate for the data path | the end-to-end path of §1, with the client as a separate process and the negatives that make it a boundary |
| a new QEMU gate for lifecycle | crash-in-flight, restart, stale client — **separate assertions**, so no one of them is satisfied by another's evidence |
| `docs/evidence/STAGE4_CLIENT_DATA_PATH.md` | the positive claim, its negatives, and its explicit non-claims |
| `docs/evidence/STAGE4_SERVICE_LIFECYCLE.md` | §5–§8, with the split of §7 stated in the document and not only in the gate |
| an extension of `stage4-profile.sh`'s vocabulary guard | so the client/service split does not become a route for device vocabulary into a production binary |
| **no** performance report yet | see §10 |

## 10. Deliberately outside this slice

- **A filesystem, a block cache, a partition model, an object store.** None is
  in the gate's question.
- **Persistent object/state storage and the capsule-to-repository handoff.**
  These are Stage 4 deliverables in `docs/16` and they are **not** in this
  slice. Consequence, which must be recorded now and not negotiated later:
  **this slice does not close Stage 4.** `docs/16` gives Stage 4 two exits — an
  engineering exit ("persistent storage works through a textual user-space
  driver") and an identity exit ("the textual driver performs actual I/O from
  canonical source; no binary shadow driver"). The slice advances the identity
  exit and leaves the engineering exit open.
- **The Stage 4 performance report.** `docs/35`'s Stage 4 budgets are written
  about "one payload copy between **client** memory and device-visible memory"
  and "four address-space/scheduler handoffs per unbatched request". Neither is
  measurable before the client exists. Once it does, the existing contract
  applies unchanged and the report is built then — not designed now around a
  path that does not exist.
- **IOMMU semantics.** Open in ADR-0079 §4 and untouched here. `docs/34` S5 and
  ADR-0082 §5 record the no-IOMMU reference profile as a weaker security
  profile, stated rather than hidden.
- **Multiple devices, multiple queues, multiple clients.** `docs/35`'s "no
  global driver lock serializes independent queues" is a long-term contract
  about a system with independent queues; this slice has one.
- **Zero-copy.** Forbidden by ADR-0037 today (§1).

## 11. ADR sketch — reset authority (`Proposed`, nothing chosen)

### The problem it must solve

A device whose driver has died or wedged is left in a device-defined state with
its queues configured. The nucleus drains the *assignment* — decoding and
mastering off — but performs no device-level reset, and must not: ADR-0082 §9
forbids ring 0 from knowing what a queue is. Someone must be able to return a
function to a known state, and today no capability in the system carries that
power deliberately.

### Constraints already fixed

- ADR-0079 §10: `PciFunction` rights are `config_read` and `config_write`,
  separate; **reset has no right allocated**.
- ADR-0079 §4: reset authority is **open**, and this is not a nucleus policy
  question the ADR may answer implicitly.
- ADR-0082 §5 and §9: some configuration registers are the nucleus's; ring 0
  knows no device semantics.
- ADR-0081 §14 / ADR-0082 §6: windows and sources are **descendants** of the
  assignment and keep it alive. Any reset that changes decode interacts with
  this rule directly.
- ADR-0084 §5b: the drain proof rests on PCIe Transactions Pending — the bit an
  FLR waits on — and ADR-0084 uses it *without entering reset*.
- `docs/11`: reset is described as happening "through a bus service", which
  does not exist.
- The §6 finding: FLR appears reachable today through `config_write`.

### Options

**R1 — no reset object; the successor resets at the VirtIO level.** A restarted
driver writes 0 to `device_status` through its own window and re-initializes.
Nothing new in the ABI. Explicitly states that FLR through `config_write` is
either permitted (and the BAR interaction handled) or reserved like the other
nucleus-owned bits.

**R2 — a `reset` right on the function capability, with an operation.** The
holder of the function may reset it; a third party may not. Requires deciding
what "reset" means at the PCI level, what happens to live descendants, and
whether the assignment generation advances.

**R3 — a separate reset authority, held by a supervisor or bus service.** A
third party may reset a function it does not hold, which is what a wedged
driver actually requires. Requires an object, a right, and a rule for what it
does to a live assignment and its descendants.

**R4 — defer.** State that Stage 4 proves recovery without device-level reset,
and that reset arrives with the bus/management separation.

### Consequences

| | R1 | R2 | R3 | R4 |
|---|---|---|---|---|
| new ABI surface | none | one operation, one right | one operation, one right, one object | none |
| reaches a wedged driver | no | no — the holder is the wedged one | yes | no |
| interacts with ADR-0081 §14 descendants | only if FLR is permitted | directly | directly, and across processes | no |
| resolves the §6 finding | must, explicitly | must | must | leaves it standing |
| trusted-base impact | none | ring 0 gains a reset operation, no device semantics | ring 0 gains an operation usable across an ownership boundary | none |

### What each would let Stage 4 prove

- R1: a restarted driver brings a device back from whatever the dead one left,
  by re-initialization alone. `docs/11` step 2 is answered as "not needed at
  this stage", with a reason.
- R2: the same, plus an architected return to a known state that does not
  depend on the device's initialization sequence being able to recover it.
- R3: `docs/11` step 2 as written — a supervisor resetting a device whose
  driver cannot.
- R4: nothing about reset; the gate's "device-reset behavior" evidence line is
  recorded as not met, rather than met by a narrower reading.

### What each leaves outside Stage 4

R1 and R2 leave third-party reset to the later bus/management separation. R3
does not by itself give a bus service; it gives the right one would need. R4
leaves all of it.

### Obligations created for later stages

R2 and R3 both create a versioned operation in `SYSTEM_ABI_V1` that Stage 5's
recovery paths and Stage 6's self-modification must keep working. R3
additionally creates a cross-process power over hardware that `docs/34` must
cover with its own threat entry and negative tests — a right that can reset a
device someone else is driving is a denial-of-service primitive, and it must be
introduced as one.

## 12. ADR sketch — publication of `block.device.v1` (`Proposed`, nothing chosen)

### The problem it must solve

A client must obtain a capability naming a running block service without
knowing which process it is, and that capability must behave sanely when the
service dies and is replaced. `CAPABILITY_V1` §6 fixes the *right* to publish;
it fixes nothing about a registry, about lookup, or about what an entry's
lifetime is.

### Constraints already fixed

- `CAPABILITY_V1` §6 / ADR-0051 §2: the right to publish an interface is a
  capability whose nominal type is the interface. No self-declared `provides`;
  the registry never holds an entry nobody granted.
- ADR-0079 §4 and §11: publication of `block.device.v1` is **open**, and its
  shape "needs nothing new".
- ADR-0077: a launch plan is how a creator endows a child, so granting the
  publication right needs no new mechanism.
- `IPC_V1` §2 and `CAPABILITY_V1` §4: endpoints, delegation, attenuation and
  revocation are decided.
- `docs/11` §Crashes-and-restart step 4 — "restore published interface
  endpoints" — is a normative obligation on whatever is chosen.

### Options

**P1 — no registry at Stage 4; the launcher wires client to service directly.**
The launcher creates the endpoint, endows the service with `receive` and the
client with `call`. Nothing new. Publication is deferred whole.

**P2 — a registry object in the nucleus.** Publish and lookup as operations.
Smallest surface for clients; largest trusted-base cost, and it puts an
interface-name namespace inside ring 0.

**P3 — a registry as an ordinary textual service.** A name service holding
publication entries, reached over IPC like anything else. No trusted-base
growth; a bootstrapping question of its own, and one more process in the path.

**P4 — the supervisor is the registry.** It already creates both parties and
already learns of endings through op 14. It hands out endpoint capabilities as
part of launching a client.

### Consequences

| | P1 | P2 | P3 | P4 |
|---|---|---|---|---|
| clients can find a service they did not launch | no | yes | yes | only through the supervisor |
| trusted-base growth | none | **a namespace in ring 0** | none | none |
| survives publisher death | not applicable | registry decides | registry decides | supervisor decides, and it is already watching |
| new ABI surface | none | publish + lookup | none | none |
| how far from `docs/11` step 4 | step 4 is vacuous | close | close | close |

### What each would let Stage 4 prove

- P1: the whole data path and the whole lifecycle, with "who found whom"
  outside the frame. The identity gate's question is answered; `docs/11` step 4
  is not.
- P2/P3/P4: additionally that a client reaches a service by interface rather
  than by wiring, and that the reference survives — or is deliberately
  invalidated by — a restart.

### What each leaves outside Stage 4

P1 leaves publication entirely, and with it the honest form of §8's stale
client: without a registry there is no "re-lookup after restart", so recovery
can only be tested as "the capability the client already holds does X". That is
a real narrowing and should be named as one rather than discovered when the
evidence is written.

### Obligations created for later stages

P2 creates a ring-0 namespace that Stage 5 must reconcile with `/system` being
a commit tree and `/dev` being a capability namespace (`docs/09`). P3 creates a
service that must itself be restartable and must exist before any driver,
touching the `docs/11` bootstrapping sequence. P4 makes the supervisor a
single point of failure for device access and must be stated as one in
`docs/34`.

## 13. "Persistent data": the two branches

`docs/37` asks whether the driver moves **persistent data**. The word is not
defined in `docs/37`, `docs/11` or `docs/09`, and the two available readings
differ by more than a detail.

### What is already true at Stage 4D-5

The gate proves: a 512-byte non-uniform pattern composed by a textual module,
sent as `VIRTIO_BLK_T_OUT` to sector 5 of a real VirtIO block device; the
source buffer then scrubbed to zero and the scrub verified, so the pattern
exists nowhere in guest memory; a `VIRTIO_BLK_T_IN` of the same sector into a
**separate** poisoned buffer returning exactly those bytes, every byte
recomputed from its index rather than compared to a saved copy; and on the host
afterwards, the backing image equal to the reference image with sector 5
rewritten and no other byte changed.

The gate explicitly does **not** claim: durability, fsync semantics, survival of
QEMU termination, a flush implementation, a block service or a filesystem. The
backend is `cache=writeback` and the post-run image does not prove
commit-at-completion.

### Branch A — the medium changed

**What would be proved:**

1. *The write reached the device.* Already proved, and proved the hard way:
   §2.7.5.1 lets a device decline to read a buffer wrongly marked
   device-writable while still reporting `VIRTIO_BLK_S_OK`, so the status byte
   is not the evidence and the read-back is.
2. *After completion, a re-read returns the changed data.* Already proved,
   through a separate poisoned buffer after the source was scrubbed.
3. *The data survives stopping and restarting the service.* **New.** The
   successor instance re-claims the function, re-maps, re-derives, re-allocates,
   re-initializes the device, reads the sector, and finds what the predecessor
   wrote. This is a within-one-boot claim and needs no new contract.
4. *What happens at device reset.* Dependent on §11. Under R1, the successor
   re-initializes and the claim is that re-initialization does not disturb the
   medium. Under R2/R3, the same claim after an architected reset.
5. **Boundaries that remain.** Nothing about power loss. Nothing about *when*
   the write became durable. Nothing about the host's storage implementation —
   `cache=writeback` means QEMU may hold it, and a clean exit flushing it is a
   property of clean exit, not of the completion. And the host image comparison
   remains what it has always been: a target-and-location check.

**Cost for this slice:** the service/client split and the lifecycle work, which
§1–§8 require anyway. Branch A adds **no device work, no new contract and no
new ADR** beyond the two already sketched. Claims 1 and 2 are done; claim 3 is a
consequence of the restart criterion; claim 4 is a consequence of the reset
decision.

### Branch B — survives power loss

**What is added over A:**

1. **Flush/barrier semantics.** `VIRTIO_BLK_F_FLUSH` would have to be
   negotiated — this driver negotiates neither it nor
   `VIRTIO_BLK_F_CONFIG_WCE` — and `VIRTIO_BLK_T_FLUSH` issued and completed.
   That changes feature negotiation, adds a request type, and changes the
   driver's completion path.
2. **A defined durability point.** Today the contract is read the other way
   round: §5.2.6.2 classifies a completed write as stable under case 1
   *precisely because* neither feature is negotiated, and Stage 4D-5 cites that
   and then declines to claim durability from it. Branch B must name the
   instant after which a write is durable — flush completion — and make every
   claim relative to it.
3. **A reset/power-cycle model.** A clean QEMU exit flushes; therefore the
   present post-run image comparison proves nothing about power loss, and a
   harness that merely restarts QEMU proves nothing more. Branch B needs either
   an ungraceful kill of QEMU at a chosen instant, or a backend configured not
   to hold writes, and it must say which and why. QMP `system_reset` is a
   machine reset, **not** a power cycle of the emulated storage backend, and
   using it as one would be the kind of convenient reading this project
   refuses.
4. **What counts as confirmation.** Reopening the image after an ungraceful
   kill and finding the bytes. Note the asymmetry, which is Branch B's real
   difficulty: the positive is provable, but the matching **negative is not**.
   "Without a flush the data is not durable" is unprovable, because
   `cache=writeback` is permitted to have written it anyway. A branch whose
   negative cannot fail is a branch whose evidence is weaker than this
   project's other evidence, and that must be stated rather than glossed.
5. **New requirements on device, driver and harness.** Device: a backend whose
   caching behaviour is pinned by the profile rather than inherited. Driver:
   feature negotiation, a flush request path, and a completion rule that
   distinguishes written from durable. Harness: an instant-precise kill, a
   reopen path, and a reference image discipline that survives it.

**Cost for this slice:** roughly a second slice of device work on top of
everything Branch A needs — comparable to one of the 4D sub-stages — plus a
harness capability that does not exist, plus the honest statement in point 4.

### The question this poses

Branch A answers `docs/37`'s question under the reading "the device's durable
medium changed, and an independent reader through the same device sees it".
Branch B answers it under "survives power loss".

**What the note can say.** Nothing in Tier 0 or Tier 1 requires B at Stage 4.
`docs/11` warns that "storage drivers require special care to avoid silent data
corruption" and that a crash may force read-only mode or revalidation — a
statement about crash recovery policy, not a durability contract. `docs/09`'s
durable-state machinery is about services with state schemas, which is the
engineering exit, not the identity one. `docs/35` sets no durability budget.

**What the note must not say.** Which branch is right. The choice determines
whether Stage 4's identity exit can close on A with B as a separate later
durability gate, or whether TOS's architecture already commits it to B — and
that is a Level 2 reading of the gate's own words.

## 14. Decisions the Project Architect must make

Only what is needed to fix the architectural boundary. Everything else in this
note is either already decided by an accepted contract or is an implementation
choice.

1. **What "persistent data" means for the Stage 4 identity gate — Branch A or
   Branch B (§13).** It changes the size of the slice roughly twofold and
   determines whether the identity exit can close without a durability gate.

2. **Reset authority — which of R1…R4 (§11), and with it the disposition of the
   FLR finding in §6.** The slice cannot state its device-reset acceptance
   criterion until this is fixed, and the finding should be dispositioned
   deliberately either way.

3. **Publication of `block.device.v1` — which of P1…P4 (§12).** It decides
   whether §8's stale-client criterion is "re-lookup after restart" or the
   narrower "the capability already held behaves thus", and whether `docs/11`
   step 4 is in scope at all.

4. **Confirmation that this slice does not close Stage 4 (§10).** The identity
   exit advances; persistent object/state storage and the capsule-to-repository
   handoff remain, and so does the performance report. If the intent is instead
   that Stage 4 closes here, that is a different and much larger slice and this
   note does not describe it.

Not on this list, deliberately: the IPC message shape (§2 — inside `IPC_V1`),
the `block.device.v1` operation set (§3 — inside whatever §12 decides), the
evidence document titles (§9), and anything about performance (§10 — the
contract already exists and applies once there is a client).
