<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0076: One physical account — frames, memory authority, process grants and region backing

- Status: **Accepted**
- Date: 2026-08-31
- Decision level: 2 — it fixes where every dynamically allocated byte of user
  memory is charged, and makes the process grant a funded allocation rather than
  a helping from a free pool. It changes no TOS Core semantics, no ABI operation
  and no accepted ceiling; it does change what `RuntimeMemoryGrantV1` is a
  statement about
- Project Architect approval: **given, 2026-08-31**, conditional on the
  corrections this revision makes: the funded-grant semantics of §3, the
  retirement path for operations 8 and 15 in §4, the granularity of §7, and the
  mapping and sharing rules of §5. §8 carries a physical finding the Architect
  asked to be brought back rather than resolved here
- Related: ADR-0075 (Accepted) — the authority tree and the region lifecycle this
  funds. ADR-0069 — the reference process grant. ADR-0050, ADR-0041 — grants and
  the pool. ADR-0074 (Draft) — the build workspace this has to be able to pay
  for. `docs/evidence/STAGE3_BUILD_WORKSPACE.md`

## 1. Two counters over one memory

`nucleus/src/process.rs` funds a process grant straight from the pool:
`grant_bytes` asks `frames.available()` whether `RUNTIME_GRANT` fits and carves
it if it does. ADR-0075 §2b puts a root `MemoryAuthority` over "the finite
memory pool once the nucleus has taken its fixed reserves".

Those are the same frames. Built as they stand, an authority would report
`remaining = N` for frames a later `process_create` had already taken, and a
grant would succeed against frames an authority had already promised. **Two
independent counters over one physical resource is not an accounting model**,
and the failure it produces is the worst kind: an allocation that was refused on
paper and granted in fact, or the reverse.

There is a second fact in the way. The measured worst-case build workspace is
`90.77 MiB` (`STAGE3_BUILD_WORKSPACE.md`), and `RuntimeMemoryGrantV1` is
`54 MiB`. ADR-0074 §1 makes the workspace the worker's grant. **A build worker
cannot run under a fixed 54 MiB grant**, so "every process gets the same grant"
has already stopped being true of the system being built.

## 2. Decision

**One pool, one tree, and everything dynamic hangs off it.**

```text
Frames                            every usable frame the boot admitted
  └── fixed nucleus reserves      process table, page tables and metadata that
                                  are bounded and proved before any process
                                  exists — physically excluded from the tree
  └── root MemoryAuthority        everything else, endowed at boot, named in
        │                         the launch and audit record
        ├── child authority       a supervisor's allowance
        │     ├── child          a build worker's, attenuated to its grant
        │     │     ├── grant     charged at creation: the process's arena
        │     │     └── region    the bundle backing it allocates
        │     └── child          a target's
        └── …
```

Four rules:

1. **Every dynamically allocated user byte is charged to exactly one
   authority.** Process grants and region backing both, and nothing else exists
   in that class.
2. **A process grant is a funded allocation** (§3). It is charged to a
   `MemoryAuthority` the creator names, held against that authority's accounting
   node for as long as the process lives, and returned up the same funding
   lineage when the process is reclaimed.
3. **Kernel-only overhead may live outside the tree only if it is bounded and
   reserved before the tree exists.** The process table, the page tables a
   proved maximum of address spaces needs, and the nucleus's own metadata
   qualify; anything whose size depends on what processes later ask for does
   not.
4. **There is no second free-memory counter.** After the root authority is
   endowed, nothing allocates user memory by asking the pool directly.

## 2a. What is actually outside the tree (added 2026-09-01)

Rule 3 above is a test, and applying it to the code rather than to the earlier
estimate moved most of what had been called "fixed reserve" inside the tree.

**Outside, pre-reserved: page tables, and nothing else.** They are the one thing
that is both bounded before any process exists and unreachable by any process —
a table is the nucleus's own structure, and no mapping names one. They are also
the one thing that was still being taken from the pool *after* the point the
root authority would be endowed, one frame at a time, at every `map_page`. So
the frames are carved out of the pool before the endowment, into a reserve the
pager is given **instead of** the pool: `paging` no longer takes `&mut Frames`
at all, which makes "a page table cannot be built out of promised memory" a
property of the signatures rather than a rule to remember.

