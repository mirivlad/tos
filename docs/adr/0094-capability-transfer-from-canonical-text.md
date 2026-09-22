<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0094: How a canonical textual module carries a capability in a message

- Status: **Resolved as §6.1's option A — textual schema and runtime only, over
  existing selectors** (Project Architect-directed closure, 2026-09-21). The
  nucleus was not changed and no ABI operation was added; schema rows and
  records over the existing selectors were enough, which is what §6.1 describes.
  It was reached by implementing the nearest slice rather than by weighing the
  four against each other, and this line names the outcome rather than claiming
  a weighing that did not happen. §11 records what was built, §11a what it did
  **not** settle, and §11b a correction to both
- Date: 2026-09-21, resolved 2026-09-21
- Decision level: **2 or 3, depending on the option** (§8). Every option adds
  TOS Core surface and therefore a language minor; option B additionally adds
  `SYSTEM_ABI_V1` operations, which is what took ADR-0086 to level 3
- Related: **`IPC_V1`** §3 (bounds), §4 (request and reply, and where the reply
  capability arrives), §5 (regions travel by their own rules), §6 (capability
  transfer, delegation, and what is queued); **ADR-0058** (bulk call arguments:
  the transfer table and its offsets); **ADR-0057** (the counts:
  `MAX_TRANSFERRED_CAPABILITIES = 4`, `MAX_TRANSFERRED_REGIONS = 2`);
  **ADR-0063** (operation 13 `endpoint_reply_receive`); **ADR-0061** (a grant
  binds to an `import capability` request); **ADR-0080**, **ADR-0085**
  (capability representation, and that no erased capability value exists in
  TOS Core); **ADR-0086** (the precedent for adding source operations and a
  language minor); **ADR-0093** §3a (P3: the registry is a textual name
  service, which is what needs this); `CAPABILITY_V1` §3, §4, §7;
  `SYSTEM_INTERFACE_V1` §4

## 0. What this decision is for

ADR-0093 accepted P3: the interface registry is an ordinary textual service,
publication and lookup are ordinary IPC. Implementation of that decision was
started and stopped immediately, because **no canonical textual module can
serve a call or carry a capability in a message.** This ADR is about the
missing surface and nothing else.

**Nothing is chosen here.** Four options, their consequences, and what each
keeps or breaks of the accepted contracts.

**Two things this ADR refuses to do**, because the brief refuses them and so
does the architecture: it does not reach for a launcher-mediated direct
endowment, and it does not accept a Rust runtime stage as a stand-in for
canonical text. Either would leave P3 unproved while looking like progress.

## 1. What already works, and it is more than expected

**The nucleus transports capabilities in messages today, completely.** This is
not a mechanism to be built; it is a mechanism with no textual caller. Read out
of `nucleus/src/syscall.rs` and `nucleus/src/process.rs`:

| step | where | what it does |
|---|---|---|
| the sender's handles are read | `resolve_transfers` | reads `MESSAGE_CAPABILITIES + 8*i` from the **sender's own** argument region, for `i < r10` |
| holding is the only right needed | same | `resolve(caller, handle, 0)` — "sending a capability is not an operation *on* the object it names" |
| regions are refused here | same | `is_delegable()` excludes `Region` and `DmaRegion`; they travel by `IPC_V1` §5's own rules and their own bound of two |
| rights that travel | same | `rights_of(caller, handle)` — **exactly what the sender holds**, never more; sending less is what `capability_attenuate` (operation 5) is for |
| the reply is created | `call` | `Object::Reply { caller, generation: next_reply_token(caller) }` with `RIGHT_REPLY`, made by the nucleus at the moment of the call |
| where the reply goes | `send_transaction` | `granted[MAX_TRANSFERRED - 1]` — the **last** slot, always, so a receiver needs no count |
| the queue takes ownership | `retain_in_transit` | before the sender loses anything, so a message in flight is never held by nobody |
| the receiver gets its own names | `hand_over` | `capability::grant(receiver, object, rights, scope)` per slot, written into the **receiver's** table; unfilled slots zeroed |
| bounds | ADR-0057 | four capabilities and two regions per message; a call spends one capability place on its answer |

**And the reply's lifetime is already exact.** `reply_generation` advances in
exactly two places: when a `Waiting::Reply` wait ends *however* it ends —
answered, cancelled, or the caller taken away — and when the caller's process
is over. So the capability stops naming anything the instant the call it names
is over, by construction rather than by anyone remembering.

