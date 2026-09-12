<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4D-3 — one split virtqueue, reused

One canonical textual module performs **two sequential `VIRTIO_BLK_T_IN`
requests through one initialized split virtqueue**, reclaiming and reusing the
descriptors of the first request to serve the second.

```text
init once ->  request 1  3 of 4 descriptors, head 0 -> avail.idx 0->1 -> notify
                         -> irq -> consume -> used.idx 0->1 -> reclaim
          ->  request 2  3 of 4 descriptors, head 3 -> avail.idx 1->2 -> notify
                         -> irq -> consume -> used.idx 1->2 -> reclaim
```

## 0. The three claims, kept apart

```text
the device accepted the queue substrate          Stage 4D-1
the device performed DMA through it, once        Stage 4D-2
the queue survives a request and serves another  claimed here
requests in flight together, writes, a service   none of them, and not designed
```

## 1. What Stage 4D-2 actually was, audited before anything was written

The Stage 4D-2 driver is a one-shot by construction, and the audit that opened
this slice established exactly where:

| Question | Stage 4D-2 |
|---|---|
| how are descriptors allocated and owned? | they are not — descriptors 0, 1 and 2 are written at three fixed offsets, `next` is the literal 1 then 2 then 0, and the chain head is `const CHAIN_HEAD = 0` |
| where do `avail.idx` and the observed `used.idx` live, and who owns them? | only in the DMA region. `avail.idx` is written as the literal `1`; the observed `used.idx` is compared against the literal `1` in a local that dies with the call. There is no producer or consumer counter |
| does it assume a permanently one-shot chain? | yes, in five independent places: the literal `avail.idx`, the available ring slot fixed at 0, the used entry read from slot 0, the constant chain head, and the absence of any reclaim |
| what must persist between requests? | the descriptor pool and its per-descriptor state, the producer index, the consumer index, the queue size, and the region and window bindings |
| does any helper embed a one-shot assumption? | `read_sector_zero` does — it is one request, one sector, one chain, called once as `drive`'s tail. The generic helpers (`align_up`, `scale`, `put_le*`, `read_le*`, `add_status`, `negotiate`, `usable_queue`, `window_offset`, `multiplier_admitted`) carry none |
| what is the smallest coherent change? | keep initialization byte for byte, add a descriptor pool with an explicit lifecycle, add the two 16-bit ring counters, loop the request, and give the backing image sectors that can be told apart |

**No accepted contract stood in the way.** No ABI operation, interface, Core
minor, authority or nucleus mechanism was needed: descriptor recycling is
entirely a driver-private matter under §2.7.13.1, and `dma_publish` /
`dma_consume` are whole-region operations (ADR-0086 §8) that compose over any
number of requests without a range parameter.

## 2. Clauses audited

Read from the OASIS document, not from memory:

> Virtual I/O Device (VIRTIO) Version 1.4, Committee Specification 01,
> 8 April 2026, OASIS, docs.oasis-open.org/virtio/virtio/v1.4/cs01/

| Clause | What it fixed here |
|---|---|
| **§2.7.6** | `virtq_avail { flags; idx; ring[] }` — the ring is the queue size, the index is not |
| **§2.7.6.1** | "A driver MUST NOT decrement the available idx on a virtqueue" |
| **§2.7.8** | `virtq_used`, `virtq_used_elem { le32 id; le32 len; }` |
| **§2.7.8.2** | "The device MUST write at least len bytes to descriptor, beginning at the first device-writable buffer, prior to updating the used idx" |
| **§2.7.8.3** | "The driver MUST NOT make assumptions about data in device-writable buffers beyond the first len bytes" |
| **§2.7.9** | `VIRTIO_F_IN_ORDER` — **not negotiated**, so no used-order assumption is made |
| **§2.7.13** | the seven steps, including the two barriers at steps 4 and 6 |
| **§2.7.13.1** | "Get the next free descriptor table entry, d"; `d.next` "is usually used to chain free descriptors, and a separate count kept" — **the protocol mandates no recycling mechanism**, so the one below is this driver's own |
| **§2.7.13.3** | `avail->ring[avail->idx % qsz] = head;` and **"idx always increments, and wraps naturally at 65536"** |
| **§2.7.13.3.1** | "The driver MUST perform a suitable memory barrier before the idx update" |
| **§2.7.14** | `if (vq->last_seen_used != le16_to_cpu(virtq->used.idx)) { … virtq.used->ring[vq->last_seen_used % vsz] … }` — the consumer index is the driver's and the device never writes it |
| **§2.7.10.1** | "The driver MUST initialize flags in the used ring to 0"; "The device MUST handle spurious notifications from the driver" |
| **§2.7.7.1** | "The driver MUST handle spurious notifications from the device" |
| **§2.1.1** | "The driver MUST NOT clear a device status bit" |
| **§3.1.1** | the initialization order — performed **once** for the whole boot |
| **§4.1.4.3**, **§4.1.4.4**, **§4.1.4.4.1** | the common configuration, the notification location and its two branches |
| **§4.1.5.1.2** | MSI-X vector configuration and readback |
| **§5.2.6** | `virtio_blk_req`; `VIRTIO_BLK_T_IN` = 0; `VIRTIO_BLK_S_OK` = 0 |

