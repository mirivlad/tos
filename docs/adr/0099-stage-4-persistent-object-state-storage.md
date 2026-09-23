<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0099: Stage 4 persistent object/state storage — a private native store, substrate first

- Status: **Proposed** (raised 2026-09-23 on Project Architect direction; **not
  accepted, and nothing in the tree implements it**)
- Date: 2026-09-23
- Decision level: **3** — architectural, **requiring Project Architect
  approval**. `docs/21` places *"changes persistent formats"* at Level 3, and
  `ADR-0017` applied that test to itself in as many words — *"Explicitly **not**
  Level 3: no capsule byte changes"*. This decision **creates** the first
  normative persistent state format, which is not less architectural than
  changing one. §12 is the architecture impact statement `docs/21` requires
- Project Architect approval: **not granted; this is a draft for review**
- Depends on: **ADR-0098**, which must be accepted first. This decision's store
  reaches the device only through `block.device.v1`, and building it on the
  lifecycle fixture's deliberately non-normative encoding would make a fixture a
  production dependency
- Related: **ADR-0092** §0 (Branch A: the Stage 4 persistence reading);
  **ADR-0093** §5 (case D), §9 (which names this deliverable as separate);
  **ADR-0095** §6 (a second published interface is undecided); **ADR-0097**
  (the textual region surface); **ADR-0075** §5a (linear transfer);
  **ADR-0077** (launch plans); `docs/09` §`/state`, §State schema versions,
  §Filesystem implementations; `docs/11` §Bootstrapping step 4; `docs/16`
  §Stage 4; `docs/19` §Decisions still requiring ADRs; `docs/35` §Stage 4;
  `docs/40` §primitive types; `docs/02` I-04, I-09, I-13, I-16;
  `docs/research/STAGE4_PERSISTENT_STATE_BOUNDARY.md`, which is the note this was
  raised from and is authority for nothing

## 0. What this decides

`docs/16` §Stage 4 owes **persistent object/state storage**, and the stage's
engineering exit is *"persistent storage works through a textual user-space
driver"*. `docs/19` §Decisions still requiring ADRs already names *"first
persistent object/state filesystem"* as a decision requiring one. This is it.

**It decides seven things and no more:** the Stage 4 reading of the deliverable,
who owns the store and what authority it holds, what identifies an object, where
objects live, where format and schema identity live, the `state.store.v1`
protocol, and how the `docs/35` handoff budget is measured. The proposed contract
is `docs/proposed/STATE_STORE_V1.md`.

## 1. Stage 4 interpretation: substrate first

**Stage 4 delivers a private native persistent object-store substrate. It does
not expose `/state`.** No path namespace, no VFS, no directories, no POSIX
semantics, no mount.

`docs/09`'s `/state` is **unchanged and not redefined**: it remains the
architectural namespace class for *"Mutable durable state owned by services"*,
whose paths are *"namespaced by service identity and protected by
capabilities"*, and a later layer will map it onto stores. Two accepted clauses
put the substrate first: `docs/09` §Filesystem implementations admits that *"the
first implementation may use a simple native object store and state filesystem
under QEMU"* while separately requiring that *"the VFS and capability contracts
must not assume a particular disk format"* — a VFS arriving above a store that
already exists — and `docs/11` §Bootstrapping step 4 is *"Text driver initializes
persistent storage"*, which is substrate language.

**The format is not a VFS dependency.** No VFS exists here, and a later VFS
contract must remain format-independent exactly as `docs/09` requires. What this
decision fixes is the bytes **one service** writes and reads; nothing above it is
permitted to assume them.

## 2. Ownership, authority and topology

```text
provisioning:   initializer          --(block.device.v1)-->  block service  -->  device
                (collected before the state service starts)

steady state:   client  --(state.store.v1)-->  state store  --(block.device.v1)-->  block service  -->  device
```

- **The state store service is launcher-wired.** It receives its endpoints from a
  sealed launch plan (`ADR-0077` §3–§5), which is what `ADR-0093` §3a answer 3
  already requires of every client's first capability. **It does not publish**,
  and this decision introduces **no second publication class** — `ADR-0095` §6
  leaves that undecided and §8 states that the spare endpoints are not an
  argument for it;
