<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 closure — one group of decisions, and what each one unblocks

> **HISTORICAL — RESOLVED, AND STAGE 3 IS CLOSED.**
>
> Every decision this packet asked for was given, every item its table below
> marks *pending*, *blocked* or *not started* has since been implemented and
> evidenced, and the Project Architect closed Stage 3 on 2026-09-03 for evidence
> commit `77970cb`.
>
> **Read `docs/evidence/STAGE3_CLOSURE_AUDIT.md` for the current state**; the
> approval is archived in
> `source/legal/publication-records/77970cb-stage3-closure-approval.md`.
>
> This document is kept for the questions it asked and the reasoning behind
> them. Its status lines describe 2026-09-02 and describe nothing now. Where it
> says a thing needs new ABI operation numbers, or is blocked, or is not
> started, the resolution is:
>
> | This packet said | Resolved by |
> |---|---|
> | typed system-interface results — decision pending | `SYSTEM_INTERFACE_V1` §5; `STAGE3_LAUNCH_PLANS.md` §1 |
> | heterogeneous textual endowment — needs new ABI operations | ADR-0077, operations 21–23 |
> | textual supervisor that *creates* — blocked | ADR-0078, then `tests/vectors/supervision/init.tos` |
> | canonical supervisor, `/system/policy/` — pending | `STAGE3_SUPERVISION.md` §1 |
> | build worker — pending | ADR-0074 §4a; `qemu_build_topology` |
> | service manifests — not started | still not started, and out of Stage 3 |
> | terminal failure / restart evidence — pending | `STAGE3_SUPERVISION.md` §3–§5 |
> | central operator-visible journal — not started | `RUNTIME_OBSERVABILITY_V1` §9 |
> | observer / performance — environmental prerequisite | run in the repository's own workflow: P2, `p99 = 39.147 µs` |
> | identity exit audit — pending | `STAGE3_CLOSURE_AUDIT.md` §10 |

- Status: **historical decision packet — resolved; see the closure audit**
- Date: 2026-09-02
- Written at: operations 19 and 20 implemented and green, operations 8 and 15
  retired
- Why one packet: the remaining work is a single round of implementation whose
  shape depends on four answers. Asking them one at a time would produce another
  chain of syscall-sized iterations, each blocked on the next
- Related: ADR-0037, ADR-0051, ADR-0055, ADR-0060, ADR-0061, ADR-0067, ADR-0069,
  ADR-0071, ADR-0073, ADR-0074 (**Draft**), ADR-0075, ADR-0076;
  `SYSTEM_INTERFACE_V1`, `SYSTEM_ABI_V1`, `CAPABILITY_V1`; docs/37, docs/40,
  docs/42, docs/43, docs/44

## 0. What is already true

Everything below is implemented, and every number is measured on the current
reference boot rather than quoted from earlier prose.

| Mechanism | State |
|---|---|
| `MemoryAuthority`, reservation accounting | operations 16, 17 — green |
| Region: mutable → immutable → shared | operations 18, 7 — green |
| Region IPC, affine and shared | green |
| Funded process creation | operation 19 — green |
| Ambient root spending | **retired**: operations 8 and 15 answer `E_NOT_SUPPORTED` |
| Bundle target creation, restart from one artifact | operation 20 — green |
| Target-owned `TOSBUNDLE/v1` admission | green: a corrupt bundle creates a process that refuses itself |
| A textual module reaching a system operation | green (`module-operation`, `supervisor-text`) |
| A textual module **creating a process** | **not possible** — see §A |

## A. The textual `SYSTEM_INTERFACE` bridge

### A.1 The problem, stated exactly

`SYSTEM_INTERFACE_V1` §4 is the only way a TOS Core module reaches the system.
Four properties of it now block Stage 3's identity claim:

1. **Every declared result is `i64`.** The ABI returns a capability in `rdx` for
   operations 5, 7, 16, 17, 18, 19 and 20. A schema that can only carry a status
   cannot carry any of them.
