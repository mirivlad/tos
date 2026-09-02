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
positions in §5 order when that operation is added. Operation 13 is the first,
and its row assigns `rdi` and `rsi`; its values then start at `rdx`.

The three self-only operations — `context_yield`, `time_monotonic`,
`process_exit` — require no capability, and `rdi` carries whatever their own
entry in §5 says it does, or nothing.

**An operation that can block takes a flag register, and blocking is the
default** (ADR-0059). Bit 0 means *do not wait*: a call that would have blocked
is answered `E_WOULD_BLOCK` or `E_LIMIT` instead. The default is blocking
because that is what `IPC_V1` describes — §4's `endpoint_call` "sends and
blocks", §7's sender blocks "with a cancellation path" unless it asked not to —
and because a process that must poll to make progress spends every turn it is
given on asking again. Which register carries the flags is fixed per operation
in §5, after that operation's own values.

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
| 1 | `endpoint_send` | endpoint handle with `send` | `IPC_V1` §3; `rsi` = payload length, `rdx` = flags, `r10` = how many transferred **capabilities**, `r8` = how many transferred **regions**. The handles of both are in the argument region at `MESSAGE_CAPABILITIES` and `MESSAGE_REGIONS` (ADR-0058) |
| 2 | `endpoint_receive` | endpoint handle with `receive` | `IPC_V1` §3; `rsi` = flags, and the length taken is returned in `rdx`. The receiver's own handles are written to the same two areas of its own argument region, with unfilled entries zeroed |
| 3 | `endpoint_call` | endpoint handle with `call` | request/reply, `IPC_V1` §4. `rsi` = payload length, `r10` and `r8` as for `endpoint_send`; one capability place is spoken for by the answer, so a call carries at most three of its own |
| 4 | `endpoint_reply` | reply handle (single use) | `IPC_V1` §4 |
| 5 | `capability_attenuate` | the capability being attenuated | `CAPABILITY_V1` §4 |
| 6 | `capability_release` | the capability being released | consumes the handle |
| 7 | `region_share` | region handle with `share` | `IPC_V1` §5, ADR-0037 §4. **Consuming**: the immutable affine region it names becomes a shared one, the handle presented goes stale, and `rdx` returns a new handle to the same region carrying `read` alone. The caller's mapping does not move — same backing, same address, still read-only. No exclusive holder remains, so a further capability naming it may be made by operation 5 and another process may be given one |
| 8 | `process_create` | — | **retired; always `E_NOT_SUPPORTED`** (ADR-0076 §4). It created a process funded from the boot's accounting anchor with no caller presenting a `MemoryAuthority`, which is the ambient spending that decision retires. Replaced by 19. The number stays assigned forever and is never reused |
| 9 | `process_terminate` | process-authority capability for that process | ends it |
| 10 | `context_yield` | *(self only)* | gives up the rest of the quantum |
| 11 | `time_monotonic` | *(self only)* | reads the monotonic tick |
| 12 | `process_exit` | *(self only)* | ends the calling process. `rdi` = the status it claims for itself; does not return (ADR-0054) |
| 13 | `endpoint_reply_receive` | **two**: `rdi` = reply handle (single use), `rsi` = endpoint handle with `receive` | answers the call the reply names, then waits for the next message on the endpoint, without returning to CPL 3 in between. `rdx` = the answer's length, `r10` = flags. The length taken is returned in `rdx`, as for `endpoint_receive` (ADR-0063) |
| 14 | `process_wait_child` | process capability with `wait_child` | the earliest pending ending among that process object's **direct children** (ADR-0067). `rsi` = flags. `rdx` returns the ended child's instance id, and the record is written to the caller's argument region at `WAIT_CHILD_RECORD`. Blocks with no pending ending; `E_WOULD_BLOCK` when asked not to; `E_CANCELLED` when the relation it watches ends or the liveness rule fires |
| 15 | `process_create_with_generation` | — | **retired; always `E_NOT_SUPPORTED`**, with 8 and for the same reason. Its one addition — the supervisor-asserted restart generation of ADR-0067 — is carried by 19 in `CreateFundedRecord`. The number stays assigned forever and is never reused |

