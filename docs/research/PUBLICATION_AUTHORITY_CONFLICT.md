<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Publication authority: what ADR-0093 accepted and what was built

Status: **Research note. It accepts nothing, chooses nothing and is authority
for nothing.** It is the conflict report ADR-0093-Q1 names, written so that the
Project Architect has the whole gap, the options and their consequences in one
place. `docs/38` assigns authority; this note has none.

Date: 2026-09-23. Raised by an external audit of commits `6c17b10`, `b276fde`
and `89f7599`, whose finding is confirmed here rather than argued with.
**§9–§18 were added the same day**, on direction to examine Q1-A deeply: §9 is
what that study established, §14 is the constraint that makes §5's sketch of Q1-A
unimplementable as written, and §13 corrects §7.

**§19–§26 were added after a second review of `e2ad80a`, and they supersede
§9–§18 wherever they contradict them.** Read them before acting on anything
earlier. In short: §15's schema is self-contradictory (§20); the nominal interface
is not preserved through a launch plan, so both retyping directions succeed today
(§21); Q1-A therefore cannot prove the conformance item it exists for (§22); §11's
"a trust boundary, not a hole" is withdrawn (§23); §17's recommendation of Q1-A is
withdrawn and no replacement is offered (§26); and §18's decision text must not be
adopted.

## 1. The finding, in one paragraph

ADR-0093 was accepted as option P3 on 2026-09-21 and says that a publisher
presents **its publication capability** when it registers. `CAPABILITY_V1` §6
and ADR-0051 §2 say what such a capability is: an authority **whose nominal type
is the interface being published**, requested by the module through
`import capability`, read by the launcher out of the verified IR's
`capability_imports`, and granted or denied under policy. What
`name-service.sh` and `block-service.sh` build is not that. The block service
holds an ordinary `system.ipc.Endpoint` under the binding name `publish`; the
registry registers whatever arrives on the endpoint it receives on; no interface
name travels in the protocol at all; and nothing anywhere in either boot names
`block.device.v1`. The implementation is the suspect, not the ADR.

**What was corrected without a decision**, in the same commit as this note: every
comment, gate line and journal sentence claiming that two endpoints *are*
`CAPABILITY_V1` §6's publication authority. They are not, and the claim was the
more serious half of the defect — code that under-implements an accepted
contract is a gap, and documentation that says the gap is the contract is drift.

## 2. What is actually decided, quoted

| where | what it fixes |
|---|---|
| `CAPABILITY_V1` §6 | "The right to publish an interface is itself a capability, whose nominal type is the interface (ADR-0051 §2). A process that holds it may register; one that does not, cannot." |
| ADR-0051 §2 | the source form is `import capability net.adapter.V1Publisher as publisher;` — "the nominal capability type **is** the interface being published"; the launcher "reads `capability_imports` from the verified IR, sees exactly which interface the module intends to publish, and grants or denies it under policy" |
| ADR-0051 Evidence | "A service whose publish capability is denied fails to start with `CapabilityDenied`, does not appear in the interface registry, and says so in the audit record." |
| ADR-0093 §3a.1 | the publication capability is held by "each publisher, granted by its launcher from a sealed launch plan … as the authority to publish `block.device.v1` and nothing else" |
| ADR-0093 §3a.2 | "The block service calls the name service over IPC, **presenting its publication capability** and the endpoint it wants named." |
| ADR-0093 §10.1 | "A service that was not granted the publication capability cannot publish, and says so in the audit record (ADR-0051's requirement, finally testable)." |

## 3. Why the accepted meaning cannot be implemented as the tree stands

Three separate blockers, each of which is a decision rather than a task.

**§10–§15 carry these three further and one of them is now answered as a
proposal.** §3a stands: a per-interface path is a new Tier 2 contract, and §15
specifies the one Q1-A would need. §3b's undecided object kind is proposed as
`endpoint`, with §11 showing why two interface paths over one object are legal and
§12 showing what that does and does not enforce. §3c stands as written: a textual
registry still cannot check the kind of what a message handed it, and §12 is where
that lands.

### 3a. No accepted schema declares a publication interface, and it cannot be
### added silently

An `import capability P` is answered only if `P` is in the accepted interface
set: `interfaces::interface(path)` is consulted, and `ACCEPTED` in
`crates/tos-core/src/interfaces.rs` is "every interface `SYSTEM_INTERFACE_V1` §4
declares, and no others". Every path in it today is `system.*` or `platform.*`.

A publication capability for `block.device.v1` needs a path such as
`block.device.V1Publisher`. `SYSTEM_INTERFACE_V1` §2 says where such a thing
belongs and, by saying so, says that it is not a schema edit:

> "A Stage 4 driver interface is another instance of these rules, not a special
> case of this document."

So the path is a **new accepted Tier 2 interface contract** — the same class of
document ADR-0060 admitted and ADR-0079 instantiated for the platform schema —
and instantiating that class is not something an implementation may do for its
own convenience.

### 3b. Which object kind a publication capability names is undecided, and both
### answers are already constrained

`ObjectKind::InterfacePublication` exists in the frontend and
`OBJECT_INTERFACE = 4` is allocated in `tos-launch`'s sequence, while
`nucleus/src/capability.rs` has no such kind. ADR-0093 §2 records this as
"reserved and unimplemented", and §4 says P1, P3 and P4 "leave it reserved and
empty". So:

- filling it is P2's move, and P3 was accepted instead;
- not filling it means the object behind a publication capability is an existing
  kind, and the only candidate is `Endpoint`.

Declaring an interface whose path is `block.device.V1Publisher` and whose object
kind is `endpoint` is coherent — `Interface.object` is documented as "a check on
a grant, never the rule that chooses one" — but it makes the object-kind check
vacuous for this authority: any endpoint satisfies it, and the whole weight
falls on the launcher's binding. Whether that is the intended reading of
`CAPABILITY_V1` §6 is exactly the question, and it is not one an implementation
should answer by picking the option that compiles.

### 3c. A textual registry cannot check what it was handed

Under P3 the registry is canonical text. A capability arrives as
`ReceivedCall.carried`, whose declared type is `system.ipc.Endpoint`; ADR-0094
§11 already records that a declared type there is a claim the nucleus checks at
*use*, not a proof about what arrived. So a textual registry cannot ask "is this
a publication capability for `block.device.v1`?" — nothing in the language or
the schema lets it. Its only honest basis for authority is **which channel the
call arrived on**, which is ordinary capability discipline and is what is built
— but it is authority over *an* endpoint, not over an interface name.

Filling `OBJECT_INTERFACE` would let the nucleus answer the question. That is
P2's trusted-base growth, which ADR-0093 weighed and declined.

## 4. The mandatory negative gate, and why it is not written

The corrective brief requires: *a service that was not granted the publication
capability cannot register `block.device.v1`*, and states that the absence of a
generic `publish` endpoint does not count as that proof. Both halves of that are
right, and together they are why no such gate exists in this commit:

- the **only** thing a service can be un-granted today is the generic `publish`
  endpoint, which is the form the brief excludes;
- `block.device.v1` is not a name anything in the system can register, refuse to
  register, or be asked about, so "cannot register `block.device.v1`" has no
  subject;
- ADR-0051's and ADR-0093 §10.1's own form of the evidence — a startup
  `CapabilityDenied` naming the interface, in the audit record — requires §3a's
  interface path to exist.

**§13 supersedes the last of those three.** The negative gate is designable
today and §13 designs it: a publisher declaring the import and not endowed it is
refused before its first instruction, in a separate boot, with a refusal line that
already exists. What remains missing is only §3a's interface path — so the gate is
still not written, and for one reason rather than three.

Recorded as **BLOCKED**, not as passing and not as failing. What *is* gated
today, and what the gates now say in as many words, is narrower: a process that
holds only `lookup` cannot cause a registration, proved by a client carrying its
own inbox into `lookup` and the next lookup still answering with the publisher's
endpoint (`name-service.sh`). That is a real assertion about capability
discipline. It is not ADR-0093 §10.1.

