<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0099: Stage 4 persistent object/state storage — a private native store, substrate first

- Status: **Proposed** (raised 2026-09-23 on Project Architect direction; **not
  accepted, and nothing in the tree implements it**)
- Date: 2026-09-23
- Decision level: **3 on this draft's reading, and the Project Architect directed
  2.** §0a states the disagreement rather than resolving it quietly. `docs/21`
  puts *"changes persistent formats"* at Level 3, and `ADR-0017` applied that
  test to itself in as many words — *"Explicitly **not** Level 3: no capsule byte
  changes"* — so on the project's own established reading, a decision that
  **creates** the first persistent state format is Level 3 rather than Level 2.
  The practical difference is the approval ceremony: `docs/21` requires the
  nine-question impact statement at Level 2 **and** above, and it is in §12
  either way. The level is the Project Architect's to set on approval
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

## 0a. The level, stated rather than assumed

The Project Architect directed Level 2. This draft reads `docs/21` as putting it
at Level 3, and records the reasoning so the ruling is made on the argument
rather than on a classification nobody looked at:

- `docs/21` §Level 3 is *"Moves trust boundaries, **changes persistent formats**,
  introduces a runtime dependency, changes source identity or modifies owner
  control"*;
- `ADR-0017` is the project's own application of that clause. It classed itself
  Level 2 with the explicit reason *"Explicitly **not** Level 3: no capsule byte
  changes"*, which reads the clause as: touching the persistent bytes lifts the
  level;
- `ADR-0020`, also Level 2, said the same from the other side — it accepted
  contracts *"without changing a Tier 0 invariant, runtime trust boundary,
  **persistent byte layout** or implementation behavior"*;
- this decision **creates** a persistent byte layout where none existed. Creating
  the first one is not less consequential than changing one.

Nothing else about the decision changes with the answer. It adds no ABI
operation, no capability kind, no object kind, no nucleus change and no trust
boundary movement either way, and §12 answers the nine questions that Level 2 and
Level 3 both require.

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
client  --(state.store.v1)-->  state store service  --(block.device.v1)-->  block service  -->  device
```

- **The state store service is launcher-wired.** It receives its endpoints from a
  sealed launch plan (`ADR-0077` §3–§5), which is what `ADR-0093` §3a answer 3
  already requires of every client's first capability. **It does not publish**,
  and this decision introduces **no second publication class** — `ADR-0095` §6
  leaves that undecided and §8 states that the spare endpoints are not an
  argument for it;
- **for this Stage 4 slice it is the sole ordinary client endowed with the block
  service's endpoint.** Clients of the store hold no `block.device.v1` capability
  at all, which is what makes "the reader could not have reached a sector" a fact
  about the boot's capability topology rather than a claim about its source;
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

so the store occupies sectors `0..64` — 65 sectors — and owns the device from
sector 0. There are no partitions at Stage 4 and one device, which is why a base
offset would be a field with one possible value.

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

## 6. Initialization and opening

```text
capacity()                         must report at least 65 sectors
read sector 0 through block.device.v1
```

- **an all-zero sector 0 is not a valid store.** Uninitialized storage is
  uninitialized, and a zero-filled device is exactly what the Stage 4 harness
  presents on a fresh image;
- a **first-generation** service may **initialize** a new store by writing one
  complete valid v1 header — magic, `format_version = 1`, its own schema identity,
  `occupancy = 0`, reserved zero;
- **before initializing, it calls `capacity()` and refuses to initialize if the
  device cannot contain the whole bounded v1 layout.** This is why `ADR-0098`
  keeps `capacity` in v1: a bounded store must establish that its layout fits
  before it writes anything, rather than discovering the bound by issuing a
  request built to be refused;
- a **successor** opens **only** by reading and validating the header from the
  device. It does not initialize, and nothing is handed to it in memory. Validation
  is: magic equal; `format_version == 1`; `reserved` all zero; and
  `schema_identifier` and `schema_version` equal to its own declared constants.

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

- **`PUT` is one atomic call** carrying the request word and one immutable
  512-byte-covering region, through `ADR-0098` §2a's `endpoint_call_word_region`.
  It is **not** a region message followed by a request, for `ADR-0098` §1's
  reason;
- **`GET` is a call carrying the client's answer endpoint**, and the service sends
  the immutable region there **before** replying success;
- the request encoding, refusal codes, region-size rule and id bound are fixed in
  `STATE_STORE_V1`.

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

**Header construction is metadata, not payload.** The store allocates its own
512-byte region for the header, composes it, freezes it and writes it as **its
own block request** — separately accounted, and never mixed with a payload
region.

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
state store A   opens, and on a fresh device initializes the store
writer          put(1, pattern A); put(2, pattern B)
writer ends     and its ending is collected
state store A ends   and its ending is collected and its slot reclaimed
state store B   the same canonical module; reads and validates the header from the device
reader          get(2); verifies all 512 bytes of pattern B in canonical text
```

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

1. **omit the persistent header write** — the successor cannot open the store, or
   cannot find the object;
2. **omit or falsify the real payload device write** — the final byte witness
   fails;
3. **answer `get(2)` from object 1's sector** — the final byte witness fails;
4. **ignore the occupancy bitmap** — `get(3)`, an id never created, must be
   refused as absent; the mutation makes it answer with sector 3's seeded
   `0xC3` fill instead, and the negative gate must fail.

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
