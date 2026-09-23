<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — persistent object/state storage: the boundary before the decision

- Status: **note, not a decision.** Tier 4 under `docs/38`: research and
  explanatory material, incorporated by no ADR.
- Date: 2026-09-23
- Audience: the Project Architect, before any persistent-storage implementation
  is begun and before the decision surface in §8 is ruled on.

## 0. What this is

The next Stage 4 deliverable under `docs/16` is **persistent object/state
storage**, with the stage's engineering exit *"persistent storage works through
a textual user-space driver"*. This note answers what that requires, from
accepted clauses only, and stops at the point where a decision is needed.

**It accepts nothing and proposes no format.** Where an answer is a preference
rather than a requirement it is labelled as one. `capsule-to-repository handoff`
is a **separate** `docs/16` deliverable and is treated as separate throughout
(§7).

## 1. Accepted requirements

**The deliverable and the exit.** `docs/16` §Stage 4 lists seven deliverables,
of which two are relevant and distinct:

> - persistent object/state storage;
> - capsule-to-repository handoff;

and the stage's engineering exit is *"persistent storage works through a textual
user-space driver"*, with the identity exit *"the textual driver performs actual
I/O from canonical source; no binary shadow driver or hidden host path exists"*.

**What `state` means.** `docs/09` §`/state` defines the namespace class as
*"Mutable durable state owned by services"*, and adds the rule that matters for
identity: *"State paths are namespaced by service identity and protected by
capabilities."* Its examples are service-level — databases, queues, leases,
indexes, update transaction records, session metadata.

**What the first implementation is permitted to be.** `docs/09` §Filesystem
implementations: *"The first implementation may use a simple native object store
and state filesystem under QEMU. The VFS and capability contracts must not
assume a particular disk format."* Two grants in one clause — a simple native
store is admitted, and no contract may be written that bakes a disk format into
it.

**What a service with durable state must declare.** `docs/09` §State schema
versions requires, of *every* such service: a state schema identifier and
version, compatible source-module versions, migration functions, downgrade
policy, snapshot requirements, and a maximum supported migration chain. For a
first schema with no predecessor the migration chain is empty and the downgrade
policy is *none*, but the **identifier and version must exist in the bytes**,
which is also what `docs/02` I-09 requires of every versioned boundary.

**How persistent numbers must be written.** `docs/40` §primitive types:
`size` *"MUST NOT be serialized in a persistent/public format … Public and
persistent forms use one of the explicit fixed-width integers."* Any on-disk
field is therefore an explicit fixed-width little-endian integer, and not a
`size`.

**What the persistence boundary is.** `ADR-0092` §0 records the Project
Architect's selection of **Branch A** on 2026-09-21:

> Stage 4 proves that the write reached the device, that a later read sees the
> change, and that the data remains available across a stop and restart of the
> block service. Power-loss durability, `VIRTIO_BLK_F_FLUSH` and
> ungraceful-termination evidence are **not** Stage 4's.

That is the accepted boundary, and it is narrower than the word "durable"
suggests. It is also the clause that decides §5.

**What an ADR is already owed.** `docs/19` §Decisions still requiring ADRs lists
**"first persistent object/state filesystem"** and, separately, **"state
snapshot mechanism"**. The first names this deliverable directly.

**Invariants that bear on it.** I-04 (runtime state does not implicitly modify
the active system commit), I-09 (versioned from first implementation), I-13 (no
milestone by a known throwaway path — the demonstration must exercise the real
contract), I-16 (source-to-runtime traceability).

**The performance contract.** `docs/35` §Stage 4 sets hard budgets *after queue
initialization*, two of which constrain the shape of any service inserted above
the block driver:

> - no more than one payload copy between client memory and device-visible
>   memory; zero-copy is preferred where the DMA contract permits it;
> - no more than four address-space/scheduler handoffs per unbatched request;

The first is a design constraint, not a preference (§4). The second is a
tension, and §8 treats it as part of the decision surface rather than assuming
it away.

## 2. Existing mechanisms

