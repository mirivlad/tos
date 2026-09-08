<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0085: An interface that is not its own value type

- Status: **Proposed — not Project Architect-approved.** Nothing is implemented,
  and Stage 4C-2's surface stays stopped until this is ruled on
- Date: 2026-09-08
- Decision level: **2** — it amends the accepted interface model in one place:
  what a capability position requires of the value supplied to it. It adds no
  operation, no object kind, no right, no IR variant and no type constructor
- Related: **ADR-0080** (§5, §7: the interface path *is* the capability type, and
  the verifier's equality test), `SYSTEM_INTERFACE_V1` §4.1 and §5, ADR-0078
  (authority sources), **ADR-0081 §2** (`DmaRegion<T>` indexed access),
  **ADR-0084 §6b** (the nucleus resolves a bounded device-visible offset),
  ADR-0037 (`DmaRegion` affinity), `docs/42` §2

## 1. What was found, and where

Stage 4C-2 stopped before writing its surface because a `DmaRegion` has to be two
things at once and the accepted model can express one of them per object.

- **ADR-0081 §2** gives `DmaRegion<T>` and `DmaRegion<mut T>` indexed CPU
  access — `region[i]` — which requires the value's type to be the **nominal
  region family** the frontend resolves in `types.rs`;
- **ADR-0084 §6b** requires a bounded device-visible offset to be resolved **by
  the nucleus**, through an operation reached on the region capability — which
  requires an **interface path**, because that is the only thing a capability
  position can name.

Those two namespaces are disjoint, and nothing bridges them. In the accepted
schema a capability requirement is `Requirement { interface, right }`; an
`Interface` is `{ path, object, operations }`; and no nominal type appears as a
requirement anywhere, because until now none needed to. `MmioRegion` looks like
a counter-example and is not: it is a **result** type only, and its accesses are
language intrinsics rather than operations, so it never occupied a capability
position.

**This is a gap in the model rather than a defect in an implementation**, which
is why Stage 4C-2 stopped rather than choosing. Two workarounds were available
and both were rejected by the review, correctly: resolving the offset in the
runtime would move an arithmetic ADR-0084 §6b had just placed in ring 0, and
splitting the object into a CPU-accessible value and an operation-capable
interface object would defer the collision to the first real Virtqueue, which
needs both at once.

## 2. The old model, stated exactly

Two ideas that have always coincided, and were therefore never separated:

| | |
|---|---|
| **interface** | the set of operations and the effect identity — what a holder may *do*, and what a `uses` clause names |
| **capability value type** | the nominal type a value of that authority *has* in a program |

`SYSTEM_INTERFACE_V1` §5 says it as plainly as it can be said:

> an interface path written as a type resolves to the capability type it is
> rather than to a nominal record that merely shares its name

and ADR-0080 §7 turns it into the verifier's test:

> `Value(operand)` — the operand's type is `TypeDef::Capability(interface)` with
> the interface **equal** to the schema-required one for that position

For `system.ipc.Endpoint`, `platform.pci.FunctionConfig` and
`platform.irq.Source` this is exactly right and stays exactly as it is: the path
is the type, the type is the path, and there is nothing else a value of that
authority could be.

**`DmaRegion` is the first object for which it is not a general law.** Its
authority is a capability; its value is a member of a region family with an
element type and a mutability, and it is that family which makes `region[i]`
mean something.

## 3. The new model, minimally

> An accepted interface **may declare a value family distinct from its own
> path**. Where it does, a capability position of that interface admits a value
> of that family. Where it does not — which is every interface that exists
> today — the position admits exactly what it admits now: a value whose type is
> that interface path.

That is the whole amendment. Written as the schema entry it would be:

```text
interface path   platform.dma.Region
object kind      DmaRegion                 → OBJECT_DMA_REGION
value family     DmaRegion<T>, DmaRegion<mut T>
operations       dma_device_address(offset: size) -> Result<u64, i64>
                 capability_release() -> i64
```

and a program then writes

```tos
let byte: u64 = region[0B];                        // ADR-0081 §2, unchanged
let at: u64 = dma_device_address(region, 64B);     // ADR-0084 §6b, now expressible
```

with **one capability object** behind both.

### What this deliberately is not

Each of these would turn a narrow amendment into a different model, and each is
refused:

- **not** "any nominal type is a capability". The relation exists only where an
  accepted schema entry declares it, and nowhere else;
- **not** an implicit conversion. No value becomes a capability by being passed
  somewhere, and no capability becomes a value;
- **not** `AnyCapability`, an erased handle, or structural typing. The value
  family is named, closed and per-interface;
- **not** a way to make a schema record a capability. A record is data and stays
  data;
- **not** automatic. An interface with no declared value family behaves exactly
  as today, and adding one to an existing interface would be a separate decision
  about that interface.

## 4. The five checks, and that they stay five

The amendment changes **one** of them and leaves the other four alone. They are
listed together because the point of the change is that these are four different
questions that a single name was answering by accident:

| | Check | Where | Changed? |
|---|---|---|---|
| 1 | the operation belongs to the declared interface | frontend, from the schema | no |
| 2 | the enclosing function's effect set admits that interface | frontend and verifier | no |
| 3 | **the supplied value's type is admitted by that interface** | frontend and verifier | **yes**: equality becomes admission, and equality is what admission means for every interface without a declared family |
| 4 | the runtime object kind matches the interface | launcher and nucleus at grant | no |
| 5 | handle, object, right, generation and liveness | nucleus, per call | no |

So for the DMA case the four dimensions stay four, and none of them is
load-bearing for another:

```text
effect interface   platform.dma.Region
value type         DmaRegion<mut u8>
runtime object     OBJECT_DMA_REGION
required right     whatever operation 31 declares
```

## 5. Impact

**TOS Core language version.** None. No new type constructor, no new syntax, no
new predeclared name. `DmaRegion<T>` is already in the V1 type surface
(`docs/40` §2) with its arity and its four ADR-0037 facts; what changes is that
a schema may now say a capability position accepts it. A program written against
V1 is a V1 program before and after.

**IR.** None. `tos-ir/v1` has carried `TypeDef::Capability(interface)` and the
region families since it was written, and this adds no variant. What a capability
position records is unchanged: `Signature.effects` still carries the **interface
path**, because effect identity was always the path and never the value type —
ADR-0080 §4 settled that and this does not disturb it.

**Verifier.** One predicate widens. ADR-0080 §7's authority-source proof keeps
both alternatives and both remain per-position; the `Value(operand)` arm changes
from *the operand's type equals the required interface* to *the operand's type is
admitted by the required interface*, where admission is equality unless the
schema declares a family. `Import(index)` is untouched. The verifier still proves
the effect and the authority source independently, which is the property ADR-0080
§7 exists to have.

**Nucleus.** None. It never saw a value type: it resolves a handle to an object,
checks kind, right, generation and liveness, and that is check 5 above.

### Old interfaces do not change meaning or identity

Three separate claims, each checkable:

1. **No existing schema entry declares a value family**, so for every interface
   that exists today admission is equality and check 3 is the same predicate on
   the same inputs;
2. **no effect changes**, because effect identity is the interface path and no
   path moves;
3. **no artifact changes**, because nothing in the encoding moves: the same
   `TypeDef::Capability(path)` for the same positions, the same effect set, the
   same operation rows. A module compiled before this amendment and one compiled
   after produce the same IR and the same digest.

The conservative reading is available and is worth stating: this amendment makes
strictly more programs well-typed and rejects none that were.

## 6. Why one capability object, with no alias and no copy

The obvious worry is that a value which can be both indexed and passed to an
operation is two handles wearing one name. It is not, and three separate
mechanisms say so:

- **there is one value.** `region[i]` and `dma_device_address(region, …)` name the
  same binding, in the same scope, under the ordinary ownership rules. Neither
  form constructs a value, converts one, or derives a second from the first;
- **the object is affine, in the language and in the nucleus.** ADR-0037 §2 makes
  both `DmaRegion` forms non-shareable and non-transferable, and Stage 4C-2 put
  `Object::DmaRegion` into `is_affine` and out of `is_delegable` — so generic
  attenuation, IPC transfer, endowment and launch-plan delegation each refuse. A
  second name cannot be made by any path, which is a stronger statement than the
  type system alone makes;
- **one handle reaches the nucleus.** A capability position crosses the ABI as a
  handle in the register that position assigns, and the nucleus resolves that one
  handle. Indexed access crosses nothing — it is a load through a mapping the
  process already has, which is exactly what ADR-0081 §2 decided it is.

So the two forms are two things done *with* one authority, which is what a
capability is for. The alternative — B in the Stage 4C-2 STOP — would have been
the version with two objects, and the reason to refuse it is that a Virtqueue
needs both at once and would have had to hold two names for one buffer.

## 7. The exact contract text this amends

Minimal and located, so a reviewer can see the whole of it:

- **`SYSTEM_INTERFACE_V1` §5**'s sentence that an interface path written as a
  type "resolves to the capability type it is" gains its exception: it does,
  **unless the schema entry declares a value family**, in which case the position
  admits that family. §4.1's mechanism for declaring capability parameters is
  untouched;
- **ADR-0080 §7**'s `Value(operand)` arm changes *equal to* into *admitted by*,
  with admission defined as equality for an interface that declares no family.
  §4's "effect identity is the interface path" is explicitly **not** amended;
- the schema type in `tos-core` gains one optional field on `Interface`, and no
  existing entry sets it.

Nothing else in either document moves.

## 8. Conformance evidence this ADR will require

1. every interface accepted before this amendment resolves and verifies
   identically, and a corpus module's IR and digest are unchanged;
2. a capability position of an interface with **no** declared family still
   refuses a value of any other type, including a region family;
3. a capability position of `platform.dma.Region` accepts `DmaRegion<mut u8>`
   and refuses a `Region<mut u8>`, an `MmioRegionMut` and a bare integer;
4. the four dimensions are refused independently: right interface and wrong
   effect; right effect and wrong value family; right value and wrong runtime
   object kind; and right everything with the wrong right;
5. `region[i]` and `dma_device_address(region, …)` in one function over one
   binding produce **one** handle at the ABI, and no second capability entry
   exists in the caller's table;
6. no nominal type outside a declared family is admitted in any capability
   position — checked structurally over the schema, so the amendment cannot be
   read as "any nominal type is a capability".

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none. No new authority, no new
  object, no widening of what any capability confers.
- **Canonical representation:** unchanged.
- **Trusted-base impact:** none. The nucleus's five-part check is untouched, and
  it never saw a value type.
- **Source-to-runtime impact:** none. No IR variant, no artifact-format change,
  and existing modules produce identical artifacts.
- **Compatibility profile:** `SYSTEM_INTERFACE_V1` gains one exception clause and
  the schema type one optional field. **TOS Core stays at V1.**
- **Threat-model impact:** none. The amendment makes more programs well-typed and
  loosens no runtime check; a value admitted here still faces checks 4 and 5
  unchanged.
- **New dependencies:** none.
