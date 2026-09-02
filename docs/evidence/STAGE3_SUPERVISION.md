<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 — Section H: a supervisor written in TOS Core

What this records: the Section H work named as unfinished in
`STAGE3_LAUNCH_PLANS.md` §8, completed. The textual supervisor and
`/system/policy/`, the restart-window policy, `BLOCKED`, the terminal `FAILED`
latch, the journal, T1 against Capsule v1, and `CreatedProcess`/`ChildEnding` as
schema-declared record types.

## 1. What is textual TOS Core, and what is host-side evidence

**Textual, in the capsule, and canonical:**

| Module | What it decides |
|---|---|
| `/system/policy/services.tos` | which services exist, the module each runs, the arena each gets, how many own failures each may have, how wide its window is, what it depends on, and how many supervision rounds to run |
| `/system/boot/init.tos` | the state machine: which state each service is in, when to start one, what an ending means, when the budget is exhausted, when a dependency prevents a launch, and what to write in the journal |
| `/system/boot/worker.tos` | the thing supervised. Asks for nothing and returns |

**Host-side, and only ever observing:** `tests/integration/tests/supervision.rs`
answers operations and decides *when a child ends and at which tick*, which is
the nucleus's half. It contains no policy, no state, and no decision — it cannot,
because the numbers it would need are in the policy module and the transitions
are in the supervisor. `host-tools/qemu-test/supervision.sh` reads the boot log.

Neither could produce the run: the supervisor's return value is composed from
services created, services latched and services blocked, and the journal is a
sequence the supervisor writes.

## 2. The state machine

Four states. The difference between them is the point, and each is journalled as
its own decision.

```text
                    ┌───────────────────────────────────────┐
                    │                                       │
   ┌──────────┐  dependency not running   ┌─────────┐       │
   │STARTABLE ├──────────────────────────►│ BLOCKED │       │
   │          │◄──────────────────────────┤         │       │
   └────┬─────┘  dependency running again └─────────┘       │
        │                                                   │
        │ create                                            │ ending, and
        ▼                                                   │ budget left
   ┌──────────┐         ending observed                      │
   │ RUNNING  ├───────────────────────────────────────────────┘
   └────┬─────┘
        │ ending observed, and the budget is now full
        ▼
   ┌──────────┐
   │  FAILED  │  terminal. Never reconsidered, and nothing leaves it
   └──────────┘
```

- **STARTABLE** — no instance running and nothing preventing one.
- **RUNNING** — an instance exists and has not been observed to end.
- **BLOCKED** — no instance running **and** a required dependency prevents a
  fair launch. It is reconsidered every round, because it is a statement about
  *now*; it consumes no restart budget, because nothing about that service
  failed.
- **FAILED** — the restart policy is exhausted. It is not reconsidered, and the
  question that would have started it is answered with the latch.

## 3. The restart window, and its boundary

Failure **density**, not a running total. On each own failure:

1. drop every recorded failure older than the window *relative to this ending's
   tick*;
2. record this one;
3. if what remains fills the budget, latch `FAILED`; otherwise restart.

Old failures expire lazily, when another one happens. The clock is
`ChildEnding.ended_tick`, which ADR-0067 makes boot-monotonic — the supervisor
needs no clock of its own and asks for none.

**The boundary is proved at the exact tick, in both directions.** Service 1's
window is `1 000 000`:

| Ticks scripted | Gap between its two failures | Outcome |
|---|---:|---|
| `[0, 1, 2, 1000000]` | exactly `1 000 000` | the older is **out**; nothing latches; the run returns `1071` |
| `[0, 1, 2, 999999]` | `999 999` | the older is **in**; the budget fills; the run returns `1161` |

Nothing else about the two runs differs. An implementation writing `<=` instead
of `<` fails exactly there and nowhere else.

The other direction is proved too: service 2's window is one tick and every gap
is larger, so ten endings never accumulate — while the two wide-window services
have both latched. That is the test that fails if failures are *counted* rather
than *dated*.

## 4. `BLOCKED`, and what it is not

Service 0 requires service 2 and is considered **before** it each round. So in
the first round its dependency is not running and it is `BLOCKED` — and in the
next round it is running, and service 0 starts. That pair is the whole meaning:

- not **ended**: it never ran, and no failure is inferred about it;
- not **failed**: its budget is untouched, and it starts as soon as it can;
- not **skipped**: it is considered every round, and the decision is recorded
  every time;
- not **sleeping**: nothing about time makes it startable, only its dependency.

Once its dependency latches `FAILED`, service 0 goes on being blocked, because
the dependency is not running and never will be. **Terminal dominates
dependency recovery** in the other direction too: after service 0 and 1 latch,
service 2 goes on being restarted — and neither latch is undone by it.

The Architect's rule that a **running** dependent is not terminated when its
dependency becomes unavailable holds by construction: nothing in the machine
terminates anything.

## 5. `FAILED` latches

A latched service is not reconsidered. The round that would have started it
records `supervisor.policy.latched-no-start` instead — a decision, not silence,
so the latch is observable rather than inferred from an absence.

The QEMU boot shows the discriminating fact: after the last latch there are
still `result.created` records, for the service that has not latched. A latch is
one service's and not the run's, and nothing in the machine can leave it. There
is no reset, no administrative escape hatch, and no implicit retry — operator
action would be the only way out, and Stage 3 does not decide what that is.

## 6. The journal

Five kinds of record, kept apart on purpose:

| Kind | Example | Whose statement |
|---|---|---|
| observed | `supervisor.observed.ending` | a fact the nucleus asserted, received |
| inferred | `supervisor.inferred.own-failure` | what the supervisor concluded from it |
| policy | `supervisor.policy.budget-exhausted` | what the policy decided about that |
| action | `supervisor.action.create` | what it attempted |
| result | `supervisor.result.created` | what came back |

Each decision is followed by the module path it is about, which the policy
supplies. The gate checks the order — an ending before anything inferred from
it, an attempt before its result — because a journal that recorded a result
before the attempt would be describing a system that answers before it is asked.

**It is the supervisor's, not the host's.** The record is composed in TOS Core
and sent with `endpoint_send_text`, a second schema row over `SYSTEM_ABI_V1`
operation 1 whose payload is declared as a `string` rather than as a length over
bytes the module cannot write. It is the only way a TOS Core module can put text
into the world in its own words. The runtime edge renders it; it does not decide
what it says.

**Bounded, by the model that already exists.** `IPC_V1` §3 bounds an inline
payload at 256 bytes and a queue at four messages. A record past the bound is
refused before the call is made; the supervisor drains its own endpoint after
every record, because a journal that filled its sink would stop being one. No
new resource, no new bound, and nothing persisted.

**What Stage 3 does not decide, and this does not invent.** Who *consumes* an
operator journal — persistence, rollover, truncation, recovery, and a central
important-event view across services — is not settled by any accepted contract.
What is implemented is the minimum Stage 3 acceptance needs: the decisions
exist, they are the supervisor's, they are ordered, and they are bounded. A
Stage 4 logging design is not started here.

**A limitation, stated rather than worked around.** TOS Core V1 has no string
formatting, so a journal record is a literal the supervisor selects. A service is
therefore identified by the module path the policy supplies, and *instance*
identities appear on the nucleus's own lines rather than in the journal text.
That division is the right one — an instance identity is a fact only ring 0 can
assert — but a record that carried structured values from TOS Core would need
either formatting in the language or a value-carrying journal operation, and
neither is decided.

## 7. T1 against Capsule v1

`host-tools/qemu-test/build-topology.sh`, ADR-0074's T1 performed:

```text
supervisor resident
  -> creates a transient build worker, funded at its own role's grant (96 MiB)
  -> the worker builds a real TOSBUNDLE/v1 over this capsule's own canonical
     source, read back out of its own launch record
  -> freezes it, shares it, hands the shared region to the supervisor over the
     endpoint it was given, and exits
  -> the supervisor collects the worker's ending with process_wait_child
  -> and only then creates the target from the bundle
```

**Every arrow is checked, and the ordering is read off the log** rather than
assumed from the code: the gate requires the worker's reclamation line to come
before the target's creation, and requires the ending collected to be the
worker's own instance. A topology whose steps merely all occurred would not be
one.

