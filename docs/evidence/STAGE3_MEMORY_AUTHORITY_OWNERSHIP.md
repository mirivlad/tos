<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Decision packet — how many capabilities may name one `MemoryAuthority`

- Status: **open — Project Architect decision required before `Object::MemoryAuthority` and operation 16**
- Date: 2026-09-01
- Why it is a STOP: the accepted corpus fixes what operation 16 does and rules
  out one wrong answer, but does not settle whether a second capability may name
  an authority node that already has one. `Authority.named: bool` — the field
  the whole reservation lifecycle turns on — is only correct under one of the
  two answers
- Related: ADR-0075 §2, §2a, §6; ADR-0076 §2b, §3, §9; `CAPABILITY_V1` §3, §4;
  `IPC_V1` §6

## 1. What the corpus already settles

**Operation 16 is not aliasing.** ADR-0076 §9: `capability_attenuate_scoped`
over a `MemoryAuthority` "makes a child accounting node and reserves its scope
out of the parent atomically. It is not a second handle onto one counter."
ADR-0075 §2 B reaches the same place from the other side: attenuation is
*refinement*, refinement reserves nothing, and "allocation is not refinement,
and no amount of scope-narrowing turns one into the other."

**Generic attenuation is refinement and does not consume.** `CAPABILITY_V1` §4:
attenuation "produces a new capability whose rights, scope and lifetime are each
a subset of the input's", and ADR-0075 §2 B relies on it "not consuming the
input". So whatever operation 5 is allowed to do to a `MemoryAuthority`, it
cannot reserve anything and it leaves the caller holding what it had.

**Linearity is expressible per object kind, and has been used once.**
`CAPABILITY_V1` §4 defines transfer of a *linear* capability as consuming the
sender's handle atomically. ADR-0075 §5a makes a mutable `Region` linear on
exactly that basis.

**Delegation aliases by default.** `CAPABILITY_V1` §4: "The receiver gets its
own handle, in its own table, with its own generation." Nothing there is
conditional on the object kind, so an object that must *not* be aliased has to
say so.

## 2. What the corpus does not settle

**Whether `MemoryAuthority` is linear.** It is never said. ADR-0075 §5a makes
`Region` linear and gives the reason (a writable alias defeats the freeze); no
equivalent statement exists for an authority, and the reason for `Region` does
not transfer — an authority has no backing to alias.

**What the build-worker lifecycle actually requires.** ADR-0076 §3 says:

```text
supervisor allowance
  └── attenuate: worker allowance = grant + bundle budget
        ├── process_create_funded(…, worker allowance, grant = W)   charges W
        └── the remainder is endowed to the worker explicitly, in its
            endowment and its launch record, and pays for the bundle region
```

The supervisor holds the worker allowance to fund the creation from it, and the
same node is then endowed to the worker. **Whether the supervisor still holds it
afterwards is not stated.** If it does, two capabilities name one node and the
answer is B. If it does not, the endowment consumed the supervisor's handle and
the answer is A — but then a supervisor cannot revoke a stuck worker's unspent
reservation, and the only way that memory comes back is the worker dying.

**What `named: bool` means with more than one handle.** The field drives the
whole reservation lifecycle: ADR-0075 §2a returns an authority's unspent
remainder upward when it stops being named, and keeps the accounting node alive
while its allocations and descendants drain. Today the nucleus sets `named` from
one revoke. With several handles that is simply wrong: the first release would
return a remainder two other holders still believe is theirs.

**What the current capability layer already permits.** `capability_release`
removes an entry; `capability::clear()` at process death removes entries without
telling the object; `Endowment::Existing` hands the same object to another
process; `capability_attenuate` (5) makes another handle onto the same object.
For `Endpoint` and `Process` that is sound. For `MemoryAuthority` each of those
is a path to a second name for one budget.

## 3. The two answers, stated so they can be chosen between

### A — a `MemoryAuthority` capability is affine: at most one live handle

What must then be true, and proved:

- operation 5 may not produce a handle carrying `spend` over a
  `MemoryAuthority`;
- endowment and delegation of a `MemoryAuthority` are **consuming**: the giver's
  entry goes atomically with the receiver's acquisition, as `CAPABILITY_V1` §4
  already defines for a linear capability;
- `capability_release` and process death make the node unnamed at once, and the
  unspent remainder returns upward immediately;
- `named: bool` stays correct exactly as written, and no reference count is
  needed.

Cost: the supervisor gives up the worker allowance when it endows it. It cannot
revoke that reservation afterwards, and cannot fund a second creation from it.
Whether the ADR-0076 §3 worker lifecycle can be expressed under this needs to be
answered, not assumed.

### B — several capabilities may name one node

What must then be true, and proved:

- a bounded `capability_refs` on the accounting node, not `named: bool`;
- every path that creates a name increments it: endowment, delegation, operation
  5's alias if allowed;
- every path that destroys one decrements it: release, process death (once per
  entry actually destroyed), revoke;
- the node becomes unnamed and returns its unspent remainder **only** when the
  last name goes;
- allocations, charges and descendants keep the node alive past that, and their
  bytes return along the original lineage later, exactly as ADR-0075 §2a already
  says;
- generation and slot reuse keep stale handles from resolving.

The rule that must not be lost either way: **several handles are several names
for one accounting node and one budget, never several reservations.**

## 4. Recommendation

**B**, for one reason that is about the system rather than the implementation:
the ADR-0076 §3 worker shape reads naturally only if the supervisor keeps the
allowance it funded from, and a supervisor that cannot revoke a stuck worker's
unspent reservation has lost the ability ADR-0075 §2a was written to give it.
A is simpler and its simplicity is not free — it makes endowment of an authority
a one-way door, which is a policy the corpus never asked for.

But B costs a reference count on a load-bearing lifecycle, and the count has to
be right in four places (`grant`, `endow`, `release`, `clear`) that today do not
know the object kind they are touching. That is a real change to
`capability.rs`, not a field.

**This is a Level-2 decision and it is not mine to take.** The corpus rules out
the alias reading of operation 16 and says nothing about the rest.

## 5. Also to be settled in the same answer

**What operation 5 does to a `MemoryAuthority`.** Three candidates, and the
choice interacts with §3 above:

1. rights-only refinement (a handle that can be released or delegated but not
   spent) — coherent only if a `spend`-less authority handle is useful;
2. refused outright for this object kind — the narrowest reading, and the one
   that makes "16 is not 5" impossible to misread;
3. something else, explicitly proved not to create the reservation semantics
   operation 16 owns.

A request through operation 5 that keeps `RIGHT_SPEND` must not accidentally
produce whichever of A/B is not chosen.

**Whether the initial supervisor holds the root or a child of it.** Its
endowment is explicit either way — there is no "give me the root" system call
and nothing is inherited (ADR-0076 §3). But if the supervisor holds the root
itself, then under A the boot has given the root away, and under B the root's
lifetime is tied to a process that may be restarted. If the corpus does not fix
this and the choice affects root lifetime or restart, it belongs in the same
decision.

## 6. What is already done and is not waiting on this

The funding spine, the transaction, the root endowment and the tree-wide funding
stop are all in and green. Nothing in this packet changes them; what it decides
is how a `MemoryAuthority` becomes reachable from ring 3 at all.