- **once provisioning is over it is the steady-state holder of the block-service
  client capability.** The only other process ever endowed with one is §6's
  initializer, which exists before it and is collected before it starts. Clients
  of the store hold no `block.device.v1` capability at all, which is what makes
  "the reader could not have reached a sector" a fact about the boot's capability
  topology rather than a claim about its source;
- **it holds no PCI bus, function, MMIO window, interrupt source or DMA region.**
  The block service remains the only holder of hardware authority. The store's
  isolation boundary **is** capability topology, and the boot journal is where it
  is read.

## 3. Object identity: a store-local `u64`, bounded to 1..64

An object is named by a `u64` **object id**, and the valid domain at v1 is
`1..64` inclusive. `0` is not an id, so a zero word is never a valid request.

- **not a path**, and not a name. A later `/state` layer may map names onto these
  ids without changing v1 object identity, because a resolution layer above the
  store is additive;
- **not a content id.** It bears no relation to the bytes, so nothing here can be
  mistaken for `docs/08`'s content-addressed model or for a Git identity;
- **private to this store instance and its owner.** It has no global meaning, and
  two stores' id spaces are unrelated. `docs/09`'s *"namespaced by service
  identity"* is satisfied at the capability layer — which store endpoint a service
  holds is which store it can reach — and not by the id, because the store cannot
  learn who is calling: `system.ipc.ReceivedCall` and
  `system.ipc.ReceivedCallRegion` carry no caller identity.

**Why 64 and not a larger bound.** The bound is the width of the occupancy field
in §5's header, and choosing them together is the point: an object capacity
recorded as its own field could disagree with the bitmap that implements it.

## 4. Object semantics

Each present object is exactly **`SECTOR_BYTES` = 512 opaque payload bytes**, and
**there is no per-object header**. The store does not interpret them.

**Presence is determined solely by the header's occupancy bitmap:**

```text
bit (id - 1) clear  ->  object absent, whatever bytes its sector already holds
bit (id - 1) set    ->  object present
```

This is deliberate, and it is the mechanism by which **seeded or stale sector
contents are non-authoritative**. A store that inferred presence from the bytes
could be convinced by anything that had been on the device before it, including
the Stage 4 harness's own seeded sectors.

**Not in v1:** delete, enumeration, variable-size objects, an allocation cursor,
a free list, rename, transactions, or objects larger than one sector.

## 5. Placement and the persistent layout

**Deterministic placement with a versioned store header and an occupancy
bitmap.** The logical shape:

```text
sector 0        the store header
sector id       the payload of object id, for id in 1..64
```

**`STATE_STORE_V1` gives meaning to sectors `0..64` and to no others.** It
reserves exactly that bounded 65-sector extent in the Stage 4 reference layout,
and it **never reads or writes a sector at or above 65**. There are no partitions
at Stage 4 and no partition abstraction is introduced; a base-offset field would
have one possible value, which is why there is none.

**This decision assigns no meaning and no owner to the remaining sectors.** They
are not free, not reserved and not the store's — they are undecided. The
capsule-to-repository handoff is a separate decision (§14) and **must not overlap
this extent without explicitly revisiting the layout**, which is a statement this
decision makes so that a later one cannot make it by accident.

The header is 512 bytes, all numbers little-endian and fixed-width under
`docs/40`'s rule that *"Public and persistent forms use one of the explicit
fixed-width integers"* and that `size` *"MUST NOT be serialized in a
persistent/public format"*:

| Offset | Size | Field | v1 rule |
|---|---|---|---|
| 0 | 8 | `magic` | the bytes `54 4F 53 53 54 4F 52 45` (`TOSSTORE`), read as bytes so no endianness applies |
| 8 | 4 | `format_version` | `u32` LE, `= 1` |
| 12 | 4 | `schema_version` | `u32` LE, the owner's state schema version |
| 16 | 8 | `schema_identifier` | `u64` LE, the owner's state schema identifier |
| 24 | 8 | `occupancy` | `u64` LE, bit `id - 1` set iff object `id` is present |
| 32 | 480 | `reserved` | **must be zero** |

**The exact offsets and widths are normative in `STATE_STORE_V1`**, not
implementation comments in a module — a layout described only by the code that
writes it is a layout no second implementation can be checked against.

