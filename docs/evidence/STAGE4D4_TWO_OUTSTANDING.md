<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4D-4 — two requests outstanding together

One canonical textual module exposes **two disjoint `VIRTIO_BLK_T_IN` request
chains to one already-initialized split virtqueue with a single `avail.idx`
publication**, and associates, validates and reclaims both completions in
whichever order the device produces them.

```text
build A   3 of 6 descriptors, head 0, own header/buffer/status, sector 1
build B   3 of 6 descriptors, head 3, own header/buffer/status, sector 2
          pool free count 0, all six PREPARED, consumer index 0
          avail.ring[0] = 0, avail.ring[1] = 3
          dma_publish -> avail.idx 0 -> 2 (ONE store) -> dma_publish -> notify
drain     irq_wait -> dma_consume -> two used entries visible at once
          each id matched against the outstanding heads, each chain reclaimed
```

## 0. The four claims, kept apart

```text
the device accepted the queue substrate            Stage 4D-1
the device performed DMA through it, once          Stage 4D-2
the queue survives a request and serves another    Stage 4D-3
two chains are outstanding together                claimed here
writes, a second queue, a service, a scheduler     none of them, and not designed
```

## 1. What Stage 4D-3 was, and what it deliberately was not

Stage 4D-3's loop is submit, wait, consume, reclaim, submit, and its evidence
says so: *"not multiple requests in flight — the loop is submit, wait, consume,
reclaim, submit, and deliberately so."* Its pool is four descriptors and three
are spent per request, so the second request was **impossible** without the
first one's descriptors coming back. At no instant did the device hold more than
one of that driver's chains.

That is the property this stage changes, and it changes nothing else. The
device, the initialization, the feature negotiation, the queue, the
`queue_enable`, the MSI-X source, the ring geometry, the 16-bit counter
arithmetic and the seeded-sector contract are Stage 4D-3's.

## 2. Clauses audited

Read from the OASIS document during this slice, not from the previous
evidence's quotation of it:

> Virtual I/O Device (VIRTIO) Version 1.4, Committee Specification 01,
> 8 April 2026, OASIS, docs.oasis-open.org/virtio/virtio/v1.4/cs01/

| Clause | What it fixed here |
|---|---|
| **§2.6** | "Device is **not generally required to use buffers in the same order** in which they have been made available by the driver" |
| **§2.7.5.1** | "A device MUST NOT write to a device-readable buffer"; "A device MUST NOT write to any descriptor table entry" |
| **§2.7.5.2** | "Drivers MUST NOT add a descriptor chain longer than 2³² bytes in total; this implies that loops in the descriptor chain are forbidden!" |
| **§2.7.6.1** | "A driver MUST NOT decrement the available idx on a virtqueue" |
| **§2.7.8** | "id indicates the head entry of the descriptor chain"; "len the total of bytes written into the buffer" |
| **§2.7.8.2** | "The device MUST set len prior to updating the used idx"; "The device MUST write at least len bytes to descriptor, beginning at the first device-writable buffer, prior to updating the used idx" |
| **§2.7.8.3** | "The driver MUST NOT make assumptions about data in device-writable buffers beyond the first len bytes" |
| **§2.7.9** | `VIRTIO_F_IN_ORDER` — **not negotiated**, so no used-order assumption is made |
| **§2.7.13** | The seven steps. **Step 3: "Steps 1 and 2 MAY be performed repeatedly if batching is possible."** Step 5: "The available idx is increased by the **number of descriptor chain heads added** to the available ring." The barriers at steps 4 and 6 |
| **§2.7.13.2** | "in general the driver **MAY add many descriptor chains before it updates idx** (at which point they become visible to the device)", with `avail->ring[(avail->idx + added++) % qsz] = head;` |
| **§2.7.13.3** | `avail->idx += added;` — "Once available idx is updated by the driver, this exposes the descriptor and its contents. The device **MAY access the descriptor chains the driver created and the memory they refer to immediately**" |
| **§2.7.13.3.1** | "The driver MUST perform a suitable memory barrier before the idx update" |
| **§2.7.14** | The consumer index is the driver's; the device never writes it |
| **§2.7.7.1** | "The driver MUST handle spurious notifications from the device" |
| **§2.7.10.1** | "The driver MUST initialize flags in the used ring to 0 when allocating the used ring" |
| **§5.2.6** | `struct virtio_blk_req { le32 type; le32 reserved; le64 sector; u8 data[]; u8 status; }`; `VIRTIO_BLK_T_IN 0`; `VIRTIO_BLK_S_OK 0`. **"The driver enqueues requests to the virtqueues, and they are used by the device (not necessarily in order)"** |
| **§5.2.6.1** | "The length of data MUST be a multiple of 512 bytes for VIRTIO_BLK_T_IN and VIRTIO_BLK_T_OUT requests" |

