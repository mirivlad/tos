<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 Phase 4: capability handles and typed IPC

> **Scope rule:** this phase implements the accepted `CAPABILITY_V1` and
> `IPC_V1` under the boundary ADR-0048 fixes. Where an accepted contract does
> not say something the implementation needs, this phase **stops and names it**
> rather than choosing the convenient reading (AGENTS.md §2).

**Goal:** authority in this system is a handle in a table the nucleus owns, IPC
is the only way two processes reach each other, and both are exercised by real
processes rather than described. Until then `SYSTEM_ABI_V1` operations 1–9 are
refusals, `docs/37`'s Stage 3 identity question — do textual processes exercise
real capability and IPC contracts — has no evidence either way, and the
scheduler built in Phase 3 schedules processes that cannot say anything to each
other.

## What was measured before planning

At `4994da2`, by reading the shipping code and the accepted contracts:

| Fact | Where |
|---|---|
| Operations 1–9 all answer `E_NO_CAPABILITY`, by operation number and without looking at any argument | `nucleus/src/syscall.rs` |
| There is no capability table in the nucleus — not an empty one, not a stub | nucleus-wide |
| Two processes exist and are scheduled, and share nothing but the tick | `nucleus/src/process.rs`, Phase 3 |
| The launch record carries memory and text, and no handle of any kind | `tos-launch::Launch` |
| `SYSTEM_ABI_V1` §5 assigns twelve operations. **None of them creates a capability, an endpoint, or a region handle** | `interfaces/system/SYSTEM_ABI_V1.md` |
| `CAPABILITY_V1` says what a handle is, who owns the table and what validation costs. It does not say where a process's first handle comes from | `interfaces/system/CAPABILITY_V1.md` |
| `IPC_V1` declares three message bounds as "fixed maximum, declared by this contract version" and states none of the three numbers | `interfaces/system/IPC_V1.md` |
| No contract fixes which argument register carries the capability an operation requires | `SYSTEM_ABI_V1` §3 lists six argument registers and no per-operation layout |

## The gap this phase reaches immediately, stated once

**Every operation except the three self-only ones requires a capability, and no
operation produces one.** `process_create` (8) requires a process-authority
capability; `endpoint_send` (1) requires an endpoint handle; `region_share` (7)
requires a region handle. Nothing in any accepted contract manufactures the
first of any of them, and the launch record carries none.

Read literally, a conforming Stage 3 system can never hold a single capability,
and therefore can never perform any operation but `context_yield`,
`time_monotonic` and `process_exit`. That is not a defect in one document; it is
a question three documents each assume another one answers.

It is not resolvable by implementation choice. Whatever answers it fixes the
shape of the launch boundary, of `process_create`, and of who decides what a
process may do — which is the whole of Stage 3's authority story, and ADR-0048
§2 says the nucleus owns mechanism and **no service policy**.

Three decisions were therefore proposed rather than made, in ADR-0055, ADR-0056
and ADR-0057. **All three were accepted on 2026-08-19, each at option A**, and
the tasks below were implemented against them:

- the launcher endows, and the endowment travels in the launch record;
- the capability is the first argument, and bounds are checked before rights, so
  an empty table answers `E_BAD_HANDLE` at every index;
- 256 inline bytes, 4 transferred capabilities, 2 transferred regions.

---

### Task 0: The capability table exists — **done (2026-08-19)**

**Files:** `source/nucleus/src/capability.rs` (new),
`source/nucleus/src/syscall.rs`.

- [x] A per-process table in nucleus memory, not mapped into the process:
  sixteen statically reserved slots each holding object, rights, scope and
  generation (`CAPABILITY_V1` §2). Lifetime is not a field and is not pretended
  to be one — every capability this stage issues is bounded by the life of its
  holder, which is the ceiling §3 requires, and a column nothing writes would be
  worse than its absence.
- [x] Validation is index bounds, generation compare, type compare and a rights
  mask — four comparisons, constant in the number of capabilities held.
- [x] `E_BAD_HANDLE` and `E_NO_CAPABILITY` stop being the same answer. The
  dispatcher resolves the first argument before it knows what the operation
  does, in ADR-0056's order, and the first failure decides the status.
- [x] Evidence (`capabilities.sh`): a process iterating every index in range is
  refused sixteen times and gains nothing, and an index past the table is
  `E_BAD_HANDLE`. A handle is an index **and** a generation, so an index alone
  is not a guess that can succeed.

### Task 1: A process holds its first capability — **done (2026-08-19)**