**Why deterministic placement rather than an allocation index.** A store with 64
fixed slots needs no cursor, no free list and no reuse policy, and therefore has
no two-write allocation ordering question at a stage where power loss is out of
scope. What it does need is a header, and the reason is the row above: without one,
`get` of an id never written is indistinguishable from `get` of an id written as
zeros, so the "simplification" of having no persistent metadata does not survive
contact with the first absent object.

**What a future change costs.** Variable-size objects, more than 64 objects,
delete-and-reuse or multi-owner stores all need a different layout — and
`format_version` is what makes each of them a recognizable successor rather than
a misread. A v1 reader refuses a version it does not know instead of interpreting
it.

## 6. Formatting is a separate action from opening

**An earlier draft said the first generation may initialize and a successor never
does. That is not implementable**, and the reason is worth stating because it is a
property of the accepted launch model rather than of this design: the **supervisor**
knows a child's restart generation — it asserts it, through
`process_create_with_generation` — and `PROCESS_IDENTITY_V1` §3 records that the
nucleus *"records it and never computes or increments it"*. The child is told
nothing. Both state generations run the same canonical module from the same shared
launch plan (§13), so a service asked to decide for itself cannot distinguish

```text
a fresh first start                  from     a restart whose header is missing or corrupt
```

and a service that auto-formatted on an invalid header would **destroy a store to
recover from a transient failure to read one**. So it does not.

### 6a. The initializer

Formatting is a separate canonical textual bootstrap action, performed once during
provisioning by its own short-lived process:

```text
state-store initializer
    capacity()
    require capacity >= STORE_SECTORS                  (65)
    write one complete initial STATE_STORE_V1 header
    terminate
```

The header it writes is exactly: magic, `format_version = 1`, the owner's schema
identity, `occupancy = 0`, `reserved` zero. It writes **no** payload sector.

The initializer:

- **is canonical TOS text**, verified and launched like any other module. It is
  **not** a host path, not a harness step and not a privileged helper;
- reaches the device **only** through `block.device.v1`;
- holds **no** PCI bus, function, MMIO window, interrupt source or DMA region;
- **exists only during provisioning** and is collected before the ordinary state
  service is created, which is also what keeps the block-service client capability
  single-holder in steady state (§2);
- is **not** a second publication class: it publishes nothing and is
  launcher-wired like everything else here.

**This is the other reason `ADR-0098` keeps `capacity` in v1.** The initializer
must establish that the whole bounded layout fits **before** it writes a header it
would otherwise run past the end of, and it must do so without learning the bound
by issuing a request built to be refused.

### 6b. Opening

**The ordinary state service never formats storage.** It opens, or it cannot open:

```text
state service
    read sector 0 through block.device.v1
    validate: magic equal; format_version == 1; reserved all zero;
              schema_identifier and schema_version equal its own constants
```

- **an all-zero sector 0 is not a valid store.** Uninitialized storage is
  uninitialized, and a zero-filled device is exactly what the Stage 4 harness
  presents on a fresh image;
- a missing or invalid header means the service **cannot open**, and every request
  it then receives is refused with `ST_STORE`. It does not repair, reformat or
  guess;
- **nothing is handed to a successor in memory.** Both generations open the same
  way, from the device, which is why one canonical module serves both.

## 7. State schema identity, and the Stage 4 migration policy

**The store header is the persistent location of format and schema identity.**
Not per-object headers, which would cost payload out of the 512 bytes an object
is and which v1 does not need with one schema and one producer. And **not
source-only identity**: a service's source tells you what the running code
expects, never what is on the device, so after the first source revision the
bytes would be unidentifiable — and `docs/09` §State schema versions requires
*migration functions* and a *maximum supported migration chain*, both of which
presuppose reading the version the bytes were written with. I-16's traceability
points the same way.

Three distinct facts, and the contract keeps them distinct:

```text
format_version       the layout of the header and of placement — this decision's
schema_identifier    whose state this is — the owner's
schema_version       the shape of the owner's object payloads — the owner's
```

The store interprets the first and **compares** the other two; it never
interprets a payload.

**Stage 4 migration policy, stated explicitly as `docs/09` requires:**

- v1 has **no predecessor**; the migration chain length is **zero**;
- there are **no migration functions**, because there is nothing to migrate from;
- the **downgrade policy is none** — a v1 reader refuses any other
  `format_version` rather than attempting to read it;