| 16 | `capability_attenuate_scoped` | memory-authority capability with `spend` | reserves `rsi` bytes of it as a **child** authority and returns a capability naming that child. The parent's remaining amount falls by exactly what the child may spend; no physical memory moves, and the pool is untouched (ADR-0076 §2b). `E_BAD_ARGUMENT` for a size no budget could serve, `E_LIMIT` for one this budget cannot |

| 17 | `region_allocate` | memory-authority capability with `spend` | allocates `rsi` bytes of region backing out of it, maps it into the caller writable and not executable, and returns an **affine** region capability in `rdx` with `read | write`. The region's base and its **charged and mapped** length — the request rounded up to whole frames — are written to the caller's argument region at `REGION_ALLOCATE_RECORD`. The nucleus chooses the address; a caller never supplies one. `E_BAD_ARGUMENT` for a size no budget could serve, `E_LIMIT` for one this budget cannot |

| 18 | `region_freeze` | mutable region capability with `write` | the consuming mutable-to-immutable transition (ADR-0075 §3). The caller's writable window becomes read-only **in place** — same address, same backing, still not executable — the region becomes permanently immutable, the handle presented goes stale, and `rdx` returns a new handle to the same region carrying `read | share`. Base and length do not change and are not reported again. Nothing physical moves and nothing is charged. A failure before the transition leaves the region completely mutable; there is no half-frozen state |

| 19 | `process_create_funded` | **two**: `rdi` = process-authority capability with `create`, `rsi` = memory-authority capability with `spend` | creates a process and charges its **whole** user-memory footprint to that authority (ADR-0076 §3). `rdx` = the module path's length, `r10` = how many capabilities the child is endowed with, `r8` = the rights the child holds over itself, `r9` = the runtime arena it asks for. The path, the endowment and the child's self-binding are in the argument region at `CREATE_MODULE`, `CREATE_ENDOWMENT` and `CREATE_SELF_BINDING`; the **optional** restart generation is at `CREATE_FUNDED_RECORD`. `rdx` returns the child's capability handle and its instance id is written to `CREATE_INSTANCE_ID`. `E_BAD_ARGUMENT` for an arena outside the accepted `RuntimeMemoryGrant` domain or a non-canonical restart record, `E_LIMIT` for a footprint this authority cannot pay for |

**Operation 19 replaces 8 and 15 together, and that is why the restart
generation moved out of a register.** Operation 8 asserted no restart generation
and 15 asserted one, so the operation that replaces both has to keep *absent*
and *present and equal to zero* apart — `PROCESS_IDENTITY_V1` §5's rule that
absence is the true value and a zero would be a claim nobody made. A register
cannot carry that difference, so `CreateFundedRecord` carries a generation and a
flag, with one canonical encoding: when the flag is clear the field must be
zero, and every other flag bit must be zero in this contract version.

**The memory authority is presented, not consumed, and nothing is inherited.**
A creation places a charge against an accounting node; the capability is
untouched and the child receives no name for it. A parent that means its child
to spend from the same node names that capability in `CREATE_ENDOWMENT` like any
other, which under ADR-0076 §2b gives the child another name for one budget
rather than a second reservation. There is no automatic remainder for a funded
child.

**The arena is a policy figure the caller states, and never a share of what is
free.** `r9` is validated against the accepted `RuntimeMemoryGrant` bounds and
rounded up to whole frames, and what is charged is the rounded figure. There is
no `min(requested, available)`, no "what is left" and no percentage of a parent:
a size chosen from the pool is the second counter ADR-0076 §1 describes. What
the authority cannot pay for is `E_LIMIT`; what no authority could ever pay for
is `E_BAD_ARGUMENT`.

