<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0098: The `block.device.v1` wire protocol, and the atomic call that carries a region

- Status: **Accepted** (Project Architect-approved, 2026-09-24). **Nothing in the
  tree implements it yet**: acceptance fixes the contract and carries §4's
  evidence obligations, which are outstanding
- Date: 2026-09-23, accepted 2026-09-24
- Decision level: **2** — a contract extension. It accepts a versioned service
  protocol and adds two rows and one record to `SYSTEM_INTERFACE_V1` over ABI
  operations that already perform exactly what the rows need. It adds no ABI
  operation, no capability kind, no object kind, no nucleus mechanism, no IPC
  bound and **no persistent byte layout**. §6 is the architecture impact
  statement `docs/21` requires at this level
- **Not Level 3, and not a language minor.** `ADR-0097` was Level 3 because it
  added a member to `SYSTEM_INTERFACE_V1` §4.3's closed representation
  enumeration, which by `ADR-0085` §13's own test — "a capability position a
  conforming pre-amendment frontend and verifier reject becomes valid" — also
  took a TOS Core minor. This decision adds **no** enumeration member and opens
  **no** new capability position: a `system.memory.Region` parameter on an
  endpoint operation is already valid, because `endpoint_send_region` has it.
  `LANGUAGE_VERSION` does not move
- Project Architect approval: 2026-09-24, as drafted — *"architecturally approved
  as drafted, subject only to the normal status/contract promotion"* — after the
  corrective round that reversed READ's control/data ordering, made both optional
  payload positions `Option`, and replaced the endpoint-identity version rationale
