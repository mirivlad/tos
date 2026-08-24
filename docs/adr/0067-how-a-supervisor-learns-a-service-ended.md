<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0067: How a supervisor learns that a service ended

- Status: **Proposed**
- Date: 2026-08-24
- Decision level: 2 — every candidate adds an operation to the closed
  `SYSTEM_ABI_V1` §5 table or a nucleus-originated message form to `IPC_V1`;
  none changes a Tier 0 invariant or TOS Core V1 semantics
- Project Architect approval: **not given; this ADR proposes, it does not decide**

## The gap, stated once

`docs/37`'s Stage 3 evidence list ends with "service restart preserves
identity/audit records", and `PROCESS_IDENTITY_V1` §4 says what that means: a
restart produces a new instance id, increments the restart generation, and keeps
the module, source content id and supervisor lineage. §7.4 makes it a
conformance test.

**Nothing in the accepted contracts lets a supervisor find out that a restart is
due.** `SYSTEM_ABI_V1` §5 assigns thirteen operations. A process ends by
`process_exit` (12, self only), by `process_terminate` (9, performed by whoever
holds authority over it), by a fault, or by the liveness rule of ADR-0059. In
three of those four the ending is a fact the *nucleus* holds, and there is no
operation with which another process may ask for it.

The nucleus already knows everything the record needs — `retire` emits
`TOS.RUN.PROCESS_EXIT`, `TOS.RUN.PROCESS_TERMINATED` and
`TOS.RUN.PROCESS_DEADLOCKED` with the ending kind, the self-reported status where
one exists, and the ticks and quanta the process was charged. Those events reach
the serial log. They do not reach the supervisor, and the supervisor is the party
`PROCESS_IDENTITY_V1` §3 makes responsible for asserting the restart generation.

So the current textual supervisor (`/system/boot/init.tos`) can start what the
policy names and can never notice that one of them stopped. A supervisor that
restarts on a timer instead would be asserting a generation for an event it never
observed, which is the shape `PROCESS_IDENTITY_V1` §2 exists to forbid.

docs/10 lists "process lifecycle notification" among the primitives the nucleus
provides, and `IPC_V1` §1 repeats "lifecycle" in the primitive half it claims to
be. Neither document says what the primitive is. This ADR is that question, not a
new requirement.

## What the accepted documents already constrain

- `SYSTEM_ABI_V1` §6: blocking is always on **a handle the process holds**;
  there is no wait-for-anything primitive; every blocking operation declares a
  cancellation path observable as `E_CANCELLED`.
- ADR-0059 (option D): the nucleus cancels every block when no context is
  runnable and nothing routed can change that. A new blocking operation inherits
  that rule and needs no new cancellation machinery.
- `SYSTEM_ABI_V1` §4: the status space is **closed**. A candidate that needs a
  new status is a heavier change than one that does not.
- `SYSTEM_ABI_V1` §7: operation numbers are assigned once, never reused, and an
  addition is a minor version — a process built against the smaller set is
  unaffected, and one built against the larger set receives `E_NOT_SUPPORTED` on
  an older nucleus.
- `IPC_V1` §7: every endpoint queue is bounded and **the system never grows a
  queue to accept a message**. The implementation's bound is four
  (`nucleus/src/ipc.rs`, `QUEUE_DEPTH`).
- `IPC_V1` §6: "what is queued is the object, not the sender's handle". A
  received message carries no sender identity, and no accepted contract gives it
  one.
- `PROCESS_IDENTITY_V1` §2: the launch record governs and a process's claim about
  itself is labelled as such and is never the audit record.
- ADR-0055/0058/0061: a process's first capabilities arrive as the endowment its
  creator passes at `process_create`; nothing else produces one.

Two of those decide more than they look like they do. Because a message names no
sender, **a lifecycle message is not evidence** unless the endpoint it arrives on
is one no process can send to. And because a queue is bounded at four and may not
grow, a design that delivers endings as messages must say what happens when a
supervisor with five dead children has a full queue — while `retire` runs in the
nucleus, with no context to block and no allocation available.

## The three shapes an answer can take

### A. Wait on the child's own authority handle

A new operation — `process_wait`, number 14 — takes the process-authority
capability the supervisor already holds for that child (the same handle
`process_terminate` requires) and blocks until that process is over, then returns
the ending kind and, where one exists, the self-reported status.

- It obeys §6 literally: the handle waited on is one the process holds, and it
  names exactly the one child.
- It needs no queue, no reservation and no allocation: the ending is already in
  the child's slot.
- It needs no new status: `E_CANCELLED` covers the liveness rule, and the
  existing per-handle validation covers the rest.
- **It has a race the implementation makes visible.** `retire` today advances the
  slot's generation and clears its authority, so every capability naming that
  child stops naming anything the instant it ends. A supervisor that calls
  `process_wait` after the ending therefore gets a capability error where the
  ending should be, and cannot distinguish "it already ended" from "I never held
  authority over it". Option A is only correct if the ending survives retirement
  in the slot until the one holder of that authority collects it, which is a
  change to the retire path and to when a slot may be reused.
- **Its cost is composition.** A supervisor with one task blocked in
  `process_wait` on child 1 cannot notice child 2 ending, and cannot receive IPC
  while waiting. Stage 3 processes have `tasks: 1` in the resource header of the
  fixtures we have. A supervisor of N services would need N waits it cannot make
  concurrently, so the shape scales to one supervised child or to a polling loop
  that is not a wait at all.

### B. A lifecycle message to an endpoint named at creation

