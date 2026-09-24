<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS State Store Interface — `state.store.v1`

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0099 (Project Architect-approved, 2026-09-24), a Level 3 decision
which fixes the persistent layout and the protocol this contract states. It
consumes `BLOCK_DEVICE_V1` (ADR-0098) and nothing else.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, and to
docs/09, whose `/state` namespace class it does **not** implement (§1).

**No implementation of this contract exists yet.** It is accepted as the shape the
Stage 4 store must take; the conformance evidence §12 requires is outstanding, and
Stage 4 does not close on an accepted contract.

## 1. Role

`docs/16` §Stage 4 owes *"persistent object/state storage"*, with the engineering
exit *"persistent storage works through a textual user-space driver"*. This
contract is the store's two halves: the **protocol** its clients speak, and the
**persistent layout** it keeps on a block device.

**Substrate, not a namespace.** It does not implement `docs/09`'s `/state`, and
`/state` is unchanged: it remains the architectural namespace class that a later
layer will map onto stores (`ADR-0099` §1). No paths, no directories, no VFS, no
POSIX semantics, no mount.

**What it is not.** Not a filesystem, not a database, not a key-value store with
names, not a cache, not content-addressed, not a repository, and not a snapshot
mechanism.

## 2. Topology and authority

```text
provisioning
    initializer  --(block.device.v1, `call`+`send`)-->  block service
    (§4.4; collected before any state service starts)

steady state
    client  --(state.store.v1 endpoint, `call`)-->  state store  --(block.device.v1, `call`+`send`)-->  block service
    client  <--(one immutable Region<u8>)---------  state store  <--(one immutable Region<u8>)-------  block service
```

- the store is **launcher-wired** from a sealed launch plan and publishes nothing,
  and so is the initializer;
- **the store never formats storage** (§4.4). Only the initializer writes an
  initial header, and it is collected before the store starts — after which the
  store is the sole holder of the block-service client capability;
- the store holds **no** PCI bus, function, MMIO window, interrupt source or DMA
  region;
- a **client of the store holds no `block.device.v1` capability**. It cannot
  address a sector, and that is the store's isolation boundary — capability
  topology, readable from the boot journal.

The store's own endowment at v1 is four capabilities, which is `MAX_ENDOWMENT`: a
memory authority, `receive` on its request endpoint, `send | call` on the block
service's endpoint, and `send | receive` on its own answer endpoint for the block
service's replies.

### 2a. The answer channel is a transient alias, not a fifth grant

**A delegation carries the rights the sender holds** (`IPC_V1` §6) and **one endpoint
has one receive-rights holder at a time** (§2). So neither this store nor its clients
may hand over the name they receive on: the accepting receive would refuse the whole
message and both processes would wait for each other. The channel handed over is a
**send-only alias**, made per request by `capability_attenuate` (ADR-0100):

```text
reply_to = capability_attenuate(state_inbox, RIGHT_SEND)
result   = endpoint_call_word_carrying(reply_to, block_service, request)
capability_release(reply_to)
then interpret result
if success: receive the region on state_inbox
```

**Released whether the call succeeded or not**, and before the result is read: the
callee has its own name, and a store that tidied up only on the happy path would leak
under stress. A client's `GET` is the same pattern on `client-inbox`.

**What it costs**: startup endowments stay **four** for the store and **three** for a
client, `MAX_ENDOWMENT` stays 4, and each outstanding request occupies exactly one
additional capability-table entry. It cannot accumulate, because §8b permits at most
one outstanding `GET` per answer endpoint. The alias is local temporary authority,
never a startup grant, and the receiver identity stays one process — attenuation
grants to the holder that already held it.

**The persistent layout and the wire encoding are untouched by this.** It is how a
channel is obtained, not what travels on it.

## 3. Constants