- **compatible source-module versions** are those declaring the same
  `schema_identifier` and `schema_version`; a mismatch is a refusal, not a
  conversion;
- the **snapshot mechanism remains a separate `docs/19` decision** and is not
  part of this one. `docs/09` §Snapshot linkage and §Transaction boundaries stay
  unimplemented, and nothing here claims otherwise.

**At v1 a store has exactly one owner**, whose schema identity is a constant of
the store service's canonical text. A later version serving several owners needs
per-owner schema identity, which is a layout change and therefore a
`format_version` change.

## 8. The `state.store.v1` protocol

**A new versioned userspace service protocol under I-09**, which lists *"IPC
schemas"* among what is versioned from first implementation. Being built out of
generic Endpoint and Region operations exempts it from nothing: the publication
path is the precedent in the other direction, since `ADR-0093` and `ADR-0095`
decided a naming protocol that also added no ABI operation and still took two
decisions and a negative gate.

At Stage 4 it exposes two operations and no others:

```text
put(id, Region<u8>)      create or update
get(id) -> Region<u8>
```

- **`PUT` is one atomic call** carrying the request word and one immutable region
  covering at least the object's 512 bytes, through `ADR-0098` §2a's
  `endpoint_call_word_region`. It is **not** a region message followed by a
  request, for `ADR-0098` §1's reason;
- **`GET` is a call carrying the client's answer endpoint**, and its **control
  reply precedes its data**: the store obtains the object completely, replies
  success, and only then sends exactly one region to the delegated endpoint. That
  is `ADR-0098` §2d's ordering and it is here for the same reason — the reverse
  order can leave an orphan region queued on an endpoint that outlives the process
  that was waiting for it. A client may have **at most one outstanding `GET` per
  answer endpoint**;
- the request encoding, refusal codes, region-extent rule and id bound are fixed
  in `STATE_STORE_V1`.

**A failure after success has been replied cannot become a refusal**, so there is
no refusal code for a failed region delivery. `ST_ANSWER` as first drafted is
withdrawn; what remains is the pre-reply check the `Option` fields make possible —
a `GET` carrying no answer endpoint is refused as `ST_NO_ANSWER` **before**
anything is read.

**No `delete`, no enumeration, no `capacity`, no `stat`, no rename.** A client
learns an object is absent by asking for it.

## 9. `PUT` ordering, and what does not follow from it

```text
previously absent object:   write payload sector
                            write header with its occupancy bit set
                            acknowledge

existing object:            write payload sector
                            acknowledge
```

The order is the useful one: an object becomes visible only after its bytes are
on the device, so a store that stopped between the two writes has a sector
written and no object — which is a lost write, not a corrupt store.

**No power-loss atomicity follows from this order, and none is claimed.**
`ADR-0092` §0 Branch A excludes power-loss durability, `VIRTIO_BLK_F_FLUSH` and
ungraceful-termination evidence from Stage 4, and this decision adds no crash
consistency, no journal and no transaction. If the service dies after some device
effect and before its reply, the outcome is **deliberately ambiguous**,
consistently with `ADR-0093` §5's case D, which `ADR-0098` §9 retains verbatim.
**No transaction id, request journal, retry rule, idempotency guarantee or
exactly-once `PUT`.**

## 10. The one-copy rule

`docs/35` §Stage 4 budgets *"no more than one payload copy between client memory
and device-visible memory"*, in absolute terms. The store therefore **forwards**
payload regions linearly and never copies one:

```text
PUT   client region  ->  state store  ->  the same region  ->  block service  ->  DMA copy
GET   block service region  ->  state store  ->  the same region  ->  client
```

`ADR-0075` §5a makes the forward exact: a successful send takes the sender's
handle and its mappings atomically, so the store does not hold what it passed
on. **The store allocates no second payload region and copies no payload byte.**

**Header construction is metadata, not payload.** The store allocates **a region
for the 512-byte header**, composes the header in its first 512 bytes, freezes it
and writes it as **its own block request** — separately accounted, and never mixed
with a payload region.

**A region for 512 bytes is not a 512-byte region**, and the contract keeps the
two quantities apart (§4, `BLOCK_DEVICE_V1` §8): an object and a sector are exactly
512 bytes, while the region carrying one covers **at least** that — currently at
least one frame — and every byte at or beyond 512 is ignored by both protocols.