With ADR-0082 (a wake is a delivery fact and carries no memory meaning),
ADR-0084 (the two authorities, bounded offsets, no address exposure) and
ADR-0086 (the two publication points, and consume before reading the device's
writes).

## 3. The descriptor pool, and why it is four

**Four descriptors of the 128-entry table, three spent per request.** The pool
is deliberately smaller than two chains, so the second request is
*arithmetically impossible* unless the first one's descriptors came back. The
claim of this slice is enforced by the pool's size rather than asserted in a
comment, and a run that had quietly consumed a larger preallocated table could
not have completed at all.

The free list is a **circular FIFO** rather than a stack, and that choice is
what makes the second chain a different chain:

```text
                      free queue            chain          head
start                 [0 1 2 3] front 0
request 1  allocate   [3]       front 3     0 -> 1 -> 2    0
           reclaim    [3 0 1 2] front 3
request 2  allocate   [2]       front 2     3 -> 0 -> 1    3
```

A stack would have returned `2 -> 1 -> 0` and left descriptor 3 forever unused;
the queue moves the head to a descriptor the first request never touched and
keeps two the first request did. **Two of three descriptors are reused, and the
chain head is not the same descriptor.**

### The lifecycle

```text
FREE ---allocate---> PREPARED ---avail.idx---> DEVICE ---used entry---> COMPLETED
  ^                                                                          |
  +--------------------------- reclaim --------------------------------------+
```

The state lives in a driver-private `array<u64, 4>`, **not** in the descriptor
table's `next` fields: bookkeeping the device can see is bookkeeping the device
can be blamed for. Three checks make the ownership mechanical rather than
documentary:

- a descriptor is handed out **only** from `FREE`, and the free queue only ever
  receives descriptors the reclaim step released — so **there is no path from
  `DEVICE` back into a chain**;
- nothing is published unless every member of the chain is `PREPARED`;
- nothing is reclaimed unless every member is `DEVICE`, and the pool must be
  whole again afterwards.

The pool is also checked for consistency before the first request: every
descriptor `FREE`, and every descriptor named exactly once by the free queue. A
pool that started with a duplicate would hand one descriptor to two chains and
the reuse would be an accident rather than a discipline.

**No general-purpose allocator was built.** There is no size class, no
coalescing, no arbitrary chain length and no framework: four slots, a FIFO, and
four states.

## 4. The ring counters, and the arithmetic that is not exercised

Two counters survive every request:

```text
avail_idx        this driver's copy of the producer index it publishes
last_seen_used   §2.7.14's consumer index, which the device never writes
```

Every index in the boot is **computed**:

| Stage 4D-2 | Stage 4D-3 |
|---|---|
| `avail.ring[0] = 0` | `avail.ring[avail_idx % queue_size] = chain head` |
| `avail.idx = 1` | `avail_idx = ring_next(avail_idx)`, then published |
| `used.idx == 1` | `ring_distance(published, last_seen_used) == 1` |
| `used.ring[0]` | `used.ring[last_seen_used % queue_size]` |
| head is `const 0` | head is whatever the pool handed out |

Two moduli are in play and they are different: the counters wrap at **65536**
(§2.7.13.3) and the ring arrays are indexed **modulo the queue size**. Using one
where the other belongs is invisible for thousands of requests, which is exactly
why it is not left to a run to discover.

