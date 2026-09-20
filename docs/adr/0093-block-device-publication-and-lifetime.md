<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0093: Publication and lifetime of `block.device.v1`

- Status: **Proposed**
- Date: 2026-09-21
- Decision level: **2** under `docs/21`. P1 adds nothing; P2 adds an object
  kind, two operations to the closed `SYSTEM_ABI_V1` §5 table and an
  interface-name namespace to the trusted base; P3 and P4 add no ABI surface
  but fix where a system-wide naming authority lives, which every later stage
  inherits
- Related: **ADR-0051** §2 (publishing is a requested authority, never a
  self-declared `provides`) and its evidence list, which names an "interface
  registry" no accepted decision defines (§2); **ADR-0077** §3–§5 (a launch
  plan is how a creator endows a child), §8 (no restart policy in the nucleus);
  **ADR-0067** (a supervisor learns of an ending through operation 14);
  **ADR-0076** §3 (funded creation); `CAPABILITY_V1` §3, §4, §6; `IPC_V1` §2,
  §4, §5, §7; `SYSTEM_ABI_V1` §5 operations 1–4, 13, 14, 19, 20;
  `docs/11` §Driver-interfaces and §Crashes-and-restart; `docs/09`;
  `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §3, §8, §12, which is the note
  this decision was raised from and is not authority for anything

## 0. What this decision is for

A client must reach a running block service without knowing which process it
is, and what it holds must behave defensibly when that service dies and is
replaced. `CAPABILITY_V1` §6 fixes the **right to publish**. It fixes nothing
about a registry, about lookup, or about what an entry's lifetime is — and
`docs/11` §Crashes-and-restart step 4, "restore published interface endpoints",
is an obligation on whatever is chosen.

**Nothing is chosen here.** Four options with their consequences, no preference,
and P1…P4 in enumeration order rather than rank.

**Scope is fixed and narrow by direction of the Project Architect.** The
interface surface is `read`, `write`, `capacity` and nothing else. No
filesystem, no partitions, no cache, no object store, no enumeration framework,
no multi-device management. This ADR decides **lifetime and publication
semantics**; the wire shape of the three operations is interface design inside
`IPC_V1` and is not an ADR question unless it needs something `IPC_V1` does not
offer, which on present analysis it does not.

## 1. What is already decided, and must not be re-decided

- **Publishing is an authority the system grants** (ADR-0051 §2). A service does
  not declare `provides "block.device.v1"`; it *requests* the authority to
  publish that interface through the accepted capability-import form, where the
  nominal capability type **is** the interface. The launcher reads
  `capability_imports` from the verified IR and grants or denies under policy.
  `docs/37`'s Stage 3 failure condition "textual manifest grants itself
  authority" is why.
- **The right to publish is itself a capability** whose nominal type is the
  interface (`CAPABILITY_V1` §6). There is no self-declared `provides`, and the
  registry never holds an entry no one granted.
- **Endowment needs no new mechanism.** A sealed launch plan carries whatever
  the creator gives the child (ADR-0077 §3–§5, operations 19 and 20).
- **Endpoints, delegation, attenuation and revocation are decided**
  (`IPC_V1` §2, `CAPABILITY_V1` §4). So is what a call carries (`IPC_V1` §4,
  operation 3: at most three capabilities of its own, one place being spoken
  for by the answer) and how a server answers and re-waits in one crossing
  (operation 13).
- **A supervisor learns of an ending** through operation 14 with
  `RIGHT_WAIT_CHILD` (ADR-0067), and restart policy is canonical supervisor
  text, not nucleus policy (ADR-0077 §8).

## 2. What is not decided, and the gap that shows it

Nothing anywhere defines a registry: not what object it is, not who holds it,
not how a client asks it anything, not what happens to an entry when its
publisher dies.

**And an accepted ADR already depends on one.** ADR-0051's evidence list
requires that a service whose publish capability is denied "does not appear in
the **interface registry**". That sentence assumes an object that no accepted
decision has ever specified. It is not a contradiction — an absent thing cannot
contain a denied entry — but it is a requirement written against a mechanism
that does not exist, and this ADR is where that is either supplied or
consciously deferred.

**One number is already spent.** `OBJECT_INTERFACE = 4` is allocated in
`tos-launch`'s object-kind sequence and `InterfacePublication` exists in the
frontend's `ObjectKind`, while `nucleus/src/capability.rs` has no such kind.
The number is reserved and unimplemented. P2 would fill it; P1, P3 and P4 leave
it reserved and empty, which is a state worth naming rather than leaving to be
rediscovered.

## 3. The questions this decision must answer

Each option in §4 is judged against all eight.

1. **Who holds the publication capability?**
2. **How does a published interface come into existence?**
3. **How does a client obtain a capability naming it?**
4. **What happens to the publication when the service dies?**
5. **What happens to the capability a client already holds?**
6. **What does a client do after the service restarts?**
7. **Is re-lookup a mandatory recovery mechanism, or one of several?**
8. **Which stale-client cases are guaranteed distinguishable, and which are
   deliberately left ambiguous?**

Question 8 is the one this ADR exists for. The others can be answered by any
competent design; 8 is where a wrong answer is silently wrong, because a client
that cannot tell "not done" from "done, answer lost" will retry a **write**.

## 4. The options

### P1 — no registry at Stage 4; the launcher wires client to service

The launcher creates the endpoint, endows the service with `receive` and the
client with `call`, both through sealed launch plans. Publication is deferred
whole.

1. Nobody — no publication capability is granted or needed.
2. It does not. There is an endpoint, not a publication.
3. From its own launch plan, before its first instruction runs.
4. Nothing; there is no publication.
5. It names an endpoint whose receiver is gone. `IPC_V1` §2 and the liveness
   rule decide what a call on it does.
6. Whatever the supervisor arranges; the client has no way to ask for a new
   name.
7. There is no lookup, so recovery cannot be by re-lookup. It must be by the
   supervisor re-endowing, which means restarting the client, or by the
   endpoint outliving its receiver.
8. Narrowest set — see §5.

### P2 — a registry object in the nucleus

Publish and lookup become operations; `OBJECT_INTERFACE` is filled.

1. A service endowed with an `InterfacePublication` capability naming
   `block.device.v1`.
2. The service calls publish, presenting the publication capability and the
   endpoint it wants named.
3. A lookup operation, by interface name.
4. The nucleus observes the death and decides: entry removed, or entry retained
   and marked.
5. The nucleus decides: revoked by generation, or left naming a dead endpoint.
6. Re-lookup.
7. Available and can be made mandatory.
8. Widest set, at the cost below.

**The cost is the trusted base.** An interface-name namespace in ring 0 is a
naming authority in the binary trusted base, and `docs/38`'s Tier 0 invariants
and `AGENTS.md` §8's narrow-nucleus rule both bear on it. It also collides at
Stage 5 with `/system` being a commit tree and `/dev` being a capability
namespace (`docs/09`).

### P3 — a registry as an ordinary textual service

A name service holding publication entries, reached over IPC like anything
else.

1. Each publisher, granted by its launcher; the name service holds the registry
   itself.
2. The service calls the name service over IPC, presenting its publication
   capability.
3. A lookup call to the name service, for which the client needs an endpoint
   capability for **it** — which the launcher must supply, so one wiring step
   remains no matter what.
4. The name service decides, and it must learn of the death — which means it
   needs the supervisory relationship of ADR-0067 or a notification from
   whoever has it.
5. As P2, but decided in canonical text.
6. Re-lookup.
7. Available and can be made mandatory.
8. Wide, and the rules are inspectable text rather than nucleus behaviour.

**Its own bootstrapping question.** The name service must exist before any
driver, which touches `docs/11`'s bootstrapping sequence, and it must itself be
restartable — a registry that cannot be restarted is a single point of failure
with extra steps.

### P4 — the supervisor is the registry

It already creates both parties and already learns of endings through operation
14. It hands out endpoint capabilities as part of launching a client.

1. The supervisor, as part of its existing authority over what it creates.
2. The supervisor records that a service it launched serves an interface.
3. From the supervisor: at launch, or by asking it later.
4. The supervisor knows first — operation 14 is exactly this notification.
5. The supervisor decides; it can revoke, or leave it and answer questions
   about it.
6. Ask the supervisor again.
7. Available; whether mandatory is a choice inside P4.
8. Wide, with the caveat below.

**It makes the supervisor a single point of failure for device access**, and
`docs/34` must say so. It also blurs supervision and naming, which ADR-0051 §3
deliberately separated — supervision is "decisions *about* a component",
naming is a fact about the system.

## 5. Question 8 in detail: what is distinguishable, and what is not

The four stale-client cases, and what each option can guarantee.

| case | P1 | P2 / P3 / P4 |
|---|---|---|
| **A.** client blocked in `endpoint_call` when the service dies | distinguishable — the call fails, and `IPC_V1` §4 with the liveness rule says how | distinguishable, same mechanism |
| **B.** client holds a capability for an endpoint whose receiver is gone, and has not yet called | distinguishable — the first call fails | distinguishable; with re-lookup the client can also obtain a live name |
| **C.** client calls after a restart, holding a pre-restart capability | **not** distinguishable from B without a registry: the client can only observe that its call fails, and cannot tell "gone" from "replaced" | distinguishable — lookup answers with the new publication, and generation says it is a different one |
| **D.** a request the old instance accepted and never answered | **ambiguous, and deliberately so — under every option** | **ambiguous, and deliberately so** |

**Case D is the important one and no option solves it.** A block write whose
answer was lost is exactly where "retry" and "do not retry" differ, and no
capability mechanism can distinguish "the device never got it" from "the device
did it and the answer died with the process". The honest options are to leave
it ambiguous and say so, or to make the operation idempotent at the interface
level — and the second is a property of `block.device.v1`'s *shape*, not of its
publication, which is why this ADR names it and does not decide it.

**What this means for Stage 4 evidence.** Case D must be written into the
slice's evidence as an explicit non-claim, in the form Stage 4D-5 already uses
for durability. A gate that asserted "the client always knows" would be
asserting something no option delivers.

## 6. Consequences

| | P1 | P2 | P3 | P4 |
|---|---|---|---|---|
| client can find a service it did not launch | no | yes | yes | only via the supervisor |
| trusted-base growth | none | **a namespace in ring 0** | none | none |
| new ABI surface | none | object kind + 2 operations | none | none |
| fills `OBJECT_INTERFACE` | no | yes | no | no |
| answers ADR-0051's registry requirement | no | yes | yes | yes |
| `docs/11` step 4 | vacuous | met | met | met |
| case C distinguishable | **no** | yes | yes | yes |
| case D | ambiguous | ambiguous | ambiguous | ambiguous |
| new single point of failure | none | the nucleus, which is one anyway | the name service | **the supervisor** |

## 7. What each would let Stage 4 prove, and what it leaves outside

| | proves at Stage 4 | leaves outside Stage 4 |
|---|---|---|
| **P1** | the whole Branch-A data path and the whole lifecycle, with "who found whom" outside the frame. The identity gate's question is answered | `docs/11` step 4; case C; any claim that a client *recovers* rather than *is re-launched* |
| **P2** | additionally: a client reaches a service by interface, and a pre-restart name is provably distinct from a post-restart one | the Stage 5 reconciliation of a ring-0 namespace with `/system` and `/dev` |
| **P3** | the same, in canonical text, with the rules inspectable | the name service's own supervision and bootstrap ordering |
| **P4** | the same, with no new component | the separation ADR-0051 §3 drew between supervising and naming |

**The narrowing P1 causes should be named now, not discovered in the evidence.**
Without a registry there is no re-lookup, so §3's question 7 has no mechanism
and case C is not distinguishable. The stale-client criterion then becomes "the
capability the client already holds behaves thus" rather than "the client
recovers by re-acquiring". That is a smaller claim and a defensible one, but it
is not what `docs/11` step 5 describes, and the slice would have to say so.

## 8. Obligations each option creates for later stages

- **P1** obliges a later stage to introduce publication before any component
  can reach a service it did not launch, and leaves ADR-0051's evidence
  requirement unsatisfiable until then.
- **P2** creates a ring-0 interface namespace that Stage 5 must reconcile with
  `/system` as a commit tree and `/dev` as a capability namespace, and that
  Stage 6's self-modification must keep coherent across activation.
- **P3** creates a service that must exist before any driver and must itself be
  restartable, which touches `docs/11`'s bootstrapping sequence and Stage 5's
  activation order.
- **P4** makes the supervisor load-bearing for device access; it must enter
  `docs/34` as such, and Stage 5 must decide what happens to naming when the
  supervisor is itself replaced by activation.

## 9. What this ADR does not decide

- **The wire shape of `read`, `write` and `capacity`.** Interface design inside
  `IPC_V1`; no new IPC mechanism, capability type or ABI operation is to be
  created for it.
- **Idempotency of `write`.** Named in §5 as the only lever on case D, and left
  to the interface's own decision.
- **Anything beyond the three operations.** Filesystem, partitions, cache,
  object store, enumeration, multi-device management: out of scope by
  direction.
- **Restart policy.** Canonical supervisor text (ADR-0077 §8).
- **Persistent object/state storage and the capsule-to-repository handoff.**
  Stage 4 deliverables under `docs/16`, and not in this slice.

## 10. Conformance evidence this ADR would require once accepted

Listed so acceptance carries its test obligations, and **not** to be written
into any evidence document before the tests exist and are gated.

1. A service that was not granted the publication capability cannot publish,
   and says so in the audit record (ADR-0051's requirement, finally testable).
2. A client reaches the service holding **only** an endpoint capability: no
   function, no window, no source, no DMA region, and no means of naming the
   device.
3. Cases A, B and — under P2/P3/P4 — C are each distinguished by a separate
   assertion, so that no one of them is satisfied by another's evidence.
4. Case D is recorded as an explicit non-claim, in the form Stage 4D-5 uses for
   durability.
5. Under P2: the nucleus still contains no device-protocol vocabulary, and the
   namespace it gained is bounded and stated.
