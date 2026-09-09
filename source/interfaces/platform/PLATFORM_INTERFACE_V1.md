<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Platform Interface Schema — Version 4

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0079 (Project Architect-approved, 2026-09-03), which fixes the
authority model this schema declares operations over, amended to version 2
by ADR-0082 (Project Architect-approved, 2026-09-05), which decides the
mechanism the third interface below is declared for, to version 3 by
ADR-0085 (Project Architect-approved, 2026-09-08), which separates an
interface's identity from the class of values that represents it, and to
version 4, which declares the operation that makes a DMA region under
ADR-0084's authority model and ADR-0085 §18's approved concrete result
(Project Architect-approved, 2026-09-09).

**What version 4 adds, and nothing else.** One operation on
`platform.pci.FunctionConfig` — `dma_region_allocate` — which is
`SYSTEM_ABI_V1` operation 30, the one version 3 named as decided and not yet
declarable. Nothing else moves: no interface is added, no representation
changes, and every operation of versions 1 to 3 keeps its name, arity,
parameter types, result type and effect.

**What version 3 added.** One interface —
`platform.dma.Region` — with the two operations a holder of one can perform:
the bounded-offset translation ADR-0084 §6b puts in the nucleus, and the
generic release. It is the first and only interface of any accepted schema whose
capability representation is not `AsInterface` (`SYSTEM_INTERFACE_V1` §4.3).
Versions 1 and 2 are unchanged in every operation's name, arity, parameter type,
result type and effect, and every interface they declared keeps the default
representation.

**What version 2 added.** One interface — `platform.irq.Source`
— one operation on `platform.pci.FunctionConfig` that produces it, and the two
rights those need. Version 1's operations are unchanged in name, arity,
parameter type, result type and effect; what version 1 already narrowed about
operations 26 and 27 is restated here rather than re-decided.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, and to
the language half of the model fixed by
`docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md` §2 under ADR-0028.

## 1. Role

`SYSTEM_INTERFACE_V1` §2 said this document would exist before it did:

> A Stage 4 driver interface is another instance of these rules, not a special
> case of this document.

This is that instance. Every rule of `SYSTEM_INTERFACE_V1` §3–§9 applies here
unchanged — how a module reaches an operation, what a parameter may be, what a
result means, determinism, blocking, provenance — and this document repeats none
of them. What it adds is a second set of **interfaces**, over platform objects
rather than system ones.

It is not an FFI and admits none of the things `SYSTEM_INTERFACE_V1` §1 refuses.
Its target ABI is `SYSTEM_ABI_V1`, operations **24–31**, and nothing else.
Version 1 of this schema said 24–26, which was true of it: 27 arrived with
ADR-0081 §13's device-memory mapping, 28–29 with ADR-0082's interrupt authority,
31 with ADR-0085's DMA region interface and 30 with version 4's allocating
operation — each when its mechanism, and then its declared surface, was
decided.

## 2. What this version declares, and why so little

Four interfaces. `docs/11_DRIVER_MODEL.md` illustrates several —
`platform.mmio.RegionMap`, `platform.irq.Binding`, `platform.dma.Allocator` and
a class publisher — and **none of those is declared here**, including the
interrupt one: what version 2 declared is `platform.irq.Source`, whose mechanism
ADR-0082 decided, and not the binding that document sketched. ADR-0079 §11 left
MMIO, interrupts and DMA open, and all three are now decided — DMA as
`platform.dma.Region` (§4.4) with the allocating operation on the assignment it
descends from, rather than as the `platform.dma.Allocator` that document showed.
`SYSTEM_INTERFACE_V1` §4's rule applies to this schema as much as to that one:

> Nothing speculative: an interface that declared an operation the system does
> not perform would be a contract describing a system that does not exist.

An interface arrives here when its mechanism is decided, not when a document
first shows its name.

| Interface | Object kind | Capability representation |
|---|---|---|
| `platform.pci.Bus` | pci bus | `AsInterface` |
| `platform.pci.FunctionConfig` | pci function | `AsInterface` |
| `platform.irq.Source` | irq source | `AsInterface` |
| `platform.dma.Region` | dma region | `DmaRegionFamily` |