**The wrap is proved rather than reached.** This stage issues two requests, so
no boot of it can arrive at 65536, and building a 65,536-request boot to force
one would be a worse test than checking the primitive across the boundary
directly. The module therefore validates, before it touches the device:

```text
ring_next(65535)          == 0
ring_next(0)              == 1
ring_next(65534)          == 65535
ring_distance(0, 65535)   == 1
ring_distance(3, 65533)   == 6
ring_distance(5, 5)       == 0
ring_distance(65535, 0)   == 65535
ring_slot(65535, 128)     == 127
ring_slot(ring_next(65535), 128) == 0
```

and refuses the boot with `-20` if any of them does not hold. The last two are
the two moduli meeting: at the last index a 128-entry ring is at slot 127, and
at the next index it is at slot 0.

## 5. The backing image, and why the sentinel alone was not enough

Stage 4D-2's witness is that a 0xA5 sentinel was replaced by sector 0's zeros.
Repeating that twice would prove two DMA writes but **not two different device
operations**: a second read satisfied by the first read's bytes looks exactly
like a second read.

So the Stage 4 reference profile now seeds the backing image:

```text
sector 0            zero-filled, untouched — Stage 4D-2's evidence depends on it
sectors 1 .. 4      512 copies of the byte 0xC0 + sector
everything else     zero
```

The rule is the profile's contract with the fixtures, and the module computes
the byte it expects from the sector it asked for rather than carrying a table of
answers. Both requests read into the **same** 512-byte buffer, poisoned with a
**different** sentinel before each:

```text
request 1   poison 0xA5   read sector 1   every byte must be 0xC1
request 2   poison 0x5A   read sector 2   every byte must be 0xC2
```

Neither untouched memory (0x00), nor the poison (0xA5 / 0x5A), nor the previous
request's result (0xC1) can satisfy request 2's witness. All 512 bytes are
checked, not a prefix.

## 6. The layout inside the one 4 KiB region

Queue size the driver settled on, reported by the boot: **128** descriptors.
Identical to Stage 4D-2's, and derived the same way — by the module's own
`align_up` and counting `scale`, not copied from the earlier evidence:

```text
desc       0 ..  2048   2048 bytes   align 16     16 * 128
avail   2048 ..  2310    262 bytes   align  2      6 + 2 * 128
used    2312 ..  3342   1030 bytes   align  4      6 + 8 * 128
                                                   queue end 3342

header  3344 ..  3360     16 bytes   align 16     virtio_blk_req head
data    3360 ..  3872    512 bytes   align 16     device-writable
status  3872 ..  3873      1 byte                 device-writable

request end 3873 <= 4096
```

**Six device addresses, six bounded offsets, one region — resolved once and used
by both requests.** The second request adds no allocation and no address
arithmetic: it rewrites the header's sector field and the sentinel, and it points
different descriptors at the same three addresses. The boot journals exactly one
`TOS.RUN.DMA_REGION` with `capability_delta=1`.

## 7. Runtime evidence

`TOS.RUN.COMPLETED` carries a thirty-bit proof mask plus six measurements. The
boot returns:

```text
TOS.RUN.COMPLETED value=i64:565994011492351

  proof mask   1073741823   the complete set; no partial run can produce it
  avail.idx    2            the producer index after both requests
  consumer     2            used entries consumed
  head 1       0            the first chain's head descriptor
  head 2       3            the second chain's head descriptor
  reused       2            of 3 — descriptors shared with the first chain
  queue size   128
```