**Block read and write.** A canonical textual service claims one VirtIO block
function and performs real `VIRTIO_BLK_T_IN` and `VIRTIO_BLK_T_OUT` of one
512-byte sector per request, proved by `block-data-path.sh` and
`block-lifecycle.sh`. Its request encoding is one fixture's — eight payload
bytes carrying `sector * 2 + direction` — and `ADR-0093` §9 still leaves the
wire shape of `read`, `write` and `capacity` undecided, so **no general
`block.device.v1` protocol is accepted**.

**Ordinary Region IPC.** `ADR-0097` and `SYSTEM_INTERFACE_V1`
`endpoint_send_region` / `endpoint_receive_region`: one **immutable** ordinary
`Region<u8>` crosses a message in the region area, **linearly** — a successful
send takes the sender's handle and its mappings atomically (`ADR-0075` §5a), and
a `Region<mut u8>` may not be sent at all (`IPC_V1` §5, `ADR-0037`). A region is
originated by `region_allocate` on a memory authority and made sendable by
`region_freeze`.

**What a receiver can actually read.** `SYSTEM_INTERFACE_V1` §4.2:
`system.ipc.ReceivedCall` carries `reply`, `carried`, `length` and `word`, where
`word` is *"the first eight payload bytes, little-endian"*; `system.ipc.Answer`
carries `length` and `word`. **There is no operation that reads payload bytes as
`bytes` or as a `string`.** `endpoint_send_text` can put a string on the wire and
nothing can read one back — the contract says so in as many words. So the whole
readable request surface of a message is: one `u64`, one delegated capability,
one reply capability, and (in a second message) one region.

**Process and service restart.** `process_create_funded` from a sealed
`LaunchPlan`, `process_wait_child` returning a `ChildEnding`, and
`process_create_with_generation` for a restart lineage. `PROCESS_IDENTITY_V1`
§4: *"A restart produces a new process instance id and increments the restart
generation, keeping the same module and supervisor lineage."*

**What canonical text can represent as persistent bytes.** Exactly one thing: a
`Region<mut u8>` it allocated and filled byte by byte (`payload[at] = …`), frozen
and sent. Element type `u8` and only `u8` (`ADR-0085` §18, `ADR-0097` §8b). Any
number written into those bytes is serialized by hand as explicit fixed-width
little-endian bytes, which is also what `docs/40` requires.

**Stable identity mechanisms that already exist.**

| Mechanism | Where | Stable across | Reachable from canonical text |
|---|---|---|---|
| canonical absolute path | `CAPSULE_FORMAT_V1` §4.1 — `/`-rooted, UTF-8, no `.`/`..`, sorted, distinct | the capsule's life | **no** — a path is a build-time name; text cannot receive a string |
| `content_digest` (SHA-256 of content bytes) | `CAPSULE_FORMAT_V1` §4.2 | content | **no** — no accepted schema declares a hash operation |
| module name + source content id (`sha256:` over normalized source) | `PROCESS_IDENTITY_V1` §3, asserted by the launcher | restart, boot | **no** — it is a launch record, not a value a module holds |
| process instance id | `PROCESS_IDENTITY_V1` §3 | *"unique for the life of the boot; never reused"* | yes, as `CreatedProcess.instance` |
| restart generation | `PROCESS_IDENTITY_V1` §3–§4, asserted by the supervisor | the lineage | yes, in `ChildEnding` |
| capability handle | `CAPABILITY_V1` §7 | nothing — *"an index in one table and means nothing in another"* | yes |
| binding name | `ADR-0061`, `endow_for_launch`'s `binding: string` (≤ 64) | the launch | written, never read back |

**There is no accepted persistent namespace and no accepted persistent object
identifier.** `docs/09` names the namespace *classes* and no implementation;
`source/interfaces/` contains no storage contract of any kind (boot, platform,
runtime, system — and none of them mentions a store); `OBJECT_INTERFACE = 4`
stays reserved and empty (`ADR-0093` §2, §4). The only identity that is both
stable across a restart **and** expressible in a message today is a plain `u64`,
because `word` is the only readable field a protocol may choose.

**What survives only in RAM today.** Everything except sectors:

- the publication registry's entries — `ADR-0093` §3b: *"Its registry does not
  survive its own death"*, and that is the accepted Stage 4 answer;
- the nucleus tables: processes (`MAX_PROCESSES = 4`), capabilities (16 per
  process), endpoints (6), plans (4), queues (depth 4), regions, assignments and
  their generations;
- every `Region<u8>`, whose lifetime ends with a release or the holder's death
  (`SYSTEM_INTERFACE_V1` §`region_allocate`);
- launch plans — `plan.rs`: a plan ends *"by an explicit release of its
  capability, or by the death of the process that held it"*;
- the supervisor's journal, which leaves the machine as serial text and is not
  read back by anything textual;
- `PROCESS_IDENTITY_V1`'s whole identity plane, including the restart lineage;
- the disk image itself, in the harness: `run.sh` does `rm -f "$STAGE4_IMAGE"`
  and recreates it on **every** run, so nothing survives a QEMU restart today by
  construction (§5).

**Existing decisions that defer filesystems, repositories, partitions and
caches.** `ADR-0093` §0: *"No filesystem, no partitions, no cache, no object
store, no enumeration framework, no multi-device management"*, restated in §9 as
out of scope by direction, which also names *"Persistent object/state storage and
the capsule-to-repository handoff"* as Stage 4 deliverables **not in that slice**.
`ADR-0097` §5: *"No filesystem, partitions, cache, VFS or storage protocol."*
`ADR-0048` §(authorization): Stage 3 authorizes *"no filesystem, no
repository-backed `/system`"*. `ADR-0072` §6: the source provider *"has no
filesystem, network or environment fallback"*. `ADR-0030`: `/vendor` is defined
and needs no implementation before the stage that first needs it.

## 3. The exact semantic gap

**The block service already provides durable-across-restart byte storage.**
`block-lifecycle.sh` writes 512 bytes through a service, kills it, starts a
successor, and reads the same bytes back. So the gap is not durability, and it is
not device work.

The gap is three things, and only the third is hard:

1. **Indirection.** A client must name *an object*, not a sector. Today the
   client computes the sector itself, which means every client knows the device's
   geometry — the opposite of `docs/09`'s *"State paths are namespaced by service
   identity"*.
2. **Plurality.** "Object/state storage" with exactly one object is a sector with
   extra steps: a one-entry index is indistinguishable from a constant, and no
   gate could tell the two apart. Two objects is the smallest number that makes
   an index load-bearing.
3. **The index must itself be persistent.** This is the whole of the deliverable.
   A store whose object-to-location mapping lives in the storage service's RAM
   loses every object when that service dies — which is precisely the state the
   registry is accepted to be in (`ADR-0093` §3b) and precisely what
   "persistent object storage" must not be. Making the mapping survive means
   **bytes on a sector whose meaning is decided**, and no accepted clause decides
   any meaning for any byte of any sector.

That third item is the gap: **there is no accepted answer to "what does a sector
mean", and a persistent object store cannot exist without one.** Nothing smaller
will do, because the alternative — the state service's index being handed to it
at launch — makes the launcher the store and moves the problem, not the bytes.

## 4. The minimum model, and what forces each answer

