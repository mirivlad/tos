<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — persistent object/state storage: the boundary before the decision

- Status: **note, not a decision.** Tier 4 under `docs/38`: research and
  explanatory material, incorporated by no ADR.
- Date: 2026-09-23, **corrected 2026-09-23** after Project Architect review of
  the first revision (commit `011f41c`), and **closed 2026-09-23** by the
  rulings §0b records. §0a records what was withdrawn.
- Audience: the Project Architect, before any persistent-storage implementation
  is begun and before the decision surface in §15 is ruled on.

## 0. What this is

The next Stage 4 deliverable under `docs/16` is **persistent object/state
storage**, with the stage's engineering exit *"persistent storage works through
a textual user-space driver"*. This note answers what that requires, from
accepted clauses only, and stops at the point where a decision is needed.

**It accepts nothing and recommends without deciding.** Where an answer is a
choice rather than a requirement, it is presented as a set of alternatives with a
recommendation and the reasons on both sides. `capsule-to-repository handoff` is
a **separate** `docs/16` deliverable and is treated as separate throughout (§14).

## 0a. What the first revision claimed too strongly, and what replaces it

Four conclusions in the first revision were stated as forced by the accepted
architecture when they are not. They are withdrawn here, and the sections that
replace them present alternatives instead.

| Withdrawn claim | Why it was wrong | Replaced by |
|---|---|---|
| *"a `u64` is the only identifier the accepted transport can carry"* | inline IPC exposes only `word`, but `endpoint_receive_region` yields a `Region<u8>` whose every byte canonical text can read. A bounded byte name is expressible today | §4, two identity families compared |
| *"a persistent object store cannot exist without a persistent object-to-location index"* | deterministic placement needs no allocation map. The claim confused *a store must have persistent bytes with decided meaning* — which holds — with *those bytes must be an index* — which does not | §5, three placement options |
| *"`docs/09` and I-09 require a schema identifier and version in the bytes"* of every object | they require a durable service to *declare* schema identity and a versioned boundary. **Where** that identity is persistently recorded is a separate question | §6, three versioning locations |
| *"a four-layer stack crosses more than four handoffs, so it conflicts with `docs/35`"* | architectural boxes are not scheduler handoffs, and the budget's measurement endpoints are not stated. No violation is established | §12, the interpretation question |

One blocker the first revision missed is added: **`block.device.v1` has no
accepted wire protocol** (§7). And one exclusion it stated as derived is restored
to a decision: **`/state`** (§9).

The findings in §16 are unchanged and were accepted.

## 0b. The rulings, 2026-09-23: which alternatives are closed

**The research phase is over.** The Project Architect ruled on every open choice
below, and the decisions are drafted as **ADR-0098** (`block.device.v1`) and
**ADR-0099** (Stage 4 persistent object/state storage), with proposed contracts
`docs/proposed/BLOCK_DEVICE_V1.md` and `docs/proposed/STATE_STORE_V1.md`. The
alternatives are **kept** below as the record of what was weighed; they are no
longer offered.

| Question | Ruling | Where it is decided |
|---|---|---|
| §4 object identity | **A** — a store-local `u64`, bounded to `1..64`; not a path, not a content id | ADR-0099 §3 |
| §5 placement | **C, corrected** — deterministic placement + a versioned store header **with an occupancy bitmap**, not an object count | ADR-0099 §4, §5 |
| §6 versioning location | **the store header**; not per-object headers, not source-only identity | ADR-0099 §7 |
| §7 the block protocol | **all three operations in v1** — `READ`, `WRITE` and `CAPACITY`. `capacity` is *not* deferred: a bounded store must prove its layout fits before writing, rather than discovering the bound by issuing a request built to be refused | ADR-0098 §2c |
| §7 the two-message write | **not promotable.** `WRITE` is one atomic call carrying word and region, over the region support ABI operation 3 already has | ADR-0098 §1, §2a |
| §8 the store protocol | **a versioned userspace protocol**, `state.store.v1`, exposing `put` and `get` only | ADR-0099 §8 |
| §9 `/state` | **A — substrate first.** `docs/09`'s `/state` is unchanged and is not implemented here | ADR-0099 §1 |
| §12 the handoff budget | **per completed block request**, at the immediate block interface; a store operation's several block requests are each subject to it independently, and end-to-end remains observational | ADR-0099 §11 |

**A second round of rulings, 2026-09-24**, on review of the drafts. Four more
things this note said or implied are superseded:

| What the note said | The ruling |
|---|---|
| §15 classed ADR-β **Level 2** | **Level 3.** `docs/21` places persistent-format changes there, and creating the first normative persistent state format is not less architectural than changing one. ADR-0098 stays Level 2 |
| §4's identity discussion leaned on the endpoint object as the thing that fixes a protocol | **withdrawn.** `ADR-0095` §3's object fixes a publication class and authority; the endpoint later published as a *service* endpoint is a different object, and `block-lifecycle.sh` already runs two distinct service endpoints for one `block.device.v1`. Requests still carry no in-band version field, but because the service contract is configured by topology and v1 has no runtime negotiation |
| §7's read shape replied **after** sending the region | **reversed.** Control precedes data: obtain, reply, then send exactly one region. The old order admits an orphan region left queued on an endpoint that outlives the process that was waiting for it, and endpoint objects live for the boot |
| §5's placement option C initialized the store from its first generation | **split.** A child is never told its restart generation, so a service cannot tell a fresh start from a restart whose header failed to read, and must not auto-format. Formatting is a separate short-lived canonical **initializer** process; the ordinary state service only opens |