2. **§4.1 admits no list.** An endowment is a list of
   `(capability, rights, binding)` triples whose capabilities have *different*
   nominal types. `array<T, N>` is homogeneous, and TOS Core V1 has no
   user-defined generics (docs/40 §"There are no user-defined generic
   functions").
3. **`process_create` was withdrawn with operation 8** (this round). It was the
   only creation a module could reach, and it was declared as taking a path and
   endowing the child nothing — which was honest for operation 8 and is not
   expressible for 19.
4. **Startup denial semantics are load-bearing.** ADR-0061 and docs/42 §2 require
   a module's `import capability` requests to be answered *before its first
   instruction*, with an unanswered request reported as `CapabilityDenied` at
   startup. Any scheme that provisions a child after it starts changes that.

Stage 3's identity question (docs/37) is "do textual processes exercise real
capability/IPC contracts rather than running as decorative scripts around
privileged binary services?" Today the answer is *partly*: a module performs
operations and reads a policy, and the thing that **starts services** is a Rust
image. Closing Stage 3 means the supervisor and the build policy are text.

### A.2 What already exists, and what would have to move

**The value representation exists.** `tos_engine::Value::Capability(Handle)` is
already a variant, and a capability already crosses the boundary as an
*argument*. Its doc states the current rule plainly: "no operation of the
language produces one … a capability arrives as an argument of the run and
leaves as an argument of an interface operation". Making an operation *return*
one is a change to that rule and to nothing about the representation.

**`Result<T, E>` exists** (docs/40 §"…`Result<T,E>` takes two [type
arguments]"), as do `Option<T>`, `Shared<T>` and `Region<T>`, all in the closed
predeclared-generic list of docs/43. A typed result therefore needs **no new
type and no new type constructor**.

**What is genuinely open** is whether `tos-ir/v1` and the verifier admit an
extern call whose result type is a nominal capability type or `Region<T>`. That
is one question with one place to answer it — the extern-call lowering in
`tos-core/src/lower.rs` and the operation-result typing in
`tos-core/src/boundary.rs` — and it must be confirmed before any of the options
below is accepted, because if the answer is no, the option becomes a change to a
**closed** contract rather than an addition to an open one.

### A.3 Options for typed results

| | shape | reaches | cost |
|---|---|---|---|
| **R1** | schema declares `result: Result<T, i64>` per operation, `T` a nominal capability type or `Region<T>`/`Shared<T>` | every capability-returning operation | `SYSTEM_INTERFACE_V1` §4 gains a result column that is not `i64`; the frontend types the extern against it; the engine returns `Value::Variant` wrapping `Value::Capability` |
| **R2** | schema keeps `i64` and adds a separate "last produced capability" retrieval operation | the same, in two calls | a second call whose meaning depends on what the first one did — state between two operations, which is what a status-and-out-parameter is in a language with no out parameters. Rejected below |
| **R3** | schema declares the bare capability type as the result, with failure as a trap | everything | a refusal becomes a trap, so a supervisor cannot handle `E_LIMIT` without unwinding. Rejected below |

**Recommended: R1.** It uses only types TOS Core V1 already has, it keeps a
refusal a *value* the supervisor can branch on — which is what a supervisor does
all day — and it does not introduce an operation whose result depends on a
previous one. R2 is a hidden out-parameter and R3 makes ordinary backpressure
unhandleable.

The exact schema shape recommended:

```text
| region_allocate | system.memory.Authority with spend | bytes: u64
                  | Result<Region<u8>, i64> | 17 |
| region_freeze   | Region<mut u8> with write
                  | Result<Region<u8>, i64> | 18 |
| region_share    | Region<u8> with share
                  | Result<Shared<Region<u8>>, i64> | 7 |
```

### A.4 Options for a heterogeneous endowment

This is the hard half. A supervisor must be able to say *child binding X gets
capability A, child binding Y gets capability B*, where A and B have different
nominal types.

| | shape | verdict |
|---|---|---|
| **E1** | a **launch-plan capability object**: `system.process.LaunchPlan`, created by an operation, extended by one operation *per capability interface* — `plan_endow_endpoint(plan, endpoint, rights, binding)`, `plan_endow_memory(plan, authority, …)`, `plan_endow_process(plan, process, …)` — and consumed by `process_create_funded(control, memory, plan, path, grant)` | **recommended** |
| **E2** | an `AnyCapability` type source can hold and pass | rejected: it is a type through which source can widen or forge authority, and `CAPABILITY_V1` §2 keeps rights out of a handle's bits precisely so that no holder can reason about them |
| **E3** | raw handles as integers | rejected for the same reason, one level cruder |
| **E4** | post-start IPC provisioning: the child starts endowed nothing and asks a parent | rejected: **it cannot satisfy startup denial semantics.** A child endowed nothing reports `CapabilityDenied` for every `import capability` and never runs its first instruction, which is exactly what ADR-0061 requires it to do. Provisioning after that point is provisioning a process that has already refused to start |
| **E5** | inherit the funding authority automatically | rejected: ADR-0076 §3 makes a creation a *charge*, not an inheritance, and an automatic name would be authority nobody decided to give (ADR-0055) |

**Why E1 is expressible under V1 as it stands.** §4.1 already types each
capability parameter by the interface it must be of, and already admits several
capability parameters in one operation (operation 13 has two). One `plan_endow_*`
operation per object kind is a *finite* set — there are four kinds — every one of
which is ordinarily typed, forges nothing, and widens nothing: the rights it can
grant are still intersected with what the caller holds, by the same rule
`capability_attenuate` uses. The plan is a capability like any other, so a
process that was not given the right to make one cannot make one.

**What E1 would require, named exactly:**

- `SYSTEM_ABI_V1` §5: **new operation numbers** for creating and extending a
  launch plan. This is the one place the recommendation goes beyond what this
  round implemented, and it is a decision for the Architect rather than one this
  round took — the STOP rule reserved new public operations, and operations 19
  and 20 needed none.
- `SYSTEM_INTERFACE_V1` §4: the `system.process.LaunchPlan` interface, the
  `plan_endow_*` operations, and a `process_create_funded` row.
- `SYSTEM_INTERFACE_V1` §4.1: the typed-result rule of §A.3.
- **Nothing in TOS Core V1 or `tos-ir/v1`**, *if* §A.2's open question answers
  yes. If it answers no — if an extern call cannot be typed to return a nominal
  capability type without changing the closed language or IR contract — then
  **that is a Level-2 STOP** and this recommendation must not be implemented
  around it.

## B. The freestanding build-worker and supervisor lifecycle

### B.1 The current physical account, measured

From the reference boot at this commit (`TOS.MEM.ACCOUNT`, `TOS.MEM.RESERVE`,
`TOS.RUN.PROCESS_CHARGE`):

```text
admitted                 58 901 frames
table reserve             1 452 frames   (1 451 runtime baseline, 1 permanent
                                          RegionBackingSpace root)
root MemoryAuthority     57 426 frames = 224.32 MiB

one process, beyond its arena             2.08 MiB
   (writable data, stack, report, arguments, launch record)
54 MiB arena  -> total charge            56.08 MiB
96 MiB arena  -> total charge            98.08 MiB
```

**The margin on that last line is now zero.** Four ordinary processes need
57 424 frames and the root holds 57 424. It was 4 frames two commits ago and 2
one commit ago: `admitted_frames` falls as the boot artifacts grow, and the
production runtime image grew this round by the bundle-target launch path. This
round put every evidence-only workload behind a feature so that the image a
canonical boot runs no longer carries them — which recovered the margin to zero
rather than to comfort. **Any further growth of the nucleus or the production
runtime image will take it negative**, and the next thing that does should be
paid for rather than absorbed: either by shrinking those artifacts or by a
decision that four *maximal-arena* processes is not what `MAX_PROCESSES` claims.
That decision is not taken here.

Page tables for region mappings are **already reserved and cost nothing new**:
`process_region_mapping_frames = 163` and `region_backing_frames = 308` are part
of the 1 452, and operation 20's target mapping is an ordinary region mapping.
The reserve did not move in this round and frame 1 453 is not needed.

### B.2 The question is the peak, not the sum

```text
supervisor(54) + worker(96)                  154.16 MiB   leaves  70.16 MiB
supervisor(54) + worker(96) + target(54)     210.23 MiB   leaves  14.09 MiB
supervisor(54) + target(54)                  112.16 MiB   leaves 112.16 MiB
```

The bundle is a region charged to an authority like anything else, so the
admissible bundle backing is whatever is left at the **peak** of the topology
chosen. `MAX_PROCESSES` is 4, so three coexisting processes fit the table.

### B.3 Options

| | topology | peak | bundle bound | verdict |
|---|---|---|---|---|
| **T1** | supervisor resident; worker transient; target created **after** the worker exits | supervisor + worker | **70.16 MiB** | **recommended** |
| **T2** | as T1 but the worker is still alive when the target starts | all three | 14.09 MiB | feasible, and needlessly tight for no property gained |
| **T3** | the worker owns orchestration, creates the supervisor after the build and hands the bundle over, then exits | worker + supervisor | 70.16 MiB | same bound, more moving parts, and a build worker holding creation authority is a larger trusted surface |
| **T4** | a measured smaller supervisor grant | smaller | larger | **not chosen here**: there is no measurement of a supervisor's minimum, and choosing one to make arithmetic green is exactly what must not happen |

**Recommended: T1.** It needs no new decision and no new number: the accepted
policy grants (ADR-0069's 54 MiB runtime, ADR-0074's 96 MiB worker) are used as
they stand, the bundle bound is 70.16 MiB, and the artifact this round actually
produced over the boot's own closure is **1 147 bytes** — four orders of
magnitude inside it. Nothing about T1 lowers a ceiling or changes ADR-0040.

**What is deliberately not decided here.** ADR-0074 is a **Draft**. Its
Architect-approved parts — build products outside the workspace, one bundle
region per exact closure — are what this round used. Its build/supervisor
lifecycle and residency conclusions are not an implementation spec, and T1 is a
recommendation about *this* arrangement rather than a declaration that the
canonical worker lifecycle is settled. No permanent `BuildWorkspace` size is
proposed, and docs/44's ceilings are untouched.

## C. Service restart policy — the two questions left open

Both are from `docs/evidence/STAGE3_OPERATOR_REQUIREMENTS.md`, which is recorded
and not implemented. The requirements already accepted and **not** reopened
here: terminal `FAILED` is latching; only an explicit operator action leaves it;
automatic recovery may observe and log a terminally failed service but may not
restart or mutate it; and a dependency-derived inability is a different fact from
a crash.

### C.1 Is a restart-attempt count sufficient, or is a time window needed?

| | rule | failure it has |
|---|---|---|
| **C1a** | a plain count: `restart_attempts` failures ever, then terminal | a service that crashes once a week reaches terminal after five weeks of working correctly. The count measures the service's whole life rather than its health |
| **C1b** | count within a window, with a **readiness reset**: the counter returns to zero when the service has been up and ready for longer than the window | needs a definition of "ready" that the supervisor can observe |

**Recommended: C1b, with readiness defined as the weakest thing the system can
actually observe** — the service has been running, uncrashed, for the window.
Not a health check, not a readiness endpoint, neither of which exists. That keeps
the rule implementable today and leaves a stronger readiness signal as a later
refinement that can only *shrink* what counts as ready.

State-machine consequence: one extra field per service (`window_started_tick`)
and one rule at the restart decision — if `now - last_failure > window`, reset
the count before testing it. `E_CANCELLED`-style ambiguity does not arise,
because the tick is the nucleus's and the supervisor only reads it.

### C.2 Is a dependency-derived failure terminal-latched?

| | rule | consequence |
|---|---|---|
| **C2a** | terminal-latched like any other failure | a transient dependency outage permanently kills every dependent, and an operator must restart a fan-out by hand. It also contradicts the accepted causality rule: a service that never got a fair launch has not failed |
| **C2b** | **not** latched: the dependent enters a distinct `BLOCKED` state, is retried when the dependency becomes available, and consumes **no** restart budget while blocked | a dependency that never recovers leaves dependents blocked rather than failed, which an operator sees as "waiting on X" rather than "X and everything under it is broken" |

**Recommended: C2b.** It is the reading the accepted requirement already forces:
"a dependency-derived inability does not consume the dependent's own restart
budget when it was never started and never ran". A state that consumed no budget
but latched terminally would be a service permanently failed for something that
never happened to it.

State-machine consequence:

```text
STOPPED -> STARTING -> RUNNING -> (crash) -> STARTING   [budget -1]
                    -> BLOCKED  (dependency unavailable, budget unchanged)
BLOCKED -> STARTING  when the dependency reaches RUNNING
RUNNING/STARTING -> FAILED  when the budget is exhausted   [latched]
FAILED  -> STOPPED   only by explicit operator action
```

`BLOCKED` is distinct from `FAILED` in the journal, and the operator view must
say which dependency is being waited on — the accepted requirement's
`reason = dependency A unavailable`.

## D. What is between HEAD and Stage 3 closure

| Item | State |
|---|---|
| funded process creation (19) | **implemented**, evidence green |
| retirement of 8 and 15 | **implemented**, evidence green |
| bundle target creation (20), restart from one artifact | **implemented**, evidence green |
| target-owned `TOSBUNDLE/v1` admission | **implemented**, evidence green |
| Region model, freeze, share, IPC transport | **implemented**, evidence green |
| typed system-interface results | **decision pending** — §A.3 |
| heterogeneous textual endowment | **decision pending** — §A.4, and it needs new ABI operation numbers |
| textual supervisor that *creates* | **blocked on §A**; mechanism exists, no textual consumer |
| `/system/policy/` supervision policy as text | **mechanism exists, no textual consumer**: a policy module is read today (`supervisor-text`), and what it can drive is limited to what §A unblocks |
| canonical supervisor | **decision pending** — §A and §B |
| build worker | **decision pending** — §B; the mechanism is proved by a supervisor that builds a real bundle |
| bundle handoff | **implemented** |
| target restart from one bundle | **implemented**, evidence green |
| service manifests | **not started, and deliberately**: it depends on §C |
| terminal failure / restart evidence | **evidence pending**, after §C |
| central operator-visible important-error journal | **not started**; requirements recorded in `STAGE3_OPERATOR_REQUIREMENTS.md` |
| authority-denial evidence | **implemented** (`module-operation`, `process-control`, and the withdrawn-operation gate) |
| full physical account | **implemented and green**: pool and reserve close on every gate |
| observer / performance gates | **implemented**; environmental prerequisite (an ADR-0066 observer QEMU) is not present on this host |
| identity exit audit | **evidence pending**, once the supervisor is textual |

## E. What is asked of the Architect

Four answers, in one group:

1. **§A.3** — accept `Result<T, i64>` typed results in `SYSTEM_INTERFACE_V1`, or
   name another shape. Conditional on the open question in §A.2 answering yes;
   if it answers no, this becomes a Level-2 STOP rather than an amendment.
2. **§A.4** — accept the launch-plan object (E1), which requires **new
   `SYSTEM_ABI_V1` operation numbers**, or name another way to express a
   heterogeneous endowment from source.
3. **§B.3** — accept topology T1, or name another.
4. **§C** — accept C1b and C2b, or name the rules to implement instead.

After those, the remaining work is one broad implementation round — the typed
bridge, the textual supervisor and policy, service manifests with the accepted
restart rules, the operator journal, and the identity exit audit — rather than
another chain of syscall-sized iterations.