## 11. The `docs/35` handoff budget: measurement endpoints

**Recorded as a clarification of what the accepted budget measures, not a
relaxation of it.** `docs/35` §Stage 4's *"no more than four address-space/
scheduler handoffs per unbatched request"* sits among siblings that are all
per-completed-block-request and device-facing, so its endpoints are

```text
immediate block client  <->  block service  <->  device        one completed block request
```

A `state.store` operation may issue **several** block requests — a `PUT` of an
absent object issues two — and **each remains subject to the block budget
independently**. No end-to-end conformance for a state operation is claimed from
it, and none may be inferred.

**What the Stage 4 performance-contract report must therefore do:** measure the
block-request budget explicitly against these endpoints, and retain end-to-end
`state.store` latency and handoff counts as a **separate observational metric**
unless and until a threshold is assigned to them. Today the metric is **P0** —
unmeasured design — and `docs/35` §Reporting status is that *"No stage closes on
P0 for a metric assigned to that stage."* The hard block-driver budget is not
weakened: it is given the endpoints it was always about.

## 12. Architecture impact statement (`docs/21`)

- **Which invariants are affected?** I-09 — a persistent format and a service
  protocol are versioned from their first implementation, which §5 and §8 are how.
  I-04 — the store holds runtime state and touches no system commit; it is not
  `/system` and cannot become it. I-13 — §13's mutations exist because a
  demonstration must exercise the real contract. I-16 — traceability is why §7
  refuses source-only schema identity. No invariant is amended.
- **What becomes canonical after the change?** `STATE_STORE_V1`'s header layout
  and protocol, for this store. Nothing above the store may assume the layout,
  and `docs/09`'s `/state` is unchanged.
- **What enters or leaves the trusted base?** Nothing. No nucleus change, no ABI
  operation, no capability or object kind; the store is an ordinary textual
  service with no hardware authority.
- **Can the active runtime still identify its exact source?** Yes, unchanged; and
  the store now records which schema wrote its bytes, which is a traceability
  gain rather than a cost.
- **Can all derived artifacts be discarded and regenerated?** The store's bytes
  are **not** a derived artifact — they are the mutable durable state I-01
  distinguishes from caches, and deleting them is data loss, which `docs/09`
  §Namespace classes at a glance already says of `/state`. No cache format
  changes.
- **Can the owner still recover and boot a previous commit?** Yes. The store is
  not in the boot path, holds no part of `/system`, and no activation depends on
  it. I-05 is untouched.
- **Does the change create a hidden host dependency?** No. The host harness seeds
  and retains a raw image and judges reported results; every byte that crosses is
  moved by textual modules.
- **Does it alter licensing or patent exposure?** No third-party format, algorithm
  or mechanism is adopted. The layout is the project's own and deliberately
  unlike any existing filesystem.
- **How is the behavior tested?** §13.

## 13. Conformance evidence this decision requires

**At least two ids**, and the shape is:

```text
initializer      capacity() >= 65; writes one initial header; terminates
                 its ending is collected
state store A    opens and validates the existing header
writer           put(1, pattern A); put(2, pattern B)
                 its ending is collected
state store A    ends; its ending is collected and its slot reclaimed
state store B    the same canonical module; reads and validates the header
                 from the device
reader           get(2); verifies all 512 bytes of pattern B in canonical text
```

### 13a. The bounds, counted before implementation

Seven process instances, and none of the accepted bounds moves. **`MAX_PLANS` is
not raised**, and the reason it does not need to be is that a sealed plan *"is not
consumed by the creation that reads it"* (`plan.rs`), so one plan may launch two
sequential processes:

| Bound | Value | This fixture |
|---|---|---|
| `MAX_PROCESSES` | 4 | peak **4**: `init + block + state + client`. The initializer is collected before state A is created, and the writer and state A are collected before state B and the reader exist |
| `MAX_PLANS` | 4 | exactly **4**: block; initializer; **state**, shared by A and B; **client**, shared by writer and reader |
| `MAX_ENDPOINTS` | 6 | **4**: `block-serve`, `state-serve`, `state-inbox`, `client-inbox`. Two spare |
| `MAX_ENDOWMENT` | 4 per plan | block **3** (`budget`, `block-serve` receive, `device` claim); initializer **2** (`budget`, `block-serve` send\|call); state **4** (`budget`, `state-serve` receive, `block-serve` send\|call, `state-inbox` send\|receive); client **3** (`budget`, `state-serve` send\|call, `client-inbox` send\|receive) |
| `MAX_CAPABILITIES` | 16 per process | the supervisor is the only one near it, as in `block-lifecycle`; its peak must be counted during implementation and child controls released after each collection |

