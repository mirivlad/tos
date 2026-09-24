<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0100: Attenuating an endpoint from canonical text

- Status: **Proposed** (raised 2026-09-24 on Project Architect direction; **not
  accepted, and nothing in the tree implements it**)
- Date: 2026-09-24
- Decision level: **2** — a contract extension. It declares an operation that
  already exists, on an interface that does not yet declare it. No ABI operation,
  no nucleus mechanism, no capability or object kind, no bound, no
  representation-family member, no `LANGUAGE_VERSION` move
- Project Architect approval: **not granted; this is a draft for review**
- Related: `CAPABILITY_V1` §4 (attenuation); `IPC_V1` §2 (one receive-rights
  holder) and §6 (a delegation carries the rights the sender holds);
  `SYSTEM_ABI_V1` operation 5; `SYSTEM_INTERFACE_V1` §4; **ADR-0078** §1, which
  named attenuation of runtime-obtained authority as a case the IR representation
  had to carry; **ADR-0098**, whose implementation established the consequence
  this decision answers; **ADR-0099** §2, §8, whose accepted endowment counts
  depend on it

## 0. What this is for

**`ADR-0099`'s accepted topology does not fit its accepted bounds, and the reason
is a transport fact `ADR-0098` proved rather than a mistake in either.**

`BLOCK_DEVICE_V1` `READ` and `STATE_STORE_V1` `GET` both delegate an answer
endpoint, because a reply cannot carry a region. `IPC_V1` §6 delegates a
capability **at exactly the rights the sender holds**, and §2 admits one
receive-rights holder per endpoint at a time. So a client that delegated the name
it receives on would be asking the service to become a second receiver — and the
accepting receive refuses the **whole message**, leaving the request in the queue
while both processes wait for each other.

That is not a theory. `block-protocol.sh` was built with one name and deadlocked
exactly there; it needs two names for one endpoint:

```text
inbox      receive        what the client takes its answer on
reply_to   send           the send-only name the request hands over
```

**`ADR-0099` cannot simply do the same.** Its accepted endowment counts are four
for the state service and three for the client (§2, §8), and a second startup name
for each inbox would make them five and four. `MAX_ENDOWMENT` is four.

**The mechanism already exists in the nucleus and cannot be named from canonical
text.** That is the whole of the gap, and this decision is one row.

## 1. What is already true

Verified against the tree on 2026-09-24, and each of these is why the row is a
declaration rather than a mechanism:

1. **`SYSTEM_ABI_V1` operation 5 is generic `capability_attenuate`** — "the
   capability being attenuated", `CAPABILITY_V1` §4. It is not typed to an
   interface;
2. **the nucleus accepts an ordinary endpoint.** `capability::attenuate` refuses
   only `Object::is_affine()`, which is `Region`, `DmaRegion`, `LaunchPlanBuilder`
   and `LaunchPlan`. `Object::Endpoint` is not among them;
3. **the one-receiver rule cannot fire on an attenuation**, and the nucleus says so
   in as many words: *"Attenuation grants to the process that already held it, so
   the one-receiver rule cannot fire here: the holder is the same holder"*, and
   `receiver_of`'s own contract note is *"a process may attenuate its own receive
   right and hold both results; a second process may not hold one at all"*;
4. **`SYSTEM_INTERFACE_V1` declares no `capability_attenuate` on
   `system.ipc.Endpoint`.** It declares one on `system.process.Control`, and the
   platform schema declares one on `platform.pci.Bus`, `platform.irq.Source` and
   `platform.pci.FunctionConfig` — four non-affine interfaces already have it;
5. **the runtime image has no corresponding endpoint row**, for the same reason.

So the operation is reachable, evidenced and already declared four times over. What
does not exist is a way for a TOS Core module to write it down for an endpoint.

## 2. The decision

`SYSTEM_INTERFACE_V1` gains one row on `system.ipc.Endpoint`:

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `capability_attenuate` | `system.ipc.Endpoint` with `none` | `rights: u64` | `Result<system.ipc.Endpoint, i64>` | 5 |

**`none`, as on every other interface that declares it**, and for the reason
`SYSTEM_INTERFACE_V1` already gives: attenuation names no right of its own — what
it requires is that the caller *hold* the capability, and what it produces is
bounded by that capability's rights whatever the caller asked for.

The semantics are `CAPABILITY_V1` §4's and are not restated here. The use this
decision exists for:

```text
full inbox                 send | receive, one startup grant

send_name = capability_attenuate(full inbox, RIGHT_SEND)

   full inbox   still held, still the receiver — one holder, one process
   send_name    send only, and safe to delegate
```

**Nothing is consumed.** Attenuation does not consume its input, so the original
remains the receiving name; the alias is a second entry in the *same* process's
table, which `IPC_V1` §2 admits because the holder is one process.

## 3. What this does not decide

- **No ABI operation**, no nucleus change, no capability or object kind, no new
  object, no change to `IPC_V1`, `CAPABILITY_V1` or `SYSTEM_ABI_V1`;
- **no bound.** `MAX_ENDOWMENT` stays four, `MAX_CAPABILITIES` stays sixteen, and
  no message bound moves. What an attenuation costs is one transient capability
  entry, released when the alias has been delegated;
