<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — types, evaluation, ownership, and memory

- Status: **Accepted Tier 2 contract — production implementation in progress**
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
`Ok(T)` and `Err(E)`. `ConversionError` is the fixed V1 standard error type
returned by checked numeric conversions; ordinary code receives it only through
those `Result` values. `Task<T>` is an owned scoped task handle.
`TaskResult<T>` has variants `Completed(T)` and `Cancelled`; it is the result
of consuming a task handle through `join` or `await`. This keeps cancellation
distinct from a child value of type `T`, including when `T` is itself
`Result<U,E>`. `Shared<T>` is an immutable shareable value. `Region<T>` and
`DmaRegion<T>` are opaque
nucleus-granted typed region handles. `Mutex<T>`, `RwLock<T>`, `Channel<T>`,
`Event`, `Semaphore`, `Barrier`, `Latch`, `AtomicBool`, `AtomicU32`, and
`AtomicU64`, and `ConversionError` are non-generic typed runtime contracts,
not magic host APIs.
Their exact dynamic semantics are in
`docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`.

Arrays `array<T, N>` have a compile-time nonnegative `N` that is representable as
`size`. `slice<T>` means a borrowed view and cannot be stored or returned as an
owned value in V1. A function type `fn(A, B) -> R` is a non-capturing callable
type. Full-profile closures have a compiler-defined anonymous callable type and
cannot cross a module boundary until a later version defines stable closure ABI.

Enum variant names are local to their defining module and may be used
unqualified there; an imported enum variant uses a qualified type/module name.
`Some`, `None`, `Ok`, and `Err` are the fixed V1 constructors for `Option` and
`Result`, not host-library names.

### Pattern name resolution

Every pattern is checked against an expected type: the scrutinee type for
`match`, the initializer type refined by any annotation for `let`, the element
type for `for`, and the corresponding tuple element or enum payload type for a
nested pattern.

A bare identifier that exactly names a variant of the expected enum type is the
constructor pattern for that variant. Any other bare ordinary identifier
introduces a new pattern binding. `Name(...)` is a constructor and destructuring
pattern resolved the same way, and its sub-patterns are checked against that
variant's payload positions.

Resolution is nominal. There is no capitalization rule, and an existing lexical
or value binding of the same name does not change the outcome, so two enums may
declare variants with the same name and each is disambiguated by the expected
type of the subject. `Some`, `None`, `Ok`, `Err`, `Completed` and `Cancelled`
remain non-shadowable constructors and are never bindings.

A qualified pattern path — `Signal.Low`, or `other.Signal.Low` for an imported
enum — always denotes a constructor and never a binding. A qualified path that
names no reachable variant is an error; it does not degrade into a catch-all. A
local variant may be written either short or qualified. See ADR-0033.

There are no user-defined generic functions, traits, implicit interfaces, or
ad-hoc overload resolution in V1. The listed library type constructors are the
only parameterized types. This keeps type identity, diagnostics, and
independent verification bounded.

The complete V1 constructed-type arity is fixed and is shared with docs/39 and
docs/43: `Option<T>`, `Task<T>`, `TaskResult<T>`, `Shared<T>`, `Region<T>`,
`DmaRegion<T>`, `Mutex<T>`, `RwLock<T>`, `MutexGuard<T>`, `ReadGuard<T>`,
`WriteGuard<T>`, `Channel<T>`, and `slice<T>` take one
type argument; `Result<T,E>` takes two. `Event`, `Semaphore`, `Barrier`,
`Latch`, `AtomicBool`, `AtomicU32`, `AtomicU64`, and `ConversionError` take no
type arguments.
`array<T, N>` takes one type argument and one compile-time `size` constant;
its comma is a declarative type-parameter separator, not a statement
terminator. Its second argument is a constant rather than a type, so it is not
one of the parameterized constructors above. `slice<T>` is the only
borrowed-view type form and retains the nonescaping restrictions above.

The number of type arguments is a static type property, not a parser decision.
The parser builds a constructed-type node for any known V1 constructor written
with `<...>`, and the checker compares the actual count against the fixed arity
above; a mismatch is `E1204_TYPE_ARGUMENT_ARITY` with the constructor and both
arities. This is not an implementation-defined generic application, and it
admits no user generics: an arbitrary `Foo<T>` is not V1 type syntax.

