<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0049: Interrupts and preemption

- Status: **Proposed**
- Date: 2026-08-12
- Decision level: 2 — extends the accepted ADR-0023 exception baseline within
  the boundary ADR-0048 fixes; no Boot ABI v1 layout or result-code change
- Project Architect approval: *(pending)*

## Context

ADR-0023 is explicit about what it is not: "Maskable external interrupts remain
disabled. Vectors above 31 do not form an interrupt ABI in Stage 1, and an
exception is fatal: Stage 1 does not resume with `iretq`." That was the right
Stage 1 and Stage 2 decision. A boot path that validates a capsule and runs one
module to completion has nothing to schedule and nothing to be interrupted for,
and every disabled mechanism is a mechanism that cannot go wrong.

ADR-0048 makes processes preemptible user-mode contexts. That requires the three
things ADR-0023 deliberately withheld: an enabled interrupt source, vectors
above 31 with a stable meaning, and a handler that returns.

## Decision

**Maskable interrupts are enabled once, by the nucleus, after the process
substrate is initialized and before the first process is scheduled. The only
Stage 3 interrupt source is a monotonic timer, and its only Stage 3 purpose is
preemption and timekeeping.**

1. **Controller.** The legacy 8259 PIC is masked entirely. Interrupt routing is
   through the local APIC, using its timer in periodic or TSC-deadline mode; the
   I/O APIC is configured but no external device source is routed in Stage 3,
   because Stage 3 has no drivers. The concrete calibration source and mode are
   implementation choices recorded in the interface contract, not in this ADR.
2. **Vector space.** Vectors 0–31 keep their ADR-0023 meaning exactly. Stage 3
   claims a small, documented range above 31: one timer vector and one spurious
   vector. Every other vector above 31 stays absent, and an interrupt on an
   unclaimed vector is a fault, not a no-op.
3. **Resumption.** Exceptions taken at CPL 0 remain fatal on the ADR-0023 terms
   and keep `RESULT_EXCEPTION`. What becomes resumable is different in kind: an
   interrupt taken at CPL 3 returns through `iretq` to the interrupted process
   or to another one. A fault taken at CPL 3 does not return to the faulting
   process — it terminates that process and the system keeps running. **A fault
   in the nucleus is still the end of the boot; a fault in a process is not.**
4. **Preemption model.** Round-robin over runnable contexts within one priority
   band, with a fixed quantum. Stage 3 does not introduce priorities, deadlines,
   fair-share accounting or an SMP scheduler: one CPU is brought up, and the
   Full-profile SMP path of docs/41 remains a later stage's work.
5. **Nucleus interrupt safety.** The nucleus does not allocate, take a
   blocking lock or perform unbounded work in interrupt context. This is the
   same discipline ADR-0023 imposed on the exception handler, extended to the
   only handler that now returns.
6. **Timekeeping.** The timer establishes a monotonic tick. Stage 3 exposes it
   only as far as a scheduler and a bounded IPC timeout need. Wall-clock time,
   a `system.time.Clock` capability implementation and any notion of a trusted
   time source are out of scope; docs/34 assigns time threats to Stage 7.

## What this deliberately does not do

- No external device interrupt is routed. The first one belongs to Stage 4 with
  its own contract, and routing one early to "test the path" would create an
  undocumented driver boundary.
- No interrupt is visible to a TOS Core module. A process learns about time and
  events through IPC, never by taking a vector.
- Nested interrupts are not enabled. The timer handler runs with interrupts
  masked.

## Consequences

Enabling interrupts changes what "the nucleus is quiescent" means for every
existing measurement. Stage 1 and Stage 2 evidence was gathered with interrupts
off, and a preempted measurement is a different measurement. Any Stage 3 timing
evidence states the quantum and whether preemption was active; existing Stage 1
and Stage 2 numbers are not re-labelled as Stage 3 numbers.

The Stage 1 exception-injection gates keep working unchanged, because they run
before the substrate is initialized and therefore before interrupts are enabled.

## Evidence required

- A boot with the timer enabled and no process running reaches the existing
  result code with the existing serial contract: enabling the mechanism does not
  change the outcome by itself.
- Two runnable processes each make progress without either yielding — measured,
  not asserted, by an observable both processes advance.
- An interrupt on an unclaimed vector above 31 is diagnosed as a fault rather
  than silently ignored.
- A CPL 3 fault terminates exactly one process, is attributed to it in the audit
  record, and leaves its peers running; the same fault at CPL 0 still ends the
  boot with `RESULT_EXCEPTION`.
- A process spinning without a system call is preempted, which is the property
  that makes fuel unnecessary for fairness (ADR-0048).