| Fact | Bit |
|---|---|
| reset observed, `VERSION_1` offered, `FEATURES_OK` accepted | 1, 2, 4 |
| queue 0 exists; size accepted; layout fits; addresses aligned | 8, 16, 32, 64 |
| `queue_desc`, `queue_driver`, `queue_device` read back | 128, 256, 512 |
| MSI-X entry accepted; `queue_enable` 1; `DRIVER_OK` set | 1024, 2048, 4096 |
| the request storage fits the same region | 8192 |
| the notification capability was found and validated | 16384 |
| **the 16-bit wrap arithmetic holds across 65535 → 0** | 32768 |
| **the pool started whole: every descriptor free, named once** | 65536 |
| *for every request* — a chain came out of the pool, each member `FREE` when handed out | 131072 |
| *for every request* — **exactly one** new used entry appeared, by wrapping distance | 262144 |
| *for every request* — `used.id` is **this request's** chain head | 524288 |
| *for every request* — `used.len` covers the data and the status byte | 1048576 |
| *for every request* — status = `VIRTIO_BLK_S_OK` | 2097152 |
| *for every request* — all 512 bytes are this sector's content, sentinel gone | 4194304 |
| *for every request* — the chain returned and the pool is whole again | 8388608 |
| the loop completed `REQUESTS` times | 16777216 |
| **`avail.idx` reached 2**, not 1 | 33554432 |
| **the consumer index reached 2**, not 1 | 67108864 |
| **at least one descriptor was reused** | 134217728 |
| **the second chain's head is a different descriptor** | 268435456 |
| **the two reads returned different bytes** | 536870912 |

The per-request bits are "for **every** request" rather than "for the last one"
because a request that failed any of them returned a negative naming it and
never reached the count.

### What the nucleus witnessed, which no module can write

```text
TOS.RUN.PCI_ASSIGNED  ... generation=1 express=1 dma=1     one assignment
TOS.RUN.DMA_REGION    ... bytes=4096 capability_delta=1    one region
TOS.RUN.IRQ_DELIVERED ... deliveries=1 woke=1 latched=0    request 1
TOS.RUN.INTERFACE operation=irq_wait status=0
TOS.RUN.IRQ_DELIVERED ... deliveries=2 woke=1 latched=0    request 2
TOS.RUN.INTERFACE operation=irq_wait status=0
TOS.RUN.ACCOUNTING fuel=60032/4194304 depth=7/16 allocation=432/4096
```

`TOS.RUN.IRQ_DELIVERED` is written by the interrupt handler that took the
message, and its counter is the nucleus's own. **Two real MSI-X messages arrived
from the device on one source**, and both woke a waiter rather than finding a
latch — so neither request was completed by a wake the other had already caused.

## 8. Why a one-shot implementation run twice cannot pass

Each of these is independently sufficient to refuse it:

| A one-shot driver would show | The gate requires |
|---|---|
| `avail.idx` = 1, because a reinitialized queue starts over | 2 |
| consumer index = 1 | 2 |
| two `TOS.RUN.PCI_ASSIGNED`, or two `TOS.RUN.DMA_REGION` | exactly one of each |
| the same chain head twice | two different heads |
| no descriptor in common, if it used a bigger table | at least one reused |
| the same bytes from the same sector | two different sectors' content |

## 9. Negative evidence

**Against the host judgement.** The gate's acceptance is one function, and
before the boot runs it is fed six crafted completion values — the shape of a
correct result, a queue reinitialized between requests, a run that recycled
nothing, a second chain identical to the first, a run whose data did not match
the sector, and a module refusal. It requires the first to be accepted and every
other to be refused. This proves the assertions would *notice*; it proves
nothing about the device, and the gate says so where it is written.

**Against the module.** Four mutations were run through the real device and each
was refused at the step it broke, with the exact code the source names:

| Mutation | Result |
|---|---|
| both requests read sector 1 | `-70` — the two reads are not distinct |
| the expected content byte off by one | `-62` — the 512 bytes are not this sector's |
| the available ring slot hardcoded to 0, as Stage 4D-2 has it | the second request never reaches the device; the boot blocks and the harness times out |
| the used entry always read from slot 0 | `-58` — `used.id` is not this chain's head |

The mutations were reverted; they are recorded here rather than kept as
fixtures, because each of them is a driver that does not work rather than a
device that misbehaves.

## 10. Stability

Five consecutive runs of the gate, each a fresh boot:

```text
run 1..5   PASS   value=565994011492351   deliveries 1,2   fuel=60032
```

Identical every time — the same proof mask, the same two chain heads, the same
reuse count, the same interrupt count. Nothing in the assertions depends on
timing: completion is observed through `used.idx` after a wake and a
`dma_consume`, never inferred from elapsed time, and a wake that did not advance
the index is waited past (bounded at sixteen per request).

## 11. Architectural invariants

**The nucleus learned nothing.** No ABI operation, no interface version, no
Core minor, no authority, no new mechanism and no new endowment. The endowment
is `test-dma-driver`, unchanged. A descriptor, a chain, a free list, a ring
index and a sector are all words that occur only above the boundary.

