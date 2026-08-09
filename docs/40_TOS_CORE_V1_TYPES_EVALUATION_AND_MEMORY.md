<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — types, evaluation, ownership, and memory

- Status: **Proposed Stage 2 contract — not implementation authority**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`
- Companion execution contract:
  `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`

## 1. Static model

TOS Core is statically typed. A well-typed safe program has no type confusion,
unbounded implicit coercion, arbitrary pointer access, or undefined behavior
caused by a safe data race. Type checking is deterministic for identical
normalized source, declared imports, language version, profile, and resource
declarations. It has no ambient filesystem, network, clock, random, current
directory, or environment input.

V1 has nominal record, enum, capability, region, and module types. Primitive
types are structural only within their exact name. The type of `A::T` is not
identical to `B::T` merely because their fields match. A type name resolves
through the declared import graph, never by host search paths.

The primitive types are `bool`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`,
`u32`, `u64`, `size`, `duration`, `string`, `bytes`, and `unit`. `size` is an
unsigned target-ABI-sized value used only for in-memory indexing and allocation
bounds. It MUST NOT be serialized in a persistent/public format. `duration` is
an unsigned `u64` count of nanoseconds. Public and persistent forms use one of
the explicit fixed-width integers.

`Option<T>` has variants `Some(T)` and `None`; `Result<T,E>` has variants
`Ok(T)` and `Err(E)`. `Task<T>` is a scoped task result. `Shared<T>` is an
immutable shareable value. `Region<T>` and `DmaRegion<T>` are opaque
nucleus-granted typed region handles. `Mutex<T>`, `RwLock<T>`, `Channel<T>`,
`Event`, `Semaphore`, `Barrier`, `Latch`, `AtomicBool`, `AtomicU32`, and `AtomicU64` are
typed runtime contracts, not magic host APIs. Their exact dynamic semantics
are in `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`.

Arrays `[T; N]` have a compile-time nonnegative `N` that is representable as
`size`. `slice<T>` means a borrowed view and cannot be stored or returned as an
owned value in V1. A function type `fn(A, B) -> R` is a non-capturing callable
type. Full-profile closures have a compiler-defined anonymous callable type and
cannot cross a module boundary until a later version defines stable closure ABI.

Enum variant names are local to their defining module and may be used
unqualified there; an imported enum variant uses a qualified type/module name.
`Some`, `None`, `Ok`, and `Err` are the fixed V1 constructors for `Option` and
`Result`, not host-library names.

There are no user-defined generic functions, traits, implicit interfaces, or
ad-hoc overload resolution in V1. The listed library type constructors are the
only parameterized types. This keeps type identity, diagnostics, and
independent verification bounded.

## 2. Bindings, functions, effects, and capabilities

`let name = expression;` creates an immutable binding. `let mut name =
expression;` creates a mutable binding. A binding annotation constrains the
expression type. Assignment requires a mutable binding or a place reached
through one active mutable borrow. Assigning to a nonmutable place is
`E1201_ASSIGN_TO_IMMUTABLE`.

Function parameters without `borrow` consume an owned argument unless its type
is `Copy`. `borrow parameter: T` creates an immutable temporary borrow;
`borrow mut parameter: T` creates an exclusive mutable temporary borrow. V1
borrows cannot be returned, stored in records/enums/arrays, captured by a
Full-profile closure, sent through a channel, or placed in a task. These
restrictions make their region exactly the caller expression or callee body and
avoid hidden lifetime inference.

Functions are pure with respect to authority unless their `uses { ... }` set
names imported capability parameters or capability values. An operation that
requires a capability is type-correct only if its required capability name is
present in the enclosing function's transitive effect set. An empty effect set
is written by omission. Calling a function requires the caller effect set to
include every effect the callee requires; otherwise the checker emits
`E1501_UNDECLARED_CAPABILITY_EFFECT`. Capability values are opaque,
nonconstructible, and non-comparable except for identity logging by a privileged
runtime contract. An integer, string, cast, deserialization, record literal, or
unsafe block cannot mint one.

`async fn` returns `Task<Result<T, E>>` when its declared return type is
`Result<T, E>` and `Task<T>` otherwise. `await task` obtains the successful
task value or propagates its `Err`/cancellation according to `?`; it is
Full-profile only. `spawn async` and `spawn parallel` capture values according
to the ownership rules below. `spawn` has no detached form in V1.

