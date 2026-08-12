<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0048: Where TOS Core executes relative to the isolation boundary

- Status: **Proposed**
- Date: 2026-08-12
- Decision level: 3 — moves a trust boundary and fixes the nucleus/process
  interface for every later stage
- Project Architect approval: *(pending)*

## Context

Stage 2 is closed. The nucleus validates the boot ABI record and the capsule,
runs one canonical module through the reference path and halts. That module runs
**in the nucleus's own address space, at CPL 0, on the nucleus's stack, out of a
single heap adopted once** (ADR-0041). There is exactly one execution context in
the system, `run()` is run-to-completion, and maskable interrupts have been
disabled since the loader handed over (ADR-0023).

Stage 3 is the process, IPC and capability substrate. docs/16 lists isolated
address spaces and a scheduler among its deliverables; docs/37 asks whether
"textual processes exercise real capability/IPC contracts rather than running as
decorative scripts around privileged binary services"; docs/03 fixes eight trust
zones with the nucleus alone at the top.

None of that can be implemented without first answering one question, because
every other Stage 3 decision is downstream of it: **when a TOS Core module runs
as a process, where does it run relative to the isolation boundary?**

The accepted documents constrain the answer without settling it. docs/42 §2
already gives the language its half of the contract — a capability import is a
request, not a grant; a denied request produces the typed launch error
`CapabilityDenied`; a handle cannot be forged, encoded into bits or recreated
after consumption — and states explicitly that the real interfaces "belong to
later stages and must be separately versioned". So the language is ready. The
mechanism is not, and choosing the mechanism badly would either erase the
architecture or reopen Stage 2.

## Decision

**TOS Core modules execute at CPL 3, in their own address space, one runtime
instance per process. The nucleus provides mechanism only — address spaces,
scheduling, capability tables, IPC transport — and is reachable only through the
versioned system ABI published as `SYSTEM_ABI_V1`.**

Concretely:

1. A process is an address space, a capability table, one or more execution
   contexts, a memory grant (ADR-0050) and a runtime instance executing exactly
   one verified TOS Core module.
2. The nucleus owns the page tables, the frame allocator, the scheduler, the
   capability tables and the IPC transport. It owns no service policy.
3. Preemption is a timer interrupt (ADR-0049). A process is interrupted the way
   any user-mode code is interrupted.
4. `/system/boot/init.tos` becomes the first process rather than a function the
   nucleus calls. Stage 3 replaces the Stage 2 halt with a launch.

## Why, and what the alternatives cost

**Alternative B — software isolation: keep everything at CPL 0 and let the
verifier and the engine be the boundary.** Rejected on two grounds, one
documentary and one architectural.

Documentary: docs/16 names isolated address spaces as a Stage 3 deliverable, and
docs/37 asks Stage 3 to demonstrate real enforcement. B would require amending
an accepted stage contract to describe the thing that was built — a Level 3/4
amendment whose only motivation is that it is easier.

Architectural, and this is the heavier objection: with everything in one address
space, preemption has to come from inside the interpreter, so the engine needs a
yield/resume contract. That reaches straight back into accepted Stage 2
semantics — the deterministic evaluation order of docs/40, the resource and
scheduling neutrality rules of docs/41, and the accounting model ADR-0043's
budget is measured against. **B buys a simpler scheduler by reopening the
language runtime contract Stage 2 just closed**, which is the trade this project
does not make.

**Alternative C — hardware address spaces with the engine mapped read-only into
each.** Not a different boundary; it is an implementation of this decision that
shares one physical copy of the runtime text. It is explicitly permitted later
without a new ADR, because it changes what is mapped, not who is isolated from
whom.

## Consequences that must not be discovered later

**The verifier stops being the isolation mechanism.** After this decision, one
process cannot reach another's memory because the hardware does not map it, not
because the IR was checked. The verifier keeps its Stage 2 role — source-to-IR
integrity, declared resources, capability-operation legality — and loses the
role it never formally had. Nothing in the system's isolation may be justified
by "the verifier accepted it", and no later change may quietly make process
safety depend on verification again.

