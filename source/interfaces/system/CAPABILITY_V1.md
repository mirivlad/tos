<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Capability Contract — Version 1

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0048 (Project Architect-approved, 2026-08-12), which fixes the
boundary this contract describes.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, and to
the language half of the model already fixed by docs/42 §2 under ADR-0028.

## 1. Role

docs/42 §2 fixed what a capability means *in source*: a request rather than a
grant, an opaque value of a nominal type, unforgeable, non-encodable,
non-recreatable, attenuable only downward. It also said the concrete interfaces
belong to later stages and must be separately versioned. This is that contract
for Stage 3: what a handle **is**, who owns the table, and what the nucleus does
when one is presented.

The language contract and this one must agree at exactly one point: a value the
checker treats as a capability corresponds to an entry in the holder's table,
and nothing else in the system does.

## 2. Representation

A capability is named by a **handle**: a process-local index into a
nucleus-owned table. It is not a pointer, not a token, not a signed bearer
value, and it carries no rights in its own bits.

| Property | Rule |
|---|---|
| Scope | process-local; the same index in two processes names different things, or nothing |
| Storage | the table lives in nucleus memory and is not mapped into the process |
| Contents | object, rights, scope, lifetime, generation |
| Validity | index in range **and** generation matching |

The generation is what makes a stale handle detectably stale. Releasing a
capability and reusing its slot must not let an old index silently address the
new occupant — the same reasoning that puts a generation in a memory grant
(ADR-0050 §2).

Because the table is nucleus-owned, docs/42's non-forgeability rules cost the
implementation nothing to honour: a process cannot construct a handle, because
constructing one would mean writing into a table it cannot address. A guessed
index either misses, or hits an entry the process was already given.

**Where the entries come from** (ADR-0055, ADR-0075 §5). `SYSTEM_ABI_V1` creates
no ambient authority. A capability an operation returns must have an explicitly
defined normative origin: either authority the caller presented to that
operation, which bounds what is produced, or an explicitly accepted bounded
self-only creation rule. No operation creates authority over a pre-existing
external object out of nothing, and no operation widens what its caller held.
An operation reachable *without* a capability that creates authority would be
ambient authority with a handle in front of it, and there is none. A process's table is written by the nucleus **before the process is
entered**, from the endowment the party that launched it decided. The endowment
travels in the launch record (`LAUNCH_VERSION` 2), and `process_create` carries
the same shape from a parent to a child, where every entry must be an
attenuation of something the parent itself holds.

The recursion terminates at the boot process, whose endowment is the launcher's
own stated constant until `/system/policy/` exists (ADR-0051 §3). That constant
is named in the audit record rather than implied, because a default is what
nobody decided, and authority whose root cannot be named is authority nobody
granted.

Two consequences worth stating: a process cannot widen its table, only shrink it
(`capability_release`) or refine it (`capability_attenuate`); and a parent
cannot grant what it does not hold, which is what makes escalation by spawning
impossible rather than merely policed.

## 3. What a capability names

```text
capability = object + rights + scope + lifetime + generation
```

- **object**: the endpoint, region, process or interface publication it refers
  to; never a class of objects, never "all of them";
- **rights**: a finite set from the object type's declared rights — for an
  endpoint `send`, `receive` and `call` (`IPC_V1` §2); for a process `create`,
  `terminate` and `wait_child` (ADR-0067), which are exactly the operations of
  `SYSTEM_ABI_V1` §5 that name one; for a **region** `read`, `write` and
  `share`, which is what ADR-0037's accepted type model requires — a
  `Region<mut T>` is readable and writable and is neither shareable nor
  transferable, so `write` and `share` never appear together, and a
  `DmaRegion` is granted neither `share` nor transfer in V1;
- **scope**: the range, subset **or finite resource amount** the rights apply
  to, where the object has one. An amount never widens, so a derived authority
  may spend at most what its parent had.

  **A quantity is reserved by scoped attenuation, and by nothing else.**
  Generic attenuation refines *rights* and leaves an amount exactly as it was:
  what it produces is another name for the same authority, spending from the
  same remainder (ADR-0076 §2b). Reserving a smaller amount out of a larger one
  is a different operation — it makes a new accounting node and reduces the
  parent's remainder by what the child may spend (ADR-0075 §2a) — and the two
  must not be reachable through one call, because one of them changes what
  everybody else can spend and the other does not. So:

  | | object | amount | effect on the parent |
  |---|---|---|---|
  | generic attenuation | the same one | unchanged | none |
  | scoped attenuation | a new child | the child's | its remainder falls by that |

  An earlier revision of this section said an amount "narrows under
  attenuation", which read as though one operation did both;