The third column is `SYSTEM_INTERFACE_V1` §4.3, which applies to this schema as
every other rule of that document does. **`platform.dma.Region` is the one
non-default row in the whole accepted corpus**, and §4.4 below is why.

## 3. Where a capability of these comes from

**Not from any operation of this schema, and that is the point.** A
`platform.pci.Bus` capability is a **platform root** in the sense
`CAPABILITY_V1` §2 admits: minted at the boot/platform boundary under this
contract, with explicit scope and identity, on the launch and audit record. No
operation creates one, here or anywhere.

What a module holds is therefore always something somebody decided to give it:

```text
boot/platform
    ↓  minted once, scope and identity in the launch record
root platform.pci.Bus
    ↓  retained by the canonical textual boot supervisor
    ↓  delegated under /system/policy/ by launch_plan_endow
platform.pci.Bus, at the rights the supervisor chose
    ↓  held by the canonical textual PCI bus service
    ↓  pci_function_claim
platform.pci.FunctionConfig, one per assigned function
    ↓  delegated to a driver
```

**There is no rule in this contract, and none in the nucleus, naming which
module may hold a bus capability.** That would be service policy in a place
that has no business knowing a module name (ADR-0048 §2, ADR-0079 §5). What
constrains the flow is the flow itself: explicit delegation, textual launch
policy, source identity and the audit record.

The root survives a PCI service that crashes, because the supervisor holds it
and the service holds a delegated name for it. Restarting the service is the
ordinary Stage 3 lifecycle and re-delegation, not a re-mint.

## 4. The interfaces this version declares

Four, and each declares which kind of object a capability of it names, exactly
as `SYSTEM_INTERFACE_V1` §4 does — so a launcher answering a module's request can
refuse a grant of the wrong kind at startup rather than letting the module
discover it at its first call. Each also declares its capability representation
(`SYSTEM_INTERFACE_V1` §4.3), and for three of the four that is the default.

### `platform.pci.Bus`

A capability naming one PCI bus scope: a segment, and the range of bus numbers
within it that the holder may address.

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `pci_function_claim` | `platform.pci.Bus` with `claim` | `bus: u64`, `device: u64`, `function: u64` | `Result<platform.pci.FunctionConfig, i64>` | 24 |
| `endow_for_launch` | `platform.pci.Bus` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |
| `capability_attenuate` | `platform.pci.Bus` with `none` | `rights: u64` | `Result<platform.pci.Bus, i64>` | 5 |
| `capability_release` | `platform.pci.Bus` with `none` | *(none)* | `i64` | 6 |

**The segment is the capability's, not a parameter.** A holder names a bus, a
device and a function within the scope it was granted; it cannot name a segment,
because the segment is part of what it was granted rather than part of what it
asks for.

**`claim` is one right and it is the whole of what a bus capability is for.**
Possession of a bus capability *is* the authority to address functions within
its scope, which is why operation 24 takes a BDF and no other operation does.

**Delegation is by `endow_for_launch`, and it copies.** The supervisor keeps its
capability; the child receives its own name for the same bus at the intersection
of the rights asked for and the rights held (`SYSTEM_ABI_V1` §5 operation 22).
Nothing about the supervisor's authority is spent by delegating it.

**One boundary this version does not close, stated rather than left to be
discovered.** `capability_attenuate` refines *rights* and leaves scope exactly as
it was (`CAPABILITY_V1` §3), so V1 has no way to narrow a bus capability's
**range**: a delegate receives the granting capability's scope with at most its
rights. That is sufficient while one root covers one segment and one service
manages it, and it is honest about what a delegation currently means. Narrowing a
range needs an operation that makes a new object rather than a new name — the
distinction `CAPABILITY_V1` §3 draws between generic and scoped attenuation — and
that operation is not declared here because nothing needs it yet.

### `platform.pci.FunctionConfig`