- [x] The endowment is written into the process's table by the nucleus **before
  the process is entered**, from what the launcher decided, and described back
  to the process in the launch record (`LAUNCH_VERSION` 2). A process cannot
  widen its table; it can only shrink or refine it.
- [x] The launcher's decision is on the audit record as a decision:
  `TOS.RUN.PROCESS_ENDOWED process= capabilities= policy=launcher-constant
  asserted_by=launcher`, emitted for every process including one endowed with
  nothing.
- [x] **The canonical boot's endowment is empty, and that is the policy.**
  `system.boot.init` requests no capability, and the launcher's stated constant
  grants nothing a module did not ask for. `stage2-runtime.sh` checks it: a
  boot process holding nothing is what makes every later grant attributable.

### Task 2: Two processes exchange a message — **done (2026-08-19)**

**Files:** `source/nucleus/src/ipc.rs` (new).

- [x] An endpoint with a bounded queue that is never grown to accept a message
  (`IPC_V1` §7), messages delivered whole or not at all, inline payload bounded
  at ADR-0057's 256 bytes.
- [x] The payload crosses in a per-process message slot the launcher maps, not
  in the call: `SYSTEM_ABI_V1` §3 admits no pointer the nucleus walks, and six
  registers do not hold 256 bytes. The nucleus reads and writes that slot
  through its own identity map — the arrangement the report region has used
  since the first process, and for the same reason.
- [x] Evidence: 28 bytes cross between the two processes Phase 3 schedules, and
  a payload one byte past the bound is refused rather than truncated. The two
  processes hold **separate halves** of one endpoint — `send` and `receive` are
  separate rights (`IPC_V1` §2) — and each is refused the other's half on its
  own handle.

### Task 3: Attenuation, transfer and the confused deputy — **partly done**

- [x] Attenuation produces no superset in any dimension. The nucleus intersects
  the requested rights with the held ones rather than checking and refusing, so
  **widening is not an error code, it is unexpressible**: a caller asking for
  every right receives what it already had. Evidence: the attenuated handle
  still refuses the half its holder was never given.
- [x] A released handle is stale by generation, and naming it again is refused
  rather than silently addressing whatever occupies the slot next.
- [ ] Transfer of a linear capability, consumed in the sender atomically with
  the receiver's acquisition (`CAPABILITY_V1` §4, `IPC_V1` §6). **Not
  implemented**, and neither is region transfer (`IPC_V1` §5), so §9.1's refusal
  evidence holds for the inline bound and not yet for the capability and region
  counts — a message cannot name either, which is stronger than refusing them
  and is not the same claim.
- [ ] `endpoint_call`/`endpoint_reply` and a blocking receive with the
  cancellation path `SYSTEM_ABI_V1` §6 requires of anything that blocks. A
  receive with nothing to take answers `E_WOULD_BLOCK` today, which §4 assigns
  to exactly that.
- [ ] `CAPABILITY_V1` §7.6 — the confused deputy. It needs a broker holding a
  strong capability and a client holding a weak one, which needs transfer.
  docs/37 names this test explicitly and it is the one that fails quietly in
  systems that pass the other five, so it is named here as outstanding rather
  than approximated by something easier.

## Why the second process is where the evidence lives

The scheduler, the table, the endpoint and the edge are production code on every
boot. What is behind `test-two-processes` is the launcher's *constant* — the
decision that there are two processes and that they are paired by one endpoint.
That is policy, and ADR-0048 §2 says the nucleus owns none of it; ADR-0055 says
the constant stands until `/system/policy/` exists and must be visible on the
log, which it is.

So the canonical boot exercises the same table and the same dispatcher, with an
endowment of nothing, and reports that it holds nothing. The paired build
exercises what an endowment makes possible. Neither is a mock: the second
process runs the same image out of the same pool, and the message that crosses
is copied by the same code either way.

## Global constraints

- **The result does not move.** The canonical path reports `i32:240` and the
  module-set path `i32:42`, with the timer running and the scheduler scheduling.
- **No operation becomes reachable without a capability.** `SYSTEM_ABI_V1` §5
  calls that a design defect, and "except this one, which we needed" is how the
  authority story ends.
- **The nucleus owns no policy.** It owns the table, the transport and the
  validation. What a process is granted is decided by whoever launched it
  (ADR-0048 §2, ADR-0051 §2).
- **Validation cost is constant** in the number of capabilities held, or the
  alternative bound is documented and tested (`CAPABILITY_V1` §5).
