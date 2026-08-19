<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0059: What it means to wait, and what ends a wait nobody can end

- Status: **Proposed**
- Date: 2026-08-19
- Decision level: 2 — it fixes the scheduler's termination condition, adds a
  process state, and settles which party owns the bound on waiting; it adds no
  operation, no status and no right
- Project Architect approval: *(unsigned)*

## The gap, stated once

`IPC_V1` §4 says `endpoint_call` "sends and **blocks** for the reply". Operation
3 is therefore not implementable without blocking, and the implementation
answers `E_WOULD_BLOCK` to a receive with nothing to take, which is conformant
for `endpoint_receive` and no answer at all for `endpoint_call`.

`SYSTEM_ABI_V1` §6 states the obligation that comes with blocking: an operation
that can block declares a cancellation path, cancellation is observable as
`E_CANCELLED`, and "no operation blocks indefinitely without one: an unkillable
process is an authority the system cannot revoke". Blocking is always on a
handle the process holds; there is no wait-for-anything primitive.

**And there is a fact about this stage that decides the shape of the answer.**
ADR-0049 routes exactly one interrupt, the timer, and the timer wakes nobody. So
a state in which no context is runnable and something is blocked is a state
nothing in Stage 3 can leave. The scheduler's loop today ends when it finds
nothing runnable, and treats that as "everything finished" — which, once
blocking exists, would report a system that is permanently stuck as a boot that
succeeded. That is the failure this project exists to prevent, and it is a
consequence of adding blocking rather than a pre-existing defect.

A measured cost, so the question is not abstract: in a boot where an expected
message never arrived, the waiting process was charged **2077 ticks** of retrying
against **81** for the peer that did all of the work. A tick is exactly one
quantum (measured: `ticks=81 quanta=82`), so the waiter took eighty-one turns
of the machine and spent every one of them asking again.

## What the accepted documents already constrain

- `IPC_V1` §4: `endpoint_call` blocks; the caller's cancellation path is the
  release valve; the reply capability's lifetime is bounded by the caller's.
- `IPC_V1` §7: a sender meeting a full queue is told — `E_LIMIT` for a
  non-blocking send, "blocking with a cancellation path otherwise". Senders
  block too.
- ADR-0049 §6: the tick is exposed "only as far as a scheduler **and a bounded
  IPC timeout** need".
- docs/34 X3.5 names "holds a receiver blocked forever" as a threat, with "every
  blocking operation cancellable" as its control — and states an explicit Stage 3
  non-goal: fair-share scheduling and priority-inversion control.
- `SYSTEM_ABI_V1` §4's status space is **closed**. There is no `E_TIMEOUT`, and
  adding a status is a heavier change than adding a field.

## The two questions that wear one word

"How long may a process wait" is two decisions, and merging them is what makes
the answer look impossible.

**Liveness** — may the system be in a state it cannot leave? That is a property
of the machine, and the nucleus is the only party that can see it. It needs no
number.

**Patience** — how long may *this* process wait before somebody decides it is
stuck? That is a decision *about a component*, of the same class as restart
policy and shutdown timeout, which ADR-0051 §3 already placed in
`/system/policy/` under whoever has the authority to launch the component.

## Options

### A — non-blocking only, as today

`E_WOULD_BLOCK`, and the caller asks again.

Costs: operation 3 stays unimplementable, because `IPC_V1` §4 says it blocks.
A waiter spends its whole quantum retrying — measured above. docs/35's IPC
round-trip budget cannot be measured at all, because what would be measured is
scheduling noise. Every service written against it is a polling service, and
they all change shape when blocking arrives.

### B — blocking, released only by a peer or by cancellation

Costs: the only valve is cancellation, and cancellation requires
`process_terminate` — an authority that exists now but that no launcher constant
grants to a peer of an ordinary process. A mutual wait is therefore unbreakable
by anything but the nucleus, and B gives the nucleus no rule for breaking it.
Total deadlock becomes a state the scheduler exits into a successful halt.

### C — blocking with a deadline the caller supplies, in ticks

Costs: expiry has nowhere to land. `E_WOULD_BLOCK` reads "a **non-blocking**
operation had nothing to do" and the operation was blocking; `E_CANCELLED` reads
"was cancelled" and nobody cancelled it. A new status is a heavier change than
this question warrants. Worse, it puts time into protocol semantics on a system
whose tick is deliberately uncalibrated (ADR-0049 §6), so a service that works
on a fast machine and fails on a slow one is a correctness bug wearing the
costume of a flaky test.

