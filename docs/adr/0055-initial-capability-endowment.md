<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0055: Where a process's first capability comes from

- Status: **Proposed**
- Date: 2026-08-19
- Decision level: 2 — it fixes the launch boundary's authority half and the
  initial state of every process's capability table, inside the interface
  ADR-0048 fixed; it changes no Tier 0 invariant and no TOS Core V1 semantics
- Project Architect approval: *(unsigned)*

## The gap, stated once

`SYSTEM_ABI_V1` §5 assigns twelve operations. Nine of them require a capability;
three are self-only. **None of the twelve produces a capability.**

- `endpoint_send`, `endpoint_receive`, `endpoint_call` require an endpoint
  handle, and nothing creates an endpoint.
- `region_share` requires a region handle, and nothing creates a region
  capability — the memory grant reaches a process as a base and a length in the
  launch record, which is not a handle and carries no rights.
- `process_create` requires a process-authority capability, and nothing creates
  one; so the operation that would let a supervisor endow a child cannot itself
  be reached by anyone.
- `capability_attenuate` and `capability_release` operate on a capability the
  caller already holds.

`CAPABILITY_V1` says precisely what a handle is, who owns the table, what
validation costs and how attenuation, delegation, transfer and revocation
behave. It does not say where the first one comes from. `IPC_V1` describes
endpoints as objects that exist. The launch record (`tos-launch::Launch`,
ADR-0053) carries memory and text and no handle of any kind.

Read literally, a conforming Stage 3 system holds no capability, ever, and can
perform no operation but the three self-only ones. That is the state the
implementation is in today — and it is currently indistinguishable from correct.

This is not one document being wrong. It is a question each of three accepted
documents assumes another answers, which AGENTS.md §2 requires be reported
rather than settled by picking a reading.

## What must not be done about it

**An operation invented at the edge because the implementation needed one.**
`SYSTEM_ABI_V1` §5 states that an operation reachable without a capability is a
design defect rather than a convenience. A `capability_create` reachable by
anyone is ambient authority with a handle in front of it, which docs/02 rules
out and `CAPABILITY_V1` §3 names in those words.

**A well-known handle.** "Handle 0 is always your grant" makes authority
positional rather than granted, and makes every process's authority identical
regardless of what anyone decided about it.

**A capability the nucleus decides on.** ADR-0048 §2 gives the nucleus
mechanism and explicitly no service policy. A nucleus that chooses what a
process may do has taken the decision that Stage 3's whole authority story says
belongs to whoever launched it.

## Options

### A — the launcher endows, and the endowment travels in the launch record

The party that launches a process decides its initial capability set. The
nucleus builds the process's table from that decision before entering it, and
the launch record gains a table of initial handles: for each, the object, the
rights, the scope and the index the process will find it at. `LAUNCH_VERSION`
becomes 2; a nucleus and an image that disagree do not run together, which is
what ADR-0053 already established that field for.

For the **first** process there is no launcher but the nucleus, so the nucleus's
launcher decides — and ADR-0051 §2 already says what it decides *from*: it reads
`capability_imports` from the verified IR, sees exactly which capabilities the
module intends to use, and grants or denies each under policy. Stage 3's policy
for the boot process can be the narrowest possible one — deny everything not
named, grant nothing a module did not request — and still be a policy the system
applied rather than a default it fell into.

`process_create` (8) then takes the child's endowment as an argument, attenuated
from what the caller itself holds: a parent cannot grant what it does not have,
which makes the recursion terminate at the boot process and makes escalation by
spawning impossible in the same breath.

Costs: the launch record grows a variable-length table and a version. The
nucleus gains a path that writes a process's table before the process runs —
which is the same path `process_create` needs, so it is written once. Something
must decide the boot process's policy, and until `/system/policy/` exists
(ADR-0051 §3) that decision is a constant in the launcher, which must be visible
in the audit record rather than implied.

### B — a self-only creation operation

A thirteenth operation, in the class of `context_yield` and `time_monotonic`:
a process may create an object only it can name — an endpoint, say — and receive
a handle to it. Creating something nobody else can reach confers no authority
over anyone, so it does not violate §5's rule in substance; authority still
spreads only by delegation and transfer.

Costs: it hands every process, including an untrusted one, the ability to make
the nucleus allocate, which is a resource-exhaustion channel that must then be
bounded per process and accounted — and docs/35's Stage 3 contract does not
currently describe such a bound. It also does not answer the question that is
actually blocking: two processes that each create their own endpoint still
cannot reach each other, because neither can hand the other a handle without an
endpoint they already share. **B is necessary for a system where processes make
new objects; it is not sufficient for bootstrap, and bootstrap is what is
blocking.**

### C — the grant becomes the first capability, and nothing else changes

The memory grant every process already receives becomes a region capability
rather than a base and a length: one handle, at launch, of a type
`CAPABILITY_V1` already names. Nothing new is invented, and `region_share` (7)
becomes reachable, which makes shared memory the first thing two processes can
do.

Costs: it changes ADR-0041's grant and ADR-0050's per-process grant into a
capability, which is a change to two accepted decisions and to the runtime's
adoption path (`GlobalHeap::adopt`) — the one part of Stage 2 that survived the
move to ring 3 unchanged. It also answers only the region question: endpoints
and process authority still have no origin, so `endpoint_*` and `process_create`
stay unreachable and Stage 3's IPC deliverable stays blocked.

## Recommendation

**A**, with B available later as a separate decision when a process needs to
make objects it was not given.

A is the only option that answers the question that blocks — how *anything*
first holds authority — and it answers it in the place the accepted documents
already point: ADR-0051 §2 has the launcher reading capability requests from the
verified IR and granting or denying them under policy, which is an endowment
mechanism described from the policy side with no transport underneath it. A
supplies the transport.

It also makes the recursion honest. `process_create` endows a child from what
the parent holds, attenuated; the chain terminates at the boot process, whose
endowment is the launcher's constant and is on the audit record. A system where
authority has a root that can be named and audited is the system docs/12
describes; one where every process can conjure authority is not.

If A is accepted, then:

1. `LAUNCH_VERSION` becomes 2 and the record gains an initial-capability table:
   count, and per entry the object, the rights mask, the scope and the index.
2. The nucleus builds the process's capability table from that record before the
   process is entered, and the table is nucleus memory the process cannot
   address (`CAPABILITY_V1` §2).
3. `process_create` (8) takes the child's endowment as an argument and refuses —
   `E_NO_CAPABILITY` — any entry that is not an attenuation of something the
   caller holds. The nucleus checks the subset relation and does not take the
   caller's word (`CAPABILITY_V1` §4).
4. Every grant is on the audit record with what was granted, to which process,
   and on whose authority. A grant nobody can attribute is the thing
   `CAPABILITY_V1` §3 refuses to call a capability.
5. The boot process's endowment is a stated constant of the launcher until
   `/system/policy/` exists, and it is named in the log as such — not as a
   default, because a default is what nobody decided.

## Boundary

Phase 4 Task 0 and Task 1 cannot start until this is decided; Task 2 and Task 3
depend on Task 1. Phase 3 does not depend on it and is closed. Nothing in the
scheduler, the process table or the timer changes under any of the three
options.
