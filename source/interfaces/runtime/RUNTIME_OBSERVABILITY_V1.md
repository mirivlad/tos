<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Runtime Observability v1 — the `TOS.RUN.*` contract

Status: **Accepted Tier 2 interface contract.**

Its authority is assigned by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`, and it
conforms to the Tier 0 invariants and to every accepted Tier 1 ADR, which take
precedence over anything written here. Accepted by ADR-0042 (Project
Architect-approved, 2026-08-12), which also makes `TOS.RUN.*` the delegated
namespace Boot ABI v1 section 7 admits between its own success identifiers.

**§9 is a Project Architect-approved amendment, Vladimir Tomashevskiy,
2026-09-03**, granted against closure commit `77970cb` as the Stage 3
operator-visible error-view semantics. What it accepts is stated in §9 itself
and summarised there under §9.7; the boundary of §9.6 is approved with it.

Producer: `source/crates/tos-pipeline` (`render::events`) and the component
driving it.
Consumers: the serial boot log, host test harnesses,
`source/host-tools/qemu-test/stage2-runtime.sh`.

## 1. Role

Boot ABI v1 (`interfaces/boot/BOOT_ABI_V1.md`) is the loader-to-nucleus
handoff: what the machine handed over and whether the capsule is what it
claims. It says nothing about *running* anything, because at the time it was
fixed nothing ran.

This contract covers the other question: what the TOS Core reference runtime
did with the canonical source it was given. It is a separate interface with a
separate vocabulary, because the two answer to different components and change
for different reasons. Boot ABI v1 is unchanged by this document.

## 2. What an event is

One line, terminated by CRLF on a serial transport, beginning with a stable
identifier matching `^TOS\.RUN\.[A-Z0-9_]+`, followed by space-separated
`key=value` fields. A value never contains whitespace: the producer substitutes
`_` for any whitespace or control character in a value that can carry free text
(a detail string, a rendered value), so a field can neither invent a field nor
split an event. Codes, digests, identifiers and numbers are never altered.

Identifiers and the fields listed as required below are stable. An
implementation MAY append further `key=value` fields after the required ones;
it MUST NOT remove one, reorder the required ones, or change what one means.

## 3. Progress

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.BEGIN` | `path=` `bytes=` `entry=` `grant_base=0x<hex>` `grant_length=` `grant_version=`, and `modules=` when the run is over a source set | A run is starting over the named source with the named memory grant. |
| `TOS.RUN.STAGE` | `name=<read\|parse\|check\|resolve\|lower\|verify\|execute>` | The named stage is being **entered**. |

`modules` is an appended field under section 2's extension rule: `path` names
the entry module, and a run that resolved a set executed more than that. A log
showing only the entry would understate what ran.

`TOS.RUN.STAGE` is emitted before the stage runs, not after. A stage that never
returns is then named by the last event in the log, which is the only way a
hang or a trap inside a stage identifies itself from outside.

The stages appear in exactly this order, and the order is the reference path:
transport validity, grammar, checking, module-name-to-path agreement, lowering
to `tos-ir/v1`, independent verification, execution. A log in which they appear
in any other order, or in which one is missing before a later one, is not a
conforming run.

## 4. Outcome

Exactly one outcome event ends a run, and it is always the last of the run's
events.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.COMPLETED` | `value=` | The entry function returned. `value` is the rendered result. |
| `TOS.RUN.TRAP` | `code=` `at=` `detail=` | A defined dynamic failure ended the run. `at` is `path:line:column-line:column`, or `<unmapped>` when the IR carried no source map entry. |
| `TOS.RUN.REFUSED` | `stage=` and stage-specific fields | A stage refused to hand its output on. |

`TOS.RUN.REFUSED` carries the stage that refused and its reason:

| `stage=` | Further required fields |
|---|---|
| `read` | `code=` `byte=`, and `path=` when the run was over a source set |
| `parse`, `check`, `resolve` | `count=`, preceded by `count` × `TOS.RUN.DIAGNOSTIC` |
| `lower` | `construct=` `bytes=<start>..<end>` |
| `verify` | `code=` `at=` `detail=` |
| `execute` | `reason=` |

`TOS.RUN.DIAGNOSTIC` carries one frontend diagnostic in the docs/41 section 7
model: `<CODE> severity= stage= bytes=<start>..<end> at=<line>:<column>`, then
the module identity when the diagnostic carries one, then the diagnostic's
structured fields in their own order. The byte span is the normative locator;
line and column are derived from it.

`path` on a `read` refusal is an appended field under section 2's extension
rule, and it exists because a set has more than one unit: a byte offset with no
unit names nothing. A transport refusal never reaches a diagnostic, so nothing
else in the event can say which unit was refused.

A refusal by `parse`, `check` or `resolve` is a statement about the *program*.
A refusal by `lower` is a statement about the *lowerer* — the source is valid
and checked and this implementation cannot yet represent one of its constructs.
A refusal by `verify` is a statement about the *IR the frontend emitted*, which
is why it is never merged with the frontend's own diagnostics.

## 5. Verification and accounting

Emitted before the outcome on a completed run.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.VERIFIED` | `module=` `digest=sha256:<64 hex>` `verifier=` | The independent verifier issued a receipt for this exact module. |
| `TOS.RUN.ACCOUNTING` | `fuel=` `depth=` `tasks=` `allocation=` `cleanup=` `workers=`, each `used/limit` | What the run consumed against what the module declared. `shared=` and `sync=` are appended in the same form. |