A capability naming one **assignment** of one PCI function: a segment, a bus, a
device, a function and the generation of the assignment, all held in
nucleus-owned state.

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `pci_config_read` | `platform.pci.FunctionConfig` with `config_read` | `offset: u64`, `width: u64` | `Result<u64, i64>` | 25 |
| `pci_config_write` | `platform.pci.FunctionConfig` with `config_write` | `offset: u64`, `width: u64`, `value: u64` | `i64` | 26 |
| `pci_bar_map_read` | `platform.pci.FunctionConfig` with `map` | `bar: u64`, `offset: size`, `length: size` | `Result<MmioRegion, i64>` | 27 |
| `pci_bar_map_write` | `platform.pci.FunctionConfig` with `map` | `bar: u64`, `offset: size`, `length: size` | `Result<MmioRegionMut, i64>` | 27 |
| `pci_interrupt_claim` | `platform.pci.FunctionConfig` with `interrupt` | `entry: u64` | `Result<platform.irq.Source, i64>` | 28 |
| `dma_region_allocate` | `platform.pci.FunctionConfig` with `dma`, then `system.memory.Authority` with `spend` | `bytes: size` | `Result<DmaRegion<mut u8>, i64>` | 30 |
| `endow_for_launch` | `platform.pci.FunctionConfig` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |
| `capability_attenuate` | `platform.pci.FunctionConfig` with `none` | `rights: u64` | `Result<platform.pci.FunctionConfig, i64>` | 5 |
| `capability_release` | `platform.pci.FunctionConfig` with `none` | *(none)* | `i64` | 6 |

**No operation here takes a BDF.** A configuration access names an offset and a
width; which function it reaches is decided by the capability. So a holder
cannot address a different function — not because it is forbidden to, but
because there is no parameter through which to say so, and a fabricated device
number is a value with nowhere to go.

**Mapping is a third right, and the form is the operation** (ADR-0081 §13). A
holder that may read a device's registers is not thereby a holder that may map
its memory, so `map` is separate from both configuration rights. And a writable
window is asked for by calling the *other* operation rather than by passing a
flag: the two produce different types, so a module cannot arrive at a writable
mapping by computing a number.

**The caller never supplies a physical address.** It names a BAR index and a
page-aligned window inside that BAR; the base comes from what the device
reported and what the nucleus measured when the function was claimed. A request
not entirely inside the BAR's extent is refused rather than clamped, an I/O or
unimplemented BAR never becomes authority, and the granted scope is exactly the
pages mapped — a grant narrower than the window it hands out would be a contract
that lies about what its holder can reach.

**`config_read` and `config_write` are separate rights.** A capability carrying
only `config_read` refuses operation 26 with `E_NO_CAPABILITY`. This is the
attenuation a manager performs before handing a function to something that
should only look at it.

**Allocation takes two authorities and neither is sufficient** (ADR-0084 §4).
`dma_region_allocate` is declared here, on the assignment, because that is what
a DMA region descends from: memory becomes reachable *by a device*, and the
device is the one this capability names. The second capability is the
`system.memory.Authority` the run is charged to, at `spend`.

> a process holding only memory authority cannot make any memory reachable by
> any device, and a process holding only a function cannot spend somebody
> else's memory to do it

`dma` is its own right for the reason `map` is: a holder that may read a
device's configuration, or map its registers, is not thereby a holder that may
give it access to this machine's memory. A capability without `dma` refuses
operation 30 with `E_NO_CAPABILITY`, and so does an authority without `spend` —
independently, and neither refusal is the other's.

**The result is `DmaRegion<mut u8>`, and the byte is the decision**
(ADR-0085 §18, Project Architect-approved 2026-09-09). Operation 30 allocates a
contiguous run measured in **bytes**, and `u8` is the only concrete element type
for which every ABI-valid byte length has an exact representation — no invented
divisibility, alignment, tail or element-count rule for a caller to obey. A
driver encodes its protocol-defined structures into bytes explicitly, which is
better than implying that a TOS nominal or layout type has a device-visible
binary layout. ADR-0084's `DmaRegion<mut T>` is the abstract semantic shape;
this is its V1 instantiation, and §5 says what that does and does not mean.

