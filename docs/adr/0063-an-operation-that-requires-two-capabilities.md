<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0063: An operation that requires two capabilities, and the exchange that needs one

- Status: **Proposed**
- Date: 2026-08-20
- Decision level: 2 — it extends what an accepted interface schema may require of
  a caller, adds an operation to a closed ABI, and is the first place the schema
  states a *right* rather than only a type
- Project Architect approval: **not given**

## What was checked before anything was proposed

The instruction was to prove the bound unreachable before proposing a way to
reach it, and not to fit the architecture to a preferred answer. So the bound is
derived first, and the proposal follows from the derivation rather than the other
way round.

### The bound, and that it is tight

`docs/35` line 103, restated by `IPC_V1` §8: "one request/reply exchange requires
no more than four user/kernel boundary crossings excluding scheduler
preemption".

A crossing is a transition between CPL 3 and CPL 0 through the one edge
`SYSTEM_ABI_V1` §3 admits. Preemption is excluded and is separately excluded by
construction: a tick returns through the timer stub, which is neither door.

Four crossings are **forced**, one for each of four facts:

1. the request exists in the client at CPL 3 and must reach the nucleus — the
   client crosses inward;
2. the request must reach the server at CPL 3 — the server crosses outward;
3. the answer is computed by the server at CPL 3 and must reach the nucleus —
   the server crosses inward;
4. the answer must reach the client at CPL 3 — the client crosses outward.

These are four distinct events: two per process, one in each direction, and none
can be merged with another because each pair is separated by work done at the
other privilege level. So an exchange costs **at least** four, and `docs/35`
allows exactly four. **The bound is tight**: a conforming implementation must
hit the minimum with no slack anywhere.

### What this system costs, counted

Measured, not estimated (`IPC_V1` §9.7, and the balanced counter whose invariant
the request/reply gate asserts): one exchange costs **six**. In steady state the
server performs two operations and each costs a crossing pair:

| | crossing |
|---|---|
| `endpoint_receive` returns with the request | out |
| `endpoint_reply` is entered with the answer | in |
| `endpoint_reply` returns | **out — surplus** |
| `endpoint_receive` is entered to wait again | **in — surplus** |

plus the client's `endpoint_call`, one in and one out. Six.

The two surplus crossings are identified exactly: the *return* of
`endpoint_reply` and the *entry* of the next `endpoint_receive`. Nothing happens
between them. The server leaves CPL 0 having answered and re-enters immediately
to wait, in the same iteration of the same loop.

### Five ways to remove them without a new operation, and why each fails

**(a) A flag on `endpoint_reply` meaning "and then wait".** `SYSTEM_ABI_V1` §6
already gives blocking operations a flag register, so the mechanism exists. It
fails on authority, not on mechanism: the operation would have to wait on an
*endpoint*, and `endpoint_reply`'s only capability is the reply capability. A
reply capability names the call and the caller waiting for it (`IPC_V1` §4);
`CAPABILITY_V1` §3 says a capability names *an* object and never a class. To wait
on an endpoint the operation must be given that endpoint. So (a) still needs a
second capability passed explicitly — it saves an operation *number*, not the
extension. It is therefore not an alternative to this decision but a variant of
it, and it is carried below as option B.

**(b) A non-blocking receive, so the server never waits.** Polling adds
crossings: every poll is an in and an out, and a poll that finds nothing has
bought none of the four.

**(c) The client using `endpoint_send` then `endpoint_receive` instead of
`endpoint_call`.** Four crossings for the client alone, six or more in total.
`endpoint_call` is already the minimal client.

**(d) Batching — one receive delivering several requests.** `IPC_V1` §3 makes one
message the unit and §7's queue delivers one per receive, so this is itself a new
operation. It also meets the letter while missing the point: §8 bounds *an*
exchange, and a system with one request outstanding still costs six.