`process_create` gains a fourth argument-region slot naming an endpoint the
creator holds. When that child is retired, the nucleus queues one inline message
on that endpoint: instance id, ending kind, self-reported status where present.
The supervisor blocks in `endpoint_receive` on one endpoint and learns of any of
its children.

- It composes: one wait covers N children, and the same wait can carry ordinary
  service traffic.
- It is the reading docs/10 and `IPC_V1` §1 already use — "notification" is a
  message.
- **It owes two answers.** First, the bounded queue: the honest form is a slot
  reserved at `process_create` for the one message that child will eventually
  produce, so the notice can never be dropped and the queue never grows — the
  reservation is what makes it deliverable, and a create that cannot reserve
  fails at create time rather than losing a death notice later. That raises the
  effective per-endpoint bound from a constant to a constant plus the number of
  live children, which is a change to `IPC_V1` §7's arithmetic and must be
  written into the contract rather than left to the implementation.
  Second, forgeability: because messages name no sender, a process holding `send`
  on the supervisor's lifecycle endpoint can fabricate a death and cause a live
  service to be restarted. The available answer is that the endowment gives the
  supervisor `receive` only and grants `send` on that endpoint to nobody, since
  `capability_attenuate` cannot add a right — provable by a gate, but it makes the
  security of the mechanism a property of every endowment rather than of the
  mechanism.

### C. A non-blocking status query

`process_status`, number 14, takes the same authority handle and answers
immediately: running, or over with this ending. A supervisor polls.

- It is the smallest change and needs no blocking semantics at all.
- ADR-0059 measured what polling costs on this scheduler: a waiting process was
  charged **2077 ticks** against **81** for the peer doing the work, one tick per
  quantum, every one of them spent asking again. That ADR chose blocking over
  exactly this shape for exactly this reason.
- It is listed because it is implementable and because rejecting it explicitly is
  worth more than leaving it unstated.

## What is recommended, and what is left open

**B, with A's honesty about scope, is the recommendation** — and it is a
recommendation, not the decision. It is the only shape in which a supervisor of
several services waits once, which is what supervision is; C was already rejected
by ADR-0059's measurement in a different guise; and A is a correct operation whose
limit is reached at the second child.

Three things B must carry, and they belong in the decision rather than in the
implementation that follows it:

1. `IPC_V1` §7 gains the reservation rule: one queue place per live child of the
   endpoint's holder, reserved at `process_create`, released when the notice is
   taken. The queue still never grows to accept a message; it is sized for the
   messages that were promised.
2. The notice is a **wakeup, not an audit record**. The audit record stays the
   nucleus's serial event, and `PROCESS_IDENTITY_V1` §2's rule is unchanged: a
   supervisor that restarts on a forged notice has restarted a live service, and
   the log shows both the restart and the absence of an ending.
3. The restart generation stays the supervisor's assertion (§3), so B adds no
   nucleus knowledge of restarts and no nucleus restart policy. Restart policy
   remains "a decision about a component" in `SYSTEM_ABI_V1` §6's words.

If the Project Architect prefers A, the Stage 3 restart evidence is still
reachable with a one-service supervisor, and the gate should then say plainly
that it demonstrates restart for one child rather than supervision in general.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. I-16 (a running
  component can report the source it came from) and the identity plane of docs/03
  are served: the ending stays the nucleus's assertion under every option.
- **Canonical representation:** unchanged. The supervisor and the policy remain
  canonical TOS Core text; this adds no binary configuration and no new file kind.
- **Trusted-base impact:** option A adds one operation and no new nucleus state;
  option B adds one operation argument, a per-child queue reservation and one
  nucleus-originated message form. Neither adds a driver, a parser or an
  allocation to the nucleus.
- **Source-to-runtime impact:** unchanged.
- **Recovery and rollback impact:** none at this stage. A bounded, observable
  restart loop (docs/10) is a supervisor-policy question that this ADR
  deliberately does not answer.
- **Stage identity gate:** supplies the missing mechanism for the fifth Stage 3
  evidence item in docs/37; it closes no gate by itself.
- **Threat-model impact:** option B introduces a forgeable notice unless the
  lifecycle endpoint's `send` right is held by nobody, and that requires a
  negative test — a process holding `send` must be shown either not to exist in
  the endowment or not to be able to obtain the right by attenuation. Option A
  introduces no message and no forgery, and its authority check is the existing
  per-handle one.
- **Performance contract:** none of the three touches the measured IPC path. A
  reservation changes an accounting check at `process_create`, which is not a
  Stage 3 budgeted path; the claim must nonetheless be measured rather than
  asserted if `process_create` later acquires a budget.
- **Compatibility profile:** a minor version of `SYSTEM_ABI_V1` under §7 in every
  option; no operation number is reused and no existing operation changes meaning.
- **New dependencies:** none.
- **Licence and patent impact:** none identified; no mechanism here is on the
  `PATENTS.md` high-risk list.
- **Tests the decision requires:** a child that ends is observed by its
  supervisor and by nothing else; a restart increments the generation, changes
  the instance id and preserves module and supervisor lineage
  (`PROCESS_IDENTITY_V1` §7.4); an ending that arrives while the queue is at its
  bound is delivered rather than dropped (option B); a process without the
  authority handle receives the existing capability denial (options A and C); the
  liveness rule cancels a wait for a child that outlives every runnable context
  (option A).

## Consequences

Accepting any option makes the fifth Stage 3 evidence item reachable and leaves
restart *policy* — how often, how many times, what marks a commit unhealthy —
where docs/10 puts it, in the supervisor. Rejecting all three leaves Stage 3
unable to demonstrate restart at all, which should then be recorded as a
deliberate deferral with its own reason rather than as an item still open.