**The existence proof is a gate that passes today.** `deputy.sh` and
`request-reply.sh` exercise all of this. Neither uses a `.tos` vector: both are
Rust runtime stages, and the runtime reads its reply out of
`MAX_TRANSFERRED_CAPABILITIES - 1` with its own `transferred()` helper.

## 2. What is missing, and it is only the textual surface

Three gaps, all in TOS Core's schema and the runtime bridge, none in the
nucleus:

**2.1 A module cannot write the transfer table.** `endpoint_send`,
`endpoint_call` and `endpoint_send_text` declare `u64` or `string` and nothing
else. There is no way to say "carry this capability", and no way to set the
count in `r10`.

**2.2 A module cannot read the transfer table.** Nothing returns a handle the
nucleus wrote there. So a received delegated capability is unreachable, and so
is the reply.

**2.3 Nothing produces a `system.ipc.Reply`.** The interface is declared with
two operations — `endpoint_reply` and `endpoint_reply_receive` — and **no
operation of the whole schema has it as a result**. `endpoint_receive`'s result
is `i64`.

**2.3 is worse than a missing producer, and this is the part not to skip.**
The only way a module could hold a `Reply` today is `import capability`, and
that is exactly how `SYSTEM_INTERFACE_V1` §"Capability parameters"
illustrates it:

```tos
import capability system.ipc.Reply    as answer;
import capability system.ipc.Endpoint as inbox;
```

That shape cannot be right. An `import capability` is answered from a launch
endowment (ADR-0061), granted before the process runs; a reply capability names
**one specific call** by **one specific caller**, is created when that call is
made, and dies when it ends. A module endowed with one at launch would hold the
right to answer a call that had not happened. So the declared surface assumes
something a reply is not — the same class of finding as `OBJECT_INTERFACE = 4`
being spent in the launch ABI while `nucleus/src/capability.rs` has no such
object kind.

**Corroborated by sweep, not by inference.** Four `.tos` vectors touch IPC —
`supervisor`, `process-control`, `interface-operation`, `supervision` — and all
four only *drain* with `endpoint_receive`. No canonical textual module has ever
answered a call.

## 3. Lifecycle and ownership of a per-call `Reply`, before any syntax

The brief is right that `Result<system.ipc.Reply, i64>` must not be adopted
because it reads well. Here is what the object actually is, from the source:

- **Who creates it.** The nucleus, inside operation 3, before the message is
  queued. Not the caller, not the receiver, not the runtime.
- **What it names.** The calling *context* and a generation token taken at
  creation: `Object::Reply { caller, generation }`. It does not name the
  endpoint, the message, or the payload.
- **Who holds it, and when.** Nobody, briefly: it is placed into the message's
  granted set and the **queue** retains it, so it survives the sender releasing
  handles or ending before delivery. On delivery, `hand_over` grants the
  **receiver** its own handle with `RIGHT_REPLY`. The caller never holds it.
- **When it dies.** When the wait it answers ends, by any route — replied,
  cancelled, or the caller gone — and when the caller's process ends. Both
  advance `reply_generation`, after which the handle resolves to nothing.
- **What "single-use" is.** A counter, not a flag: `IPC_V1` §4 says so and the
  code matches. Nothing has to remember to clear it.
- **An open question this ADR must answer rather than inherit.**
  `is_delegable()` is `!is_region() && !is_dma_region()`, so a `Reply` **is**
  delegable, and `resolve_transfers` refuses only regions. A receiver can
  therefore forward the right to answer a call to a third party. That is
  defensible — the right to answer one call, once, is a capability like another
  — and `launch_plan_endow` already refuses a `Reply` explicitly, so the two
  paths differ deliberately. But a textual surface would make it reachable from
  canonical text for the first time, and this decision should say whether that
  is intended.

**What follows for the type.** The value a receiver obtains is not "a reply"
in general; it is the right to answer the call currently at the head of what it
just received. Any option must make three things true: it cannot be obtained
without a receive having happened; it cannot outlive the call; and holding it
must not imply anything about the endpoint it arrived on, because the two are
separate authorities — which is precisely the property ADR-0063 built operation
13 around.

## 4. Does this need a new ABI selector?

**On the evidence, the transport needs nothing.** Every byte the textual
surface would need is already written or read by the nucleus at the fixed
offsets ADR-0058 fixed:

- to send a capability, the bytes go in the sender's own table and the count in
  `r10`, both of which operations 1 and 3 already consume;
- to receive one, the nucleus has already written the receiver's handle into
  the receiver's own table before the receive returns;
