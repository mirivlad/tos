<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0085: Capability representation, separated from interface identity

- Status: **Proposed — not Project Architect-approved. Revision 2 (C′).**
  Nothing is implemented; Stage 4C-2's surface stays stopped
- Date: 2026-09-08
- Decision level: **3, and §13 argues why** rather than assuming it. Revision 1
  claimed Level 2 and that claim is withdrawn
- Related: **ADR-0078** (`CapabilitySource::Value`), **ADR-0080** (§4, §5, §7),
  `SYSTEM_INTERFACE_V1` §4.1 and §5, **ADR-0081 §2** (`DmaRegion<T>` access),
  **ADR-0084 §6b** (the nucleus resolves a bounded offset), ADR-0037
  (`DmaRegion` affinity), `docs/40` §2, `docs/44`

**Revision 1 is withdrawn.** It stated the change as "an interface *admits* a
value family", which is a general relation between a capability interface and
arbitrary value families — a trait-like model TOS does not have and this slice
must not introduce. Revision 2 states a **closed representation mechanism**
instead, and §4 fixes the cardinality rules that keep it closed.

## 1. The gap, restated exactly

A `DmaRegion` must be two things and the accepted model expresses one per object:
ADR-0081 §2 gives it indexed CPU access, which needs the nominal region family;
ADR-0084 §6b puts the bounded-offset arithmetic in the nucleus, reached by an
operation, which needs an interface path. A capability requirement names only an
interface path, and no nominal type has ever occupied a capability position.

Options A (resolve the offset in the runtime) and B (two authority-bearing
objects) are both refused and stay refused: A moves an arithmetic ADR-0084 §6b
deliberately placed in ring 0, and B gives a Virtqueue two names for one buffer.

## 2. The old model

| | |
|---|---|
| **interface path** | identity of operations and of effects |
| **capability value type** | the type a value of that authority has in a program |

They coincide for every accepted interface, and two normative sentences say so:

- `SYSTEM_INTERFACE_V1` §5: "an interface path written as a type resolves to the
  capability type it is";
- ADR-0080 §7: `Value(operand)` requires the operand's type to be
  `TypeDef::Capability(interface)` with the interface **equal** to the required
  one.

## 3. C′ — the new model

A schema entry gains **one** field:

```text
Interface {
    path                       identity of operations and effects
    capability_representation  the exact class of TOS Core values this
                               authority is represented by
    object_kind                the runtime object a grant must name
    operations
}
```

`capability_representation` is drawn from a **closed set fixed by this ADR**, not
from types in general:

```text
representation ::= AsInterface                 // TypeDef::Capability(path)
                 | DmaRegionFamily             // TypeDef::DmaRegion | DmaRegionMut
```

`AsInterface` is the default and is what every existing interface has. Adding a
member to that enumeration is a decision of the same weight as this one — a new
ADR — which is the mechanism that keeps it closed rather than a promise that it
will stay small.

```text
interface path            platform.dma.Region
capability_representation DmaRegionFamily
object_kind               OBJECT_DMA_REGION
operations                dma_device_address(offset: size) -> Result<u64, i64>
                          capability_release() -> i64        // §10: generic op 6
```

**Not** "an interface admits a type". A representation is a named member of a
closed enumeration, chosen per interface by the accepted schema.

## 4. Cardinality and uniqueness

1. **one representation family belongs to at most one accepted interface.**
   `DmaRegionFamily` belongs to `platform.dma.Region` and to nothing else, so a
   value of that family has exactly one interface it can satisfy;
2. **an interface has exactly one representation.** No interface has two
   unrelated families;
3. **the association exists only in the accepted schema.** There is no attribute,
   annotation or declaration by which a module could create one;
4. **a program-defined or ordinary nominal type can never be a representation.**
   The enumeration's members are fixed here and name only families the language
   contract already defines;
5. **absence means exactly the old semantics** — `AsInterface`, which is
   `TypeDef::Capability(path)`.

Refused, and each would be a different model: arbitrary nominal types,
user-defined records, structural typing, `AnyCapability`, erased handles,
implicit conversions, automatic promotion, and any trait-like relation.

## 5. Frontend type rule

At a capability position of operation `op` requiring interface `I`:

```text
representation_of(I) = AsInterface        ⇒ type(arg) must be exactly Capability(I)
representation_of(I) = DmaRegionFamily    ⇒ type(arg) must be DmaRegion<T>
                                            or DmaRegion<mut T>, any T
```

No other type is accepted in either case, and there is no coercion in either
direction. The effect check (`I` ∈ enclosing `uses`) is unchanged and separate.

## 6. Lowering rule

Unchanged. A capability position lowers to `CapabilitySource::Value(operand)` or
`CapabilitySource::Import(index)` exactly as today; the operand is the SSA value
of the argument, and its type is whatever the type table already says. Nothing
about the encoding of a capability position depends on the operand's type.

