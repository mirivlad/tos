<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS IPC Contract — Version 1

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0048 (Project Architect-approved, 2026-08-12), which fixes the
boundary this contract describes.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs.

## 1. Role

docs/10 fixes the primitive set the nucleus provides — typed endpoint handles,
send and receive, capability transfer, region transfer, notification,
cancellation, lifecycle — and puts request/reply, streams, pub/sub and discovery
in textual libraries and services above them. This contract is the primitive
half.

The distinction matters for what is *not* here: there is no service discovery by
name, no broadcast, no routing and no message bus. Discovery returns handles
under a namespace capability (docs/10), which makes it a service.

## 2. Endpoints

An endpoint is an object; an endpoint **handle** is a capability naming it with
rights (`CAPABILITY_V1` §3). Rights are `send`, `receive`, `call`, and they are
separate: holding the right to be called is not the right to call.

An endpoint has exactly one receive-rights holder at a time. A second one would
make delivery non-deterministic in a way no schema could describe.

## 3. Messages

```text
message = inline bytes + transferred capabilities + transferred regions
```

| Property | Bound |
|---|---|
| Inline payload | **256 bytes** — small enough to copy without allocation |
| Transferred capabilities | **4** per message |
| Transferred regions | **2** per message |

The three numbers are fixed by ADR-0057 and are constants of this contract
version. They are stated here rather than left to an implementation for the
reason §7 gives: a refusal test against an unstated bound tests the
implementation's opinion of it.

256 bytes is the boundary between the path that copies and the path that maps,
not a limit on what can be communicated — anything larger travels as a region
(§5). It is deliberately far below a page: the copy happens twice per round trip
on a path that runs with interrupts masked, and ADR-0049 §5 forbids unbounded
work there. Four capabilities is enough for a request that hands over an
endpoint, a reply endpoint and a region handle with one spare, and it bounds how
many of a *receiver's* table slots one call can consume.

Anything larger travels as a region (§5). The inline maximum is a constant of
the contract, not a per-endpoint parameter, because a per-endpoint size makes
the nucleus's fast path depend on data it must first go and read.

A message is delivered whole or not at all. There is no partial receive, so a
receiver never has to reason about a half-message.

## 4. Request and reply

`endpoint_call` sends and blocks for the reply. The nucleus creates a single-use
reply capability and gives it to the receiver; replying consumes it. A receiver
that never replies does not hold the caller forever: the caller's cancellation
path (`SYSTEM_ABI_V1` §6) is the release valve, and the reply capability's
lifetime is bounded by the caller's.

Single-use is the property that keeps a reply from becoming an unbounded channel
back into the caller.

**Where the reply capability arrives** (ADR-0058). It occupies the **last** slot
of the message's transfer table, always, so a receiver knows where to look
without being told how many capabilities the caller chose to send. A call
therefore carries one fewer capability of its own than a send does: one place is
spoken for by the answer.

**A call does not wait for room.** A full queue means the request could not be
made and `E_LIMIT` says so; what a call waits for is the answer. Blocking for
room and then calling would be a call assembled in two steps, with a half-made
call held in the nucleus in between.

Single use is a property of a counter, not of a flag anyone must remember to
clear: the reply capability names the call, and anything that ends that call —
the reply itself, a cancellation, or the caller ending — moves the counter, after
which the capability resolves to nothing. That is also how `E_CANCELLED` on a
blocked caller invalidates the reply rather than leaking it (§9.5).

## 5. Regions

A large payload is a memory region transferred as a capability, exactly as
docs/42 §2 requires — `Region<T>` originates only through a capability
operation, with element type, alignment, access, size, lifetime and transfer
rules declared by the interface.

The nucleus maps and unmaps; it does not copy the payload through itself. A
transferred region leaves the sender's address space at transfer, if the
interface declares the transfer linear; a shared region is mapped in both under
the access mode the grant declares, and `Shared`/`mut` rules from docs/40–41
govern what may be done with it.

## 6. Capability transfer

Capabilities travelling in a message follow `CAPABILITY_V1` §4: the receiver
gets its own handle, linear capabilities are consumed atomically, and a message
that fails to deliver transfers nothing.

**Where they are named** (ADR-0058). The handles a message carries are a list,
so they do not travel in registers: they sit in the sender's argument region at
`MESSAGE_CAPABILITIES`, and the call says in a register how many of them to
read. The receiver finds *its own* handles at the same offset in its own region,
with unfilled slots zeroed — and a handle of all zeros names nothing in any
table, so a receiver reads the whole table and needs no count beside it.

**What is queued is the object, not the sender's handle.** A handle is a name in
one table and means nothing in another, and the sender may release it, or end,
between the send and the delivery. The send resolves what it was given, refuses
the whole message if any of it does not resolve, and the queue carries the
objects; the receiver's names are made when the message reaches it. That is also
why a failed send transfers nothing — there is no point at which a partial
transfer exists.

Sending a capability is **delegation**: the sender keeps what it had. Transfer
that consumes the sender's handle is `CAPABILITY_V1` §4's *linear* case, and it
applies to capabilities an interface declares linear. No Stage 3 object type is
so declared, so nothing in Stage 3 is consumed by being sent — which is a
statement about what exists rather than a relaxation of the rule.

## 7. Queues and backpressure

Every endpoint queue is bounded. When it is full a sender is told — `E_LIMIT`
for a non-blocking send, blocking with a cancellation path otherwise. **The
system never grows a queue to accept a message.**

