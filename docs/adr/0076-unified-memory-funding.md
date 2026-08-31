<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0076: One physical account — frames, memory authority, process grants and region backing

- Status: **Proposed**
- Date: 2026-08-31
- Decision level: 2 — it fixes where every dynamically allocated byte of user
  memory is charged, and makes the process grant a funded allocation rather than
  a helping from a free pool. It changes no TOS Core semantics, no ABI operation
  and no accepted ceiling; it does change what `RuntimeMemoryGrantV1` is a
  statement about
- Project Architect approval: **not given**
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
        │     │     ├── grant     consumed at creation: the process's arena
        │     │     └── region    the bundle backing it allocates
        │     └── child          a target's
        └── …
```

Four rules:

1. **Every dynamically allocated user byte is charged to exactly one
   authority.** Process grants and region backing both, and nothing else exists
   in that class.
2. **A process grant is a funded allocation.** It is paid for by a child
   authority attenuated to exactly the grant's size, consumed when the process
   is created, and returned to the funder when the process is reclaimed.
3. **Kernel-only overhead may live outside the tree only if it is bounded and
   reserved before the tree exists.** The process table, the page tables a
   proved maximum of address spaces needs, and the nucleus's own metadata
   qualify; anything whose size depends on what processes later ask for does
   not.
4. **There is no second free-memory counter.** After the root authority is
   endowed, nothing allocates user memory by asking the pool directly.

## 3. What `RuntimeMemoryGrantV1 = 54 MiB` becomes

It stays exactly what ADR-0069 measured and fixed: the grant of an **ordinary
runtime process**, and the number the reference platform's four-process budget
was computed from. What it stops being is *the* grant size: a funded creation
names the size it is paying for, and a build worker names one the measurements
say it needs.

This ADR proposes no new number. It makes the size an argument of a funded
creation and leaves what a build worker is given to whoever funds it — which is
the only way a workspace bound that is still being measured can be honoured
without freezing it prematurely.

## 4. Legacy `process_create` (8) and `process_create_with_generation` (15)

Their signatures do not change, and neither does what a caller of them may
expect. What changes is where the frames come from, and there are only three
possibilities:

- **A — legacy creations are funded from the root authority itself.** The
  nucleus debits the root for `RUNTIME_GRANT` on behalf of a caller that named
  no authority. Simple; the caller sees nothing new; and it keeps one counter.
  Its cost is that the root pays for something nobody chose to fund, so a
  supervisor's carefully attenuated allowance can still be undercut by an
  ordinary `process_create` elsewhere.
- **B — legacy creations are funded from the caller's own authority, when it
  has one.** Closer to the model, but it makes operation 8's behaviour depend on
  an argument it does not take, which is the kind of hidden coupling this
  project has refused elsewhere.
- **C — legacy creations are refused once the tree exists.** Honest and
  disruptive: every existing caller and every QEMU gate that creates a process
  would have to move to the funded form in the same change.

**Recommended: A, with the root's debit visible.** It is the only one that
neither changes an accepted operation's meaning nor breaks the gates, and the
property that matters — no frame is spent twice — holds under it, because the
root is inside the tree rather than beside it. When the funded form exists and
the gates use it, C becomes a cheap follow-up rather than a flag day.

What is **not** acceptable under any of the three: a legacy creation reaching
`frames.available()` directly while an authority believes it holds those frames.
That is the defect this ADR exists to prevent.

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
- **Freezing does not move or unmap a reader.** It removes writable mappings —
  the writer's own included — and leaves readable ones addressing the same
  bytes at the same place. A holder that was writing must therefore expect its
  window to become unreadable-for-writing at exactly the instant it asked for
  that, and to map it again to read.
- **A linear transfer takes the sender's mapping with the handle**, atomically,
  before ownership is anywhere else (ADR-0075 §5a). The sender's window stops
  resolving; the receiver is given its own.
- **The receiver learns its mapping the same way any other capability arrives**
  — through the receive path, with base and length in its argument region — so
  there is no second mechanism for "where is my region".
- **`Shared<Region<T>>`**, when operation 7 is implemented, is the immutable
  form mapped read-only in more than one address space at once. Each holder has
  its own window; none of them can write; the region is reclaimed when the last
  of them goes.

Layout beyond this — which addresses, in what order, with what alignment — stays
an implementation detail.

## 6. What this ADR does not decide

Operation numbers, register layouts and the names of anything. They belong to
the ABI packet that follows this decision, and none of them can be fixed before
§2 and §4 are.
