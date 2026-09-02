<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0077: A launch plan is an object, and it outlives the creation that reads it

- Status: **Accepted**
- Date: 2026-09-03
- Decision level: 2 — it adds three `SYSTEM_ABI_V1` operations, two capability
  object kinds and one nucleus table, and it changes the input shape of
  operations 19 and 20. It changes no TOS Core semantics, no accepted ceiling
  and no physical accounting rule
- Project Architect approval: **given, 2026-09-03**, in the round instruction
  that reserved operation numbers 21, 22 and 23, accepted the LaunchPlan
  concept, rejected the packet's E1 formulation for conflating a finite set of
  nucleus object kinds with an open-ended set of nominal capability interfaces,
  and required 19 and 20 to take the sealed plan as input rather than maintain a
  second endowment mechanism beside it
- Related: ADR-0055 (Accepted) — what an endowment is. ADR-0061 (Accepted) — how
  an endowment binds to a module. ADR-0063 (Accepted) — an operation that
  requires two capabilities. ADR-0067 (Accepted) — restart generations and how a
  supervisor learns a service ended. ADR-0075 (Accepted) — the consuming
  transition this is shaped after. ADR-0076 (Accepted) — the funding this is
  the endowment half of. `SYSTEM_INTERFACE_V1` §4, §4.1, §5

## 1. An endowment that belonged to nobody

Before this, a child's endowment was a table in the creator's own argument
region. A parent wrote up to four entries at `CREATE_ENDOWMENT`, said how many
in a register, and called operation 19; the nucleus read the table at the
instant of creation and built the child from it.

Three things follow from that shape, and all three are wrong for a supervisor.

**It is valid for one call.** The table is the caller's scratch memory. Any
other operation that writes the argument region overwrites it, so the endowment
has to be rewritten immediately before every creation — and "immediately before"
is a property nothing checks.

**It is held by nobody.** Between the moment a parent decides what a child will
hold and the moment the child is built, the decision exists only as bytes in a
page. If the parent releases one of the capabilities it named, the entry now
names nothing; if the parent is restarted, the decision is gone.

**A restart is therefore a second decision, not the same one.** ADR-0067 gives a
supervisor a restart generation so that a replacement is recognisably a
replacement. But what the replacement is *endowed with* was re-derived, at
restart time, from whatever the supervisor could still reach. Two launches of
one service under one policy could differ, and nothing in the system would say
so.

The problem is not that the mechanism is unsafe. It is that launch policy had no
object. Everything else in this system that persists a decision is a capability
naming a nucleus object with a lifetime; this alone was a page of bytes.

## 2. The decision: a launch plan is a capability

A **launch plan** is bounded nucleus metadata naming up to `MAX_ENDOWMENT`
capabilities, each with the rights it carries and the binding it answers
(ADR-0061). It has two public states and one lifetime.

`launch_plan_create` (operation 21) makes an empty **builder**. It requires
process authority with `create`.

**Creation authority is required for a thing that creates nothing**, and that is
deliberate. A plan grants access to no object and can reach nothing its author
could not reach by calling directly; requiring `create` is not about what a plan
can do, it is about who may hold one. A process that may not create children has
no business accumulating launch policy for them, and — since the plan table is
bounded — no business occupying it with decisions nothing will ever apply.

**A plan capability carries no rights.** Holding it is the authority over it.
Every operation on a plan is decided by the object's *kind*, which is its state,
and by the creation authority that operation separately requires. A rights field
would be a second place the same decision is made, and two such places
eventually disagree.

## 3. Writing one: one selector, every kind of authority

`launch_plan_endow` (operation 22) adds one entry. Its first capability is **the
one being delegated**; the builder is second.

That ordering is the whole design. The operation is reached *through* the
authority it delegates, so:

- the right to place an endpoint in a plan is holding that endpoint. There is no
  general "may endow" right that anybody was granted, and therefore none that
  could be over-granted;
- the rights recorded are the intersection of what was asked for with what the
  caller holds. A caller asking for more receives less, so widening is not
  refused so much as unexpressible;
- and there is **one** ABI selector for every kind of authority there is. An ABI
  is finite; an interface set is open-ended. A number per interface would make
  the first grow with the second, which is the formulation this ADR was directed
  to reject.

`SYSTEM_INTERFACE_V1` declares this as one operation on each interface whose
capabilities may be a startup endowment, differing only in the interface of the
first parameter. The exact nominal type is therefore retained at every call
site: there is no `AnyCapability`, no integer handle and no erased capability
value anywhere in TOS Core. A module that endows two kinds of authority declares
two `extern` items of the same name, and which one a call reaches is decided by
the interface of its first argument — the same rule `SYSTEM_INTERFACE_V1` §4.1
already used to decide which interface an operation is performed under.

**The plan takes a reference of its own on what each entry names.** This is what
makes it a holder rather than a note. A creator may place a capability in a plan
and then release its own handle; the plan goes on naming it, because the plan is
now the thing that holds it. That is the opposite of inheritance: nothing is held
implicitly, and what a plan holds is exactly what somebody wrote into it.

**Three kinds are refused**, each for its own reason and none of them
incidental:

- a **region**, because a capability is half of what a holder needs and the
  other half is a mapping in an address space that does not exist when the entry
  is written. Operation 20 is where a process is created *with* a region;