**The two roles are told apart by what they were endowed with.** The supervisor
holds process authority with `create`, `terminate` and `wait_child`, the root's
remainder, and one endpoint it *receives* on as `inbox`. The worker holds an
authority to spend and `send` on the same object as `outbox`, and no authority
over any process — it cannot create, cannot terminate, and never learns what the
artifact is for. One endpoint, two names: ADR-0061 makes the binding the
identity of a request.

**The chain is real.** The capsule's canonical source, its identity and its
source binding travel into the bundle; the target parses the artifact itself,
verifies every image itself, and runs its entry. Nothing was weakened to get
there — no receipt crosses, no host verdict crosses, and the nucleus reads not
one byte of the bundle.

**The capacity claim, measured with both roles resident:**

```text
supervisor 14357 + worker 25109 = 39466 frames of 57410
70.09 MiB left for bundle backing
```

Held against the largest bundle any Capsule v1 configuration produces —
`50.52 MiB`, at 255 modules of 128 KiB, from `STAGE3_BUILD_WORKSPACE.md` — that
leaves **19.57 MiB** to spare. Deliberately *not* held against this boot's own
1147-byte evidence artifact, which §D said not to use as the capacity argument.

The claim, stated precisely:

> Stage 3 proves the canonical Capsule-v1 freestanding build/launch path under
> the T1 topology, with the worst measured Capsule-v1 bundle fitting the
> measured headroom.

It does **not** claim that a future installed-source backend can freestanding-
build every generative docs/44 ceiling corpus with the same supervisor resident.
ADR-0073's separation of the reference algorithm, the Capsule-v1 boot path and a
future installed-source backend is not collapsed.

## 8. `CreatedProcess` and `ChildEnding`

Schema-declared nominal record types (`SYSTEM_INTERFACE_V1` §4.2), not tuples
with remembered positions and not host-only structs.

A schema record is an ordinary TOS Core nominal record: the same type
constructor a module's own `record` declaration produces, the same
`TypeDef::Nominal` in the artifact, checked by a verifier the same way. What is
new is only *who declares it* — this schema rather than a module, exactly as it
already declares the interfaces and operations a module may name. No new record
ABI, and the language type system is untouched.

A module cannot construct one: nothing in the language names a schema record's
constructor, so the only way to hold one is to have been given it by the
operation that produces it.

**The three optional facts of an ending are `Option<u64>`.** ADR-0067 states the
rule the other way round from a register-and-offset contract — absence is the
true value, and a zero would be a claim its caller never made. The
flag-beside-a-value translation happens once, at the boundary, instead of in
every supervisor that reads one.

Field order is part of the contract, because a value's parts are matched to
their names by position. The gate holds three statements of it together: the
document's §4.2 tables, the frontend's table, and the order the runtime image
builds the record in.

They survive lower → TOSIMAGE → decode → independent verifier
(`capability_source.rs`), and `CreatedProcess.control` is used as a
capability value in both the host proof and the QEMU boot — a capability field
of a record is a capability value like any other (ADR-0078).

## 9. What the work found

Two real defects, each caught by a test that could only fail for it.

**A leaked launch plan.** The supervisor built a plan per launch and never
released one, so `MAX_PLANS` restarts exhausted the table and every creation
afterwards was refused. The host tests could not see it — they answer
`launch_plan_create` from a script — and the QEMU boot showed it as eight
consecutive `supervisor.result.refused` records. ADR-0077 §5 already said what to
do: a sealed plan survives the creation that reads it, so there is now one plan
for the whole boot, which is also what makes a restart the same decision.

**A clobbered register.** `launch_plan_endow` declared `rdx` as an input alone,
while every operation answers in `rax` and `rdx`. The compiler is entitled to
believe the length is still there afterwards and to reuse it, so the *second*
entry of a plan got a binding of length zero. One entry never shows it; two do,
and a build worker's endowment is two. Found by T1, fixed as an `inlateout`.

## 10. Gates

`scripts/preflight.sh` — **36 of 36**, with `supervision.rs` added to the host
tests (9 tests).

Two QEMU gates are new: `supervision` and `build-topology`. Nothing was
weakened, no threshold moved, and every existing import-only, launch-plan,
funding, runtime-authority, image and verifier gate is unchanged.

The `check-interface-schema` gate grew three checks: §4.2's records field by
field and in order, the order the runtime image builds `ChildEnding` in, and
scoping of the operation extraction to `ACCEPTED` so a record's field is not read
as an operation.
