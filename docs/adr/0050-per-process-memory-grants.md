<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0050: Per-process memory grants

- Status: **Accepted** (Project Architect-approved)
- Date: 2026-08-12
- Decision level: 2 — extends the accepted `RuntimeMemoryGrantV1` interface to
  more than one runtime, within the boundary ADR-0048 fixes
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-12

## Context

ADR-0041 settled who owns memory in Stage 2 with one sentence worth preserving
exactly: the nucleus grants, and the runtime never discovers. The runtime
"receives a base and a length or it does not run". The implementation carries
that property literally — `GlobalHeap` refuses every allocation until a grant is
adopted, so a runtime with no grant has no memory.

That contract was written for exactly one runtime. Its shape is right and its
property is the one Stage 3 needs; what it lacks is plurality. There is one
global allocator, one adoption, and no owner of physical frames — the region is
derived once from the memory map by subtracting everything that is spoken for.

Stage 3 has many processes, each with its own address space and its own runtime
instance, created and destroyed while the system runs.

## Decision

**The nucleus owns a physical frame allocator. Every process receives its
backing store as a `RuntimeMemoryGrant` derived from that allocator. The
grant contract keeps its shape and its property; what changes is that there is
now more than one grant and that grants are reclaimed.**

### 1. The frame allocator is nucleus-owned and boot-derived

At boot the nucleus takes the same memory topology Stage 1 already validates,
subtracts the same occupied spans the Stage 2 derivation subtracts — its own
image including `.bss`, the capsule, the handoff record, the converted map, the
framebuffer, its own stack — and the remainder becomes the frame allocator's
pool rather than a single region handed to a single runtime.

The subtraction rule does not weaken. Memory a process could write over is not
protected by one component's bookkeeping being correct.

### 2. A grant is per process, and its version says so

`RuntimeMemoryGrantV2` keeps V1's fields — version, base, length, alignment,
nucleus identity — and adds what a plural world needs:

```text
RuntimeMemoryGrantV2 {
  version           the grant contract version
  base              start of the granted region, in the process's address space
  length            bytes granted
  alignment         guaranteed alignment of `base`, a power of two
  identity          which nucleus build produced the grant
  owner             the process instance this grant belongs to
  generation        incremented on reuse of the same physical frames
}
```

`owner` and `generation` exist so that a stale grant is detectably stale. Frames
reused by a later process must not let an old reference be mistaken for a live
one — the same reasoning that made the Stage 2 grant carry a nucleus identity
rather than trusting the caller's word.

V1 is not retracted or reinterpreted. A nucleus that grants to one runtime with
no process substrate is still a V1 grant, and the Stage 2 evidence taken against
it stays valid.

### 3. Death returns memory, and returns it clean

When a process ends — normally, by fault, or by supervisor decision — its grant
is reclaimed by the nucleus. Reclaimed frames are cleared before they back
another process's grant. Uncleared reuse would make one process's data
observable to the next through nothing but timing, which is a disclosure channel
the isolation boundary is supposed to close.

### 4. What a process may ask for, and what it may not

A process's grant size is decided by its launcher from the module's declared
resource envelope and the launcher's policy, not requested by the running
process. Growth, if it is ever admitted, is a capability operation with its own
contract; Stage 3 does not admit it. A process that exhausts its grant fails on
its own declared terms and does not take memory from anyone else.

Shared memory between processes is not a second grant mechanism. It is the
region transfer described by `IPC_V1`, originating from a capability operation
exactly as docs/42 §2 requires of `Region<T>`.

## Consequences

The single `#[global_allocator]` disappears from the nucleus binary as the
system's allocator. It remains the shape of the *per-process runtime's*
allocator, which is where ADR-0041's property belongs; the nucleus keeps its own
bounded, non-allocating discipline in interrupt and IPC paths (ADR-0049, docs/35
§Stage 3).

The measured Stage 2 arena bound stays meaningful for one module in one process
and does not silently become a system-wide claim. A multi-process bound is a new
measurement.

## Evidence required

- A process cannot address a frame outside its grant; the attempt faults and is
  attributed to that process.
- Frames from a dead process are cleared before reuse, demonstrated by writing a
  pattern, ending the process, and reading the frames from its successor.
- A stale grant reference — right base, wrong generation — is refused rather
  than honoured.
- Creating and destroying processes in a loop returns the pool to its initial
  free extent: the substrate does not leak the memory of the dead.
- With no process substrate active, the Stage 2 single-grant path still produces
  its existing `TOS.RUN.MEMORY` evidence unchanged.
