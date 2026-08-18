<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 Phase 2: the isolation boundary and the first process

> **Scope rule:** this phase implements accepted contracts — ADR-0048…0050 and
> `SYSTEM_ABI_V1` — and changes none of them. It does not touch TOS Core V1,
> `tos-ir/v1`, the verifier contract, Boot ABI v1 or the capsule format. If a
> task turns out to need one of those changed, it stops at that boundary and
> says so, the way Phase 1's Task 0 did.

**Goal:** `/system/boot/init.tos` stops being a function the nucleus calls and
becomes a process: its own address space, CPL 3, its own runtime instance, its
own memory grant, reachable to the nucleus only through `SYSTEM_ABI_V1`. Until
that exists there is no isolation to give a service, and a Stage 3 "supervisor"
would be a name for a function call.

**Architecture:** the nucleus keeps mechanism — frames, page tables, the
syscall edge, fault containment — and gives up nothing else. The engine leaves
the nucleus and becomes a per-process artifact, which is the consequence
ADR-0048 says must not be discovered later.

## What was measured before planning

At `82644ec`, by reading the shipping code rather than assuming:

| Fact | Where |
|---|---|
| The nucleus runs on the firmware's page tables. `CR3` is never written, and no page table is built anywhere in `source/` | no `cr3`/PML4 reference exists in loader or nucleus |
| The GDT holds five entries — null, kernel code, kernel data, TSS low, TSS high. There is no user code or data descriptor | `nucleus/src/exception.rs` |
| `TSS.rsp0` is zero. Only `IST[0]` is filled, for `#DF` | `exception.rs`, `install()` |
| No `syscall` entry exists: `EFER.SCE`, `IA32_STAR`, `IA32_LSTAR` and `IA32_FMASK` are never written | nucleus-wide |
| Every exception is fatal and ends the boot with `RESULT_EXCEPTION` | `exception_fatal` |
| There is no frame allocator. One region is derived from the map by subtracting occupied spans, and handed to one heap | `region::derive`, `runtime.rs` |
| The engine executes at CPL 0, on the nucleus stack, out of the nucleus's own heap | `runtime::execute_boot_text` |
| Maskable interrupts have been disabled since the loader (ADR-0023) | `main.rs`, loader handoff |

So Stage 2's "nucleus/runtime boundary" is one of *authority* — enforced by
what each side can name — exactly as `runtime.rs` says in its own header. This
phase makes it a boundary of *hardware*.

## Global constraints

- **The Stage 2 result does not move.** The canonical boot path still reports
  `value=i32:240`, and the module-set path still `i32:42`. A phase that changes
  what the system computes while changing where it computes it cannot say which
  change did what.
- **Nothing is trusted because the verifier accepted it.** After ADR-0048 the
  isolation is the page tables. No test in this phase may justify a safety
  property by citing verification.
- **The identity plane arrives with the first process, not after it.** A
  process that cannot say which source it came from is not a Stage 3 process
  (ADR-0048), so the launch record lands in the same task as the launch.
- **No service is written in Rust because IPC is not ready.** The only ring-3
  code this phase introduces is the runtime image itself, which is a mechanism,
  not a service.
- **Every field of a claim has one asserter** (`PROCESS_IDENTITY_V1` §3). The
  nucleus asserts what it did; the launcher asserts what it passed; a process's
  own words about itself are labelled as such.

## A boundary this phase is expected to reach

`SYSTEM_ABI_V1` §5 has no self-exit operation. A process ends by
`process_terminate`, which requires a process-authority capability *for that
process* — authority the first process does not hold over itself, and which no
supervisor exists to hold. So "init ran to completion" has, today, no
contractual way to be reported.

That is checked in Task 5, not guessed at now. If it holds, the answer is an
ADR proposing how a process's own completion is reported and who observes it —
not an operation invented at the edge because the boot needed one.

---

### Task 1: The nucleus owns physical frames — **done (2026-08-17)**

**Files:**
- Create: `source/crates/tos-frames/{Cargo.toml,src/lib.rs}`
- Modify: `source/nucleus/src/runtime.rs`, `source/nucleus/src/main.rs`
- Modify: `source/Cargo.toml`

- [x] A frame allocator built from the validated memory map minus every
  occupied span — the same subtraction `region::derive` performs today, which
  does not weaken because memory a process could write over is not protected by
  one component's bookkeeping being right (ADR-0050 §1).
- [x] 4 KiB frames, allocated and released one at a time, plus contiguous
  carving for a region a grant backs. A released frame is cleared before it can
  back anything else (ADR-0050 §3), and the test that proves it writes a
  pattern, releases, reallocates and reads.
- [x] The Stage 2 heap grant becomes an allocation from the pool rather than
  the whole largest hole. It stays a **V1** grant: ADR-0050 §2 keeps V1 valid
  for a nucleus granting to one runtime, and V2's `owner`/`generation` arrive
  with the process that needs them.
- [x] Hosted unit tests over a real backing buffer, because an allocator tested
  against arithmetic only is tested against its own assumptions.
- [x] The boot path is unchanged in what it computes and in what it reports:
  `value=i32:240`, peak below the grant.