**(e) The nucleus answering the server's blocked receive directly.** It already
does exactly this — a send hands the message to a waiting receiver and answers
its suspended call rather than making it ask again. That is what removes the
crossings a polling design would have; it cannot remove these two, because the
server must run at CPL 3 to compute the answer, so it must leave and come back.
The only open question is whether coming back is one operation or two.

### Conclusion of the check

Four crossings is reachable **only** if the server's outward and inward crossings
are the two ends of one operation, and such an operation **must** take two
capabilities explicitly: the reply it consumes and the endpoint it then waits on.
Both halves are forced by the accepted documents; neither is a preference.

One half is already decided. `SYSTEM_ABI_V1` §3: "Should an operation ever
require two capabilities, this contract assigns their positions in §5 order when
that operation is added." The ABI anticipated this and pre-fixed the rule. What
is genuinely undecided is the **schema** half.

## The gap, stated once

`SYSTEM_INTERFACE_V1` §2 gives an interface "a path, a capability type, and a
finite set of operations", and §3 says "The first parameter is the capability, of
the interface's declared type. The remaining parameters are values."

An operation requiring a `system.ipc.Reply` **and** a `system.ipc.Endpoint` has
no home in that model: it belongs to one interface, and its second requirement is
not a value.

A second thing is missing with it. `docs/42` §2, quoted by §3 of the schema
itself, requires that "the capability type, **requested operation/right**,
resource range, and the enclosing `uses` effect all match a declared interface
contract". The schema declares the type and the operation; it has never declared
the **right**. Today the right is checked only by the nucleus at `resolve`, so a
module's declaration and the authority it actually needs are related by nothing a
reader or a verifier can see. With one capability per operation that was a
latent gap; with two it becomes the difference between "reply here and wait
there" and "reply here and wait wherever the second handle happens to point".

## What must not be done about it

**Deriving the endpoint from the reply capability.** A reply names one call. An
endpoint is a different object with different rights and a different lifetime.
Manufacturing one from the other is the operation `CAPABILITY_V1` §2 says does
not exist: "No operation of `SYSTEM_ABI_V1` produces a capability."

**Merging the two into one object or one grant.** A capability that meant "may
answer this call *and* may receive on that endpoint" is a class, not an object,
and `CAPABILITY_V1` §3 rules that out in one line: "never a class of objects,
never 'all of them'". It would also make the two rights inseparable, so a
supervisor could not hand out one without the other — the attenuation
`CAPABILITY_V1` §4 exists to make possible.

**Moving request/reply into the nucleus.** `IPC_V1` §1 puts "request/reply,
streams, pub/sub and discovery in textual libraries and services above" the
primitives. The operation proposed here is a transport primitive — deliver this
answer, then wait for the next message — and carries no protocol: it does not
correlate requests, does not name a session, and does not know that the message
it waits for has anything to do with the answer it just delivered.

## Options for the schema half

### S-A — an operation may declare further capability parameters, each with its own interface and right

```tos
import capability system.ipc.Reply    as answer;
import capability system.ipc.Endpoint as inbox;

extern fn endpoint_reply_receive(
    reply: system.ipc.Reply,
    on: system.ipc.Endpoint,
    length: u64
) -> i64 uses [answer, inbox];
```

The operation still *belongs* to one interface — the one its first capability
names, which is what `uses` and the instruction's interface path record. Each
further capability parameter declares its own interface and the right it must
carry, and the module supplies each from its own `import capability` binding
(ADR-0061). The verifier proves, for every capability parameter, that the binding
named is an import of the declared interface, and that the function's effects
admit it.

Costs: the schema gains a per-parameter interface and right; `Op::Capability`
carries one import index and would need the others; and the verifier's check
becomes a check per capability rather than per instruction. It is the largest
change, and it is the only one that leaves both capabilities separate,
separately attenuable, and separately checkable — which is the property the
instruction asked for by name.

### S-B — a compound interface whose capability type carries both rights