**The charge is the whole footprint and not the arena alone.** Writable data,
the rounded arena, the stack, the report region, the argument region and the
launch record are all charged to the presented authority before a frame moves.
Page tables stay outside the tree, in the proved reserve (ADR-0076 §2).

**Two capabilities, and the first failure decides.** §3 assigns their positions;
the refusal order *between* them is the row's own order, so a caller that does
not hold authority to create is answered from `rdi` alone and learns nothing
about the authority it also named. Within each handle the order is §4's: index
bounds, generation, type, rights.

**A region capability is affine while the region is.** Exactly one names a
mutable or immutable region, so operation 5 refuses to refine one — refinement
does not consume its input and could only add a second — and neither an
endowment nor a delegation carries one, because both copy. An immutable region
travels by the **linear** transfer of `IPC_V1` §5, in a message's region area
and never in its capability area.

**A shared region is the one copyable form**, and operation 7 is the only way
to reach it. After it, operation 5 may make a further name in the same process
and a message may carry it to another without the sender losing anything. A
process holding several such handles still has exactly one mapping of the
region, at one address, and that mapping goes when the last of those handles
does.

**Both forms describe themselves as `OBJECT_REGION`.** The distinction is
structural — whether a second holder may exist — and a process learns what it
may do from the rights it was granted, which is where `CAPABILITY_V1` §3 puts
that question. The public object-kind space is not widened by it.

**Operations 18 and 7 are consuming, and neither reuses the handle it was
given.** Changing an entry's rights under a numeric handle the caller already
holds would leave a process unable to tell a frozen region from one it wrote a
moment ago. So each keeps the capability's table slot and advances its
generation: the presented handle is stale by exactly the rule that makes any
released handle stale (`CAPABILITY_V1` §2), the returned one is the only live
name, and the region's own reference count is one throughout — it never passes
through zero, and no second table slot is required.

**The two message bounds are separate and are refused separately.** A count in
`r10` above four capabilities, or in `r8` above two regions, is
`E_BAD_ARGUMENT` and the whole message is unchanged: the numbers are constants
of `IPC_V1` §3 that the caller knew before it called, which is not the
"retry later" that `E_LIMIT` means. A message naming a **mutable** region is
refused whole for the same reason — `Region<mut T>` is neither shareable nor
transferable — and so is one naming the same affine region twice, because a
linear object cannot be consumed twice.

**Operation 16 is not operation 5 with an amount.** Generic attenuation (5)
refines rights and returns another *name* for the same authority, spending from
the same remainder; there is no path through it to a smaller amount, because the
amount is not in the capability. Scoped attenuation makes a new accounting node
and moves budget down into it. One changes what everybody else may spend and the
other does not, which is why they are two operations rather than two spellings
of one (`CAPABILITY_V1` §3).

Operation `0` is not assigned and never will be. A register that was never
written holds zero, so a zero selector is overwhelmingly likely to be a caller
that forgot to name an operation; giving it a meaning would turn that mistake
into a successful call.

A module is named by **path**, never by an ordinal. An ordinal fits a register,
which is its only advantage; it is a position in a list nobody published, and two
boots whose capsules differ would give the same one to different modules.

Every entry of the endowment names a capability the **parent** holds and the
rights it wants the child to have; what the child gets is the intersection, so a
parent cannot give what it does not hold and widening is unexpressible rather
than refused. An entry that does not resolve refuses the whole creation: a child
half-endowed would hold authority nobody decided to give it. The rights a child
holds over *itself* cannot be one of those entries — they name capabilities the
parent holds, and this one names a process that does not exist until the instant
it is granted — so they travel in a register of their own, bounded by the
authority the parent used.

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

**Two parties can cancel a wait** (ADR-0059). One is a process holding
authority over the waiting process, which is `process_terminate` and ends it.
The other is the nucleus, and its rule is not a duration: when no context is
runnable and some context is blocked, **and nothing routed can change that**,
every block is cancelled at that instant and the nucleus records who was blocked
on what. `E_CANCELLED` is exact there rather than approximate — the operation
was cancelled, and the canceller is the nucleus.