## 5. The options, weighed and not chosen

### Option Q1-A — a per-interface publication capability over the endpoint kind

A new accepted Tier 2 schema document declares `block.device.V1Publisher` with
object kind `endpoint` and representation `AsInterface`. The publisher writes
`import capability block.device.V1Publisher as publication;`, the launcher
endows it with a name for the registry's publication endpoint for that interface
and nothing else, and a module not endowed it fails to start with
`CapabilityDenied` naming the interface.

- **Keeps:** the nucleus untouched; `OBJECT_INTERFACE` reserved and empty; P3
  intact; ADR-0051 §2's source form exactly as written; ADR-0093 §10.1 and
  ADR-0051's evidence line both testable, in the form they ask for.
- **Costs:** a new Tier 2 interface contract to accept — the first non-`system`,
  non-`platform` one, and the precedent for every later driver interface. The
  object-kind check becomes vacuous for this class of authority (§3b): the
  nominal type carries the meaning and the launcher's endowment carries the
  enforcement. One publication endpoint per published interface, which bears on
  `MAX_ENDPOINTS` (§6).
- **Decision level:** 2 under `docs/21`, on the reading that it instantiates
  ADR-0060's admitted class rather than amending it.

### Option Q1-B — fill `OBJECT_INTERFACE` in the nucleus

The reserved kind becomes real: a publication capability is a distinct object,
the nucleus resolves its kind, and a registry — textual or not — can be handed
one it can trust.

- **Keeps:** §3b's check meaningful and §3c's question answerable; the declared
  type becomes a checked fact rather than a claim.
- **Costs:** this is P2's mechanism arriving inside P3. ADR-0093 §4 named the
  cost as "a naming authority in the binary trusted base" and weighed it against
  `docs/38`'s Tier 0 invariants and `AGENTS.md` §8's narrow-nucleus rule. It also
  reopens Stage 5's reconciliation with `/system` as a commit tree and `/dev` as
  a capability namespace (`docs/09`).
- **Decision level:** 3. It changes the trusted base.

### Option Q1-C — the interface name travels as data, checked by text

No new capability type. The publication call carries the interface name in its
payload, and the registry keys its entries by it. Authority stays "you hold a
name for the publication endpoint".

- **Keeps:** nothing new anywhere — the payload already carries a string
  (`endpoint_send_text`), and `block.device.v1` becomes a name the system can
  actually be asked about, which today it cannot.
- **Costs:** **it does not implement `CAPABILITY_V1` §6.** A process holding the
  publication endpoint could register any name, so the authority is not "to
  publish this interface". It would have to be recorded as a deliberate
  narrowing of the accepted contract at Stage 4, with the §6 form deferred —
  which is a decision about an accepted Tier 2 contract, not a smaller version
  of one.
- **Decision level:** 2, and it is an amendment to `CAPABILITY_V1` §6's reach at
  Stage 4 rather than an implementation of it.

### Option Q1-D — defer publication authority, and say so

Leave the implementation as it is, and record in ADR-0093 that §3a.1–§3a.2 and
§10.1 are **not** met at this Stage 4 slice, with the gap named.

- **Keeps:** no work, no new contract, and the tree stops claiming something it
  does not do — which this note's §1 has already done for the claims.
- **Costs:** ADR-0051's evidence requirement stays unsatisfiable, as it has been
  since 2026-08; ADR-0093's conformance obligation §10.1 stays open against an
  accepted decision; and the next slice that needs a second published interface
  inherits a registry with one unnamed slot.
- **Decision level:** 2. Deliberately not meeting an accepted ADR's conformance
  item is a decision, not an omission.

## 6. What every option touches

| | Q1-A | Q1-B | Q1-C | Q1-D |
|---|---|---|---|---|
| nucleus / trusted base | unchanged | **an object kind and an interface namespace** | unchanged | unchanged |
| `SYSTEM_ABI_V1` | unchanged | unchanged, or one operation | unchanged | unchanged |
| accepted schema documents | **one new Tier 2 contract** | one new contract plus the kind | unchanged | unchanged |
| `OBJECT_INTERFACE = 4` | stays reserved | **filled** | stays reserved | stays reserved |
| ADR-0093 P3 | intact | strained: P2's mechanism inside P3 | intact | intact |
| `CAPABILITY_V1` §6 | implemented | implemented | **narrowed, explicitly** | **unmet, explicitly** |
| ADR-0051 evidence | testable | testable | not testable | not testable |
| ADR-0093 §10.1 | testable | testable | not testable | not testable |
| `MAX_ENDPOINTS = 4` | pressed: one publication endpoint per interface | unaffected | unaffected | unaffected |
| `docs/34` threat model | a new authority class to state | a new ring-0 namespace to state | nothing | nothing |

## 7. One implementation constraint the options should be weighed against

**Superseded by §13, and wrong.** It is kept as the record of what was assumed
before anybody counted: it speculated that Q1-A would press `MAX_ENDPOINTS` and
that a negative gate would want a process with an endpoint of its own, it never
looked at `MAX_PROCESSES`, and both halves are false. Read §13 instead.

`ipc::MAX_ENDPOINTS` is 4, over statically reserved storage, and the
`block-service` boot already uses all four: publish, lookup, the service's own
endpoint and the client's inbox. Any option that adds an endpoint per published
interface, or that adds a fourth process with an endpoint of its own — which a
negative gate with an unauthorised publisher would want — needs that bound
raised first. It is a nucleus constant and not a contract number: `IPC_V1` §7
requires only that a queue never be grown to accept a message, which a larger
static table does not affect. Raising it is nevertheless a change to the trusted
base's footprint and is noted here rather than done.

## 8. What this note does not do

- It does not choose an option, and it does not rank them.
- It does not amend `CAPABILITY_V1`, ADR-0051 or ADR-0093. The only edits made
  alongside it withdraw claims that were untrue.
- It does not propose a `block.device.v1` wire shape. ADR-0093 §9 leaves that to
  interface design inside `IPC_V1`, and the request/answer shape corrected in
  the same commit stays inside it: an inline payload, no new mechanism.
- It does not touch Stage 4 closure. Nothing here closes 4C, 4D or Stage 4.

## 9. Feasibility study of 2026-09-23, and what §7 got wrong

**Added after §1–§8, on Project Architect direction to examine Q1-A deeply
without implementing it.** Everything below is read out of the tree at commit
`a1ae3cb`. It still accepts nothing and chooses nothing.

**§7 is superseded by §13 and was wrong.** It speculated that Q1-A would press
`MAX_ENDPOINTS` and that a negative gate would want "a fourth process with an
endpoint of its own". Neither is true, it never checked `MAX_PROCESSES`, and the
paragraph is left in place as the record of what was assumed before anybody
counted. §13 has the arithmetic.

**Q1-A is viable.** It is not viable in the shape §5 sketched, because of a
constraint §5 did not see (§14), and it carries two consequences that have to be
stated rather than discovered (§11, §12). With those recorded it implements
`CAPABILITY_V1` §6, ADR-0051 §2 and ADR-0093 §3a as written, with the nucleus
untouched and no ABI operation added.

## 10. A — the shape of the authority, and why the call form follows

### 10a. What ADR-0093 §3a.2 licenses

> "The block service calls the name service over IPC, **presenting its
> publication capability** and the endpoint it wants named."

Two readings are grammatically available: the authority is *invoked through*, or
the authority is *delegated as payload*. **The second is excluded by §3a.1 of
the same decision**, not by implementation convenience:

> "Each publisher, granted by its launcher … The name service holds the
> registry; **it does not hold anyone's right to publish**."

A capability sent in a message is delegated — `IPC_V1` §6: "Sending a capability
is **delegation**: the sender keeps what it had", and the receiver "gets its own
handle". So a publication capability arriving as payload would leave the registry
holding a publisher's right to publish, which §3a.1 forbids in as many words.
The authority must therefore be the capability the call is **made through**, and
the published endpoint must be the thing that travels.