`digest` is the complete `tos-ir/v1` module digest, and the engine ran the
module that digest names — a receipt for another module is refused before any
instruction executes (docs/43 section 5). A consumer that wants to know whether
verification really happened checks this event, not the absence of a refusal.

Every `used` in `TOS.RUN.ACCOUNTING` is bounded by its `limit`; a run that
exceeded one would have been stopped before the effect, so a log showing
otherwise is a defect and not a permitted state.

## 6. Implementation resources

Emitted after the outcome, by the component that supplied the memory.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.MEMORY` | `granted=` `peak=` `committed=` `blocks=` `free=` | The arena the run needed, against the region it was granted. |
| `TOS.RUN.STACK` | `used=` `capacity=` | Stack the run actually used, measured, against the region it ran on. |
| `TOS.RUN.TICKS` | `begin=` `end=` `spin_begin=` `spin_end=` | The monotonic tick the runtime read before and after the run. It counts timer interrupts (ADR-0049) and is not a duration: this contract carries no wall-clock time and no trusted time source. `spin_begin` and `spin_end` bracket a loop the runtime runs **without making any system call**, so a tick larger at its end was advanced by an interrupt taken while the process was executing its own instructions — which is a different claim from a tick that moved between two calls, and the stronger one. Absent when the system offers no tick. |
| `TOS.RUN.UNSTARTABLE` | `reason=` | The runtime could not be started at all. No stage ran. |

### Authority, from the holder's side

Emitted by the runtime, into its report region, and therefore subject to the
attribution limit stated at the end of §7: with more than one process these
interleave and carry no `process=`. That is not a gap, because none of them is
a claim *about* a process — each is the answer the nucleus gave to a call, and
the process is only reporting what it was told.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.CAPABILITY` | `held=` and, when non-zero, `handle=0x<hex>` `object=` `rights=` | What the process found in its launch record: how many capabilities it holds and what the first names. |
| `TOS.RUN.CAPABILITY.PROBE` | `out_of_range=` `in_range_refused=` `guessed=` | What guessing a handle is worth (`CAPABILITY_V1` §7.2). An index past the table refuses with `E_BAD_HANDLE`, one inside it with `E_NO_CAPABILITY`, and `guessed=` is how many guesses produced a usable capability — zero, or the table is forgeable. |
| `TOS.RUN.CAPABILITY.ATTENUATED` | `status=` `asked=` `widened_half=` | An attenuation and the check that it narrowed. `asked=all` means every right was requested; `widened_half=` is the status of an operation the original capability could not perform, which must still refuse. |
| `TOS.RUN.CAPABILITY.RELEASED` | `status=` `reuse=` | A release, and the status of naming the same handle afterwards (`CAPABILITY_V1` §7.3). |
| `TOS.RUN.IPC.SENT` | `bytes=` `status=` `oversize=` `other_half=` | A message sent. `oversize=` is the status of a payload one byte past the inline bound, which `IPC_V1` §9.1 requires be refused rather than truncated; `other_half=` is the status of the operation this handle's rights do not permit. |
| `TOS.RUN.IPC.RECEIVED` | `bytes=` `text=` | A message taken from an endpoint, and its payload. `text=` carries no spaces: a value with one would be two fields to a reader that splits on them. |
| `TOS.RUN.IPC.POLLED` | `status=` | The answer to a receive that asked not to wait. |
| `TOS.RUN.IPC.WAIT` | `status=` `attempt=` | A blocking receive that did not return a message, and which attempt it was. A process reporting this has been resumed, which is the only way it could report anything. |
| `TOS.RUN.DEPUTY.REFUSED` | `request=` `reason=` `bytes=` | A process acting for another refused a request that named its object by value rather than by capability. A number is not a handle, and using one would be acting on one's own authority at a stranger's direction. |
| `TOS.RUN.DEPUTY.ACTED` | `request=` `for_client=` `on_own_account=` | The same operation performed with the capability a client supplied and with the actor's own. The two statuses differing is `CAPABILITY_V1` §7.6: authority attaches to what the actor was given for the work, not to the actor. |
| `TOS.RUN.DEPUTY.ASKED` | `named_by_value=` `with_capability=` | A client's own view of the two requests it made. |
| `TOS.RUN.IPC.CALLED` | `status=` `bytes=` and, on success, `answer=` | A request whose answer arrived inside the call that asked it (`IPC_V1` §4). |
| `TOS.RUN.IPC.REPLIED` | `status=` `handle=0x<hex>` `again=` | A call answered with the capability that came with it. `again=` is the status of using that capability a second time, which single use makes a refusal. |
| `TOS.RUN.IPC.DELEGATED` | `handle=0x<hex>` `send=` | A capability that arrived with a message, as the receiver's own handle, and the status of using it for something the receiver's own capability was refused. |
| `TOS.RUN.IPC.RIGHTS` | `other_half=` | The status of the half of an endpoint this holder's rights do not include (`IPC_V1` §2). |
| `TOS.RUN.CAPABILITY.TYPE` | `operation=` `status=` | The status of an operation whose object this handle is not — the index and generation are right and the answer is still a refusal (`SYSTEM_ABI_V1` §8.1). |
| `TOS.RUN.PROCESS.CREATED` | `status=` `child=0x<hex>` | A process created a process on authority it holds, and the handle it received over what it made. |
| `TOS.RUN.PROCESS.ENDED` | `status=` `again=` | It ended that child. `again=` is the status of naming the same handle afterwards: a capability's lifetime is bounded by its object (`CAPABILITY_V1` §3), so it does not survive to name the slot's next occupant. |
| `TOS.RUN.PROCESS.REFUSED` | `reason=` `status=` | A creation the nucleus refused, and why.

