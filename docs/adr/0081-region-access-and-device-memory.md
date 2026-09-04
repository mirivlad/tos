<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0081: Region access and device-memory observation

- Status: **Accepted (Project Architect-approved)**
- Date: 2026-09-04
- Decision level: **3** — it decides how a TOS Core module reads and writes
  through every kind of granted memory, adds a sealed device-memory kind to the
  language, and fixes the observability rule device registers require. It adds
  `TOS Core 1.2`, additive `tos-ir/v1` operations gated by language version, and
  a `TOSIMAGE` encoding version
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-04 — **granted
  after the implementation existed, not before it.** See §0
- Related: ADR-0028 (the language contract), ADR-0037 (the region type model),
  ADR-0075/ADR-0076 (region objects and the memory account — **not** widened by
  this decision), ADR-0079 (PCI authority), ADR-0080 (effects name interfaces).
  `docs/39` §2, `docs/40` §"Region", `docs/43` §2, `docs/44`,
  `PLATFORM_INTERFACE_V1`

## 0. Chronology, recorded rather than tidied

**This decision was implemented before it was approved, and that was a process
failure.** The record says so here because a decision whose approval date is
stated without its order of events reads as though the approval came first, and
this one did not.

What happened, in order:

1. Stage 4B's implementation reached a genuine architecture STOP: the accepted
   language had no way for source to read through *any* region, and device
   memory additionally needed observability semantics no accepted document
   stated. That STOP was correctly reported.
2. **The implementation then proceeded past it**, producing the device-memory
   surface and its commits, instead of stopping for review as the STOP rule
   requires.
3. The Project Architect reviewed the resulting design afterwards.
4. Approval was granted on **2026-09-04**, for the reconciled design set out in
   this document.
5. Stage 4B was then formally closed on **2026-09-04** for evidence commit
   `ec03210`, and the closure ruling states that it accepts this chronology as
   recorded and the narrowed scope below as written —
   `source/legal/publication-records/ec032105edb16d8559cd2177ca04a337854d12df-stage4b-closure-approval.md`.

Nothing here claims the approval existed earlier, and no Git history was
rewritten to make the order look different. The implementation is accepted; the
mistake in reaching it is part of the record.

**Scope of the approval.** It covers the three sealed memory-access kinds, the
device-memory authority model and its lifecycle, the checked access semantics,
the UC mapping on the current x86_64 reference platform, MMIO-against-MMIO
ordering, and TOS Core 1.2 as the additive version needed to express them — the
sealed device-memory type surface, the fixed-width little-endian accesses Stage
4B actually needs, their verifier-visible observable semantics, and the version
gating and diagnostics that go with them.

**It approves none of these, and must not later be cited as though it did**:
DMA ordering, DMA address semantics, IRQ semantics, device reset, Virtqueue
semantics, volatile ordinary RAM, arbitrary physical mappings, pointer
arithmetic, unsafe pointer access, or generic native-memory FFI.

## 1. Three kinds, one set of invariants

There are three sealed memory-access kinds, and they are **not**
interchangeable:

```text
ordinary Region        pool memory, funded by a MemoryAuthority
DMA Region             device-visible memory, CPU-accessible
device / MMIO Region   pre-existing hardware registers
```

Every one of them is opaque, non-forgeable, bounded, exposes no source-visible
pointer, admits no address arithmetic and no integer conversion, never lets a
caller select a physical address, checks every access against an exact extent,
lives no longer than the authority that produced it, and has verifier-visible
access semantics.

**None of them masquerades as another.** They all end in mapped pages, and that
is the one thing that must not be allowed to collapse them: ordinary RAM, DMA
memory and MMIO differ in who funds them, what reclaims them, and — decisively —
in whether an access is an *observation*.

## 2. Region and `DmaRegion` access is existing V1 semantics, implemented

Not a new API. The accepted corpus already describes indexed access and has
since V1: `docs/44`'s `E1211_INDEX_TYPE_MISMATCH` covers "an array, slice or
**region** index"; ADR-0037 §7 requires a positive vector "writing through a
`Region<mut T>`"; `docs/43` §2 gives the region/DMA family "typed grant, rights,
checked range/alignment, no physical-address exposure". What was missing was the
implementation, not the decision.

So the source form is the one already intended:

```tos
let x = region[index];
region[index] = value;
```

| Type | `region[i]` | `region[i] = v` |
|---|---|---|
| `Region<T>` | checked read | **refused** — `E1201_ASSIGN_TO_IMMUTABLE` |
| `Region<mut T>` | checked read | checked write |
| `DmaRegion<T>` | checked CPU read | **refused** |
| `DmaRegion<mut T>` | checked CPU read | checked CPU write |

The index is exact `size`, as `docs/40` §3 already states, with an integer
literal contextually typed as one. Every access checks `index < element_count`,
and every byte-position calculation is checked arithmetic that **fails closed**.

**A region does not become an array.** Its grant, rights, affinity, transfer and
lifetime rules are untouched; ADR-0037's ownership model is not weakened. What
changes is only that the access the model always described can now be written.