| Question | Answer | Forced by, or preference |
|---|---|---|
| What is an object at Stage 4? | one 512-byte sector's worth of opaque `u8`, named by a number | forced: `ADR-0097` carries one region, the accepted device request is one sector, and `u8` is the only element type |
| What is state? | the current bytes of an object, overwritable | forced: `docs/09` §`/state` says *"Mutable"*, so overwrite is in the definition |
| What identifies an object across restart? | a bounded `u64` object id, chosen by the client | forced by elimination: a path cannot be received (`§2`, no readable `bytes`/`string`), a content hash cannot be computed (no accepted hash operation, and `docs/19` still owes the cryptographic-algorithm ADR), a capability handle *"means nothing in another table"*, and an instance id is boot-scoped. `word` is what a protocol may choose, so a `u64` is what an identifier can be |
| Which operations? | `put(id, region)` — create or update — and `get(id) -> region`. Two. | required: `put` follows from *"Mutable durable state"*, `get` from the exit. **Delete is not required** by any clause, and enumerate, capacity, rename, truncate and stat are not either |
| Names, numeric ids or content hashes? | numeric, and **not** content hashes | forced as above. Content addressing is `docs/08`'s and would be the next deliverable's vocabulary, not this one's (§7) |
| More than one object? | **yes, at least two** | §3 item 2: with one object the index is unfalsifiable and the gate would prove nothing |
| Must the layout be versioned? | yes — a magic and a format version in the bytes | `docs/09` §State schema versions, `docs/02` I-09 |
| Must fields be fixed-width LE? | yes | `docs/40`: `size` may not be serialized; persistent forms use explicit fixed-width integers |
| Copies? | the state service **forwards the client's region** rather than copying it | forced by `docs/35`: *"no more than one payload copy between client memory and device-visible memory"*. A state service that copied into its own region would break an accepted Stage 4 hard budget. The linear transfer makes forwarding exact: the sender loses the handle and the mappings atomically (`ADR-0075` §5a) |
| Crash consistency, journaling, transactions? | **none, and stated as a non-claim** | `ADR-0092` §0 Branch A excludes power-loss and ungraceful termination. I-05 is about activating a commit or a service revision, not a state write; `docs/09` §Transaction boundaries puts coordinated update records in the update model, which is `docs/13`'s and Stage 5's |

**What this model is not.** No paths, no directories, no hierarchy, no
enumeration, no POSIX semantics, no partitions, no block cache, no mmap, no
journal, no transactions, no refs, no commits, no content addressing, no
`/state` implementation — the namespace class in `docs/09` keeps its meaning and
gets no bytes here — and no `/system` repository semantics.

## 5. The persistence boundary

| Boundary | Required? | Reachable today? |
|---|---|---|
| the writing **client** dies | yes, implied | yes |
| the **persistent-state service** dies and a successor opens the store | **yes — this is the deliverable** | yes |
| the **block service** dies | yes | yes, already proved by `block-lifecycle.sh` |
| whole TOS reboot in one QEMU run | **no** | **no**: the nucleus has no reboot path; a boot ends at `TOS.HALT` |
| a second QEMU run against the same image | no | **not today**: `run.sh` deletes and recreates the image each run. One harness flag would change that |
| power loss, `VIRTIO_BLK_F_FLUSH`, ungraceful termination | **explicitly excluded** | — |

`ADR-0092` §0 Branch A is the clause: *"across a stop and restart of the block
service"*. The deliverable adds one restart that matters more — the **state
service's own**, because that is where the index lives. Everything below and
above that line is either already proved or explicitly out of scope.

**A preference, labelled as one.** Retaining the image across two QEMU runs and
booting a second time with no writer at all would close the "RAM retained"
confounder absolutely, and costs one flag in `run.sh`. It is *not* required by
Branch A, it still claims nothing about power loss, and it should be a separate
gate if it is wanted at all.

## 6. Does the layering hold?

The intended stack is

```text
canonical textual client  →  persistent object/state service  →  textual block
service  →  VirtIO block  →  device
```

**It holds, and the state service acquires no hardware authority.** It holds a
memory authority, its own receive endpoint, `send | call` on the block service's
endpoint, and a reply channel — no PCI bus, no function, no MMIO window, no
interrupt source, no DMA region. The block service remains the only holder of
those, exactly as in `block-lifecycle.sh`.

**Two accepted bounds nearly refuse it, and one wiring choice is load-bearing.**

- **A second published interface is not decided.** `ADR-0095` §6 leaves it
  undecided and §8 says the spare endpoints are not an argument for it. So the
  state service must **not** publish through the registry; its endpoint is
  launcher-wired, from a sealed plan, which is what `ADR-0093` §3a answer 3
  already requires of every client's first capability. Case C is accepted and is
  not re-proved here.
- **`MAX_PLANS = 4` and canonical text cannot release a plan.** `plan.rs` sets
  `MAX_PLANS = MAX_PROCESSES`, and `SYSTEM_INTERFACE_V1` declares no
  `capability_release` row on `system.process.LaunchPlan` — the nucleus supports
  it (operation 6 reaches `plan::destroy`), but no accepted schema row names it,
  so a boot reaches **at most four plans**. Five launches therefore need one plan
  used twice, which `plan.rs` explicitly endorses: a sealed plan *"is not
  consumed by the creation that reads it … a restart is the same policy applied
  to a new process instance"*.