So the form is

```tos
publish(publication, service_endpoint)
```

and it is a derivation from the accepted text rather than a convenience.

### 10b. That the model can express it

`SYSTEM_INTERFACE_V1` §4.1: "An operation takes **one or more** capabilities, and
they come first, in the order §4 lists them. Each declares the interface it must
be of and the right it must carry; **the first is the operation's own
interface**, which is the one the instruction records and `Signature.effects`
names (ADR-0060)."

Three accepted rows already take two or more: `endpoint_reply_receive`
(`Reply` + `Endpoint`), `endpoint_call_carrying` (`Endpoint` + `Endpoint`),
`process_create_funded` (three). And the bridge's `Placed` list is positional
over the declared capabilities, so the authority in a register and the published
endpoint in transfer slot 0 is an ordering the existing vocabulary expresses:

```rust
capabilities: &[Placed::Register(Reg::Rdi), Placed::Transfer(0)],
values: &[Slot::Fixed(Reg::R10, 1)],
operation: ENDPOINT_CALL,      // selector 3, unchanged
```

**The authority is declared first, and that is load-bearing rather than
stylistic.** §4.1 makes the first capability the one `Signature.effects` records,
so declaring it first is what puts `block.device.V1Publisher` into the verified
IR as the effect of the call. That is the same fact ADR-0051 §2 wants the
launcher to read, appearing a second time in the artifact. It reverses
`endpoint_call_carrying`'s convention of putting the delegated capability first,
and the convention is a convention: `endow_for_launch` adopted it because the
delegated capability *is* that operation's own interface, which here it is not.

### 10c. Worked source shape

```tos
// The authority, whose nominal type is the interface it publishes. Answered from
// the launch endowment, which is the only thing that can answer an import
// (ADR-0061) — see §14, which is why this matters.
import capability block.device.V1Publisher as publication;

// The endpoint being published is an ordinary endpoint and arrives as a value,
// so it is named by the dotted form (ADR-0080) and no import requests it.
extern fn publish(
    cap: block.device.V1Publisher,
    endpoint: system.ipc.Endpoint
) -> i64 uses [publication, system.ipc.Endpoint];
```

Checked against `boundary.rs::unavailable`, which is where an `extern` meets the
schema: one effect per required capability in order, each effect's resolved
interface equal to the requirement's, each capability parameter's *written type*
equal to the requirement's interface path, and the first requirement equal to the
declaring interface. The shape above satisfies all four. **A module cannot
smuggle this authority into an endpoint row**: `uses [publication, …]` on
`endpoint_call_carrying` resolves the first effect to
`block.device.V1Publisher` ≠ `system.ipc.Endpoint` and is `E1801_FFI_NOT_AVAILABLE`
with reason "a capability effect is not of the interface the operation requires".

## 11. B — two names for one endpoint, and why it is not a conversion

**Its closing paragraph is withdrawn by §23**, which is where the two claims it conflated are separated. Everything before that paragraph stands.

The publisher holds a capability nominally `block.device.V1Publisher` with
`call`; the registry must hold one nominally `system.ipc.Endpoint` with
`receive`, over the **same** runtime Endpoint object. The question is whether the
accepted model permits that.

**It does, and the contract that permits it is the object-kind-as-check rule.**
`SYSTEM_INTERFACE_V1` §4: "**The kind is a check, not the mechanism that chooses
a grant.** Which grant answers which request is decided by the binding the module
declared (ADR-0061), because two imports of one interface are legal and a kind
cannot tell them apart." The launcher's implementation is exactly that — the
runtime image's `granted()` finds the endowment by binding name and then tests
`capability.object == wanted`, where `wanted` is the interface's declared
`ObjectKind`. Both paths declare `endpoint`, so both grants validate over one
object.

Three further checks, because a permission by omission is not a permission:

- **`CAPABILITY_V1` §3 attaches no interface to an object.** A capability names
  "the endpoint, region, process or interface publication it refers to" — object
  kinds. There is nothing for two interface paths over one object to contradict.
- **ADR-0085 §4.3 rule 1 is about representations, not interfaces.** "One family,
  one interface" makes representation → interface a function; `AsInterface` is
  exempt "by construction — it resolves through the interface's own path", and
  both paths here are `AsInterface`.
- **`IPC_V1` §2 is satisfied.** One receive-rights holder: the registry. The
  publisher holds `call`. Two names with disjoint rights over one endpoint is
  already what `block-service`'s `serve`/`advertise` pair is, made by the
  launcher rather than by an attenuation.

**And it is not a conversion.** No operation of `CAPABILITY_V1` §2 changes a
capability's interface; `capability_attenuate` narrows rights and nothing else.
The two names are two independent grants the launcher made over one object, and
neither holder can obtain the other's:

| holder | holds | can reach |
|---|---|---|
| publisher | `block.device.V1Publisher`, `call` | `publish` only. `endpoint_call_carrying` is refused at the frontend and at the verifier |
| registry | `system.ipc.Endpoint`, `receive` | `endpoint_receive_call` only. `publish` is refused the same way |

**The residual, stated rather than hidden.** A launcher *could* endow the
publication endpoint to a third module under a `system.ipc.Endpoint` binding,
and that module could then reach it with `endpoint_call_carrying` and register
anything. Two rows over one ABI selector are one instruction to the nucleus. So
the integrity of "only the publisher may publish" rests on the launcher's plan —
which is precisely ADR-0051 §2's model ("the launcher … grants or denies it under
policy") and the launcher is canonical, auditable text. It is a trust boundary,
not a hole, and §12 is where it is located.

## 12. C — enforcement, layer by layer

**Where "this authority is for `block.device.v1` and nothing else" is actually
enforced**, with trusted facts separated from source-level claims. Read out of
the tree, not inferred.

| layer | what it enforces | status |
|---|---|---|
| frontend / checker (`tos-core/src/boundary.rs::unavailable`) | per `extern`: effect count equals required capability count; each effect's resolved interface equals the requirement's; each capability parameter's written type equals the requirement's interface path; the first requirement is the declaring interface | **mechanical, not trusted.** It refuses before an artifact exists, and the artifact is what travels — the reason the verifier repeats it |
| verified IR | `Signature.effects` carries interface paths; every capability instruction carries `unsafe_interface`; an import is typed `TypeDef::Capability(interface)` | **trusted fact.** Covered by the module digest and by `capability_interface_digest` in the header |
| independent verifier | `V2013_CAPABILITY`: "an operation declares X and is performed through Y" — per capability position, the value's interface equals the instruction's declared interface. `V2033_UNSAFE`: the enclosing `uses` admits that interface. Plus: an import must be typed as its own interface | **trusted fact, and independent.** `representation.rs` states in as many words that reading `tos_core::interfaces` would destroy that independence |
| launch binding (ADR-0061) | which endowment answers which `import capability`, by binding name — not by position, not by kind | **trusted fact about the launcher's text.** The policy decision itself is the launcher's |
| launcher object-kind validation (`granted()`) | the endowed object's kind equals the interface's `ObjectKind`; an unanswered request is denied **before the first instruction runs** and emits `TOS.RUN.REFUSED stage=execute reason=capability-denied binding=<name> interface=<path>` | **trusted fact.** This line *is* the audit record ADR-0051 and ADR-0093 §10.1 require, and four existing gates already assert its exact shape |
| runtime bridge (`PERFORMED`) | which ABI selector, which register, which transfer slot | **trusted fact about the boundary**, and it knows nothing of interfaces beyond the row it performs |
| nucleus, at use (`capability::resolve`) | object kind and rights: an `Endpoint` with `call`. **It cannot distinguish a publication capability from any other endpoint capability** | **trusted fact, and the narrowest one.** The nominal type is invisible here, by design: `Object` has no interface field and no `InterfacePublication` variant |
| textual registry | holds `receive` on the publication endpoint and treats every call arriving there as a registration of `block.device.v1` | **source-level claim in canonical text.** Inspectable, and unchanged from today |