The bound is derived, not measured. It mirrors what the pager actually does —
2 MiB leaves for the bulk of described memory, which need no page table, and
4 KiB pages only for the chunks holding the nucleus image or the framebuffer —
and adds each region a process is given at the largest the accepted limits let
it be. Bounding it by the highest described address instead would have sized the
reserve by where the firmware put the framebuffer: a page directory per gigabyte
of a terabyte-wide map, for a machine with a few hundred megabytes in it.

**Inside the tree: everything a process is given.** The grant, the stack, the
report region, the argument region, the launch record and the image's writable
data. `MAX_PROCESSES = 4` does not make those a fixed reserve: they are created
per request, at sizes the request influences, and a bound that exists only
because a table has four slots is a bound on how many can be outstanding, not a
statement that the memory was set aside. They are charged.

This replaces the `1.78 MiB` and `7.90 MiB` lines of §8, which were an estimate
of a different classification.

## 2b. A `MemoryAuthority` is a reservation, not a limit (added 2026-09-01)

"Finite budget" in §3 is right and reads two ways, and only one of them is what
this ADR decided. Three things have to stay apart:

| | what it is | when the frames move |
|---|---|---|
| **committed** | physical frames a process or region actually holds | now |
| **reservation** — a `MemoryAuthority` | a guaranteed right to obtain a finite number of frames later | the right moves now, the frames later |
| **a memory limit** (`memory.max`) | a ceiling a process may not pass, with nothing set aside behind it | never guaranteed at all |

`attenuate(parent, N)` is the middle one. `N` stops being available to the
parent and its other children **the moment the child exists**, whether or not a
single frame has been allocated; the child may spend it whenever it likes; the
unspent remainder stays guaranteed to the child for as long as the authority
lives; and release, revoke or the death of whoever held it returns that
remainder up the funding lineage.

So this is the intended behaviour, not a leak:

> A supervisor holding `200 MiB` attenuates `100 MiB` to process A. A uses
> `18 MiB`. `82 MiB` are physically free and B cannot have them.

An operator who wanted "A may use up to `100 MiB` **if the memory exists**" was
asking for a limit, which is a different mechanism with a different failure
mode: a limit can be handed out several times over and discovers it was lying
when somebody tries to spend. **This ADR does not decide limits and Stage 3 does
not implement them.** No object, right or operation for `memory.max` is
introduced here; if overcommitted quota is ever wanted it is a policy layer
above real funding, never a second reading of this one.

Two consequences worth stating, because they are where the distinction bites:

- **A funded creation charges the footprint it actually needs now, and reserves
  nothing on the side.** Whatever memory a process may obtain later belongs to
  it only through a `MemoryAuthority` its creator explicitly endowed. There is no
  implicit allowance because a process was funded — §3's *nothing is inherited*
  is exactly this rule, and it is what keeps a creation from silently taking a
  reservation out of its funder.
- **A build worker's allowance is a policy exception, not a mechanical one.**
  When the reference contract requires a worker to be *guaranteed* to survive an
  admissible build, the supervisor attenuates `funding + bundle reservation` and
  endows the remainder explicitly (§3). The bytes the creation spends become
  committed; the remainder stays reserved and later pays for the bundle region;
  if it is never needed it goes back on release, revoke or death. That is one
  deliberate reservation, not a general quota.

`RuntimeMemoryGrantV1` is unchanged by this. The runtime grant is physically
backed at creation because the current runtime contract says a process is handed
its arena, and introducing demand paging to make reserved and committed differ
more prettily would be a change to that contract rather than a clarification of
this one.

## 3. A funded grant is an allocation, not a consumed authority

An earlier revision said the worker's authority is attenuated to exactly the
grant and consumed at creation, and in the same breath had that authority pay
for the bundle region. Those cannot both be true. The rule:

- a `MemoryAuthority` is a **finite budget**, not a one-shot ticket;
- `process_create_funded(…, memory_authority, grant_bytes)` makes an ordinary
  funded allocation out of it: `charged` bytes leave the authority's remainder
  and are held against its accounting node;
