<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 Phase 0: process, IPC and capability contracts

> **Status: complete.** ADR-0048…0051 were accepted on 2026-08-12 and the four
> contracts are published. This plan was documentation-only and stayed that way:
> no process model, address space, scheduler, capability table, IPC transport,
> supervisor or manifest implementation was written under it. Implementation
> begins with Phase 1, against the signed contracts.

**Goal:** publish a complete, reviewable contract set for Stage 3 —
the isolation boundary, the nucleus/process interface, the capability and IPC
model, process identity and the service manifest surface — so that Stage 3
production implementation can begin against a signed contract rather than
against an implementation's incidental choices.

**Architecture:** the nucleus keeps mechanism and gives up policy. TOS Core
modules execute in their own address spaces at CPL 3, one runtime instance per
process, and reach the nucleus only through a versioned system ABI. Authority is
carried by unforgeable capability handles held in a nucleus-owned per-process
table. Every process carries a source identity asserted by whoever launched it.

**Tech stack:** Markdown contracts and ADRs, plus canonical `.tos` examples for
the manifest surface. No production code, no new runtime dependency, no change to
any accepted Stage 2 contract.

**Outcome:** ADR-0048…0051 accepted 2026-08-12; the four contracts published
under `source/interfaces/system/` and registered in
`docs/SPECIFICATION_SOURCES.txt`. They were drafted in
`docs/superpowers/specs/` and moved on signature, because
`source/interfaces/` carries accepted authority only and
`check-interface-contract-authority.sh` enforces it.

## Global constraints

- Stage 2 remains **CLOSED**. TOS Core V1 semantics, `tos-ir/v1`, the verifier
  contract, ownership/resource semantics, Boot ABI v1, cache/provenance
  identity and the Stage 2 closure evidence are not reopened. If Stage 3 needs
  one of them changed, stop at that boundary and say so.
- A contract is **Proposed** until the Project Architect signs it, and a
  Proposed contract is not added to `docs/SPECIFICATION_SOURCES.txt`: the
  consolidated view carries accepted authority only.
- Stage 3 authorizes no Stage 4+ work: no drivers, PCI, MMIO, DMA, filesystem,
  repository-as-system, network, shell or UI.
- The identity plane is not deferred. A process that cannot say which source it
  came from is not a Stage 3 process, and no privileged service may appear as a
  binary "until IPC is ready".
- Performance contracts are defined before they are measured. A benchmark
  chosen after the first measurement is a fitted number, not evidence.

---

### Task 1: Fix the isolation and execution boundary

**Files:**
- Create: `docs/adr/0048-stage3-isolation-and-execution-boundary.md`
- Modify: `PROGRESS.md`

- [x] State where TOS Core executes relative to the isolation boundary, and why
  the alternatives were rejected on architecture rather than on convenience.
- [x] State the consequences that follow and cannot be revisited quietly: the
  engine becomes a per-process derived artifact with its own identity, the
  verifier stops being the isolation mechanism, and the identity plane must name
  who asserts each field.
- [x] State exactly what Stage 3 authorization does and does not cover.

### Task 2: Extend the exception baseline to interrupts and preemption

**Files:**
- Create: `docs/adr/0049-stage3-interrupt-and-preemption-baseline.md`

- [x] Record that ADR-0023 deliberately left maskable interrupts disabled, and
  what changes when they are enabled.
- [x] Fix the timer source, the preemption model, the interrupt-safety rules for
  the nucleus, and the negative tests that must exist before the first
  preemptive schedule.

### Task 3: Extend the memory grant to many processes

**Files:**
- Create: `docs/adr/0050-per-process-memory-grants.md`

- [x] Keep ADR-0041's property — a runtime with no grant has no memory — while
  admitting more than one runtime.
- [x] Fix the frame allocator's ownership, the per-process grant derivation, and
  what happens to a grant when its process dies.

### Task 4: Resolve the service manifest surface

**Files:**
- Create: `docs/adr/0051-service-manifest-surface.md`
- Modify: `docs/11_DRIVER_MODEL.md`
- Modify: `docs/45_SYSTEM_SOURCE_HIERARCHY.md`

- [x] Report the contradiction between docs/11's `manifest` block, docs/45's
  "declared inside its own module source", and the accepted V1 grammar, which
  admits no `manifest` item.
- [x] Fix the Stage 3 manifest by splitting it along authority: what a module
  needs stays in accepted V1 source, what it offers becomes a capability
  request, and how it is supervised becomes the supervisor's policy source.