A status in any of these is one of `SYSTEM_ABI_V1` §4's, by its number. They are
reported as numbers rather than names because the number is what crossed the
boundary, and a name would be this image's reading of it.

These are **implementation** figures and are never a statement about the module.
A module's `resource [allocation: ...]` is its own declared budget, enforced by
the engine and reported in `TOS.RUN.ACCOUNTING`; `TOS.RUN.MEMORY` is what the
implementation running it required. Exhausting one is not the other (ADR-0041).

`peak` is the highest address the arena was ever carried to, so it includes
block metadata, grain rounding, remainders too small to split off, and holes
below the frontier. It never falls when memory is freed: a bound must err
upward. `committed` is the live figure in whole blocks.

`TOS.RUN.STACK used` is measured, not estimated: the unused stack is painted
with an address-derived pattern before the run and read back after it.

## 7. Processes

A second producer emits under this namespace: the nucleus's process substrate.
`PROCESS_IDENTITY_V1` §6 delegates it here rather than to a namespace of its
own, because two vocabularies describing one system eventually disagree.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.PROCESS_BEGIN` | `process=` `module=` `runtime_engine=sha256:<64 hex>` `system_commit=` `asserted_by=launcher` | A process was built over the named module by the named runtime image, and occupies the named slot. |
| `TOS.RUN.PROCESS_EXIT` | `process=` `asserted_by=nucleus` `self_reported_status=` `ticks=` `quanta=` `first_tick=` `last_tick=` | The process ended by saying so (`process_exit`, ADR-0054). The four counts are the nucleus's, because a process cannot observe how long it was off the processor: `ticks` is the timer interrupts charged to **this** process, `quanta` how many times it was given the processor, and `first_tick`/`last_tick` the first and last tick it ran at. |
| `TOS.RUN.PROCESS_FAULT` | `process=` `vector=` `error=0x<hex>` `rip=0x<hex>` `cr2=` `cpl=` | The process took a fault and ended. The system did not, and neither did its peers. |
| `TOS.RUN.PROCESS_RECLAIMED` | `process=` `frames=` `available=` | What the pool took back when the named process ended, and what it holds now. |
| `TOS.RUN.PROCESS_TERMINATED` | `process=` `by=` `ticks=` `quanta=` `asserted_by=nucleus` | The process was ended by another process holding authority over it (`process_terminate`). `by=` is that process. The whole event is the nucleus's assertion: nothing in it is anyone's claim about themselves. |
| `TOS.RUN.BLOCK_CANCELLED` | `process=` `operation=` `endpoint=` `reason=` `asserted_by=nucleus` | A wait was cancelled by the nucleus. `reason=no-runnable-context` is ADR-0059's liveness rule: nothing was runnable, something was waiting, and nothing routed could change that. |
| `TOS.RUN.LIVENESS` | `blocked=` `routed=` `verdict=` `asserted_by=nucleus` | The census ADR-0059's rule was decided from, at an instant when nothing was runnable. `blocked=` is how many contexts were waiting and `routed=` how many of those a **live routed source** could still wake; `verdict=` is `stalled` or `awaiting-hardware`. Emitted for every `stalled` verdict and on entry to `awaiting-hardware`. |
| `TOS.RUN.DEADLOCK` | `asserted_by=nucleus` | The liveness rule fired twice with no message delivered in between. The contexts are not waiting for something that has not happened yet. |
| `TOS.RUN.PROCESS_DEADLOCKED` | `process=` `operation=` `endpoint=` `asserted_by=nucleus` | A context ended because the system could not continue. Not a fault, not its own claim and not another process's decision — a statement about the arrangement. |
| `TOS.RUN.PROCESS_ENDOWED` | `process=` `capabilities=` `policy=` `asserted_by=launcher` | What authority the process was given, before it ran its first instruction (ADR-0055). `policy=` names where the decision came from — `launcher-constant` until `/system/policy/` exists (ADR-0051 §3). |

`TOS.RUN.PROCESS_ENDOWED` is emitted for every process, including one endowed
with nothing, and `capabilities=0` is the commonest value. It is not omitted in
that case and must not be: a grant nobody can attribute is ambient authority
with a handle in front of it (`CAPABILITY_V1` §3), and an endowment nobody
announced is indistinguishable from one nobody decided. The event says the
launcher decided, and `policy=` says what it decided from.

Two fields carry their asserter in their name, and that is not decoration.
`asserted_by=nucleus` on an exit says the *fact* of the exit is the nucleus's;
`self_reported_status` says the number beside it is the process's claim about
its own work. A reader must never have to guess which kind of claim it is
holding (`PROCESS_IDENTITY_V1` §2), and merging the two would make the guess
necessary.

**`TOS.RUN.LIVENESS` carries the numbers a verdict about the whole system was
reached from, and that is why it is separate from the cancellation it usually
precedes.** `TOS.RUN.BLOCK_CANCELLED` says what happened to one wait;
`TOS.RUN.LIVENESS` says what the nucleus found when it looked at all of them.
`routed=0` is the observable evidence that the second half of ADR-0059's rule —
"and nothing routed can change that" — was evaluated and came back empty, which
a reader cannot otherwise distinguish from a nucleus that never asked. On a
system where nothing is blocked the event is absent: that is not a finding but
the ordinary end of the scheduler's loop, and the boot's own verdict already
says it.

`system_commit=absent` is the true value for a capsule-launched Stage 3 process:
Stage 3 reads no repository, and writing a commit the system never read is the
failure Stage 1 was built to prevent.

`process=` is the slot the nucleus's process table holds that process in. It is
a name for **this boot's** process, not an identity that survives one, and a
slot is reused once the process that had it has ended and its memory has gone
back. It appears on every nucleus-asserted process event so that a reader with
more than one process on the log can tell which one each statement is about.

The four counts on an exit are what makes preemption checkable rather than
claimed. `quanta` greater than one means the processor was taken from that
process and handed back, which is not something a process can arrange for
itself; and two processes whose `[first_tick, last_tick]` intervals overlap ran
interleaved, because two that ran one after the other cannot produce overlapping
intervals. Both are ADR-0049 §4 evidence, and both are the nucleus's to assert.

The `TOS.RUN.*` events of §3–§6 are the *runtime's* — a process cannot reach a
serial port, so it writes them into the region its launch record names and the
nucleus relays them unchanged. Relaying is not authorship: the events say what
the runtime did, and the nucleus adds nothing to them.

**With more than one process, those events interleave, and they carry no
`process=`.** Each process writes into its own report region and the nucleus
drains whichever process just entered the edge, so the relative order of two
processes' lines on the transport is the order the scheduler produced. A line is
never split: a runtime publishes a line by advancing the region's `written`
count after the bytes are in it, so a process interrupted mid-write has written
nothing yet. Attributing a §3–§6 event to a process is therefore not something a
reader can do from the transport alone — and nothing in this contract asks them
to, because every claim that needs an owner is a nucleus event above, which
carries one.

## 8. Relationship to the boot log

When a runtime is driven from the nucleus, these events appear on the same
serial transport as the Boot ABI v1 events, between `TOS.IDENTITY` and
`TOS.HALT`. Boot ABI v1's own identifiers keep their relative order and their
meanings; nothing in this contract changes, removes or reinterprets any of them.

ADR-0042 settles the two questions this raised. Boot ABI v1's success
identifiers are a required **ordered subsequence** of the transport, and
identifiers of an accepted versioned interface contract — this one — MAY appear
between them. That is a delegation to an accepted contract, not an opening for
arbitrary unknown `TOS.*` namespaces: an identifier belonging to no accepted
contract is still unknown, and the Boot ABI rule for one is unchanged.

The Boot ABI terminal result remains authoritative for whether the boot
succeeded. Nothing in this contract reports a boot verdict; it reports what the
runtime did with the source it was given.

When the canonical boot module does not complete, the nucleus emits
`TOS.BOOTMODULE.FAIL` and halts with `RESULT_BOOT_MODULE_FAILED` (`0x25`),
whose exact condition Boot ABI v1 section 2 states. Which stage refused, and
why, is in the `TOS.RUN.*` events above — the result code says the boot module
failed, not how.

## 9. Severity, and the operator-visible important-error view

Stage 3 requires one operator-visible view of the events that matter. This
section says what that view is and how a reader selects it. It adds no
transport, no second channel and no storage: every question below is answered
by something the system already has.

### 9.1 The view is the transport

**The single operator-visible view is the diagnostic transport itself**, in the
order it was produced. It is already the one place every component's account
converges — the loader's, the nucleus's, the runtime edge's, and any process's
own journal — and a second place would be a second truth about what happened.

The *important-error* view is that transport filtered to severity `WARN` and
above. It is a **selection**, not a copy: nothing is duplicated, nothing is
summarised, and an event appears once.

### 9.2 Severity is a property of the event kind

| Severity | What it means |
|---|---|
| `DEBUG` | detail a component emits for its own diagnosis. **No identifier of this contract is `DEBUG`.** |
| `INFO` | the system did what it was asked. The default. |
| `WARN` | something was refused, cancelled or released, and the system carried on **as designed**. An operator should know; nothing is broken. |
| `ERROR` | a component's work did not succeed. The boot may continue; that component's job did not. |
| `FATAL` | the boot did not do what it was asked and stops. |

**Severity is declared per identifier, not chosen per occurrence.** What an
event kind means does not vary between two of them, so an emitter has no
severity to decide and no field to fill; a reader applies the table below. This
is also why adding severity costs no emitter a line of code and no gate a
change: it is a statement *about* a vocabulary that already exists.

Everything this contract declares is `INFO` unless listed here. Boot ABI v1's
own failure vocabulary (§7 "Failure vocabulary and extension rule") is `FATAL`
in its entirety, by that contract rather than by this one.

| Identifier | Severity | Why |
|---|---|---|
| `TOS.NUCLEUS.INVARIANT` | `FATAL` | the nucleus found its own accounting wrong and will not go on |
| `TOS.RUN.UNSTARTABLE` | `FATAL` | no stage ran; there is nothing to report about |
| `TOS.RUN.PROCESS_FAULT` | `ERROR` | a process touched memory no capability of its own authorised |
| `TOS.RUN.PROCESS_DEADLOCKED` | `ERROR` | a process is waiting for something that cannot happen |
| `TOS.RUN.DEADLOCK` | `ERROR` | nothing is runnable and something is waiting |
| `TOS.RUN.BUNDLE.REFUSED` | `ERROR` | a target refused the artifact it was created from |
| `TOS.RUN.PROCESS_REFUSED` | `WARN` | a creation could not be funded or had no slot — an ordinary bound, named |
| `TOS.RUN.BLOCK_CANCELLED` | `WARN` | a wait was cancelled by the liveness rule (ADR-0059) |
| `TOS.RUN.WAIT_CANCELLED` | `WARN` | as above, for a lifecycle wait |
| `TOS.RUN.NOTICE_RELEASED` | `WARN` | an ending nobody was left to collect was released rather than kept |

### 9.3 A process's own journal reaches the same view

A process writes its own records as `string` values of
`SYSTEM_INTERFACE_V1`'s `endpoint_send_text`, which the runtime edge renders on
this transport as the `said=` field of `TOS.RUN.INTERFACE`. That is the only way
a TOS Core module can put text into the world in its own words, and it is
already how the Stage 3 supervisor's journal is written.

**A record names its own severity as its first dotted segment:**

```text
said=<severity>.<producer>.<kind>.<what>
     warn.supervisor.state.blocked
     error.supervisor.policy.budget-exhausted
     info.supervisor.action.create
