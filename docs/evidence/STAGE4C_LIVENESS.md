<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4C-0 — the liveness rule, with both of its halves

- Status: **evidence, 2026-09-05.** Not a stage closure and not a new decision:
  `SYSTEM_ABI_V1` §6 and ADR-0059 option D both state the rule with both halves,
  and this round is the second half becoming something the nucleus evaluates
  rather than something the stage made true by construction
- Decision carried: `docs/adr/0059-blocking-and-the-liveness-rule.md`
  (**Accepted**, Vladimir Tomashevskiy, 2026-08-19), §"Realisation"
- Prerequisite recorded by: ADR-0079 §12, which found the defect during Stage 4A
  and named it "a mandatory prerequisite to the first routed device interrupt"
- Gate: `source/host-tools/qemu-test/blocking.sh`, in preflight as *QEMU
  blocking and the liveness rule* (`qemu` profile, `full-only`)

## 1. The defect, exactly as it stood

`SYSTEM_ABI_V1` §6:

> when no context is runnable and some context is blocked, **and nothing routed
> can change that**, every block is cancelled at that instant

`nucleus/src/process.rs` implemented the first half:

```rust
None if !any_blocked() => return,
None => { /* fire the rule */ }
```

and said so in its own comment — "**Stage 4 must revisit this.** The rule is
'nothing runnable *and nothing routed can change that*', and the second half
stops being free the day a device interrupt can wake a driver."

It was **correct** for every stage up to and including 4B, and it was correct
for a reason that is a fact about those stages rather than about the rule:
ADR-0049 routes one interrupt, it is the timer, and the timer wakes nobody. So
"something is blocked" and "something is blocked that nothing can satisfy" were
the same predicate. A driver blocked on its own device interrupt, alone in the
system, would have been cancelled at the instant it blocked.

## 2. The audit: every blocking reason this system has

`SYSTEM_ABI_V1` §6 requires that blocking is always on a handle the process
holds, so the set of reasons is closed and enumerable. All four are classified,
and each is classified by what would have to happen for the wait to end:

| Reason | The operation that ends it | Who performs that operation | Wake source |
|---|---|---|---|
| `Waiting::Message(endpoint)` | `endpoint_send`, `endpoint_call` | another context | **peer** |
| `Waiting::Room(endpoint)` | `endpoint_receive` | another context | **peer** |
| `Waiting::Reply` | `endpoint_reply`, `endpoint_reply_receive` | another context | **peer** |
| `Waiting::ChildOf(instance)` | a child ending — `process_exit`, a fault, or `process_terminate` | another context, or the child itself | **peer** |

`ChildOf` is the one worth checking rather than assuming, because an ending is
not obviously an operation somebody performs. It is: a child ends by running, or
because a context that is running ends it, and an ending that has **already**
been recorded was handed to its waiter before the scheduler went looking for
something to run. So there is no state in which a pending ending is what a
blocked `ChildOf` is waiting for.

**Every reason is peer.** The census therefore returns zero routed sources on
every boot this system can currently produce, the rule fires in exactly the
states it fired in before, and the repair changes no behaviour. That is the
property this round is built to have: the classification is what changes, and
the first routed interrupt is then a new arm in one match rather than a rewrite
of the scheduler's termination condition.

## 3. What the nucleus decides from now

The classification is exhaustive and carries no wildcard, so a new blocking
reason cannot be added without the compiler demanding a liveness answer for it:

```rust
fn wake_source(&self) -> WakeSource {
    match self {
        Waiting::Nothing   => WakeSource::NotWaiting,
        Waiting::Message(_) => WakeSource::Peer,
        Waiting::Room(_)    => WakeSource::Peer,
        Waiting::Reply      => WakeSource::Peer,
        Waiting::ChildOf(_) => WakeSource::Peer,
    }
}
```

and "nothing is runnable" resolves into three states rather than two, because
§6 needs three and a boolean carries two:

```text
nothing blocked                            → Liveness::Finished
                                             the boot's work is over
blocked, and a live routed source exists   → Liveness::AwaitingHardware
                                             idle: halt until an interrupt
blocked, and no live routed source         → Liveness::Stalled
                                             the rule fires
```

