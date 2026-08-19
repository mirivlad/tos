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

Three decisions are therefore proposed rather than made, in ADR-0055, ADR-0056
and ADR-0057. None is implemented before the Project Architect signs it.

---

### Task 0: The capability table exists — **blocked on ADR-0055, ADR-0056**

**Files:** create `source/nucleus/src/capability.rs`; modify
`source/nucleus/src/syscall.rs`, `source/nucleus/src/process.rs`.

- [ ] A per-process table in nucleus memory, not mapped into the process:
  fixed slots, each holding object, rights, scope, lifetime and generation
  (`CAPABILITY_V1` §2). Fixed-size and statically reserved, for the reason the
  process table is: the nucleus does not allocate on behalf of a caller.
- [ ] Validation is index bounds, generation compare, type compare and a rights
  mask — constant time in the number of capabilities held (`CAPABILITY_V1` §5,
  docs/35 Stage 3).
- [ ] `E_BAD_HANDLE` and `E_NO_CAPABILITY` stop being the same answer. Which one
  an empty table returns is ADR-0056's question, and the dispatcher does not
  guess it.
- [ ] Evidence: a process iterating every index in range receives only what it
  was granted, and an out-of-range index receives `E_BAD_HANDLE` and never a
  fault (`CAPABILITY_V1` §7.2, `SYSTEM_ABI_V1` §8.2).

*Why blocked:* a table nothing can fill answers every question the same way the
current code does, so the evidence for it would be evidence about nothing. What
fills it is ADR-0055.

### Task 1: A process holds its first capability — **blocked on ADR-0055**

- [ ] The endowment reaches the process by the mechanism ADR-0055 fixes, and the
  nucleus is not the party that decides its content.
- [ ] Evidence: the audit record names what was granted, to which process, and
  on whose authority — a grant nobody can attribute is ambient authority with a
  handle in front of it (`CAPABILITY_V1` §3).

### Task 2: Two processes exchange a message — **blocked on ADR-0055, ADR-0057**

- [ ] An endpoint with one receive-rights holder (`IPC_V1` §2), messages
  delivered whole or not at all, inline payload within the bound ADR-0057 fixes.
- [ ] Evidence: a message larger than the inline maximum is refused, not
  truncated (`IPC_V1` §7.1); and a message crosses between the two processes
  Phase 3 already schedules.

### Task 3: Attenuation, transfer and the confused deputy — **blocked as above**

- [ ] Attenuation produces no superset in any dimension; the nucleus checks the
  subset relation rather than taking the caller's word (`CAPABILITY_V1` §4).
- [ ] Transfer of a linear capability consumes the sender's handle atomically
  with the receiver's acquisition — no window in which both hold it, none in
  which neither does.
- [ ] Evidence: `CAPABILITY_V1` §7.6 — a broker holding a strong capability,
  asked by a weak client to act on an object the client cannot name, refuses,
  and the refusal is attributable to the client. docs/37 names this test
  explicitly and it is the one that fails quietly in systems that pass the
  other five.

---

## What is *not* blocked, and why it is still not started

The table could be written today against an endowment that does not exist, and
the two operations that only consume a handle (`capability_release`,
`capability_attenuate`) could be written to refuse everything. Both would
compile, both would pass a gate written to their behaviour, and neither would be
evidence of anything: with no way to obtain a handle, "refuses every handle" is
the behaviour the nucleus already has, reached by more code.

This phase does not build a mechanism whose only demonstration is that it is
unreachable. That is the shape AGENTS.md §4 calls a disguised throwaway, and the
distance between it and the real thing is exactly one signature.

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