The second half of that condition is load-bearing and is stated rather than
implied: in a stage that routes no device interrupt, "no runnable context" and
"a state nothing can leave" are the same thing, and in a stage that routes one
they are not. An implementation whose rule reads only "nothing runnable" becomes
wrong at the moment a driver exists.

How long *a particular process* may wait is not this contract's question and not
the nucleus's. It is a decision about a component, of the same class as restart
policy, and it belongs to whoever has the authority to launch that component.

## 7. Versioning

The version is a whole-contract version, reported in process identity
(`PROCESS_IDENTITY_V1` §3). Operation numbers are assigned once and never
reused: a retired operation returns `E_NOT_SUPPORTED` forever rather than being
recycled into a different meaning. The assignment that rule governs is the one
in the §5 table, and the status values are in §4; both were added to this
contract when the first implementation of the edge was written, because a rule
about numbers that never states the numbers cannot be conformed to. Neither was a
new decision: no operation, status, right or guarantee changed by writing them
down.

**Retirement is what §7 was written for, and 8 and 15 are the first to use it.**
A retired operation returns `E_NOT_SUPPORTED` forever rather than being recycled
into a different meaning, and its number is never reassigned. The status is the
right one rather than a convenient one: `E_NOT_SUPPORTED` says "the operation
exists in another version of this ABI", which is exactly true of an operation
this version has removed. A process built against the earlier set is told so and
is not terminated for asking. Neither is examined before it is refused — a
refusal that first resolved a handle would be reporting on authority for an
operation that is not there to need it.

Operations 12 (`process_exit`, ADR-0054), 13 (`endpoint_reply_receive`,
ADR-0063), 14 (`process_wait_child`) and 15 (`process_create_with_generation`,
both ADR-0067) **are** additions, and each was decided by an ADR rather than
here — this table carries those decisions rather than making them. So are 16
and 17 (ADR-0076), and so is 18: ADR-0075 §3 decided the consuming
mutable-to-immutable transition and deliberately left "the name, the operation
number and the register shape" open, and this table is where a decision of that
shape is carried. Operation 7 was assigned before any of them and is not an
addition; what changed is that its row now states the semantics ADR-0037 §4 and
`IPC_V1` §5 already fixed for it.

**The capability and region counts in `r10` and `r8` are a minor version by the
rule above, and are safe for exactly the reason `r8` was safe on operation 15.**
A process built against the earlier set leaves both registers uninitialised, so
neither may be read from a caller that did not write it. `r10` was already the
capability count on both operations before this revision and is unchanged; `r8`
is new on them and was previously unassigned, and a nucleus reading it from an
older caller would be reading a register nobody wrote. Callers of this contract
version write both, including zero. `r9` remains unused by every operation.

Nothing is widened on `endpoint_reply` (4) or `endpoint_reply_receive` (13). No
accepted document requires a reply to carry a capability or a region:
`IPC_V1` §4 describes the answer to a call and states only that a reply
capability is single-use, and §5's region transfer is stated over the message
path. A reply that transferred authority would need its own preflight, its own
bound and its own refusal semantics, and inventing them here would be this
contract making a decision rather than carrying one.

**Operation 8 is unchanged by the addition of 15**, and the reason is the rule
in this section rather than caution: a process built against the earlier set
does not initialise `r8`, so reading a restart generation from it would read a
register nobody wrote, and `rdx` already carries the child's capability handle
on return. A number was cheaper than a compatibility break. A child created by
operation 8 therefore has **no** restart generation — not zero, which would be
a claim its caller never made. Each is a minor
version of this contract by the rule above: a process built against the
earlier set calls nothing that has changed meaning, and one built against this
set that runs on an older nucleus receives `E_NOT_SUPPORTED` for 12 and is not
terminated for asking.

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
