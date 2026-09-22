<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Publication authority: what ADR-0093 accepted and what was built

Status: **Research note. It accepts nothing, chooses nothing and is authority
for nothing.** It is the conflict report ADR-0093-Q1 names, written so that the
Project Architect has the whole gap, the options and their consequences in one
place. `docs/38` assigns authority; this note has none.

Date: 2026-09-23. Raised by an external audit of commits `6c17b10`, `b276fde`
and `89f7599`, whose finding is confirmed here rather than argued with.

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
