<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0056: Where the capability an operation requires is named, and what an empty table returns

- Status: **Proposed**
- Date: 2026-08-19
- Decision level: 2 — it states two things `SYSTEM_ABI_V1` requires conformance
  to and does not currently state; no operation, status, right or guarantee
  changes, so it is the same kind of addition §7 already records having made
- Project Architect approval: *(unsigned)*

## The gap, stated once

Two questions, both of which the first line of a capability-checking dispatcher
has to answer, and neither of which any accepted contract answers.

**Where is the handle?** `SYSTEM_ABI_V1` §3 lists six argument registers and
says arguments are "values and handles, never pointers the nucleus dereferences
without bounds". §5 says each operation "names the capability it requires" — in
prose, in a table column. Nothing says which *argument* carries it. Two
conforming implementations could put an endpoint handle in different registers
and both be conforming, which makes the contract unconformable in exactly the
way §7 says a rule about numbers that never states the numbers is.

**What does an empty table return?** §4 is emphatic that `E_NO_CAPABILITY` and
`E_BAD_HANDLE` are distinct and must not be merged: the first says the process
holds the wrong authority, the second says it named nothing at all, and "an
audit log that cannot tell them apart cannot describe an attack". A process with
an empty table that passes index 0 has done both at once — it holds no
authority, and index 0 is outside a table of size zero. The two required
refusals disagree about which one it gets.

The current nucleus answers `E_NO_CAPABILITY` to operations 1–9 by operation
number, without reading any argument. That is defensible under §8.1 and
unconformant to §8.2, which requires that an out-of-range handle index yield
`E_BAD_HANDLE` — a requirement the implementation cannot satisfy while it never
looks at the index.

## Options

### A — the capability is the first argument; range is checked before rights

`rdi` carries the capability an operation requires, for every operation that
requires one. Where an operation requires two — none does today — the contract
assigns them in §5 order at the time the operation is added.

The refusal order is: **index bounds first, then generation, then type, then
rights.** So an out-of-range index is `E_BAD_HANDLE`, and everything else is
`E_NO_CAPABILITY`. An empty table therefore refuses every index with
`E_BAD_HANDLE`, because there is no index inside it.

Costs: it fixes an ABI position that later operations must respect, and it
changes what a process sees today from `E_NO_CAPABILITY` to `E_BAD_HANDLE` for
operations 1–9 — visible in any conformance test written against the current
behaviour, of which there are none.

### B — the capability is the first argument; absence outranks range

The same position, the opposite precedence: a process that holds no capability
of the required type receives `E_NO_CAPABILITY` whatever index it passed, and
`E_BAD_HANDLE` is reserved for a process that holds *some* capabilities and
named an index beyond its table.

Costs: the answer depends on the caller's holdings rather than on its argument,
so the same call from two processes gets different statuses for the same
mistake. That is the merge §4 forbids, performed by precedence instead of by
naming — an auditor reading `E_NO_CAPABILITY` cannot tell whether the caller
asked for authority it lacks or named nothing at all.

### C — leave it to each operation

`IPC_V1` fixes the layout for the endpoint operations, `CAPABILITY_V1` for the
capability operations, and so on, each in its own contract.

Costs: three contracts each own part of one calling convention, and the fourth
operation added in a hurry owns none of it. The convention is a property of the
edge, and the edge has exactly one contract.

## Recommendation

**A.**

The handle is what the operation is *about*, and the first argument is where the
subject of a call goes in every convention this system already uses — the launch
record's address arrives in `rdi`, and `process_exit`'s status does too.

On precedence: bounds first is the order that makes the status a fact about the
call rather than about the caller. "You named nothing" is checkable from the
argument alone; "you lack the authority" requires there to be something at that
index to lack authority over. Answering in that order is also the order that
costs least — a bounds check before a table read — and it is the order
`CAPABILITY_V1` §2 lists validity in: "index in range **and** generation
matching".

The consequence worth stating plainly: until ADR-0055 gives a process a first
capability, **every** capability-bearing operation answers `E_BAD_HANDLE`, and
that is the honest answer. A process that holds nothing names nothing.

If A is accepted, `SYSTEM_ABI_V1` gains one sentence in §3 fixing the position,
and §4 gains the refusal order. Neither adds an operation, a status or a right,
so the contract stays at version 1 by the rule §7 already states.

## Boundary

Phase 4 Task 0 cannot be written without this, because the first line of a
capability-checking dispatcher is which argument to read and what to say when it
is wrong. It is independent of ADR-0055: A is the same decision whether the
table is filled by a launch record or by anything else.