| Name | Value | Meaning |
|---|---|---|
| `OBJECT_BYTES` | `512` | the payload of one object, and one sector |
| `MIN_ID` | `1` | the lowest valid object id |
| `MAX_ID` | `64` | the highest valid object id, and the width of `occupancy` |
| `HEADER_SECTOR` | `0` | where the store header lives |
| `STORE_SECTORS` | `65` | `MAX_ID + 1` — the whole bounded v1 layout |
| `OPCODE_BITS` | `2` | the width of the opcode field in a request word |
| `OP_GET` | `0` | read one object |
| `OP_PUT` | `1` | create or update one object |
| `OP_RESERVED_2`, `OP_RESERVED_3` | `2`, `3` | reserved; always refused |
| `REQUEST_BYTES` | `8` | the inline length a well-formed request carries |
| `REFUSED` | `9223372036854775808` = `2^63` | the reply-word bit that marks a refusal |
| `MAGIC` | bytes `54 4F 53 53 54 4F 52 45` | `TOSSTORE` |
| `FORMAT_VERSION` | `1` | this layout |

`REQUEST_BYTES`, the request word, the reply word and every persistent field are
fixed-width little-endian. `docs/40` forbids serializing `size` in a persistent
or public form and requires *"one of the explicit fixed-width integers"*; the
contract uses `u32` and `u64` only.

## 4. The persistent layout

```text
sector 0        the store header
sector id       the payload of object id, for id in MIN_ID..MAX_ID
```

**This contract gives meaning to sectors `0..64` and to no others.** It owns —
reserves — exactly that bounded 65-sector extent in the Stage 4 reference layout,
and it **never reads or writes a sector at or above `STORE_SECTORS`**.

**It assigns no meaning and no owner to the remaining sectors.** They are not
free, not reserved and not this store's: they are undecided. The
capsule-to-repository handoff is a separate decision and must not overlap this
extent without explicitly revisiting the layout.

There are no partitions at Stage 4 and **no partition abstraction is
introduced**; a base-offset field would have exactly one possible value, so there
is none.

### 4.1 The store header (512 bytes, sector 0)

| Offset | Size | Field | Rule |
|---|---|---|---|
| 0 | 8 | `magic` | the bytes of `MAGIC`, in ascending address order. Read as bytes, so no endianness applies to it |
| 8 | 4 | `format_version` | `u32` LE. `= FORMAT_VERSION` at v1 |
| 12 | 4 | `schema_version` | `u32` LE. The **owner's** state schema version |
| 16 | 8 | `schema_identifier` | `u64` LE. The **owner's** state schema identifier |
| 24 | 8 | `occupancy` | `u64` LE. Bit `id - 1` is set exactly when object `id` is present |
| 32 | 480 | `reserved` | every byte **must be zero** |

**These offsets and widths are normative.** A layout described only by the
canonical text that writes it is a layout no second implementation could be
checked against, which is what `docs/02` I-09 requires of a versioned boundary.

**There is no `object_capacity` field.** The capacity *is* the width of
`occupancy`: 64 bits, 64 ids. A separate field could disagree with the bitmap
that implements it, and one of the two would then be wrong.

**There is no `payload_base_sector`, `object_bytes` or `header_bytes` field.**
Each has exactly one value at v1, and `format_version` is what a later layout
changes them through. A field whose value the version already determines is a
second place for one fact.

### 4.2 Three identities, kept distinct

```text
format_version       the layout of this header and of placement    — the store's
schema_identifier    whose state this is                           — the owner's
schema_version       the shape of the owner's object payloads      — the owner's
```

The store **interprets** the first and **compares** the other two. It never
interprets a payload byte. At v1 a store has exactly one owner, whose schema
identity is a constant of the store service's canonical text; a later version
serving several owners needs per-owner identity, which is a layout change and
therefore a `format_version` change.

### 4.2a The scope of `schema_identifier`

**It is not a global namespace**, and this contract creates none:

```text
schema_identifier
    a u64 chosen by the owner of this store
    stable for one schema lineage across compatible source revisions
    interpreted only together with this STATE_STORE_V1 store extent
    not globally unique
    not a content id
    not a module id
    not a /state path id
```

`schema_version` versions that owner's schema **within** that lineage. The pair is
what §4.3 compares on opening; a mismatch is a refusal, never a conversion.

At Stage 4 there is exactly one owner and one store, so **no registry, no
allocation mechanism and no uniqueness rule is needed**, and none is defined. A
later multi-owner or multi-store design may need a stronger identity contract; it is
not decided here.

### 4.3 Validation

A store is **open** only when sector 0 satisfies all of:

- `magic` equals `MAGIC`;
- `format_version == FORMAT_VERSION`;
- every byte of `reserved` is zero;
- `schema_identifier` and `schema_version` equal the opening service's own
  declared constants.

`occupancy` needs no validation: every one of its `2^64` values is a legal set of
present ids, and `0` means an initialized but empty store — which is a valid
store and is **not** the same thing as uninitialized storage.

An all-zero sector 0 fails the first condition. **Uninitialized storage is not a
store**, and a zero-filled device is exactly what a fresh image presents.

### 4.4 Formatting, which is not something a state service does

**No `state.store.v1` service ever formats storage.** A missing or invalid header
means it **cannot open**, and it then refuses every request with `ST_STORE`. It
does not repair, reformat or guess — and it could not decide to safely, because a
child is never told its restart generation: the supervisor asserts it and, as
`PROCESS_IDENTITY_V1` §3 records, the nucleus *"records it and never computes or
increments it"*. Both generations run the same canonical module from the same
shared launch plan, so a service cannot distinguish a fresh first start from a
restart whose header failed to read, and one that auto-formatted would destroy a
store to recover from a transient failure.

**Formatting is a separate canonical textual bootstrap action**, performed once
during provisioning by a short-lived **initializer** process:

```text
capacity()
require capacity >= STORE_SECTORS

READ HEADER_SECTOR

if all 512 bytes are zero:
    WRITE one complete valid header:
        MAGIC, FORMAT_VERSION, the owner's schema identity,
        occupancy = 0, reserved zero
    terminate success
else:
    REFUSE TO FORMAT
```

It writes **no payload sector**. A device that cannot contain the whole bounded
layout is refused before the header is written, rather than discovered later by
running past the end of it — which is why `ADR-0098` keeps `capacity` in v1.

**Only an all-zero header is permission to format.** An initializer that wrote
unconditionally would silently reset `occupancy` against an existing store, losing
every object in it while reporting success — unacceptable for an action that is
separated from ordinary startup *because* it is destructive.

**This test is deliberately stricter than §4.3.** Opening asks whether a store is
usable; formatting asks whether there is certainly nothing at all. So none of these
is permission to initialize:

| Sector 0 holds | Why it is not permission |
|---|---|
| an **already valid** header | the store exists |
| a header **valid but for another schema** | it is another owner's store, and `schema_identifier` is not a claim on this extent (§4.2a) |
| a **future or unknown `FORMAT_VERSION`** | a v1 initializer cannot know what it would destroy |
| a **corrupt** header | the likeliest reading is a store whose header failed to read |
| **merely non-zero garbage** | something wrote it, and this contract gives no meaning to bytes it did not write |

**Sectors 1..64 are not inspected before formatting, and must not be.** The Stage 4
harness deliberately seeds some of them, so their contents say nothing about whether
a store exists: `occupancy` is authoritative, and it is only meaningful once a store
does.

The initializer is canonical TOS text, reaches the device **only** through
`block.device.v1`, holds **no** PCI, MMIO, IRQ or DMA authority, publishes
nothing, and is **collected before the ordinary state service starts** — after
which the state service is the sole holder of the block-service client capability.
It is not a host path and not a harness step.

### 4.5 Opening

Both generations open the same way, from the device:

```text
read sector 0 through block.device.v1
validate per §4.3
```

**Nothing is handed to a successor in memory**, which is what lets one canonical
module serve every generation.

## 5. Object semantics

An object is exactly `OBJECT_BYTES` of opaque payload, and **there is no
per-object header**. The store does not interpret the bytes.

**Presence is determined solely by `occupancy`:**

```text
bit (id - 1) clear  ->  absent, whatever bytes sector `id` already holds
bit (id - 1) set    ->  present
```

A `GET` of an absent id is refused and **its sector is not read**. This is what
makes seeded, stale or adversarial sector contents **non-authoritative**: a store
that inferred presence from bytes could be convinced by anything that had been on
the device before it.

**Not in v1:** delete, enumeration, `capacity`, `stat`, rename, variable-size
objects, objects spanning sectors, an allocation cursor, a free list,
transactions.

## 6. The request word

```text
word = id * 4 + opcode           opcode = word % 4,  id = word / 4
```

`id` must be in `MIN_ID..MAX_ID`. **`id = 0` is never valid**, so a zero word —
which is what an absent or malformed payload looks like — is refused rather than
meaning something.

