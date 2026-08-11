<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0035: TOS Core V1 `defer` ownership semantics and the borrow-conflict class

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — fixes the ownership meaning of an accepted V1 statement
  form and broadens the stable condition of an already allocated diagnostic
  code, both of which conformance evidence depends on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

The ownership slice of the reference frontend reached two boundaries the
contract described operationally but had not settled semantically.

**`defer` and ownership.** `docs/40` section 5 states when a defer body runs —
in reverse registration order whenever its enclosing block exits — and what it
may not contain (`E1225_INVALID_DEFER`). It does not state what happens to
ownership. Two readings were available and they disagree about the same program:

```tos
take(message);
defer { take(message); }
```

Read as a capture at registration, the second `take` reserves `message` and the
first one is the error. Read as a deferred use, the registration does nothing
and the deferred `take` is the error. The two readings also disagree about
whether the enclosing scope may keep using a resource whose cleanup consumes it.
Choosing by implementation convenience would have fixed a language rule by
accident, so the ownership walk analysed nothing inside a defer body and the
question was recorded as an Architect decision.

**The reach of `E1302`.** The registry condition for `E1302_CONFLICTING_BORROW`
named only borrow-against-borrow, and `E1303_MUTATE_WHILE_BORROWED` only a write
under an immutable borrow. Three operations that violate the exclusivity
`docs/40` section 5 states — "a value may have either any number of immutable
borrows or exactly one mutable borrow, never both" — had no code at all:

```tos
let mut c = Counter(value: 0i32);
let m = borrow mut c;
return c.value;        // owner read under a live mutable borrow
```

```tos
let view = borrow message;
take(message);         // move under a live borrow
```

```tos
let mut c = Counter(value: 0i32);
let m = borrow mut c;
c.value = 1i32;        // owner write under a live mutable borrow
```

Silence on these is unsound: the exclusivity rule is stated, and a checker that
proves it only for one of the four ways to break it does not prove it.

## Decision

### 1. `defer` is deferred lexical cleanup, not a capture

`defer` is a deferred lexically scoped cleanup block. It is not a closure and
does not use the closure-capture rules of `E1305_INVALID_CLOSURE_CAPTURE`.

Executing the `defer` statement registers the cleanup. At that moment:

- the lexical names inside the body bind to the binding identities visible at
  the point of registration;
- the values of those bindings are not read, not borrowed and not moved;
- no ownership effect of the body takes place.

The body runs only when the enclosing block is actually left. On each exit path,
in this order:

1. the action that caused the exit has already been evaluated — the `return`
   operand, the `break`, the `continue`, a propagation;
2. the defers registered on the path actually taken run in reverse registration
   order;
3. the ownership and borrow state left by one defer is the input state of the
   next;
4. only after cleanup do the bindings leave scope and their bounded `drop` run.

A defer body is therefore analysed against the ownership state that exists on
the concrete exit path.

```tos
take(message);
defer { take(message); }        // E1301_USE_AFTER_MOVE inside the defer
```

```tos
defer { take(message); }
take(message);                  // E1301_USE_AFTER_MOVE inside the defer
```

Registering a consuming cleanup deliberately neither reserves nor moves the
value at the point of registration. Ordinary correct use between registration
and exit is allowed:

```tos
let file = open(path);
defer { close(file); }
read(borrow file);
read(borrow file);
```

The obligation is the other way round: a program must leave every defer that can
actually run ownership-valid on every exit path that runs it.

Shadowing after registration does not change which binding a defer refers to.
Binding identity is fixed lexically at the point of registration.

`E1225_INVALID_DEFER` is unaffected. It remains a separate syntactic and typed
restriction on the form of a defer body, not a statement about ownership.

There is one cleanup mechanism. `return`, `break`, `continue`, normal block
exit, and the other contract exits all unwind the cleanups of the lexical blocks
they leave, through the same model. `?` and cancellation use that same model
wherever their flow semantics are already representable; no second cleanup
mechanism is introduced for them.

### 2. `E1302_CONFLICTING_BORROW` covers the whole exclusivity violation

No new `E13xx` code is allocated. The normative condition of the existing code
is broadened.

`E1302_CONFLICTING_BORROW` means any operation that violates the exclusivity of
a live borrow of an overlapping place, not only the creation of a second borrow.
It covers:

1. a new borrow incompatible with a live overlapping borrow;
2. an ordinary owner read or use of an overlapping place while a mutable borrow
   is live;