**The conclusion, and it is the honest answer to the question.** The per-interface
property is carried by **two** things at once, and by neither alone:

1. **object identity at the nucleus** — the registry receives on a *distinct
   endpoint object per published interface*, so "which endpoint you may call" is
   what the nucleus actually enforces, unforgeably;
2. **nominal type in the verified artifact and in the launcher's mediation** —
   which interface that endpoint *means*, who asked for it, and what the audit
   record says when the request is refused.

`CAPABILITY_V1` §6 — "A process that holds it may register; one that does not,
cannot" — is satisfied by the conjunction. It is **not** satisfied by the nominal
type alone, and any document describing Q1-A must say so; a reader who assumed the
nucleus checked the interface would be wrong.

## 13. D — the negative gate, and the corrected budget

### 13a. The topology, which needs no endpoint for the unauthorised publisher

**§7 assumed the unauthorised publisher would need an endpoint of its own. It
needs none, because it never runs.** A module declaring
`import capability block.device.V1Publisher as publication` whose launcher does
not endow it is refused by `granted()` **before its first instruction**, which is
`Refusal::CapabilityDenied` and emits

```text
TOS.RUN.REFUSED stage=execute reason=capability-denied binding=publication interface=block.device.V1Publisher
```

That is ADR-0093 §10.1's "cannot publish, and says so in the audit record" and
ADR-0051's "fails to start with `CapabilityDenied` … and says so in the audit
record", in one line that already exists. Four gates assert this exact shape
today — `process-control.sh`, `module-operation.sh`, `pci-discovery.sh`,
`irq-routed.sh` — and the last two run it as a **second boot** in `$OUT/denied/`,
which is the established idiom for a negative of this class.

### 13b. The counts, separately for each boot

| | processes (`MAX_PROCESSES = 4`) | endpoint objects (`MAX_ENDPOINTS = 4`) |
|---|---|---|
| positive boot, today | 4: launcher, registry, block service, client | 4: publish, lookup, service, client inbox |
| positive boot, under Q1-A | **4, unchanged** | **4, unchanged** — today's generic `publish` endpoint simply *is* the publication endpoint for `block.device.v1`. One interface, one endpoint |
| negative boot, separate | 3: launcher, registry, the publisher that fails to start | **1**: the publication endpoint the registry receives on |

**So neither bound needs raising, and that is the finding §7 got wrong.** The
negative evidence is a separate boot — which it has to be anyway, since the
positive boot already holds four processes — and in it the unauthorised publisher
consumes a process slot and no endpoint.

Running the registry in the negative boot is worth the third process: it makes
"does not appear in the interface registry" an assertion (the registry reports
zero registrations) rather than a vacuous truth about a registry that was not
there.

### 13c. Where the bound *would* be pressed, recorded as an obligation

Under Q1-A each published interface needs its own publication endpoint, and the
registry needs an `import capability … with receive` for each. So with N
interfaces the registry's endowment holds N + 1 entries against
`MAX_ENDOWMENT = 4`, and the positive boot needs N + 3 endpoints against
`MAX_ENDPOINTS = 4`. **N = 1 fits exactly; N = 2 does not.** Stage 4's scope is
one interface by ADR-0093's own direction ("One interface, `block.device.v1`, one
publisher, one lookup"), so nothing is pressed now — but the second published
interface, whenever it comes, raises a nucleus bound or changes the shape. §16
weighs that against Q1-C, which does not have the property.

## 14. The constraint §5 did not see: the block service's endowment is full

**This is the one thing that makes Q1-A's §5 sketch unimplementable as written,
and it is resolvable.**

An `import capability` is answered **only** from the launch endowment (ADR-0061,
and `granted()` searches `self.held`, which comes from the launch record). A
capability that arrives in a message is a *value*, typed by the producing
operation's result — `ReceivedCall.carried` is `system.ipc.Endpoint` and the
verifier checks it (V2013). So a nominal publication type can reach a module
**only** through the endowment.

And the block service's endowment is already full. `MAX_ENDOWMENT = 4`
(ADR-0077 §2), and it holds:

| binding | what | why it cannot go |
|---|---|---|
| `serve` | its own endpoint, `receive` | it is the receiver (`IPC_V1` §2) |
| `advertise` | its own endpoint, `call` | what it hands to the registry. Cannot be derived from `serve`: attenuation narrows rights and cannot add `call` |
| `device` | `platform.pci.Bus`, `claim` | `dma_region_allocate` needs both this and `budget`, and neither is sufficient alone |
| `budget` | `system.memory.Authority`, `spend` | as above |

This is exactly why commit `89f7599` had the launcher **send** the registry's
publish endpoint in a message: the fifth thing did not fit, and a value was the
only way in. Under Q1-A the fifth thing is the *authority*, which cannot be a
value. Five needs, four places.

**The resolution is an inversion, and it fits exactly with no bound changed.**
Endow `publication` and let `advertise` arrive by message:

| binding | |
|---|---|
| `serve` | endowed, `receive` |
| `publication` | endowed — `block.device.V1Publisher`, `call`. **Must** be an endowment: it is the nominal type |
| `device` | endowed |
| `budget` | endowed |
| the service's own `call` name | **arrives in a message from the launcher**, held as a value of `system.ipc.Endpoint` and passed as `publish`'s second argument |

Four endowments, and the thing that travels as a value is the thing whose type
carries no meaning. That the *authority* must be endowed and the *ordinary
endpoint* may be a value is forced by ADR-0061 rather than chosen, which is what
makes this a derivation rather than a workaround.

**What this costs.** The launcher keeps a `send` name for the service's own
endpoint in order to hand it over after the process exists — it already keeps
`service_send` for exactly this pattern — and the service's first receive becomes
the handover, as it already is today. The registry's canonical text does not
change at all: it still receives on an endpoint it holds `receive` on and reads
`ReceivedCall.carried`. **Textual-registry complexity under Q1-A is zero
additional.**

## 15. E — the Tier-2 document Q1-A needs, as a proposal only

**Superseded by §20 and §24a.** The table below omits `endow_for_launch`, which §20 shows the interface must declare, and the whole section assumes a nominal type that §21 shows is not preserved. Kept as the record of what was proposed.

**Not written here, and this section does not create it.** What follows is the
specification of a document the Project Architect would have to accept.

| | |
|---|---|
| **purpose** | declare the publication authority of `block.device.v1`, and nothing else. **Not** `read`, `write` or `capacity`: ADR-0093 §9 leaves their wire shape to interface design inside `IPC_V1`, nothing needs them as schema rows, and an interface declaring an operation the system does not perform is what `SYSTEM_INTERFACE_V1` §4 forbids |
| **path** | `block.device.V1Publisher`, following ADR-0051 §2's own worked form `net.adapter.V1Publisher` |
| **operations** | one. `publish(cap: block.device.V1Publisher, endpoint: system.ipc.Endpoint) -> i64`, over `SYSTEM_ABI_V1` operation **3**, unchanged and not re-specified |
| **object kind** | `endpoint`. `OBJECT_INTERFACE = 4` stays reserved and empty, as ADR-0093 §2 and §4 leave it under P3 |
| **representation** | `AsInterface` |
| **rights** | `call` on this interface. The registry's `receive` is on a `system.ipc.Endpoint` capability over the same object, and §11 is why that asymmetry is legal — **the document must state it**, because a reader who expected `receive` on this interface would look for an operation that does not exist |
| **importability** | yes, and necessarily: `AsInterface` ⇒ `startup_importable()`, and §14 shows the endowment is the only way a nominal type reaches a module |
| **instantiates, and does not change** | `SYSTEM_INTERFACE_V1` §2 (a schema is a class of document; "a Stage 4 driver interface is another instance of these rules"); ADR-0060 (admits the class); ADR-0061 (a binding answers a request); ADR-0085 §4.3 (representation); `CAPABILITY_V1` §6; ADR-0051 §2; ADR-0093 §3a |
| **adds** | no ABI operation, no object kind, no capability representation, no nucleus change, no IPC mechanism, no language minor (a schema row is not an acceptance rule — `endpoint_call_word` and `ReceivedCall` were added at 1.4 without one) |

