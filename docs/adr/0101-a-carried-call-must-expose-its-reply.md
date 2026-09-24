<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0101: A call that carries a capability must expose its reply

- Status: **Proposed** (raised 2026-09-24 during the ADR-0099 implementation; **not
  accepted, and nothing in the tree implements it**)
- Date: 2026-09-24
- Decision level: **2** — a corrective contract extension. It changes the declared
  **result** of one already-accepted schema row to the result the same ABI call
  already produces, and switches one runtime-image row from `Produced::Status` to
  `Produced::Answer`. No ABI operation, no nucleus change, no capability or object
  kind, no bound, no representation-family member, no `LANGUAGE_VERSION` move, no new
  IPC semantics and no persistent format. **Explicitly not Level 3** on `docs/21`'s
  test: nothing here moves a trust boundary, changes a persistent format, introduces
  a runtime dependency, changes source identity or touches owner control
- Project Architect approval: **not granted; this is a draft for review**
- Related: **ADR-0098** and `BLOCK_DEVICE_V1` §5, §6a, §7, which already require the
  observation this row erases; **ADR-0099** and `STATE_STORE_V1` §8b, whose `ST_BLOCK`
  cannot be produced honestly without it; **ADR-0058** (`MESSAGE_PAYLOAD`);
  **ADR-0063**; `IPC_V1` §4; `SYSTEM_ABI_V1` operation 3; `SYSTEM_INTERFACE_V1` §4,
  §4.2, §5

## 0. What this corrects

**`BLOCK_DEVICE_V1` requires a client to read something the accepted schema row does
not let it read.** §5 is normative and unconditional:

> Every reply is sent with `endpoint_reply_word`, so its inline length is
> `REQUEST_BYTES` and its `word` is the answer. A client reads
> `system.ipc.Answer{length, word}` and checks `length == REQUEST_BYTES` before
> reading `word` at all.
>
> `word < REFUSED` … success. `word >= REFUSED` … refusal. `word - REFUSED` is the
> refusal code of §7.

And §6a's `READ` is made with the one row that carries an answer endpoint:

```text
client:   endpoint_call_word_carrying(answer_endpoint, service, sector * 4 + OP_READ)
```

whose accepted declaration is

| Operation | … | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|
| `endpoint_call_word_carrying` | … | `i64` | 3 |

with `Produced::Status` in the runtime image. **So a conforming `READ` client cannot
observe its own reply.** It learns that the call was answered and nothing about the
answer — which is exactly the failure `SYSTEM_INTERFACE_V1` §4.2 records as the
reason `system.ipc.Answer` exists at all:

> A row producing only the length would leave a caller able to learn that its call was
> answered and nothing about the answer — and a service with no other way to send a
> number back would encode its result as the length of a reply it never sent.

**The reply is delivered; the row discards it.** `ipc::hand` copies the replier's
payload into the waiting caller's own argument region and the nucleus returns the
answer's inline length to the woken caller in `rdx` — which is precisely what
`Produced::Answer` reads, and what `endpoint_call_word` and
`endpoint_call_word_region` already read over the same operation 3. Nothing about the
transfer table interferes: a delegated capability goes to `MESSAGE_CAPABILITIES` and
the answer's bytes to `MESSAGE_PAYLOAD`, which are different areas (ADR-0058).

**This is an ADR-0098 defect, not an ADR-0099 one.** ADR-0099 made it impossible to
ignore, because `STATE_STORE_V1` §9 has to translate a lower-layer refusal into
`ST_BLOCK` and there is nothing to translate.

## 1. What must not be done instead

**Inferring the refusal from the region's absence.** The shape

```text
the call returned OK, then no region arrived, therefore ST_BLOCK
```

is wrong, and the accepted contracts are the reason. It conflates at least three
states the contracts deliberately distinguish:

- a **`BLK_DEVICE`** refusal — the device answered and its answer was a failure;
- a **`BLK_RANGE`**, `BLK_OPCODE`, `BLK_MALFORMED` or `BLK_NO_ANSWER` refusal — the
  request was wrong and the device was never touched (`BLOCK_DEVICE_V1` §7);
- a **success reply followed by incomplete delivery** — `BLOCK_DEVICE_V1` §6a puts the
  reply *before* the region send precisely so that "the read succeeded and exactly one
  region is owed" is a state a client can be in, and a service that died in the gap
  leaves the client to the ordinary liveness path.

A client that could not tell those apart would be reporting one of them while in
another. It is also unimplementable without blocking: waiting for a region that a
refusal means will never come is a wait the liveness rule ends, which turns a refused
request into a cancelled boot.

**Adding a second, near-duplicate call row.** The row `BLOCK_DEVICE_V1` §6a names is
this one. A second row differing only in its result would leave the named row still
wrong, and would make which of two identical calls a protocol meant a matter of
spelling.

## 2. The decision

`SYSTEM_INTERFACE_V1`'s row becomes:

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `endpoint_call_word_carrying` | `system.ipc.Endpoint` with `none`, then `system.ipc.Endpoint` with `call` | `word: u64` | `Result<system.ipc.Answer, i64>` | 3 |

and the runtime-image row's result becomes `Produced::Answer`.

