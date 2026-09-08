<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0085: Capability representation, separated from interface identity

- Status: **Proposed — not Project Architect-approved. Revision 3 (C′).**
  Nothing is implemented; Stage 4C-2's surface stays stopped
- Date: 2026-09-08
- Decision level: **3** (§13), and it requires **TOS Core 1.3**. Revision 1
  claimed Level 2 and revision 2 left the language minor open; both are settled
  here
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

## 4a. `DmaRegionFamily` is operation-produced and not importable

Revision 2 left a hole the review found: it kept both capability sources for
every interface, while also requiring `Capability("platform.dma.Region")` to be
refused at that interface's own position. Those cannot both stand — an import is
typed `TypeDef::Capability(interface)` and there is nowhere in

```tos
import capability platform.dma.Region as region;
```

for the element type or the mutability of `DmaRegion<T>` to come from. So the
rule is narrowed rather than patched:

```text
representation = AsInterface       Import or Value, under the existing rules
representation = DmaRegionFamily   Value only; Import is invalid
```

**`platform.dma.Region` is therefore operation-produced, not
startup-requestable.** A DMA region is made by operation 30 out of two
capabilities a process was granted; a launcher could not mint one before the
process exists, because the memory has not been charged and the assignment has
not been claimed. The restriction describes what was already true.

**Frontend diagnostic.** An `import capability platform.dma.Region` declaration
is rejected where the import is resolved, with the reason named — the interface
has a representation that no import can produce — rather than as a
capability-not-found. `E1801_FFI_NOT_AVAILABLE`'s neighbourhood is where this
belongs; the exact code is the frontend's to assign against `docs/44`.

**And no general `requestable` mechanism is introduced.** Importability follows
from the representation: `AsInterface` is importable because an import is exactly
that type, and `DmaRegionFamily` is not because no import can be that type. A
separate per-interface flag would be a second thing to keep in step with the
first, and nothing in the accepted contracts needs one.

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

Unchanged in shape. A capability position lowers to
`CapabilitySource::Value(operand)` or `CapabilitySource::Import(index)` exactly as
today; the operand is the SSA value of the argument, and its type is whatever the
type table already says. Nothing about the encoding of a capability position
depends on the operand's type.

**One narrowing** (§4a): at a `DmaRegionFamily` position the frontend emits
`Value` only, because there is no import that could be one. `Import` at such a
position is not a lowering a frontend produces, which is exactly why the verifier
has to refuse it independently — §17.2.

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
- **it does not trust the frontend, and §7a says mechanically why.**

**The predicate's shape changes, and that is worth stating exactly.** Today the
verifier *derives* an interface from the source and then compares:

```text
Value(operand) => match type_of(operand) {
    Capability(interface) => interface,       // then compared to the required one
    _ => reject,
}
```

A `DmaRegion` type names no interface, so derivation has no answer for it. The
predicate therefore inverts: it **checks the source against the required
interface** instead of deriving one from the source. The two forms are
equivalent for `AsInterface`, and §4's uniqueness rule — one family belongs to at
most one interface — is what makes a derive-form available for
`DmaRegionFamily` too, should an implementation prefer it.

## 7a. Where `representation_of` lives

The verifier must not trust the frontend's type-check output, a frontend
callback, or any representation claim a producer put in the artifact. So:

> **The closed representation mapping is a verifier-owned table**, and a
> repository gate pairs it against the canonical interface schema line for line.

This is the narrowest form consistent with the dependency architecture and it is
not a new mechanism: `scripts/tests/check-interface-schema.sh` already pairs the
accepted schema against the frontend's table for operations, object kinds, ABI
assignments and capability requirements, and fails when they disagree. The
representation column joins those. The verifier keeps no dependency on the
frontend, and the two tables cannot drift without a gate going red.

**What the verifier reads per position** is therefore: the required interface
from the accepted operation row, the representation from its own table, and
`operand_type` from the artifact's own type table. All three are independent of
whoever produced the module, which is what makes §17's forged artifacts
detectable rather than believed.

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

