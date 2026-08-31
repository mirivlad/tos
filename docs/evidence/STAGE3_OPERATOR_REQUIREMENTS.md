<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 operator requirements — supervisor, failure and the error journal

- Status: **recorded, not implemented.** These are requirements to design
  against when the supervisor and its service manifests are built, which is
  after Regions and the funded ABI. Nothing here is a decision about a
  mechanism that exists today, and nothing here authorises building one early
- Date recorded: 2026-09-01
- Source: the Project Architect, relaying an operator with long Unix/Linux
  administration experience. They describe expected *operational* behaviour —
  what an administrator must be able to see, conclude and do — rather than an
  implementation
- Related: ADR-0067 (a supervisor learns a service ended), ADR-0059 (the system
  could not continue), ADR-0055 (endowment), ADR-0076 (funding)

## A. Restart policy belongs to the service manifest

Not to the nucleus. There is to be **no nucleus-wide hard limit on attempts**:
the manifest states how many start or restart attempts a service gets, and the
supervisor obeys the manifest. A default of `restart_attempts = 5` is
acceptable if one is needed.

When the policy is exhausted the service enters a terminal broken state.

**Left open on purpose.** Whether "attempt" alone is enough to stop a
crash-loop, or whether a time window or a readiness threshold is also needed,
is *not* settled here. If the counting turns out to be insufficient it becomes
its own decision packet rather than something invented quietly while
implementing.

## B. `FAILED` is latched until an operator acts

The invariant, and the reason it is one:

```text
FAILED
  ── only an explicit operator action ──▶ STARTING
```

Once a service has exhausted its policy and been declared terminally failed,
**nothing gives the supervisor the right to try it again on its own**: not a
timer, not a configuration change, not the network coming back, not memory
becoming available, not a dependency recovering, and not the passage of time.
The operator fixes the cause and explicitly starts or resets-and-starts it. If
that explicit start succeeds, the supervisor resumes ordinary responsibility
for the service under its manifest policy.

The reason is evidence. After a terminal failure the operator may be preserving
artefacts, reading state, changing configuration, detaching dependencies or
investigating an outage. Autonomous recovery at that moment can destroy the
evidence or finish breaking what still works. Automation may keep **watching,
diagnosing and logging** a terminally failed service; it may not change its
state.

## C. A dependency failure is a different fact from a crash

If B cannot run because A is unavailable, the operator must see the causal
chain and not a fiction:

```text
B unavailable/failed
  reason = dependency A unavailable
```

B is allowed to look broken — the operator asked for that explicitly — but it
must be broken *for the stated reason*. A failure of this kind **must not spend
B's own launch or restart budget** when B was never started and never
demonstrated a fault of its own.

**Left open on purpose.** Whether a dependency-derived failed state is itself
terminal-latched, or clears when the dependency recovers, is a separate operator
semantics decision and is not settled here. What is settled now is narrower and
sufficient to design against: do not merge own failure with dependency failure,
keep the causality, and never attribute to B a crash that did not happen.

## D. A restart that worked is still something that happened

A successful automatic restart does not mean there was no failure. The record
must let an operator reconstruct it afterwards:

- the service;
- the source and process identity;
- the instance;
- why the previous instance ended;
- when;
- which restart attempt this was;
- the new instance;
- and the terminal failure, if the attempts ran out.

The operator will usually not need to intervene in a successful auto-restart.
They must be able to find out that it happened.

## E. One central error view

Significant messages must not have to be hunted for across dozens of
per-service logs. There is to be a single operator-visible error journal — call
it `ERR.log` for now; **neither the name nor a filesystem path is fixed here** —
carrying the system's consequential events:

- resource exhaustion;
- service terminal failure;
- dependency failure;
- a device disappearing;
- I/O errors;
- corruption;
- a forced read-only transition;
- capability or resource limits, where they affect operation;
- other `WARN`/`ERROR`/`FATAL` events an operator would act on.

Per-service and per-device history stays, for drilling down from an entry. The
representation may be structured internally, but a **human-readable textual
form is a required operator interface**, not a rendering somebody adds later.

Severities use the familiar ladder — `DEBUG`, `INFO`, `WARN`, `ERROR`,
`FATAL` — because inventing a new vocabulary here buys nothing.

## F. The operator-familiarity principle

Design guidance for the Stage 3 supervisor and especially for Stage 4:

> **Novel internal TOS mechanisms must not create novel operator intuition
> without a concrete benefit.**

TOS is not obliged to copy systemd, Linux kernel internals, `/var/log`, `errno`
or the Linux driver model, and should not copy them by default. But an
experienced Unix or Linux administrator must not have to relearn basic
cause-and-effect for ordinary failures. For:

- a device that disappeared;
- an I/O error;
- storage forced read-only;
- a corrupt object;
- a device rediscovered;
- resource exhaustion;
- an unavailable dependency;

the state, the diagnosis and the recovery must be predictable to that operator.

TOS's originality belongs in architecture, ownership and source identity — not
in making the system harder to run.