## 7. Verifier rule

```text
capability_representation_matches(required_interface, operand_type) :=
    match representation_of(required_interface) {
        AsInterface     => operand_type == Capability(required_interface),
        DmaRegionFamily => operand_type is DmaRegion(_) or DmaRegionMut(_),
    }
```

Two properties this has to have, and does:

- **for every interface that exists today it is the old predicate**, character
  for character: `AsInterface` reduces to the exact equality of ADR-0080 §7;
- **it does not trust the frontend.** `representation_of` is read from the
  accepted schema, which the verifier compiles in exactly as the frontend does;
  `operand_type` is read from the artifact's own type table. Both inputs are
  independent of whoever produced the module, which is what makes a forged IR
  detectable — §17 lists the negatives.

## 8. Ownership: use, copy and consume are three things

The distinction the review asked for, and it is already the system's:

| | |
|---|---|
| **use** in a capability position | **non-consuming**. The value is read, its handle crosses the ABI, and the binding is still live afterwards |
| **copy** — a second value naming one object | what affinity forbids (ADR-0037), and what `Object::is_affine`/`is_delegable` forbid in the nucleus |
| **consume** | no operation of this interface consumes its receiver in the language |

So this is well-typed and each line is an ordinary use of one live binding:

```tos
let a: u64 = dma_device_address(region, 0B);
region[0B] = 1u64;
let b: u64 = dma_device_address(region, 64B);
```

**This is not new and is not a concession to DMA.** Existing vectors already use
a capability twice — `capability_attenuate(child, …)` and later
`capability_release(child)` — so a capability parameter has never consumed its
argument. Affinity is about *how many values name the object*, and using one
value twice creates no second value.

`capability_release(region)` ends the authority **at the nucleus**: the last name
goes, the region is destroyed, its backing is quarantined, and the handle stops
resolving. Afterwards the binding is a stale handle and every operation on it
answers `E_NO_CAPABILITY` — proved at runtime by the object's generation, exactly
as Stage 4C-1b's `irq-wait-negative` proves for a released `platform.irq.Source`.
**This ADR does not add a consuming rule to the language**, and if the Project
Architect wants release to be statically consuming that is a separate decision
about every capability kind, not about this one.

## 9. Why one object, with no alias and no hidden copy — now provable

Revision 1 argued this from the type system. Revision 2 can point at the runtime:

**an `MmioRegion` value is already `Value::Capability(handle)`**, and its base and
length live in a side table keyed by that handle. The mapping-producing
operations return `Value::Capability(Handle::new(value))`; `remember` files
base/length under the same handle; and every indexed access looks the base up
from it.

So a `DmaRegion` value is one handle. `region[i]` resolves that handle to a base
and loads; a capability position passes that same handle to the nucleus. **There
is no second value, no second handle and no conversion** — the two forms are two
things done with one authority, which is what a capability is for.

## 10. `capability_release` is generic operation 6, not new surface

It is `SYSTEM_ABI_V1` operation 6, declared per interface in the schema exactly
as it is for `platform.pci.Bus`, `platform.pci.FunctionConfig` and
`platform.irq.Source`. Declaring it for `platform.dma.Region` adds no mechanism;
**omitting** it would mean a DMA region could never be released from text, which
is not symmetry, it is the difference between an object with a lifecycle and one
without. It is included for that reason and no other.

## 11. IR impact — argued, not asserted

**No new variant, no new tag, and the reason is that the case already exists.**
`TypeDef::DmaRegion` (tag 20) and `TypeDef::DmaRegionMut` (tag 22) are in
`tos-ir/v1` today and are encoded and decoded by the image format. A capability
position stores an operand id, and types come from the type table; nothing in the
encoding of `CapabilitySource::Value` depends on the operand's type.

What is genuinely new is that a capability position may now *refer to* an operand
whose type is one of those tags. That is a new **combination** of existing
encodings, not a new encoding. `MmioRegionMut` already demonstrates the halves
separately: a non-`Capability` TypeDef whose runtime value is a handle.

**What this ADR does not claim**: that no verifier code changes. §7's predicate is
a real change to the verifier, and a verifier that did not implement it would
reject a valid artifact. That is a conformance change and is why §13 argues
Level 3.

## 12. TOSIMAGE, encoding and identity impact

- **encoding**: no format change. No tag is added, removed or renumbered;
- **existing artifacts**: byte-identical. No existing schema entry declares a
  representation other than `AsInterface`, no lowering path changes, and no type
  is encoded differently. A module compiled before and after produces the same
  image and the same digest — this is a claim about **existing** modules only,
  and modules using the new form are new artifacts with new digests, which is
  ordinary;
- **canonical identity**: unchanged. Module identity is the source and its
  provenance, and neither moves.

These are stated as consequences of "no encoding path is touched", and that is
the thing a reviewer should check rather than the conclusions.