## 8a. Release, and the two different things that go stale

The nucleus invalidating a handle settles a later *operation*. It does not settle
a later *indexed access*, because indexed access is served from runtime mapping
metadata — the side table of §9 — and never reaches the nucleus. Both cases must
be closed, and they close differently:

```text
nucleus operation through a stale handle   →  E_NO_CAPABILITY
local indexed access through stale metadata →  deterministic runtime refusal
```

So, normatively:

- **a successful `capability_release` of a `DmaRegion` removes that handle's
  runtime mapping entry before control returns to TOS Core.** The bridge already
  keys base and length by handle and already refuses an access it cannot serve;
  what this adds is that a successful release *retires* the entry rather than
  leaving it behind;
- **a failed `capability_release` leaves the entry intact**, because the object
  did not end and the mapping is still the process's;
- **an indexed access through a released binding refuses before any memory
  access**, deterministically, as a stale or unheld mapping. The existing refusal
  is `RUNTIME_DEVICE_REFUSED`, which already fires before the processor is asked
  to do anything;
- **the page table is not the mechanism.** Relying on a fault from dereferencing
  the old base would be relying on the nucleus having already unmapped the lane
  *and* on a fault being the answer, and neither is a contract. The check is the
  bridge's, before the load.

**None of this makes `capability_release` statically consuming**, and §8's three
categories are unchanged: the binding is still live in the language, and every
use of it after release fails — one at the nucleus, one at the bridge, both
deterministically.

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
`TypeDef::DmaRegion` and `TypeDef::DmaRegionMut` are in `tos-ir/v1` today and are
encoded and decoded by the image format.

**Revision 2 gave the wrong tags, by conflating two tag spaces**, and the
correction is recorded rather than quietly applied. The numbers it quoted — 20
and 22 — are from `tos-core`'s interface-surface table, which is *not* the
TOSIMAGE encoding. The encoding is:

| TypeDef | TOSIMAGE tag |
|---|---|
| `Region` | 19 |
| `DmaRegion` | 20 |
| `Capability` | 29 |
| `RegionMut` | 34 |
| `DmaRegionMut` | **35** |
| `MmioRegion` | 36 |
| `MmioRegionMut` | 37 |

`DmaRegionMut` is 35 and not 22. The conclusion is unchanged — every tag this
amendment needs already exists — and it is now stated from the encoder and parser
rather than from the wrong table. The digest and identity claims of §12 rest on
"no encoding path is touched", which this correction does not disturb: no tag is
added, removed or renumbered. A capability
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

**TOS Core version: 1.3, and it is settled here rather than left open.** The
language gains no syntax and no type constructor, and that is not the test. This
changes a normative *acceptance* rule: a capability position a conforming pre-C′
frontend and verifier reject becomes valid. An implementation that does not
support it must not accept such a module, and one that does must not apply the
rule to a module that did not ask for it — which is exactly what a language minor
is for.

ADR-0081 established **1.2** for the device-memory features, so this amendment is
**1.3**, with the ordinary gating:

- a `1.0`, `1.1` or `1.2` module **does not** receive C′ semantics, whatever
  frontend compiles it. Running on a 1.3-capable frontend changes nothing about
  what its header declares;
- a `1.3` module may use the capability-representation rule;
- an implementation that does not support 1.3 rejects the module **whole, by its
  header**, rather than part-way through a function;
- the language minor stays part of artifact, cache and provenance identity, so a
  1.2 and a 1.3 module are different artifacts even where their source text
  matches.

**And documentation drift is repaired while the version contract is open.**
`docs/42` still enumerates "`TOS Core 1.0` and `TOS Core 1.1` (ADR-0080)" despite
ADR-0081 having accepted 1.2 and the implementation supporting it, and `docs/44`
carries the same gap. History is not rewritten: 1.2 is documented as ADR-0081's
extension, at the date it was accepted, and 1.3 as this ADR's — so the sequence
reads as what happened rather than as a list corrected after the fact.

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
3. `CapabilitySource::Value(DmaRegion<T>)` and `Value(DmaRegion<mut T>)` are
   **accepted** at a `platform.dma.Region` position, and `Region<mut u8>`,
   `MmioRegionMut`, a scalar and `Capability("platform.dma.Region")` written
   directly are refused;
