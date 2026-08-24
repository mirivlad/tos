<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0067: How a supervisor learns that a service ended

- Status: **Proposed** (option D; the first form of this ADR offered A, B and C
  and recommended B)
- Date: 2026-08-24, decision form 2026-08-25
- Decision level: 2 — it adds an operation to the closed `SYSTEM_ABI_V1` §5
  table, a right to the process object's declared set, an argument register to
  an existing operation, a return value to it, and one piece of per-process
  nucleus state. It changes no Tier 0 invariant and no TOS Core V1 semantics
- Project Architect approval: **not given; this ADR proposes, it does not decide**
- Amends, if accepted: `SYSTEM_ABI_V1` §3, §5 and §7; `CAPABILITY_V1` §3's
  process-object rights; `PROCESS_IDENTITY_V1` §3 and §4

## The gap, stated once

`docs/37`'s Stage 3 evidence list ends with "service restart preserves
identity/audit records", and `PROCESS_IDENTITY_V1` §4 says what that means: a
restart produces a new instance id, increments the restart generation, and keeps
the module, source content id and supervisor lineage. §7.4 makes it a
conformance test.

**Nothing in the accepted contracts lets a supervisor find out that a restart is
due, and nothing lets it assert the generation when it does.** `SYSTEM_ABI_V1`
§5 assigns thirteen operations. A process ends by `process_exit` (12, self
only), by `process_terminate` (9), by a fault, or by the liveness rule of
ADR-0059. In three of those four the ending is a fact only the *nucleus* holds,
and no operation lets another process ask for it.

The nucleus already knows everything the record needs — `retire` emits
`TOS.RUN.PROCESS_EXIT`, `TOS.RUN.PROCESS_TERMINATED` and
`TOS.RUN.PROCESS_DEADLOCKED` with the ending kind, the self-reported status
where one exists, and the ticks and quanta the process was charged. Those events
reach the serial log. They do not reach the supervisor, which is the party
`PROCESS_IDENTITY_V1` §3 makes responsible for the restart generation.

So `/system/boot/init.tos` can start what the policy names and can never notice
that one of them stopped. A supervisor that restarted on a timer instead would
be asserting a generation for an event it never observed, which is the shape
`PROCESS_IDENTITY_V1` §2 exists to forbid.

docs/10 lists "process lifecycle notification" among the primitives the nucleus
provides, and `IPC_V1` §1 repeats "lifecycle" in the primitive half it claims to
be. Neither says what the primitive is. This ADR is that question.

## What the accepted documents already constrain

- `SYSTEM_ABI_V1` §3: the capability an operation requires is `rdi`; arguments
  are `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`; the result is `rax` status and
  `rdx` value; an operation that can block takes a flag register and **blocking
  is the default**, bit 0 meaning do not wait.
- `SYSTEM_ABI_V1` §4: the status space is closed. `E_WOULD_BLOCK`,
  `E_CANCELLED`, `E_LIMIT`, `E_NO_CAPABILITY`, `E_BAD_HANDLE` all exist; no new
  status is needed and none is added. Refusal order is index, generation, type,
  rights.
- `SYSTEM_ABI_V1` §6: blocking is always on **a handle the process holds**;
  there is no wait-for-anything primitive; every blocking operation declares a
  cancellation path observable as `E_CANCELLED`.
- ADR-0059: the nucleus cancels every block when no context is runnable and
  nothing routed can change that. A new blocking operation inherits that rule.
- `SYSTEM_ABI_V1` §7: operation numbers are assigned once and never reused; an
  addition is a minor version, and an older nucleus answers `E_NOT_SUPPORTED`.
- `CAPABILITY_V1` §3: rights are "a finite set from the object type's declared
  rights", and §4 makes attenuation subtractive — there is no operation that
  adds a right.
- `IPC_V1` §6: a message names no sender, and §7 bounds every endpoint queue at
  a constant the system never grows.
- `PROCESS_IDENTITY_V1` §2: the launch record governs, the process's own claim
  about itself is labelled and never the audit record; §3 assigns the restart
  generation to the supervisor and the instance id to the nucleus.
- ADR-0058: what cannot travel in a register travels in the caller's argument
  region at a fixed offset the nucleus knew before it read anything.

## Decision: D

### 1. `process_wait_child`, operation 14

| Register | Meaning |
|---|---|
| `rax` | operation 14 |
| `rdi` | a capability on a **Process object** carrying `RIGHT_WAIT_CHILD` |
| `rsi` | flags; bit 0 = do not wait |
| `rax` (out) | `OK`, `E_WOULD_BLOCK`, `E_CANCELLED`, `E_NO_CAPABILITY`, `E_BAD_HANDLE` |
| `rdx` (out) | the **child process instance id** the record describes |

