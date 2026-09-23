<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Block Device Interface — `block.device.v1`

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0098 (Project Architect-approved, 2026-09-24), which fixes the
wire shape this contract states and answers ADR-0093 §9's reserved question.
ADR-0093 §0 fixes the interface surface it covers.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, and to
ADR-0093 and ADR-0095 where those decisions fix its subject matter.

**No implementation of this contract exists yet.** It is accepted as the shape the
Stage 4 block service and its clients must take; the conformance evidence §11
requires is outstanding, and `block-data-path` and `block-lifecycle` still carry
their own fixture encoding until a later slice migrates them (ADR-0098 §3).

## 1. Role

`docs/11` §Driver interfaces names `block.device.v1` as a device-class interface
whose purpose is that drivers publish it *"rather than exposing
hardware-specific details to applications"*. `ADR-0093` §0 fixes its surface —
`read`, `write`, `capacity` and nothing else — and its publication and lifetime.
`ADR-0093` §9 left the wire shape undecided. This contract is that shape.

**What it is not.** Not a filesystem, not a partition table, not a cache, not a
VFS, not an object store, not a request scheduler, not a batching protocol. One
sector per request, one request per call.

## 2. Topology

```text
client  --(block.device.v1 endpoint, `call`)-->  block service  -->  device
client  <--(one immutable Region<u8>)---------  block service
```

The client holds a `system.ipc.Endpoint` with `call` naming the service's request
endpoint, and — for `READ` — a second endpoint it holds `send` and `receive` on,
which it delegates with the request so the answer has somewhere to arrive. It
holds **no** PCI bus, function, MMIO window, interrupt source or DMA region: the
service is the only holder of hardware authority (`ADR-0079`, `ADR-0081`,
`ADR-0082`, `ADR-0084`).

## 3. Constants

| Name | Value | Meaning |
|---|---|---|
| `SECTOR_BYTES` | `512` | the bytes of one sector, and of one request's payload |
| `OPCODE_BITS` | `2` | the width of the opcode field in a request word |
| `OP_READ` | `0` | read one sector |
| `OP_WRITE` | `1` | write one sector |
| `OP_CAPACITY` | `2` | report the addressable sector count |
| `OP_RESERVED` | `3` | reserved; always refused |
| `MAX_SECTOR` | `4611686018427387903` = `2^62 - 1` | the largest sector a v1 request can name |
| `REQUEST_BYTES` | `8` | the inline length a well-formed request carries |
| `REFUSED` | `9223372036854775808` = `2^63` | the reply-word bit that marks a refusal |

All numeric values that appear on a wire are little-endian, as `ADR-0058` fixes
for the message payload. `docs/40`'s rule applies: no `size` value is ever
serialized; `sector`, the request word and the reply word are `u64`.

## 4. The request word

```text
word = sector * 4 + opcode           opcode = word % 4,  sector = word / 4
```

The opcode occupies the low `OPCODE_BITS` bits; the sector occupies the
remaining 62. **`MAX_SECTOR` is stated rather than implied**: a sector larger
than `2^62 - 1` has no v1 encoding, and a device that reports a larger capacity
has its excess sectors unreachable through this interface. That is a limitation
of v1, not an error, and a client learns the reachable bound from §6.

For `OP_CAPACITY` the sector field **must be zero**. A non-zero sector field on a
capacity request is malformed, because a request that carries a number nobody
reads is a request whose meaning two implementations could differ about.

**There is no in-band protocol-version field in the word**, and the reason is the
configured contract:

- `block.device.v1` is a **versioned Tier 2 service contract**;
- accepted publication, binding and launch topology supplies a client with an
  endpoint **implementing that contract**;
- v1 has **no runtime version negotiation**: there is nothing for a version tag to
  select between;
- so a per-request tag would be redundant with the service contract the topology
  already configured.

**It is not redundant with endpoint-object identity, and that must not be
claimed.** `ADR-0095` §3's dedicated object fixes a publication class and
authority; the endpoint later published as a service endpoint is a different
object, and one protocol has more than one of them — `block-lifecycle.sh` runs two
generations on two distinct service endpoints while both implement the same
`block.device.v1`.

## 5. The reply

Every reply is sent with `endpoint_reply_word`, so its inline length is
`REQUEST_BYTES` and its `word` is the answer. A client reads
`system.ipc.Answer{length, word}` and checks `length == REQUEST_BYTES` before
reading `word` at all.

```text
word < REFUSED     success. READ and WRITE answer 0. CAPACITY answers the
                   addressable sector count (§6).
word >= REFUSED    refusal. `word - REFUSED` is the refusal code of §7.
```