**Everything it needs already exists**, which is what makes this a correction rather
than a mechanism:

| | |
|---|---|
| `SYSTEM_ABI_V1` operation | 3, unchanged |
| the transferred capability | transfer slot 0, unchanged |
| the request word | `MESSAGE_PAYLOAD`, filling the length register itself, unchanged |
| the reply's payload and length | already copied and already returned; `Produced::Answer` is how two other rows read them |
| bounds | unchanged. A call still reserves the last transfer slot for its answer and may carry three of its own |

## 3. Scope

**Nothing else moves.** No ABI operation, nucleus mechanism, capability kind, object
kind, IPC bound, representation-family member or language version. No persistent
format. `system.ipc.Answer` is unchanged — this row starts producing the record the
schema already declares.

**Existing callers are updated, not grandfathered.** Four call sites read the row's
result as an `i64` today and become `match` arms:
`block-data-path/client.tos`, `block-lifecycle/client.tos` (two), and
`block-protocol/client.tos` (two). Their behaviour does not change — a status of `OK`
becomes `Ok(answer)` and the accounts they report stay the same — which is what makes
their existing gates the regression for this change.

## 4. Conformance evidence this decision requires

**Acceptance carries these obligations**, and none is met today:

1. **the success class** — a server replies `length = 8` and a known success word, and
   the caller observes `Answer.length == 8` and exactly that word;
2. **the refusal class** — a server replies `REFUSED + code`, and the caller observes
   exactly that word;
3. **`BLOCK_DEVICE_V1` `READ`, success** — `Answer{length: 8, word: 0}` **and then**
   the region, in that order;
4. **`BLOCK_DEVICE_V1` `READ`, refusal** — a refusal `Answer` and **no region
   follows**. `BLK_RANGE` or `BLK_NO_ANSWER` is enough and is honestly exhibitable;
   **`BLK_DEVICE` is not invented**, because the reference VirtIO device answers every
   well-formed in-range request with `VIRTIO_BLK_S_OK` and no fake device is built to
   manufacture a failure (the class `virtio-queue.sh` records for its withdrawn MSI-X
   negative);
5. **the reply-before-region mutation re-run** — sending the region before the reply
   must still turn `block-protocol`'s ordering assertion red, now with a caller that
   can see the reply.

## 5. What this then lets ADR-0099 do honestly

`STATE_STORE_V1`'s `GET` maps the lower layer's result instead of guessing at it:

```text
reply_to     = capability_attenuate(state_inbox, RIGHT_SEND)
block_result = endpoint_call_word_carrying(reply_to, block_service, READ(sector))
capability_release(reply_to)

Err(status)                          the lower IPC operation failed — an incomplete
                                     operation, not a refusal
Ok(answer), answer.length != 8       ST_BLOCK
Ok(answer), answer.word >= REFUSED   ST_BLOCK
Ok(answer), answer.word == 0         exactly one region is owed; receive it on
                                     state_inbox
```

**No region is waited for after a lower refusal**, which is the whole point. And a
successful lower reply whose owed region never arrives stays an **incomplete lower
operation** — it is not turned into a fabricated `BLK_*` refusal, because
`BLOCK_DEVICE_V1` §6a made those two different observations on purpose.

## 6. The ADR-0099 bound accounting this implementation also corrects

**Found by building it, and it is an accounting correction rather than a new
topology.** The initializer legitimately needs an **answer inbox of its own**: a block
`READ` delegates a channel, and the initializer cannot attenuate the *block service's*
endpoint for that — that name is the service's, not an inbox. So ADR-0099 §13a's
sketch of two endowments and four endpoints was one short in each. To be applied to
ADR-0099 and `STATE_STORE_V1` **after** this decision is handled:

```text
endpoints: 5 of 6      block-serve, state-serve, state-inbox, client-inbox, init-inbox
plans:     4 of 4      block, initializer, state A/B, writer/reader client
startup endowments     block 3, initializer 3, state 4, client 3
```

`MAX_ENDOWMENT` remains 4 and `MAX_ENDPOINTS` remains 6; both are still satisfied with
room. Nothing else about the topology changes, and the initializer is still collected
before the ordinary state service starts.

## 7. Architecture impact statement (`docs/21`)

- **Which invariants are affected?** None is amended. I-09 is served rather than
  strained: a versioned schema row is corrected to describe what the system does, and
  the correction is recorded rather than slipped in. I-13 is the reason this is a
  decision at all — a client that could not observe its own reply would have made
  `BLOCK_DEVICE_V1` §5 a claim no boot could exercise.
- **What becomes canonical after the change?** That a call carrying a capability
  answers with the same record as a call that does not. Nothing about what a protocol
  may put in that record changes.
- **What enters or leaves the trusted base?** Nothing. The nucleus already produces
  both halves of the answer; one runtime-image row stops discarding them.
- **Can the active runtime still identify its exact source?** Unchanged.
- **Can all derived artifacts be discarded and regenerated?** Unchanged.
- **Can the owner still recover and boot a previous commit?** Unchanged.
- **Does the change create a hidden host dependency?** No.
- **Does it alter licensing or patent exposure?** No.
- **How is the behavior tested?** §4's five obligations, and the four existing callers
  whose unchanged accounts are the regression.