- **One shared plan for both state-service generations then implies one shared
  receive endpoint**, and `IPC_V1` §2 admits exactly one receive-rights holder at
  a time — enforced in `capability.rs` as `NotGranted::ReceiverExists`. So the
  successor can only be created after the predecessor's slot is cleared, which is
  the quarantine/drain/reclaim path `block-lifecycle.sh` already proves. That is
  a constraint that pays for itself: the successor's creation succeeding is
  evidence the predecessor is gone.

A bound sketch that fits, with the reasoning rather than as a design:

- **6 process instances over 4 slots** — supervisor, block service, state#1,
  writer, then (after the writer and state#1 end) state#2 and reader. Peak 4.
- **4 plans** — block, state (shared by both generations), writer, reader.
- **5 endpoints of 6** — block-serve, state-serve, state-inbox, writer-inbox,
  reader-inbox. One spare.
- **4 endowments per plan at most**, which is `MAX_ENDOWMENT`: the state plan is
  exactly `budget`, `state-serve` (receive), `block-serve` (send|call),
  `state-inbox` (send|receive).

**No blocker, therefore — provided the state service does not publish.** If it
must publish, the blocker is exact and is `ADR-0095` §6.

**One tension that is not resolved by analysis.** `docs/35` §Stage 4 budgets *"no
more than four address-space/scheduler handoffs per unbatched request"*. A request
through the four-layer stack crosses client→state→block→device and back, which is
more than four. Either that budget is written about the block-driver path and a
stacked store's path is a contract that does not exist yet, or the stack as
drawn conflicts with an accepted hard budget. This is a reading the Project
Architect owns, and Stage 4 still owes its performance contract report, so it
belongs in the decision surface rather than in an implementation's assumptions.

## 7. The smallest credible evidence, and the mutations that must make it red

**The shape, which is close to the one suggested and differs in three places
that matter.** One boot, one canonical module per role, and the supervisor
sequencing it:

1. the **writer** puts **two** objects — ids 1 and 2 — each 512 bytes composed
   in canonical text, through the state service, through the block service, to
   the device;
2. the **writer ends**; the supervisor collects its ending;
3. the **state service ends**; the supervisor collects its ending and the
   journal shows the process reclaimed, as `block-lifecycle.sh` already asserts;
4. a **second state-service instance starts from the same canonical module**,
   reads its index **from the device**, and finds a store with two objects;
5. the **reader** — a different process, created after the writer is gone — gets
   object **2**, and verifies all 512 bytes in canonical text.

**Where it differs from the suggested shape, and why.**

- **Two objects, not one** (§3 item 2, §4). The reader asks for the *second*, so
  a store that ignored its index and answered from the first data sector returns
  the wrong object's bytes rather than an error.
- **The data sector is allocated by a cursor kept in the index**, so the reader
  cannot compute where object 2 lives. A reader that never read the index has
  nowhere to look.
- **The state service is one module across both generations**, so the successor's
  open is the ordinary path rather than a special case, and the format's magic is
  what distinguishes *"never initialized"* (a zero-filled sector) from *"a store
  with two entries"*.

**How it distinguishes persisted state from each confounder.**

| Confounder | What rules it out |
|---|---|
| RAM retained by the old service | the predecessor is dead and its slot reclaimed before the successor exists, and `IPC_V1` §2 makes the successor's creation *impossible* until it is (`NotGranted::ReceiverExists`, §6) |
| seeded image contents | the harness seeds sectors 1–4 with 512 copies of `0xC0 + n` and leaves sector 0 zeroed; the objects go to unseeded sectors and carry a **per-byte varying** pattern, which no constant fill and no zero fill can match |
| host-side knowledge | the host gate judges reported account bits and journal order only; the 512-byte comparison happens in canonical text, as `block-lifecycle.sh`'s client already does |
| the client retaining and replaying | writer and reader are different processes, the writer is collected before the reader is created, and the reader derives its expectation from the **object id** it asked for |
| a fixture reconstructing state without reading storage | the *location* of object 2 is in the on-disk index and nowhere else; the reader holds no sector number and the state service holds none either until it has read one |

**The mutations.** At least the first two, and the gate must go red on its own
assertion rather than on a cascade:

1. **omit the index write** in the first state-service instance, keeping both
   data writes. The successor reads a zero-filled sector, finds no magic, reports
   an empty store, and the reader's `get` fails. This is the one that proves the
   index is on the device and not in RAM;
2. **omit or falsify the actual device write**: make the block service skip
   `VIRTIO_BLK_T_OUT` while still answering OK, or shift the data write by one
   sector. The reader's byte count is wrong. This is the mutation the directive
   requires;
3. **make the successor ignore the index** and answer from the first data sector.
   The reader receives object 1's bytes where object 2's were asked for, which is
   what makes "more than one object" load-bearing rather than decorative.

**What the gate must state as non-claims**, in the form `block-lifecycle.sh`
already uses: no power-loss durability, no `VIRTIO_BLK_F_FLUSH`, no
crash-consistency, no filesystem, no directories, no paths, no content
addressing, no enumeration, no delete, no objects larger than one sector, no
`/state` implementation, and no Stage 4 closure.

## 8. The boundary with capsule-to-repository handoff

The two deliverables share a device and nothing else. This slice would prove
**none** of:

- any Git object encoding — loose or packed — or any commit, tree, blob or tag;
- commit identity, refs, branches, protected-ref transactions or a commit graph;
- content addressing of any kind. Its object ids are opaque numbers chosen by a
  client and bear no relation to content, so nothing here can be mistaken for
  `docs/08`'s content-ID model;
- `/system` semantics, an active tree, a read-only mount, or a working overlay;
- I-03's *"boot record … able to name that commit unambiguously"*, or the
  transition of `PROCESS_IDENTITY_V1` §5's `system commit id` from absent to
  present;
- I-05 transactional activation, candidate boots, promotion or rollback;
- `docs/08` §Work decomposition steps G1 and beyond — bounded loose-object
  reading, deterministic local object writing, protected refs — which is where
  the handoff actually lives, G0 being the capsule/provenance source identity
  that Stage 1 already delivered.

**And a structural reason they must stay apart.** `docs/08` §Nucleus versus
userspace responsibility puts *"bounded commit/tree traversal through a narrow
object-store interface"* and *"protected transactional ref primitives"* in the
**nucleus**, while the object and commit *creation* side is textual. The
repository store therefore sits on a different trust boundary from a service's
state store, which holds nothing the nucleus reads. Merging them would drag a
state format onto the nucleus's path, and that is a trust-boundary change nobody
has asked for.

## 9. The decision surface

Three of the conditions that require a decision before implementation are met,
so implementation should not begin:

1. **a new persistent on-disk format** — unavoidable per §3;
2. **a new object identity contract** — a `u64` state-object id, which is the
   only identifier the accepted transport can carry;
3. **a namespace question** — whether the id space is private to one service, and
   how it relates to `docs/09`'s `/state`, which it must not silently implement.

Not required: crash-consistency semantics (`ADR-0092` §0 excludes them), nucleus
changes, new ABI operations, a new capability kind, a new object kind, a change
to `IPC_V1`, `CAPABILITY_V1`, `SYSTEM_ABI_V1` or any accepted schema, and a
second publication class (§6, provided the state service is launcher-wired).

**And an ADR is owed regardless of how small the format is**, because `docs/19`
§Decisions still requiring ADRs names *"first persistent object/state
filesystem"* as one. `docs/09` §Filesystem implementations bears on its shape: the
format may be *"a simple native object store"*, and *"the VFS and capability
contracts must not assume a particular disk format"* — which argues for the
layout living in the state service's own canonical text with only its identity,
version rule and non-claims fixed by the decision, rather than for a new Tier 2
storage contract.

The fourth question for that decision is §6's last paragraph: **what `docs/35`'s
four-handoff budget is about.**