The rest of the record does not fit in a register and therefore travels the way
ADR-0058 already fixed: the nucleus writes it into the caller's argument region
at a fixed offset, `WAIT_CHILD_RECORD`, known before the call. The nucleus walks
no address the caller chose.

The record is:

- child process instance id (the same value returned in `rdx`);
- the parent supervisor's instance id — the process object the capability named;
- ending kind: exited, faulted, terminated, or ended by the liveness rule;
- self-reported status, **present only when the child reached `process_exit`**,
  and labelled as the child's own claim (`PROCESS_IDENTITY_V1` §2);
- ended-by: the instance id of whoever terminated it, where something did;
- the ending order: a boot-monotonic ending sequence number, and the tick the
  ending was recorded at.

The ending order is in the record because "which of my children died first" must
be answerable without comparing ticks that can repeat, and because the order two
supervisors observe must be the same order.

### 2. The event set is capability-scoped, not "anything"

The operation observes **only the direct children of the process object the
capability names** — the processes whose parent is that object, a relation the
nucleus already asserts and `PROCESS_IDENTITY_V1` §3 already records.

An ordinary supervisor waits on **authority over itself**: it holds a capability
on its own process object with `RIGHT_WAIT_CHILD`, and so learns about the
processes it created. Nothing about that is a wait-for-anything primitive, which
`SYSTEM_ABI_V1` §6 forbids for the reason it gives — waiting on authority one
was never given. Here the wait is on a handle the process holds, and the set of
events it can observe is exactly the set that handle names.

Two consequences follow and are stated rather than left implicit. A grandparent
does not see a grandchild's ending: the relation is direct parentage, not
descent. And a supervisor may be given `RIGHT_WAIT_CHILD` over *another*
process, which lets a supervision hierarchy be built without ambient authority —
and is an information flow that §7 treats as one.

### 3. A lifecycle notice is not a message

It is **not IPC**. It does not enter an endpoint queue, does not consume the
four-message bound of `IPC_V1` §7, and cannot be forged: it is produced by the
nucleus and read from the nucleus, never sent by anything.

That is the substantive difference from option B, which this ADR previously
recommended. A message carries no sender (`IPC_V1` §6), so a process holding
`send` on a supervisor's lifecycle endpoint could fabricate a death and cause a
live service to be restarted. Making the mechanism safe would have made its
safety a property of every endowment rather than of the mechanism.

### 4. The ending is kept in the process slot, as a tombstone

When a child ends, the nucleus does what it does today — releases its address
space, its grant, its frames and its capabilities immediately, and emits the
audit event — and then keeps the slot in `State::Over` holding the record of §1.
Execution resources are freed at the ending; only the record survives.

**A slot holding an uncollected lifecycle record is not reusable.** That single
rule is what makes the notice impossible to lose: the storage for the notice is
the storage for the process, and it was reserved when the process was created.

### 5. Boundedness, and what happens when a supervisor ignores its children

Pending records are bounded by the process table, whose bound the nucleus
already fixes — `MAX_PROCESSES`, four in the implementation today. There is no
queue, no allocation, and nothing that grows: the maximum number of uncollected
endings is the maximum number of processes that ever existed at once.

A supervisor that never collects therefore stops being able to start things: the
next `process_create` finds no free slot and answers `E_LIMIT`, the status that
already means "a declared bound would be exceeded". No new status, no silent
drop, and backpressure that names the real cause. That is a real consequence
with a small table, and it is the intended one — a system that forgot a death
notice in order to keep creating processes would be a system whose audit trail
depends on load.

### 6. Collection, order and blocking

`process_wait_child` returns the **earliest pending direct-child record** by the
ending order of §1. With no pending record it blocks; with bit 0 of `rsi` set it
answers `E_WOULD_BLOCK` instead. A blocked wait is cancellable, observably as
`E_CANCELLED`, and ADR-0059's liveness rule applies unchanged: a supervisor
waiting for a child that can never end is not a state the system can sit in.

A successful collection releases the tombstone. The slot becomes free and
reusable at that instant and not before.

### 7. Slot reuse, generations and instance identity

The slot's capability generation is advanced **when the process ends**, as it is
today, not when the tombstone is collected. Every capability naming that child
therefore stops resolving at the ending, and a handle held across a collection
and a reuse names the generation that ended: by the refusal order of
`SYSTEM_ABI_V1` §4 it is `E_NO_CAPABILITY`, not a live handle on a different
process. **A stale child capability must not come alive.**

Identity has to be separated from both of the things that are convenient to
confuse it with:

- **the capability handle is not identity.** It is an index in one table and
  means nothing in another, which `IPC_V1` §6 already says of endpoints;
- **the slot index is not identity.** Slots are reused; an id that came back
  would make two executions indistinguishable, which `PROCESS_IDENTITY_V1` §4
  forbids in as many words.

The instance id is therefore a boot-monotonic value the nucleus assigns at
creation and never repeats. **The implementation has neither today** — the
nucleus has a slot index and a per-slot generation, and no parent field at all.
Adding the instance counter and the parent link is part of implementing this
ADR, not an assumption it makes.

### 8. Correlation: `process_create` returns the child's instance id

`process_create` gains a return value: `rdx` carries the new child's instance id,
using the result convention `SYSTEM_ABI_V1` §3 already fixes. The lifecycle
record of §1 carries the same id.

Without this a creator would have to correlate by handle, which §7 rules out, or
by module name, which is not unique across restarts — the very case this ADR
exists to serve.

### 9. The restart generation is the supervisor's, and travels in `r8`

`process_create` gains `r8` = **supervisor-asserted restart generation**.

- The nucleus **records** it in the launch record and in process identity. It
  does not compute it, does not increment it, and does not validate it beyond
  its domain. A nucleus that incremented it would be asserting a field
  `PROCESS_IDENTITY_V1` §3 assigns to the supervisor.
- A first launch passes `0`. A supervisor restarting a service passes the ended
  instance's generation **plus one**, which it knows because it collected the
  ending.
- The nucleus asserting the instance id and the supervisor asserting the
  generation is exactly §2's rule: each field has one asserter, and the two are
  never merged.

This closes the restart-identity half of the gap. Whether to restart, how often,
and what marks a candidate unhealthy remain the supervisor's, and are not
decided here.

### 10. When the supervisor itself ends

Its pending tombstones have no authorized receiver left. Then:

- **the audit records remain.** They were emitted at each child's ending and are
  on the boot log; nothing about them depends on collection. No evidence is lost;
- **the programmatic notices are released**, and their slots become free.

A notice nobody can read is not evidence, and keeping it would make a slot
eternal for the sake of a reader that does not exist. This is the one place
where a record is discarded, and it is discarded only after the party entitled
to it has itself ended.

**No reparenting.** The children of an ended supervisor are not adopted, not
re-assigned and not automatically terminated by this ADR; what happens to a
running service whose supervisor died is a policy question this ADR does not
answer. Nor does it introduce any restart policy.

## Threat model

- **Forgery is structurally impossible.** The notice is nucleus-produced and
  nucleus-read. No process can synthesise one, which is the property option B
  could not have without a rule about every endowment.
- **Observation is a capability.** `RIGHT_WAIT_CHILD` over a process object
  discloses its children's ending kinds and their self-reported statuses. That is
  an information flow, granted deliberately by whoever delegates the right and
  removable by attenuation (`CAPABILITY_V1` §4). A negative test must show that a
  process-authority capability *without* the right answers `E_NO_CAPABILITY`.
- **The self-report is still a claim.** A child that lies about its exit status
  changes only the labelled self-report; the ending kind, the ended-by and the
  audit record are the nucleus's. A supervisor that reads the claim as the
  finding has misread the contract, and the record's shape keeps them apart.
- **Availability, deliberately bounded.** Uncollected endings consume slots and
  ultimately answer `E_LIMIT` to `process_create`. A hostile child cannot amplify
  this: its ending produces exactly one record. A negligent *supervisor* can
  exhaust its own ability to create processes, which is a bound it holds the
  authority to release.
- **No unkillable wait.** A supervisor blocked in `process_wait_child` is
  cancellable by authority over it and by the liveness rule, so the operation
  adds no authority the system cannot revoke (`SYSTEM_ABI_V1` §6).
- **No new ambient surface.** The operation requires a capability, writes only
  into the caller's own argument region at a fixed offset, and adds no path by
  which a process reaches a process it was not given.

## Conformance tests the decision requires

1. **Two children that ended before a single wait.** A supervisor creates two
   children; both end before it calls `process_wait_child`. The first call
   returns the child that ended first by the ending order, the second call
   returns the other, and a third call blocks — or answers `E_WOULD_BLOCK` when
   asked not to wait. Neither record is lost, merged or reordered.
2. **A slot with an uncollected record is not reused.** With the table full of
   ended-but-uncollected children, `process_create` answers `E_LIMIT`; after one
   collection it succeeds exactly once more.
3. **Stale child capabilities do not revive.** A handle to a collected child,
   used after its slot has been reused by a different process, answers
   `E_NO_CAPABILITY` — not `OK`, and not `E_BAD_HANDLE`.