- the reply is already in the last slot.

So the question is not "can the data move" but **"who is allowed to say what
the moved thing is"** — and that is where an argument for a new selector
appears, in §6.3.

## 5. `Produced` and `PERFORMED`, concretely

The bridge in `runtime-image/src/main.rs` describes each operation as
capability registers, value slots and a `Produced`. Today's `Produced` is
`Status`, `Authority` (the handle in `rdx`), `CreatedProcess`, `Mapping` and
the rest — **all of them read a register or a fixed record**, none reads the
transfer table.

What any option needs:

- **a value slot meaning "put this held capability's handle into transfer slot
  *i*, and count it"**, the analogue of the existing `Slot::Held(Reg)` which
  passes a held capability in a register;
- **a `Produced` meaning "the handle at transfer slot *i*, as a capability of
  the declared interface"**, with *i* fixed by the row — `MAX_TRANSFERRED - 1`
  for a reply, a small constant for a delegated capability.

Both are additions to the bridge's vocabulary, not to its authority: the bridge
would read and write offsets the contract fixes, in a region the nucleus chose
and gave this process.

## 6. The options

### 6.1 Option A — textual schema and runtime only, over existing selectors

New schema rows over the existing ABI numbers, following the documented
precedent of **two schema rows over one selector** (`endpoint_send_text` sits
over selector 1 beside `endpoint_send`). Roughly:

- a receive row whose result is `Result<system.ipc.Reply, i64>`, over selector
  2, leaving today's `endpoint_receive -> i64` exactly as it is;
- a send/call row per carried interface, declared the way `endow_for_launch` is
  declared once per interface, so the carried capability is the operation's own
  first capability and its nominal type is retained at the call site;
- a receive row per expected interface for a delegated capability.

**Keeps:** every nucleus contract untouched — `IPC_V1` §3–§6, ADR-0057's
counts, ADR-0058's offsets, ADR-0063's operation 13, ADR-0086's ordering. No
`SYSTEM_ABI_V1` change at all. ADR-0093 P3 becomes implementable. ADR-0085's
"no erased capability value" is respected by declaring per interface rather
than by an `any` type.

**Breaks, or at least strains, one thing — and it is the reason this option is
not obviously right.** A receive row that declares
`Result<system.ipc.Endpoint, i64>` is a *static* claim about a handle whose
object kind was decided by **the sender**. Nothing in the language proves it.
The failure is fail-closed at first use — `capability::resolve` matches on
object kind and answers `E_NO_CAPABILITY` — and a handle cannot be forged,
because handles are indices into a table the process cannot address
(`CAPABILITY_V1` §7). So no authority is gained. But the verifier would have
proved a type that the runtime cannot establish, which is a soundness claim
this language has not had to weaken before.

### 6.2 Option B — new ABI operations

Selectors that name the transfer table explicitly: take slot *i* and refuse
unless it names an object of kind *K*; place a held capability into slot *i*.

**Keeps:** the static type becomes a checked fact rather than a claim, because
the **nucleus** verifies the kind before handing the handle back, and the
nucleus is the only party that knows it. Option A's strain disappears.

**Costs:** `SYSTEM_ABI_V1` §5 grows, which is the closed table ADR-0079 and
ADR-0063 each argued carefully before touching; two operations that duplicate
data movement the existing operations already perform; and a second way to
obtain something a receive already delivered, which is the kind of second
truth this project avoids.

### 6.3 Option C — hybrid

The send side by schema alone (the sender's own handle, its own kind, nothing
to verify), and the receive side by a new selector that returns a kind-checked
handle. Splits along exactly the line §6.1's strain falls on: **what a process
already holds needs no check; what a stranger sent it does.**

**Keeps:** everything A keeps, plus B's soundness, at half B's ABI growth.
**Costs:** two mechanisms where one might do, and the asymmetry has to be
explained every time someone reads the schema.

### 6.4 Option D — an existing mechanism, if one is found

Recorded because the brief rightly asks. **None was found.** The sweep covered
every `Produced`, every schema row of `system.ipc.Endpoint` and
`system.ipc.Reply`, every `.tos` vector, and `SYSTEM_INTERFACE_V1` §4's own
worked example. The closest thing to a provision is the `system.ipc.Reply`
interface itself, and §2.3 is the argument that it is a provision whose shape
does not fit. Option D is kept as an option only so that its rejection is on
the record.

## 7. What each option does to the accepted contracts