**Everything fallible happens before anything is spent.** A refused allocation
leaves no region, no charge, no mapping and no capability — there is no partial
outcome a caller has to undo, which is why the two capabilities are resolved and
the caller's table checked before any memory is taken.

**Region grant facts** (`docs/42` §2). That document admits a
`Region<T>`/`DmaRegion<T>` grant **only** through a capability operation whose
accepted interface declares element type, alignment, access, size, DMA domain,
lifetime and transfer/share rules. Until version 4 no accepted operation
originated a region, so the requirement was met by there being nothing to meet
it for. One does now, and all seven are declared here rather than left to be
inferred from the nucleus:

| Fact | `dma_region_allocate` |
|---|---|
| element type | `u8`, and only `u8` (ADR-0085 §18). The result type says it, and no operation of any accepted schema produces a region of another element type |
| alignment | the region begins at a frame boundary, and one base plus an offset is one address because the run is **physically contiguous** (ADR-0084 §6a) |
| access | read and write, which is what `mut` in `DmaRegion<mut u8>` says. There is no read-only form of this operation and no right to attenuate to one |
| size | the whole frames covering `bytes`, which is **at least** what was asked for and is exactly what the holder may reach. A grant narrower than the extent it hands out would be a contract that lies about what its holder can reach; `bytes` of zero, or more than the contract's maximum, is `E_BAD_ARGUMENT` |
| DMA domain | the one assigned function the first capability names, and no other device. There is no parameter through which to name a second, and a function with no PCI Express capability is refused rather than given memory it could not be proved to have stopped reaching (ADR-0084 §5c) |
| lifetime | from the successful allocation until a successful `capability_release` of the region, which quarantines the backing before it returns to the pool. The region is a descendant of the assignment, so the assignment does not end while one lives (§4.1) |
| transfer/share rules | **neither** (ADR-0037). `DmaRegion<T>` and `DmaRegion<mut T>` are non-transferable and non-shareable in both modes: a region does not cross a task boundary and `share` does not accept one |

**What a `FunctionConfig` does not confer, even with every right** (ADR-0082 §5,
§5a–§5d). These are not exceptions carved out of the rights model. They are
places where the hardware happens to keep the *platform's* state inside a range
these operations reach:

- a **window overlapping the MSI-X table or pending-bit array** is refused by
  `pci_bar_map_read` and `pci_bar_map_write` alike. The table holds a message
  address and a message data word, so writing it is choosing which interrupt is
  delivered to which vector — authority taken by writing a number, which is what
  this whole schema exists to make impossible;
- a **configuration write touching the MSI-X capability**, or the conventional
  **MSI capability over the extent that capability itself reports**, is refused.
  Both decide where interrupts go, and the rule is about interrupts rather than
  about a transport: a device offering only MSI must not thereby offer a way
  round the interrupt authority model;
- a **configuration write that would change a resource-placement register** is
  refused, stated over the function's **reported header type**. For a Type-0
  function that is BAR0–BAR5 and the expansion ROM register; for a Type-1 bridge
  it is its two BARs, its bus numbers, every forwarding window, its expansion
  ROM register, and the Bridge Control bits that alter downstream routing or
  reset what is behind it. **A 64-bit BAR pair is one placement and both halves
  are protected**, so neither can move while the other looks untouched;
- a **configuration write that would change Memory Space Enable or Bus Master
  Enable** is refused. Each follows its own predicate over the assignment's live
  descendants — decoding for a window, mastering for an interrupt source — and
  the two are independent.

**Reads still work**, and that is the model rather than an oversight: where a
table lives, where a function decodes and whether it is a bus master are facts
the device reports, and §4 already says nothing read here is authority.

**"Would change" is literal.** A write that puts back the value already there
proceeds, judged byte by byte for a register and bit by bit for a bit, so a
caller reading and writing back is never refused for a field it did not alter.
Status and error-reporting bits are not reserved merely for sharing a register
with something that is.

