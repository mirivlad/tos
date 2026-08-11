<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0036: TOS Core V1 synchronization guard representation

- Status: **Proposed** — needs Project Architect approval to become Accepted
- Date: 2026-08-11
- Decision level: 2 — adds type constructors to the accepted V1 type surface,
  which conformance evidence and the IR type table depend on
- Project Architect approval: *(pending)*

## Context

`docs/41` section 4 gives `Mutex<T>` a lock that "grants an affine mutable
guard", and `RwLock<T>` "multiple immutable read guards or one affine write
guard". A guard "cannot await, cross a task boundary, or be dropped after its
lock resource disappears". `docs/40` section 6 lists a lock guard among the
values that are not `Transferable`.

The V1 type surface names no guard. `docs/39` section 3 lists `Mutex` and
`RwLock` among the parameterized constructors, but nothing for the value a lock
operation yields, and `docs/39` gives no `lock` operation either. So a checker
cannot establish that a value *is* a guard except by guessing from the
constructor name of the object it came from — which ADR-0035 section 3 forbids,
because a synchronization object is not its guard.

The consequence is concrete: the ownership slice reports nothing for guards, the
IR has no type for one, and the verifier cannot check the guard rules `docs/43`
section 3 lists under its synchronization family. Every one of those is blocked
on the same missing name.

## Decision

### 1. Three guard type constructors join the V1 type surface

```text
MutexGuard<T>      the affine mutable guard a Mutex<T> lock grants
ReadGuard<T>       an immutable read guard an RwLock<T> grants
WriteGuard<T>      the affine write guard an RwLock<T> grants
```

Each takes exactly one type argument, the type the lock protects. They join the
`predeclared-type`/`constructed_type` productions of `docs/39` section 3 and the
fixed-arity table of `docs/40` section 2, so `E1204_TYPE_ARGUMENT_ARITY` covers
them without a new rule.

They are **not constructible from source**. There is no constructor syntax for a
guard; a guard value exists only as the result of a lock operation. Writing one
as a constructor is the nonconstructible-type error of ADR-0039.

### 2. Lock operations

```text
Mutex<T>.lock()      -> MutexGuard<T>
RwLock<T>.read()     -> ReadGuard<T>
RwLock<T>.write()    -> WriteGuard<T>
```

Each is a typed operation on the synchronization object, in the same
receiver-operation form the atomics already use. Releasing is the guard's
bounded `drop`: there is no `unlock` operation taking a guard, because a
released guard that still had a name would be exactly the use-after-release the
affine rule exists to prevent.

### 3. What the three types are

- `MutexGuard<T>` and `WriteGuard<T>` are affine and non-`Copy`, grant mutable
  access to the protected value, and are **not** `Transferable`: they may not
  cross a task boundary, be captured by a task or closure, be returned, stored
  in an aggregate, or sent through a channel.
- `ReadGuard<T>` is affine and non-`Copy` and grants immutable access. It is
  likewise not `Transferable`.
- No guard may be held across an `await`.

A guard's scope is its binding's block. Its `drop` releases the lock and is
bounded: it allocates nothing, awaits nothing and acquires no authority.

### 4. Diagnostics

No new source diagnostic is allocated. A guard crossing a task or closure
boundary is `E1304_INVALID_TASK_CAPTURE` or `E1305_INVALID_CLOSURE_CAPTURE` with
`reason=lock guard`, which is the reason `docs/40` section 6 already names. A
guard held across `await` is `E1401`-adjacent but distinct, and is deliberately
**left open**: it needs the async slice, and this ADR does not decide it.

The verifier family `V2031_SYNC` gains the guard rules: a guard operand may not
appear in a spawn capture, a closure capture, an aggregate, a return, or a
channel operation.

### 5. Conformance evidence

At least: a positive taking and releasing a mutex guard within a block; a
positive taking two read guards of one `RwLock`; a negative capturing a guard
into a task; a negative returning a guard; and a negative applying a constructor
to a guard type.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-09 and I-15 are
  served by naming what was described but unnamed.
- **Canonical representation:** unchanged. No accepted source becomes invalid —
  V1 source cannot name a guard today, so nothing can break.
- **Trusted-base impact:** none. **Threat-model impact:** positive: the guard
  rules become checkable instead of unstated.
- **Compatibility profile:** TOS Core 1.0; the three constructors are fixed for
  V1 and change only through a versioned language decision.
- **Tests:** the five conformance cases above, checker unit tests per rule, and
  the mechanical gate binding the constructors to the arity table.

## Consequences

The synchronization slice becomes implementable, and `V2031_SYNC` stops being a
family with no rules. The cost is three more names fixed for V1.

## Alternatives considered

**Infer a guard from the receiver's type.** Rejected: it is the guess ADR-0035
forbids, and it cannot distinguish a guard from the object once the value is
passed on.

**One `Guard<T>` for all three.** Rejected: a read guard and a write guard have
different aliasing rules, and one type would make the difference invisible
exactly where the verifier has to see it.

**Model release as an `unlock(guard)` operation.** Rejected: it leaves a named
guard after release, which is the use-after-release the affine rule prevents.