**The host implements none of the behaviour.** `run.sh` seeds the backing image
— which is the disk's content, not the driver's conduct — and the gate reads
events and asserts. Queue management, descriptor recycling, index arithmetic and
completion detection are in `tests/vectors/virtio-block-reuse/init.tos` and
nowhere else.

**Two language findings were made, recorded, and worked around rather than
papered over.** Neither is a Stage 4D-3 blocker and no contract was bent around
either:

- TOS Core V1 aggregates are `Copy`, and the reference engine passes a
  `borrow mut` parameter of a `Copy` type **by copy** — the callee's writes are
  lost silently, with no diagnostic and no trap. It is reproducible for a bare
  `u64` as well as for an `array<u64, N>`, and it has not been hit before
  because every `borrow mut` in the tree today is a region or a capability
  handle, which are not `Copy`. The pool and the ring counters therefore live in
  one function's scope — which is both what the language supports today and the
  smallest representation.
- The parser requires an integer **literal** as an array's length
  (`E1104_EXPECTED_LITERAL` on `array<u64, POOL_SLOTS>`), while `docs/40` §3
  states that a named constant is admissible as `array<T, N>`'s compile-time
  `size`. The fixture writes the literal and says in its own text why.

> **Addendum, 2026-09-12, after this stage closed.** Both findings were repaired
> in a separate language-correctness change. Nothing above is amended: the
> workarounds in `tests/vectors/virtio-block-reuse/init.tos` are the state of
> the implementation at Stage 4D-3's closure and stay as they were written, and
> the real-device witness recorded here was re-run against the repaired frontend
> and engine and is byte for byte the same — the same completion value, the same
> fuel, the same two interrupts, and Stage 4D-2's module digest unchanged at
> `sha256:21101a2d…bcbcb3`. The repair's own evidence is the progress-log
> entry of that date; its conformance sets are
> `source/tests/integration/tests/mutable_borrow.rs` and
> `source/tests/integration/tests/array_length_constants.rs`.

## 12. Stage 4 performance accounting

```text
initialization (once)   claim, walk, map, negotiate, one dma_region_allocate,
                        zero the region, write the ring, program common config,
                        claim one MSI-X source, enable, DRIVER_OK, resolve six
                        device addresses, check the pool and the wrap arithmetic

per request             allocate 3 descriptors from the pool, write the header,
                        512 sentinel bytes and 3 descriptors, one ring entry,
                        publish, one avail.idx store, publish, one MMIO notify,
                        irq_wait, dma_consume, read used id/len, status and 512
                        bytes, reclaim 3 descriptors
```

**The second request allocates nothing.** One DMA region for the whole boot, no
growth of the capability table (`capability_delta=1`), and the only per-request
storage is the descriptor pool's four slots, which are reused rather than
extended. Measured fuel for both requests together: **60032** of 4194304, against
Stage 4D-2's 42737 for one — so the marginal cost of the second request is
roughly 17k units of the same work, not a second initialization.

Ring-0 crossings per request, against ADR-0082 §11's stated budget of two per
unbatched request: the descriptor and ring writes are ordinary stores, the notify
is one mapped store, `irq_wait` is one crossing in and the device's wake is one
out. `dma_publish` and `dma_consume` cost **no** crossing — they are compiler and
execution barriers in ring 3 (ADR-0086 §11).

## 13. What this is not

- **not** multiple requests in flight — the loop is submit, wait, consume,
  reclaim, submit, and deliberately so;
- **not** a write of any kind: no `VIRTIO_BLK_T_OUT`, no flush, no `GET_ID`, and
  the backing image is never modified by the guest;
- **not** more than one queue, an indirect descriptor, an event index, a packed
  virtqueue, a queue reset or a queue resize — none of those features is
  negotiated;
- **not** a block service, a filesystem, a cache, a scheduler or a client-facing
  interface, none of which is designed;
- **not** a generic device-driver abstraction: there is one driver, for one
  device, in one module;
- **not** a steady-state throughput or latency claim. Two requests measure the
  marginal cost of the second, and nothing else.

## 14. Reproduction

```sh
bash source/host-tools/qemu-test/virtio-block-reuse.sh
./scripts/preflight.sh --profile qemu
```