- a **reply**, because it names one call of one caller and is single-use. No
  accepted contract makes one a startup endowment, and inventing that here would
  be this ADR deciding something `IPC_V1` has not;
- a **plan**, because an entry naming another plan hands a child a decision its
  parent is still holding, leaving two holders of one affine object.

## 4. Sealing: the shape a region already established

`launch_plan_seal` (operation 23) consumes the builder and replaces it **in the
same capability slot at an advanced generation** with a sealed plan. It is the
transition `region_freeze` performs, applied to a different kind: one object
throughout, no reference taken or dropped, no free table slot required, and the
old handle detectably stale.

After sealing the entries cannot change. That is what makes a plan a decision
rather than a buffer.

Sealing requires the same creation authority that making one required, for the
same reason: the two operations bracket one activity, and a process that may
hold an unfinished plan and not a finished one would be a distinction with
nothing behind it.

## 5. Creating from one, and why it survives

Operations 19 and 20 take the **sealed** plan as an input capability and derive
the child's endowment from it, atomically, as part of the one transaction that
already charges the child's footprint. A builder is refused: a decision still
being written is not one anything may be created from.

**The plan is not consumed by a successful creation.** This is the point of the
whole ADR. A restart is the same service policy applied to a new process
instance, and a plan a creation took would make the second launch a second
decision. So:

```text
same sealed plan
same shared bundle
new process instance
```

There is deliberately no second endowment mechanism beside it. The raw
`CREATE_ENDOWMENT` area of the argument region is **removed**, not deprecated: a
table that is still read is a second way to endow a child, and two ways to decide
one thing is what this ADR exists to remove. Operations 19 and 20 have never been
declared by `SYSTEM_INTERFACE_V1` and have no caller outside this repository, so
the register reshuffle that follows is a change to unreleased operations rather
than a compatibility break — and it is the last one either will get.

Failure preserves everything: the plan, the funding capability, the bundle
capability, and every derived reference the plan holds. There is no partial
child, which is ADR-0055's rule unchanged.

**No plan entry grants a reservation of its own.** An endowment of a memory
authority is another *name* for one budget (ADR-0076 §2b). A plan that reserved
on its own account would be a way to spend a supervisor's funding by writing
policy, which is exactly the ambient spending ADR-0076 retired.

## 6. Lifetime: exactly one death

A plan is **affine in both states**, so exactly one capability names it at any
time. The loss of that name — an explicit `capability_release`, or the clearing
of a dead process's table — destroys the plan and releases every reference its
entries took, once each.

Affinity here is not the region's argument. A region is affine because a second
handle to a mutable one is a second writer. A plan is affine because it is the
*holder* of the references its entries describe, and an object whose destruction
releases references must have exactly one death. A copyable plan would release
each entry as many times as it had been copied.

For the same reason a plan is refused by generic capability transfer, by IPC
delegation and by `Endowment::Existing`: every one of those copies, and a copy
would produce a second holder of one decision.

## 7. Two object kinds, not one with a flag

`CAPABILITY_V1` gives a region's two affine states **one** public object kind,
on the ground that the public kind space should not be widened by an internal
distinction and that a process learns what it may do from its rights.

A plan's two states get **two** kinds, and the difference is about interfaces
rather than about objects. A region's two forms declare the same operations; a
builder and a sealed plan declare different ones — a builder is written to and
sealed, a sealed plan is created from, and neither may be used where the other
belongs. A launcher answering `import capability system.process.LaunchPlan` with
a builder would be answering a request for a decision that has been made with
one that has not, and the object-kind check at startup (ADR-0061) exists exactly
to refuse that.

The nucleus decides state from the *variant*, never from the rights. That is
ADR-0075's rule about structural affinity, applied to a second kind.

## 8. What this does not decide

**It does not make an endowment reachable from text for capabilities obtained at
runtime.** `SYSTEM_INTERFACE_V1` §4.1 admits a capability *value* parameter — a
plan produced by operation 21 travels to 22, 23 and 19 as an ordinary module
value — but the operation's own first capability is still supplied from an
`import capability`. That boundary is `tos-ir/v1`'s: `Op::Capability` names the
operation's own capability as an import index and nothing else. Every operation
in this ADR is shaped so the question does not arise; widening it is a decision
about the IR, and this ADR does not take it.

**It does not change what a child does with what it holds.** A plan decides what
a process is given. ADR-0061 still decides how each grant binds to the child's
`import capability` requests, and the child still reports `CapabilityDenied` for
a request nothing answered.

**It does not add restart policy.** A plan makes a restart able to be the same
decision; *when* to restart, how often, and when to stop is service supervision
policy and belongs in canonical text, not in the nucleus.

## 9. Conformance evidence

1. A plan is made, written through two different nominal capability interfaces,
   sealed, and used to create a funded process — from TOS Core text, with the
   plan travelling between five calls as a typed value and no raw handle
   anywhere in the source or the artifact.
2. A builder is refused where a sealed plan belongs.
3. An entry naming a capability its author does not hold is refused **when the
   entry is written**, not when a child is created from it.
4. One sealed plan creates two processes; the second creation is the same
   decision as the first, and the plan is unchanged by either.
5. A creator releases its own handle to a capability it placed in a plan, and
   the plan goes on holding it — evidenced as the same reservation refused and
   then granted, with only the plan's release in between.
6. Every plan is destroyed by the end of a boot: the nucleus reports the live
   plan count beside the frames and page tables it reclaims, and it is zero.