**There is no in-band protocol-version field in the word.** `state.store.v1` is a
versioned Tier 2 service contract; accepted binding and launch topology supplies a
client with an endpoint **implementing that contract**; and v1 has **no runtime
version negotiation**. A per-request tag would be redundant with the service
contract the topology already configured — **not** with endpoint-object identity,
which is a different thing and cannot carry a protocol version: one protocol may
have several endpoint objects, as `block-lifecycle.sh` demonstrates for
`block.device.v1` across two service generations.

## 7. The reply

Every reply is sent with `endpoint_reply_word`, so its inline length is
`REQUEST_BYTES`. A client checks `length == REQUEST_BYTES` before reading `word`.

```text
word < REFUSED     success. Both operations answer 0.
word >= REFUSED    refusal. `word - REFUSED` is the code of §9.
```

## 8. The operations

### 8a. `PUT` — create or update

```text
client:  endpoint_call_word_region(store, region, id * 4 + OP_PUT)
store:   endpoint_reply_word(reply, 0)
```

**One atomic call.** The request word and the payload region cross in the same
message, through `ADR-0098` §2a's row. A region sent in one message and a request
in another is **not** this protocol: it is correct only where one client is
serialized against itself, and two clients' messages interleave in one queue.

The store **forwards the client's region** to `block.device.v1`'s `WRITE`
without copying it (§10), and replies only after the block service has
acknowledged every device request the operation needed.

Ordering, which is normative:

```text
previously absent (bit clear):   write payload sector `id`
                                 write sector 0 with bit (id - 1) set
                                 reply success

already present (bit set):       write payload sector `id`
                                 reply success
```

An object becomes visible only after its bytes are on the device, so a store that
stopped between the two writes has a written sector and no object — a lost write,
not a corrupt store. **No power-loss atomicity follows from this order**, and
none is claimed (§11).

### 8b. `GET` — read

```text
client:  endpoint_call_word_carrying(answer_endpoint, store, id * 4 + OP_GET)
store:   endpoint_reply_word(reply, 0)
store:   endpoint_send_region(carried, the object's region)
```

The store checks `occupancy` first and refuses an absent id **without reading its
sector**. Otherwise it reads sector `id` through `block.device.v1`'s `READ`
**completely**, replies success, and **then** forwards the region it received
(§10) to the endpoint the request delegated.

**Control before data, and the order is normative**, for `BLOCK_DEVICE_V1` §6a's
reason: the reverse admits an orphan response. An endpoint object lives for the
boot, so a region queued on an answer endpoint whose waiter then died stays there,
and a later holder of receive — including a successor created from the same launch
plan — could take it as the answer to its own request.

| The client sees | It means |
|---|---|
| a **refusal** reply | refused; **no region follows** |
| a **success** reply | the object was obtained and **exactly one region is owed** |
| the **region** arriving | the `GET` is complete |

**A client may have at most one outstanding `GET` per answer endpoint**, and may
not issue another until the owed region has been received.

**If the store dies after the success reply and before the send**, the client
observes an incomplete operation through the ordinary liveness path — a blocked
receive cancelled with `E_CANCELLED` (`SYSTEM_ABI_V1` §6) — and no stale region has
been queued. If the region is sent, it belongs to the one outstanding successful
operation.

**A reply never carries a region**, which is why the request has to say where to
answer.

## 9. Refusals

| Code | Name | When |
|---|---|---|
| 1 | `ST_OPCODE` | the opcode is reserved |
| 2 | `ST_ID` | `id` is outside `MIN_ID..MAX_ID`, including `0` |
| 3 | `ST_MALFORMED` | the inline length is not `REQUEST_BYTES` |
| 4 | `ST_NO_REGION` | `OP_PUT` whose call carried no region |
| 5 | `ST_NO_ANSWER` | `OP_GET` whose call carried no answer endpoint |
| 6 | `ST_ABSENT` | `OP_GET` of an id whose occupancy bit is clear |
| 7 | `ST_STORE` | the store is not open: no valid header, or the device is too small |
| 8 | `ST_BLOCK` | `block.device.v1` refused, or reported a device failure |

