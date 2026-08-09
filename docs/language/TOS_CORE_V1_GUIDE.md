<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 programmer guide

> **Status: proposed language, not implemented.** This guide describes the
> Proposed TOS Core V1 contract in docs/39–44. No production frontend exists
> yet; `.tos` files linked here are canonical proposed examples, not runnable
> claims. If this guide conflicts with the numbered specification, the numbered
> specification wins.

## What a TOS Core program is

A TOS Core program is human-readable, normalized UTF-8 `.tos` source in a
source-identified system tree. It is not a wrapper around a hidden binary: its
AST, typed IR, bytecode, and native code are disposable derived artifacts. A
file starts by naming one module, language version, and profile, then gives the
resource envelope the module needs. Start with
[first.tos](examples/first.tos).

The `bootstrap` profile is a deliberately small, bounded subset for recovery
and the first reference runtime. `full` has the same core meanings but permits
the richer async, closure, unsafe, and real-SMP execution path. It is not a
different language. The current Stage 2 design does not implement either
profile yet.

## Files, modules, and imports

A module name determines its repository-relative path: `system.boot.init`
means `system/boot/init.tos` under its declared root. Imports name other modules
explicitly. Resolution uses the active source set and declared dependency lock;
it never searches your current directory or downloads a package. See
[modules.tos](examples/modules.tos).

The common mistake is to expect an implicit standard library or wildcard import.
V1 has neither; the diagnostic is `E1604_IMPORT_NOT_FOUND` or a syntax error.
This keeps a restored system and an ordinary build looking at the same source
closure.

## Values and basic types

`let` binds an immutable value; `let mut` permits assignment through that one
binding. Integers are fixed width (`u8` through `u64`, `i8` through `i64`), so
portable source never relies on a host word size. `size` is only for in-memory
sizes; serialized formats use explicit fixed-width types. `KiB`/`MiB` and
duration literals are typed units. The basic example is
[values.tos](examples/values.tos).

Arithmetic is checked. Overflow, division by zero, and invalid shifts are
defined language traps, not whatever a backend happens to do. A checked
narrowing/sign-changing conversion uses its spelled destination function, such
as `to_u8(value)`, and returns `Result<u8, ConversionError>`. It is not a
generic cast. Explicit wrapping operations are only for code that genuinely
needs wrapping.

## The punctuation model

V1 uses five syntax rules consistently: `()` holds parameters, arguments, and
expression grouping; `[]` holds declarations, data lists, and collections;
`{}` holds executable code; `,` separates list members; and `;` ends a simple
executable action. `return` explicitly returns a normal value. This means a
record declaration is `record Point [x: i32, y: i32]`, while a record value is
`Point(x: 1i32, y: 2i32)`. Braces never mean a record value in V1.

## Functions, records, tuples, and enums

Functions state parameter and return types. Records have named fields; tuples
group a small fixed sequence; enums model a finite set of alternatives. The
language deliberately has no user-defined generics or inheritance in V1: clear
nominal types make diagnostics and verifier checks smaller. See
[data.tos](examples/data.tos).

Function calls, enum tuple-variant construction, and nominal record
construction use the same `name(...)` parse family and evaluate arguments
left-to-right. Only records accept named arguments, and they require every
field exactly once. `if` and `match` are statement-oriented: each branch is an
executable `{ ... }` block, branches are not comma-separated, and a non-unit
function returns through explicit `return value;` on every normal path. There
is no hidden tail expression or semicolon-dependent return rule. `match` must
handle every enum/`Option`/`Result` case; an omitted case receives
`E1220_NONEXHAUSTIVE_MATCH`, rather than becoming a runtime surprise.

## Option, Result, errors, and diagnostics

Use `Option<T>` for an expected absence and `Result<T, E>` for a recoverable
failure. `?` returns the `Err` from the current function; it does not catch a
language trap or panic. [results.tos](examples/results.tos) is the canonical
proposed example.

Every diagnostic identifies its stable code, module, source-set/content IDs,
repository path, byte span, line/column, and relevant structured fields. That
is why a simple source error can be tied to an eventual typed IR and runtime
event. An invalid source and its intended primary diagnostic live in
[reject](conformance/v1/reject/).

## Ownership and borrowing

Most nontrivial values have one owner. Assigning, returning, or passing one to
an owning parameter moves it. Primitive values are Copy; tuples and arrays copy
only when all their elements Copy. User records and enums remain affine in V1,
even when their fields are primitive. `Option`, `Result`, and `TaskResult` also
move. A string, region, task, lock, channel, or capability still moves. Use
`borrow value` to lend an immutable view and
`borrow mut value` to lend the only mutable view for a small lexical scope.
V1 deliberately does not let borrows escape a function, be stored, or be sent
to a task; that keeps the first checker auditable. See
[ownership.tos](examples/ownership.tos).