## 13. Level, and the language-version question

**Level 3 is proposed**, and revision 1's Level 2 is withdrawn.

The argument: this changes an accepted **semantic rule about what may occupy a
capability position**. There is no new syntax and no new type, so it is tempting
to call it a clarification of `SYSTEM_INTERFACE_V1` §5 — but §5's sentence is not
ambiguous. It says the path *is* the type, and this says it need not be. A
conforming implementation that predates this amendment rejects a program this
amendment makes valid, which is a difference in the accepted language surface
rather than in a metric or a document.

**TOS Core version.** The language gains nothing: no constructor, no syntax, no
predeclared name. What changes is which programs are well-typed, and whether
that requires a minor turns on whether `docs/44`'s conformance statement
enumerates what a capability position accepts. **The conservative answer is a
TOS Core minor**, and it is put as a question for the Project Architect rather
than decided here — the two readings differ in what a conformance suite must
assert, not in what a program means.

**Interface-schema version.** `SYSTEM_INTERFACE_V1` gains an exception clause to
§5 and one field in the schema type. **Its version should move**, on the same
rule that moved `PLATFORM_INTERFACE_V1` to 2 and to 3.

## 14. Compatibility proof for existing interfaces and programs

Three claims, each checkable independently:

1. **no accepted entry declares a representation other than `AsInterface`**, so
   §7's predicate is the old predicate on the same inputs for every existing
   interface — structurally checkable over the schema;
2. **no effect identity moves.** ADR-0080 §4 is untouched: `Signature.effects`
   carries interface paths, and no path changes;
3. **no artifact changes**, per §12.

The amendment makes strictly more programs well-typed and rejects none that were
— which is also why it cannot silently break a module that was relying on the
old rejection: a rejection is not a behaviour a program can depend on.

## 15. Threat model

- **no widening of authority.** A value admitted here still faces the runtime
  object-kind check at grant and the nucleus's handle/object/right/generation/
  liveness check per call. Nothing about what a capability *confers* changes;
- **no new forgery surface.** A `DmaRegion` value cannot be constructed — the
  family is nonconstructible in the V1 type surface, exactly as `MmioRegion` is —
  so a program cannot manufacture one to present;
- **the closed enumeration is the control.** An open "admits" relation would let
  a future schema edit widen what occupies a capability position without a
  decision; §4's rules make each widening an ADR;
- **verifier independence** (§7) is what makes a hand-written artifact detectable
  rather than trusted.

## 16. Conformance tests

1. every existing interface resolves and verifies identically, and a corpus
   module's image and digest are unchanged;
2. a capability position of an `AsInterface` interface refuses a `DmaRegion`, a
   `Region`, an `MmioRegionMut` and a scalar;
3. a capability position of `platform.dma.Region` accepts `DmaRegion<mut u8>` and
   `DmaRegion<u8>`, and refuses `Region<mut u8>`, `MmioRegionMut`, a scalar and
   `Capability("platform.dma.Region")` written directly;
4. the four dimensions refuse independently: wrong effect; wrong representation;
   wrong runtime object kind at grant; wrong right at the call;
5. `region[i]` and `dma_device_address(region, …)` over one binding produce
   **one** handle at the ABI and one entry in the caller's capability table;
6. a second use of the same binding after a use is accepted — non-consuming —
   and a use after `capability_release` fails at runtime with
   `E_NO_CAPABILITY`;
7. no representation outside the closed enumeration is accepted anywhere,
   checked structurally so the amendment cannot be read as a general relation.

## 17. Forged-IR negatives

The verifier must reject each of these in an artifact no frontend produced:

1. `Value(operand)` at a `platform.dma.Region` position whose operand type is
   `Capability("platform.dma.Region")` — the representation is the family, and
   the interface's own path is **not** a member of it;
2. `Value(operand)` at an `AsInterface` position whose operand type is
   `DmaRegion`;
3. `Value(operand)` whose operand type is a region or a scalar at any capability
   position;
4. a correct representation with the interface absent from the enclosing
   `Signature.effects`;
5. a type table entry claiming a representation the accepted schema does not
   declare for that interface.

## Architecture impact statement

- **Change level:** 3 (§13). **Invariants affected:** none. No new authority, no
  new object kind, no widening of what any capability confers.
- **Canonical representation:** unchanged.
- **Trusted-base impact:** none. The nucleus never saw a value type.
- **Source-to-runtime impact:** the verifier's capability-position predicate
  changes (§7, §11). No IR variant, no encoding change, existing artifacts
  identical.
- **Compatibility profile:** `SYSTEM_INTERFACE_V1` gains an exception to §5 and
  one schema field; its version should move. A TOS Core minor is **proposed as a
  question**, conservatively answered yes (§13).
- **Bounded-resource impact:** none.
- **New dependencies:** none.