- [x] Restate docs/11's example in accepted V1 form and narrow docs/45's
  sentence to what remains true. *(done on acceptance, 2026-08-12.)*

### Task 5: Publish the nucleus/process system ABI

**Files:**
- Create: `source/interfaces/system/SYSTEM_ABI_V1.md`

- [x] Entry mechanism, register convention, operation numbering, argument and
  result encoding, error space, and the rule that no operation may block
  without a declared cancellation path.
- [x] The complete Stage 3 operation set, with each operation's required
  capability. An operation reachable without a capability is a design defect,
  not a convenience.
- [x] Versioning, extension and refusal rules, and the negative tests a
  conforming implementation must pass.

### Task 6: Publish the capability contract

**Files:**
- Create: `source/interfaces/system/CAPABILITY_V1.md`

- [x] Handle representation, per-process table ownership, and why a handle
  cannot be guessed, forged, encoded into bits or recovered after consumption.
- [x] Rights, object scope, lifetime, attenuation (output rights a subset of
  input), delegation, revocation and linear transfer.
- [x] Constant-time validation with respect to the holder's capability count, or
  the documented alternative bound docs/35 admits.
- [x] The denial and confused-deputy tests docs/37 requires as Stage 3 evidence.

### Task 7: Publish the IPC contract

**Files:**
- Create: `source/interfaces/system/IPC_V1.md`

- [x] Endpoint model, message shape, maximum inline size, capability transfer,
  shared-region transfer, bounded queues and explicit backpressure.
- [x] The docs/35 Stage 3 budgets restated as implementation obligations: no
  nucleus allocation on the fast path, at most two copies inline, at most four
  boundary crossings per request/reply, large payloads by region rather than by
  copy.
- [x] The exact in-process function-call benchmark the relative budget is
  measured against, defined before any measurement exists.

### Task 8: Publish the process identity contract

**Files:**
- Create: `source/interfaces/system/PROCESS_IDENTITY_V1.md`

- [x] The docs/10 identity fields, and for each one **who asserts it**: the
  launcher, the nucleus, or the process itself. Self-reported introspection is
  labelled as such and is never the audit record.
- [x] How identity survives restart, and what a restart generation means.
- [x] The observability events that carry it, as an extension of the delegated
  `TOS.RUN.*` vocabulary rather than a second, competing one.

### Task 9: Cover the new boundary in the threat model and the budgets

**Files:**
- Modify: `docs/34_THREAT_MODEL.md`
- Modify: `docs/35_PERFORMANCE_CONTRACTS.md`

- [x] Stage 3 threat entries with adversary powers, required responses, negative
  tests and honest evidence levels, written against the existing T0–T9 classes
  and A1–A9 assets. docs/34's own change rule forbids closing a stage whose new
  boundary has no threat entry.
- [x] The in-process function-call benchmark defined in `IPC_V1` §8, before any
  measurement exists, so the relative IPC budget cannot be satisfied by choosing
  a slow denominator.
- [x] Merged into the accepted documents on signature, 2026-08-12. The draft
  `docs/evidence/STAGE3_THREAT_ENTRIES.md` was removed rather than kept beside
  the accepted text: two copies of one normative statement drift.

### Task 10: Record the phase

**Files:**
- Modify: `PROGRESS.md`
- Regenerate: `TOS_DEVELOPMENT_SPECIFICATION.md`, `MANIFEST.txt`, `SHA256SUMS`

- [x] Record what is Proposed, what is measured, what is still unknown, and the
  single decision required before Phase 1 begins.
- [x] Keep the generated artifacts current at the publishing commit.

---

## What Phase 1 is, and why it is not in this plan

Phase 1 is multi-module source sets: real import resolution inside the capsule,
a dependency digest over the actual resolved list, cache identity for a closure
and a measured arena bound for more than one module. It implements accepted
contracts (docs/42, ADR-0038, ADR-0044) and needs no new decision, but it is
Stage 3 work: a service is a separate module, and until a module can import
another one, there is nothing for a supervisor to launch.

Phase 1 also carries one gap measured while writing this plan. A module-level
`const` — an accepted V1 item form — is parsed and type-checked today and then
dropped: reading it from a function refuses at lowering with
`construct=unbound place`. No Stage 3 contract depends on it (ADR-0051 §
"Consequence" explains why the design that would have depended on it was
rejected), but an accepted declaration form that silently disappears is a defect
in Stage 2's completeness and belongs in Phase 1.