### 15a. Decision level and architecture impact statement (`docs/21`)

**Level 2 — contract extension.** It adds versioned behaviour while preserving
invariants, and it requires a design note and an ADR. It is **not** Level 3: no
trust boundary moves, no persistent format changes, no runtime dependency
appears, source identity is unchanged and owner control is untouched. It is the
first instance of ADR-0060's admitted class outside `system.*` and `platform.*`,
which is a precedent worth naming in the accepting decision and is not by itself
a level.

Answering §21's nine questions:

- **Which invariants are affected?** None. `docs/02`'s Tier 0 set is untouched;
  the narrow-nucleus rule (`AGENTS.md` §8) is strengthened rather than weakened,
  because the naming authority stays entirely outside ring 0.
- **What becomes canonical after the change?** One more accepted interface path
  and one operation over an existing selector. The publication authority of one
  interface becomes a declared capability type rather than an undeclared endpoint.
- **What enters or leaves the trusted base?** Nothing enters. The nucleus, the
  ABI and the object-kind set are unchanged; `OBJECT_INTERFACE` stays reserved
  and empty.
- **Can the active runtime still identify its exact source?** Yes, and more
  precisely: the intent to publish `block.device.v1` becomes a fact in the
  verified IR's `capability_imports` and in `Signature.effects`.
- **Can all derived artifacts be discarded and regenerated?** Yes. Nothing
  persistent is added.
- **Can the owner still recover and boot a previous commit?** Yes. A module that
  does not declare the import is unaffected.
- **Does the change create a hidden host dependency?** No.
- **Does it alter licensing or patent exposure?** No.
- **How is the behaviour tested?** §13: a positive boot where the publisher holds
  the authority and registers, and a separate negative boot where a publisher
  that was not endowed it is refused before its first instruction, with the
  refusal line asserted and the registry reporting zero registrations.

## 16. F — the four options, re-compared after the study

**Superseded by §24c**, which recomputes it with Q1-A failing, a new Q1-A2, and Q1-B re-read on better terms than this section gave it.

**No option is chosen here.** Q1-A is examined in depth because the direction
asked for that; the others are re-weighed against what the study found.

| | Q1-A per-interface nominal authority over an endpoint | Q1-B fill `OBJECT_INTERFACE` | Q1-C interface name as payload data | Q1-D defer, and say so |
|---|---|---|---|---|
| **contract fidelity** | `CAPABILITY_V1` §6 and ADR-0051 §2 as written, by the conjunction of §12's two layers. ADR-0093 §3a.1/§3a.2 exactly | §6 with the nominal type checked by the nucleus itself — the strongest fidelity available | **does not implement §6**: a holder of the publication endpoint could register any name. An explicit Stage 4 narrowing of an accepted contract | §6 unmet, explicitly and on the record |
| **nucleus / trusted-base impact** | **none** | **an object kind and an interface-name namespace in ring 0** — P2's cost arriving inside P3, against `docs/38` Tier 0 and `AGENTS.md` §8 | none | none |
| **new Tier-2 surface** | one document, one operation, one path (§15) | one document **plus** the object kind and probably an operation | none — the payload already carries a string | none |
| **textual-registry complexity** | **zero additional** (§14): the registry's canonical text is unchanged | small: it would resolve a kind-checked handle | grows: it keys entries by a name it must parse and compare, and a row carrying both a capability and a string is needed | unchanged |
| **negative evidence** | available today, in the existing `capability-denied` line, as a separate boot (§13) | available, and additionally checkable at use time | **not available in ADR-0093 §10.1's form**: there is no per-interface authority to withhold | not available |
| **endpoint / endowment budget** | fits exactly at N = 1 and **not** at N = 2 (§13c); the block service's endowment needs the inversion of §14 | one publication object, no per-interface endpoint — scales freely | one publication endpoint for every interface — **scales freely** | unchanged |
| **Stage 5 consequences** | the second published interface forces a bound rise or a reshape; nothing to reconcile with `/system` or `/dev` | a ring-0 interface namespace that Stage 5 must reconcile with `/system` as a commit tree and `/dev` as a capability namespace (`docs/09`), and Stage 6 must keep coherent across activation | nothing structural; the naming rules stay text | ADR-0051's evidence requirement stays unsatisfiable, as it has been since 2026-08 |
| **reversibility** | high: one document and one import. Removing it leaves the tree where it is today | **low**: an object kind in the trusted base is the hardest thing in this project to withdraw | high, but the narrowing it records would have to be un-recorded | total, and it is the current state |

**Is any option internally inconsistent?** One is, and it should be said plainly.

- **Q1-C cannot be described as implementing `CAPABILITY_V1` §6.** §6 makes the
  right to publish a capability whose nominal type *is* the interface. Q1-C's
  authority is "a `call` name for the one publication endpoint", which is
  interface-independent by construction: the same holder can register any name it
  writes into the payload. It is a coherent design and it may well be the right
  Stage 4 answer, but adopting it is **amending the reach of an accepted Tier 2
  contract**, not implementing it, and its decision text must say so.
- **Q1-A, Q1-B and Q1-D are each internally consistent.** Q1-A only after §14's
  inversion; the §5 sketch of it was not implementable.
- **Q1-B is consistent and is the option ADR-0093 already weighed and declined**
  as P2's cost. Adopting it now would be reopening that weighing, which is a
  Level 3 decision, not a Level 2 one.

## 17. Recommendation, offered as a recommendation

**Withdrawn by §26.** It rested on §11's unproven sentence and on §15's self-contradictory schema, and it did not follow §12's own finding through to the negative gate. Kept as the record of what was recommended and why it was wrong.

**Q1-A**, in the shape §10, §14 and §15 give it — and this is a technical
recommendation, not a decision, and not a claim that the Project Architect should
agree.

The reasons, in order of weight:

1. **It is the only option that implements an accepted contract rather than
   changing one or leaving it unmet.** Q1-C narrows `CAPABILITY_V1` §6, Q1-D
   leaves it unmet, and Q1-B reopens a weighing ADR-0093 finished.
2. **It costs the trusted base nothing**, which is the property the whole
   publication question has been circling since ADR-0051. The nucleus, the ABI
   and the object-kind set are untouched, and `OBJECT_INTERFACE` stays reserved.
3. **It makes ADR-0093 §10.1 provable with machinery that already exists** — the
   `capability-denied` refusal line, asserted by four gates today.
4. **It is the most reversible of the three options that change anything.**

Against it, and the Architect should weigh these rather than take my ordering:

- **It does not scale past one published interface** without a nucleus bound
  rising or the shape changing (§13c). If a second interface is expected soon,
  that argues for looking at Q1-C's shape first and recording the §6 narrowing
  honestly.
- **The nominal type is not checked by the nucleus** (§12). A reader could
  mistake Q1-A for stronger enforcement than it is, and the accepting decision
  should state the two-layer conjunction so that nobody has to rediscover it.
- **It is the first accepted interface path outside `system.*` and
  `platform.*`**, and therefore the precedent for every later driver interface.

## 18. Decision text, if the Project Architect agrees

**Not to be adopted.** It states that the nucleus, the ABI and the object-kind set are unchanged and that the negative evidence discharges ADR-0093 §10.1; §21 and §22 show the second of those is false under the schema it describes. Kept as the record of what was offered.

**Offered for adoption or rejection, and adopted by nobody yet.** If it is
adopted it belongs in a new ADR — ADR-0095 on the present numbering — with
ADR-0093 §11's open question then answered and ADR-0093-Q1 removed from the
journal's `open-questions` fence by the commit that closes it.

