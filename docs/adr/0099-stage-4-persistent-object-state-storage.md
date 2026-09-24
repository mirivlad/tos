<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0099: Stage 4 persistent object/state storage — a private native store, substrate first

- Status: **Accepted** (Project Architect-approved, 2026-09-24) and **implemented**
  on 2026-09-25. §13's evidence is `source/host-tools/qemu-test/state-store.sh`: six
  canonical modules, eight processes, the reference VirtIO device, and all seven
  required mutations verified to turn it red. **Stage 4C, Stage 4D and Stage 4 do
  not close here** (§14)
- Date: 2026-09-23, accepted 2026-09-24. **§13a's bound accounting was corrected on
  2026-09-25** (ADR-0101 §6), after implementation found that the initializer needs an
  answer inbox of its own: five endpoints of six and three startup endowments for the
  initializer, not four and two. `MAX_ENDPOINTS` and `MAX_ENDOWMENT` do not move, and
  no other part of this decision changes
- Decision level: **3** — architectural, **requiring Project Architect
  approval**. `docs/21` places *"changes persistent formats"* at Level 3, and
  `ADR-0017` applied that test to itself in as many words — *"Explicitly **not**
  Level 3: no capsule byte changes"*. This decision **creates** the first
  normative persistent state format, which is not less architectural than
  changing one. §12 is the architecture impact statement `docs/21` requires
- Project Architect approval: 2026-09-24, **as a Level 3 architectural decision**,
  after the corrective round that split formatting from startup, made the
  initializer refuse any non-zero header, bounded `schema_identifier`'s scope, and
  corrected the receiver/reclamation wording of §13a
- Depends on: **ADR-0098**, accepted on the same date and necessarily before this
  one. This decision's store reaches the device only through `block.device.v1`, and
  building it on the lifecycle fixture's deliberately non-normative encoding would
  make a fixture a production dependency
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
protocol, and how the `docs/35` handoff budget is measured. The contract is
`source/interfaces/state/STATE_STORE_V1.md`.

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

### 2a. How an answer channel is handed over, and what it costs

**A delegation carries the rights the sender holds** (`IPC_V1` §6) and **one
endpoint has one receive-rights holder at a time** (§2), so a `GET` cannot hand over
the name its caller receives on: the accepting receive would refuse the whole
message, and both processes would wait for each other. That is not a hypothesis —
`block-protocol` deadlocked exactly there.

**So the channel handed over is a transient send-only alias**, made by ADR-0100's
`capability_attenuate` row on `system.ipc.Endpoint`:

```text
state-inbox = send | receive          one startup grant, and it stays one

for every block READ:
    reply_to = capability_attenuate(state_inbox, RIGHT_SEND)
    result   = endpoint_call_word_carrying(reply_to, block_service, request)
    capability_release(reply_to)
    then interpret result
    if success: receive the region on state-inbox
```

**The alias is released whether the call succeeded or not**, and before the result is
interpreted. It has done its one job either way — the callee holds its own name now,
because a delegation of a non-affine object copies rather than moves — and a store
that only tidied up on the happy path would leak exactly when it was under stress.

A client's `GET` is the same pattern on `client-inbox`.

**The accounting, stated so that a later reader does not have to derive it:**

- **startup endowments do not move.** The state service keeps **four**
  (`budget`, `state-serve` receive, `block-serve` send|call, `state-inbox`
  send|receive) and the client keeps **three** (`budget`, `state-serve` send|call,
  `client-inbox` send|receive);
- **`MAX_ENDOWMENT` remains 4.** The alias is not a startup grant and never appears
  in a launch plan;
- **each outstanding request temporarily occupies one additional capability-table
  entry**, and exactly one;
- **it does not accumulate**, because v1 permits at most one outstanding `READ` or
  `GET` per answer endpoint (`BLOCK_DEVICE_V1` §6a, `STATE_STORE_V1` §8b). One
  outstanding request, one alias, released before the next;
- **the receiver identity stays one process.** Attenuation grants to the holder that
  already held it, so `IPC_V1` §2 is not strained — the nucleus reads a *holder* as a
  process, and both names are the same process's.

