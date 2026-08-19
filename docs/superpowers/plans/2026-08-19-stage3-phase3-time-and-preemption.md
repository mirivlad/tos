<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 Phase 3: time, preemption, and more than one process

> **Scope rule:** this phase implements ADR-0049, which is accepted. It changes
> no contract that is not already delegated to it. If a task needs one changed,
> it stops there and says so.

**Goal:** the system has a monotonic tick, a process that never calls anything
is still interrupted, and two processes each make progress without either
yielding. Until then `time_monotonic` has nothing true to return and fuel is
still doing a job ADR-0048 says it must not do.

## What was measured before planning

At `5edab35`, by reading the shipping code:

| Fact | Where |
|---|---|
| Maskable interrupts have been disabled since the loader; nothing ever executes `sti` | loader handoff, `nucleus/src/main.rs` |
| The IDT holds 32 entries. Vectors above 31 have no gate, so an interrupt on one is already a fault rather than a no-op | `exception.rs`, `EXCEPTION_VECTOR_COUNT` |
| The 8259 PIC is never touched — neither masked nor programmed | nucleus-wide |
| The local APIC is never enabled and its registers are not mapped: `paging::build` maps described memory and the framebuffer, and the LAPIC is neither | `paging.rs` |
| `time_monotonic` is assigned and unimplemented, and says so | `syscall.rs` |
| A process runs to its own end: `process::run` returns only when the process faults or exits | `process.rs` |

## Global constraints

- **The result does not move.** The canonical path still reports `i32:240` and
  the module-set path `i32:42`, with the timer running.
- **Existing evidence is not relabelled.** Stage 1 and Stage 2 numbers were
  measured with interrupts off; a preempted measurement is a different
  measurement, and ADR-0049 says any Stage 3 timing evidence states the quantum
  and whether preemption was active.
- **No external device interrupt is routed.** The first one belongs to Stage 4
  with its own contract, and routing one early to exercise the path would create
  an undocumented driver boundary.
- **A tick is a tick.** Stage 3 claims no wall-clock time and no trusted time
  source; docs/34 assigns time threats to Stage 7.

---

### Task 1: A monotonic tick exists — **done (2026-08-19)**

**Files:**
- Create: `source/nucleus/src/apic.rs`
- Modify: `source/nucleus/src/exception.rs`, `source/nucleus/src/paging.rs`,
  `source/nucleus/src/syscall.rs`, `source/nucleus/src/main.rs`

- [x] The 8259 PIC is masked entirely and the local APIC is enabled, with its
  registers mapped uncacheable — a cached write to a device register is a write
  whose arrival is nobody's promise.
- [x] One timer vector and one spurious vector are claimed above 31. Every other
  vector above 31 stays absent, so an interrupt on one is a fault, which is what
  ADR-0049 §2 asks for and what the current 32-entry IDT already does by
  accident rather than by decision.
- [x] The handler does no allocation, takes no lock and performs no unbounded
  work: it increments a tick and acknowledges. It is the first handler in this
  system that returns.
- [x] `time_monotonic` returns that tick, and stops being the one assigned
  operation this nucleus does not implement.
- [x] Interrupts are enabled once, after the substrate is initialized and before
  the first process is entered.
- [x] Evidence: a boot with the timer running reaches the same result with the
  same events, and the tick observed by a process advances between two calls —
  measured by the process, which is the only party that can observe both.

### Task 2: A process that does not yield is still interrupted — **done (2026-08-19)**

- [x] The timer interrupt taken at CPL 3 returns through `iretq` to the
  interrupted process, which is a resumption the nucleus has never performed:
  every interrupt before this one was fatal.
- [x] A process spinning without a system call is interrupted, proven by a tick
  that advances across a loop making no call at all — 355 to 395 on the
  canonical path — and by the boot completing afterwards.

### Task 3: Two processes make progress — **done (2026-08-19)**

**How the switch works, decided while Task 1 was built and written down here so
that implementing it is not re-deriving it.** The timer stub already saves all
fifteen registers plus the processor's frame, in `TrapFrame` order, and hands
the handler its address. A context switch is therefore two writes and nothing
clever: copy the interrupted frame into the current process's slot, copy the
next runnable slot's frame into the interrupted frame, and load that process's
`CR3`. The `iretq` at the end of the stub then returns into the other process —
the stub does not know it changed its mind, because everything it reads is what
the handler left.

Death is the other direction and already exists: a process that exits or faults
resumes the nucleus's captured context (`process_capture`/`process_resume`), so
the scheduler's loop lives at CPL 0 and can enter any live process by `iretq`
from its saved frame. Nothing needs a second mechanism.

What is missing is the table: a slot holding `root`, `frame`, whether it is
live, and the report region the nucleus drains for it — `REPORT_PHYS` and
`REPORT_LENGTH` become per-slot — plus a `launch` that builds two processes
before entering either.

- [x] More than one process exists at once. `launch` became `create` — building
  a process and entering one are now separate operations — plus a `schedule`
  that runs every runnable slot to its end. The table holds four slots; the
  report region became per-slot, and so did the tick accounting.
- [x] Round-robin over runnable contexts within one priority band, fixed
  quantum, no priorities and no SMP (ADR-0049 §4). The search starts after the
  running slot and wraps; with one runnable process the switch is skipped rather
  than performed onto itself, which is why the canonical path is unchanged.
- [x] Evidence: both processes advance an observable neither yields to give the
  other, and the tick each observes is the same tick — `scheduler.sh`, measured
  from both sides. The nucleus reports each process's `ticks`, `quanta` and
  `[first_tick, last_tick]`; the two intervals overlap, which two processes run
  one after the other cannot produce. The processes report the tick before and
  after a loop that makes **no system call at all**; those two brackets overlap
  too, which needs no knowledge of which process wrote which line. Both return
  `i32:240`.

**What was decided while implementing, and why the second process is
test-gated.** The scheduler, the table and the switch are production code, on
every boot. *How many processes a boot has* is not this nucleus's decision: no
accepted contract says the canonical boot has two, and a nucleus that decided so
on its own would own service policy, which ADR-0048 §2 says it does not. So the
second process lives behind `test-two-processes`, exactly as the ring-3
excursions live behind theirs — the mechanism is real, the policy is not
invented. When a contract does say what the boot's process set is (ADR-0051's
`provides`/`restart` residue, Stage 3's service story), the launcher reads it
instead of a feature flag, and nothing below the launcher changes.

**A fault now happens with a peer.** The ring-3 excursion used to run to its
fault before the first process was built. It is now admitted into the same table
and entered by the same scheduler, so the fault it takes is taken while a peer
exists — and the gates check that the process that faulted is not the process
that finished the work. That is the remaining half of ADR-0049 §3's evidence:
*terminates exactly one process, attributed to it, and leaves its peers
running.*