One `system.ipc.Server` capability meaning "answer and receive".

Costs: refused above and refused by `CAPABILITY_V1` §3. Recorded so that the
refusal is on the record rather than assumed.

### S-C — the operation belongs to no interface

A schema-level operation, outside any interface.

Costs: `docs/42` §2 requires the enclosing `uses` effect to match "a declared
interface contract", and an operation belonging to none has no effect to match.
It would also make `Signature.effects` — which ADR-0060 made the artifact's
statement of which interfaces a module reaches — silent about this one.

## Options for the ABI half

### B — operation 13, `endpoint_reply_receive`

A new number, taking two capabilities, and doing both things atomically.

```
| 13 | endpoint_reply_receive | reply handle (single use) in rdi;
                                endpoint handle with `receive` in rsi
                              | rdx = the answer's length, r10 = flags |
```

`SYSTEM_ABI_V1` §3's rule for two capabilities is applied as it stands: the
positions follow §5's order, so the reply — which the operation consumes — is
first and the endpoint it then waits on is second. The values move after them.

**Atomic means one thing precisely**: the process does not run at CPL 3 between
the answer being delivered and the wait being entered. There is no instant in
which it holds a spent reply capability and is not waiting.

Failure modes, and they are the part that needs deciding rather than describing:

- the reply capability does not resolve → refused, **nothing is delivered and no
  wait is entered**; the operation is a no-op with a status, exactly as a failed
  send transfers nothing (`IPC_V1` §9.3);
- the endpoint capability does not resolve or lacks `receive` → refused the same
  way, **and the reply is not delivered either**, because a half-performed
  operation would leave the caller answered and the server not waiting, which is
  the state this operation exists to make impossible;
- both resolve, the answer is delivered, and the wait is then cancelled
  (ADR-0059) → `E_CANCELLED`, and **the reply stands**. Cancellation ends a wait;
  it cannot un-answer a caller that has already been answered, and pretending
  otherwise would make a delivered message conditional on something that happened
  afterwards.
- `IPC_V1` §2's one-receiver rule applies unchanged, and is already enforced
  where authority is granted rather than where a receive is called.

### C — a flag on `endpoint_reply` meaning "and then wait"

The same semantics under operation 4, with the endpoint in a second capability
argument that is read only when the flag is set.

Costs: it makes one operation number mean two operations, and makes *which
capabilities an operation requires* depend on a runtime flag. Every check that
exists to be static — the schema's declaration, the verifier's proof, the
conformance test that "every operation in §5 refuses with `E_NO_CAPABILITY` when
the required capability is absent" — becomes conditional on a bit. It saves one
number and `SYSTEM_ABI_V1` §7 says numbers are the cheap thing: "assigned once
and never reused" is a rule about not *recycling* them, not about hoarding them.

### D — leave the bound unmet and record the divergence

Costs: `docs/35`'s number is not decoration; it is the one quantitative Stage 3
IPC obligation that can be checked without a clock, and it is checkable **now**.
Leaving it unmet while the counters exist to measure it is choosing to have a
measured, recorded failure rather than a fixed one.

## Recommendation

**S-A and B.** The schema gains per-parameter capability requirements including
the right; the ABI gains operation 13.

S-A because it is the only option that keeps the property the instruction named:
each capability separate, explicitly passed, with its own interface, binding and
right, and checkable by the verifier. S-B merges authority the system is built to
keep separable; S-C removes the very thing that makes an artifact say which
interfaces it reaches.

B over C because a flag that changes which capabilities an operation requires
turns a static check into a dynamic one, and every existing conformance test of
this ABI is written against the static form. B over D because the bound is
checkable today with counters that already exist.

**Declaring the right in the schema is not incidental to this decision.** It is
`docs/42` §2's "requested operation/right" arriving where the contract already
said it belonged, and it is what makes "an endpoint with `receive`" a thing the
verifier can check rather than a thing the nucleus discovers.