A refusal is always a **reply**: a store that failed to answer would leave its
caller blocked until the liveness rule cancelled it (`SYSTEM_ABI_V1` §6), and a
client could not tell a refusal from a dead store.

**Every refusal is decided before the reply is sent, and that is why there is no
code for a failed region delivery.** §8b replies success **before** sending the
region, so a delivery that fails afterwards cannot become a refusal — the reply is
already spent. A store in that position records the fault in its journal, and the
client sees an incomplete operation. `ST_NO_ANSWER` is the part that *is*
checkable, and it is checked before anything is read, because the receive record
reports an absent answer endpoint as absence rather than as a handle.

`ST_OPCODE`, `ST_ID`, `ST_MALFORMED`, `ST_NO_REGION`, `ST_NO_ANSWER` and
`ST_ABSENT` touch the device not at all. `ST_BLOCK` reports what the layer below
said. A refusal after a payload sector was written but before the occupancy bit
was set leaves the object **absent**, which §8a's order is chosen to make true.

## 10. The one-copy rule

`docs/35` §Stage 4 budgets *"no more than one payload copy between client memory
and device-visible memory"*, absolutely. So:

```text
PUT   client region  ->  store  ->  the same region  ->  block service  ->  DMA copy
GET   block region   ->  store  ->  the same region   ->  client
```

**The store allocates no second payload region and copies no payload byte.**
`ADR-0075` §5a makes the forward exact: a successful send takes the sender's
handle and its mappings atomically, so after forwarding the store holds nothing
and a later access through that handle is refused rather than reading memory it
no longer owns.

**Header construction is metadata, not payload.** The store allocates **a region
for the 512-byte header**, composes the header in its first `OBJECT_BYTES`,
freezes it and writes it as **its own block request**, accounted separately
(`ADR-0099` §11).

**A region for 512 bytes is not a 512-byte region.** An object and a sector are
exactly `OBJECT_BYTES`; the region carrying one covers **at least** that —
currently at least one frame — and every byte at or beyond `OBJECT_BYTES` is
ignored by this protocol and by `block.device.v1` (§8 there). The rule is
guaranteed at the origin: the only way to obtain a sendable region is
`region_allocate` followed by `region_freeze`, and neither can produce one shorter
than a frame, so a too-short region cannot be originated from canonical text at
all.

## 11. What v1 does not claim

- **no power-loss durability, no `VIRTIO_BLK_F_FLUSH`, no ungraceful-termination
  behaviour, no crash consistency, no journaling, no transactions**
  (`ADR-0092` §0, Branch A);
- **no exactly-once `PUT`.** If the store dies after some device effect and before
  its reply, the outcome is deliberately ambiguous, consistently with `ADR-0093`
  §5's case D. No transaction id, request journal, retry rule or idempotency
  guarantee exists at v1;
- no filesystem, `/state` mount, path, directory, VFS or POSIX semantics;
- no content addressing, Git identity or repository semantics — an id bears no
  relation to the bytes;
- no enumeration, delete, rename or snapshot;
- no multi-owner store, second store, or store discovery;
- no ordering guarantee between two operations beyond §8a's within one `PUT`.

## 12. Conformance evidence

`ADR-0099` §13 is the obligation list: an initializer formatting a fresh device and
being collected; a store service opening the header it wrote; two objects written by
a writer that then ends; the store service ending, being retired and having its
ending collected by its supervisor; a successor of the same canonical module
re-reading and validating the header from the device; and a new reader — holding no
`block.device.v1` capability — getting the second object and verifying all 512 bytes
in canonical text.

**One negative is about formatting rather than about persistence:** the initializer
run against a **valid existing header** must refuse, and the header and `occupancy`
must be **unchanged** afterwards. A mutation removing §4.4's zero-header check must
turn that negative red.

Its seven required mutations: omit the **initializer's** header write, so the state
service cannot open; omit or falsify the payload device write; answer `get(2)` from
object 1's sector; ignore the occupancy bitmap, which must make an id never created
become visible and must turn the negative gate red; omit `PUT(2)`'s
occupancy-header update, so that a successor reading the persisted header finds
object 2 **absent** however much of its sector was written; send `GET`'s region
before replying success, which must turn the ordering assertion red; and remove the
initializer's zero-header check, which must turn the refuse-to-reformat negative
red.