**One discriminator for all three operations**, which is why the bit is bit 63
rather than a separate field: a capacity count and a refusal code would otherwise
share a value space, and a device with three sectors would be indistinguishable
from refusal code 3.

## 6. The operations

### 6a. `READ`

```text
client:   endpoint_call_word_carrying(answer_endpoint, service, sector * 4 + OP_READ)
service:  endpoint_reply_word(reply, 0)
service:  endpoint_send_region(carried, one immutable region covering >= SECTOR_BYTES)
```

The service performs one `VIRTIO_BLK_T_IN` of the named sector, copies
`SECTOR_BYTES` out of device-visible memory into a region it allocated for the
sector's bytes, and freezes it — **completely, before it replies**. Then it
replies success. Then it sends the region to the endpoint the request delegated.

**Control before data, and the order is normative.** The reverse — region first,
reply second — admits an orphan response: the region is queued, the service dies,
the caller's call is cancelled by the liveness rule, and the region stays on the
answer endpoint. An endpoint object **lives for the boot**; a process dying
releases its receive authority and does not make the endpoint a fresh object or
drain what is queued on it. A later holder of receive — including a successor
created from the same launch plan — could take that old region as the answer to
its own request.

The three observations have distinct meanings:

| The client sees | It means |
|---|---|
| a **refusal** reply | refused; **no region follows** |
| a **success** reply | the read succeeded and **exactly one region is owed** |
| the **region** arriving | the `READ` is complete |

**A client may have at most one outstanding `READ` per answer endpoint**, and may
not issue another until the owed region has been received. Nothing enforces this
in the nucleus and nothing needs to: a client that broke it could not say which
region answered which of its own requests.

**If the service dies after the success reply and before the send**, the client
observes an incomplete operation through the ordinary liveness path — its receive
blocks and is cancelled with `E_CANCELLED` (`SYSTEM_ABI_V1` §6) — and **no stale
region has been queued**. If the region is sent, it belongs to the one
outstanding successful operation.

The region leaves the service linearly (`ADR-0075` §5a): after the send the
service holds neither the handle nor the mapping.

**A reply never carries the region.** `ipc::hand` copies payload bytes and
touches neither the transfer table nor the region area, which is why the request
has to say where to answer.

### 6b. `WRITE`

```text
client:   endpoint_call_word_region(service, region, sector * 4 + OP_WRITE)
service:  endpoint_reply_word(reply, 0)
```

**One message.** The request word and the payload region cross together, in the
payload area and the region area of the same message (`ADR-0058`). The service
reads `SECTOR_BYTES` out of the region, copies them into device-visible memory,
performs one `VIRTIO_BLK_T_OUT`, and replies **after** the device has completed
the request.

The two-message form — a region sent, then a call — is **not** this protocol.
It is correct only where one client is serialized against itself; with two
clients the region of one request and the word of another are indistinguishable
in one queue. `ADR-0098` §1 is the reasoning and §2a is the row that removes the
need.

### 6c. `CAPACITY`

```text
client:   endpoint_call_word(service, OP_CAPACITY)
service:  endpoint_reply_word(reply, sectors)
```

`sectors` is the device's own reported 64-bit capacity in 512-byte sectors, read
from the VirtIO configuration space under §2.5.1's generation protocol, **clamped
to `MAX_SECTOR + 1`** so that every value the client receives is a count of
sectors this protocol can name. It is never a constant compiled into the service.

A client uses it to establish that its layout fits **before** writing anything. A
client that instead discovered the bound by issuing a request it expected to be
refused would be learning a fact by misbehaving, and would be indistinguishable
from a client with a bug.

## 7. Refusals

| Code | Name | When |
|---|---|---|
| 1 | `BLK_OPCODE` | the opcode is `OP_RESERVED` |
| 2 | `BLK_RANGE` | the sector is at or above the device's reported capacity, or above `MAX_SECTOR` |
| 3 | `BLK_MALFORMED` | the inline length is not `REQUEST_BYTES`, or `OP_CAPACITY` carried a non-zero sector field |
| 4 | `BLK_NO_REGION` | `OP_WRITE` whose call carried no region |
| 5 | `BLK_NO_ANSWER` | `OP_READ` whose call carried no answer endpoint |
| 6 | `BLK_DEVICE` | the device answered with a status other than `VIRTIO_BLK_S_OK` |