**A claim leaves the function in a defined state**: decoding off, mastering off,
MSI-X disabled and function-masked, MSI disabled — whatever the firmware left,
and before the first capability naming the assignment exists. The pre-claim
state is not restored on release.

**Conventional configuration space only.** `offset + width` must lie within the
first **256** bytes, `width` must be 1, 2 or 4, and `offset` must be a multiple
of `width`. Every violation is `E_BAD_ARGUMENT`; nothing wraps, nothing is
truncated, and a refused access reads and writes nothing.

That bound is this version's honest promise rather than a temporary limitation
dressed as one: it is what the accepted mechanism reaches, and it is what Stage
4A needs — the VIRTIO PCI capability list is reached through the capability
pointer at `0x34` and lives in standard configuration space. Extended
configuration space is a later version of this contract with a different
mechanism underneath it, and **the capability model above does not change when
that happens**: a `FunctionConfig` names a function, not a way of reaching one.

**What a value read here is, and is not.**

A configuration read returns a number the **device** reported. Two consequences
worth stating, because both are places a reader might assume otherwise:

- **A BAR is data.** Offsets `0x10`–`0x27` return base-address registers. **No
  operation of any accepted schema takes one**, so a BAR value grants no mapping,
  no physical memory access, and cannot be presented where authority is
  required. That is unchanged by `pci_bar_map_read` and `pci_bar_map_write`
  existing: they take a BAR *index* and a window inside it, and the base comes
  from what the nucleus measured when the function was claimed. A module that
  read a BAR and passed the number back would be passing it to no parameter.
- **Nothing read here is authority.** A vendor identifier, a class code and a
  capability pointer are facts about hardware. Deciding which driver should own
  a function is policy, evaluated by a bus manager, and ADR-0051 deliberately
  leaves it open.

### `platform.irq.Source`

A capability naming **one routed interrupt of one assigned function**: the
assignment it descends from, the MSI-X table entry it occupies, and its own
generation, all held in nucleus-owned state.

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `irq_wait` | `platform.irq.Source` with `wait` | *(none)* | `i64` | 29 |
| `endow_for_launch` | `platform.irq.Source` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |
| `capability_attenuate` | `platform.irq.Source` with `none` | `rights: u64` | `Result<platform.irq.Source, i64>` | 5 |
| `capability_release` | `platform.irq.Source` with `none` | *(none)* | `i64` | 6 |

**Where a capability of this comes from, and from nothing else.** Operation 28
on a live `platform.pci.FunctionConfig` carrying `interrupt`. There is no
interrupt-controller capability, no "may route anything" authority, and no rule
anywhere naming which module may have interrupts: a holder of a function may ask
for that function's interrupts, and a holder of no function cannot ask for
anybody's.

**A number is not authority, and this schema has nowhere to put one.** No
operation of any accepted schema takes a CPU vector, a GSI, a legacy IRQ number,
an MSI address/data pair or a BDF, and none of those appears in any result. The
one number operation 28 takes is an **MSI-X table entry index within the function
the caller's capability already names** — the same class of argument as
operation 27's BAR index, selecting among things the capability covers and
unable to reach outside them. A fabricated index is `E_BAD_ARGUMENT`, and a
fabricated one that happens to be in range still names an entry of the caller's
own device.

**`wait` is the only right, and the two that are absent are absent for a
reason.** There is no mask right and no acknowledge right, because neither has
an operation: delivery is edge-triggered into a one-bit latch, which needs no
masking for correctness; the nucleus masks a table entry when its source is
released, which is mechanism rather than an exposed operation; and an MSI-X
interrupt is ended by the local APIC before the handler returns, so there is
nothing for a process to acknowledge. A right with no operation would be a
contract describing a system that does not exist.

**At most one live source per (assignment, entry), and at most one waiter.** A
second claim of an occupied entry is `E_LIMIT` while the first lives; the
exclusivity is a property of the claim rather than of the capability, so
`capability_attenuate` may still make another **name**. What stays singular is
the wait: a second concurrent `irq_wait` is `E_LIMIT`, however many capabilities
name the source.