With ADR-0082 (a wake is a delivery fact; the pending latch; one wake may cover
a batch), ADR-0084 (bounded offsets, no address exposure) and ADR-0086 (the two
publication points, and consume before reading the device's writes).

**§5.2.6 is the clause that makes this stage's completion handling mandatory
rather than defensive.** It is the block device's own statement that requests
are used "not necessarily in order" — stronger for this purpose than §2.6's
general sentence, because it is about exactly the requests this driver issues.

## 3. The single publication, and why two stores would not do

```text
avail.ring[0] = head A
avail.ring[1] = head B
dma_publish                       <- §2.7.13 step 4
avail.idx = 0 + 2                 <- §2.7.13 step 5, ONE store, added = 2
dma_publish                       <- §2.7.13 step 6
notify                            <- §2.7.13 step 7
```

Two stores of `+1` would be a different program **even with nothing between
them**. §2.7.13.3 says that once `avail.idx` is updated "the device MAY access
the descriptor chains the driver created and the memory they refer to
immediately", so after a first `+1` the device may legally complete request A
before the second store lands — and such a run would have proved only what
Stage 4D-3 already proved. One store from `i` to `i + 2` is §2.7.13.2's own
batching form, and §2.7.13 step 3 is the clause that permits it.

### 3a. What proves the shape, and what does not

**No observation a driver can make about itself distinguishes one store of `+2`
from two stores of `+1`.** Both arrive at the same counter; a program that did
the second immediately after the first would satisfy every runtime bit this
module reports. The evidence is therefore split, and the split is written into
the source beside the check rather than only here:

- **the runtime bits prove the precondition** — both chains allocated and
  disjoint, the pool empty, all six descriptors `PREPARED`, the consumer index
  still 0, and all six device-owned immediately afterwards;
- **a static check on the vector text proves the shape** — exactly one store
  into `avail.idx`, exactly one use of the `+= added` helper, and zero uses of
  the `+1` helper on the producer index.

The bit the module reports for the transition is named
`publication_added_two`, not "single publication", because that is what it
proves: the producer index moved by exactly the number of chains that were
exposed.

## 4. The pool is exactly two chains

**Six descriptors of the 128-entry table, three per chain.** After both
allocations the free count is **zero**: neither chain can have been reclaimed to
make room for the other, and a third chain is arithmetically impossible. Stage
4D-3's pool was four — one chain short of two — which is precisely what forced
its requests to be sequential.

```text
free queue           chain A        chain B        free after
[0 1 2 3 4 5]        0 -> 1 -> 2    3 -> 4 -> 5    []
```

Chain B's head is the descriptor after chain A's last member, which is what the
initial FIFO order gives it — **and that relation proves nothing about
concurrency**. A driver that allocated A, reclaimed it and then allocated B
would hand B descriptors 3, 4 and 5 as well, because A's three go back behind
the three still ahead of the front:

```text
start       [0 1 2 3 4 5]  front 0
A alloc     [3 4 5]        front 3
A reclaim   [3 4 5 0 1 2]  front 3
B alloc     [0 1 2]        front 0     — B still receives 3, 4, 5
```

The gate checks the relation as a **determinism check on the allocator** and
says so where it is written. What proves the two chains were outstanding
together is §4a.

### 4a. What actually proves the two chains were outstanding together

Not the heads, not the interrupt count, and not any single bit. It is the
conjunction of these facts, every one of them established **immediately before**
the single producer publication and none of them reachable by a driver that
served the two requests one after the other:

| Fact | Established by |
|---|---|
| request A's three descriptors are `PREPARED` | the readiness loop, over both chains |
| request B's three descriptors are `PREPARED` | the same loop, same iteration |
| all six descriptors are distinct | every member of B compared with every member of A (`-80`) |
| the free descriptor count is **zero** | `free_held != 0` refuses (`-81`), **checked at the publication and not at the allocation** — see below |
| no completion has been consumed | `last_seen_used != 0` refuses (`-83`) |
| `last_seen_used` is still its pre-publication value | the same check; the value is the one the boot started with |
| both available-ring slots already hold the two heads | the two `put_le16` stores precede the readiness loop, and the two slots are asserted distinct (`-82`) |
| one `dma_publish(region)` precedes the publication | source order — §2.7.13 step 4 |
| **exactly one** source-level store advances `avail.idx` by two | a static check on the vector's text (§3a) |
| all six descriptors are `DEVICE` **after** that store | the ownership count is asserted after the publication (`-86`) |
| no `irq_wait`, `dma_consume` or reclaim happens before that publication | source order — the drain loop is entirely after it |

**A note on where the zero-free check is written, and why it moved.** It was
first written immediately after the two allocations, which is the natural place
and the wrong one: descriptors can be returned to a pool between the allocation
and the publication. The serialized mutation in §10 did exactly that and was
caught four steps later by the final pool-whole check (`-64`) instead of by the
invariant that is supposed to guard the moment. The check now sits with the
other two, immediately before the publication, and the same mutation is refused
`-81`. The instruction count did not change: the same boot value and the same
`fuel=84779`.

**A note on where the `DEVICE` marking is written.** The bookkeeping transition
is made in one loop over both chains immediately *before* the store rather than
after it, which is the conservative direction: the driver stops treating the
descriptors as its own a moment early rather than a moment late, and nothing
runs in between. What is asserted **after** the store is that all six are
device-owned — so the fact the evidence rests on is established at the point the
ownership boundary is actually crossed.

That conjunction is the claim. Everything else in this document is either a
measurement or a guard against a different failure.

**The queue floor is the pool.** Stage 4D-3's floor of four descriptors is not
sufficient for a driver that holds two chains, so a device offering fewer than
six is refused before any descriptor index exists. The reference device offers
128 and this driver does not rely on that.

## 5. Completion order is not assumed

Identity is `used_elem.id`, matched against the two outstanding heads. The ring
slot a completion arrives in says nothing about which request it is, and a used
`id` must name **exactly one** outstanding head that has **not** already
completed — an unknown head is `-58`, a duplicate is `-88`.

Each completion selects its own status byte, its own buffer and its own expected
sector content from the head that completed. Reclaim releases exactly that
request's three descriptors, and while the other request is still outstanding
its three are checked to be still device-owned (`-89`).

**Both orders pass.** The gate's judgement accepts `A then B` and `B then A`,
and the order observed is reported as a measurement. On the reference profile
QEMU completed A first in every run, so the B-first branch of the driver is
**not hardware-witnessed** — it is covered by construction, by the symmetry of
the association code, and by the judgement's acceptance of the reversed order.
That is stated rather than glossed.

## 6. Two independent witnesses

```text
request A   sector 1   buffer A poisoned 0xA5   every byte must be 0xC1
request B   sector 2   buffer B poisoned 0x5A   every byte must be 0xC2
status      both poisoned 0xFF, because VIRTIO_BLK_S_OK is 0 and so is untouched memory
```

The two requests are outstanding together, so they cannot share a buffer at all
— which is a stronger separation than Stage 4D-3's, where one buffer was
re-poisoned between requests. Neither untouched memory (0x00), nor either poison,
nor the other request's result can satisfy either witness. All 512 bytes are
checked, not a prefix, and which buffer is checked against which sector is
chosen by the head that completed rather than by the order of completion.

## 7. The geometry, in one 8 KiB region

Two request blocks do not fit 4 KiB: the ring ends at 3342, two blocks need
1075 bytes, and 754 remain. So the region is 8 KiB — **one** region, one
`dma_region_allocate`, one capability-table entry, funded from the same
authority with the same lineage and the same drain semantics.

```text
desc        0 ..  2048   2048 bytes   align 16     16 * 128
avail    2048 ..  2310    262 bytes   align  2      6 + 2 * 128
used     2312 ..  3342   1030 bytes   align  4      6 + 8 * 128
                                                    queue end 3342

header A 3344 ..  3360     16 bytes   align 16     virtio_blk_req head
data A   3360 ..  3872    512 bytes   align 16     device-writable
status A 3872 ..  3873      1 byte                 device-writable
header B 3888 ..  3904     16 bytes   align 16
data B   3904 ..  4416    512 bytes   align 16     device-writable
status B 4416 ..  4417      1 byte                 device-writable

request end 4417 <= 8192
```

**The queue substrate and request A are byte for byte where Stage 4D-3 put
them**, and request B is appended under the same alignment rules. Every offset
is computed by the module's own `align_up` and `scale`; the disjointness of the
two blocks is checked rather than laid out and trusted.

**Nine device addresses, nine bounded offsets, one region** (ADR-0084 §6b), and
the boot's nine `dma_device_address` calls are in its own event log. Nothing
adds to an address and no scalar address is supplied by the source.

### 7a. `resource allocation` is unchanged, and that was checked

The declaration stays at `allocation: 4KiB` for an 8 KiB region, because they
are not the same account. `docs/41` §6 makes `allocation` the module's maximum
live allocatable bytes, and the engine charges it only where a value is
constructed. A DMA region is funded from the `system.memory.Authority`
capability: the nucleus charges `charge_grant` against that authority, not
against the Core envelope.

Measured, which is what settles it: this boot holds an 8192-byte region and
reports `allocation=576/4096`. If the region were charged to the Core envelope,
the peak could not be 576.

## 8. Runtime evidence

```text
TOS.RUN.COMPLETED value=i64:144688603790835711

  proof mask   1073741823   the complete set; no partial run can produce it
  avail.idx    2            the producer index after ONE publication
  consumer     2            used entries consumed, one at a time
  head A       0            chain A's head descriptor
  head B       3            chain B's head — the one after A's last member
  order        0            A completed first (a measurement, not a gate)
  wakes        1            irq_wait returns (a measurement)
  max fresh    2            used entries one consume made visible (a measurement)
  queue size   128
```

| Fact | Bit |
|---|---|
| reset observed, `VERSION_1` offered, `FEATURES_OK` accepted | 1, 2, 4 |
| queue 0 exists; size accepted; layout fits; addresses aligned | 8, 16, 32, 64 |
| `queue_desc`, `queue_driver`, `queue_device` read back | 128, 256, 512 |
| MSI-X entry accepted; `queue_enable` 1; `DRIVER_OK` set | 1024, 2048, 4096 |
| both request blocks fit the same region | 8192 |
| the notification capability was found and validated | 16384 |
| the 16-bit wrap arithmetic holds, **including `+= 2` across 65535 → 0** | 32768 |
| the pool started whole: every descriptor free, named once | 65536 |
| **the queue holds two chains** — fewer than six descriptors is refused | 131072 |
| **the two request blocks are disjoint** | 262144 |
| **both chains allocated, six distinct descriptors, no member shared** | 524288 |
| **the pool is empty after both** — neither was reclaimed for the other | 1048576 |
| **all six are `PREPARED`** at the moment of publication | 2097152 |
| **nothing had been consumed** — the consumer index is still 0 | 4194304 |
| **the producer index moved by exactly two chains' worth** | 8388608 |
| **all six are device-owned** immediately after the publication | 16777216 |
| *for every completion* — `used.id` named exactly one outstanding, uncompleted head | 33554432 |
| *for every completion* — `used.len` covers that request's data and status | 67108864 |
| *for every completion* — status = `VIRTIO_BLK_S_OK` | 134217728 |
| **both buffers hold their own sector's 512 bytes**, and they differ | 268435456 |
| *for every completion* — exactly that chain's three descriptors came back | 536870912 |

### What the nucleus witnessed, which no module can write

```text
TOS.RUN.PCI_ASSIGNED  ... generation=1 express=1 dma=1          one assignment
TOS.RUN.DMA_REGION    ... bytes=8192 contiguous=1 capability_delta=1
TOS.RUN.IRQ_DELIVERED ... deliveries=1 woke=1 latched=0
TOS.RUN.INTERFACE operation=irq_wait status=0
TOS.RUN.INTERFACE operation=dma_device_address status=0          x9
TOS.RUN.ACCOUNTING fuel=84779/4194304 depth=7/16 allocation=576/4096
```

**One wake exposed two completed used-ring entries.** The nucleus counted one
delivery, the module made one `irq_wait` return, and one `dma_consume` made
**two** used entries visible — exercising ADR-0082 §10's batching property,
"one wake may cover any number of completions, and a driver that drains until
empty needs no further wakeups", for the first time in this project. Until this
stage every wake covered exactly one completion.

**What that corroborates is the driver's drain-until-empty path, and nothing
more.** It does not establish parallel device execution, and it is not offered
as evidence of it. A device that delivered two interrupts would be equally
conforming and this boot would be equally valid, which is why the count is a
measurement rather than a gate.

**The normative Stage 4D-4 concurrency claim stays where §4a puts it**: at the
driver/device ownership boundary created by the single `avail.idx += 2`
publication.

## 9. What this proves, exactly

It proves:

- queue depth greater than one **from the driver's side of the protocol
  boundary**;
- two chains simultaneously device-owned after a single ownership publication;
- order-independent completion association by `used_elem.id`;
- independent per-request storage, so no witness can be satisfied by the other
  request's bytes;
- per-chain reclamation, with the other chain untouched while it is outstanding;
- drain-until-empty semantics, with the consumer index advancing once per entry
  rather than once per wake.

**It does not prove that the device executed both requests at the same physical
instant**, and no wording here should be read as claiming it. What is proved is
that this driver never offered the device a state in which only one chain was
outstanding: both were exposed by one publication, and neither was consumed or
reclaimed until after it. "Parallel disk I/O" is not claimed and is not
observed.

## 10. Negative evidence

**Against the host judgement.** The gate's acceptance is one function, fed eight
crafted values before the boot runs: a correct A-then-B result and a correct
**B-then-A** result, both of which must be accepted; and Stage 4D-3's own
discipline, two `+1` publications, two chains sharing a head, a run whose data
did not match its sector, a queue too small for two chains, and a module
refusal — every one of which must be refused.

**Against the module, on the real device.** Four mutations, each reverted:

| Mutation | Result | Kind |
|---|---|---|
| every used entry read from ring slot 0, as Stage 4D-2 has it | `-88` — a second entry named a request already completed | hardware |
| reclaim the chain that did **not** complete | `-89` — the outstanding chain's descriptors are no longer device-owned | hardware |
| swap the two requests' expected sector content | `-62` — the 512 bytes are not this request's sector | hardware |
| two stores of `+1` rather than one of `+2` | refused **before the boot**: "the vector stores avail.idx 2 times; the claim is one publication" | static |
| **Stage 4D-3's discipline restored** — publish A, wait, consume, reclaim A, then publish B | refused **before the boot** by the same static shape check: the serialization needs a second `avail.idx` store and cannot have one | static |
| **the same serialization without a second store** — chain A given back to the pool before the single publication | `-81` — the free count is not zero at the publication | hardware |

**The serialized mutation is the sharp one, and it is refused twice over.** In
its faithful form — publish A, wait, consume, reclaim, then publish B — it needs
a second store into `avail.idx` and is refused by the static shape check before
the machine boots. Stripped of that, so that only the *reclaim* survives before
the single publication, it reaches the device and is refused `-81` by the
zero-free-count invariant at the publication point. **That invariant is the one
that catches it**, and finding out that it had been written four steps too early
is what the mutation was worth.

Which invariants are static and which are hardware-witnessed is stated rather
than blurred:

- **the publication shape** is static, for the reason in §3a — no runtime
  observation distinguishes one `+2` store from two `+1` stores, so the check is
  on the text;
- **everything in §4a's conjunction** is a runtime fact of the boot, and the
  zero-free count is the one a serialized driver cannot satisfy.

The mutations are recorded here rather than kept as fixtures, because each is a
driver that does not work rather than a device that misbehaves.

## 11. Stability

Five consecutive runs of the gate, each a fresh boot:

```text
run 1..5   PASS   value=144688603790835711   fuel=84779   deliveries=1
```

Identical every time — the same proof mask, the same two heads, the same
completion order, the same single interrupt, the same batch of two. Nothing in
the assertions depends on timing: completion is observed through `used.idx`
after a wake and a `dma_consume`, never inferred from elapsed time, and a wake
that made nothing visible is waited past within a bounded policy.

## 12. Architectural invariants

**The nucleus learned nothing.** No ABI operation, interface version, capability
type, Core minor, authority, endowment, `tos-ir/v1` field or `TOSBUNDLE`
version. The endowment is `test-dma-driver`, unchanged. A descriptor, a chain, a
request identity, a ring index and a sector are words that occur only above the
boundary. The one thing that grew is the byte count passed to an existing
operation.

**The host implements none of the behaviour.** `run.sh` seeds the backing image
— the disk's content, not the driver's conduct — and the gate reads events and
asserts. Chain construction, the single publication, completion association,
per-chain reclamation and the drain loop are in
`tests/vectors/virtio-block-two-inflight/init.tos` and nowhere else.

**Stage 4D-1, 4D-2 and 4D-3 are untouched**, source and module digest alike.

### 12a. A correction to the audit that opened this slice

The audit recorded that every Stage 4 fixture is `profile bootstrap`, and that
callable `PassMode` erasure was therefore structurally unreachable because
Bootstrap cannot construct a closure. **That was wrong**: every Stage 4 vector,
this one included, is `profile full`, where a closure is permitted.

The conclusion survives on a narrower and checkable basis. The erasure is
reachable only through a value of function type, the only producer of such a
value is a closure expression, and this vector contains none — which the gate
checks against the vector's text rather than assuming. A future Full-profile
driver that used a closure **would** reach the gap, and that is now a stated
risk rather than a dismissed one.

## 13. What this is not

- **not** a claim that the device serviced both requests simultaneously;
- **not** a write of any kind: no `VIRTIO_BLK_T_OUT`, no flush, no `GET_ID`, and
  the backing image is never modified by the guest;
- **not** more than one queue, an indirect descriptor, an event index, a packed
  virtqueue, a queue reset or a queue resize — none of those features is
  negotiated;
- **not** a request allocator or a general in-flight table: two fixed request
  records and a six-slot pool;
- **not** a block service, a filesystem, a cache, a scheduler or a client-facing
  interface, none of which is designed;
- **not** a throughput or latency claim. Two requests measure that two can be
  outstanding, and nothing else.

## 14. Reproduction

```sh
bash source/host-tools/qemu-test/virtio-block-two-inflight.sh
./scripts/preflight.sh --profile qemu
```
