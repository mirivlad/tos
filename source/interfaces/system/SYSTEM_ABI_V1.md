<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS System ABI — Version 1

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0048 (Project Architect-approved, 2026-08-12), which fixes the
boundary this contract describes.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs.

Producer: the nucleus. Consumers: every Stage 3 process, through its runtime.
Companions: `CAPABILITY_V1.md`, `IPC_V1.md`, `PROCESS_IDENTITY_V1.md`.

## 1. Role

ADR-0048 places TOS Core execution at CPL 3 in its own address space. This
contract is the only edge across that boundary: everything a process can ask the
system to do, it asks here.

Boot ABI v1 is the loader-to-nucleus handoff and is unchanged by this document.
`RuntimeMemoryGrant` (ADR-0041, ADR-0050) is how a runtime gets memory and is
also a separate contract with its own version. Three edges, three versions; a
change to one is not a change to the others.

## 2. What this ABI is not

It is not a POSIX-shaped system-call surface, and it is not a place to add
operations because a service needs something. It carries mechanism the nucleus
alone can provide: address spaces, execution contexts, capability handles, IPC
transport, and the small amount of time a scheduler needs. Filesystems, devices,
repositories, networks and consoles are services reached through IPC, not
operations added here. **If an operation could be a service, it is a service.**

## 3. Entry

| Property | Value |
|---|---|
| Mechanism | `syscall` / `sysret` (x86_64), IA32_LSTAR-based |
| Operation selector | `rax` |
| Arguments | `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9` — six, by value |
| Result | `rax` = status, `rdx` = value |
| Clobbered | `rcx`, `r11` by the instruction itself |
| Preserved | every other general-purpose register |
| Flags | interrupts masked on entry, restored on return |

`int 0x80` is not an entry. There is exactly one mechanism, so there is exactly
one path to audit.

Arguments are values and handles, never pointers the nucleus dereferences
without bounds. Where an operation needs a buffer, the buffer is named by a
handle to a region the process already holds (`IPC_V1` §5), so the nucleus never
walks an address a process chose.

**The capability an operation requires is its first argument, `rdi`** (ADR-0055,
ADR-0056). Every operation in §5 that requires one requires exactly one, and it
is always in the same place: a convention is a property of this edge, not of
each operation, and an operation that put its handle elsewhere would make the
dispatcher's first action depend on which operation it was dispatching. Should
an operation ever require two capabilities, this contract assigns their
positions in §5 order when that operation is added.

The three self-only operations — `context_yield`, `time_monotonic`,
`process_exit` — require no capability, and `rdi` carries whatever their own
entry in §5 says it does, or nothing.

## 4. Status space

`rax` returns zero for success or a negative status. The space is small and
closed: an operation returns a status from this table or it is a defect.

| Status | Value | Meaning |
|---|---|---|
| `OK` | `0` | the operation completed |
| `E_NO_CAPABILITY` | `-1` | the handle was absent, wrong type, or lacked the right |
| `E_BAD_HANDLE` | `-2` | the handle index is outside the process's table |
| `E_BAD_ARGUMENT` | `-3` | an argument was outside its declared domain |
| `E_WOULD_BLOCK` | `-4` | a non-blocking operation had nothing to do |
| `E_CANCELLED` | `-5` | a blocking operation was cancelled |
| `E_LIMIT` | `-6` | a declared bound would be exceeded |
| `E_NOT_SUPPORTED` | `-7` | the operation exists in a later version of this ABI |

`E_NO_CAPABILITY` and `E_BAD_HANDLE` are distinct on purpose and must not be
merged for tidiness: the first says the process holds the wrong authority, the
second says it named nothing at all, and an audit log that cannot tell them
apart cannot describe an attack.