**Nothing about the persistent layout or the wire encoding changes.** This is how a
channel is obtained, not what travels on it.

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

    READ HEADER_SECTOR

    if all 512 bytes are zero:
        WRITE one complete initial STATE_STORE_V1 header
        terminate success
    else:
        REFUSE TO FORMAT
```

The header it writes is exactly: magic, `format_version = 1`, the owner's schema
identity, `occupancy = 0`, `reserved` zero. It writes **no** payload sector.

**It reads before it writes, and only an all-zero header is permission to
format.** An initializer that wrote unconditionally would silently reset
`occupancy` if it were ever run against an existing store — losing every object in
it while reporting success. For a provisioning action that is deliberately
separated *because* it is destructive, that is not acceptable.

**The test is deliberately stricter than §6b's ordinary validation**, and the
difference is the point. Ordinary opening asks "is this a store I can use?";
formatting asks "is this certainly nothing at all?". So none of the following is
permission to initialize:

- a header that is **already valid** — the store exists;
- one valid **but for another schema** — it is somebody else's store, and
  `schema_identifier` is not a claim on the extent (§7a);
- one of a **future or unknown `format_version`** — a v1 initializer cannot know
  what it would be destroying;
- a **corrupt** header — the most likely reading is a store whose header failed to
  read, which is exactly the case §6's opening paragraphs refuse to paper over;
- **merely non-zero garbage** — something put bytes there, and this decision
  assigns no meaning to bytes it did not write.

Only the all-zero sector denotes the fresh, uninitialized reference state, which is
also what the Stage 4 harness presents on a new image.

**It does not inspect sectors 1..64 before formatting, and must not.** The harness
deliberately seeds some of them, so their contents say nothing about whether a
store exists; `occupancy` is authoritative, and `occupancy` is only meaningful once
a store does.

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

### 7a. What `schema_identifier` is, and what it is not

**It creates no global schema-ID namespace**, and stating that is the point of this
subsection — an identifier written into a persistent format is exactly the kind of
field that acquires a registry nobody decided on.

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

`schema_version` versions that owner's schema **within** that lineage; the pair is
what §6b compares on opening, and a mismatch is a refusal rather than a
conversion.

At Stage 4 there is exactly **one owner and one store**, so no registry, no
allocation mechanism and no uniqueness rule is needed — and none is created. A
later multi-owner or multi-store design may need a stronger identity contract, and
**that is not decided here**.

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
initializer      capacity() >= 65; reads sector 0; finds it all-zero;
                 writes one initial header; terminates
                 its ending is collected
state store A    opens and validates the existing header
writer           put(1, pattern A); put(2, pattern B)
                 its ending is collected
state store A    ends; is retired; the supervisor collects its ending
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
| `MAX_ENDPOINTS` | 6 | **5**: `block-serve`, `state-serve`, `state-inbox`, `client-inbox`, `init-inbox`. One spare |
| `MAX_ENDOWMENT` | 4 per plan | block **3** (`budget`, `block-serve` receive, `device` claim); initializer **3** (`budget`, `block-serve` send\|call, `init-inbox` send\|receive); state **4** (`budget`, `state-serve` receive, `block-serve` send\|call, `state-inbox` send\|receive); client **3** (`budget`, `state-serve` send\|call, `client-inbox` send\|receive). **Startup grants only**: the send-only alias a `GET` hands over is transient and is not one of these (§2a) |
| `MAX_CAPABILITIES` | 16 per process | the supervisor is the only one near it, as in `block-lifecycle`; its peak must be counted during implementation and child controls released after each collection |

**One client plan for the writer and the reader**, as directed: they are
sequential, and the writer simply does not use the `client-inbox` the plan grants
it — a `PUT` is one atomic call answered by a word, so only the reader needs an
inbox at all. An unused grant is not a defect; a plan is a policy, and
`granted()` records what was installed.

**The initializer does need an inbox, and the first count of this said it did not.**
`CAPACITY` is an ordinary call and `WRITE` is an atomic call carrying its region, so
neither is answered with a region — but the initializer has to **read** the header
before it decides whether to write one, and a `READ` is answered with a region on a
channel the asker delegates (`BLOCK_DEVICE_V1` §6a). It cannot attenuate the *block
service's* endpoint for that: that name is the service's, not an inbox, and a
send-only alias of it would deliver the answer back to the service. So the
initializer holds an endpoint of its own.

**Corrected accounting, found by building it** (2026-09-25, ADR-0101 §6):

```text
endpoints: 5 of 6      block-serve, state-serve, state-inbox, client-inbox, init-inbox
plans:     4 of 4      block, initializer, state A/B, writer/reader client
startup endowments     block 3, initializer 3, state 4, client 3
```

**This is a correction to the count and not a new topology decision.**
`MAX_ENDPOINTS` stays 6 and `MAX_ENDOWMENT` stays 4; both are still satisfied, with
one spare endpoint instead of two and the initializer one endowment below its bound
instead of two. Nothing about what the processes are, what they hold, or the order
they are created and collected in changes, and the initializer is still collected
before the ordinary state service starts.

**Sharing one plan makes the receive-holder rule load-bearing, and three separate
things must not be merged into one claim.**

| | What enforces it |
|---|---|
| **receiver exclusivity** — while state A is live and holds `receive(state-serve)`, creating state B from the same plan is **refused** | the **nucleus**. `IPC_V1` §2 admits one receive-rights holder at a time and `capability.rs` answers `NotGranted::ReceiverExists` |
| **process-slot reuse** — six instances over four slots | the **nucleus**, as `MAX_PROCESSES` |
| **the exact order "A ends, A retired, `wait_child(A)`, A's ending collected, B created"** | **canonical supervisor policy**, and the gate's journal evidence. Not the nucleus |

**The third row is not a nucleus guarantee, and an earlier draft claimed it was.**
`process::retire` sets the slot `Over` and calls `capability::clear`, so A's receive
authority is gone at **retirement** — before `wait_child` collects its tombstone. If
another process slot were already free, B could in principle occupy it before that
collection. Nothing in the nucleus makes collection the only possible ordering.

**The fixture still requires that order, as an evidence obligation on the
supervisor.** The persistence proof needs A's address space and capabilities gone
before B exists, and an explicit collection gives a far stronger journal witness
than an inference from timing — which is why the supervisor performs it and the
gate asserts it, rather than the ADR asserting that nothing else was possible.

- **the reader holds no `block.device.v1` endpoint**, asserted from the boot
  journal's capability records, not from source;
- **the successor re-reads the header from the device**, and no state crosses from
  A in memory. The nucleus refuses to create B from the shared plan while A is
  **live** (`IPC_V1` §2, `NotGranted::ReceiverExists`); the stronger ordering the
  proof relies on — A retired, collected by `wait_child`, *then* B created — is
  the **supervisor's** policy and an assertion this gate makes on the journal, not
  something the nucleus guarantees (§13a);
- **the patterns vary per byte.** The harness seeds sectors 1–4 with 512 copies of
  `0xC0 + n` and leaves sector 0 zeroed, so a constant fill and a zero fill are
  both distinguishable from either pattern — and ids 1 and 2 land on seeded
  sectors on purpose;
- **the byte comparison happens in canonical text.** The host gate judges reported
  account bits and journal order;
- **the supervisor's ordering is asserted on the journal**, as §13a requires: A
  retired, `wait_child(A)` returning, then B created.

**And one negative that is about formatting rather than persistence.** It is a
separate obligation because it proves a separate claim:

```text
run the initializer against a valid existing header
    -> it refuses to format
    -> the header and its occupancy are unchanged afterwards
```

"Unchanged" is checked by reading the header back and by a subsequent `get` of an
object the header said was present. Mutation 7 removes the zero-header check and
must turn this red.

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
   `ADR-0098` §4 obligation 9, applied to this protocol, must turn red;
7. **remove the initializer's zero-header check** — the refuse-to-reformat negative
   below must turn red.

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