4. **`import capability platform.dma.Region` is rejected in source** (§4a), with
   the reason named rather than as a missing capability;
5. the four dimensions refuse independently: wrong effect; wrong representation;
   wrong runtime object kind at grant; wrong right at the call;
6. `region[i]` and `dma_device_address(region, …)` over one binding produce
   **one** handle at the ABI and one entry in the caller's capability table;
7. a second use of the same binding after a use is accepted — non-consuming;
8. **after a successful release, the two stale paths refuse differently and both
   deterministically**: `dma_device_address` answers `E_NO_CAPABILITY` at the
   nucleus, and `region[index]` refuses at the bridge **before any memory
   access** (§8a);
9. **a failed release preserves the mapping**, and indexed access through the
   binding still works afterwards;
10. **version gating**: an existing `1.0`, `1.1` or `1.2` artifact keeps its old
    semantics and its exact image bytes and digest; and a `1.2` module compiled
    by a 1.3-capable frontend is **still refused** the C′ rule — it does not
    acquire the semantics by the toolchain it met;
11. an implementation without 1.3 rejects a 1.3 module by its header, whole;
12. no representation outside the closed enumeration is accepted anywhere,
    checked structurally so the amendment cannot be read as a general relation.

## 17. Forged-IR negatives

The verifier must reject each of these in an artifact no frontend produced:

1. `Value(operand)` at a `platform.dma.Region` position whose operand type is
   `Capability("platform.dma.Region")` — the representation is the family, and
   the interface's own path is **not** a member of it;
2. **`Import(index)` at a `DmaRegionFamily` position**, whatever the import's
   declared interface (§4a). No frontend emits it, which is precisely why the
   verifier must refuse it on its own;
3. `Value(operand)` at an `AsInterface` position whose operand type is
   `DmaRegion` or `DmaRegionMut`;
4. `Value(operand)` whose operand type is a region or a scalar at any capability
   position;
5. a correct representation with the interface absent from the enclosing
   `Signature.effects`;
6. a module whose header declares `1.2` and whose body uses a `DmaRegionFamily`
   capability position — the version gate is a verifier obligation and not only a
   frontend one.

## Architecture impact statement

Stated as the narrower facts rather than as two "none"s, because one of them
would have been false: the nucleus is untouched and the **verifier's trusted
logic is not**.

- **Change level:** 3 (§13). **TOS Core 1.3.**
- **Tier-0 and nucleus authority invariants:** unchanged. Non-forgeability,
  ambient-authority absence and the capability model itself are untouched.
- **Nucleus mechanism and ABI:** unchanged. No operation number, no object kind,
  no right, and no raw handle exposed anywhere new. The nucleus never saw a value
  type and still does not.
- **Interface-path ↔ capability-representation relation:** **changed.** This is
  the amendment: an accepted semantic rule of `SYSTEM_INTERFACE_V1` §5 and
  ADR-0080 §7 now has one closed exception.
- **Verifier trusted predicate:** **changed.** The capability-position check
  becomes representation-aware (§7), reads a verifier-owned closed table (§7a),
  and refuses `Import` at a `DmaRegionFamily` position (§4a). An older verifier
  rejects a valid 1.3 artifact, which is the conformance difference the language
  minor gates.
- **Runtime bridge:** **changed**, narrowly: a successful release retires the
  mapping entry for that handle (§8a).
- **Canonical representation and source-to-runtime identity:** unchanged. No IR
  variant, no TOSIMAGE tag added, removed or renumbered; existing artifacts are
  byte-identical and keep their digests.
- **Threat model:** §15. No widening of authority and no new forgery surface; the
  closed enumeration is the control that keeps a future widening an ADR.
- **Bounded-resource impact:** none. **New dependencies:** none.