`AwaitingHardware` **halts** (`sti; hlt`) rather than spinning or concluding.
Halting is the honest answer to "nothing to run, something to wait for", and the
`sti` is load-bearing twice over: a context that ended inside a system call
returns to the scheduler's loop with interrupts masked, where a bare `hlt` would
be a halt nothing could end; and the architecture's one-instruction `sti` shadow
is what stops an interrupt arriving between the two from being lost.

There is **no producer of `WakeSource::Routed` yet**, and this document says so
rather than implying otherwise. Stage 4C-1 is the first, and the value's whole
purpose before then is that the rule is written over the classification instead
of over "is anything blocked".

## 4. What is observable

`TOS.RUN.LIVENESS blocked= routed= verdict=` (`RUNTIME_OBSERVABILITY_V1` §7),
emitted at the instant the verdict is reached. It is a new event rather than a
new field on `TOS.RUN.BLOCK_CANCELLED`, because it is a statement about a
different subject: the cancellation is about one wait, and the census is about
the system.

`routed=0` beside `verdict=stalled` is the difference between a nucleus that
evaluated the second half of the rule and one that never asked — a distinction
no reader could otherwise make, because both produce the same cancellation.

From the gate's own run:

```text
TOS.RUN.IPC.POLLED status=-4
TOS.RUN.LIVENESS blocked=1 routed=0 verdict=stalled asserted_by=nucleus
TOS.RUN.BLOCK_CANCELLED process=0 operation=2 endpoint=0 reason=no-runnable-context asserted_by=nucleus
TOS.RUN.IPC.WAIT status=-5 attempt=1
TOS.RUN.LIVENESS blocked=1 routed=0 verdict=stalled asserted_by=nucleus
TOS.RUN.DEADLOCK asserted_by=nucleus
TOS.RUN.PROCESS_DEADLOCKED process=0 operation=2 endpoint=0 asserted_by=nucleus
TOS.RUN.PROCESS_RECLAIMED process=0 frames=14358 available=57352 tables_free=1494 plans_live=0
TOS.BOOTMODULE.FAIL stage=process
```

`blocking.sh` asserts both census lines exactly, asserts that no
`verdict=awaiting-hardware` appears in a stage that routes no device interrupt,
and keeps every assertion it already made.

## 5. The proofs this round can carry, and the ones it cannot

Stage 4C's brief asks for seven negative proofs. Four of them name an object
that does not exist until Stage 4C-1, and claiming them here would be claiming
evidence from a fixture:

| Proof | State |
|---|---|
| 6. ordinary deadlock with no future wake source is still detected | **gated** — `blocking.sh`, unchanged in outcome and stronger in what it asserts |
| 7. Stage 1–4B termination and liveness tests stay green | **gated** — the whole `qemu` profile |
| 4. process death removes its wait registration | **gated** — a dead slot is not `Blocked`, so it is not in the census; `blocking.sh` and `supervision.sh` both end blocked contexts |
| 1. no runnable process + live routed IRQ wait → the system does not terminate | **Stage 4C-1**: no wait of that class exists yet |
| 2. an interrupt wakes the intended blocked process | **Stage 4C-1** |
| 3. after route revocation a stranded wait does not keep the system live | **Stage 4C-1** |
| 5. device/assignment death removes or resolves dependent waits | **Stage 4C-1** |

## 6. The limitation this creates, named here rather than discovered later

Once any wait has a live routed source, a **peer** deadlock that coexists with
it is not diagnosed. A context blocked on an interrupt may, when that interrupt
arrives, send the message a deadlocked pair is waiting for, so cancelling them
while the route is live would be cancelling waits the system could still
satisfy.

This is the same class as the partial starvation ADR-0059 already named, and it
is bounded by the same kind of thing rather than by nothing: a routed source is
an **authority**, not an ambient condition. It ends with the process that holds
it, with the device assignment it descends from, and with any revocation its
launcher performs — and at that instant the waits depending on it resolve and
the ordinary rule applies again.

The rule does not degrade into "something is blocked, therefore the system is
live forever". It degrades into "a live routed capability exists, therefore the
system may still make progress", which is true, and which a supervisor can end.

## 7. What this round did not touch

No new ABI operation, no new status, no new right, no new object kind, no change
to `SYSTEM_ABI_V1` §6's meaning, and no change to any accepted decision. One
additive observability event, and one classification the compiler now enforces.