| | A | B | C | D |
|---|---|---|---|---|
| `IPC_V1` §3–§6 | unchanged | unchanged | unchanged | — |
| ADR-0057 counts | unchanged | unchanged | unchanged | — |
| ADR-0058 offsets | unchanged, and relied on | unchanged, and named in an ABI row | unchanged | — |
| ADR-0063 operation 13 | unchanged; gains its first possible textual caller | unchanged | unchanged | — |
| ADR-0086 ordering | untouched | untouched | untouched | — |
| ADR-0093 P3 | implementable | implementable | implementable | **not implementable** |
| `SYSTEM_ABI_V1` §5 | **unchanged** | two operations added | one operation added | — |
| ADR-0085 no erased value | respected | respected | respected | — |
| language soundness | a declared type the runtime cannot establish | checked by the nucleus | checked where it matters | — |
| decision level | 2 | 3 | 3 | — |

## 8. What the decision must settle, beyond picking a letter

1. **Whether a `Reply` may be forwarded from canonical text** (§3's open
   question). `launch_plan_endow` refuses one; a message send does not.
2. **Which object kinds may be carried at all.** Today: everything except a
   region and a DMA region. The textual surface could expose that whole set or
   a narrower one. Stage 4 needs exactly one kind — an endpoint.
3. **Whether the receive row's declared interface is a claim or a check**,
   which is the whole of A versus B and C.
4. **Whether the reply row replaces or joins `endpoint_receive`.** Joining
   keeps four existing vectors compiling unchanged; replacing means one way to
   receive.

## 9. Required outcomes, as the brief asks

### 9.1 Minimum new surface

Under any option: one way to place a held capability into an outgoing message,
one way to take a delegated capability out of a received one, and one way to
obtain the per-call reply. **Three capabilities of expression, not three
mechanisms** — the reply is a special case of the second, at a fixed slot.

### 9.2 What stays unchanged in the nucleus ABI

Under A: everything. Under C: the send side and all five IPC selectors keep
their present meaning. Under B and C, the existing selectors are not
re-specified — anything added is additional, and operations 1, 2, 3, 4 and 13
continue to mean what `IPC_V1` and ADR-0063 say they mean. No option changes
the queue bound, the transfer counts, the offsets, region rules, the ordering
contract, or what rights travel.

### 9.3 New capability and lifetime obligations

- A textual holder of a reply must be unable to keep it past the call: already
  true mechanically, and it becomes a **stated** obligation once text can hold
  one.
- A textual sender delegates at exactly its own rights; sending less requires
  `capability_attenuate` first. This is already the rule and becomes visible.
- A textual receiver's handle is its own name, with its own generation; nothing
  about the sender's indices is visible. Already true, now relied on by text.
- If §8.1 permits forwarding a reply, the obligation that a forwarded reply
  still dies with its call must be tested, not assumed.

### 9.4 Tests required before any production implementation

1. A textual service receives a call, obtains the reply, answers it, and the
   caller — also textual — gets the answer. This has never happened.
2. The reply is refused after the call ends: answered twice, or answered after
   the caller was terminated.
3. A capability delegated in a message arrives with the sender's rights and no
   more, and an attenuated send arrives with less.
4. A region and a DMA region are each refused in the generic transfer path, as
   they are today, now attempted from text.
5. A receive row declaring one interface, handed a capability of another kind,
   fails **closed** — and the assertion says whether it failed at reception
   (B, C) or at first use (A).
6. The transfer table cannot be used to forge: a textual module writing an
   arbitrary number where a handle goes gains nothing.
7. The counts hold from text: five capabilities refused, a call carrying four
   of its own refused because one place is the answer's.

### 9.5 The Stage 4 claim this makes possible

**One, and it is the one blocking everything else:** a canonical textual client
reaches a canonical textual service through IPC and receives an endpoint
capability it did not have before, granted by a third textual service. That is
ADR-0093 P3, and with it slice 1 of the Stage 4 client/service path —
publication, lookup, the capability boundary, and stale-client case B.

### 9.6 What remains unproved after this

Everything else. No device is touched: no VirtIO, DMA, MMIO or interrupt path
changes or is exercised. Case C still needs a real restart and republication
path. Case D remains **intentionally ambiguous** and this ADR adds no
idempotency, no request journal, no transaction identity and no exactly-once
semantics — ADR-0093 §5a is unchanged and unweakened. Persistent storage, the
capsule-to-repository handoff and the Stage 4 performance report remain outside.
And Stage 4 does not close.

## 10. What this ADR does not decide

