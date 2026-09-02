<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0078: Every capability position says where its capability came from

- Status: **Accepted**
- Date: 2026-09-03
- Decision level: 2 — it repairs the `tos-ir/v1` representation of one
  instruction and bumps the TOSIMAGE storage encoding version. It changes **no**
  TOS Core V1 semantics, no language version, no type constructor, no ABI
  operation and no accepted ceiling
- Project Architect approval: **given, 2026-09-03**, as a narrow consistency
  correction, with the mechanism fixed (`CapabilitySource::Import(index) |
  CapabilitySource::Value(value)`), the two alternatives of
  `STAGE3_LAUNCH_PLANS.md` §6 explicitly re-rejected, and the requirement that
  it be general for **all** capability positions rather than the first
- Related: ADR-0056 (Accepted) — the capability is the first argument. ADR-0060
  (Accepted) — the interface schema. ADR-0061 (Accepted) — how an endowment
  binds to a module. ADR-0063 (Accepted) — an operation requiring two
  capabilities. ADR-0070 (Accepted) — the versioned storage encoding this bumps.
  ADR-0077 (Accepted) — launch plans, whose evidence exposed this.
  `docs/43` §3, `SYSTEM_INTERFACE_V1` §4.1

## 1. A representation narrower than the semantics it represents

`Op::Capability` named the capabilities of an interface operation as import
indices: one `import`, then a list of `further_imports`, each an index into the
module's `capability_imports`. Nothing else could fill a capability position.

That was written when an import was the only way a module could come to hold
authority, and it was true then. It stopped being true when operations began to
*return* capabilities — a refined one from operation 5, a scoped budget from 16,
a region from 17, a child from 19, a launch plan from 21. Each of those is a
capability a module holds and none of them answers an `import capability`
request, because none of them existed when the module started.

So an operation acting on one could not be written down. Concretely, and these
are the cases that surfaced it:

- `process_terminate` on a **child**, whose authority operation 19 produced;
- `capability_release` and `capability_attenuate` on anything obtained at
  runtime;
- `endow_for_launch` through a **scoped** `MemoryAuthority` from operation 16.

**The mismatch was in the representation, not in the language.** TOS Core V1
already has a capability type constructor; `tos-ir/v1` already has
`TypeDef::Capability`; the image format already encodes it; the engine's
boundary already returns a value rather than an integer. A capability value was
a thing V1 admitted and `Op::Capability` could not consume. That is a
representation below its own accepted semantics, and this ADR is the repair of
the representation.

## 2. The decision: an explicit source per position

`Op::Capability` carries, per capability position, one of:

```text
CapabilitySource::Import(index)   the capability answering one of the module's
                                  `import capability` requests (ADR-0061)
CapabilitySource::Value(operand)  a capability the module holds as a value,
                                  because an operation produced it
```

**An explicit discriminator, never a sentinel.** There is no reserved index
meaning "not an import": a reader that had to know one number was special is a
reader that can mistake a real index for it. The two cases are two cases.

**General over every position.** The first capability of an operation is not a
special case of this rule, and neither is the second. A repair that had applied
only to the operation's own capability would have moved the same contradiction
one position along — the second capability of operation 19 is a memory
authority, and a supervisor's is scoped at runtime.

**Existing operations are unchanged.** An import-supplied position is the
explicit `Import` case and means exactly what it meant: the same index, the same
table, the same request. Nothing about the import path's semantics moved.

## 3. What is not in it, and why each was rejected

The `STAGE3_LAUNCH_PLANS.md` §6 alternatives stay rejected, and are recorded
here so that they are rejected somewhere permanent:

**Reinterpreting `import` as "the authority that licensed the reach"** while an
operand names the object acted upon. It needs no shape change, and that is
precisely what makes it dangerous: it silently changes the meaning of an
existing encoded field, so an older verifier reading a newer artifact would
misread which capability an operation acts on and report that it had checked it.