- Related: **ADR-0093** §0 (the surface is `read`, `write`, `capacity` and
  nothing else) and **§9**, which reserved the wire shape and is answered here;
  **ADR-0095** §3 (an endpoint object's identity fixes one publication class);
  **ADR-0097** (the textual region surface, and §8c's reason not to extend
  `system.ipc.ReceivedCall`); **ADR-0037** and `IPC_V1` §5 (transfer rules);
  **ADR-0075** §5a (a successful linear transfer takes handle and mappings);
  **ADR-0057**, **ADR-0058** (message bounds and areas); **ADR-0092** §0
  (Branch A's persistence reading); `SYSTEM_ABI_V1` operations 2 and 3;
  `docs/11` §Driver interfaces; `docs/02` I-09;
  `docs/research/STAGE4_PERSISTENT_STATE_BOUNDARY.md` §7, which raised this and
  is authority for nothing

## 0. What this decides, and why it is needed now

**`block.device.v1` has clients but no protocol.** `ADR-0093` fixed the
interface's surface — `read`, `write`, `capacity` — and §9 deliberately left the
wire shape out: *"Interface design inside `IPC_V1`; no new IPC mechanism,
capability type or ABI operation is to be created for it."* What exists today is
one fixture's encoding, documented as a fixture's in the fixture, in `README.md`
and in `PROGRESS.md`: eight payload bytes carrying `sector * 2 + direction`, a
region sent in a message of its own beforehand.

**A second client is what turns that into a boundary.** The Stage 4 persistent
object/state store (ADR-0099) is a client of this interface. Under `docs/02` I-09
a driver contract is *"versioned from [its] first implementation"*, and
`docs/11` §Driver interfaces gives `block.device.v1` its purpose — a device-class
interface *"rather than exposing hardware-specific details to applications"*. A
store built on a deliberately non-normative encoding would make a fixture its
production dependency, and the first fixture change would be a silent protocol
change.

**So this decision does two things.** It fixes the protocol, in the Tier 2
contract `BLOCK_DEVICE_V1` (`source/interfaces/device/BLOCK_DEVICE_V1.md`), and it fixes the
one mechanism that protocol needs and canonical text cannot currently name: a
**call that carries a region and a request word in the same message**.

## 1. The two-message write is not promotable

The fixture writes in two messages:

```text
endpoint_send_region(service, region)          message 1
endpoint_call_word_carrying(inbox, service, w) message 2
```

**This is sound only under the fixture's sequencing.** One client, one request at
a time, one receiver taking them in order. With two clients the messages of two
requests interleave in one queue, and the service has no way to tell which region
belongs to which word: `IPC_V1` §3 gives a message a payload, a capability area
and a region area, and nothing ties one message to another. A protocol whose
correctness depends on there being one client is not a protocol.

**It is also unnecessary.** The nucleus already sends a call with a region:
`syscall.rs`'s `call` (operation 3) passes `frame.r8` to `send_transaction` as
the region count and supplies the reply capability, and `send_transaction`
resolves, retains and linearly transfers regions for a call exactly as it does
for a send. The receive half is the same: `write_regions` writes the arrived
region records at `MESSAGE_REGIONS` in the receiver's own argument region for
**every** accepted message, and the reply capability is in the last transfer
slot. Both halves exist and are exercised; what does not exist is a *schema row*
by which canonical text names them.

## 2. The decision

### 2a. Two additive schema rows and one record

`SYSTEM_INTERFACE_V1` gains, on `system.ipc.Endpoint`:

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `endpoint_call_word_region` | `system.ipc.Endpoint` with `call`, then `system.memory.Region` with `none` | `word: u64` | `Result<system.ipc.Answer, i64>` | 3 |
| `endpoint_receive_call_region` | `system.ipc.Endpoint` with `receive` | *(none)* | `Result<system.ipc.ReceivedCallRegion, i64>` | 2 |

and one record:

### `system.ipc.ReceivedCallRegion`

| Field | Type |
|---|---|
| `reply` | `system.ipc.Reply` |
| `carried` | `Option<system.ipc.Endpoint>` |
| `region` | `Option<Region<u8>>` |
| `length` | `u64` |
| `word` | `u64` |

**Five fields, and each is required by a request this protocol has.** `reply`
answers the call. `word` is the request. `length` is what makes `word`
trustworthy — `SYSTEM_INTERFACE_V1` §4.2 already states that `word` *"is
meaningless unless `length` is at least eight — which is the receiver's check to
make, in canonical text"*, and a service that could not make it would read a
garbage opcode out of a malformed call as `READ` of sector 0. `carried` is how
`READ` is answered, because a reply cannot carry a region. `region` is `WRITE`'s
payload.

**Both optional positions are `Option`, and neither is a zero handle.** A
conforming `WRITE` carries no answer endpoint and a conforming `CAPACITY` carries
neither, so absence is an ordinary outcome of this protocol rather than an error
state. TOS Core already has the typed absence model — `system.process.ChildEnding`
uses `Option<u64>` for exactly this reason, which `ADR-0067` states as *absence is
the true value, and a zero would be a claim its caller never made* — and a record
that said `system.ipc.Endpoint` while meaning "possibly nothing" would put that
rule back in every reader's hands.

**It is not merely tidier for the region; for the region it is the only safe
form.** A zero capability handle *"names nothing in any table"*, so an endpoint
operation on one answers `E_NO_CAPABILITY`, a refusal a service recovers from. An
indexed access to a **region** the host holds no mapping for is a **trap**
(`RUNTIME_DEVICE_REFUSED`), which ends the process — so a service that had to
touch a region to learn whether one arrived could not serve `READ` at all. What
was drafted as an asymmetry is a single rule with one position where breaking it
is fatal rather than recoverable.

**Verification, as the direction required, and it passes.** Neither field needs a
new representation-family member or a language minor:

- **no new family member.** `SYSTEM_INTERFACE_V1` §4.3's enumeration —
  `AsInterface | DmaRegionFamily | RegionFamily` — stays closed and unchanged.
  `system.ipc.Endpoint` is `AsInterface`; `system.memory.Region` is
  `RegionFamily`, admitted by `ADR-0097`. Representation is a property of the
  *interface*, and both derivation mirrors key on the type constructors
  `Region`/`RegionMut`/`DmaRegion`/`DmaRegionMut`
  (`interfaces::interface_of_representation`, `tos_verifier::representation::interface_of`).
  `Option<T>` is a TOS Core V1 type constructor and represents nothing;
  wrapping adds no family;
- **`Option<system.ipc.Endpoint>` needs no change at all.** `lower.rs`'s
  `schema_field_type` already strips `Option<…>` recursively and then resolves an
  accepted interface path to `TypeDef::Capability`, so this field resolves under
  the frontend as it stands;
- **no language minor**, on `ADR-0085` §13's own test — whether a capability
  position a conforming pre-amendment frontend and verifier reject becomes valid.
  `RegionFamily` is valid from TOS Core 1.5, which `ADR-0097` established, and a
  module obtaining a `Region<u8>` at 1.5 is doing what 1.5 admits.
  `LANGUAGE_VERSION` does not move.

**Two implementation obligations follow, and acceptance carries them.** They are
bounded frontend work inside this decision, not a change of its class:

1. **`schema_field_type` gains one arm for `Region<u8>`.** It admits integers,
   `Option<…>`, accepted interface paths and schema records today, and a region
   spelling is none of those — so `Option<Region<u8>>` would be a gap rather than
   a type. The arm mirrors what `resolve_type` already does for written syntax
   (`"Region" => TypeDef::Region(first)`), and the existing `Option` recursion
   then covers the wrapper;
2. **a schema record must propagate the largest representation minor its fields
   require to every module that names it.** `checker.rs`'s
   `named_representation_minor` walks the type syntax a module *writes*, and a
   module writing `Result<system.ipc.ReceivedCallRegion, i64>` names a **record**
   path, on which `representation_of` answers `AsInterface` — so a module
   declaring 1.4 could name this record and obtain a `Region<u8>` without ever
   writing a 1.5 form. That is the same class of hole as the feature-gate outer
   guard corrected on 2026-09-23, and it must be closed in the checker **and** the
   verifier, with a 1.4-module negative, before this record exists.

**One receive row serves all three operations.** A service cannot know before
receiving whether the next call carries a region (`WRITE`), an answer endpoint
(`READ`) or neither (`CAPACITY`), so the row that serves the protocol must be the
superset. `system.ipc.ReceivedCall` is **unchanged**, for `ADR-0097` §8c's
reason: its four fields matched by position are part of what the
capability-transfer, publication and lifecycle boots already prove.

### 2b. What this is, stated as the distinction the direction asked for

| | New? |
|---|---|
| a `SYSTEM_ABI_V1` operation | **no.** Operations 2 and 3, unchanged |
| a nucleus IPC mechanism | **no.** `call` already passes `frame.r8` as a region count; `send_transaction` already resolves, retains and transfers a call's regions; `write_regions` already reports them to a receiver |
| a capability kind | **no** |
| an object kind | **no** |
| an IPC bound | **no.** `ADR-0057`'s 256 inline bytes, four capabilities and two regions stand; this uses one region of the two, and a call's reply still takes the last capability slot |
| a capability representation member | **no.** `ADR-0085`'s enumeration is closed and stays closed; `RegionFamily` already exists |
| a `SYSTEM_INTERFACE_V1` / runtime-image surface | **yes, additively.** Two rows and one record, and one `Produced::ReceivedCallRegion` arm in the runtime image composing reads the image already performs |

The contract's own version moves, because a document that gained rows is a new
version of that document. Nothing it already said changes.

### 2c. The protocol, in `BLOCK_DEVICE_V1`

Accepted as a Tier 2 contract, `source/interfaces/device/BLOCK_DEVICE_V1.md`, whose
normative content is summarized here and stated there:

- **all three operations of `ADR-0093` §0's surface are in v1** — `READ`,
  `WRITE` and `CAPACITY`. `CAPACITY` is not deferred: a bounded client must be
  able to establish that its layout fits **before** it writes anything, and the
  alternative — discovering the device's bounds by issuing a request designed to
  be refused — is a client learning a fact by misbehaving;
- **one request word**, opcode in the low two bits, sector in the rest:
  `word = sector * 4 + opcode`, with `0 = READ`, `1 = WRITE`, `2 = CAPACITY`,
  `3 = reserved`. The maximum addressable sector is stated as a constant rather
  than left to overflow: `MAX_SECTOR = 2^62 - 1 = 4611686018427387903`. A device
  reporting more sectors than that has the excess unreachable in v1, and the
  contract says so;
- **no in-band protocol-version field in the word**, and the reason is the
  configured contract rather than the endpoint object. `block.device.v1` is a
  **versioned Tier 2 service contract**; accepted publication, binding and launch
  topology supplies a client with an endpoint *implementing* that contract; and
  v1 has **no runtime version negotiation**. A per-request version tag would
  therefore be redundant with the service contract the topology already
  configured.

  **It is specifically not redundant with endpoint-object identity, and an
  earlier draft of this ADR had that wrong.** `ADR-0095` §3's dedicated object
  fixes a *publication class and authority*; the endpoint later published as a
  service endpoint is a **different object**. The lifecycle evidence proves the
  distinction directly: `block-lifecycle.sh` uses two distinct service endpoint
  objects for the two generations — `serve_a` and `serve_b`, which `ADR-0093`
  §3a answers 5 and 7 require to be distinct — while **both instances implement
  the same `block.device.v1`**. So endpoint object identity cannot be the
  protocol version: one protocol already has more than one endpoint object;
- **`WRITE` is one atomic call** carrying the request word and one immutable
  region, through §2a's row. There is no preceding region message;
- **`READ` is a call carrying the caller's answer endpoint**, and its control
  reply comes **before** its data. The service obtains the sector completely,
  replies, and only then sends exactly one immutable region to the delegated
  endpoint. §2d is why that order and not the other one;
- **`CAPACITY` is an ordinary call** whose sector field must be zero and whose
  answer is the addressable sector count;
- **one reply discriminator for all three.** Bit 63 of the reply word clear means
  success; set means refusal, with the refusal code in the low bits. So a
  refusal is never mistaken for a capacity, and a client checks one thing.

### 2d. Why `READ`'s reply precedes its region

An earlier draft had the service send the region and *then* reply success. That
admits an **orphan response**:

```text
region queued on the answer endpoint
service dies
the caller's call is cancelled by the liveness rule
the region is still queued
```

**An endpoint object lives for the boot.** A process dying releases its
*receive authority*; it does not make the endpoint a fresh object, and nothing
drains what is queued on one. A later holder of receive on that endpoint —
including a successor created from the same launch plan, which is how two
generations share one inbox within `MAX_ENDPOINTS` — could then take an old
response as the answer to its own request. That is a correctness failure of the
protocol, not of the process that died.

**So v1 fixes the order as control-then-data:**

```text
obtain the requested data completely
reply success
send exactly one region to the delegated answer endpoint
```

and the three observations have distinct meanings:

| The client sees | It means |
|---|---|
| a **refusal** reply | the operation was refused; **no region follows** |
| a **success** reply | the read succeeded and **exactly one region is now owed** |
| the **region** arriving | the `READ` is complete |

**Two rules make that sound, and both are the client's:** a client may have **at
most one outstanding `READ` per answer endpoint**, and it may not issue another
until the owed region has been received. Nothing enforces this in the nucleus and
nothing needs to: a client that broke it would be unable to say which region
answered which of its own requests, which is a statement about that client and not
about the wire.

**What a death in the gap now looks like.** If the service dies after replying
success and before sending the region, the client observes an **incomplete
operation** through the ordinary liveness path — its receive blocks and is
cancelled with `E_CANCELLED` (`SYSTEM_ABI_V1` §6) — and **no stale region has been
queued**. If the region was sent, it belongs to the one outstanding successful
operation. That is strictly better than the orphan case and it costs nothing.

**No request identifiers, sequence numbers, reply-carrying-region nucleus
semantics or any other IPC mechanism** is added to reach it. The ordering is the
whole of the fix.

**And a consequence for the refusal set:** a failure that happens *after* success
has been replied **cannot become a refusal**, because the reply is already spent.
So `BLK_ANSWER` — drafted as "the region could not be delivered" — is withdrawn as
a refusal code. What remains is the pre-reply check that the new `Option` makes
possible: a `READ` whose call carried **no** answer endpoint is refused before
anything is read, as `BLK_NO_ANSWER`. A send that fails after success was replied
is a service fault, recorded in the service's journal, and the client learns of it
as an incomplete operation.

## 3. What this deliberately does not decide

- **Persistent bytes.** Nothing here gives any byte of any sector a meaning. That
  is ADR-0099's, and the reason these are two decisions.
- **Case D.** `ADR-0093` §5's boundary is retained verbatim: a `WRITE` accepted
  by a service that dies before delivering the reply leaves the client unable to
  distinguish "not performed" from "performed and the reply lost". **No
  transaction id, request journal, retry rule, idempotency guarantee or
  exactly-once semantics** is added, at Stage 4, for any purpose including
  removing case D.
- **Durability.** `ADR-0092` §0 Branch A stands: no power-loss durability, no
  `VIRTIO_BLK_F_FLUSH`, no ungraceful-termination evidence, no crash consistency.
  A successful `WRITE` means the device accepted and completed the request.
- **More than one sector per request**, request batching, multiple outstanding
  requests from one client, queue multiplexing policy, scheduling, `TRIM`,
  `FLUSH`, or any other VirtIO block feature.
- **Filesystems, partitions, caches, VFS or object stores** — `ADR-0093` §0 and
  `ADR-0097` §5, unchanged.
- **Who may hold a `block.device.v1` endpoint.** Publication and lifetime are
  `ADR-0093`'s and `ADR-0095`'s; this is the shape of the messages only.
- **Migrating the existing fixtures.** `block-data-path` and `block-lifecycle`
  keep their encoding until a later slice moves them, and that migration is
  explicitly not part of this decision. Their accepted evidence is evidence about
  what it was taken on.

## 4. Conformance evidence this decision requires

**Acceptance carries these obligations**, and they are not satisfied today:

1. **normative `READ`** — a client holding only a `block.device.v1` endpoint and
   an answer endpoint reads one sector through the encoding above, and verifies
   its bytes in canonical text;
2. **normative atomic `WRITE`** — one call carrying word and region, and an
   independent read-back proving the device holds those bytes;
3. **`CAPACITY`** — the answer equals the device's own reported sector count,
   clamped as §2c states, and is not a constant in the client;
4. **invalid-opcode negative** — opcode 3 is refused with its own code, and
   nothing is written;
5. **out-of-range negative** — a sector at or above the reported capacity is
   refused with its own code, and nothing is written. The existing
   `STAGE4_BLOCK_SECTORS` harness option already builds a deliberately small
   device for a capacity negative;
6. **absent-region negative** — a `WRITE` whose call carries no region is refused
   with its own code rather than trapping the service, which is what §2a's
   `Option` is for;
7. **absent-answer-endpoint negative** — a `READ` whose call carries no answer
   endpoint is refused as `BLK_NO_ANSWER` **before** the sector is read, which is
   the other half of what §2a's two `Option` fields are for;
8. **malformed-length negative** — a call whose inline length is not eight is
   refused, and its `word` is not acted on;
9. **the ordering assertion** — for a successful `READ` the journal shows the
   reply **before** the region send, which is §2d's claim and the one an
   implementation could silently get backwards;
10. **the atomicity mutation** — the client sends a decoy region by
    `endpoint_send_region` *before* its atomic `WRITE` call, with a different
    pattern. A service taking its payload from a separate
    `endpoint_receive_region` writes the decoy and the read-back witness fails;
    the conforming service writes the call's own region. This is the mutation that
    proves §1's claim;
11. **the ordering mutation** — a service that sends the region before replying
    must turn obligation 9 red. It is the mutation that proves §2d is implemented
    rather than merely written down.

**One obligation the direction asked for cannot be met, and the reason is the
region mechanism rather than an omission.** A *wrong-length* region negative is
**not exhibitable from canonical text**, and the contract must say so rather than
fake it:

- `region_allocate` grants *"the whole frames covering `bytes`"*, so the smallest
  region any client can originate is one frame — 4096 bytes, already more than a
  sector. A region too short to hold 512 bytes cannot be constructed;
- `region_freeze` preserves base and length, so freezing cannot shrink one;
- canonical text **cannot read a received region's extent**: `Produced::ReceivedRegion`
  yields a handle and nothing else, and an out-of-range indexed access is a trap,
  not a refusal — so a service cannot probe for the length either.

The contract therefore states the size rule as **"the region must cover at least
`SECTOR_BYTES`, and only the first `SECTOR_BYTES` are the sector's"**, notes that
it is guaranteed by the only mechanism that can originate a sendable region, and
classes the negative as **static**, in the same form
`host-tools/qemu-test/virtio-queue.sh` uses for the MSI-X negative that *"was
attempted and withdrawn"* because the reference device could not be made to
exhibit it. A rule stated as *exactly* 512 bytes would be worse than
unenforceable — it would be false of every conforming request.

## 5. `ADR-0093` §9 is answered

`ADR-0093` §9's reserved item *"The wire shape of `read`, `write` and
`capacity`"* is answered by this decision, and a dated amendment line has been
added to `ADR-0093` saying so — the mechanism `ADR-0095` used, and for the same
reason: an accepted decision is not rewritten, and a later one does not amend it
silently.

`ADR-0093` §9's other reserved items are untouched: idempotency of `write` stays
a lever nobody has pulled, restart policy stays canonical supervisor text, and
*"Persistent object/state storage and the capsule-to-repository handoff"* stay
separate deliverables — the first of which is ADR-0099 and the second of which is
nobody's yet.

## 6. Architecture impact statement (`docs/21`)

- **Which invariants are affected?** I-09, satisfied rather than strained: a
  driver contract gains the version it was required to have from its first
  implementation. I-01 and I-16 are unaffected — the protocol is implemented in
  canonical text on both sides. I-13 is the reason obligation 8 exists.
- **What becomes canonical after the change?** `BLOCK_DEVICE_V1` becomes the
  canonical `block.device.v1` wire shape. The fixture encoding becomes what it
  was already documented as: one fixture's, pending migration.
- **What enters or leaves the trusted base?** Nothing. No nucleus change, no ABI
  operation, no capability or object kind. The runtime image gains one result
  arm, and the runtime image is a verified ring-3 artifact, not the trusted base.
- **Can the active runtime still identify its exact source?** Unchanged.
- **Can all derived artifacts be discarded and regenerated?** Unchanged; nothing
  here is cached or persistent.
- **Can the owner still recover and boot a previous commit?** Unchanged.
- **Does the change create a hidden host dependency?** No. The host harness
  judges reported results; the protocol is performed by textual modules.
- **Does it alter licensing or patent exposure?** No. The VirtIO citation surface
  is unchanged and this adds no third-party mechanism.
- **How is the behavior tested?** §4's eight obligations, one of which is a
  mutation and four of which are negatives, plus §4's honest statement of the one
  negative class that is static.