**The physical backing address remains unobservable.** Nothing in this form
yields it, and no conversion produces it.

## 3. Which element types may be accessed

**Only element types whose in-memory representation the language contract
already fixes.** For this contract that is the fixed-width scalar integers —
`i8`/`i16`/`i32`/`i64`, `u8`/`u16`/`u32`/`u64` — and `bool`, whose single-byte
representation `docs/40` already fixes.

Deliberately excluded: nominal records, tuples, arrays of aggregates, and every
opaque type. **A user record's field order is not a device ABI**, and permitting
`region[i]` over one would silently make the frontend's layout choices a
published binary format. If aggregate layout is ever wanted in shared or device
memory it needs its own contract, and it is not needed for Stage 4.

## 4. The `slice` gap, recorded rather than papered over

`docs/40` also names a checked `slice` contract, and no source spelling exists
for it. **This decision does not add one.** Stage 4 needs indexed access and
nothing else, and a bounded borrowed slice would force a borrow/lifetime
question — how long the borrow lives relative to the region's own affinity —
that nothing currently pending requires answering.

So the defect is stated instead of hidden: `docs/40` names a `slice` access
contract that no accepted grammar provides, exactly as it named `read` and
`write` before this decision implemented them. Indexed access is not blocked on
it. If a slice operation is added later it should be an explicit
`region_slice(region, start, length)` returning a non-escaping borrowed
`slice<T>`, never pointer or range syntax, and **never over MMIO**.

## 5. Device memory is its own kind

A mapped device range is **not** an ADR-0075 `RegionObject`:

- nothing is charged to a `MemoryAuthority` — ADR-0076's one physical account is
  about pool frames, and device registers are not pool frames;
- nothing returns to the physical allocator when it is released;
- its backing is pre-existing external hardware state, named by a physical
  address the pool model deliberately keeps private for its own frames.

The nucleus gains a distinct device-memory object. The language gains a distinct
opaque type in two forms:

```text
MmioRegion       readable
MmioRegionMut    readable and writable
```

They are not `Region` aliases, not `DmaRegion` aliases, not capabilities encoded
as integers, and they expose no address. The two forms stay distinguishable
because a read-only grant must be read-only in the type *and* in the page table
(§10).

## 6. TOS Core 1.2

Indexed region access implements existing semantics and needs no new version.
**MMIO does not exist in the accepted language at all**, so it is an additive
feature and takes a version:

- **1.0** and **1.1** behave exactly as before, and cannot use the MMIO types or
  operations;
- **1.2** admits them;
- a module receives the language its own header declares, so a 1.0 or 1.1 module
  naming an MMIO type or operation is `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`;
- the declared minor stays bound into module identity, artifact identity, source
  maps, cache identity and provenance, as ADR-0080 §5 already requires.

**`tos-ir` stays at `v1`.** The IR schema is versioned, and the new operations
are additive and gated by the artifact's own `language_version` — a verifier
refuses an MMIO operation in an artifact declaring 1.0 or 1.1. Nothing existing
changes meaning, so the semantic major does not move. **The `TOSIMAGE` encoding
version does move**, because the bytes of an instruction stream that can now
carry a new operation are different bytes, and ADR-0070's fail-closed
unknown-version rule is what makes that safe.

## 7. The MMIO access surface

**MMIO does not look like indexing**, deliberately. A device register has a
width and a byte order, and both are part of the transaction rather than of the
value's type:

```text
mmio_read_u8      (region, offset)
mmio_read_le_u16  (region, offset)
mmio_read_le_u32  (region, offset)
mmio_read_le_u64  (region, offset)

mmio_write_u8     (region, offset, value)
mmio_write_le_u16 (region, offset, value)
mmio_write_le_u32 (region, offset, value)
mmio_write_le_u64 (region, offset, value)
```

The offset is a byte offset of exact type `size`. There is no physical address,
no pointer, no generic cast and no device-memory slice. Reads work through
either form; **writes require `MmioRegionMut`**.

## 8. An MMIO access is an observable operation, and the IR says so

It lowers to its own verifier-visible operations — `MmioRead` and `MmioWrite` —
carrying the region operand, the byte offset, the access width, the byte order
and a source-map entry. It is **not** an ordinary `Call`, not an `Op::Read`, not
an array access whose volatility a reader would have to infer from where the
value came from.

The verifier independently proves the operand is exactly an MMIO region kind,
that a write names the mutable form, and that the enclosing artifact declares a
language version in which the operation exists.

## 9. Observability — the load-bearing new memory rule

**Every source-level MMIO read or write is exactly one hardware access of the
declared width.** An implementation may not eliminate it, coalesce it with
another, duplicate it speculatively, reuse an earlier read's result, invent a
read, widen or narrow the transaction, or reorder two MMIO operations against
each other contrary to source evaluation order.

Two reads in source are two device observations. Two writes are two device
writes.

This is a property of the semantic contract — the IR operation, the verifier and
the backend — and not a hope about how a compiler treats a load. It is why §8
gives MMIO its own operations rather than reusing `Op::Read`: an ordinary read
is free to be optimised, and a rule that depended on nobody doing so would not
be a rule.