4. **Scope is direct parentage.** A grandchild's ending is never returned to the
   grandparent, and a capability without `RIGHT_WAIT_CHILD` answers
   `E_NO_CAPABILITY`.
5. **Correlation holds.** The instance id `process_create` returned equals the id
   in that child's lifecycle record, across a slot reuse in between.
6. **Restart identity** (`PROCESS_IDENTITY_V1` §7.4). A restart increments the
   generation, changes the instance id, and preserves module and supervisor
   lineage — with the generation coming from the supervisor: a nucleus that
   incremented it is a defect, proven by launching with a generation the nucleus
   would not have chosen and reading it back unchanged.
7. **The ending kinds are distinguishable.** Exited with a status, terminated by
   another process, faulted, and ended by the liveness rule each produce their
   own kind, and the self-reported status is absent in the three that never
   reached `process_exit`.
8. **Cancellation.** A blocked `process_wait_child` answers `E_CANCELLED` when
   the liveness rule fires, and the nucleus records who was blocked on what.
9. **A supervisor's death releases its uncollected notices**, its children's
   audit events remain on the log, and the slots become free.
10. **`E_NOT_SUPPORTED`** from a nucleus built before operation 14, without the
    caller being terminated for asking (`SYSTEM_ABI_V1` §7).

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. I-16 and the
  identity plane of docs/03 are served: the ending stays the nucleus's
  assertion and reaches the supervisor without becoming a claim.
- **Canonical representation:** unchanged. Supervisor and policy remain
  canonical TOS Core text; no binary configuration and no new file kind.
- **Trusted-base impact:** one operation, one right, one argument register, one
  return value, and per-slot lifecycle state that replaces nothing. No
  allocation, no queue, no new nucleus subsystem, and no work proportional to
  anything but the process table.
- **Source-to-runtime impact:** unchanged.
- **Recovery and rollback impact:** none at this stage. A bounded, observable
  restart loop (docs/10) remains supervisor policy and is not introduced here.
- **Stage identity gate:** supplies the mechanism for the fifth Stage 3 evidence
  item in docs/37 and the assertion path for `PROCESS_IDENTITY_V1` §4. It closes
  no gate by itself; the tests above are what would.
- **Threat-model impact:** as the section above. New negative tests are required
  for the missing right, the stale handle and the exhaustion bound.
- **Performance contract:** none of it touches the measured IPC path. The
  operation is not on the request/reply path and carries no latency budget;
  `process_create` gains one register read and one value write.
- **Compatibility profile:** a minor version of `SYSTEM_ABI_V1` under §7.
  Operation 14 is spent forever. `process_create` gains an argument and a return
  value, which is a compatible extension only because `r8` was unassigned and
  `rdx` was unused on return — a process built against the earlier set passes
  whatever `r8` held, so the implementation must treat the missing argument as
  the domain error it is rather than as generation zero.
- **Dependencies, licence, patents:** none.

## Alternatives considered

**A — `process_wait` on the child's own authority handle.** Rejected. It obeys
§6 literally, but one task waits on one child, so a supervisor of N services
needs N waits it cannot make concurrently; and `retire` clears the child's
authority at the ending, so a wait made afterwards gets a capability error where
the ending should be. D keeps A's capability discipline and drops its
one-child-at-a-time limit by scoping the wait to a *relation* rather than to a
single object.

**B — a lifecycle message to an endpoint named at creation.** Rejected on
forgeability: messages carry no sender, so the notice would be safe only if
`send` on that endpoint were held by nobody, making the mechanism's security a
property of every endowment. Its queue also had to be extended with a
reservation rule to make the notice undroppable — D gets undroppability from the
slot the process already occupies, without touching `IPC_V1` at all.

**C — a non-blocking `process_status` poll.** Rejected: ADR-0059 measured what
polling costs on this scheduler — 2077 ticks against 81 for the peer doing the
work — and chose blocking over exactly this shape.

**A tombstone with a timeout.** Rejected: a notice that expires is a notice that
can be lost, and the loss would be silent and load-dependent. `E_LIMIT` on
create is the visible form of the same pressure.

## Consequences

The fifth Stage 3 evidence item becomes reachable, and the restart generation
becomes assertable by the party the contract assigns it to. The cost is one ABI
operation, one right, and a rule that a supervisor which ignores its children
stops being able to create them — which is a bound, stated, rather than a
notice quietly dropped.

Restart *policy* — how often, how many times, what marks a candidate commit
unhealthy — remains where docs/10 puts it, in the supervisor, and is not decided
here.