### Task 2: The nucleus owns its address space — **done (2026-08-17)**

**Files:**
- Create: `source/nucleus/src/paging.rs`
- Modify: `source/nucleus/src/main.rs`

- [x] Page tables built from the frame allocator and `CR3` loaded with them,
  replacing the firmware's map. Every mapping the nucleus needs is derived from
  the *validated* memory map, never from a firmware table read at that moment.
- [x] Supervisor-only (`U/S` clear) everywhere, `NX` on everything that is not
  nucleus text, and nucleus text mapped read-only. A mapping that is both
  writable and executable is a defect, not a convenience.
- [x] Negative evidence: a read of an address deliberately left unmapped faults
  as a page fault with the expected `CR2`, and the existing fatal path reports
  it. The gate is the existing exception-injection harness extended, not a new
  vocabulary.
- [x] The boot is otherwise identical, framebuffer included — a console that
  goes dark when the nucleus takes over paging means the framebuffer was mapped
  by luck.

### Task 3: Ring 3 exists — **done (2026-08-17)**

**Files:**
- Modify: `source/nucleus/src/exception.rs` (GDT/TSS), `source/nucleus/src/paging.rs`
- Create: `source/nucleus/src/syscall.rs`, `source/nucleus/src/syscall.S`

- [x] The GDT gains user code and data descriptors in the layout `sysret`
  requires, and `TSS.rsp0` names a nucleus stack. The existing kernel selectors
  keep their values so the Stage 1 exception evidence still describes this GDT.
- [x] `EFER.SCE`, `IA32_STAR`, `IA32_LSTAR`, `IA32_FMASK` are programmed to the
  `SYSTEM_ABI_V1` §3 entry: `syscall`/`sysret`, selector in `rax`, six
  arguments, status in `rax` and value in `rdx`, `rcx`/`r11` clobbered and
  every other register preserved. `int 0x80` remains not an entry.
- [x] An unknown operation number returns `E_NOT_SUPPORTED` and leaves the
  caller runnable (§7). Silence is indistinguishable from success and is
  therefore a defect.
- [x] The register-preservation rule is tested the way §8.6 asks: a ring-3
  caller fills every preserved register, makes the call, and compares.

### Task 4: The runtime is a per-process artifact — **stopped at a boundary (2026-08-17)**

> Carrying the image needs a decision no implementation may take quietly:
> ADR-0053 (Proposed). The predicted completion boundary is confirmed and filed
> as ADR-0054 (Proposed). Neither is implemented while its approval line is
> empty.

**Files:**
- Create: `source/runtime-image/` (a `no_std` ring-3 binary over `tos-pipeline`)
- Modify: `source/host-tools/capsule/…`, `source/nucleus/src/…`

- [ ] The engine and the pipeline are built into a ring-3 image with its own
  identity, carried and named as a derived artifact under AGENTS.md §9 —
  ADR-0048's "the engine becomes a per-process derived artifact" made literal.
- [ ] The image receives its module bytes and its grant from the launcher; it
  discovers nothing. `GlobalHeap` still refuses every allocation until a grant
  is adopted, which is ADR-0041's property surviving the move to ring 3.
- [ ] Where the image is carried is decided by what the capsule format and its
  provenance gates already admit. If carrying a binary artifact needs the format
  changed, that is a boundary and this task stops at it.

### Task 5: The first process

**Files:**
- Create: `source/nucleus/src/process.rs`
- Modify: `source/nucleus/src/main.rs`, `source/nucleus/src/runtime.rs`

- [ ] An address space, a grant (V2: `owner`, `generation`), a user stack, the
  runtime image mapped, and entry at CPL 3. The nucleus's Stage 2 in-process
  call is replaced by a launch, not kept beside it as a fallback.
- [ ] The launch record is emitted with the process, under `TOS.RUN.*` as
  `PROCESS_IDENTITY_V1` §6 requires, with each field attributed to its asserter.
  `system commit id` is **absent**, because Stage 3 reads no repository, and a
  test asserts its absence so a later stage cannot make it present by accident.
- [ ] The boot module runs to completion inside the process and returns
  `value=i32:240` — the Stage 2 answer, computed on the far side of the
  boundary. How completion is reported is the open boundary named above.

### Task 6: A fault kills the process, not the system

**Files:**
- Modify: `source/nucleus/src/exception.rs`, `source/nucleus/src/process.rs`
- Create: `source/host-tools/qemu-test/process-isolation.sh`

- [ ] A fault taken at CPL 3 terminates that process and is attributed to it;
  a fault at CPL 0 still ends the boot with `RESULT_EXCEPTION` (ADR-0049 §3).
- [ ] Negative gates, each a separate scenario: a process writing nucleus
  memory, a process reading another mapping it was not granted, and a process
  executing a privileged instruction. Each is refused by hardware and reported,
  and none of them is prevented by the verifier.
- [ ] The grant of a dead process is reclaimed and its frames are cleared
  before reuse (ADR-0050 §3), proven by the pattern test of Task 1 applied
  across a process death rather than across a free.