**The latch is a bit and not a count.** An interrupt arriving with nobody
waiting sets it, and the next `irq_wait` clears it and returns `OK` without
blocking — so a completion cannot be lost by racing the call. A count would
invite a holder to pair wakeups with completions, which is false of any device
that coalesces, and coalescing is required rather than merely permitted by
`docs/35` §Stage 4. The obligation a bit leaves is the one every queue driver
already has: after a wake, drain until empty.

**A source is a descendant of its assignment** (ADR-0081 §14, ADR-0082 §6), so
the assignment does not end while one exists. Releasing the function and
re-claiming the same BDF therefore cannot be reached by an interrupt of the
first assignment: the generation advances only when nothing reaches it.

**Waiting on a live source is not a stalled system.** `SYSTEM_ABI_V1` §6's
liveness rule asks what could still end a wait, and this is the first blocking
reason whose answer is not "a context of this system". A context inside
`irq_wait` on a live source is idle, not stopped; destroying the source cancels
the wait with `E_CANCELLED` at that instant.

**Bus mastering, stated where a holder can see it.** An MSI-X message is a memory
write the device issues, so a live source necessarily makes its function a bus
master (ADR-0082 §5d). On a platform with no IOMMU, **TOS cannot claim
hardware-enforced confinement of a malicious bus-mastering device**: the
capability model controls which sanctioned DMA objects and device-visible
addresses software may *obtain*; it does not physically prevent a malicious
driver from programming a bus-mastering device with some other address. `docs/34`
S5 governs this and is satisfied by saying it rather than by implying otherwise.

### `platform.dma.Region`

A capability naming one DMA region: a bounded run of physical memory, charged to
a `MemoryAuthority` and made reachable by one assigned function (ADR-0084). Two
operations, and no third — everything else a holder does with a region is
ordinary indexed access from TOS Core, which is not an operation of any schema.

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `dma_device_address` | `platform.dma.Region` with `none` | `offset: size` | `Result<u64, i64>` | 31 |
| `capability_release` | `platform.dma.Region` with `none` | *(none)* | `i64` | 6 |

**Possession is what issues an address, so no right is declared.** ADR-0084 §6b
puts the bounded-offset arithmetic in the nucleus and the nucleus requires no
right beyond resolving the handle: an address is not authority, so there is
nothing to refine, and making one obtainable through one name of a region and
not another would imply otherwise. `E_BAD_ARGUMENT` for an offset outside the
region's own extent — no address at all rather than a clamped one — and
`E_NO_CAPABILITY` for a handle that does not resolve to a live region.

**`capability_release` is `SYSTEM_ABI_V1` operation 6, declared here for the
reason it is declared on every other releasable interface** (ADR-0085 §10).
Omitting it would mean a DMA region could never be released from text, which is
not symmetry — it is the difference between an object with a lifetime and one
without.

**There is no `endow_for_launch` and no `capability_attenuate`.** A region cannot
be a startup endowment for the same reason it cannot be imported (§4.4), and
there is no right on it to refine.

## 4.4 `platform.dma.Region` is represented by the region family

This is the one interface of the whole accepted corpus whose capability
representation is not `AsInterface` (`SYSTEM_INTERFACE_V1` §4.3, ADR-0085), and
the reason is that a DMA region must be two things the accepted model expressed
one of per object:

| | |
|---|---|
| ADR-0081 §2 | indexed CPU access, which needs the nominal region family |
| ADR-0084 §6b | a bounded-offset translation done by the nucleus, reached by an operation, which needs an interface path |

So the interface path stays the identity — it is what `Signature.effects`
carries, what an `extern` item's `uses` names, and what the capability parameter
of the two operations above is declared as — while the **values** of that
authority are `DmaRegion<T>` and `DmaRegion<mut T>`:

```text
representation_of(platform.dma.Region) = DmaRegionFamily
```

Concretely, and each of these is checked independently:

- at a `platform.dma.Region` capability position the argument's type must be
  `DmaRegion<T>` or `DmaRegion<mut T>`, for any element type `T`. There is no
  conversion, in either direction;