> **Q1-A is accepted.** The right to publish `block.device.v1` is a capability
> whose nominal type is `block.device.V1Publisher`, an accepted Tier 2 interface
> declared by a new schema document instantiating `SYSTEM_INTERFACE_V1` §2 and
> ADR-0060. Its object kind is `endpoint`, its representation is `AsInterface`,
> it is startup-importable, and it declares exactly one operation —
> `publish(cap: block.device.V1Publisher, endpoint: system.ipc.Endpoint) -> i64`
> over `SYSTEM_ABI_V1` operation 3.
>
> **The authority is presented by being called through, and the published
> endpoint travels as the delegated capability.** ADR-0093 §3a.1 forbids the
> registry holding anyone's right to publish, so a publication capability may
> never be sent in a message.
>
> **`OBJECT_INTERFACE = 4` stays reserved and empty**, and the nucleus, the
> system ABI, the object-kind set and the IPC mechanism are unchanged. No
> language minor is taken.
>
> **The per-interface property is enforced by two layers and by neither alone:**
> object identity at the nucleus, which is a distinct endpoint object per
> published interface, and nominal type in the verified IR and the launcher's
> mediation. The nucleus does not distinguish a publication capability from any
> other endpoint capability, and no document may claim it does.
>
> **The registry's `receive` is held as a `system.ipc.Endpoint` capability over
> the same object.** Two accepted interface paths over one object are legal under
> `SYSTEM_INTERFACE_V1` §4's object-kind-as-check rule; this is not a conversion
> and no operation converts between them.
>
> **Conformance evidence, both required before this is called implemented:** a
> positive boot in which the publisher holds the authority and registers; and a
> **separate** boot in which a publisher declaring the import and not endowed it
> is refused before its first instruction, asserted as
> `TOS.RUN.REFUSED stage=execute reason=capability-denied binding=… interface=block.device.V1Publisher`,
> with the registry reporting zero registrations. This discharges ADR-0093 §10.1
> and ADR-0051's evidence requirement.
>
> **`MAX_ENDPOINTS` and `MAX_PROCESSES` are not raised.** The positive boot's
> counts are unchanged at four and four; the negative boot needs three processes
> and one endpoint. The block service's endowment is full at `MAX_ENDOWMENT = 4`,
> so the authority is endowed and the service's own `call` name arrives in a
> message — forced by ADR-0061, not chosen.
>
> **Scope, and what is not decided.** One interface. No enumeration, no metadata,
> no versioning negotiation, no query language. The wire shape of `read`, `write`
> and `capacity` remains interface design inside `IPC_V1` (ADR-0093 §9). Region
> transfer from canonical text is untouched (ADR-0094 §10). Stage 4C, Stage 4D
> and Stage 4 do not close. **A second published interface is not authorised by
> this decision**: at N = 2 the registry's endowment and the endpoint table no
> longer fit, and whoever needs it returns with that as its own decision.

## 19. Second review, 2026-09-23: Q1-A as §15 wrote it is not implementable

**§9–§18 are superseded where this section contradicts them, and §17's
recommendation is withdrawn.** A review of `e2ad80a` raised two connected
questions; both are confirmed against the tree, and together they are decisive.
§20 is the schema contradiction, §21 the retyping, §22 the normative reading,
§23 what §11 got wrong, §24 the recomputed options including a new Q1-A2, §25 the
adversarial evidence and what it would find today.

**The short form.** §15 proposed an interface whose capability must arrive by
endowment and which declares no operation that can endow it — self-contradictory.
And with that operation added, the nominal type it depends on is **not preserved
through the launch plan**: both retyping directions succeed, so "authority for
`block.device.v1` and nothing else" collapses to "a `call` name on one endpoint",
which is what the tree already has and what ADR-0093-Q1 was raised about. Q1-A
would add a Tier 2 contract and change nothing enforceable.

## 20. Confirmed: no `endow_for_launch`, and therefore no endowment

`SYSTEM_INTERFACE_V1` §4, on the operation:

> "**`endow_for_launch` is one operation declared by several interfaces** … It is
> declared on **every interface whose capabilities may be a startup endowment,
> and on no others.**"

The six that declare it, read out of `ACCEPTED`: `system.ipc.Endpoint`,
`system.memory.Authority`, `system.process.Control`, `platform.pci.Bus`,
`platform.pci.FunctionConfig`, `platform.irq.Source`. `system.ipc.Reply` does not,
and the document says why: "declaring an operation the ABI always refuses would be
advertising something that does not work."

So the implication runs both ways, and §15 took only one direction:

1. §14 established that a nominal type can reach a module **only** through the
   launch endowment, because `import capability` is answered from the launch
   record (ADR-0061) and a message-delivered capability is typed by the producing
   operation's result.
2. Therefore `block.device.V1Publisher`'s capabilities **may be a startup
   endowment** — they must be.
3. Therefore, by §4's rule, the interface **must declare `endow_for_launch`**.
4. §15 declared exactly one operation, `publish`. So a canonical textual launcher
   holding such a capability has no operation that places it in a sealed plan, and
   ADR-0093 §3a.1's "granted by its launcher from a sealed launch plan" is
   unreachable.

**A nucleus-side initial endowment is not an answer**, and the review is right to
exclude it. `ADR-0093` §3a.1 names a launcher and a sealed launch plan;
`STAGE3_LAUNCH_PLANS.md` and ADR-0077 §3–§5 make operation 22 the only way an
entry enters one; and the boot process's own endowment is "the launcher's own
stated constant until `/system/policy/` exists" (`CAPABILITY_V1` §2) — a constant
that hands authority to the *first* process, not a way for that process to hand it
on. A hard-coded initial grant would put the publication authority in the nucleus's
constant and leave the sealed-plan path still unimplemented.

**So §15's table is wrong on one row and the correction is not a repair.** Adding
`endow_for_launch` makes the interface two operations, not one. That is
mechanical. What it does not fix is §21.

## 21. Confirmed: the nominal interface is not preserved, and both directions pass

Read end to end, not from the descriptions.

### 21a. Operation 22 never sees an interface

`nucleus/src/syscall.rs::launch_plan_endow`:

```rust
let object = match capability::resolve(caller, frame.rdi, 0) { … };
…
let entry = crate::plan::Entry {
    binding,
    object,
    rights: frame.r10 as u32 & capability::rights_of(caller, frame.rdi),
    scope: 0,
};
```

The capability is resolved **at right 0** — `SYSTEM_ABI_V1` row 22's "at no
particular right" — and what is recorded is object, intersected rights and the
binding. There is no interface argument in row 22 and no interface field in the
entry.

### 21b. The plan entry has four fields and none is an interface

`nucleus/src/plan.rs`:

```rust
pub struct Entry {
    pub binding: Binding,
    pub object: Object,
    pub rights: u32,
    pub scope: u64,
}
```

### 21c. Nor does the launch record

`tos-launch::LaunchCapability`: `handle`, `object`, `rights`, `scope`, `binding`.
Its own comment says why the binding is the key — "two imports of one interface
are legal, so an interface path cannot tell them apart" — which is a statement
about two imports of **one** interface and says nothing about two interfaces.

### 21d. And `granted()` makes exactly one comparison

`runtime-image/src/main.rs`: find the endowment whose binding equals
`request.binding`; compute `wanted` from `interfaces::interface(request.interface)`'s
`ObjectKind`; then

```rust
(capability.object == wanted).then(|| Handle::new(capability.handle))
```

One comparison, on the object kind.

### 21e. Both counterexamples therefore pass

| | parent holds | endowed under binding | child declares | `granted()` |
|---|---|---|---|---|
| **forward** | `block.device.V1Publisher`, `call`, over endpoint P | `generic` | `import capability system.ipc.Endpoint as generic` | `wanted = OBJECT_ENDPOINT`, entry's object is `OBJECT_ENDPOINT` → **granted** |
| **reverse** | `system.ipc.Endpoint`, `call`, over an ordinary endpoint | `publication` | `import capability block.device.V1Publisher as publication` | `wanted = OBJECT_ENDPOINT` → **granted** |

The forward direction is the one that matters: the child receives the publication
endpoint as an ordinary endpoint and may reach `endpoint_call_carrying` on it,
having never been given `block.device.V1Publisher`. **It can publish.**