- The `block.device.v1` message shape (ADR-0093 §9).
- Any use of `OBJECT_INTERFACE = 4`. P3 is a textual name service and this ADR
  does not resurrect a ring-0 registry.
- Region transfer from text. Regions have their own rules (`IPC_V1` §5) and
  their own bound, and nothing here needs them.
- Any widening of the nucleus's transport: the counts, offsets and rules of
  ADR-0057 and ADR-0058 stand as they are.

## 11. How it was resolved, and what it did not settle

**Written after the fact, from two boots that pass.** The Project Architect
directed that this ADR stay research until the nearest vertical slice was
finished, and that it close this way if no new architectural obligation
appeared. None did.

**What was built** — `capability-transfer.sh` and `name-service.sh`:

| | |
|---|---|
| nucleus | **unchanged**. Transport, reply creation, delegation rights and the receiver's own naming were all already there |
| ABI | **unchanged**. No operation added; the rows sit over selectors 1, 2, 3 and 6 |
| schema | `system.ipc.ReceivedCall { reply, carried }`, and `endpoint_receive_call`, `endpoint_call_carrying`, `endpoint_send_carrying`, and `capability_release` on `system.ipc.Endpoint` |
| bridge | `Placed`, telling a register destination from a transfer slot, and `Produced::ReceivedCall`, which reads the two slots the nucleus filled |

This is §6.1's option A, and the §6.1 strain was measured rather than argued: a call carrying nothing leaves the slot zero, the receiver
holds a value the language calls an endpoint and the nucleus calls nothing, and
the first use of it is refused `E_NO_CAPABILITY`. Declared type is a claim the
nucleus checks at use, and it fails closed.

**Two things the implementation found that the analysis had not.**

1. **A reply cannot carry a capability.** `ipc::hand` copies payload bytes from
   the replier's argument region into the waiting caller's and never touches
   the transfer table. So a service whose answer must deliver a capability
   delivers it as a message, to a channel the asker handed over in its own
   request. That shaped ADR-0093 P3's lookup protocol and required nothing new.
2. **`IPC_V1` §2 makes `capability_release` necessary on an endpoint.** One
   process may hold `receive`, so a launcher cannot hand a child a `receive` it
   is still holding — `grant` refuses the creation whole. Endowing the plan and
   then releasing its own name is how a receiving role is given away.

## 11a. What this closure does not settle

**Not a claim to have settled it, and not left to prose.** §3's open question
stands: `is_delegable()` admits a `Reply`, so the right to answer a call can be
forwarded, while `launch_plan_endow` refuses one. Nothing built here forwards a
reply, so nothing here decides whether canonical text should be able to.

- Open question: ADR-0094-Q1 — `is_delegable()` admits a `Reply` while
  `launch_plan_endow` refuses one, so the right to answer a call may be
  forwarded in a message and may not be endowed to a child. §3 and §8.1 asked
  whether canonical text should be able to forward one; this closure did not
  answer it, because nothing it built forwards a reply. Until it is answered, no
  textual surface may be added that places a `Reply` in a message's transfer
  table.

**The line above is the tracked form, and `scripts/check-open-decisions.sh`
reads it.** An open question written only as prose is lost the moment its ADR
stops being read, which is exactly what happens when the ADR closes — and this
one closed the day it was raised. The gate compares every `- Open question:`
line in `docs/adr/` against the journal's own list, so the project cannot say
machine-checkably that nothing is open while this stands, whatever the parent
ADR's status line says.

## 11b. Correction, 2026-09-23

**§11 understates what was added, and this ADR's first status line contradicted
itself.** Recorded here rather than edited into §11, because §11 is what was
true on 2026-09-21.

1. **The status line said "No option was chosen" while §11 said "the outcome is
   A".** Those are not the same statement, and an external audit read them as a
   contradiction — correctly. The formulation now used is the second: the
   outcome is option A, reached by implementation rather than by weighing.
   Nothing about what was built changed.
2. **The schema surface grew again on 2026-09-23, and one row was retired.**
   `system.ipc.ReceivedCall` gained a `word` field; `system.ipc.Answer` was
   added; `endpoint_call_word` and `endpoint_reply_word` joined selectors 3 and
   4; and `endpoint_call_for` — which produced a call's answer *length* as the
   answer — was removed. The reason is in `SYSTEM_INTERFACE_V1`
   §`system.ipc.Endpoint`: with nothing able to read a received payload, the
   first protocol built on these rows carried its numbers in the inline length
   register, which describes a message whose declared size is not its size.
   Still option A, still no ABI operation, still no nucleus change.