3. an ordinary owner mutation of an overlapping place while a mutable borrow is
   live;
4. a move or other invalidation of an overlapping place while any borrow —
   shared or mutable — is live.

`E1303_MUTATE_WHILE_BORROWED` remains the specialized case it already was:

- a write or mutation of an overlapping place while an immutable, shared borrow
  is live.

The accepted matrix:

```text
shared borrow  + owner write   -> E1303
mutable borrow + owner read    -> E1302
mutable borrow + owner write   -> E1302
any borrow     + owner move    -> E1302
incompatible borrow pair       -> E1302
```

Operations performed through the correct borrow binding itself are not owner
aliases and stay legal according to that borrow's kind. Reading through a shared
borrow and writing through a mutable borrow remain exactly as legal as before;
only accesses that go around a live borrow to the owning place are affected.

### 3. Region and synchronization guards are out of scope

This ADR does not decide `Transferable` for regions or lock guards and does not
allocate or extend `E1304_INVALID_TASK_CAPTURE` for them. The principle stands:

- `Transferable`, shareable and mutable are read from a proved type, interface
  or capability contract;
- a constructor name alone is not that proof;
- the absence of such information produces no invented diagnostic.

The ownership information interface is prepared so a later slice can distinguish
`KnownTransferable`, `KnownNonTransferable(reason)` and `Undetermined` without
duplicating type resolution. The capability and synchronization contract itself
is left to that slice.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-09 is served — a stable diagnostic
  condition is stated rather than left to an implementation; I-15 is served by
  replacing an undefined ownership meaning with one normative reading.
- **Canonical representation after the change:** unchanged. No source text
  changes meaning at runtime. Programs that were already unsound under the
  stated exclusivity rule are now rejected instead of silently accepted, and a
  defer body whose cleanup was already invalid on some exit path is now named.
- **Trusted-base impact:** none.
- **Source-to-runtime impact:** the frontend now proves the exclusivity rule for
  all four of its violations and analyses cleanup against the state that exists
  where cleanup runs. The IR and verifier contract is unchanged.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** positive and bounded. Broadening `E1302` closes an
  aliasing hole in the safe subset; the defer model adds no unwinding and keeps
  cleanup bounded and deterministic.
- **Performance contract:** none applicable. Path-sensitive defer unwinding is
  bounded by the lexical nesting of blocks.
- **Compatibility profile:** TOS Core 1.0. Both the defer ownership semantics
  and the `E1302`/`E1303` boundary are fixed for V1 and change only through a
  versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** the conformance vectors of section 4
  below, checker unit tests for each row of the matrix and each defer exit path,
  and the mechanical language-contract gate binding the conditions to the
  registry.

### 4. Conformance evidence

For `defer`, at least: a resource still usable after registration; a move before
a deferred consuming use giving `E1301` inside the defer; a return path running
the defer; `break` and `continue` running the defers of only the blocks actually
left; nested defers in LIFO order; shadowing after registration; a defer
registered only on a reached path; and one defer's ownership effect visible to
the next.

For the borrow matrix, at least: an owner read under a live mutable borrow; a
move under a live borrow; and an owner write under a live mutable borrow.

## Consequences

The ownership frontier closes. Cleanup has one model shared by every exit, and
the exclusivity rule of `docs/40` section 5 is proved for every way to break it
rather than for one of them.

The cost is that two things previously accepted in silence are now rejected: an
owner access that goes around a live borrow, and a cleanup body that cannot run
soundly on a path that reaches it. Both were already violations of stated rules;
only the reporting was missing.

## Alternatives considered

**`defer` captures at registration.** Rejected: it would make registration a
move, so a resource could not be used after its own cleanup was registered —
which is the ordinary and intended use — and it would turn a lexical cleanup
block into a closure with capture rules it does not have.

**Analyse a defer body once, against the joined state of all exits.** Rejected:
it would report against a state no execution ever has, and it would hide a
cleanup that is invalid on exactly one path.

**Allocate new codes for owner read, owner write and move under a borrow.**
Rejected: they are one rule — the exclusivity of a live borrow — and splitting
one rule across four codes would make conformance evidence describe an
implementation's internal case analysis rather than the language.

**Fold `E1303` into `E1302`.** Rejected: `E1303` is already accepted evidence
with a distinct, useful meaning, and removing a code from a versioned registry
is a larger change than the one this ADR needs.

**Decide region and guard transferability here.** Rejected: it requires the
capability and synchronization contract, and this ADR must not settle unrelated
semantics.
