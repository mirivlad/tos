<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0095: A publication authority is a dedicated channel, not a nominal type

- Status: **Accepted** (Project Architect-directed, 2026-09-23)
- Date: 2026-09-23
- Decision level: **2** under `docs/21`. It amends an accepted Tier 2 contract —
  `CAPABILITY_V1` §6 — by **narrowing** it. Nothing enters the trusted base, no
  ABI operation is added or changed, no object kind is filled, and no nucleus,
  verifier or launch-plan semantics change
- Amends: **`CAPABILITY_V1` §6**, and ADR-0093 §1, §3a.1, §3a.2, §10.1
- Related: **ADR-0051** §2 (publishing is a requested authority, never a
  self-declared `provides`); **ADR-0093** (P3: the registry is an ordinary
  textual service); **ADR-0061** (a binding answers an import);
  **ADR-0077** §2–§5 (launch plans and their four-capability bound);
  `IPC_V1` §2 (one receive-rights holder per endpoint), §4, §6;
  `SYSTEM_INTERFACE_V1` §4; **ADR-0096** (the generic question this decision
  deliberately does not raise);
  `docs/research/PUBLICATION_AUTHORITY_CONFLICT.md`, which is research and is
  authority for nothing

## 1. What this decision is for

`CAPABILITY_V1` §6 as written requires a publication authority **whose nominal
type is the interface being published**. Stage 4 attempted to implement that and
could not, and the reason is not a missing feature: the requirement asks the
nominal type to carry authority in a layer that does not carry it.

The research is in `docs/research/PUBLICATION_AUTHORITY_CONFLICT.md` and is not
repeated here. Two facts from it are load-bearing:

- **a nominal type does not survive an endowment.** `launch_plan_endow`
  (operation 22) resolves the delegated capability at no particular right and
  records object, intersected rights and binding; a launch-plan entry and a
  launch record each carry those and no interface; and the launcher's startup
  check compares the endowed object's **kind** against the kind the requested
  interface declares, and nothing else. So two interfaces sharing one object kind
  are interchangeable at that check;
- **and it could not be otherwise cheaply.** Making it otherwise means either
  interface identity in the generic launch machinery, or a new object kind in the
  nucleus. Both are decisions about Stage 3's capability model, reached from a
  Stage 4 storage slice.

**This decision narrows §6 to what the architecture actually provides**, so that
the contract states a property the system has rather than one it does not. It is
a deliberate simplification and not a hidden implementation exception.

## 2. The decision

**A publication authority is a capability naming a dedicated publication
object.** The object's identity fixes what may be published through it. Holding
a capability that names it permits publication through that channel; not holding
one does not.

`CAPABILITY_V1` §6 is amended to read as §3 below. In summary:

1. **the right to publish is itself a capability** — unchanged, and the part of
   §6 that was never in question;
2. **a publication authority names a dedicated publication object or channel
   whose identity fixes what may be published through it.** The authority is
   possession of a capability naming that object;
3. **no source declaration and no payload string grants publication authority.**
   ADR-0051 §2 is unchanged: a service does not declare `provides`, and a name a
   module writes into a message is data, not authority;
4. **for the Stage 4 P3 textual registry the dedicated object is an endpoint.**
   The registry holds `receive` on it; an authorised publisher holds `call`. One
   receive-rights holder is `IPC_V1` §2, unchanged.

**What is explicitly not claimed.** The nominal source interface of a capability
is **not** preserved by the nucleus and **not** preserved by a launch plan. No
document may say it is. The enforcement is object identity and possession, at the
nucleus, where `CAPABILITY_V1` §2 and §3 already put it.

**What is explicitly not generalised.** This decision covers one publication
class. It does not define how a second published interface would work, and §6 as
amended does not imply that a second one is available — see §6 below.

## 3. The amended text of `CAPABILITY_V1` §6

As it now stands in the contract, reproduced here so that this decision carries
what it changed:

> The right to publish an interface is itself a capability (ADR-0051 §2). It
> names a **dedicated publication object** whose identity fixes what may be
> published through it: a process holding a capability that names it may
> register, one that does not, cannot. There is no self-declared `provides`, no
> name a module writes into a message is authority, and the registry never holds
> an entry no one granted.
>
> **The authority is object identity and possession, not a nominal type.** A
> capability is object, rights, scope, lifetime and generation (§3); an interface
> path is a fact about source and about the artifact, and it is neither carried
> by a handle nor preserved by a launch plan. A contract that required the
> publishing interface to *be* the capability's nominal type would be requiring
> an enforcement this model does not provide — ADR-0095 §1.
>
> **For the Stage 4 interface registry the dedicated object is an endpoint**
> (ADR-0093 P3). The registry holds `receive` on it and an authorised publisher
> holds `call`; `IPC_V1` §2's one-receiver rule is what makes those disjoint. A
> process that can name no such endpoint cannot publish through it, and that is
> the whole of the mechanism.
>
> **One publication class per object.** An object whose identity fixes two
> publishable interfaces would fix neither, so a second published interface needs
> a second object and a decision about how a client asks for one (ADR-0095 §6).

## 4. Why this is capability-safe

Stated as the argument rather than as an assurance, because the previous
formulation read well and was not implementable.

- **The authority is unforgeable.** A capability is a process-local index into a
  nucleus-owned table the process cannot address (`CAPABILITY_V1` §2). A process
  cannot construct a handle for an endpoint it was not given, and TOS Core has no
  way to name one: a capability value may not be a constant, an integer, a
  deserialized value or a cast (`docs/42` §2, `E1502_FORGED_CAPABILITY`).
- **It cannot be widened.** Rights travel intersected with the grantor's
  (`launch_plan_endow`), attenuation is downward only, and no operation creates
  authority over a pre-existing object out of nothing (`CAPABILITY_V1` §2).
- **It is attributable.** The endowment that granted it is named in the launch
  record by binding, and a request that policy declined is
  `TOS.RUN.REFUSED stage=execute reason=capability-denied binding=… interface=…`
  before the module's first instruction. That is ADR-0051's "says so in the audit
  record", and it is the record §5 asserts.
- **It does not depend on the registry's good behaviour for the negative.** A
  process with no capability naming the publication endpoint cannot reach the
  registry's publication channel at all; the registry is not consulted and has no
  branch to get wrong. The registry's own rule — that a call arriving there is a
  request to publish `block.device.v1` — decides what a *permitted* publication
  means, not who is permitted.
- **And the enforcement is where the model already puts every other
  enforcement.** `capability::resolve` checks object kind and rights at every
  use. Nothing new is trusted.

**The one thing it does not give**, stated so nobody looks for it: two processes
each holding `call` on the publication endpoint are indistinguishable to the
nucleus. Who may hold one is the launcher's decision, expressed in its plan and
in the audit record. That is ADR-0051 §2's model and it is unchanged.

## 5. Conformance evidence this decision requires

Replacing ADR-0093 §10.1, which asked for a property the nominal model could not
deliver.

1. **Positive.** The registry holds `receive` on the dedicated publication
   endpoint; the authorised block service obtains `call` for it; it publishes its
   service endpoint through it; a lookup returns that endpoint; and a client that
   held no name for the service reaches it.
2. **Negative, in a separate boot.** A process that is **not** given a capability
   naming the publication endpoint cannot publish. It must fail for that reason
   and for no other: not a string comparison, not a source convention, not a
   branch in the registry. The refusal is attributable in the audit record, and
   the registry must show that nothing was registered.
3. **Mutation.** Granting that same process `call` on the publication endpoint —
   changing nothing else, not the module, not its source, not its binding name,
   not the registry — must make the negative stop proving denial. A negative that
   survives the mutation was testing something else.