- `Capability("platform.dma.Region")` written directly at that position is
  **refused**. The interface's own path is not a member of the family that
  represents it;
- an `import capability platform.dma.Region` is refused **in source** with
  `E1503_NONIMPORTABLE_CAPABILITY`, because no startup import could produce a
  value of that family: there is nowhere in the declaration for an element type
  or a mutability to come from;
- a `CapabilitySource::Import` at such a position is refused by the verifier on
  its own, whatever interface the import declared. No frontend emits one, which
  is exactly why the verifier must refuse it rather than assume it;
- the enclosing function must still declare `platform.dma.Region` in its `uses`,
  the runtime object must still be a DMA region, and the nucleus still checks the
  handle, the object, the generation and the liveness per call.

**One handle, no alias and no second authority.** A `DmaRegion` value is one
runtime capability handle (ADR-0085 §9): `region[i]` resolves that handle to a
base through the runtime's own mapping metadata and loads, and a capability
position passes the same handle to the nucleus. There is no second object, no
alias capability, no hidden conversion and no promotion — two things done with
one authority, which is what a capability is for.

**Release ends both paths, and they end differently** (ADR-0085 §8a). A
successful `capability_release` invalidates the nucleus authority *and* retires
that handle's runtime mapping entry before control returns to TOS Core.
Afterwards `dma_device_address` answers `E_NO_CAPABILITY` at the nucleus, and
`region[index]` is refused by the runtime bridge **before any memory access** —
deterministically, as a mapping this process does not hold, and never by relying
on a page fault or an eventual unmapping. A **failed** release leaves both
intact, because the object did not end.

**Use is not consumption.** No operation of this interface consumes its
receiver, so a binding may be used as many times as a module writes it —
`dma_device_address`, then an indexed write, then `dma_device_address` again.
Affinity is about how many values name the object, and using one value twice
creates no second value.

## 4.1 Assignment, and the three lifetimes it is not

**At most one live assignment exists for a function under one root.** A claim of
a function that is already assigned is refused with `E_LIMIT` while the first
assignment lives. The exclusivity is a property of the **claim**, not of the
capability: several capabilities may name one assignment, because
`capability_attenuate` makes another name and a later split between a manager
and a driver needs exactly that.

Three facts, none of which implies another:

| | |
|---|---|
| the device exists | true whether or not anything names it; this contract never asserts it |
| the assignment lives | from a successful claim until **neither a capability names it nor a derived hardware object descends from it** — a mapped window or a routed interrupt source (ADR-0081 §14, ADR-0082 §6) |
| a handle resolves | one process's name for the assignment, with its own handle generation |

**The assignment carries a generation.** Releasing a function and claiming the
same one again produces a new assignment at a new generation, so a handle held
across that gap resolves to nothing rather than to the new occupant — the same
rule `CAPABILITY_V1` §2 states for every other object, applied to the one thing
here that can be released and re-made.

**Descendants keep it alive, and the row above says so because two of them now
exist.** Version 1 of this schema described the assignment as lasting to the loss
of its last name, which was exact while a `FunctionConfig` capability was the only
thing that could reach one. It is superseded: a manager releasing its own handle
must not destroy a driver's window, and releasing the last handle must not let the
same BDF be claimed again while a window or an interrupt source is still reaching
it. Only when both counts fall to zero does the claim end and the generation
advance.

## 5. What this version does not declare

No reset operation and no device-class publisher.

**The DMA surface is complete as of version 4**, in three decisions that each
had to land before the next could: ADR-0084 (Accepted 2026-09-08) fixed where
DMA authority comes from, what a device-visible address is allowed to be and how
a region is proved safe to reclaim; ADR-0085 (Accepted 2026-09-08) fixed how
such an authority is represented in TOS Core; and ADR-0085 §18's resolution
fixed the one thing neither had — the concrete result of the operation that
makes one:

```text
dma_region_allocate(...) -> Result<DmaRegion<mut u8>, i64>
```

