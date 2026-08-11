<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0036: TOS Core V1 synchronization guard representation

- Status: **Proposed (revision 2)** — direction approved by the Project
  Architect; this text needs approval to become Accepted
- Date: 2026-08-11
- Decision level: 2 — adds type constructors and one diagnostic code to the
  accepted V1 surface, which conformance evidence and the IR type table depend on
- Project Architect approval: *(pending)*
- Supersedes: revision 1 of this ADR, which left the normative "a guard may not
  be held across await" rule with no source diagnostic, said nothing about the
  lifetime relation between a guard and its lock, and did not say that the
  checker and the verifier must prove the same rule independently

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

### 4. The lifetime relation between a guard and its lock

Acquisition creates a checkable dependency: **the synchronization object must
outlive every guard it granted.** A guard names a resource inside that object,
so an object that is moved or dropped while one of its guards is live would
leave the guard naming nothing — the exact condition the affine rule exists to
prevent, one level up.

Moving a guard between bindings of the same scope does **not** release the lock.
A guard is affine, so a move transfers ownership of the guard *and the release
obligation with it*; the lock is released by the bounded `drop` of whichever
binding finally owns it. That is what makes a guard usable at all: a helper may
take one, and releasing on every move would release it at the first hand-off.

### 5. `E1402_INVALID_GUARD_LIFETIME`

Stage `type`, in the `E14xx` concurrency family. One code covers every
prohibited lifetime or escape operation on a guard, with a structured
`operation` field naming which:

```text
operation=held_across_await     a guard is live across an `await`
operation=returned              a guard is returned from a function, task or
                                closure body
operation=aggregate             a guard is placed into a record, enum, tuple or
                                array
operation=channel               a guard is sent through a channel
operation=task_boundary         a guard is moved or captured across a task or
                                closure boundary
operation=lock_outlived         the synchronization object is moved or dropped
                                while one of its guards is live
```

The diagnostic also carries the guard type and the source position where the
guard was acquired, because a lifetime finding that does not say where the
lifetime started cannot be acted on.

**Precedence, so nothing is reported twice.** A guard crossing a task or closure
boundary is `E1402_INVALID_GUARD_LIFETIME` with `operation=task_boundary`, and
**not** `E1304_INVALID_TASK_CAPTURE` or `E1305_INVALID_CLOSURE_CAPTURE`. The
capture codes keep their meaning for every other non-`Transferable` value; a
guard is routed to the guard-specific code because its rule is about the
guard's lifetime rather than about transferability alone, and a single reading
is what keeps the two families from overlapping. `docs/40` section 6 is amended
to say so: it continues to list a lock guard among the values that are not
`Transferable`, and it records that the diagnostic for one is `E1402`.

### 6. The checker and the verifier prove the same rule independently

`V2031_SYNC` gains exactly the rules of section 5, restated over IR: a guard
operand may not appear in a spawn capture, a closure capture, an aggregate
construction, a return, or a channel operation; a guard value may not be live
across an await; and a synchronization object may not be moved or dropped while
a guard derived from it is live.

Neither component may take the other's word for it. The verifier reaches the
conclusion by its own traversal of the IR, as `docs/43` section 5 requires, and
the frontend's success is not an input to it. A guard rule the checker enforces
and the verifier does not would be a rule an alternate frontend could skip.

### 7. Conformance evidence

At least: a positive taking and releasing a mutex guard within a block; a
positive taking two read guards of one `RwLock`; a positive moving a guard into
a helper binding and releasing it there, proving a move is not a release; a
negative capturing a guard into a task (`operation=task_boundary`); a negative
returning a guard (`operation=returned`); a negative holding a guard across an
`await` (`operation=held_across_await`); a negative placing a guard into a
record (`operation=aggregate`); a negative dropping the mutex while its guard is
live (`operation=lock_outlived`); and a negative applying a constructor to a
guard type. Each has a matching forged-IR negative for `V2031_SYNC`.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-09 is served —
  `E1402` becomes part of the versioned diagnostic boundary; I-15 is served by
  naming what was described but unnamed.
- **Canonical representation:** unchanged. No accepted source becomes invalid —
  V1 source cannot name a guard today, so nothing can break.
- **Trusted-base impact:** none. **Threat-model impact:** positive: the guard
  rules become checkable instead of unstated.
- **Compatibility profile:** TOS Core 1.0; the three constructors are fixed for
  V1 and change only through a versioned language decision.
- **Tests:** the nine conformance cases of section 7 with their forged-IR
  counterparts, checker unit tests per `operation` value and for the precedence
  against `E1304`/`E1305`, and the mechanical gate binding the constructors to
  the arity table and `E1402` to the registry.

## Consequences

The synchronization slice becomes implementable, and `V2031_SYNC` stops being a
family with no rules. Every prohibited guard operation has a code, so no
normative rule about guards is left without a way to report it.

The cost is three type names and one diagnostic code fixed for V1, and one
routing decision: a guard crossing a task boundary is reported as a guard
lifetime finding rather than as a capture finding.

## Alternatives considered

**Infer a guard from the receiver's type.** Rejected: it is the guess ADR-0035
forbids, and it cannot distinguish a guard from the object once the value is
passed on.

**One `Guard<T>` for all three.** Rejected: a read guard and a write guard have
different aliasing rules, and one type would make the difference invisible
exactly where the verifier has to see it.

**Model release as an `unlock(guard)` operation.** Rejected: it leaves a named
guard after release, which is the use-after-release the affine rule prevents.

**Report a guard crossing a task boundary as `E1304`/`E1305`.** Rejected as the
primary reading: the same program would then be describable by two codes from
two families, and a conformance expectation would have to pick one without the
contract saying which. One guard-specific code with an `operation` field says
exactly what happened and leaves the capture codes their own meaning.

**Release the lock when a guard is moved.** Rejected: it would release at the
first hand-off, so a guard could never be passed to a helper, and the
release point would depend on binding structure rather than on ownership.