**Refusal order** (ADR-0056): index bounds, then generation, then type, then
rights. The first of those that fails decides the status, so an index outside
the caller's table is `E_BAD_HANDLE` and everything after it is
`E_NO_CAPABILITY`. The order is not arbitrary — it is the order
`CAPABILITY_V1` §2 states validity in, and it is the order that makes the status
a fact about the *call* rather than about the caller: "you named nothing" is
answerable from the argument alone, while "you lack the authority" requires
there to be something at that index to lack authority over.

A process whose table is empty therefore receives `E_BAD_HANDLE` from every
capability-bearing operation, at every index. That is not a placeholder for a
better answer later: a process that holds nothing names nothing.

There is no `E_PERMISSION` in the ambient sense. Authority is a handle or it
does not exist.

## 5. Operations

Every operation names the capability it requires. **An operation reachable
without a capability is a design defect, not a convenience.** The two exceptions
are marked and are exactly those a process can only apply to itself.

| Number | Operation | Requires | Effect |
|---|---|---|---|
| 1 | `endpoint_send` | endpoint handle with `send` | `IPC_V1` §3 |
| 2 | `endpoint_receive` | endpoint handle with `receive` | `IPC_V1` §3 |
| 3 | `endpoint_call` | endpoint handle with `call` | request/reply, `IPC_V1` §4 |
| 4 | `endpoint_reply` | reply handle (single use) | `IPC_V1` §4 |
| 5 | `capability_attenuate` | the capability being attenuated | `CAPABILITY_V1` §4 |
| 6 | `capability_release` | the capability being released | consumes the handle |
| 7 | `region_share` | region handle with `share` | `IPC_V1` §5 |
| 8 | `process_create` | process-authority capability | creates a process |
| 9 | `process_terminate` | process-authority capability for that process | ends it |
| 10 | `context_yield` | *(self only)* | gives up the rest of the quantum |
| 11 | `time_monotonic` | *(self only)* | reads the monotonic tick |

Operation `0` is not assigned and never will be. A register that was never
written holds zero, so a zero selector is overwhelmingly likely to be a caller
that forgot to name an operation; giving it a meaning would turn that mistake
into a successful call.

`process_create` is the operation a supervisor holds and an ordinary service
does not. That distinction is the whole of Stage 3's authority story: a process
that cannot create processes cannot escalate by spawning.

## 6. Blocking and cancellation

An operation that can block declares a cancellation path, and cancellation is
observable as `E_CANCELLED` rather than as a value that looks like a result.
No operation blocks indefinitely without one: an unkillable process is an
authority the system cannot revoke.

Blocking is always on a handle the process holds. There is no wait-for-anything
primitive, because it would let a process wait on authority it was never given.

## 7. Versioning

The version is a whole-contract version, reported in process identity
(`PROCESS_IDENTITY_V1` §3). Operation numbers are assigned once and never
reused: a retired operation returns `E_NOT_SUPPORTED` forever rather than being
recycled into a different meaning. The assignment that rule governs is the one
in the §5 table, and the status values are in §4; both were added to this
contract when the first implementation of the edge was written, because a rule
about numbers that never states the numbers cannot be conformed to. Neither is a
new decision: no operation, status, right or guarantee changed, so this is still
version 1.

A process built against a later minor version that calls an unknown operation
receives `E_NOT_SUPPORTED` and is not terminated for asking. A nucleus that
silently ignores an unknown operation is a defect: silence is indistinguishable
from success.

## 8. Conformance evidence

A conforming implementation demonstrates, as automated tests:

1. every operation in §5 refuses with `E_NO_CAPABILITY` when the required
   capability is absent, including when a handle of a *different* type is
   supplied at the same index;
2. an out-of-range handle index yields `E_BAD_HANDLE` and never a fault;
3. no operation dereferences a process-supplied address;
4. an unknown operation number yields `E_NOT_SUPPORTED` and leaves the process
   runnable;
5. a blocking operation returns `E_CANCELLED` when its process is asked to stop,
   and its resources are accounted back;
6. the register-preservation rule holds across every operation, checked by a
   process that fills every preserved register and compares after return;
7. the ABI version a process reports matches the nucleus that served it.