A Full-profile closure captures each free `Copy`/`Shared<T>` value by copy and
each other permitted value by move at closure creation. It cannot capture a
borrow, mutable binding by alias, lock guard, non-transferable capability, or
plain mutable region. A closure is affine when any captured value is affine.
It may be called within its owning scope but cannot be exported, serialized,
stored in a public nominal type, or passed to an interface with a stable ABI in
V1. An invalid capture is `E1305_INVALID_CLOSURE_CAPTURE`.

## 3. Conversion, equality, and integer semantics

No nonliteral numeric conversion is implicit. An integer literal may take the
surrounding exact integer type if in range; otherwise an unsuffixed literal is
`i32`. Assigning or passing values of different integer types is
`E1210_INTEGER_TYPE_MISMATCH`. `as T` is permitted only for an integer
widening conversion that preserves signedness, `u8` to `u16`/`u32`/`u64`, or
the corresponding signed widening. All other conversion uses the typed
standard contract `convert<T>(x) -> Result<T, ConversionError>`; it checks
range and sign. Explicit wrapping arithmetic is only available through
`wrapping_add`, `wrapping_sub`, and `wrapping_mul` contracts with exact
fixed-width type arguments.

An attempt to use `as` with a capability, region, DMA region, task,
synchronization object, function, closure, or pointer-like host value is not a
generic conversion error: it is `E1502_FORGED_CAPABILITY` for a capability and
the corresponding nonconstructible-type error for the other opaque types.

Normal integer `+`, `-`, `*`, `/`, `%`, unary `-`, and shifts are checked.
Overflow, division/remainder by zero, an invalid shift count, or `MIN / -1`
is a language trap with a stable `RUNTIME_*` code and terminates the current
process; it is not host undefined behavior and cannot be caught as `Result`.
For `uN`, `-x` is rejected statically. Shift counts must be nonnegative and
strictly smaller than `N`. `size` arithmetic is checked in the target ABI;
portable source must not assume its width.

`==` and `!=` are available for primitive values, immutable records/enums whose
members are comparable, and opaque handles only where the corresponding typed
contract explicitly exposes equality. They are not available for mutable
regions, mutable synchronization guards, tasks, capabilities, functions, or
closures. Ordering exists for numeric, `size`, `duration`, `string`, and
`bytes` values only. Strings compare lexicographically by their stored Unicode
scalar sequence; source NFC is a source-identity rule, not an implicit runtime
string-normalization pass. Bytes compare lexicographically by byte.

Array, slice, and region indexes have exact type `size`; an integer literal may
be contextually typed as `size` when nonnegative and representable. Other index
types are `E1211_INDEX_TYPE_MISMATCH`. Every safe index operation performs a
checked bounds operation and returns the declared typed bounds error where the
interface exposes one; it never becomes host out-of-bounds access.

## 4. Evaluation and dynamic semantics

TOS evaluates expressions left-to-right. Specifically, a call evaluates its
callee, then arguments left-to-right, then enters the call; a binary operator
evaluates its left operand before its right; record/array/tuple fields evaluate
in lexical source order; match subject evaluates before patterns; assignment
evaluates its place base/index left-to-right before its right side. `&&` does
not evaluate its right side after false; `||` does not evaluate its right side
after true. `?` evaluates its operand once and returns the containing function
with the matching `Err` if it is not `Ok`.

The tail expression of a block is its value. A semicolon discards a statement
expression's value. `if` expressions require both branches to have the same
type; a missing `else` produces `unit`. `match` must be exhaustive for an enum,
`Option`, or `Result`; a missing case is `E1220_NONEXHAUSTIVE_MATCH`. An `_`
arm is exhaustive. Patterns bind by move unless the matched subject is an
immutable `Copy` value; borrows must be made explicitly before match.

`Result` is the sole ordinary recoverable-error transport. A runtime trap is a
defined language failure caused by a violated dynamic precondition. `panic`
denotes a violated language/runtime invariant and has the same process-ending
effect as a trap but a distinct stable code family. Neither uses host exception
unwinding. Details and diagnostic attribution are defined in docs/41.

`defer` registers a lexically scoped cleanup block. Defers run in reverse
registration order whenever their enclosing block exits normally, by `return`,
by `?`, by `break`, or after cancellation reaches that block. A defer block
cannot `return`, `break`, `continue`, `await`, `join`, spawn work, or acquire a
new resource; violations are `E1225_INVALID_DEFER`. A trap/panic while running
a defer records both the original and cleanup cause then terminates. This
bounded rule gives cancellation deterministic cleanup without implicit general
unwinding.