A type name that resolves to no primitive, fixed or predeclared type, local
nominal type or reachable imported type is `E1203_UNKNOWN_TYPE_NAME`, carrying
the name as spelled. For a qualified name the module or import part resolves
first: a missing import or module is the applicable `E16xx` code, while an
existing one that does not declare the name is `E1203_UNKNOWN_TYPE_NAME`.

Precedence is fixed. An unresolved name is `E1203_UNKNOWN_TYPE_NAME`; a
resolved parameterized constructor applied with the wrong count is
`E1204_TYPE_ARGUMENT_ARITY`; only after the arity is correct are the argument
types and remaining type rules checked, so one mistake cannot cascade into
findings derived from a constructed type that does not exist. See ADR-0034.

## 2. Bindings, functions, effects, and capabilities

`let name = expression;` creates an immutable binding. `let mut name =
expression;` creates a mutable binding. A binding annotation constrains the
expression type. Assignment requires a mutable binding or a place reached
through one active mutable borrow. Assigning to a nonmutable place is
`E1201_ASSIGN_TO_IMMUTABLE`.

A module-level `const name: T = expression;` declares a **compile-time value**,
not a runtime object (ADR-0052). Its initializer is a constant expression: a
scalar literal; a `const_expression` whose `identifier` names another constant,
of this module or an imported one; or a record, enum tuple, named-field variant
or array constructor whose arguments are themselves constant expressions. A
call, an effect, a borrow and a capability are all excluded — the last already
by section 2 of `docs/42`. An initializer that is not a constant expression is
`E1224_NONCONSTANT_INITIALIZER`.

The constant *is* its value: it is substituted where it is used, including
across a module boundary, and V1 therefore has no module-initialization phase.
There is no moment at which a constant is computed, so there is no evaluation
order to fix between constants, no trap a constant can raise, and no resource
its declaration consumes. This is what lets `array<T, N>` take a named constant
as its compile-time `size`, and what lets a launcher read a module's constants
before starting it.

What is fixed is that an initializer causes nothing to run. Projection,
indexing and conversion are excluded **inside an initializer** in V1 for want of
their own compile-time rules, and report the same code; a later version may
admit them, because they create no evaluation moment. Reading a field of a
constant in ordinary code is unaffected: `LIMITS.depth` in a function body is
the constant substituted and then projected, like any other value. A call is different in kind and stays excluded: a
runtime-initialized object, if V1's successors want one, is a separate item form
with its own keyword and its own initialization contract, since admitting calls
into a `const` initializer would move when existing source evaluates without
changing its text.

Function parameters without `borrow` consume an owned argument unless its type
is `Copy`. `borrow parameter: T` creates an immutable temporary borrow;
`borrow mut parameter: T` creates an exclusive mutable temporary borrow. V1
borrows cannot be returned, stored in records/enums/arrays, captured by a
Full-profile closure, sent through a channel, or placed in a task. These
restrictions make their region exactly the caller expression or callee body and
avoid hidden lifetime inference.

Functions are pure with respect to authority unless their `uses [ ... ]` set
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
`Result<T, E>` and `Task<T>` otherwise. `await task` consumes `Task<T>` and
has type `TaskResult<T>`; it is an asynchronous join and is Full-profile only.
For example, awaiting `Task<Result<T,E>>` produces
`TaskResult<Result<T,E>>`: `Completed(Err(e))` is the child program result,
while `Cancelled` is task cancellation. `spawn async` and `spawn parallel`
capture values according to the ownership rules below. `spawn` has no detached
form in V1.

A Full-profile closure captures each free `Copy`/`Shared<T>` value by copy and
each other permitted value by move at closure creation. It cannot capture a
borrow, mutable binding by alias, lock guard, non-transferable capability, or
plain mutable region. A closure is affine when any captured value is affine.
It may be called within its owning scope but cannot be exported, serialized,
stored in a public nominal type, or passed to an interface with a stable ABI in
V1. An invalid capture is `E1305_INVALID_CLOSURE_CAPTURE`, except for a lock
guard: ADR-0036 routes a guard crossing a task or closure boundary to
`E1402_INVALID_GUARD_LIFETIME` with `operation=task_boundary`, because the rule
broken is about the guard's lifetime rather than about transferability alone.
The capture codes keep their meaning for every other non-`Transferable` value.

## 3. Conversion, equality, and integer semantics

No nonliteral numeric conversion is implicit. An integer literal may take the
surrounding exact integer type if in range; otherwise an unsuffixed literal is
`i32`. Assigning or passing values of different integer types is
`E1210_INTEGER_TYPE_MISMATCH`. `as T` is permitted only for an integer
widening conversion that preserves signedness, `u8` to `u16`/`u32`/`u64`, or
the corresponding signed widening. Any other `as` conversion is
`E1212_INVALID_AS_CONVERSION`.