**One client plan for the writer and the reader**, as directed: they are
sequential, and the writer simply does not use the `client-inbox` the plan grants
it — a `PUT` is one atomic call answered by a word, so only the reader needs an
inbox at all. An unused grant is not a defect; a plan is a policy, and
`granted()` records what was installed.

**The initializer needs no inbox either**, because `CAPACITY` is an ordinary call
and `WRITE` is an atomic call carrying its region — neither is answered with a
region. That is what keeps it at two endowments.

**Sharing one plan is exactly what makes the receive-holder rule load-bearing.**
`IPC_V1` §2 admits one receive-rights holder at a time and `capability.rs`
enforces it as `NotGranted::ReceiverExists`, so creating state B or the reader
before its predecessor's slot is reclaimed is **refused by the nucleus**. The
sequencing above is not a convention the fixture observes; it is the only order in
which the fixture can be built at all.

- **the reader holds no `block.device.v1` endpoint**, asserted from the boot
  journal's capability records, not from source;
- **the successor re-reads the header from the device.** No state crosses from A
  in memory, and `IPC_V1` §2's one-receiver rule makes B's creation impossible
  until A's slot is reclaimed, so the sequencing is enforced rather than hoped
  for;
- **the patterns vary per byte.** The harness seeds sectors 1–4 with 512 copies of
  `0xC0 + n` and leaves sector 0 zeroed, so a constant fill and a zero fill are
  both distinguishable from either pattern — and ids 1 and 2 land on seeded
  sectors on purpose;
- **the byte comparison happens in canonical text.** The host gate judges reported
  account bits and journal order.

**Required mutations**, each of which must turn the gate red on its own
assertion:

1. **omit the initializer's header write** — state service A **cannot open**, and
   every request it receives is refused with `ST_STORE`. This is a claim about
   *formatting*, and it is kept separate from claim 5 on purpose;
2. **omit or falsify the real payload device write** — the final byte witness
   fails;
3. **answer `get(2)` from object 1's sector** — the final byte witness fails;
4. **ignore the occupancy bitmap** — `get(3)`, an id never created, must be
   refused as absent; the mutation makes it answer with sector 3's seeded `0xC3`
   fill instead, and the negative gate must fail;
5. **omit `PUT(2)`'s occupancy-header update** — A may well have written sector 2,
   the successor reads the **persisted** header, and `get(2)` is therefore
   **absent**. This is a claim about *persistence of the occupancy record*, and it
   is a different claim from 1: 1 says an unformatted store cannot be opened, 5
   says an object whose bit never reached the device does not exist however much
   of it did;
6. **send `GET`'s region before replying success** — the ordering assertion of
   `ADR-0098` §4 obligation 9, applied to this protocol, must turn red.

**Non-claims the evidence must state**, in the form `block-lifecycle.sh` uses:
no power-loss durability; no `FLUSH`; no ungraceful-termination behaviour; no
crash consistency; no exactly-once `PUT`; no filesystem; no `/state` mount; no
content addressing; no repository semantics; and **no Stage 4 closure**.

## 14. What this does not decide

- **`/state` path exposure, a VFS, directories or POSIX semantics** — §1;
- **the capsule-to-repository handoff**, which is a separate `docs/16`
  deliverable, lives at `docs/08` §Work decomposition's G1 and beyond, and sits on
  a different trust boundary because `docs/08` puts object-store traversal and
  protected ref primitives in the nucleus while this store holds nothing the
  nucleus reads;
- **the state snapshot mechanism** and snapshot linkage — `docs/19`'s, separately;
- **multi-owner stores, multiple stores, or store discovery** — the store is
  launcher-wired and there is one;
- **restart policy** for the store service, which stays canonical supervisor text
  (`ADR-0077` §8);
- **Stage 4 closure.** Stage 4C, Stage 4D and Stage 4 do not close here.
