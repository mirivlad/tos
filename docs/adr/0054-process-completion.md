<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0054: How a process says it is finished

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-17
- Decision level: 2 — it adds an operation to an accepted interface contract, or
  decides that no operation is added and completion is observed some other way
- Project Architect approval:

## Context

`SYSTEM_ABI_V1` §5 lists eleven operations. A process ends in exactly two ways
under the accepted contracts:

- `process_terminate`, which requires *a process-authority capability for that
  process* — authority a process does not hold over itself, and which in Stage 3
  is held by a supervisor;
- a fault, which ADR-0049 §3 says terminates the process and leaves the system
  running.

There is no third way. A process that runs to the end of its work and has
nothing more to do cannot say so, and cannot report what it produced.

This was predicted before the code reached it — the Phase 2 plan records it as
"a boundary this phase is expected to reach" — and checking the four accepted
contracts confirms it: `SYSTEM_ABI_V1`, `CAPABILITY_V1`, `IPC_V1` and
`PROCESS_IDENTITY_V1` contain no self-exit, no exit status and no completion
event.

It becomes blocking at the first process. `/system/boot/init.tos` returns
`i32:240` today, at CPL 0, and the boot log reports it. Once init is a process,
that value is computed on the far side of the isolation boundary, and there is
no contractual way for it to come back — nor for the boot to know that init
finished rather than hung.

## What must not be done about it

An operation invented at the edge because the boot needed one. The whole of
Stage 3's authority story is that an operation reachable without a capability is
a design defect (`SYSTEM_ABI_V1` §5), and "except this one, which we needed"
is how that story ends.

Equally: reporting completion by making the process fault deliberately. A fault
is evidence of something going wrong, and a system that manufactures one to mean
"finished" has destroyed the only signal it had.

## Options

### A — a self-only `process_exit` operation

A twelfth operation, in the same class as `context_yield` and `time_monotonic`:
*self only*, requiring no capability because it can be applied to nothing but
the caller. It takes a status value, does not return, and the nucleus emits the
completion in the audit record and makes it available to whoever holds process
authority over the process.

Consistent with §5's structure — the self-only class already exists and is
already justified there — and with the rule that authority is a handle: exiting
requires no authority over anyone, and confers none.

Costs: an accepted contract gains an operation. `SYSTEM_ABI_V1` §7 permits that
under a minor version, with the number assigned once and never reused. The
status value's domain has to be fixed (a value a process chooses is a claim by
the process, and `PROCESS_IDENTITY_V1` §2 requires that self-reports are
labelled as such and are never the audit record — so the *fact* of exit is the
nucleus's assertion, the *status* is the process's claim).

### B — completion is an IPC event to a supervisor

No new operation. A process ends by making a call on an endpoint its supervisor
gave it, and the supervisor — holding process authority — terminates it.
Completion becomes an ordinary message, and the audit record is the
supervisor's.

Consistent with "if an operation could be a service, it is a service"
(`SYSTEM_ABI_V1` §2), and it keeps the ABI at eleven operations.

Costs: it requires IPC, endpoints and capability transfer to exist — none of
which Phase 2 has — and it requires a supervisor to exist before the first
process can finish. The first process in the system has no supervisor by
construction, so B answers every case except the one that is blocking now.
It also makes an unsupervised process unable to exit at all, which turns "no
supervisor" into "runs forever".

### C — the first process does not finish

Init does not return. It becomes the supervisor, and the boot's terminal result
is decided by what it launches rather than by init returning a value. The
question is deferred to the stage that has services to supervise.

Costs: it changes what the Stage 2 evidence means. `value=i32:240` is the result
of a module that returns; a module that never returns cannot report it, so
either the boot evidence changes shape at exactly the moment the execution
boundary moves — which is the one thing Phase 2's constraints forbid, because
then no one can say which change did what — or init keeps returning and the
question comes back unanswered.

## Recommendation

**A**, with the status value labelled as a self-report.

It is the smallest addition that makes the ordinary case expressible, it fits
the class §5 already has, and it does not require a supervisor to exist before a
process can end. B is right about services and wrong about bootstrap: the first
process cannot ask a supervisor that does not exist. C is not a decision but a
postponement, and it pays for the postponement in the one currency this phase
cannot spend — the comparability of the Stage 2 result across the boundary move.

If A is accepted, the operation is number 12, `process_exit`, self-only, taking
a status value, not returning; the nucleus asserts *that* the process exited and
*when*, the process claims *with what*, and the two are never merged.

## Boundary

Phase 2 Task 5 (the first process) cannot report its result until this is
decided. The launch itself — address space, grant, entry at CPL 3 — does not
depend on it, and neither does ADR-0053.