The bound sketch in §11 and the evidence sketch in §13 are both superseded by
`ADR-0099` §13a and §13, which count seven process instances including that
initializer against `MAX_PROCESSES = 4`, `MAX_PLANS = 4` and `MAX_ENDPOINTS = 6`
without moving any of them.

**Two things this note got wrong in its corrected revision, found while
drafting**, and both are recorded here rather than left in the sections above:

- **§5's option A is weaker than the note said, for a further reason.** The
  occupancy bitmap the ruling added is what makes deterministic placement sound:
  without it, presence would have to be inferred from bytes, and the Stage 4
  harness's own seeded sectors are a demonstration that bytes on a device prove
  nothing about who put them there. `ADR-0099` §4 makes stale content
  non-authoritative by decision.
- **§7's "`capacity` is not needed" was a convenience.** It rested on the store
  learning the device's bound from a refusal — i.e. on a client establishing a
  fact by issuing a request it expected to fail, which is indistinguishable from
  a client with a bug. The ruling includes `capacity` in v1 and `ADR-0098` §2c
  states that reason.

## 1. Accepted requirements

**The deliverable and the exit.** `docs/16` §Stage 4 lists seven deliverables, of
which two are relevant and distinct:

> - persistent object/state storage;
> - capsule-to-repository handoff;

and the stage's engineering exit is *"persistent storage works through a textual
user-space driver"*, with the identity exit *"the textual driver performs actual
I/O from canonical source; no binary shadow driver or hidden host path exists"*.

`docs/11` §Bootstrapping puts the two in order and in the same words: step 4
*"Text driver initializes persistent storage"*, step 5 *"Repository-backed
versions replace capsule versions through a versioned handoff"*.

**What `state` means.** `docs/09` §`/state` defines the namespace class as
*"Mutable durable state owned by services"*, and adds the rule that matters for
identity: *"State paths are namespaced by service identity and protected by
capabilities."* Its examples are service-level — databases, queues, leases,
indexes, update transaction records, session metadata.

**What the first implementation is permitted to be.** `docs/09` §Filesystem
implementations: *"The first implementation may use a simple native object store
and state filesystem under QEMU. The VFS and capability contracts must not
assume a particular disk format."* Two grants in one clause — a simple native
store is admitted, and no contract may bake a disk format into it.

**What a service with durable state must declare.** `docs/09` §State schema
versions requires, of *every* such service: a state schema identifier and
version, compatible source-module versions, migration functions, downgrade
policy, snapshot requirements, and a maximum supported migration chain. **It says
what must be declared, not where it is stored**; §6 is that question.

**How persistent numbers must be written.** `docs/40` §primitive types: `size`
*"MUST NOT be serialized in a persistent/public format … Public and persistent
forms use one of the explicit fixed-width integers."* This is retained without
qualification: every serialized number is an explicit fixed-width little-endian
integer.

**What the persistence boundary is.** `ADR-0092` §0 records the Project
Architect's selection of **Branch A** on 2026-09-21:

> Stage 4 proves that the write reached the device, that a later read sees the
> change, and that the data remains available across a stop and restart of the
> block service. Power-loss durability, `VIRTIO_BLK_F_FLUSH` and
> ungraceful-termination evidence are **not** Stage 4's.

**What an ADR is already owed.** `docs/19` §Decisions still requiring ADRs lists
**"first persistent object/state filesystem"** and, separately, **"state snapshot
mechanism"**.

**Invariants that bear on it.** I-04 (runtime state does not implicitly modify
the active system commit), I-09 (*"Boot ABI, IPC schemas, repository metadata,
capsule format, driver contracts, language frontend contracts, and cache formats
are versioned from their first implementation"*), I-13 (no milestone by a known
throwaway path), I-16 (source-to-runtime traceability).

**The performance contract.** `docs/35` §Stage 4, hard budgets *after queue
initialization*: *"no more than one payload copy between client memory and
device-visible memory"* and *"no more than four address-space/scheduler handoffs
per unbatched request"*. The first is retained as a design constraint (§13); the
second is an interpretation question (§12).

## 2. Existing mechanisms

**Block read and write.** A canonical textual service claims one VirtIO block
function and performs real `VIRTIO_BLK_T_IN` and `VIRTIO_BLK_T_OUT` of one
512-byte sector per request (`block-data-path.sh`, `block-lifecycle.sh`). Its
driver reads the device's 64-bit `capacity` from config space under §2.5.1's
generation protocol and refuses a sector the device does not have
(`virtio-block-write`). **`capacity` is not an IPC operation**: no accepted
service protocol exposes it, and §7 is that gap.