### D — blocking, with the nucleus's liveness rule as the valve

A blocked context is not runnable and is woken by the peer operation that makes
its wait satisfiable. The nucleus's rule is not a duration:

> When no context is runnable and some context is blocked, and **nothing routed
> can change that**, every block is cancelled at that instant with
> `E_CANCELLED`, and the nucleus records who was blocked on what.

The clause matters. Today "nothing routed can change that" is true by
construction, because the timer is the only interrupt and it wakes nobody.
Stage 4 routes a device interrupt, and a process waiting on a disk *will* be
woken — so a rule written as "nothing runnable" would become quietly wrong at
exactly the moment a driver exists. Written this way, Stage 4 has to revisit it
on purpose.

`E_CANCELLED` is accurate rather than approximate here: the operation was
cancelled, and the canceller is the nucleus.

A livelock terminator, because cancelled contexts may block again: the nucleus
counts **consecutive firings of the rule with no message delivered between
them**. Two is a deadlock — one repetition is a process that handled
`E_CANCELLED` and tried once more. The blocked processes are then ended the way
a fault ends one, attributed, on the record, and the boot reaches its ordinary
verdict through machinery that already exists. That number is a count of a rare
event, not a duration.

Costs: partial starvation — one context blocked while others run and never send
— is bounded by nothing the nucleus owns. That is not an oversight; it is the
patience question, and the nucleus taking it would be taking service policy that
ADR-0048 §2 does not give it. Until a supervisor carries it, it is a **named
limitation** of Stage 3.

## Recommendation

**D**, with `E_WOULD_BLOCK` kept as the answer to a deliberately non-blocking
form, so a caller that wants to poll still can.

The mechanism of B — a blocked state and an exact wake — because `IPC_V1` §4
requires it and operation 3 does not exist without it. The nucleus's liveness
rule as the valve, because it is the only bound that depends on nothing nobody
holds, and because it fires exactly when the alternative is a permanent stop and
never otherwise: on a system that is making progress it costs nothing at all. Not
C, because a number in ticks is a guess about time on a system that has no time,
and the right moment to discover the number is when a measured service needs it.

Patience stays with whoever launched a process. Recording the limitation is
better than a nucleus constant that looks like an answer: a service for which
such a constant were the working value would have discovered not a timeout but
an absent supervisor.

If D is accepted:

1. A process slot gains a blocked state recording **what it is blocked on** — a
   handle it holds, per §6 — and the operation it is blocked in.
2. `endpoint_send` on a full queue and `endpoint_receive` on an empty one block;
   the peer operation makes the waiter runnable. Enqueue-then-wake, which is two
   payload copies and inside docs/35's bound; a direct handoff is an
   optimization with its own measurement, later.
3. The scheduler's termination condition changes from "no runnable context" to
   "no runnable context **and none blocked**". The blocked case is the liveness
   rule, not the end of the boot.
4. Cancelling a blocked `endpoint_call` invalidates its reply capability rather
   than leaking it (`IPC_V1` §9.5). The generation mechanism already does this.
5. Patience is recorded as a Stage 3 limitation in docs/34 X3.5 beside the
   control it qualifies, and belongs to `/system/policy/` when supervisors read
   it.

## What each option costs to build

| | A — non-blocking | B — peer or cancel only | C — caller deadline | D — liveness rule |
|---|---|---|---|---|
| `endpoint_call` (op 3) | impossible | works | works | works |
| New status needed | no | no | **yes** | no |
| Total deadlock | impossible | halts as success | resolves after the deadline | diagnosed at the instant it forms |
| Partial starvation | impossible | unbounded | bounded by a guess | unbounded, and named |
| Cost on a healthy run | a waiter's whole quantum | none | none | none |
| Stage 4 | unchanged | unchanged | timeouts fight device latency | the rule's second clause is what must be revisited |

## Boundary

Phase 4 Task 4's blocking half depends on this; its transfer half depends on
ADR-0058 instead, and the two are independent. Nothing in the scheduler, the
capability table or the process-authority chain built before this changes under
any of the four options.