### 21f. Why nobody noticed: the map is injective today

Ten accepted interfaces, ten distinct object kinds:

| kind | interface |
|---|---|
| `Endpoint` | `system.ipc.Endpoint` |
| `Reply` | `system.ipc.Reply` |
| `MemoryAuthority` | `system.memory.Authority` |
| `LaunchPlanBuilder` | `system.process.LaunchPlanBuilder` |
| `LaunchPlan` | `system.process.LaunchPlan` |
| `Process` | `system.process.Control` |
| `PciBus` | `platform.pci.Bus` |
| `PciFunction` | `platform.pci.FunctionConfig` |
| `IrqSource` | `platform.irq.Source` |
| `DmaRegion` | `platform.dma.Region` |

interface → object kind is **injective**, so "a grant of the matching kind"
determines the interface uniquely and retyping is unreachable. Q1-A would be the
first pair to break that, and simultaneously the first design to rely on the
nominal type to express authority. Those two together are the defect.

### 21g. And a second, independent ceiling

Even with the type preserved, `block.device.V1Publisher::publish` and
`system.ipc.Endpoint::endpoint_call_carrying` are both `SYSTEM_ABI_V1`
operation **3**, with the same register and transfer-slot layout. They are one
instruction. So a nominal type can gate **which schema rows a module may name** —
which the frontend and the verifier do enforce, on the artifact — and it can never
be a check the nucleus performs on a handle. Any decision adopting a nominal
publication type should say that, so that nobody later reads the type as a
runtime boundary.

## 22. The normative question, and it has not been decided

The review asks whether the retyping is **A**, a forbidden retyping the model
overlooked, or **B**, permitted launcher policy. The corpus supports each of two
narrower statements, and it does not adjudicate between the two readings.

### 22a. What says B — the mechanism is licensed

- **`docs/42` §2**: an import "declares that the module may receive one opaque
  value … whose nominal capability type is `system.time.Clock`. **It is a request,
  not a grant. The process launcher/supervisor, not source text, maps the request
  to a concrete grant after policy/trust evaluation.**"
- **`CAPABILITY_V1` §3**: `capability = object + rights + scope + lifetime +
  generation`. **The interface is not one of the five.** Nothing in §1–§4 attaches
  an interface to a handle.
- **`SYSTEM_INTERFACE_V1` §4**: "**The kind is a check, not the mechanism that
  chooses a grant.** Which grant answers which request is decided by the binding
  the module declared."
- **ADR-0061** is the most explicit of all: "No accepted document joins the three,
  so even 'match a request to a grant of the matching kind' is not derivable; **it
  is a decision**" — and of the check it adopted, "the kind check is necessary
  under **every** option and **is not a matching rule**."

### 22b. What says A — the intent was that a mismatch be refused

- **The precedent is exactly on point.** `system.process.LaunchPlanBuilder` and
  `system.process.LaunchPlan` are one object in two states, and the model gave
  them **two object kinds** rather than one kind plus a flag. `interfaces.rs`
  records the reason: "a builder and a sealed plan declare *different*
  operations, and **a launcher answering `import capability
  system.process.LaunchPlan` with a builder would be answering a request for
  something that has been decided with something that has not.**" Faced with two
  interfaces that could have shared an object, the model added a kind so that the
  launcher's check would refuse the mismatch — it did not trust the launcher.
- **`SYSTEM_INTERFACE_V1` §4 states the check's purpose**: "so that a launcher
  answering a module's request can **refuse a grant of the wrong kind** at startup
  instead of letting the module discover it at its first call." For a shared kind
  that purpose is vacuous.
- **The prohibitions the review cites are real but do not reach this.** "No
  **source** operation can widen a right, recreate a consumed linear capability,
  or transfer a handle by encoding its bits" (`docs/42` §2); attenuation is
  downward only; no accepted operation converts between capability interfaces. A
  launcher answering a request is not a source operation on a capability, so none
  of these forbids it — which is why the retyping is **unguarded** rather than
  permitted by anyone's decision.

### 22c. The determination

**The contracts permit the mechanism and no decision ever authorised the power.**
Those are different statements and the review is right to separate them:

1. *the launcher decides who gets authority* — accepted, repeatedly and
   explicitly;