**Ordinary Region IPC.** `ADR-0097` and `SYSTEM_INTERFACE_V1`: one **immutable**
ordinary `Region<u8>` crosses a message in the region area, **linearly** — a
successful send takes the sender's handle and its mappings atomically
(`ADR-0075` §5a); a `Region<mut u8>` may not be sent at all (`IPC_V1` §5,
`ADR-0037`). Regions are originated by `region_allocate` and made sendable by
`region_freeze`. `endpoint_receive_region` *"exposes one region and nothing
else: no count, no iteration, no second slot, no payload, no capability and no
reply"* — so **one region per message**, and two regions in one message is
explicitly *"a later decision"*.

**What a receiver can read.** `SYSTEM_INTERFACE_V1` §4.2:
`ReceivedCall{reply, carried, length, word}` where `word` is *"the first eight
payload bytes, little-endian"*, and `Answer{length, word}`. No operation reads a
payload back as `bytes` or as a `string`. **But canonical text can index every
byte of a received `Region<u8>`**, which is how 512 bytes are already verified in
`block-lifecycle`'s client. So the readable request surface of one *message* is
one `u64`, one delegated capability, one reply — and, in a second message, 512 or
more bytes of arbitrary content.

**What a receiver cannot learn.** `ReceivedCall` carries **no caller identity**.
A service cannot attribute a call to a process, a module or a source identity.
This bears directly on `docs/09`'s *"namespaced by service identity"* (§4).

**Process and service restart.** `process_create_funded` from a sealed
`LaunchPlan`, `process_wait_child` → `ChildEnding`,
`process_create_with_generation` for a lineage. `PROCESS_IDENTITY_V1` §4: a
restart produces a new instance id and increments the restart generation, keeping
module and supervisor lineage.

**What canonical text can represent as persistent bytes.** A `Region<mut u8>` it
allocated and filled byte by byte, frozen and sent. Element type `u8` and only
`u8` (`ADR-0085` §18, `ADR-0097` §8b). Numbers are serialized by hand as explicit
fixed-width little-endian bytes.

**Stable identity mechanisms that already exist.**

| Mechanism | Where | Stable across | Reachable from canonical text |
|---|---|---|---|
| canonical absolute path | `CAPSULE_FORMAT_V1` §4.1 — `/`-rooted, UTF-8, no NUL, no control characters, no `.`/`..`, no empty components, sorted, distinct | the capsule's life | as a **rule** yes; as a *value* it is a build-time name, and only a region can carry one at runtime |
| `content_digest` (SHA-256 of content bytes) | `CAPSULE_FORMAT_V1` §4.2 | content | **no** — no accepted schema declares a hash operation, and `docs/19` still owes the cryptographic-algorithm ADR |
| module name + source content id (`sha256:` over normalized source) | `PROCESS_IDENTITY_V1` §3, launcher-asserted | restart, boot | **no** — a launch record, not a value a module holds |
| process instance id | `PROCESS_IDENTITY_V1` §3 | *"unique for the life of the boot; never reused"* | yes, as `CreatedProcess.instance` |
| restart generation | `PROCESS_IDENTITY_V1` §3–§4, supervisor-asserted | the lineage | yes, in `ChildEnding` |
| capability handle | `CAPABILITY_V1` §7 | nothing — *"an index in one table and means nothing in another"* | yes |
| binding name | `ADR-0061`, `endow_for_launch`'s `binding: string` (≤ 64), `MAX_BINDING = 64` | the launch | written, never read back |

**There is no accepted persistent namespace and no accepted persistent object
identifier.** `docs/09` names the namespace *classes* and no implementation;
`source/interfaces/` contains no storage contract of any kind;
`OBJECT_INTERFACE = 4` stays reserved and empty (`ADR-0093` §2, §4).

