<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0058: How a call names more than its registers hold

- Status: **Accepted (option A)** (Project Architect-approved)
- Date: 2026-08-19
- Decision level: 2 — it fixes where an operation's arguments live when they do
  not fit in registers, inside the edge ADR-0048 established and the convention
  ADR-0056 fixed; it adds no operation, no status and no right
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-19

## The gap, stated once

`SYSTEM_ABI_V1` §3 gives a call six argument registers. ADR-0056 spends the
first on the capability the operation requires. Three accepted obligations need
more than the five that remain, and none of them can be met today.

**A message that transfers capabilities.** `IPC_V1` §3 defines a message as
inline bytes *plus transferred capabilities plus transferred regions*, and
ADR-0057 fixed the counts at four and two. `endpoint_send` already spends `rdi`
on the endpoint and `rsi` on the payload length. Four handles, two regions and
their two counts do not fit in the four registers left, and a message that
cannot **name** a capability is not a message that refuses to transfer one — it
is a contract clause with no way to be exercised.

**A child's endowment.** ADR-0055 §A states that `process_create` "takes the
child's endowment as an argument, attenuated from what the caller itself holds".
An endowment is a list. The implementation creates children endowed with nothing
because nothing else is expressible, which is the launcher's own rule one level
down and therefore honest — but it is not what the ADR says the operation does.

**Which module a process is created over.** `process_create` names it by an
index into this boot's source set, because an index fits in a register. A
supervisor reasoning about `/system/policy/` (ADR-0051 §3) has a module *name*,
and a name is a string. Naming by ordinal is a position in a list nobody
published.

One question, three places: **where does an argument live when it does not fit
in a register?**

## What the accepted documents already constrain

- **No pointer the nucleus walks.** §3: "Arguments are values and handles, never
  pointers the nucleus dereferences without bounds." Any answer that hands the
  nucleus an address a process chose is excluded before the options begin.
- **§3 already answers it for buffers**, and the answer is circular here: "Where
  an operation needs a buffer, the buffer is named by a handle to a region the
  process already holds (`IPC_V1` §5)." Region transfer is one of the three
  things that needs this decision, so a region capability cannot be its
  precondition.
- **The mechanism already exists.** Every process has a message slot: a fixed
  address the launcher maps, one frame, whose physical address the nucleus knows
  and reads through its own identity map rather than through the process's. It
  was built for `IPC_V1`'s inline payload for exactly this reason — 256 bytes do
  not fit in registers either — and the report region has worked the same way
  since the first process.
- **Bounded work.** ADR-0049 §5 keeps unbounded work out of interrupt context,
  and docs/35 bounds boundary crossings per request/reply at four. An answer
  that multiplies calls spends that budget.

## What must not be done about it

**A pointer with a length, checked.** "The nucleus validates the range" is how
every system with this bug describes it. §3's rule is not about validation
quality; it is that the nucleus never walks an address a process chose, so that
there is no range to get wrong.

**Widening the register set by shrinking the contract.** Capping messages at two
capabilities because two fit would reopen ADR-0057, which fixed four, in order
to make an implementation easier. That is the trade this project does not make.

## Options

### A — the slot becomes the call's argument region

Generalize what exists. Each execution context has one fixed-address region the
launcher maps and the nucleus knows the physical address of. An operation whose
arguments do not fit in registers reads them from there, at a layout fixed by
the contract that owns the operation; counts stay in registers, so the nucleus
knows how much to read before it reads anything.

`IPC_V1` fixes the message layout — payload, then the transferred-handle table.
`SYSTEM_ABI_V1` fixes `process_create`'s — the endowment list, and the module
name. Neither is invented at a call site.

Costs: the region stops being "the message slot" and becomes what it is, which
is a rename and a restatement of its purpose. Its contents are argument bytes,
never a channel: nothing may be left in it between calls and no operation may
report through it. And the region belongs to an **execution context**, not to a
process — Stage 3 gives a process one, and the day a process has two, two calls
in flight would otherwise share one buffer. Saying so now costs a sentence;
discovering it later costs a memory-corruption bug that looks like a scheduler
defect.

### B — arguments in a region named by a region capability

§3's literal answer for buffers, applied to arguments: the caller holds a region
capability and passes its handle; the nucleus maps what it was given.

Costs: circular at the bootstrap, which is where this decision is needed. The
first region capability has to come from somewhere, region transfer is one of
the things that needs bulk arguments, and every process needing to make such a
call would need a region in its endowment — putting a memory object into every
endowment for the sake of argument passing. It is the right answer for a *large*
payload, which is what §5 already says it is, and the wrong shape for a list of
four handles.

### C — several calls that accumulate, then commit

`endpoint_send` becomes "add a capability to the message being built", repeated,
then "send". `process_create` likewise.

Costs: it puts a partially built message in the nucleus, per context, between
calls — call-spanning state that a fault, a termination or a cancellation must
then clean up, and that `IPC_V1` §3's "delivered whole or not at all" now has to
be defended against rather than being true by construction. It also spends the
docs/35 crossing budget several times over for one message.

## Recommendation

**A**, with three things stated in the decision rather than left to be inferred.

1. The region is **the call's argument region**, named as such, one per
   execution context, mapped by whoever built the context, and known to the
   nucleus by its physical address. A process never passes its address to
   anything.
2. Its contents are arguments and nothing else. It is not a channel, nothing
   persists in it across calls, and no operation reports through it. The report
   region remains the only thing a process writes to be read.
3. Every layout in it is fixed by the contract that owns the operation, with the
   count in a register, so that the nucleus knows the extent of what it will read
   before it reads a byte of it.

A is not the smallest change; it is the one that makes the other two questions
stop being separate. The mechanism is already built, already justified, and
already carries the one property that matters — the nucleus reads memory whose
address it chose, through its own map, at a size a register declared.

If A is accepted:

- `Launch`'s `message_base`/`message_length` become `arguments_base`/
  `arguments_length`, and `LAUNCH_VERSION` becomes 3.
- `IPC_V1` §3 gains the message layout inside that region, and §6 gains the
  transferred-handle table it has so far described only in prose.
- `SYSTEM_ABI_V1` §3 gains a paragraph naming the region and the rule that
  arguments never persist in it, and §5 gains `process_create`'s layout.
- `process_create` names its module by path rather than by index, and the index
  form is retired rather than kept beside it: two ways to name a module is one
  more than a supervisor should have to choose between.

## What each option costs to build

| | A — argument region | B — region capability | C — accumulate and commit |
|---|---|---|---|
| Mechanism to build | none; a rename and per-operation layouts | region objects, region capabilities, mapping and unmapping | per-context partial-message state and its teardown on every abnormal exit |
| Contracts changed | `Launch` v3; layouts in `IPC_V1` and `SYSTEM_ABI_V1` | as A, plus a region endowment in every launcher constant | as A, plus `IPC_V1` §3's atomicity restated as an obligation |
| Bootstrap | works with what a process already has | circular: needs a capability to obtain capabilities | works |
| Crossings per message | one | one | one per capability, plus one |
| When a process has two contexts | the region is per context; already said | per context, same | partial state is per context, and cancellation between calls is a new case |

## Boundary

Phase 4 Task 4 — linear capability transfer, region transfer, and the
confused-deputy test that needs both — cannot be built without this. `process_create`
works today with an empty child endowment and an index-named module, which is
correct and narrower than ADR-0055 describes; this is what widens it to what the
ADR says. It is independent of ADR-0059: blocking needs no bulk argument.