(Project Architect-approved, 2026-09-09.) No schema polymorphism and no new
language mechanism: ADR-0084 writes the result abstractly as `DmaRegion<mut T>`,
which is its **abstract semantic shape**, and the V1 concrete surface
instantiates it at `T = u8`. Operation 30 allocates a contiguous run measured in
bytes, and `u8` is the only concrete element type for which every ABI-valid byte
length has an exact representation without inventing a divisibility, alignment,
tail or element-count rule a driver would then have to obey. A driver encodes
its protocol-defined structures into bytes explicitly, which is better than
implying that a TOS nominal or layout type has a device-visible binary layout.

**The narrowing is stated here so it cannot be mistaken for something more
general:**

> **V1 has no generic DMA allocation.** No operation of any accepted schema
> produces a `DmaRegion<mut T>` for a caller-chosen `T`, and no family of
> `_u16`/`_u32`/`_u64` rows is pre-allocated against a requirement nobody has
> demonstrated. A later driver that genuinely needs typed allocation or typed
> views must first define the element-count, alignment and layout semantics that
> would make one meaningful, and is considered separately.

The row is declared in §4 above, with the Stage 4C-2 implementation that
performs it — by §2's rule, and not before it: an interface that declared an
operation the system does not perform would be a contract describing a system
that does not exist.

Reset and the publisher remain undecided — the publisher under ADR-0051.

**No right is allocated for either operation of `platform.dma.Region`.** Both
declare `none`, which is what the nucleus requires: possession of the region is
what issues an address, and release needs the capability rather than a right
over it. A right with no operation would be the speculative declaration §2
refuses.

**Two of the four this list held in version 1 have arrived, and neither arrived
as the name that was reserved for it.** Device memory became operations 27 on
`platform.pci.FunctionConfig` rather than a `platform.mmio.RegionMap` interface
(ADR-0081 §13), and interrupts became `platform.irq.Source` derived from an
assignment rather than a `platform.irq.Binding` (ADR-0082 §3). That is the rule
working: a mechanism decides its own shape, and a name reserved in advance would
have been a decision made before the analysis.

**No reset right is allocated.** A right with no operation would be exactly the
speculative declaration §2 refuses, one layer down.

## 6. Conformance evidence

1. An `extern fn` matching an operation declared here is accepted by checker and
   verifier; one differing in name, arity, parameter type, result type or effect
   is `E1801_FFI_NOT_AVAILABLE` with the reason named — the same rule
   `SYSTEM_INTERFACE_V1` §10 states, over this schema's table.
2. A module importing `platform.pci.Bus` without being granted one fails at
   startup with `CapabilityDenied` and never reaches a call.
3. A textual module reads a real device's vendor, device, class and capability
   pointer through `pci_config_read`, and the values are the device's.
4. A capability with `config_read` and not `config_write` refuses operation 26.
5. A function capability cannot reach another function: there is no parameter,
   and a claim of a second function requires a bus capability.
6. A claim of an already-assigned function is refused while the assignment
   lives, and a handle from a released assignment refuses by generation.
7. `offset`, `width` and their sum are bounded as §4 states, and each violation
   is `E_BAD_ARGUMENT` with nothing read and nothing written.
7a. A window over the function's MSI-X table is refused in **both** map forms; a
   configuration write touching the MSI-X or MSI capability is refused; a
   configuration write that would change a resource-placement register of the
   reported header type, or Memory Space Enable, or Bus Master Enable, is
   refused. Each is `E_NO_CAPABILITY`, and each leaves the device untouched.
8. And the narrowing is shown to be a narrowing: reading the MSI-X capability
   still succeeds, writing a placement register back unchanged still succeeds, a
   window still derives the extent measured at claim time, an unrelated writable
   field is still writable, and a Command-register write that changes no owned
   bit still succeeds — a refusal that refused its neighbours too would prove
   nothing about what it was protecting.
9. A claim normalises the function, and the nucleus records what firmware left,
   because after the claim nothing else can observe it.
8. The interface paths a verified module uses are readable from its IR without
   executing it, and match the `uses` effects of its declared operations.
