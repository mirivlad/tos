<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0075: Region object lifecycle, the mutable-to-immutable transition, and reclamation

- Status: **Draft, semantically settled — not accepted, nothing implemented.**
  The model, the budget shape and the freeze transition are fixed; only the
  operation names, numbers and registers are left open, and none of them is
  needed by the next production slice, so this ADR no longer blocks
  implementation work
- Date: 2026-08-29
- Decision level: 2 — it would fix what a system Region object is, how one comes
  to exist lawfully, how a writable region becomes an immutable one, and when
  its memory returns to the pool. It changes no TOS Core semantics and no
  accepted ceiling
- Project Architect approval: **the direction is given** (2026-08-29/30):
  attenuation is refinement and not allocation, the origin is a finite
  `MemoryAuthority`, the budget model is the hierarchical attenuable one, and
  G7 is a consuming transition whose postcondition the nucleus proves. The
  document itself is not yet Accepted
- Related: ADR-0037 (Accepted, revision 3) — the **type-level** region model this
  must implement rather than re-decide. ADR-0055 (Accepted) — where authority
  comes from, and its Option B. ADR-0050, ADR-0041 — grants. ADR-0074 (Draft) —
  the build workspace and the launch bundle this exists to carry.
  `CAPABILITY_V1`, `IPC_V1` §5–§6, `SYSTEM_ABI_V1` §5

## 1. ADR-0037 already decided the semantics; this is reconciliation

ADR-0074 §5b listed "regions have no declared rights" and "no access mode" as
gaps to be **decided**. That was wrong, and the correction matters: ADR-0037
revision 3 is Accepted and fixes the facts at the type level. A system Region
contract does not get to choose differently — it has to *implement* this table.

| ADR-0037 §2–§3 | `Copy` | mutable | Shareable | `Transferable` |
|---|---|---|---|---|
| `Region<T>` | no | no | yes | yes |
| `Region<mut T>` | no | yes | **no** | **no** |
| `DmaRegion<T>` | no | no | **no** | **no** |
| `DmaRegion<mut T>` | no | yes | no | no |

And ADR-0037 §3–§4: every region handle is **affine** — one owner;
`Transferable` means ownership may move into **exactly one** task; `share`
**consumes** its argument and is admissible only when the type is transitively
immutable.

**What that determines at the system level.** These are not proposals; they are
the only readings consistent with the accepted type model:

| Type-level fact (ADR-0037) | What the system contract must provide |
|---|---|
| `Region<mut T>` is readable and writable | a right to read and a right to write, distinguishable on the capability |
| `Region<mut T>` is **not** Shareable | `region_share` (operation 7) must **refuse** a writable region handle; the `share` right may not coexist with the write right |
| `Region<mut T>` is **not** Transferable | a writable region handle may not be delegated or sent at all — `IPC_V1` §6's message path must refuse it |
| `Region<T>` is Shareable | the `share` right operation 7 already names, admissible only here |
| `Region<T>` is Transferable into exactly one task | **linear** transfer: the sender's handle is consumed atomically with the receiver's acquisition (`CAPABILITY_V1` §4's linear case) |
| every region handle is affine | at most one holder of the root at any instant; the `Shared<T>` form after `share` is the only copyable one |
| `share` consumes its argument | operation 7 must consume the input handle, which its current row does not say |
| `DmaRegion` is neither shareable nor transferable in V1 | neither right is ever granted on a DMA region |

**Two inconsistencies fall out of the reconciliation**, and they are the real
G1/G2 rather than open choices:

- **G1′.** `CAPABILITY_V1` §3 enumerates rights for endpoints and processes and
  none for regions, while `SYSTEM_ABI_V1` operation 7 already requires a `share`
  right no contract defines. The set is *determined* by the table above —
  read, write, share — and needs writing down, not deciding.
- **G2′.** `IPC_V1` §6 states that **no Stage 3 object type is declared linear**,
  and `IPC_V1` §5 describes a transferred region leaving the sender's address
  space "if the interface declares the transfer linear". ADR-0037 declares
  `Region<T>` Transferable into exactly one task, which *is* the linear case. The
  two documents disagree, and ADR-0037 is the accepted one.

## 2. Where a region may lawfully come from

ADR-0074 §5b said ADR-0055 forbids `region_create` outright. That reading was too
strong: ADR-0055 rejects **ambient** creation and explicitly leaves its Option B
— a bounded, self-only creation operation — as "necessary for a system where
processes make new objects", to be taken as a separate decision. This is that
case. Two lawful origins are compared.

### A — ADR-0055 Option B: bounded self-only region creation

A process may create a region only it can name, and receives the handle. No
authority over anyone else is conferred, so §5's rule holds in substance.

**Resource exhaustion is the whole of the analysis.** ADR-0055 named the cost
itself: it hands every process, including an untrusted one, the ability to make
the nucleus allocate. For it to be admissible the bound must be:

- **a byte budget, not a count.** Regions are pool frames; ten small regions and
  one large one are the same channel;
- **stated by the launcher, in the launch record**, beside the memory grant — the
  same place and the same authority that decides `RuntimeMemoryGrantV1`, so it is
  as explicit and launcher-controlled as every other process resource;
- **accounted and refusable**: a creation that would exceed it answers `E_LIMIT`,
  which the status space already has;
- **summed against the pool.** Per-process budgets that over-commit the pool
  turn a refusal at creation time into a failure somewhere else, so either the
  sum is checked at launch or the pool is the second bound.

What it does not answer: a supervisor cannot pass part of its budget to a
worker. The budget is a number attached to a process, not an authority, so it
does not attenuate, delegate or revoke, and a system that wanted a build worker
to spend the supervisor's allowance would have to invent a way to say so.

### B — a finite memory authority, and Region allocation as an operation on it

**A frame-range capability narrowed by attenuation does not work, and the reason
is what attenuation is.** `CAPABILITY_V1` §4 makes attenuation a *refinement* of
authority: it produces a narrower capability and **does not consume the input**.
Applied to a range of frames that means the derived sub-range

- reserves nothing — no frame is taken out of anyone else's reach;
- does not exclude a second, overlapping sub-range carved from the same root;
- leaves the root holder with undiminished authority over the very frames the
  derived capability names;
- produces no `RegionObject` with an identity and a lifetime of its own, because
  attenuation makes capabilities and not objects;
- and would leave the root holder with authority over the backing **after** a
  derived region has been frozen, which is precisely the writable-alias the
  transition in §3 exists to eliminate.

So the earlier recommendation is withdrawn. Allocation is not refinement, and no
amount of scope-narrowing turns one into the other.

**The model instead: a finite `MemoryAuthority`, and an allocation operation
that consumes from it.** The authority is abstract — it confers the right to
spend a finite memory resource, not to address particular frames. Physical
placement stays the nucleus's business and does not enter the public contract:
contiguity, alignment and where the pages actually are should be an
implementation fact until something is measured that needs otherwise.

An allocation, presented with such an authority, performs one atomic step:

```text
1  check the authority's remaining finite resource
2  reserve unique backing pages
3  exclude them from every other live allocation
4  create a nucleus-owned RegionObject
5  return a mutable, affine Region capability over it
6  charge the spend against that authority
7  on final reclamation, return the resource to the same pool
```

No overcommit, and no two live regions over the same backing — both by
construction rather than by check-and-hope.

**This is not authority ex nihilo.** A region exists only as a consequence of a
capability the caller presented, and is bounded by that capability's finite
resource. It is the same shape as `process_create`: authority over a new object,
derived from authority the caller already held, bounded by it. What ADR-0055
rejects is an operation reachable *without* a capability, and this is not one.

#### One authority, or an attenuable budget

| | One `MemoryAuthority` per holder, with accounting | Hierarchical, attenuable budgets |
|---|---|---|
| how a supervisor funds a worker | it cannot: the worker needs its own authority from the launcher, sized at launch | attenuate its own by amount, delegate the derived one |
| where the sum is checked | at launch, against the pool | at each attenuation, against the parent's remainder |
| what attenuation means | rights and lifetime only; the amount is fixed at endowment | **rights, lifetime and amount**, which needs `CAPABILITY_V1` §4 to admit a quantity as part of scope |
| revocation | by generation, as for any capability | the same, and a parent's revocation must reclaim the child's unspent remainder |
| failure mode | a worker sized wrong at launch cannot be topped up | a parent that over-delegates starves itself, which is visible and local |
| complexity in the nucleus | one counter per authority | a tree of counters, and a rule for what a child's spend does to a parent's remainder |

The build lifecycle needs the second: a supervisor decides how large a bundle a
particular build may produce, and that decision is per build rather than per
boot. But the first is what an accepted contract can express today, since
`CAPABILITY_V1` §3's *scope* is "the range or subset the rights apply to" and a
remaining quantity is neither.

**Decided: hierarchical, with the quantity named as part of the authority's
scope.** The flat alternative — one authority per process, sized at launch —
pushes every build's size decision back to boot, which is the static
partitioning this model was chosen to avoid.

The amendment that makes it expressible, to `CAPABILITY_V1` §3, replacing the
scope line:

> - **scope**: the range, subset **or finite resource amount** the rights apply
>   to, where the object has one. An amount is a scope like any other: it
>   narrows under attenuation and never widens, so a derived authority may spend
>   at most what its parent had, and a parent's remaining amount is reduced by
>   what it delegates.

Two rules follow and are part of this decision rather than left to
implementation: a child's spend is charged to its own authority and to every
ancestor's remainder, so no chain of delegation can spend a total larger than
its root; and revoking an authority by generation reclaims its unspent
remainder to the parent, because an unspendable remainder is memory nobody can
name.

Neither is implemented, and no operation number, register or right is claimed.

## 3. G7 — the mutable-to-immutable transition does not exist

**Searched, and absent.** `Region<mut T>` and `Region<T>` are both accepted
types, and nothing in docs/39–44, the ADR corpus, `CAPABILITY_V1`, `IPC_V1` or
`SYSTEM_ABI_V1` describes a transition from the first to the second. `share`
(ADR-0037 §4) presupposes immutability — "only when T is transitively
immutable" — so it cannot produce it. This is G7, and it is what the whole
build-to-launch lifecycle rests on.

Three shapes, and the requirement they are judged against: **after a successful
transition, the absence of writable aliases must be a fact the nucleus knows,
not a promise the caller makes.**

### A — a dedicated consuming transition operation

One operation, taking a writable region capability and consuming it, returning
an immutable one. The nucleus, in the same step and before it returns:

- invalidates the writable capability by generation, so no handle to it survives;
- walks the region's mappings and removes or downgrades every writable one;
- marks the region object immutable, so every future mapping is read-only and
  every future request for a write right is refused.

Because the operation is the only way to reach the immutable form, and because
the nucleus performs all three at once, "no writable alias exists" is a
postcondition it can assert. It is also the only shape under which a **failure**
is expressible: a region with a writable mapping the nucleus cannot remove is a
refusal rather than a silent half-transition.

### B — attenuation plus consumption or revocation of the writable root

Attenuate the writable capability to a read-only one, then destroy the writable
root.

It does not hold. `CAPABILITY_V1` §4's attenuation **produces** a capability and
says nothing about the input dying; there is no consuming attenuation. Adding
"and the input is released" makes immutability depend on the caller doing the
second half, which is exactly the promise the requirement forbids. Worse, the
property wanted is about *all* writable handles and *all* mappings, and neither
release nor revocation-by-generation touches a handle the owner kept: revocation
invalidates **derived** capabilities, so a root retained by a supervisor is
untouched. B cannot make the guarantee at an instant; at best it makes it
eventually, and only if everyone cooperates.

### C — an existing mechanism

None exists. `share` requires immutability rather than creating it; revocation
is about derived capabilities and is silent on mappings; `region_share`
(operation 7) is about a second reader, not about access mode. Searched and
reported, not assumed.

### Decided: A

**The Architect accepted the direction on 2026-08-29: a dedicated consuming
transition.** The name, the operation number and the register shape are
deliberately not chosen. The semantics are fixed:

```text
input     a region capability with the write right, held by exactly one process
effect    the writable capability is invalidated;
          every writable mapping of that region is removed or downgraded;
          the region object becomes permanently immutable
output    a region capability with read and share rights, affine, Transferable
refusal   a writable mapping that cannot be removed, or a caller that is not the
          sole writable holder
after     no writable alias exists — a fact of the nucleus, checkable by it
```

Stated as the postcondition it has to be: **the nucleus can prove
`writable_aliases == 0`.** Not the caller, and not by convention.

Irreversible by construction: there is no operation in the other direction, and
the immutable form has no write right to attenuate towards.

**Atomic in both outcomes.** A transition that cannot complete leaves the region
exactly as it was — mutable, with its capability and its mappings intact — and
says so. There is no half-frozen state, because a region that was partly
downgraded would be one whose access mode no single fact describes.

## 4. The lifecycle this makes possible

The Architect's preferred model, which §3-A is what makes it expressible:

```text
1  the worker holds a Region<mut u8> of a bounded size, carved by attenuation
   from an endowed range (§2 B)
2  a mutable region has exactly one writable owner — ADR-0037's affinity
3  it cannot be shared, delegated or transferred, so a second writable holder is
   unexpressible rather than forbidden
4  the worker writes the TOSBUNDLE into it
5  a successful build performs the transition of §3
6  the transition consumes the writable authority, is irreversible, removes
   every writable mapping and forbids future ones
7  what results is ADR-0037's Region<T>: readable, affine, Transferable,
   Shareable
8  the worker transfers it — linearly, into exactly one holder — to the
   supervisor
9  the supervisor may keep it, or share it, for restart; the target is given a
   read-only mapping
10 the target verifies the bundle itself, as ADR-0073 requires
11 a process ending removes its mappings and handles
12 when no capability, mapping or reference remains, the backing returns to the
   pool
```

Worker dies before the transition: no immutable region was ever produced, so no
target can be created from it; the writable capability dies with the process,
and the backing is reclaimed because nothing holds it.

**This simplifies ADR-0074 §4.** That draft made a "launch transaction" the
owner of the backing, because nothing else could carry the lifetime. Under this
model the lifetime is carried by the capability itself: ownership is affinity,
the handoff is a linear transfer, and reclamation is the last handle going away.
**No memory-owning transaction object is needed**, and ADR-0074 §4 should be
replaced by this section rather than kept beside it.

## 5. `CAPABILITY_V1` §2: a stale sentence, and the invariant that replaces it

`CAPABILITY_V1` §2 says, citing ADR-0055: "**No operation of `SYSTEM_ABI_V1`
produces a capability.**" That was true of the twelve operations ADR-0055
examined and is no longer true of the ABI as it stands. Reconciled against every
case, present and proposed:

| Operation | Returns a capability? | Where its authority comes from |
|---|---|---|
| `capability_attenuate` (5) | yes, a derived one | the capability presented, narrowed — never widened |
| `process_create` (8) | yes: `rdx` returns the child's handle | the process-authority capability presented, and an endowment every entry of which the parent already holds |
| `process_create_with_generation` (15) | the same, plus an asserted generation | the same, with the generation recorded rather than computed (ADR-0067) |
| a future `MemoryAuthority` → Region (§2 B) | yes, a mutable region capability | the authority presented, bounded by its finite remaining resource |
| ADR-0055 Option B, for non-memory objects | yes, for an object only the caller can name | an explicitly accepted, bounded self-only creation rule |

The line every one of them respects is not "no capability is returned" — it is
that **nothing arrives without a stated origin, and no origin widens**.

Proposed replacement for that sentence:

> **`SYSTEM_ABI_V1` creates no ambient authority.** A capability an operation
> returns must have an explicitly defined normative origin: either authority the
> caller presented to that operation, which bounds what is produced, or an
> explicitly accepted bounded self-only creation rule. No operation creates
> authority over a pre-existing external object out of nothing, and no operation
> widens what its caller held.

**Where it may be applied.** `CAPABILITY_V1` is an Accepted Tier 2 interface
contract, accepted by ADR-0048, and docs/38 places Tier 2 under Tier 1: a Tier 2
document conforms to accepted ADRs and does not silently amend them — and the
reverse holds too, an accepted contract is not edited without a decision that
says so. ADR-0067 is the accepted later decision that made the sentence stale,
but it did not amend `CAPABILITY_V1`, so the correction is not automatic
housekeeping.

Therefore **the amendment rides on this ADR's acceptance**, as exactly the
paragraph above, replacing one sentence and nothing else. **ADR-0055 is not
rewritten**: it recorded a twelve-operation ABI correctly, and its reasoning —
authority has a root, only narrows, and terminates at an endowment on the audit
record — is what the replacement states.

## 6. RegionObject: what the nucleus holds, and when the backing goes back

Design only. Nothing here is implemented.

A `RegionObject` is nucleus-owned state, unreachable from any process except
through a capability:

```text
identity + generation      so a stale handle is detectably stale (CAPABILITY_V1 §2)
backing                    the pages; nothing about where they are is public
access mode                mutable, or permanently immutable after the §3 transition
capability references      how many live handles name it, by mode
mapping references         how many address spaces have it mapped, by mode
charged to                 the MemoryAuthority the allocation spent from (§2 B)
```

**Reclamation, case by case.** One sentence underneath every row: *the backing
returns to the pool that funded it only when no capability, no mapping and no
internal reference can reach it.*

| Event | What happens to the region |
|---|---|
| a process dies | its handles and mappings go with it; the region survives if anything else still reaches it, and is reclaimed if not |
| `capability_release` | one capability reference goes; a mutable region losing its only handle is unreachable and is reclaimed, because a mutable region has exactly one holder by construction |
| a linear transfer that **succeeds** | the reference moves; the count never passes through zero |
| a linear transfer that **fails** | nothing moved — the sender still holds it, as `CAPABILITY_V1` §4 requires |
| the §3 transition **succeeds** | every writable capability and mapping reference is gone by the postcondition; the region continues under the read references that remain |
| the §3 transition **fails** | the region is exactly as it was, mutable, references intact |
| a shared immutable region | each `Shared` holder is a reference; the last one going releases it |
| the target dies | its read mapping and handle go; if a supervisor kept the bundle for restart, the region lives on that reference alone |
| a supervisor holds the bundle for restart | the intended case: the bundle outlives the target that verified it, and is reclaimed when the supervisor releases it |

Two properties this is built to have. **No reference is kept alive by a policy**:
every row is the same rule applied to a different event, so there is no case
somebody has to remember. And **the charge outlives the region's users**: memory
returns to the authority it was spent from rather than to a general pool, or a
long-running supervisor would slowly convert one build's budget into another's.

## 7. What is still missing after this draft

- **G4** — what happens to an existing **mapping** when the capability that
  authorized it is released or revoked. §3 needs it for the transition, and it
  is a rule about mappings rather than about handles; §6 assumes it;
- **G6** — §6's reference rule has to be something the nucleus can decide
  cheaply, and the cost of deciding it is not measured;
- **the quantity-as-scope amendment** §2 asks of `CAPABILITY_V1` §3, without
  which only the flat per-process authority is expressible;
- the **boot endowment** of §2-B: which range, decided where, named in which
  audit record;
- the ABI shape of §3-A, and of ADR-0074 §6, both of which have to wait for the
  rights in §1 to be written into `CAPABILITY_V1`.