- **the capability is not consumed.** The caller keeps it and may spend the
  rest;
- when the process is reclaimed, `charged` returns to the same funding lineage;
- a caller that *wants* one-shot funding attenuates an exact-sized authority
  first. The ABI does not require it.

For a build worker the shape is therefore:

```text
supervisor allowance
  └── attenuate: worker allowance = grant + bundle budget
        ├── process_create_funded(…, worker allowance, grant = W)   charges W
        └── the remainder is endowed to the worker explicitly, in its
            endowment and its launch record, and pays for the bundle region
```

**Nothing is inherited.** A child does not receive a `MemoryAuthority` because
its parent had one: if the worker is to hold the remainder, that is an entry in
the endowment like any other authority it holds.

## 3a. What `RuntimeMemoryGrantV1 = 54 MiB` becomes

It stays exactly what ADR-0069 measured and fixed: the grant of an **ordinary
runtime process**, and the number the reference platform's four-process budget
was computed from. What it stops being is *the* grant size: a funded creation
names the size it is paying for, and a build worker names one the measurements
say it needs.

This ADR proposes no new number. It makes the size an argument of a funded
creation and leaves what a build worker is given to whoever funds it — which is
the only way a workspace bound that is still being measured can be honoured
without freezing it prematurely.

## 4. Operations 8 and 15 are retired, not quietly funded

An operation that spends the root authority without having been given a
`MemoryAuthority` is exactly the hidden second counter this ADR exists to
remove, so the earlier recommendation — fund legacy creations from the root — is
withdrawn. **Operations 8 and 15 are retired.**

The migration has no red tree in it:

1. the funded operations are implemented, and every caller and QEMU gate that
   creates a process moves to them;
2. in the **same** slice that makes the root authority the only source of
   dynamic user memory, 8 and 15 begin answering `E_NOT_SUPPORTED`;
3. their numbers are never reused.

There is deliberately no committed state in which the root authority is
authoritative and a legacy creation still debits it behind the model's back.
`E_NOT_SUPPORTED` is what `SYSTEM_ABI_V1` §7 already says an operation this
version does not offer answers, so a caller built against the older ABI
discovers the absence rather than misbehaving.

## 5. The mapping contract this needs before any ABI

A region object with no mapping is enough to prove the lifecycle and not enough
to write a bundle into. The minimum observable semantics, so that an operation
table can be proposed against it:

- **The nucleus chooses the virtual address.** A caller never supplies one, and
  never supplies a pointer: `SYSTEM_ABI_V1` §3 already says arguments are values
  and handles, and that a buffer is named by a handle rather than by an address
  the nucleus would have to trust. Regions follow the pattern the grant, the
  stack, the report and the argument region already follow — a window the
  nucleus places.
- **A caller learns base and length from the operation that established the
  mapping**, in its result and its argument region, the way `process_create`
  already returns a handle and writes a record. Nothing is discovered by
  probing.
- **Freezing downgrades in place.** `region_allocate` establishes a writable,
  non-executable window the nucleus placed; `region_freeze` turns that same
  window read-only at the same address, invalidating what the hardware caches
  about it. There is no unmap and no remap, and therefore no `region_map`
  operation in V1: a holder that was writing keeps reading, at the address it
  already has.
- **A linear transfer takes the sender's mapping with the handle**, atomically,
  before ownership is anywhere else (ADR-0075 §5a). The sender's window stops
  resolving; the receiver is given its own.
- **The receiver learns its mapping the same way any other capability arrives**
  — through the receive path, with base and length in its argument region — so
  there is no second mechanism for "where is my region".
- **`Shared<Region<T>>` is on the path, not after it.** A one-shot target could
  be served by the linear transfer alone; a supervisor that keeps a bundle for
  restart cannot. So `region_share` (operation 7) is implemented **before** the
  final `process_create_from_bundle`: it consumes the affine immutable
  capability, hands back a shared one, leaves the caller's read-only window
  exactly where it is, and the shared form is copyable and delegable under the
  accepted model. The backing lives while any shared capability, mapping or
  internal reference remains.

