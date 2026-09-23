<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0098: The `block.device.v1` wire protocol, and the atomic call that carries a region

- Status: **Proposed** (raised 2026-09-23 on Project Architect direction; **not
  accepted, and nothing in the tree implements it**)
- Date: 2026-09-23
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
- Project Architect approval: **not granted; this is a draft for review**
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

**So this decision does two things.** It fixes the protocol, in a proposed Tier 2
contract `BLOCK_DEVICE_V1` (`docs/proposed/BLOCK_DEVICE_V1.md`), and it fixes the
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
| `carried` | `system.ipc.Endpoint` |
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

**`region` is an `Option` and `carried` is not, and the asymmetry is not
cosmetic.** A zero handle in `carried` *"names nothing in any table"*, so the
first endpoint operation on it answers `E_NO_CAPABILITY` — a refusal the service
recovers from. A zero handle in a **region** field is different: an indexed
region access the host holds no mapping for is a **trap**
(`RUNTIME_DEVICE_REFUSED`), which ends the process. A service that had to touch a
region to learn whether one arrived could not serve `READ` at all. `Option` is
`ADR-0067`'s rule applied where the schema already applies it — absence is the
true value, and a zero would be a claim.

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

Accepted as a proposed Tier 2 contract, `docs/proposed/BLOCK_DEVICE_V1.md`, whose
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
- **no version field in the word.** The protocol version is the endpoint
  object's identity (`ADR-0095` §3: one object fixes one publication class), so a
  version in every request would be a second place one fact is stated, and the
  two would eventually disagree;
- **`WRITE` is one atomic call** carrying the request word and one immutable
  region, through §2a's row. There is no preceding region message;
- **`READ` is a call carrying the caller's answer endpoint.** The service sends
  exactly one immutable region there and replies **after** that send succeeded;
- **`CAPACITY` is an ordinary call** whose sector field must be zero and whose
  answer is the addressable sector count;
- **one reply discriminator for all three.** Bit 63 of the reply word clear means
  success; set means refusal, with the refusal code in the low bits. So a
  refusal is never mistaken for a capacity, and a client checks one thing.

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
7. **malformed-length negative** — a call whose inline length is not eight is
   refused, and its `word` is not acted on;
8. **the atomicity mutation** — the client sends a decoy region by
   `endpoint_send_region` *before* its atomic `WRITE` call, with a different
   pattern. A service taking its payload from a separate `endpoint_receive_region`
   writes the decoy and the read-back witness fails; the conforming service writes
   the call's own region. This is the mutation that proves §1's claim.

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

On acceptance, `ADR-0093` §9's reserved item *"The wire shape of `read`, `write`
and `capacity`"* is answered by this decision, and a dated amendment line is
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