**What survives only in RAM today.** The publication registry's entries
(`ADR-0093` §3b: *"Its registry does not survive its own death"*); every nucleus
table — processes (4), capabilities (16 per process), endpoints (6), plans (4),
queue depth (4), regions, assignments and their generations; every `Region<u8>`;
every launch plan (`plan.rs`: a plan ends *"by an explicit release of its
capability, or by the death of the process that held it"*); the supervisor's
journal, which leaves as serial text and is read back by nothing textual;
`PROCESS_IDENTITY_V1`'s whole identity plane; and the disk image itself, because
`run.sh` does `rm -f "$STAGE4_IMAGE"` and recreates it on **every** run.

**Existing decisions that defer filesystems, repositories, partitions, caches.**
`ADR-0093` §0 (*"No filesystem, no partitions, no cache, no object store, no
enumeration framework, no multi-device management"*) and §9 (which names
*"Persistent object/state storage and the capsule-to-repository handoff"* as
Stage 4 deliverables **not in that slice**); `ADR-0097` §5; `ADR-0048`
(Stage 3 authorizes *"no filesystem, no repository-backed `/system`"*);
`ADR-0072` §6; `ADR-0030` (`/vendor` needs no implementation before the stage
that needs it).

## 3. The semantic gap

**The block service already provides durable-across-restart byte storage.**
`block-lifecycle.sh` writes 512 bytes through a service, kills it, starts a
successor, and reads the same bytes back. The gap is therefore not durability and
not device work. It is four things:

1. **No decided meaning for any persistent byte.** Nothing accepted says what
   any byte of any sector means. A store must decide *something* — what is
   withdrawn is the claim that the something must be an index (§5).
2. **Indirection.** A client names an object; today every client computes a
   sector and therefore knows the device's geometry.
3. **Plurality.** With one object an index or a slot map is unfalsifiable. Two is
   the smallest number that makes identity load-bearing.
4. **No normative protocol underneath.** The store's only way to reach the device
   is a service protocol that does not exist: `ADR-0093` §9 leaves the wire shape
   of `read`, `write` and `capacity` undecided, and the lifecycle fixture's
   `sector * 2 + direction` is documented as fixture encoding. §7.

## 4. Object identity: two families

Both are expressible with the accepted ABI today.

```text
A. bounded numeric object id, carried in `word`
B. bounded byte name, carried in a `Region<u8>`
```

**What each costs in messages.** A `put` today is already two messages — region,
then `endpoint_call_word_carrying` (`block-lifecycle`'s client). Under A that is
unchanged: the id rides in `word`. Under B the name needs a region of its own,
because `endpoint_receive_region` exposes exactly one region per message and two
regions in one message is *"a later decision"* — so a `put` becomes three
messages whose two regions are told apart **by arrival order alone**, or the name
must be packed into the same sector as the payload, which stops an object from
being 512 bytes of content. Neither is forbidden; both are protocol cost.

| Criterion | A — numeric id | B — byte name |
|---|---|---|
| `docs/09` *"State paths are namespaced by service identity and protected by capabilities"* | the namespacing is done by **which store capability a service holds**, not by the name. That satisfies the "protected by capabilities" half exactly and leaves "paths" unrepresented | a name is closer to a path, and is a partial step toward `/state`. But **the store cannot learn who is calling** (`ReceivedCall` carries no caller identity), so per-service namespacing still has to come from capabilities. B does not buy the clause; it only looks like it does |
| Stage 4 scope | minimal; adds no name rules, no comparison, no normalization | adds a name grammar, a length bound, byte-equality comparison in canonical text, and a normalization question that `CAPSULE_FORMAT_V1` §4.1 answers for paths and nobody has answered for state names |
| capability isolation | identical under both: isolation is the store endpoint, and a holder can name anything in the id space it is given | identical, plus a new failure mode — two services agreeing on a name string collide where two id spaces would not |
| boundedness | trivially bounded: a declared maximum id | needs a maximum length (`MAX_BINDING = 64` is the accepted precedent) and a bounded character rule |
| later `/state` implementation | a numeric id has no relation to a path, so a later `/state` needs a name→id resolution layer. That layer is **additive**, not a format break, if the store's format is versioned | a byte name is the thing a path component is made of, so B is less work later — at the cost of fixing name semantics now, before `/state` is decided |
| separation from Stage 5 content addressing | a `u64` cannot be mistaken for a digest | a 32-byte name **can** be mistaken for one, and the ADR would have to say in words that a state name is not a content id |

**Recommendation: A, as a choice.** Its reason is scope, not impossibility: it
adds no name semantics at a stage that has not decided `/state`, it cannot be
confused with `docs/08`'s content identity, and the work B saves later is a
resolution layer that a versioned format makes additive. **B is a legitimate
answer** and would be the better one if the ruling on §9 is that Stage 4 must
expose a `/state` namespace — in which case the name should follow
`CAPSULE_FORMAT_V1` §4.1's rules rather than invent a second grammar.

## 5. Placement: three options

```text
A. deterministic placement, no persistent metadata at all
C. deterministic placement + a store header carrying format identity
B. a persistent allocation/index mapping
```

| | A — pure deterministic | C — deterministic + header | B — persistent index |
|---|---|---|---|
| mapping | `sector = base + id`, a constant of the service's text | same | `id → sector` read from the store |
| what survives a restart | nothing needs to: placement is in the source | the store's format identity, version and object count | the mapping, the allocation cursor, the count and the format identity |
| which persistent bytes acquire meaning | object payload sectors only | payload sectors **and one header sector** | payload sectors **and one or more index sectors** |
| can `get` distinguish "never written" from "written as zeros"? | **no** — and this is A's real cost. Absence is unrepresentable unless a per-object marker is added, at which point A has acquired persistent metadata after all | yes, from the header's count or slot bitmap | yes |
| credible as a non-throwaway first architecture (I-13) | defensible — a fixed table is a real design for a bounded system, and `docs/09` admits *"a simple native object store"*. Weakened by the previous row: the first thing it needs is the metadata it claims to avoid | yes. It is a bounded store that says what it is, and I-13 asks that the demonstration exercise the real contract, not that the store be general | yes, and it is the shape a store keeps long-term |
| what future change forces a format break | variable-size objects, deletion and reuse, more objects than slots, or an id space larger than the sector range — **any of them**, because there is no version field to negotiate from | the same changes, but they are **additive**: the header's version field is what lets a later format be recognized rather than misread | multi-sector indexes and extents are additive; the version field is present from the start |
| evidence distinguishing object identity from direct sector access | **weak, and honestly so**: under deterministic placement the id *is* an affine transform of a sector number. What a gate can still prove is that the client holds **no** capability naming the block service, so it cannot reach a sector at all — a capability fact, readable from the boot journal | same as A, plus one observable header read at open, and an "omit the header write" mutation that turns the gate red | **strong**: the reader cannot compute where an object lives, and omitting the index write makes the store unopenable |

**Recommendation: C, as a choice.** It is the smallest option that is
*self-describing*, which is what I-09 asks of a boundary from its first
implementation, and it is the smallest option in which `get` on an unwritten id
has an answer. It buys that with one sector and no allocation semantics — no free
list, no cursor, no reuse policy, and therefore no two-write ordering question at
a stage where power loss is out of scope.

**The honest case against C and for A** is that the header is one more write and
one more thing to be wrong; **the case for B** is that it is the only option
whose evidence proves indirection rather than proving the absence of a device
capability. If the Project Architect wants the persistence claim to rest on
indirection rather than on capability isolation, B is the right answer and its
extra cost is a cursor.

## 6. Where schema and format identity live

`docs/09` requires a durable service to **declare** a state schema identifier and
version, compatible module versions, migration functions, downgrade policy,
snapshot requirements and a maximum migration chain. I-09 requires the boundary
to be versioned from its first implementation. Neither clause says where the
identity is recorded.

| Location | What it covers | Sufficient? |
|---|---|---|
| one store/superblock header | the store's format version and the schema identity of its single producer | **yes for Stage 4**, where there is one store, one schema and one producing service. One read at open, no payload cost |
| per-object headers | heterogeneous objects, each self-describing | not required at Stage 4, and it costs payload out of the 512 bytes that the accepted one-sector region path makes an object. Additive later if the store header carries a version |
| externally associated identity — the service's source declares it, nothing on disk | nothing, after a revision | **insufficient, and this is a determination rather than a preference.** `docs/09`'s own machinery — migration functions and a *maximum supported migration chain* — presupposes reading the version the bytes were written with. Source-declared identity tells you what the running service expects, never what is on the device, so the first source revision makes the on-disk bytes unidentifiable. I-16's traceability points the same way |

**Recommendation:** the store's format and schema identity go **in a store
header**; per-object headers are not required at Stage 4; and external-only
identity is ruled out by `docs/09`'s migration requirements rather than by taste.
`docs/40`'s fixed-width-integer rule applies to every field of whatever is
chosen. The ADR decides the location.

## 7. The missing blocker: `block.device.v1` has no accepted wire protocol

**`ADR-0093` §0 fixes the surface and §9 explicitly withholds the shape.** The
surface is *"`read`, `write`, `capacity` and nothing else"*; §9 lists as not
decided *"The wire shape of `read`, `write` and `capacity`. Interface design
inside `IPC_V1`; no new IPC mechanism, capability type or ABI operation is to be
created for it."* The lifecycle fixture's eight payload bytes carrying
`sector * 2 + direction` are documented — in the fixture, in README and in
PROGRESS — as **one fixture's encoding**, and no normative protocol follows from
their existence.

**A persistent-state service cannot depend on a deliberately non-normative
encoding.** Under I-09 `block.device.v1` is a driver contract, and driver
contracts are *"versioned from their first implementation"*; `docs/11` §Driver
interfaces names `block.device.v1` as a device-class interface whose purpose is
*"rather than exposing hardware-specific details to applications"*. A second
client — the store — is exactly the event that turns a fixture encoding into a
public boundary.

**What persistent storage minimally needs, and nothing more:**

```text
read(sector)            -> Region<u8>   (512 bytes)
write(sector, Region<u8>)               (512 bytes, acknowledged)
```

**`capacity()` is not needed by the minimum store**, and the reason is a fact
about the existing driver rather than an omission: it already reads the device's
64-bit capacity from config space and refuses an out-of-range sector, so a store
learns the bound from a refusal. Leaving `capacity` undecided keeps `ADR-0093`
§0's surface as a maximum rather than a mandate. If the ruling is that it should
be included, it is one request word and one `u64` answer and costs nothing —
`Answer{length, word}` already carries a `u64` result.

**How it maps onto accepted mechanisms, with nothing new:** a write is the two
messages the fixture already uses — `endpoint_send_region`, then
`endpoint_call_word_carrying` delivering the request word and the caller's reply
channel; a read is one `endpoint_call_word_carrying`, answered by
`endpoint_send_region` on that channel because *"a reply cannot carry a region"*.
The request word needs room for a sector and an operation. `sector * 2 + dir` is
the fixture's minimum; **`sector * 16 + opcode`** leaves room for `capacity` and
for later operations at no cost, and is worth considering precisely so that
adding one does not change the encoding of the two that exist. That is a contract
detail, not a decision this note makes.

**Where the decision belongs: a small preceding contract, not the
persistent-state ADR.** Three reasons. §9 says the wire shape is *"interface
design inside `IPC_V1`"* and not an ADR question *"unless it needs something
`IPC_V1` does not offer"* — and it does not, so the right artifact is a **Tier 2
versioned interface contract**, `BLOCK_DEVICE_V1`, admitted by `ADR-0020`'s
four-condition rule and accepted by a short ADR the way `ADR-0048` accepted
`PROCESS_IDENTITY_V1`. Second, `block.device.v1` has clients beyond this one —
Stage 4E and the Stage 5 repository both read blocks — so deriving the device's
public interface from one client's needs is the coupling `docs/11` warns against.
Third, it must be **accepted before** the store is built, or the store's
dependency is a fixture.

## 8. The persistent-state service protocol is itself a versioned boundary

If the store exposes `put` and `get`, that message shape is a public contract,
and I-09 lists *"IPC schemas"* among what is versioned from first implementation.
Three things must not be conflated:

| | Is it new? |
|---|---|
| a `SYSTEM_ABI_V1` operation | **no.** Everything uses operations 1, 2, 3, 6, 14, 17, 18, 19, 21, 22 and 23 as already accepted |
| a capability kind, an object kind, a nucleus mechanism | **no.** No new `Object` variant, no new capability representation member (`ADR-0085`'s enumeration is untouched), no nucleus change |
| a versioned userspace service protocol | **yes** — and it needs a version in its name and a contract, exactly as `block.device.v1` does. Call it `state.store.v1` for discussion |

**Being built out of generic Endpoint and Region operations does not exempt it
from I-09.** The publication path is the precedent in the other direction:
`ADR-0093` and `ADR-0095` decided a naming protocol that also added no ABI
operation, and it still took two decisions and a negative gate.

What the contract must fix: the request word encoding, which messages carry which
region and in what order, the object-size rule, the answer shape for `put` and
for `get`, the refusal statuses, and the bound on the id (or name) space.

## 9. `/state`: substrate or namespace

This is a decision and not something analysis settles. `docs/09` defines `/state`
as a namespace class; `docs/16` asks for *"persistent object/state storage"*;
`docs/19` names *"first persistent object/state filesystem"* as a decision still
requiring an ADR. Two readings are legitimate:

**A — Stage 4 delivers the private native object-store substrate; `/state` path
and VFS exposure come later.** Minimum additional semantics: none. The store is
reached by capability, its id space is private, and `docs/09`'s `/state` keeps its
meaning with no bytes yet. Support: `docs/09` §Filesystem implementations admits
*"a simple native object store"* as the first implementation, and separately
requires that *"the VFS and capability contracts must not assume a particular
disk format"* — which reads as a VFS arriving later, above a store that already
exists. `docs/11` §Bootstrapping step 4 says *"initializes persistent storage"*,
substrate language.

**B — Stage 4 exposes the first minimal `/state` namespace.** Minimum additional
semantics, honestly counted: a path-shaped name (so identity family B in §4); a
resolution step from name to object; a rule for what a `/state` path *means* with
no directories — plausibly a flat `/state/<service>/<key>` with `/` permitted
only as those two separators; and, for *"namespaced by service identity"* to be
more than a convention, a way for the store to know which service is calling —
which `ReceivedCall` cannot tell it, so it would have to be one store capability
(or one store) per service, decided here rather than later. It does **not** require
directories, POSIX semantics or a VFS, and this note does not add them to make B
look complete.

**Recommendation: A**, because B fixes name and namespace semantics before
`docs/19`'s filesystem decision is taken, and because the per-service attribution
B needs is a capability-topology decision of its own. **The ruling is the Project
Architect's**, and if it is B, §4's family B and one store capability per service
follow from it.

## 10. The persistence boundary

| Boundary | Required? | Reachable today? |
|---|---|---|
| the writing **client** dies | yes, implied | yes |
| the **persistent-state service** dies and a successor opens the store | **yes — this is the deliverable** | yes |
| the **block service** dies | yes | yes, already proved by `block-lifecycle.sh` |
| whole TOS reboot in one QEMU run | **no** | **no**: the nucleus has no reboot path; a boot ends at `TOS.HALT` |
| a second QEMU run against the same image | no | **not today**: `run.sh` deletes and recreates the image each run. One harness flag would change that |
| power loss, `VIRTIO_BLK_F_FLUSH`, ungraceful termination | **explicitly excluded** (`ADR-0092` §0) | — |

The deliverable adds the one restart that matters most — the **state service's
own**, because that is where a mapping or a header has to be re-read from the
device.

**A preference, labelled as one.** Retaining the image across two QEMU runs and
booting a second time with no writer at all would close the "RAM retained"
confounder absolutely and costs one flag in `run.sh`. It is not required by
Branch A and still claims nothing about power loss.

## 11. Does the layering hold?

```text
canonical textual client  →  persistent object/state service  →  textual block
service  →  VirtIO block  →  device
```

**It holds, and the state service acquires no hardware authority.** It holds a
memory authority, its own receive endpoint, `send | call` on the block service's
endpoint, and a reply channel — no PCI bus, no function, no MMIO window, no
interrupt source, no DMA region. The block service remains the only holder of
those.

**Two accepted bounds nearly refuse it, and one wiring choice is load-bearing.**

- **A second published interface is not decided.** `ADR-0095` §6 leaves it
  undecided and §8 says the spare endpoints are not an argument for it. So the
  state service should **not** publish; its endpoint is launcher-wired from a
  sealed plan, which is what `ADR-0093` §3a answer 3 already requires of every
  client's first capability. Case C is accepted and is not re-proved.
- **`MAX_PLANS = 4`, and canonical text cannot release a plan.** `plan.rs` sets
  `MAX_PLANS = MAX_PROCESSES`; `SYSTEM_INTERFACE_V1` declares no
  `capability_release` row on `system.process.LaunchPlan`, so although the
  nucleus supports it (operation 6 reaches `plan::destroy`) a boot reaches at
  most four launch policies. Five launches therefore need one plan used twice,
  which `plan.rs` endorses: a sealed plan *"is not consumed by the creation that
  reads it … a restart is the same policy applied to a new process instance"*.
- **One shared plan for both state-service generations implies one shared receive
  endpoint**, and `IPC_V1` §2 admits exactly one receive-rights holder at a time
  — enforced as `NotGranted::ReceiverExists`. The successor can only be created
  after the predecessor's slot is cleared, which is the quarantine/drain/reclaim
  path `block-lifecycle.sh` already proves, and the successor's creation
  succeeding is itself evidence the predecessor is gone.

A bound sketch that fits: **6 process instances over 4 slots** (supervisor, block
service, state#1, writer; then state#2 and reader); **4 plans** (block, state
shared by both generations, writer, reader); **5 endpoints of 6** (block-serve,
state-serve, state-inbox, writer-inbox, reader-inbox); **4 endowments** in the
state plan, which is `MAX_ENDOWMENT` exactly — `budget`, `state-serve` (receive),
`block-serve` (send|call), `state-inbox` (send|receive).

**No blocker, provided the state service does not publish.** If it must, the
blocker is exact and is `ADR-0095` §6.

## 12. The Stage 4 handoff budget: an interpretation question, not a violation

**Withdrawn:** the claim that the four-layer stack necessarily exceeds
`docs/35`'s *"no more than four address-space/scheduler handoffs per unbatched
request"*. Architectural boxes are not scheduler handoffs, and no path has been
counted or measured.

**What the accepted text actually scopes.** The bullet sits under `docs/35`
§Stage 4 — VirtIO block textual driver, under *"Hard budgets after queue
initialization"*, among siblings that are all per-block-request and device-facing:
*"zero dynamic allocation per completed block request"*, *"no more than one
payload copy between client memory and device-visible memory"*, *"one interrupt
wakeup may complete a batch of requests"*, *"no global driver lock serializes
independent queues"*. The natural endpoints are therefore

```text
immediate block client  <->  block service  <->  device      (one completed block request)
```

and not an end-to-end higher-level state operation. On that reading a state
operation that issues K block requests is K requests under the budget, not one.

**Where the accepted documents are insufficient, stated exactly.** `docs/35` does
not define *"address-space/scheduler handoff"* — the Stage 3 IPC section uses a
different term, *"user/kernel boundary crossings"*, for its own four-crossing
budget — and it does not say whether a request issued by a service on another
service's behalf is one request or two. Nothing establishes a violation, and
nothing establishes conformance either: the metric is **P0** (unmeasured design)
for Stage 4 today, Stage 4's performance-contract report is still owed, and
§Reporting status says *"No stage closes on P0 for a metric assigned to that
stage."*

**Recommendation:** the persistent-state ADR (or the Stage 4 performance-contract
report, whichever comes first) records the endpoints as per-block-request and
states that a store's header or index writes are **separate block requests,
accounted separately**. That is a clarification, not a weakening: `docs/35`
§Budget classes allows a threshold to be revised *"with measurements and an ADR"*
and forbids silently weakening one *"to close a stage"*, which is another reason
to write the interpretation down rather than assume it.

**The one-copy conclusion is retained.** The state service should **forward** the
client's immutable region linearly rather than copy its payload: `ADR-0075` §5a
makes the forward exact, and a service that copied into its own region would add
a payload copy between client memory and device-visible memory, which is the
budget `docs/35` states in absolute terms.

## 13. The smallest credible evidence, and the mutations that must make it red

One boot, one canonical module per role, the supervisor sequencing it:

1. the **writer** puts **two** objects, each 512 bytes composed in canonical
   text, through the state service, through the block service, to the device;
2. the **writer ends**; the supervisor collects its ending;
3. the **state service ends**; the supervisor collects its ending and the journal
   shows the process reclaimed;
4. a **second state-service instance** starts from the same canonical module,
   reads the store's persistent bytes **from the device**, and finds a store with
   two objects;
5. the **reader** — a different process, created after the writer is gone — gets
   the **second** object and verifies all 512 bytes in canonical text.

**Placement-independent by design.** Under §5's option B the reader cannot
compute where its object lives; under A or C it could, so the gate must also
assert from the boot journal that **no client and no state instance ever held a
capability naming the block service's device authority**, and that the client
held no capability naming the block service at all. §5's last row says which
claim each option can actually support; the gate must claim only that one.

| Confounder | What rules it out |
|---|---|
| RAM retained by the old service | the predecessor is dead and its slot reclaimed before the successor exists, and `IPC_V1` §2 makes the successor's creation impossible until it is |
| seeded image contents | the harness seeds sectors 1–4 with 512 copies of `0xC0 + n` and leaves sector 0 zeroed; objects go to unseeded sectors with a **per-byte varying** pattern that no constant or zero fill can match |
| host-side knowledge | the host gate judges reported account bits and journal order only; the 512-byte comparison happens in canonical text |
| the client retaining and replaying | writer and reader are different processes, the writer is collected before the reader is created, and the reader derives its expectation from the identity it asked for |
| a fixture reconstructing state without reading storage | under B, the location is on the device and nowhere else; under A or C, the capability topology is what proves the reader had no other path |

**The mutations**, each of which must turn the gate red on its own assertion:

1. **omit the store's persistent metadata write** — the index under B, the header
   under C. The successor finds zeros, reports an uninitialized store, and the
   reader's `get` fails. Under A this mutation does not exist, which is one more
   way of saying what A cannot prove;
2. **omit or falsify the actual device write** — make the block service skip
   `VIRTIO_BLK_T_OUT` while still answering OK, or shift the data write by one
   sector. The reader's byte count is wrong. **Required by direction**;
3. **answer with the wrong object** — the successor returns the first object's
   bytes where the second was asked for. **Required by direction**, and the
   reason two objects are the minimum.

**Non-claims the gate must state**, in the form `block-lifecycle.sh` uses: no
power-loss durability, no `VIRTIO_BLK_F_FLUSH`, no crash consistency, no
filesystem, no directories, no paths, no content addressing, no enumeration, no
delete, no objects larger than one sector, no `/state` implementation unless §9
is ruled B, and no Stage 4 closure.

## 14. The boundary with capsule-to-repository handoff

The two deliverables share a device and nothing else. This slice would prove
**none** of: any Git object encoding, loose or packed; any commit, tree, blob or
tag; commit identity, refs, branches, protected-ref transactions, a commit graph;
content addressing of any kind — its identities are chosen by a client and bear
no relation to content; `/system` semantics, an active tree, a read-only mount, a
working overlay; I-03's boot record naming a commit unambiguously, or the
transition of `PROCESS_IDENTITY_V1` §5's `system commit id` from absent to
present; I-05 transactional activation, candidate boots, promotion or rollback;
`docs/08` §Work decomposition steps G1 and beyond — bounded loose-object reading,
deterministic local object writing, protected refs — which is where the handoff
lives, G0 being the capsule/provenance source identity Stage 1 delivered.

**And a structural reason to keep them apart.** `docs/08` §Nucleus versus
userspace responsibility puts *"bounded commit/tree traversal through a narrow
object-store interface"* and *"protected transactional ref primitives"* in the
**nucleus**, while object and commit creation is textual. The repository store
therefore sits on a different trust boundary from a service's state store, which
holds nothing the nucleus reads. `docs/11` §Bootstrapping puts them in order:
persistent storage at step 4, the versioned repository handoff at step 5.

## 15. The smallest ADR set

Two decisions, in this order. **Both are now drafted**: ADR-α is
`docs/adr/0098-the-block-device-v1-wire-protocol.md` and ADR-β is
`docs/adr/0099-stage-4-persistent-object-state-storage.md`, each `Proposed`, with
the proposed contracts in `docs/proposed/`. The sketch below is what was asked
for before they existed and is kept as the record of that request.

**ADR-α — `block.device.v1` wire protocol.** Accepts a Tier 2
`source/interfaces/.../BLOCK_DEVICE_V1.md` under `ADR-0020`'s four-condition
rule, fixing the request encoding and message order for `read` and `write`
(and `capacity`, if it is included), and recording that `ADR-0093` §9's reserved
question is now answered. **Level 2**: a contract extension, no ABI operation, no
capability kind, no nucleus change. It must be accepted **before** any store is
built, because otherwise the store's dependency is a fixture (§7).

**ADR-β — persistent object/state storage at Stage 4.** Decides, in one place:

1. **object identity** — §4's A or B, with the bound;
2. **placement** — §5's A, C or B;
3. **where format and schema identity live** — §6, with `docs/40`'s fixed-width
   rule;
4. **the `state.store.v1` service protocol** as a versioned boundary under I-09,
   with its own Tier 2 contract or an appendix to the ADR (§8);
5. **substrate or `/state` namespace** — §9's A or B;
6. **the persistence boundary restated** for the state service's own restart,
   carrying `ADR-0092` §0's exclusions as non-claims (§10);
7. **the `docs/35` interpretation** — the four-handoff budget's endpoints, and
   metadata writes accounted as separate block requests (§12).

**Level 3** as ruled on 2026-09-24 — `docs/21` places persistent-format changes
there. The first draft of this paragraph proposed Level 2 on the reading
`ADR-0020` used of itself, namely that it accepts a
persistent byte layout and a versioned protocol without moving a trust boundary,
adding an ABI operation, or changing the nucleus. `docs/19` already lists
*"first persistent object/state filesystem"* as a decision requiring an ADR, so
β is owed regardless of how small its format is. **`state snapshot mechanism`
stays where `docs/19` put it**: a separate decision, not part of β.

## 16. Findings retained from the first revision

Accepted by the Project Architect on review and unchanged:

- an ADR is required before implementation;
- persistent state and the capsule-to-repository handoff remain separate;
- no power-loss, `FLUSH` or ungraceful-termination claim;
- no nucleus change is presently required;
- no new system ABI operation and no new capability or object kind is presently
  required;
- launcher-wiring the state service is the preferred Stage 4 shape and avoids
  deciding a second publication class;
- the state service must hold no PCI, MMIO, IRQ or DMA authority;
- at least two objects are required for useful negative evidence;
- the writer and the old state-service instance must die before the new reader
  proves persistence;
- the actual-device-write and wrong-object mutations are required.