Layout beyond this — which addresses, in what order, with what alignment — stays
an implementation detail.

## 7. Allocation granularity, and what is actually charged

Public sizes stay in bytes. What the accounting moves is what the machine
actually spends:

```text
charged = round_up(requested_bytes, allocation_granule)
```

`charged` is what leaves the authority's remainder, what the accounting node
holds, and what returns at reclamation. The operation and the launch record
report the **mapped and charged** length, not the requested one, so nothing
downstream believes it has less mapped than it has.

Two refusals follow, and both are already in the status space: an overflow in
the rounding is `E_BAD_ARGUMENT`, and a budget that no longer covers the rounded
figure is `E_LIMIT`. Charging the requested bytes while spending a whole frame
would be a hidden overcommit, which is the same defect as a second counter and
is refused for the same reason.

## 8. The physical account, and one finding to bring back

With the `BUILD_WORKER_GRANT = 100 663 296 B` (96 MiB) the Architect set — the
smallest whole MiB above the enforced hard minimum of `95 518 720 B` — the
reference platform's account is:

| Line | |
|---|---:|
| pool after the nucleus | 58 839 frames, **229.84 MiB** |
| fixed reserve: 4 address spaces of page tables at their largest mapping | 1.78 MiB |
| fixed reserve: stack, report, argument region and launch record, 4 processes | 7.90 MiB |
| **root MemoryAuthority** | **220.16 MiB** |

Against that, a build with a supervisor resident:

| Scenario | supervisor + worker + bundle | |
|---|---:|---|
| docs/44 ceiling, statement-heavy | 54 + 96 + 122.90 = **272.90 MiB** | does not fit |
| docs/44 ceiling, mixed body | 54 + 96 + 87.90 = **237.90 MiB** | does not fit |
| capsule-sized, 127 × 256 KiB | 54 + 96 + 46 = **196.00 MiB** | fits |

**The shortfall is simultaneity, not the grant.** Without a supervisor's own
`54 MiB` resident, the worst ceiling case is `96 + 122.90 = 218.90 MiB` and fits
inside `220.16` — by `1.26 MiB`. The three terms that must coexist are the
worker's workspace, the bundle it is writing, and whatever else is resident
while it does.

Nothing is resolved here: the grant is not reduced below its proved minimum, the
reference platform is not changed, and no ceiling moves. What this section does
is report the numbers the Architect asked for before the account is called
closed.

## 9. What this ADR does not decide

The `spend` right's exact name in `CAPABILITY_V1`, and the register layouts of
operations 16–20. The operation numbers are settled — 16 `capability_attenuate_scoped`,
17 `region_allocate`, 18 `region_freeze`, 19 `process_create_funded`,
20 `process_create_from_bundle`, with 7 `region_share` already assigned — and
they are written into `SYSTEM_ABI_V1` when the operations exist, not before.

Two shapes this ADR does fix, because they follow from §3 and §5 rather than
from a register budget:

- **`capability_attenuate_scoped` over a `MemoryAuthority` makes a child
  accounting node** and reserves its scope out of the parent atomically. It is
  not a second handle onto one counter: the parent's remainder falls by exactly
  what the child may spend, which is what makes ADR-0075 §2a's invariant hold.
- **`process_create_from_bundle` takes a shared bundle capability and consumes
  nothing.** It funds the target's grant from the `MemoryAuthority` it is given,
  derives the target's *own* shared capability from the caller's, maps the
  region read-only in the target from that derived capability, and writes the
  handle, base and length into the target's launch record. The supervisor keeps
  its own. A target's death takes its capability and its mapping; the
  supervisor's survives, so a restart repeats the operation without rebuilding
  anything, and the backing is reclaimed when the last shared capability,
  mapping and internal reference are gone.
- It takes **no entry path**. `TOSBUNDLE/v1` already carries the exact closure,
  the entry position and the entry path; a second answer to "what is the entry"
  would be a second truth that can diverge from the first. The target runs the
  entry the bundle declares and verifies the bundle itself (ADR-0073).