Checked conversion has no generic-call syntax. The fixed V1 standard functions
`to_i8` through `to_i64` and `to_u8` through `to_u64` are ordinary Call-form
callees defined in docs/39. Each accepts any fixed-width integer or `size`,
checks sign and range, and returns `Result<D, ConversionError>` for its
spelled destination `D`. Thus `to_u8(value)` is the source form for a checked
narrowing/sign-changing conversion; callers use its `Result` rather than
depending on host casts. Explicit wrapping arithmetic is only available through
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

TOS evaluates expressions left-to-right. Specifically, a Call evaluates its
callee, then arguments left-to-right, then enters the resolved function or
constructor; a binary operator evaluates its left operand before its right;
record/array/tuple fields evaluate in lexical source order; match subject
evaluates before patterns; assignment evaluates its place base/index
left-to-right before its right side. Ordinary function calls and tuple-variant
constructors use the same Call form and differ only at resolved-callee checking.
`&&` does not evaluate its right side after false; `||` does not evaluate its
right side after true. `?` evaluates its operand once and propagates the
matching `Err` from the nearest enclosing return scope if it is not `Ok`.

An executable block is a statement body, not a value container: it has no tail
expression. `return expression;` is the only normal value return. A function
body, closure body, and `spawn async`/`spawn parallel` body each establish a
**return scope**. Ordinary nested `{ ... }` blocks do not establish one.
`return` targets the nearest enclosing return scope, and `?` propagates `Err`
from that same nearest return scope. Every
reachable normal completion path of a function with a non-`unit` declared
return type MUST execute an explicit `return` with that exact type; reaching
the end of such a function is `E1221_MISSING_RETURN`. `return;` or a value of
the wrong type is `E1222_RETURN_TYPE_MISMATCH`. A `unit` function may reach its
end normally. A semicolon terminates a simple executable statement; it never
silently changes a would-be return value into `unit`.

A closure or spawned task body follows the same rule. Its body has result
`unit` only when every normal path reaches its end without a value `return`.
Otherwise every reachable normal completion path MUST explicitly return one
inferred exact result type; mixing a value return with a normal fallthrough is
`E1221_MISSING_RETURN`, and inconsistent returned types are
`E1222_RETURN_TYPE_MISMATCH`. This makes task/closure result production visible
without making their executable blocks expressions.

`if`, `match`, `while`, `for`, `loop`, and `parallel` are statements, not
expressions. An `if` branch, including `else`, is an executable block and has
no value typing rule. A `match` arm is likewise an executable block; arms are
not comma-separated. A `match` evaluates its subject exactly once, then considers its arms in strict
lexical source order and runs the **first** arm whose pattern matches. Later
arms take no part in selection once an arm is chosen, and exactly one arm body
executes (ADR-0047). A wildcard, a bare binding and an irrefutable tuple pattern
are therefore catch-alls that make every later arm unreachable. Unreachable arms
are permitted in V1 and have no diagnostic.

`match` must be exhaustive for an enum, `Option`, or
`Result`; a missing case is `E1220_NONEXHAUSTIVE_MATCH`. An `_` arm is
exhaustive. `break` has no value. Patterns bind by move unless the matched
subject is an immutable `Copy` value; borrows must be made explicitly before
match. `?` remains an explicit Result-propagation operation: it evaluates its
operand once and propagates the matching `Err` from the nearest enclosing
return scope; it is not an implicit block-tail return mechanism.

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

A `defer` is deferred lexical cleanup, not a capture, and it is not a closure:
the closure-capture rules of `E1305_INVALID_CLOSURE_CAPTURE` do not apply to it.
Executing the `defer` statement registers the cleanup and nothing else. At that
point the lexical names in its body bind to the binding identities visible at
the point of registration, and the values of those bindings are neither read,
borrowed nor moved. Shadowing after registration does not change which binding
the body refers to. On each exit path the action that caused the exit is
evaluated first, then the defers registered on the path actually taken run in
reverse registration order, each observing the ownership and borrow state the
previous one left; only then do bindings leave scope and their bounded `drop`
run. A defer body is therefore checked against the ownership state that exists
on the concrete exit path, so ordinary correct use of a resource between
registering its cleanup and leaving the block is allowed, while a cleanup that
cannot run soundly on a path that reaches it is rejected there. `return`,
`break`, `continue` and normal block exit unwind the cleanups of exactly the
lexical blocks they leave; `?` and cancellation use the same model rather than a
second cleanup mechanism. See ADR-0035.