The practical rule is: at one time, use either any number of readers or one
writer. Trying to use a moved value is `E1301_USE_AFTER_MOVE`; overlapping a
mutable borrow with another borrow is `E1302_CONFLICTING_BORROW`. Ordinary safe
code has neither raw pointers nor physical-address integers.

## Regions and capabilities

A capability is an opaque authority value issued by the launcher, not a number
that source can invent. A module requests it with `import capability`; the
launcher may grant or deny it. A function that uses it declares that fact in a
`uses [ ... ]` effect set. [capability.tos](examples/capability.tos) shows the
shape without pretending that the future clock service exists today.

Typed `Region<T>` and `DmaRegion<T>` are similarly granted through later
versioned interfaces. They carry access, alignment, length, and lifetime rules;
safe code cannot cast one to an address. The common error is to assume a source
declaration grants hardware access. It does not: a denied request yields the
typed launch failure, and a forged value is `E1502_FORGED_CAPABILITY`.

## Resource-bounded programming

Every module declares fuel, stack, allocation, tasks, workers, synchronization,
shared bytes, cleanup, recursion, and import limits. The launcher grants an
envelope no larger than the declaration. [resources.tos](examples/resources.tos)
shows the complete required section. Resource declarations are behavioral
contracts: `spawn` reserves a task/worker before it creates work, and an
unmetered Bootstrap loop is `E1701_UNMETERED_LOOP`.

Do not treat a bigger machine as an implicit larger budget. Worker count and
CPU count are runtime policy; correct results must not change merely because a
process gets one instead of four workers.

## Async, parallel work, and cancellation

`spawn async`/`await` represent event-driven work in Full profile; see
[async.tos](examples/async.tos). `parallel` creates a lexical scope, and
`spawn parallel` creates children that must ultimately be joined before that
scope ends. `cancel` only requests cooperative cancellation; it does not
consume the child. `join`/`await` consumes `Task<T>` and returns
`TaskResult<T>`: `Completed(value)` preserves the child result and `Cancelled`
records cancellation. A Full runtime can run independent children simultaneously on
several cores; Bootstrap executes the same valid parallel scope serially. See
[parallel.tos](examples/parallel.tos).

There are no detached V1 tasks. `E1401_UNJOINED_TASK` prevents accidentally
leaving a child and its resources behind. `cancel` is cooperative and cleanup
is bounded; joining makes a child's completion or cancellation visible to its
parent.

## Shared data, synchronization, and atomics

Share immutable data with `Shared<T>`. For mutable shared state, use a typed
mutex/RW lock, channel, event, barrier/latch, or atomics. Ordinary `mut` does
not grant simultaneous mutable access. `AtomicBool`, `AtomicU32`, and
`AtomicU64` have explicit Relaxed/Acquire/Release/AcqRel/SeqCst orderings;
their meanings are specified by TOS Core, not borrowed implicitly from Rust or
C++. [atomic-publication.tos](examples/atomic-publication.tos) is the canonical
proposed publication example.

Atomics are not a shortcut for every shared object. Publishing data uses a
release/acquire pair or another stated synchronizer; concurrent non-atomic
mutation remains rejected by ownership/IR verification. The invalid case is
[shared-mutable.tos](conformance/v1/reject/shared-mutable.tos).

## Bootstrap, Full, and unsafe/FFI

Bootstrap permits bounded scalar/aggregate code and serialized structured
parallelism, but not async I/O, closures, `defer`, `unsafe`, `extern`, or
dynamic loading. Full expands execution options without relaxing the type,
ownership, capability, or resource rules. Mark Full-only source as `profile
full`; a Bootstrap frontend must reject it explicitly.

`unsafe` is deliberately visible and unavailable in Bootstrap. It cannot forge
authority or turn a safe caller's mistake into undefined behavior. FFI is only
reserved syntax today: no C, Rust, libc, dynamic library, or host ABI is
available to TOS code until a future accepted interface contract says exactly
how it is safe, capability-gated, resource-bounded, and source-mapped.

## Where to look next

[Learning TOS Core](LEARNING_TOS_CORE.md) is the sequential path. The
[example status matrix](EXAMPLE_STATUS.md) says which proposed feature has a
specification, guide section, tutorial/example, and conformance vector. It is
deliberately honest: all rows are specified-proposed and unimplemented until
the Stage 2 contract is accepted and its production frontend exists.