**A second, import-anchored capability the ABI does not use**, declared only to
license reach. It puts an authority in a schema that the system never checks,
which is a contract describing a system that does not exist — and it makes
runtime-obtained authority depend on an otherwise unused startup import.

Also not in it: no `AnyCapability`, no raw or scalar handle in the IR or in TOS
Core, no implicit derivation of one authority from another, and no `tos-ir/v2`.
The schema version stays `1` because the semantics did not change; what changed
is that they became representable.

## 4. What a verifier proves

Per capability position, against the artifact and never on the frontend's word:

- an `Import` names an index inside the module's own capability import table;
- a `Value` names an operand whose type is `TypeDef::Capability` — a scalar, a
  constant, or a value of a nominal record type is refused here, which is
  `docs/43`'s "no construction from scalar data" made checkable;
- the interface at position zero is the accepted interface ID the instruction
  carries, so an artifact naming one interface and acting through another is
  refused as it always was;
- the interface at **every** position is one the enclosing function declares as
  an effect, which is `docs/42` §2's "enclosing `uses` effect" requirement
  applied per position — and, for a runtime-sourced capability, the exact
  nominal interface check that has no import declaration to compare against;
- and no source appears twice. ADR-0063's rule held of imports and now holds of
  sources: one grant may not stand in for two, whichever kind each is.

The interface and the required right still come from the accepted interface
schema. The source says only *which thing* fills a position that schema already
described. Runtime rights and object validity remain the nucleus's, unchanged:
`CAPABILITY_V1` §3 still bounds a capability's lifetime by its object, and a
handle to something that has ended still resolves to nothing.

## 5. The storage encoding

`TOSIMAGE` encoding version **4**. The bytes of one instruction changed, so the
version changed: an older reader given a version-4 image must refuse it rather
than read a source tag as an index, which is what ADR-0070's fail-closed
unknown-version rule is for.

A version-**3** image is still readable, and every capability position of one
decodes as `Import`. That is bounded and canonical rather than a guess: an
import index was the only source version 3 could write, so the mapping is one
for one with nothing invented. No existing encoded field changed meaning; a
field was replaced by a tagged one under a new version.

The schema version stays `1`. The module digest changes for any module that
encodes a capability operation, because the source is part of the canonical
stream — deliberately, since an artifact acting on a runtime value must not
share an identity with one acting on an import.

## 6. What this does not decide

**It does not admit an interface a module never requested.** The verifier still
requires the interface an operation reaches to be one the module imported and
the enclosing function declared. Every case this ADR unblocks satisfies that
already, and genuinely rather than by contrivance: a module obtains a child
authority by *calling* operation 19 through its own process import, and a scoped
budget by calling 16 through its own authority import. The import is used, not
manufactured. A capability of an interface a module never imports — one
delivered by a message, say — is a separate question and is not answered here.

**It does not change what a capability is at runtime.** The nucleus checks
rights, object kind and object liveness exactly as before. A module can hold
only capabilities the nucleus gave it, and the source discriminator says where a
module got one, not what it may do with it.

## 7. Conformance evidence

`source/tests/integration/tests/capability_source.rs` and
`source/host-tools/qemu-test/runtime-authority.sh`:

1. operation 19 returns a child `system.process.Control`, typed as that exact
   nominal capability in the artifact's type table;
2. that runtime capability is the **own** capability of `process_terminate`;
3. lower → TOSIMAGE → decode → independent verifier → engine → **ABI**, with the
   verifier run over the decoded artifact and the last step against the real
   nucleus in QEMU;
4. operation 16 returns a scoped `system.memory.Authority`;
5. `endow_for_launch` acts *through* that scoped authority, and the interface
   recorded on the instruction is the value's own;
6. attenuation and release of runtime-obtained authority are representable and
   are performed;
7. a forged scalar in a capability position, and a capability of the wrong
   interface, are both refused by the verifier;
8. the **second** capability position of operation 19 is runtime-sourced, so the
   hole is not left one position along;
9. an unknown container version is refused, and version 3 reads as `Import`;
10. every existing import-only gate is unchanged and green.