## 5. Ownership and borrows

Safe non-`Copy` values are affine: every value has one owner and is moved when
assigned, passed by an owning parameter, returned, put into an aggregate, or
captured by a task/closure. Use after move is `E1301_USE_AFTER_MOVE`. V1 has no
Copy declaration marker, trait, derivation, or user override. Fixed-width
numeric types, `size`, `duration`, `bool`, and `unit` are `Copy`. A tuple is `Copy`
exactly when every element is `Copy`; an array is `Copy` exactly when its
element type is `Copy`. User records and enums are always affine/non-Copy in V1,
even when every field or payload is `Copy`. `Option<T>`, `Result<T,E>`, and
`TaskResult<T>` are also affine V1 constructed values. `Shared<T>` is an
explicitly documented immutable handle and is `Copy`; strings, bytes,
capabilities, regions, DMA regions, tasks, locks, channels, events, semaphores,
barriers, latches, atomics, slices, closures, and functions are not `Copy`
unless an accepted later contract explicitly changes that type.

At any program point, a value may have either any number of immutable borrows
or exactly one mutable borrow, never both. An immutable borrow cannot mutate
the value; a mutable borrow cannot be aliased. The checker determines a borrow
region from the smallest enclosing expression/block required by use. Because
V1 borrowed values neither escape nor enter a task/aggregate, no inferred
cross-function lifetime notation is needed.

`E1302_CONFLICTING_BORROW` covers any operation that violates the exclusivity of
a live borrow of an overlapping place, not only the creation of a second borrow:
a new borrow incompatible with a live overlapping one; an ordinary owner read or
use of an overlapping place while a mutable borrow is live; an ordinary owner
mutation of an overlapping place while a mutable borrow is live; and a move or
other invalidation of an overlapping place while any borrow, shared or mutable,
is live. `E1303_MUTATE_WHILE_BORROWED` is the specialized case of a write to an
overlapping place while an immutable, shared borrow is live. The accepted matrix
is

```text
shared borrow  + owner write   -> E1303
mutable borrow + owner read    -> E1302
mutable borrow + owner write   -> E1302
any borrow     + owner move    -> E1302
incompatible borrow pair       -> E1302
```

Operations performed through the correct borrow binding itself are not owner
aliases and remain legal according to that borrow's kind. See ADR-0035.

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
serialize it as an integer.

A region's rights are part of its type (ADR-0037). The granted mode is written
inside the type argument — `Region<mut T>`, `DmaRegion<mut T>` — and `mut` is
admitted in a type for exactly these two constructors and nowhere else. The four
facts follow from the mode and are independently rechecked in IR:

| Type | `Copy` | mutable | Shareable | `Transferable` |
|---|---|---|---|---|
| `Region<T>` | no | no | yes | yes |
| `Region<mut T>` | no | yes | no | no |
| `DmaRegion<T>` | no | no | no | no |
| `DmaRegion<mut T>` | no | yes | no | no |

Both DMA variants are conservative in V1: a shareable `DmaRegion<T>` could
become a `Shared<DmaRegion<T>>`, and a `Shared<T>` is `Copy`, so the handle
could be copied into several tasks — the crossing the DMA rule exists to forbid.

Using one region from several tasks is written `share(region)`, a predeclared
operation typed `share(T) -> Shared<T>` only when `T` is transitively immutable
and Shareable. It consumes its argument, so the original name is moved-from and
using it is `E1301_USE_AFTER_MOVE`. An argument that does not satisfy the
requirement is `E1215_ARGUMENT_TYPE_MISMATCH`. Writing through a `Region<T>` is
`E1201_ASSIGN_TO_IMMUTABLE`, and capturing a non-`Transferable` region into a
task or closure is `E1304_INVALID_TASK_CAPTURE` or
`E1305_INVALID_CLOSURE_CAPTURE` with `reason=mutable region` or
`reason=DMA region`. The `Shared<T>` a `share` produces counts against the
module's declared `shared` resource limit.

A value may cross a task boundary only if it is `Transferable`: owned affine
values transfer their sole ownership; immutable `Copy`/`Shared<T>` values are
duplicated; opaque capabilities, mutable borrows, lock guards (whose diagnostic
is `E1402_INVALID_GUARD_LIFETIME`, ADR-0036), and plain
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