If S-A and B are accepted:

1. `SYSTEM_INTERFACE_V1` §4.1 gains capability parameters: each declares an
   interface path and a required right, and the first remains the operation's
   own interface.
2. Every operation's existing capability requirement is restated as a declared
   right — `endpoint_send` requires `send`, `endpoint_receive` requires
   `receive`, and so on — so the new column is filled for all of them rather
   than only for the new one.
3. `SYSTEM_ABI_V1` §5 gains operation 13 with the register assignment above, and
   §7 records it as a minor version by the same rule operation 12 was.
4. `tos-ir/v1`'s `Op::Capability` carries the further imports. Its `import` field
   is one index; the others are operands of the instruction, which is a use of
   the existing operand list rather than a schema change.
5. The verifier proves, per capability parameter: the named import exists, its
   interface is the declared one, and the enclosing function declares it.
6. The nucleus implements 13, and the runtime image performs it.

## Evidence required

Everything below is an automated gate, and the last three are the ones that
would fail quietly if the others were written carelessly.

1. **Operation numbers.** The contract's assignment and every implementation's
   constants agree, and the assignment is `1..n` exactly once each. *(Already
   built: `check-abi-operations.sh`, written for the ADR-0054 drift this ADR's
   preparation found.)*
2. **Multi-capability validation.** The operation refuses when *either*
   capability is absent, of the wrong type, or lacks its declared right — four
   refusals, distinguishable by status, not one.
3. **No substitution.** Passing the endpoint where the reply belongs, and the
   reply where the endpoint belongs, is refused. Neither position accepts the
   other's object even when the caller holds both.
4. **No extra authority.** A process holding `receive` on an endpoint and a
   reply capability gains nothing it did not hold: it cannot send on the
   endpoint, and it cannot answer a call it was not given the reply for.
5. **Single use.** The reply capability is spent by the operation. A second
   `endpoint_reply_receive` with the same reply handle is refused, and the
   refusal happens **before** any wait is entered.
6. **Cancellation.** A process cancelled while waiting in the receive half
   returns `E_CANCELLED`, and the answer it delivered before waiting is still
   delivered — checked from the caller's side, which received it.
7. **Four crossings.** One steady-state exchange costs ≤ 4, counted by the
   nucleus's own counters over a boot whose only IPC is that exchange, with the
   balance invariant (`ipc_in == ipc_out`) asserted alongside so the count is an
   instrument rather than a number.
8. **No surplus copies and no allocation.** The exchange costs no more payload
   copies than `IPC_V1` §8 allows, and the nucleus allocates nothing on the path
   — the second of which is structural (the nucleus has no allocator) and is
   asserted as such.

## What each option costs to build

| | S-A + B | S-A + C | S-B | D |
|---|---|---|---|---|
| Capabilities stay separate and separately attenuable | yes | yes | **no** | — |
| Schema declares the required right | yes | yes | partly | no |
| Which capabilities an operation needs is static | yes | **no** | yes | — |
| Existing conformance tests keep their shape | yes | **no** | yes | yes |
| ABI numbers spent | one | none | one | none |
| `docs/35`'s bound met | yes | yes | yes | **no** |

## Boundary

This decides how an operation states that it requires more than one capability,
and adds one operation that does. It decides nothing about *results*:
`SYSTEM_INTERFACE_V1` §5 keeps every result an `i64` status, and the received
message's length continues to arrive where `SYSTEM_ABI_V1` §5 already puts it.

It decides nothing about regions, which remain undeclared by any interface and
therefore unoriginated (`IPC_V1` §9.6, resolved by reading).

Nothing already built changes: the capability model, the one-receiver rule, the
liveness rule, the endowment binding of ADR-0061 and the argument marshalling of
ADR-0062 all stand as they are. What is added is an operation that costs one
crossing pair where the present pair of operations costs two.