Item 3 is the part that makes items 1 and 2 mean what they say, and it is
required rather than advisory.

## 6. What this does not decide

- **A second published interface.** One object fixes one publication class, so a
  second interface needs a second object and a way for a client to ask which one
  it wants. At Stage 4's bounds — `MAX_ENDOWMENT = 4`, `MAX_ENDPOINTS = 4` — that
  does not fit, and it is not needed: ADR-0093's scope is one interface.
  **See §8: one of those two numbers has since moved, and it changes nothing
  here.**
- **Whether a nominal interface should survive delegation or endowment in
  general.** That is a real question about Stage 3's capability model and it is
  **ADR-0096**, raised separately and deliberately not answered here. Stage 4
  creates no interface pair that exposes it.
- **`OBJECT_INTERFACE = 4`**, which stays reserved and empty, as ADR-0093 §2 and
  §4 leave it under P3.
- **Anything about the nucleus, the system ABI, operation 22, the launch-plan
  representation, the verifier or the registry protocol.** None changes.
- **The wire shape of `read`, `write` and `capacity`** (ADR-0093 §9), and region
  transfer from canonical text (ADR-0094 §10).
- **Stage 4 closure.** Stage 4C, Stage 4D and Stage 4 do not close here.

## 7. Consequences

- `CAPABILITY_V1` §6 states a weaker and true rule where it stated a stronger and
  unimplementable one. A reader can now check the contract against the tree.
- ADR-0093 §1, §3a.1, §3a.2 and §10.1 are amended to match, and ADR-0093's open
  question **ADR-0093-Q1 is closed by this decision**: publication no longer
  depends on a nominal publisher type, so the gap it named no longer exists. The
  research that produced it stays where it is, as research.
- ADR-0051 §2's evidence requirement — "a service whose publish capability is
  denied … does not appear in the interface registry, and says so in the audit
  record" — becomes testable for the first time, in the form §5.2 gives it.
- **No implementation change follows for publication.** The tree already builds
  exactly this topology; what was missing was the contract saying so and the
  negative evidence. That is the shape of a decision that narrows rather than
  extends.

## 8. Clarification, 2026-09-23: the endpoint bound named in §6 has moved

**§6 is left as it was written, and this says what changed.** When this decision
was accepted the implementation bound was `MAX_ENDPOINTS = 4`, and §6 cited it as
one reason a second published interface did not fit at Stage 4.

**The Stage 4 service-lifecycle evidence raised the static endpoint table to six**
(`block-lifecycle.sh`, `nucleus/src/ipc.rs`). The two additional objects exist for
reasons that are nothing to do with publishing a second interface:

- **one endpoint per service generation.** The predecessor and the successor must
  hold *different* endpoint objects, or the old capability would have been
  repaired into a name for the successor — which ADR-0093 §3a answers 5 and 7
  forbid, and which case C exists to distinguish;
- **the supervisor→registry withdrawal authority.** ADR-0093 §3a answer 4 requires
  the registry to learn of the death, and §4's P3 answer 4 admits learning it as a
  notification from whoever holds the supervisory relationship. That is a third
  authority on a third object rather than a second meaning loaded onto the
  publication or lookup channel.

**What did not change.** `IPC_V1` §3's message bounds — 256 inline bytes, four
capabilities, two regions — are untouched, and so is the queue depth: §7's rule is
that a queue is never grown to accept a message, and a larger statically reserved
table grows no queue. No semantics, no ABI operation and no capability kind
changed.

**And what this is not.** The additional capacity **does not introduce or imply a
second publication class.** §2 and §3 are unchanged: an object whose identity fixed
two publishable interfaces would fix neither, and this decision still defines
**exactly one** published interface at Stage 4 — `block.device.v1`. A second one
remains undecided, and the spare endpoints are not an argument for it: they are
held by two generations of one service and by a withdrawal channel.
