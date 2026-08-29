<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0075: Region object lifecycle, the mutable-to-immutable transition, and reclamation

- Status: **Draft — not accepted. Nothing here is implemented, and no operation
  number, register or right is claimed**
- Date: 2026-08-29
- Decision level: 2 — it would fix what a system Region object is, how one comes
  to exist lawfully, how a writable region becomes an immutable one, and when
  its memory returns to the pool. It changes no TOS Core semantics and no
  accepted ceiling
- Project Architect approval: **not given.** The lifecycle in §4 is the
  Architect's stated preferred model, recorded here for analysis
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

### B — a finite memory authority whose scope is a frame range

A capability whose object is a range of pool frames, endowed at boot from the
launcher's stated constant (ADR-0051 §3) and narrowed downward like any other:

```text
boot endowment: region authority over a pool range
  -> capability_attenuate (operation 5), scope narrowed to a sub-range
  -> the sub-range is the backing a build worker writes its bundle into
```

**The striking part is that this needs no new operation at all.**
`CAPABILITY_V1` §3 already defines *scope* as "the range or subset the rights
apply to", and §4's attenuation narrows rights, scope and lifetime. Carving a
backing out of a range a process already holds is attenuation — the existing
operation 5 — not creation. Nothing is conjured, so there is no
resource-exhaustion channel to bound: a process can only ever carve inside what
it was given, and the sum is bounded by construction rather than by arithmetic
at launch.

Costs, stated plainly:

- the pool must be **partitioned at boot**: some frames become the launch
  authority's range and are not available to `memory::admit_memory`'s general
  pool, which is a static reservation the nucleus does not do today;
- **reclamation matters more** (G6): if a range cannot return to the authority
  when the last handle over it dies, a long-running system leaks its launch
  memory one build at a time;
- ADR-0050's grant path is untouched, but the boot endowment gains a second kind
  of thing in it, which ADR-0051 §3's constant has to name.

### Recommendation

**B**, and A left for objects that are not memory.

B is the option that keeps ADR-0055's rule intact rather than carving an
exception into it: authority still has a root, still only narrows, still
terminates at the boot endowment that is on the audit record. It also removes
the exhaustion question instead of answering it — there is no budget to enforce
because there is nothing to conjure.

A remains necessary later, and ADR-0055 says so: an endpoint is not memory, and a
process that needs one it was not given has no range to carve it out of. That is
a different decision, for a different object, and it is not blocking here.

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

### Recommendation

**A** — a dedicated consuming transition, **name and ABI shape deliberately not
chosen here**. What this ADR would fix is the semantics:

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

Irreversible by construction: there is no operation in the other direction, and
the immutable form has no write right to attenuate towards.

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

## 5. What is still missing after this draft

- **G4** — what happens to an existing **mapping** when the capability that
  authorized it is released or revoked. §3-A needs it for the transition, and it
  is a rule about mappings rather than about handles;
- **G6** — the reclamation rule of §4 step 12: "no capability, mapping or
  reference remains" has to be something the nucleus can decide;
- the **boot endowment** of §2-B: which range, decided where, named in which
  audit record;
- the ABI shape of §3-A, and of ADR-0074 §6, both of which have to wait for the
  rights in §1 to be written into `CAPABILITY_V1`.

## 6. A wording correction this uncovered, for `CAPABILITY_V1` §2

`CAPABILITY_V1` §2 says, citing ADR-0055: "**No operation of `SYSTEM_ABI_V1`
produces a capability.**" That was a true statement about the twelve operations
ADR-0055 examined, and it is no longer true of the ABI as it stands:

- `capability_attenuate` (5) produces a **derived** capability — it always did,
  and ADR-0055's own §"none of the twelve produces a capability" was about
  authority arising, not about handles being returned;
- `process_create` (8) and `process_create_with_generation` (15) return the
  child's capability handle in `rdx`. That is authority over an object that did
  not exist before the call, and it was made explicit by **ADR-0067**, whose
  operation table states "`rdx` (out) | the child's capability handle, as
  operation 8".

Proposed amendment, to `CAPABILITY_V1` §2, replacing that one sentence:

> **No operation of `SYSTEM_ABI_V1` creates authority out of nothing.** An
> operation may return a handle to authority that already has a lawful origin:
> a capability derived by attenuation from one the caller holds, or authority
> over an object the caller's own authority brought into being. Every such
> origin is bounded by what the caller was given, and no operation widens it.

ADR-0055 is Accepted and is **not** rewritten: it recorded the state of a
twelve-operation ABI correctly. What is proposed is that the Tier 2 contract
stop restating a headcount as an invariant, and state the invariant that
actually holds and that ADR-0055's reasoning was always about.