2. *the launcher decides which accepted operation set a child may exercise over a
   given object at given rights* — **never decided, because until Q1-A it was
   unreachable**, and the one time the model came near it (22b's precedent) it
   removed the possibility rather than granting the power.

Claim 2 does not follow from claim 1, and I am not going to resolve it: it is a
Level 2-or-3 question about the capability model and it belongs to the Project
Architect. **What does not need resolving is Q1-A's fate**, because it fails under
either reading:

- under **B**, the nominal type carries no authority, so ADR-0093 §10.1's negative
  ("a service that was not granted the publication capability cannot publish") is
  **unprovable**: the forward counterexample publishes. Q1-A would buy a Tier 2
  contract, a verified-IR declaration of intent and a nicer denial message, and
  would not establish the thing it exists to establish;
- under **A**, the model must be changed to preserve the type, which is Q1-A2.

## 23. What §11 got wrong

§11 ended: "A launcher *could* endow the publication endpoint to a third module
under a `system.ipc.Endpoint` binding … It is a trust boundary, not a hole."

**That conflated the two claims of §22c and asserted the second as settled.** The
sentence is withdrawn. What is true:

- the launcher deciding *who* holds the authority is a trust boundary, accepted;
- the launcher deciding *what type* the authority has, and thereby which accepted
  operations are reachable over one object at one right, is an undecided power
  that the accepted corpus neither grants nor forbids;
- and the specific case §11 described is not merely a boundary: it is the
  counterexample that makes ADR-0093 §10.1 unprovable under Q1-A, which §11 should
  have noticed and did not.

§12's layer table stands as written, including its conclusion that the nucleus
cannot distinguish a publication capability from any other endpoint capability.
What §12 did not do is follow that through to the negative gate, which is §22c.

## 24. Q1-A2, and the options recomputed

### 24a. Q1-A2 — Q1-A plus a type-preserving endowment

**Only the minimum, and it is not designed further than the comparison needs.**

| question | answer |
|---|---|
| must a plan entry hold an interface identity | yes — §21b is where the chain breaks first, and §21c is the second break. Both the plan entry and the launch record need it |
| where does the identity come from | **from the verified operation row, never from a string the module supplies.** `endow_for_launch` is declared per interface (§20), so the bridge performing it knows which interface it is performing over; the verifier already proved the call site's exact nominal type (`V2013_CAPABILITY`). A module-supplied name would be `docs/37`'s "textual manifest grants itself authority" |
| where is it compared | in `granted()`, beside the object-kind check: the stored identity must equal `request.interface` |
| does it touch `SYSTEM_ABI_V1` operation 22 | **yes.** Row 22 has no channel for it. Either a second bounded string in the argument region at a new fixed offset with its length in `r8`, or the existing `LAUNCH_ENDOW_BINDING` becomes a two-field record. Either is a change to an accepted ABI row — not a new operation, which is cheaper than Q1-B's likely cost, but not "the ABI is unchanged" |
| launch-plan / record format | `plan::Entry` and `LaunchCapability` each gain one bounded field. At `MAX_ENDOWMENT = 4` × `MAX_PLANS = 4` × `MAX_BINDING = 64` that is 1 KiB of additional static nucleus storage, bounded like everything else |
| nucleus trusted base | grows by **an opaque field the nucleus never interprets**, exactly as it already carries a binding string: it never resolves an interface, never compares one to a list, and holds no namespace. Both ends of the comparison are in the runtime image, which is already in the trusted base and is already the party that performs the object-kind check |
| verifier / runtime | no new verifier obligation: V2013 and V2033 already prove the call site's interface. The runtime image gains the write (parent side) and the comparison (child side) |
| decision level | **argued both ways and not settled here.** Level 2 if extending row 22 is read as a contract extension preserving invariants; Level 3 if touching the ABI's closed table is read as moving a trust boundary. The accepting ADR must make that argument rather than assume it |
| compatibility | every existing entry has no identity, so absence must mean something. **It must be refused, not treated as a wildcard**: a wildcard is the retyping with extra steps, since a parent could simply omit the field. So the bridge always writes it and an entry without one fails the child's startup — which is compatible only because the bridge is the sole path to operation 22 from canonical text |

**Why a module cannot forge the identity.** A TOS Core module has no inline
assembly, no raw system call and no pointer; the typed bridge is its only route to
operation 22, and the bridge derives the identity from the `PERFORMED` row the
verified instruction named. The nucleus is trusting the runtime image, which it
already trusts for the object-kind check.

**What Q1-A2 buys beyond Q1-A.** It closes §21's chain, so the forward
counterexample is refused and ADR-0093 §10.1's negative becomes provable. It does
**not** touch §21g: `publish` and `endpoint_call_carrying` remain one instruction,
so the type is still an artifact-level gate and never a nucleus check on a handle.
And it is **generic** — it fixes the degree of freedom for every future interface
pair, not only for publication.

### 24b. Q1-B, re-read — and it is better than §16 said

§16 repeated ADR-0093 §4's objection that Q1-B puts "an interface-name namespace
in ring 0". **On re-reading, filling `OBJECT_INTERFACE` does not require a
namespace.** An `InterfacePublication` object can be an *unnamed* object of a new
kind, minted at the boot/platform boundary under `CAPABILITY_V1` §2's third origin
class, one per published interface, with which interface it *means* recorded in
the launcher's plan and in the audit record — exactly where the meaning of every
other endowment lives. The nucleus would then hold a kind and no names.

If that is right, Q1-B's cost is one object kind — whose number is **already
allocated and reserved** for this (`OBJECT_INTERFACE = 4`, ADR-0093 §2) — plus
whatever mints one, and:

- **it restores injectivity**, so the retyping of §21e is refused by the object-
  kind check that already exists. No plan field, no launch-record field, no change
  to operation 22;
- the nucleus distinguishes a publication capability from an endpoint capability,
  so §21g's ceiling disappears too: `publish` would be an operation on a different
  object kind, and a `call` on an endpoint could not reach it.

**This materially changes the weighing and it is not a recommendation.** ADR-0093
declined **P2**, which was a *registry in the nucleus*; filling `OBJECT_INTERFACE`
without a registry is not P2, and §4's stated objection may not apply to it. That
is the Project Architect's reading to make, not mine — and it is the single most
important thing this section has to say.

### 24c. The recomputed comparison

| | Q1-A | **Q1-A2** | **Q1-B** (unnamed kind) | Q1-C | Q1-D |
|---|---|---|---|---|---|
| **implementable as written** | **no** (§20, §21) | yes | yes | yes | yes, it is the status quo |
| **ADR-0093 §10.1 provable** | **no** — the forward counterexample publishes | yes | yes | no — no per-interface authority exists to withhold | no |
| **`CAPABILITY_V1` §6** | claimed, not achieved | achieved at the artifact layer | achieved at the nucleus layer | **narrowed**, explicitly | unmet, explicitly |
| **nucleus / trusted base** | none | one opaque uninterpreted field, 1 KiB static | one object kind, number already reserved; **no namespace** if §24b is right | none | none |
| **`SYSTEM_ABI_V1`** | unchanged | **operation 22 extended** | likely one operation added to mint the object | unchanged | unchanged |
| **new Tier 2 surface** | one document, two operations | same | same | none | none |
| **generality** | — | **generic**: fixes every future interface pair | specific to publication | — | — |
| **§21g ceiling (two rows, one instruction)** | remains | remains | **removed** | n/a | n/a |
| **textual registry** | unchanged | unchanged | small change: it holds a publication object rather than an endpoint | grows: keys entries by a parsed name | unchanged |
| **endpoint / endowment budget** | fits at N = 1 only | same | **better**: no per-interface endpoint is needed | best: one endpoint for all interfaces | unchanged |
| **Stage 5** | nothing to reconcile | nothing to reconcile | an object kind to carry forward; **no** `/system`–`/dev` namespace question if unnamed | nothing structural | ADR-0051's evidence stays unsatisfiable |
| **reversibility** | high | medium: an ABI row and two formats | low: an object kind is the hardest thing here to withdraw | high | total |

**Internally inconsistent options, restated.** Q1-A is now one of them: it cannot
deliver the conformance item it exists to deliver. Q1-C remains unable to be
described as implementing `CAPABILITY_V1` §6. Q1-A2, Q1-B and Q1-D are each
internally consistent.

## 25. Adversarial evidence, designed and not implemented

Two negatives, and **both would be accepted by the architecture today** — which
is the research result, not a test to be adjusted until Q1-A passes.

### 25a. A publication capability cannot be endowed as an ordinary endpoint

- **Topology**: a launcher holding `block.device.V1Publisher` with `call` over
  endpoint P; a child declaring `import capability system.ipc.Endpoint as generic`;
  the launcher endows P under binding `generic`.
- **Required outcome**: the child is refused before its first instruction, and the
  audit record names both interfaces — the one endowed and the one requested.
- **Today**: `granted()` compares only the object kind, both sides are
  `OBJECT_ENDPOINT`, and the child **starts and holds the publication endpoint as
  an ordinary endpoint**. The test fails, and it fails at the architecture rather
  than at an implementation.
- **Under Q1-A2**: refused, with the stored identity and the requested interface
  both nameable in the refusal.
- **Under Q1-B**: refused by the existing kind check, because the kinds differ.

### 25b. An ordinary endpoint cannot be endowed as `block.device.V1Publisher`

- **Topology**: the mirror — an ordinary endpoint endowed under a binding the
  child declared as the publication interface.
- **Required outcome**: refused at startup.
- **Today**: granted, for the same reason.
- **Why it matters even though its direct effect is harmless** — the child would
  hold "publication authority" over an endpoint nobody publishes on, and its calls
  would reach whatever receives there. It matters because it is the same defect
  seen from the other side, and because a model that admits it cannot claim the
  nominal type means anything about what a handle names.

### 25c. A third negative any type-preserving option needs

An entry carrying **no** interface identity must be refused rather than treated as
matching, or the retyping returns by omission (§24a, compatibility row). Worth
stating now because it is the kind of case a later implementer reads as a
compatibility convenience.

### 25d. The positive that must accompany them

A child's `import capability X` is answered by an entry endowed **through X**, and
the boot log shows the identity that travelled. Without it the three negatives
could all be satisfied by a check that refuses everything.

## 26. Where this leaves the decision

**§17's recommendation of Q1-A is withdrawn.** It rested on §11's unproven
sentence and on a schema §20 shows to be self-contradictory, and it did not follow
§12's own finding through to the negative gate.

**No replacement recommendation is offered, and that is deliberate**, because the
choice now turns on a question of the capability model that is not mine to answer:
§22c's claim 2 — whether a launcher may decide which accepted operation set a
child exercises over one object at one right. That question is prior to Q1-A2
versus Q1-B:

- if claim 2 is **rejected**, the model owes type preservation regardless of
  publication, and Q1-A2 is the generic fix — with ADR-0093-Q1 becoming a
  consumer of it rather than the reason for it;
- if claim 2 is **accepted**, no amount of nominal typing can carry this authority
  and the honest options are Q1-B (put it in the kind, where the nucleus can see
  it) or Q1-C (stop pretending the type carries it, and record the narrowing).

**What is now established, and should not be re-derived.** Q1-A as written is not
implementable (§20) and would not prove what it exists to prove (§21, §22c). The
retyping passes in both directions today (§21e), unreachably so until now (§21f).
`OBJECT_INTERFACE` can probably be filled without a ring-0 namespace (§24b),
which reopens Q1-B on better terms than §16 gave it. ADR-0093-Q1 stays open, and
the question above may deserve an open question of its own — which is also the
Project Architect's call, since raising one is a claim about the model.
