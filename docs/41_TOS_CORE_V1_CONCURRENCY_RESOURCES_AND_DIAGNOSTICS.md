<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — concurrency, resources, and diagnostics

- Status: **Proposed Stage 2 contract — not implementation authority**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md` and
  `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`

## 1. Execution contexts and profiles

TOS Core distinguishes three mechanisms:

1. an **asynchronous task** waits for an explicit typed event/I/O contract and
   need not occupy a CPU while suspended;
2. a **parallel task** is independent language-level work that may execute
   simultaneously with sibling work on several cores; and
3. a low-level **execution context** is a nucleus/runtime resource, never an
   ambient language thread API.

The Full profile MUST have a production-capable path that executes independent
parallel tasks from one process simultaneously on multiple cores sharing that
process address space. Separate processes, IPC serialization, or manual queues
are not required merely to use multiple cores. The Bootstrap profile is a
strict subset of the same source/type/ownership/effect semantics. It MAY run
all parallel scopes serially on one worker and has no asynchronous I/O task
operation. Thus a valid Bootstrap parallel computation has the same permitted
logical result under a Full runtime; only timing/overlap differ.

Neither profile promises deterministic scheduling. Correctness MUST NOT depend
on CPU number, worker count, task execution order, or topology. A deterministic
computation whose effects are properly synchronized has the same logical
result on one, two, or N workers. Operations whose result depends on external
typed events, race-to-select, or cancellation expose that nondeterminism in
their result contracts rather than silently changing ordinary memory semantics.

## 2. Structured tasks, join, and cancellation

`parallel { ... }` creates a lexical task scope. `spawn parallel { ... }`
inside it creates a child `Task<T>` owned by that scope. The child owns or
immutably shares exactly the values captured under docs/40. Every spawned
child MUST ultimately be joined/consumed before scope exit. A child body is its
own return scope and uses an explicit `return` to produce `T`; reaching its end
produces only `unit`.
A child cannot
outlive its scope, become detached, or outlive its source/capability/resource
record. Leaving a scope with an unconsumed task is `E1401_UNJOINED_TASK`.

`join Task<T> -> TaskResult<T>` waits for a child and consumes its handle.
It establishes happens-before from all child actions before completion to
actions after the join. A normal child result becomes `Completed(value)`. A
child whose result type is `Result<T,E>` therefore joins as
`TaskResult<Result<T,E>>`: `Completed(Err(e))` preserves its ordinary error,
and `Cancelled` records task cancellation. There is no implicit conversion
between these two outcomes and no cancellation trap.

`cancel task;` is an idempotent cooperative cancellation request and consumes
no ownership. **cancel alone does not discharge** the task-scope obligation:
the parent still joins and thereby consumes the cancelled task handle. If the
child has already reached normal completion, cancellation has no effect and
join returns `Completed(value)`; otherwise a child that observes the request
at a defined safe point completes as `Cancelled`. The runtime delivers
cancellation only at task creation, explicit cancellation check, `await`,
`join`, channel/event wait, loop back edge, and other verifier-visible bounded
safe points. A task that reaches a safe point after cancellation runs its
registered `defer`/bounded drop cleanup, releases its resource reservation,
and may not start new child tasks after cancellation is observed.

Full-profile `spawn async` is also scoped and produces a `Task<T>`; `await
Task<T> -> TaskResult<T>` consumes it with the same lifecycle as `join`.
Its suspension points are explicit `await` calls to typed runtime contracts.
It does not promise a dedicated worker. A V1 task cannot be detached. Future
unscoped execution requires a new language version and an explicit supervisor,
resource, cancellation, and provenance contract.

## 3. Safe shared-memory rule

Two conflicting accesses to the same non-atomic location, at least one a write,
are a data race unless ordered by happens-before. Safe well-typed TOS Core
cannot construct such a race: affine ownership and borrow rules deny a second
mutable alias; immutable `Shared<T>` grants no mutation; mutable shared state
requires a typed synchronization or atomic contract; tasks may not capture a
mutable borrow. A frontend reports the earliest applicable ownership/capture
error; a verifier rejects forged IR that would violate it. A safe data race is
therefore never undefined behavior, arbitrary memory corruption, or a
backend-dependent outcome.

There is no safe "best effort" race detector mode. An unsafe operation must
preserve the safe caller guarantee stated in docs/40. An execution engine that
cannot implement a stated atomic/happens-before rule must reject the module;
it cannot silently substitute host semantics.

## 4. Synchronization and happens-before

The standard/runtime contracts below are typed, resource-accounted, and
verifier-visible. A future library may add convenience APIs only when it maps
to one of these contracts or a later accepted version.

| Contract | Safe use and ordering |
|---|---|
| `Mutex<T>` | `lock` grants an affine mutable guard; `unlock` releases it. An unlock synchronizes-with the next successful lock of the same mutex. A guard cannot await, cross a task boundary, or be dropped after its lock resource disappears. |
| `RwLock<T>` | Multiple immutable read guards or one affine write guard. Releasing a write guard synchronizes-with a later successful read/write acquisition. Upgrade is not implicit. |
| `Channel<T>` | Sending consumes/transfers `T`; receiving obtains it once. A completed send synchronizes-with the receive of that message. Closing is explicit and receives then return `Err(ChannelClosed)`. |
| `Event` / `Semaphore` | `signal` synchronizes-with a successful `wait` that observes that signal. V1 `Event` is binary/coalescing; `Semaphore` has a declared nonnegative permit count, `release(n)` adds permits within its resource bound, and each successful `acquire` consumes one permit. |
| `Barrier` / `Latch` | A successful barrier generation orders every participant's pre-barrier actions before every participant's post-barrier actions. A latch opens after its declared nonzero count reaches zero and then orders decrements before waiters. |
| task spawn/join | Capture initialization is sequenced-before child entry; child completion is happens-before successful join. |
| cancellation | Cancellation request is visible at a defined safe point. All cleanup/completion actions happen-before the join that observes cancellation. |

An engine MAY serialize any of these operations when that preserves the same
allowed result. It MUST still enforce the lock/guard/resource rules and must
not treat serialized execution as permission for a source program with an
illegal mutable alias.

## 5. Atomics and memory order

V1 provides `AtomicBool`, `AtomicU32`, and `AtomicU64`; all are naturally
aligned opaque objects, never raw integer aliases. They expose:

```text
load(order) -> T
store(value, order) -> unit
swap(value, order) -> T
fetch_add/sub/and/or/xor(value, order) -> T     # integer atomics only
compare_exchange(expected, desired, success, failure) -> Result<T, T>
```

The only order values are `Relaxed`, `Acquire`, `Release`, `AcqRel`, and
`SeqCst`. A load accepts `Relaxed`, `Acquire`, or `SeqCst`; a store accepts
`Relaxed`, `Release`, or `SeqCst`; read-modify-write accepts all; the failure
order of `compare_exchange` accepts `Relaxed`, `Acquire`, or `SeqCst` and may
not be stronger than success. An invalid order is `E1410_INVALID_ATOMIC_ORDER`.

`Relaxed` orders only the atomic modification order of that object. A release
operation synchronizes-with an acquire operation that reads its value or a
later release sequence value. `AcqRel` has both effects. `SeqCst` operations
also participate in one total order consistent with happens-before and each
atomic's modification order. Ordinary reads/writes sequenced-before a release
become visible to ordinary reads/writes sequenced-after an acquire that reads
from it. This is the TOS rule, not an adoption by reference of Rust/C++ or a
host runtime.

Atomicity does not make a non-atomic object safe to mutate concurrently. A
program publishes a non-atomic immutable/initialized value through a release
store and acquire load, a mutex, a channel, task join, or another stated
synchronizer; it does not read/write it concurrently. Atomic operations have
no implicit global fence beyond their declared order.

## 6. Resource declarations and accounting

Each module has exactly one `resource [ ... ]` item. It declares at least:

```text
fuel:        integer,     // maximum interpreter instructions/checkpoints
stack:       size,        // maximum stack bytes per execution context
allocation:  size,        // maximum live allocatable bytes
tasks:       integer,     // maximum simultaneously live scoped tasks
workers:     integer,     // maximum runnable execution contexts requested
sync:        integer,     // maximum live synchronization objects/guards
shared:      size,        // maximum bytes of shared-region grants
cleanup:     integer,     // maximum bounded cleanup steps after cancellation
recursion:   integer,     // maximum dynamic call depth
imports:     integer,     // maximum transitive module dependencies
```

The values are compile-time constants and all maxima are inclusive. A module
may declare stricter named limits, but cannot omit or silently inherit the
required ones. The launcher grants an effective resource envelope no larger
than the declaration. A call/spawn/import is permitted only when the checker
and verifier can establish that its declared worst-case contract fits the
caller envelope. A dynamic allocation/task/worker/synchronization operation
checks the remaining envelope before it takes effect; exhaustion returns the
typed error associated with that operation where one exists, otherwise traps
with a stable `RUNTIME_RESOURCE_*` code. It never silently allocates an
unbounded host thread or heap object.

Missing a required resource key is `E1700_RESOURCE_DECLARATION_REQUIRED`; a
duplicate declaration is `E1703_DUPLICATE_RESOURCE_DECLARATION`; an unknown
key or wrong literal type is `E1704_UNKNOWN_RESOURCE_LIMIT`. The effective
envelope also carries a launcher-granted `cpu_time` duration budget for the
declared service interval. The runtime accounts the sum of CPU time consumed by
all of the process's execution contexts, not elapsed wall time, and refuses to
run further work when that budget is exhausted. Bootstrap fuel is the
deterministic instruction-level counterpart; a Full runtime records and limits
both where policy requires. This makes parallel CPU use accountably bounded
without making correctness depend on a particular scheduler or core count.

Recursive functions require a syntactic `recursion` budget. Bootstrap requires
finite `fuel`, `stack`, `allocation`, `tasks`, `workers`, `sync`, `shared`,
`cleanup`, `recursion`, and `imports`; it accepts `workers: 1` only. Full may
declare more workers, but actual core count is a scheduling choice bounded by
the lower of grant, process policy, and available runtime workers. This is
accounting, not a guarantee of throughput or CPU affinity.

Loop back edges consume fuel in Bootstrap. A verifier-visible loop may have a
statically proven finite bound or consume fuel; an unmetered unknown loop is
`E1701_UNMETERED_LOOP`. Full engines MAY schedule/preempt differently but MUST
honor the module's observable resource limits. No V1 contract requires a
stop-the-world garbage collector; an allocator strategy is internal only if it
preserves the declared allocation and pause/fuel limits.

## 7. Errors, traps, panic, and diagnostics

Recoverable program conditions use `Result<T,E>`. Language/runtime traps are
defined failures of a dynamic language precondition (for example checked
overflow); a panic is a violated implementation/language invariant. Both end
the current process through its supervisor policy. A task reports a typed
`Err` or cancellation when its signature supports it; a trap/panic ends that
task's process and is recorded as a terminal diagnostic.

Every parser, checker, verifier, runtime, or resource diagnostic has:

```text
stable symbolic code
severity (error, warning, note)
stage (lex, parse, type, ownership, effect, resource, IR, runtime)
module name and canonical repository path
source-set identity and normalized source content ID
byte start/end span and derived line/UTF-8-column
structured key=value fields
zero or more ordered causal diagnostics
```

Human wording may improve, but code, stage, primary span, field names, and
causal ordering are stable for V1. A frontend must choose the earliest source
span; at one span it chooses lexical before parse, parse before name/import,
name/import before type, type before ownership/effect, ownership/effect before
resource, then runtime only after successful static validation. The parser
recovery rule in docs/39 may emit subsequent independent errors but cannot
change this primary precedence.

Representative stable code families are `E10xx` lexical/parser,
`E12xx` type/evaluation, `E13xx` ownership, `E14xx` concurrency/atomic,
`E15xx` capability/effect, `E16xx` module/version, `E17xx` resource/profile,
`E18xx` unsafe/FFI, `V20xx` IR verifier, and `RUNTIME_*`/`PANIC_*` terminal
events. A full registry and conformance expectations are in docs/44.