```

The severity is the record's, because a process is the only thing that knows
what its own decisions mean — this contract cannot enumerate the vocabulary of
every service that will ever run, and a table that tried would be a contract
that has to change whenever a service does. What this contract fixes is the
*form*: the first segment is one of the five names in §9.2, lower case, and a
record whose first segment is not one of them is `INFO`.

The remaining segments are the producer's own. Stage 3's supervisor uses
`<producer>.<kind>.<what>` with `kind` one of `observed`, `inferred`, `policy`,
`action`, `result` or `state`, which keeps apart what the nucleus asserted, what
the supervisor concluded, what its policy decided, what it attempted and what
came back.

### 9.4 How an operator reads it

`scripts/tos-journal.py` renders the important-error view from a captured
transport, in human-readable text: severity, the component that produced it, the
event, and its detail. It is a **reader**, not a component: it holds no state,
runs on a host, and can be replaced by `grep` without losing anything, because
the selection rule is one segment of a name.

```text
$ python3 scripts/tos-journal.py boot/serial.log
WARN   supervisor  state.blocked                 system/boot/worker.tos
ERROR  supervisor  policy.budget-exhausted       system/boot/worker.tos
WARN   nucleus     TOS.RUN.PROCESS_REFUSED       reason=no-funding wanted=58806272
```

### 9.5 Bounds

Every bound is one an accepted contract already fixed, and this section adds
none:

- a process's journal record is an inline IPC payload, so `IPC_V1` §3 bounds it
  at **256 bytes**; a longer one is refused before the call is made;
- an endpoint queue holds **four** messages (`IPC_V1` §3), so a producer that
  does not drain its own sink is refused rather than growing one;
- a process's report region is a fixed part of its charged footprint
  (ADR-0076 §3), drained by the nucleus at each system call;
- the transport itself is a stream, not a store.

### 9.6 What Stage 3 does not decide

> **Approved as a boundary, not as a final answer** (Project Architect,
> 2026-09-03). Persistence, rollover, archival, retention, filesystem location
> and cross-boot journal recovery are not Stage 3 closure requirements, and this
> approval neither chooses the eventual mechanism nor assigns it to a particular
> later stage. It states only that those questions do not block Stage 3.
>
> **It does not follow that losing all diagnostic history across a real
> production reboot is an acceptable final TOS operator experience.** That
> remains a future design obligation.


**Persistence, rollover, archival, cross-boot recovery, retention and filesystem
location are not decided here and are not implemented.** No accepted contract
decides them, and a Stage 3 view that invented one would be a Stage 4
observability design arriving without a decision. What Stage 3 requires and this
provides is that the consequential events exist, are produced by the component
whose statement they are, converge on one transport in one order, carry a
severity a reader can select by, and are bounded.

### 9.7 What was approved, as the ruling states it

The Stage 3 operator-visible error-view semantics, accepted 2026-09-03:

- the diagnostic transport is the single converged operator-visible view;
- the important-error view is a **selection** of that transport, not a
  duplicated second log;
- `WARN`, `ERROR` and `FATAL` form the important-error selection;
- the severity of a contract-defined event is fixed **per event kind**;
- process-owned journal records carry their own severity in the textual form
  §9.3 fixes;
- all components converge on **one ordered transport**;
- `scripts/tos-journal.py` is a **reader** of that view — not a production
  subsystem and not a second source of truth;
- the human-readable textual operator interface is part of the Stage 3 result;
- and the existing IPC, report-region and transport bounds remain the bounds:
  no new unbounded queue and no new store is introduced.