A receive with nothing to take is the same shape the other way round:
`E_WOULD_BLOCK` for a non-blocking receive, blocking otherwise. Both forms are
selected by the flag `SYSTEM_ABI_V1` §3 describes, and blocking is the default.

**The operation that satisfies a wait performs it.** A send that queues a
message hands it to a context waiting for one and answers that context's call;
a receive that frees a place queues the message of a context waiting for room.
A woken context does not wake up to ask again — it wakes up answered. That is
two copies of an inline payload, sender to queue and queue to receiver, which is
what docs/35 budgets. docs/10 states the reason:
unbounded memory growth through message accumulation is a denial of service that
looks like generosity.

Backpressure is visible to the sender. A dropped message with a success status
would make every protocol above this one unsound.

## 8. Budgets this contract must meet

From docs/35 §Stage 3, restated as obligations on the implementation:

- no dynamic allocation in the nucleus fast path;
- at most two payload copies for an inline message;
- large payloads by region, never copied through the nucleus;
- at most four user/kernel boundary crossings per request/reply, excluding
  scheduler preemption;
- capability validation constant-time in the holder's capability count.

### The observational benchmark, and the bound that was withdrawn

This section once bounded p99 request/reply at "no more than 8 times an
in-process function-call benchmark". **ADR-0068 removed that bound from the
Stage 3 conformance budgets**, having measured that no measurement profile
available on the ADR-0040 platform yields a ratio interpretable as intrinsic IPC
overhead. It was not replaced by another coefficient and the denominator was not
redefined to make a quotient pass. The absolute bound below is the conformance
latency budget of this contract.

The benchmark itself is retained, and its definition still stands where it was
written **before any measurement existed**, because a benchmark chosen
afterwards can be made slow enough to pass anything:

> The in-process function-call benchmark is a call to an exported TOS Core
> function taking one 64-byte value parameter and returning `unit`, executed by
> the same engine build, in the same process, on the same reference platform
> (ADR-0040).

It is an in-process *TOS Core* call, not a Rust call and not an empty loop. It
is measured, retained and reported beside the IPC series, together with the
ratio it forms, as **observational and regression data**: a large movement in
either series between commits is worth investigating, and neither can make a run
red. A report presenting the ratio says which of the two it is.

The numerator is one actual 64-byte `endpoint_call` and its 64-byte reply
between the two endowed processes. One unmeasured exchange first leaves the
server blocked in atomic `endpoint_reply_receive`; `OPEN` is emitted immediately
before each subsequent call and `CLOSE` immediately after it returns. The
server's atomic answer-and-enter-wait operation is part of the request/reply
interval: it blocks the server before the client call returns. Only the server's
subsequent residence in that blocked state, client/report preparation and
shutdown stay outside the interval. Preemption is active, so a timer tail is
part of the latency rather than a removable observer cost. The nearest-rank p99
of the retained series must satisfy `<= 200 µs`; a retry cannot replace a
failure.

A latency series is **3 warm-ups and 300 retained individual samples**
(ADR-0068). At 21 the nearest-rank p99 is the maximum of the series and is below
the true p99 four times in five; at 300 it is rank 297, and it reports a tail
value across the whole measured range of interrupt rates rather than in about
half of series. The active-preemption record binds the scheduler quantum and the
APIC divider, because the tail's arrival rate is the interval divided by the
tick period.

ADR-0066 fixes the measurement boundary. One external observer on the ADR-0040
profile measures its empty marker floor, this call and the 64-byte IPC exchange
with the same QEMU build and marker path. No floor or marker cost is subtracted.
A missing, duplicate, overlapping, mismatched or out-of-plan marker, a
reversed/zero/negative interval, a wrong sample count or a dropped trace event
invalidates the series. The marker protocol's four-bit sequence identity wraps
every sixteen blocks, which is admissible only because the decoder verifies a
predeclared exact tag plan: a duplicate the plan did not predict, or a tag out
of the planned order, invalidates the run rather than being tolerated as a wrap.

Before IPC timing, the observer must resolve this call in one prepared boot.
Each retained block contains an adjacent floor/call pair with the same sequence;
the work bit distinguishes them and their order alternates by block. At least 19
of 21 paired differences must be positive, the predeclared one-sided exact sign
test at `p <= 0.000111`; every non-positive difference remains raw and counts
against the verdict. The exact nucleus/runtime hashes and Cargo features bind
the no-preemption denominator build. Failure is diagnostic evidence and IPC
timing does not begin. Batching and dividing by the batch size does not measure
the latency this section specifies.

## 9. Conformance evidence

1. Each of the three §3 bounds refuses rather than truncates: a message over
   256 inline bytes, one naming more than 4 capabilities, and one naming more
   than 2 regions are each refused whole, and none is silently reduced to the
   bound. A truncation that returned success would make the receiver's copy a
   different message from the sender's.
2. A full queue produces `E_LIMIT` or a cancellable block, never a silent drop
   and never an allocation.
3. A failed send transfers no capability and no region: checked by attempting a
   send that fails after the capability check.
4. An endpoint with one receiver cannot acquire a second.
5. A caller cancelled while blocked in `endpoint_call` returns `E_CANCELLED`,
   and the receiver's reply capability is invalidated rather than leaked.
6. A region transferred linearly is unmapped from the sender at transfer,
   demonstrated by a fault on the sender's next access.
7. The §8 budgets are measured on the ADR-0040 platform against the §8
   benchmark, with the boundary-crossing and copy counts *counted*, not
   estimated.