**Every refusal is decided before the reply is sent, and that is why there is no
code for a failed region delivery.** §6a puts the success reply **before** the
region send, so a delivery that fails afterwards cannot become a refusal — the
reply is already spent. A service in that position records the fault in its
journal; the client sees an incomplete operation, not a refusal. `BLK_NO_ANSWER`
is the part that *is* checkable, and it is checked **before** the sector is read,
because the receive record reports an absent answer endpoint as absence rather
than as a handle.

**Every refusal leaves the device untouched**, with one stated exception that is
not a refusal of the request: `BLK_DEVICE` reports what the device did.

**A refusal is a reply, not a dropped call.** A service that failed to reply
would leave the caller blocked until the liveness rule cancelled it
(`SYSTEM_ABI_V1` §6), and a protocol whose error path is a cancellation gives a
client no way to tell a refusal from a dead service.

**Codes are small positive integers added to `REFUSED`,** so `word - REFUSED` is
the code and an unrecognized code is still recognizable as a refusal. A client
that does not know code 7 must still not read the reply as a success.

## 8. The region rule, and what cannot be checked

**A `WRITE` region and a `READ` region must cover at least `SECTOR_BYTES`, and
only the first `SECTOR_BYTES` are the sector's.** Bytes beyond that are neither
read nor written and carry no meaning.

**Not "exactly `SECTOR_BYTES`", and the reason is the accepted mechanism.**
`region_allocate` grants *"the whole frames covering `bytes`"*, so the smallest
region any client can originate is one frame. A rule of *exactly* 512 bytes
would be false of every conforming request.

**A service cannot check this rule, and no service should pretend to.**
Canonical text cannot read a received region's extent — the receive surface
yields a handle and nothing else — and an out-of-range indexed access is a trap
rather than a refusal, so probing for the extent would end the service. The rule
is guaranteed at the origin instead: the only way to obtain a sendable region is
`region_allocate` followed by `region_freeze`, and neither can produce one
shorter than a frame.

A **missing** region is different and is checked: the receive record's `region`
is an `Option`, absence is `BLK_NO_REGION`, and nothing is touched. An absent
**answer endpoint** on a `READ` is checked the same way and is `BLK_NO_ANSWER`.

**Object size and transport extent are different quantities, and this contract
keeps them apart.** A sector is exactly `SECTOR_BYTES`. The region that carries
one covers **at least** that, currently at least one frame. Bytes at or beyond
`SECTOR_BYTES` are ignored by the protocol: they are neither read, written,
compared nor required to hold anything. A service allocates *a region for the
512-byte sector* — not a 512-byte region — and touches only `[0, 512)` of it.

## 9. Case D, retained unchanged

> If the old block service accepted a write request and died before delivering
> the reply, the client receives no guarantee that lets it distinguish "the write
> was not performed" from "the write was performed and the reply was lost".

`ADR-0093` §5 states this boundary and this contract does not move it. **No
transaction identifier, request journal, retry rule, idempotency guarantee or
exactly-once semantics exists at v1**, and none is added for the purpose of
removing case D. A client that needs to know re-reads the sector.

## 10. What v1 does not claim

- **no power-loss durability, no `VIRTIO_BLK_F_FLUSH`, no ungraceful-termination
  behaviour, no crash consistency** (`ADR-0092` §0, Branch A). A successful
  `WRITE` means the device accepted and completed the request;
- no more than one sector per request; no batching; no multiple outstanding
  requests from one client; no queue multiplexing or scheduling policy;
- no `TRIM`, `DISCARD`, `WRITE_ZEROES`, barriers or any other VirtIO block
  feature;
- no filesystem, partition, cache, VFS or object store;
- no ordering guarantee between two requests except that each is answered after
  the device completed it;
- no statement about who may hold a `block.device.v1` endpoint, which is
  `ADR-0093`'s and `ADR-0095`'s.

## 11. Conformance evidence

`ADR-0098` §4 is the obligation list: normative read; normative atomic write;
capacity against the device's own report; invalid-opcode, out-of-range,
absent-region, absent-answer-endpoint and malformed-length negatives; an assertion
that a successful `READ`'s reply is journalled **before** its region send; the
decoy-region mutation that proves a `WRITE` uses the region its own call carried;
and the ordering mutation that proves §6a's control-before-data rule is
implemented rather than only written down.

**One negative class is static, and is recorded as static.** A region shorter
than `SECTOR_BYTES` cannot be constructed from canonical text (§8), so no boot
can exhibit that refusal. This follows the precedent of
`host-tools/qemu-test/virtio-queue.sh`, which records an MSI-X negative that
*"was attempted and withdrawn"* because the reference device could not be made to
fail it, rather than building a fake device to manufacture one.