- **no representation-family member and no language minor.**
  `system.ipc.Endpoint` is `AsInterface`, so `ADR-0085` §4.3's enumeration is
  untouched and `ADR-0085` §13's test — a capability position a conforming
  pre-amendment frontend rejects becoming valid — is not tripped: an endpoint
  capability parameter and an endpoint result are both already valid;
- **nothing about `STATE_STORE_V1`'s persistent format or wire shape.** §4 is the
  only consequence for `ADR-0099`, and it is an accounting clarification;
- **no rule about who *may* attenuate.** Holding the capability is the authority,
  as it is for the four interfaces that already declare this operation.

## 4. The consequence for ADR-0099, stated and not yet applied

On acceptance, `ADR-0099` §2 and `STATE_STORE_V1` §2 keep their endowment counts —
**four and three** — and gain the mechanism by which a `GET` answers:

```text
state-inbox = send | receive, one startup grant

for each block READ:
    reply_to = attenuate(state-inbox, RIGHT_SEND)
    call block READ carrying reply_to
    release reply_to once the call has transferred it
    receive the region on state-inbox
```

and a client's `GET` is the same pattern on `client-inbox`. So: startup endowments
unchanged, `MAX_ENDOWMENT` unmoved, one transient table entry per request, and the
receiver identity stays one process. **That text is not written until this decision
is accepted**, and this ADR is not authority for it in the meantime.

## 5. Conformance evidence this decision requires

**Acceptance carries these obligations**, and none is met today:

1. **the positive** — one process holds `send | receive` on an endpoint, attenuates
   to `RIGHT_SEND`, and **still receives** through the original;
2. **the alias crosses** — the attenuated name is delegated to another process and
   the message is *accepted*, which is the half the deadlock proved was missing;
3. **the delegated process cannot receive through it** — a receive on the alias is
   refused, so `RIGHT_SEND` means send;
4. **the negative, as a mutation** — delegating the **unattenuated** `send |
   receive` name must turn a gate red on `IPC_V1` §2's existing second-receiver
   rule rather than on a new one;
5. **attenuation refuses to widen** — asking for a right the holder does not have
   yields at most what it has, which is `CAPABILITY_V1` §4 and is checked rather
   than assumed.

## 6. Why not the alternatives

- **Two startup names, as `block-protocol` has.** That is what `ADR-0099` cannot
  afford: five endowments for the state service and four for the client, against a
  bound of four. It also scales the wrong way — every future interface that answers
  with a region would cost another startup grant.
- **Raise `MAX_ENDOWMENT`.** Explicitly excluded by direction, and rightly: a bound
  raised to fit a workaround is a bound that stops meaning anything.
- **Let a delegation narrow rights at the send site.** That would change `IPC_V1`
  §6 — a delegation carries what the sender holds — which is a transport rule this
  decision has no business touching, and it would put a rights mask in every
  carrying row.
- **Declare it inside a storage contract.** A generic endpoint operation buried in
  `STATE_STORE_V1` would be authority a storage document assigned itself over the
  IPC schema. `SYSTEM_INTERFACE_V1` is where interfaces are declared.

## 7. Authority, and why this is an ADR at all

`docs/38` §Amendment rules: *"subsystem contract changes: Level 2 or 3 according to
impact"*. `SYSTEM_INTERFACE_V1` is a Tier 2 subsystem contract, and `docs/21`
§Level 2 requires *"a design note and generally an ADR"*.

**No accepted clause licenses adding this row without one**, and the question was
asked before this was drafted. `SYSTEM_INTERFACE_V1` §4's *"Only operations that
already exist, are already reachable through `SYSTEM_ABI_V1`, and are already
evidenced"* is a **necessary condition** on what may be declared, not a permission
to declare. `ADR-0078` §1 names *"`capability_release` and `capability_attenuate` on
anything obtained at runtime"* as a case the **IR representation** of capability
sources had to carry, and that is what it decided — a representation, not a schema
row.

The repository's own practice is the same both times it came up: `ADR-0097` added
the region rows and `ADR-0098` added the call-with-region rows, and each took a
decision. This is the third and it is the smallest of the three.

## 8. Architecture impact statement (`docs/21`)

- **Which invariants are affected?** None is amended. I-09 is satisfied rather than
  strained: the row goes into the versioned schema that already carries the
  operation for four other interfaces. I-02 is the reason this is a declaration —
  nothing moves into the nucleus, because the mechanism is already there.
- **What becomes canonical after the change?** That a module may name the
  attenuation of an endpoint it holds. Nothing about what it may then do with the
  result changes: `IPC_V1` §2 and §6 are untouched.
- **What enters or leaves the trusted base?** Nothing. No nucleus code, no ABI
  operation; the runtime image gains one table row over a selector it already
  performs for four interfaces.
- **Can the active runtime still identify its exact source?** Unchanged.
- **Can all derived artifacts be discarded and regenerated?** Unchanged.
- **Can the owner still recover and boot a previous commit?** Unchanged; nothing
  here is in the boot path or persistent.
- **Does the change create a hidden host dependency?** No.
- **Does it alter licensing or patent exposure?** No.
- **How is the behavior tested?** §5's five obligations, one of which is a mutation
  against the existing second-receiver rule.
