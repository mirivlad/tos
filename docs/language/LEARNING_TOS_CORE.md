<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Learning TOS Core

> **Specification status.** This is a learning path for accepted TOS Core V1.
> It is not a claim that a parser or runtime exists. Each linked `.tos` file is the
> one canonical example source; this document intentionally does not duplicate
> snippets that could drift from it.

## 1. What TOS Core is

TOS Core is the textual language whose source, rather than a cache or binary,
is the installed program. Read the [programmer guide](TOS_CORE_V1_GUIDE.md)
first for terminology and the numbered specification for exact rules.
TOS Core 1.0 checks each source file as Unicode 17.0.0 / UAX #15 Revision 57
NFC after newline normalization. That makes the bytes named by source identity
the same on every supported host; it does not make identifiers Unicode.

## Five syntax rules to remember

- `()` — parameters, arguments, and grouping.
- `[]` — lists, data, and declarations.
- `{}` — executable code.
- `,` — separates list items.
- `;` — ends a simple action.
- `return` — returns a normal value explicitly.

These rules are deliberate: a record is declared with `[]`, constructed with
named `()`, and code always lives in `{}`. A final expression does not quietly
become a return value.

The same `()` rule applies to a small anonymous closure: `fn (value: i32) { ... }`.
Fixed array types are written `array<T, N>`, so `;` remains
the marker for an executable action rather than a hidden type-list separator.

## 2. First program

Open [first.tos](examples/first.tos). Notice three things before its function:
the module identity, the language/profile declaration, and the resource
envelope. A small program still says what it may consume; this is how recovery
execution remains bounded.

## 3. Source files and modules

[modules.tos](examples/modules.tos) names another module explicitly. The
path/module relationship and source-set-bound resolver mean that the same
canonical source closure is used after cache deletion or recovery. There is no
implicit prelude or ambient package search.

## 4. Values, bindings, and basic types

[values.tos](examples/values.tos) introduces immutable `let`, mutable `let
mut`, fixed-width integers, `size`, and `duration`. A checked conversion names
its destination, for example `to_u8(value)`, and returns `Result`; a missing or
overflowing conversion is never a quietly target-dependent value.

## 5. Functions

Functions have typed inputs and output, evaluate calls left-to-right, and have
an optional capability-effect list. Read [data.tos](examples/data.tos) for a
function used with records and tuples. The first implementation will check this
source, but it does not exist yet.

## 6. Records, tuples, and enums

[data.tos](examples/data.tos) uses a record for named data, a tuple for a small
fixed grouping, and an enum for alternatives. Its `match` branches contain
explicit `return` actions; neither `match` nor `if` quietly produces a value.
TOS Core V1 chooses nominal records/enums so that types from different modules
are not accidentally interchangeable. Primitive values, suitable tuples, and
arrays Copy; user records/enums otherwise move.

## 7. Option and Result

[results.tos](examples/results.tos) separates "there is no value" from an
ordinary recoverable error. `?` carries an `Err` outward; it does not catch
checked integer traps or runtime panics.

## 8. Conditions and loops

The first program and [resources.tos](examples/resources.tos) use blocks,
conditions, and a metered loop. In Bootstrap an unknown loop consumes fuel;
that is a visible program resource, not hidden interpreter policy.

## 9. Ownership in practice

[ownership.tos](examples/ownership.tos) shows a value moving to a function.
After a move, do not use the old binding. The deliberately invalid
[use-after-move.tos](conformance/v1/reject/use-after-move.tos) explains the
expected `E1301_USE_AFTER_MOVE` diagnostic.

## 10. Borrowing and mutable access

Use `borrow value` to lend read access and `borrow mut value` for the one
temporary writer. V1 borrows stay in a short lexical scope and cannot escape to
a return value or child task. This smaller initial rule is easier to audit than
implicit lifetime inference.

## 11. Errors and diagnostics

Read [the lexical expectations](conformance/v1/EXPECTATIONS.md) and the reject corpus.
A diagnostic is structured evidence: it has a stable code, source path/content
identity, byte span, line/column, stage, and causal records. This lets tools
show the same error after an IR/cache regeneration.

## 12. Modules and imports

Revisit [modules.tos](examples/modules.tos). Imports resolve from declared
source inputs only. An import cycle, missing dependency, or ambiguous declared
root is a deterministic error, not an opportunity to search the host machine.

## 13. Capabilities: authority is not a value you can forge

[capability.tos](examples/capability.tos) requests a typed `Clock` capability
and declares its effect. It does not grant it. [forged-capability.tos]
(conformance/v1/reject/forged-capability.tos) is why a number/string/cast is
not an authority token. Concrete device/service APIs wait for later stages.

## 14. Resource-bounded programming

[resources.tos](examples/resources.tos) records fuel, stack, allocation,
tasks, workers, synchronization, shared-memory, cleanup, recursion, and import
limits. Choose bounds that describe the service you intend to run; a runtime
cannot silently grant more workers or memory than policy allows.

## 15. Async tasks

Async tasks are Full-profile work that waits for a declared typed event/I/O
contract; [async.tos](examples/async.tos) shows scoped spawning and awaiting.
The V1 source form is specified, but no Stage 2 implementation or Stage 3 I/O
contract exists today. This chapter therefore teaches the boundary, not a fake
runnable API: async is scoped, explicitly awaited, and does not promise a
dedicated CPU thread. `await` consumes a `Task<T>` and yields
`TaskResult<T>`; match `Completed(value)` and `Cancelled` explicitly.

## 16. Structured parallelism

[parallel.tos](examples/parallel.tos) is the accepted CPU-parallel shape:
children are created in one lexical scope and joined there. A cancellation
request still requires that final join. A Full engine must
have a true multicore path; Bootstrap may serialize the same work. Correctness
does not depend on which worker ran it.

## 17. Shared data and synchronization

Share immutable data through `Shared<T>`. For mutable data, use a typed lock,
channel, event, barrier/latch, or a typed shared region. The reject case
[shared-mutable.tos](conformance/v1/reject/shared-mutable.tos) demonstrates
that safe mutable sharing is not permitted simply because tasks exist.

## 18. Atomics and when not to use them

[atomic-publication.tos](examples/atomic-publication.tos) illustrates
release/acquire publication. Atomics are for small state/coordination fields;
they do not make arbitrary mutable data safe. Prefer ownership, a lock, or a
channel when that explains the program more directly.

## 19. Bootstrap restrictions

Bootstrap is the same language with explicit restrictions: one worker, bounded
resources, serial parallel execution, and no async/closure/defer/unsafe/FFI/
dynamic loading. The [full-profile-async.tos]
(conformance/v1/reject/full-profile-async.tos) case is intentionally rejected
in Bootstrap rather than silently changing meaning.

## 20. A small realistic module

[counter-service.tos](examples/counter-service.tos) combines a module header,
resource envelope, record/Result values, ownership, and a requested capability
shape. It is deliberately not a Stage 3 service or driver: IPC, device, DMA,
and process APIs do not exist yet. The point is to show how source-level
authority, limits, and diagnostics fit together before those subsystems arrive.

## What comes after this checkpoint

After the Project Architect accepts the semantic/IR contract, this path will
gain tested implementation status one slice at a time: lexer, parser,
diagnostics, checker, ownership, IR/verifier, Bootstrap interpreter, and then
examples that mechanically parse/type-check/execute. Until then, consult
[EXAMPLE_STATUS.md](EXAMPLE_STATUS.md) rather than assuming a canonical example
is executable.