- **lifetime**: bounded by the object, and never longer than the grantor's own.

A capability with unbounded scope is not a capability, it is ambient authority
with a handle in front of it. docs/02 rules out ambient global privilege, and
this is where that rule is enforced or lost.

## 4. Attenuation, delegation, transfer, revocation

**Attenuation** produces a new capability whose rights, scope and lifetime are
each a subset of the input's. The nucleus checks the subset relation; it does
not take the caller's word. Widening is not an error code, it is impossible to
express: there is no operation that adds a right.

**Delegation** is sending a capability over an endpoint (`IPC_V1` §6). The
receiver gets its own handle, in its own table, with its own generation. Nothing
about the sender's index is visible.

**Transfer** of a linear capability consumes the sender's handle atomically with
the receiver's acquisition. A failed send does not consume; a successful send
does not leave a copy. There is no window in which both hold it, and none in
which neither does.

**Whether a second capability may name an object is a property of the object,
not of the rights on any handle to it.** A region is affine while it is mutable
and while it is immutable-and-unshared, and copyable once `share` has consumed
the affine form (ADR-0037 §3–§4, ADR-0075 §3); an immutable affine region and a
shared one carry the same absence of `write`, so a rule that read affinity off
the rights mask would treat them as the same thing and let attenuation turn one
into the other by dropping a bit. An implementation therefore asks the object,
and an attenuation that narrows rights never changes what may hold the result.

**A consuming transition keeps the capability's slot and advances its
generation.** Where an accepted decision makes an operation consume a
capability and return another naming the same object — `share`, and the
mutable-to-immutable transition of ADR-0075 §3 — the presented handle must go
stale rather than silently acquire different rights, because a process cannot
otherwise tell the state it asked for from the state it had. Reusing the slot is
what keeps the object's reference count at exactly one across the transition,
and it is why neither operation needs a free slot to succeed.

**Revocation** exists where the object's owning service defines it, as docs/12
requires. Stage 3 provides the mechanism the owner needs — invalidating derived
capabilities by generation — and does not invent a global revoke: a system-wide
revoke primitive would be an ambient authority to destroy authority.

## 5. Validation cost

Validation is index bounds, generation compare, type compare and a rights mask
test: **constant time with respect to the number of capabilities the process
holds**, as docs/35 §Stage 3 requires. If an implementation ever needs a
structure whose lookup is not constant-time, the alternative bound is documented
and tested rather than quietly accepted, because a validation cost that grows
with holdings is a denial-of-service channel against the most privileged
processes.

## 6. Interface publication

The right to publish an interface is itself a capability, whose nominal type is
the interface (ADR-0051 §2). A process that holds it may register; one that does
not, cannot. There is no self-declared `provides`, and the registry never holds
an entry no one granted.

## 7. Conformance evidence

1. **Forgery**: a process that writes arbitrary values where its handles live
   gains nothing; handles are indices into a table it cannot address.
2. **Guessing**: iterating every index in range yields only capabilities the
   process was granted, and out-of-range indices yield `E_BAD_HANDLE`.
3. **Staleness**: a released handle reused for a different object refuses the
   old index by generation.
4. **Attenuation**: for a generated set of right/scope pairs, no attenuation
   produces a superset in any dimension.
5. **Linearity**: after a successful transfer the sender's handle is invalid and
   the receiver's is valid — checked for both a normal send and a send that
   fails midway.
6. **Confused deputy**: a broker holding a strong capability, asked by a weak
   client to act on an object the client cannot name, refuses; the refusal is
   attributable to the client in the audit record. This is the test docs/37
   names explicitly, and it is the one that fails quietly in systems that pass
   the other five.
7. **Denial**: a module requesting a capability policy withholds starts with
   `CapabilityDenied`, not with a null, a zero handle or a working default.