## 5. Ownership and borrows

Safe non-`Copy` values are affine: every value has one owner and is moved when
assigned, passed by an owning parameter, returned, put into an aggregate, or
captured by a task/closure. Use after move is `E1301_USE_AFTER_MOVE`. `Copy`
types are fixed-width numeric types, `bool`, `duration`, `unit`, and explicitly
documented immutable value handles; strings, bytes, capabilities, regions,
tasks, locks, channels, arrays, records, and enums are non-`Copy` unless all
members are `Copy` and their type declaration says so.

At any program point, a value may have either any number of immutable borrows
or exactly one mutable borrow, never both. An immutable borrow cannot mutate
the value; a mutable borrow cannot be aliased. The checker determines a borrow
region from the smallest enclosing expression/block required by use. Because
V1 borrowed values neither escape nor enter a task/aggregate, no inferred
cross-function lifetime notation is needed. A conflicting borrow is
`E1302_CONFLICTING_BORROW`; mutation while immutably borrowed is
`E1303_MUTATE_WHILE_BORROWED`.

An owned record/array/enum may be partially moved only when the remaining value
is never used except to move/drop its untouched fields. A mutable field borrow
locks the containing path, not unrelated fields; indexed elements are treated
as overlapping unless their indices are compile-time unequal constants. This
conservative rule is deterministic and safe.

Values leave scope in reverse binding order. Each type has a bounded `drop`
contract defined by its standard/module declaration. `drop` may release a
region, task reservation, synchronization object, or capability reference, but
may not allocate, await, acquire authority, or execute user callbacks.
Declaring a type whose cleanup does not have a finite documented bound is
rejected from Bootstrap as `E1708_UNBOUNDED_CLEANUP`.

## 6. Sharing, regions, and task transfer

`Shared<T>` is created only by the typed `share` contract for a `T` whose full
transitive contents are immutable and `Shareable`. It provides immutable
borrows and can be copied into multiple scoped tasks. It never grants mutation.
Controlled mutable sharing uses a `Mutex<T>`, `RwLock<T>`, atomic, channel, or
typed shared `Region<T>` operation; ordinary `mut` does not become globally
shareable.

`Region<T>` is an opaque process-local/shared-memory grant with declared
element type, byte length, alignment, access rights, and lifetime. Safe code
may obtain it only from an authority-bearing typed service operation, access it
only with checked `read`, `write`, or `slice` contracts, and never observe its
physical address. `DmaRegion<T>` additionally records a nucleus-granted DMA
mapping and device-domain authority; safe code may not construct, cast to, or
serialize it as an integer. Whether a particular region is shareable/mutable
is stated in its capability contract and independently checked in IR.

A value may cross a task boundary only if it is `Transferable`: owned affine
values transfer their sole ownership; immutable `Copy`/`Shared<T>` values are
duplicated; opaque capabilities, mutable borrows, lock guards, and plain
mutable regions are non-transferable unless their own contract exposes a
specific attenuation/transfer operation. A closure/task with an invalid capture
is `E1304_INVALID_TASK_CAPTURE`.

There are no safe raw pointers, address literals, pointer arithmetic, address
casts, layout reinterpretation, arbitrary physical addresses, or implicit FFI
conversions. The TOS abstract address space is a set of typed regions, not a
48-bit x86_64 number. This preserves a path to LA57 and non-x86 targets.

## 7. Explicit unsafe boundary

`unsafe { ... }` is Full-profile only and changes neither ownership nor
capability authority. It only permits calls to an imported interface operation
explicitly marked `unsafe` by an accepted future interface contract. The block
MUST contain a leading line comment beginning `SAFETY:` that names the local
preconditions. A missing rationale is `E1802_UNSAFE_RATIONALE_REQUIRED`.
Unsafe code remains subject to declared capabilities, resource limits, source
maps, and IR verification. An unsafe block cannot forge a capability or turn a
safe caller's data race into undefined behavior; the unsafe operation's
interface must state how it preserves safe caller guarantees.

No V1 base operation currently requires `unsafe`; `extern` is rejected until a
later accepted contract exists. This is an explicit boundary, not an ambient
escape hatch or an invitation to inherit a Rust/C/host ABI.