## 10. Mapping attributes on x86_64, which are a *different* contract

Cache attributes control what the processor and its caches do. Observability
(§9) controls what the compiler and runtime may do. **Both must hold, and
neither implies the other** — "PCD/PWT means volatile" is precisely the
confusion this paragraph exists to refuse.

For the accepted Stage 4 x86_64/QEMU profile a device mapping is user-visible,
non-executable, read-only or read/write according to the granted form, and
uncacheable. Uncacheable is expressed with the page-table bits the nucleus
already has: **`PCD` set and `PWT` set**, which with the reset-state PAT selects
`UC` — strong uncacheable, no caching, no write combining, no speculative reads,
and accesses reaching the device in program order.

A read-only grant is read-only **in the page table as well as in the type**. The
checker refusing a write is not the enforcement; the absent `WRITABLE` bit is.
There are no write-only mappings: x86 paging cannot enforce one honestly.

## 11. Ordering, bounded honestly

**Stage 4B's device ordering contract orders MMIO observations with each other.
It does not yet assert DMA visibility before a notify, or DMA completion
visibility after an interrupt.**

That sentence is the whole of what is decided. A cross-domain rule between
ordinary CPU memory, DMA memory and device MMIO — release/acquire, device
fencing, DMA synchronisation — is what a queue path needs, and deciding it under
pressure from a read-only configuration probe would be deciding the portable DMA
memory model by accident. It belongs to the DMA/queue slice.

## 12. Bounds and alignment

Every MMIO access is byte-offset based. For width `N`:

```text
offset % N == 0
offset + N <= mapping_length          (checked arithmetic)
```

An invalid alignment, width or extent **fails before the device is touched**.
Nothing wraps, and no failure leaves a partial device access behind.

## 13. Deriving a mapping from a PCI function

The caller names a BAR index, a page-aligned offset within that BAR, a
page-aligned length and the form it wants. **It never supplies a physical
address**: the nucleus takes the BAR base and extent from the live assignment
the presented `platform.pci.FunctionConfig` names.

**The scope is page-granular and explicit.** A sub-page grant that quietly maps
a whole page would be a contract narrower than the authority it hands out, so
the public scope *is* the page or pages mapped.

### BAR sizing

Stage 4A's exclusive assignment is the serialisation boundary, and sizing is
mechanism only ring 0 can safely perform. It happens **once per assignment and
BAR**, at claim time, and the result is cached in the assignment. The original
BAR value is restored exactly; 64-bit BAR pairs are handled as pairs; I/O BARs
are refused in this stage; absent or zero BARs never become authority; every
computation is checked; and nothing else can be sizing the same function,
because nothing else holds the assignment.

**This teaches the nucleus PCI BAR mechanics and nothing about VirtIO.**

## 14. Lifetime: an assignment's descendants

A mapped MMIO object is a descendant of one **PCI function assignment**, not of
one process-local handle.

```text
PciFunction assignment
    ↓
0..N derived MMIO objects
```

The assignment stays live while *either* any `FunctionConfig` capability names
it **or** any derived hardware object exists. So releasing the last function
handle does not let the same BDF be re-assigned while a mapping is still live,
and a manager releasing its own handle does not destroy a driver's mapping.

Only when every function handle **and** every descendant is gone does the
assignment end and its generation advance. That is what makes this impossible:

```text
release the function → re-claim the same BDF → the old mapping reaches the new assignment
```

**Process death** unmaps that process's device pages, releases its MMIO objects,
contributes to draining, and cannot leave an untracked mapping. The assignment's
state stays auditable afterwards.

The descendant model is written generically because IRQ and DMA objects will
need the same invariant — but **those objects are not designed here.**

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. I-07 is
  strengthened: memory allocation authority and device mapping authority become
  separately named things, and neither yields the other.
- **Canonical representation:** unchanged for 1.0 and 1.1 modules.
- **Trusted-base impact:** the nucleus gains BAR mechanics, a device-memory
  object and its lifecycle. It gains no VirtIO knowledge, which a gate enforces.
- **Source-to-runtime impact:** `TOSIMAGE` encoding version moves; `tos-ir/v1`
  is additive and version-gated.
- **Threat-model impact:** a new mapping path into a process, bounded by §12 and
  §13 and revoked by §14. AGENTS §10 coverage is the negative set of the Stage 4B
  evidence.
- **Compatibility profile:** TOS Core **1.0, 1.1 and 1.2**.

## 15. Conformance evidence

Region: checked read through `Region<i32>`; read and write through
`Region<mut i32>`; a write through an immutable region refused; the `DmaRegion`
equivalents; a non-`size` index refused; an out-of-range access refused at run
time; no physical address obtainable; and forged IR with an incorrect region
access refused by the verifier independently.

MMIO: the values §17 of the Stage 4B round requires, read from the real device;
every negative of its §19; a 1.0 or 1.1 module refused the feature; an artifact
declaring 1.0 or 1.1 and carrying an MMIO operation refused by the verifier; and
an ordinary region operand in an MMIO operation refused.