**The engine becomes a per-process derived artifact with an identity.** A ring-3
runtime is a binary loaded into each process. docs/10 already requires a
"runtime engine ID" in process identity, so this is not a new field, but it is
now load-bearing: the capsule must carry the runtime image, its identity must be
reported, and it is a derived artifact whose provenance rules (AGENTS.md §9)
apply in full.

**Fuel stops being a fairness mechanism and stays a declared bound.** With timer
preemption, a process that ignores its own accounting cannot starve the system.
The declared resource envelope remains what the module promised about itself and
what the verifier checks; it is not the scheduler's admission control.

**Identity must name its asserter.** A process that reports its own module
digest is reporting a claim, not evidence. The record that belongs in the
identity plane and the audit log is the one made by whoever had the capability
to create the process — the launcher computed the source identity from the bytes
it passed in. Self-reported introspection remains available and is labelled as
such. `PROCESS_IDENTITY_V1` must state, field by field, who asserts it.

**Two copies of the boot text stop being acceptable by accident.** Once init is
a process, the capsule's copy and the repository-backed copy of
`/system/boot/init.tos` are related by the handoff protocol of docs/04. Stage 3
does not implement the repository, so Stage 3 launches from the capsule and must
say so in the identity record rather than implying a commit it did not read.

## What this ADR authorizes, and what it does not

It authorizes Stage 3 production implementation to begin **only after** ADR-0049
(interrupts and preemption), ADR-0050 (per-process memory grants) and ADR-0051
(service manifest surface) are accepted and the four interface contracts —
`SYSTEM_ABI_V1`, `CAPABILITY_V1`, `IPC_V1`, `PROCESS_IDENTITY_V1` — are
published into `source/interfaces/system/`. A partial contract set is not a
partial authorization.

Those four are drafted alongside this ADR in `docs/superpowers/specs/` and stay
there until acceptance. `source/interfaces/` carries accepted authority only —
`scripts/tests/check-interface-contract-authority.sh` enforces exactly that —
and a proposal filed where accepted contracts live would be authority a document
assigned to itself.

It authorizes nothing in Stage 4 or later: no PCI, MMIO, IRQ or DMA interface,
no filesystem, no repository-backed `/system`, no network, no shell, no UI. A
Stage 3 process that needs one of those is evidence that the slice is wrong, not
grounds for a small exception.

It does not change TOS Core V1, `tos-ir/v1`, the verifier contract, Boot ABI v1,
capsule format or provenance identity, and it does not reopen the Stage 2
closure.

## Evidence required before Stage 3 can close

From docs/37 §Stage 3, made concrete:

- process source identity bound to a commit or capsule blob, asserted by the
  launcher, present in the audit record and reproducible from the artifact;
- an explicit granted capability set per process, and a denied request that
  produces `CapabilityDenied` at startup rather than a fabricated success;
- negative tests: a process cannot read or write nucleus memory or another
  process's memory; cannot execute a privileged instruction; cannot acquire
  authority by guessing, forging or re-encoding a handle; a confused-deputy
  attempt through a broker fails and is attributable;
- a fault in one process kills that process and leaves the system and its peers
  running;
- privileged policy lives in a source-identified textual process, not in the
  nucleus; the dependency inventory shows no service logic moved inward for
  convenience (docs/31);
- service restart preserves identity and audit records;
- the docs/35 Stage 3 IPC budgets measured against a benchmark defined before
  the first measurement.

## The risk this decision is most exposed to

docs/19 states it plainly: years of ordinary boot, scheduler, PCI and driver
work could produce a conventional microkernel with scripts before Git-native
identity becomes visible. Ring 3, a scheduler and an IPC path are exactly that
kind of work, and doing them well is not evidence that TOS still exists.

The mitigation is structural, not editorial. The identity plane is implemented
with the first process, not after the substrate feels finished; and no
privileged service may ship as a binary on the argument that IPC is not ready
yet. If a service cannot be written as canonical text, the substrate is
unfinished — which is a reason to stop, not a reason to write the service in
Rust and call it Stage 3.
