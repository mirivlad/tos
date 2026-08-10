<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0030: External vendor-controlled opaque material and the `/vendor` namespace

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 3 — introduces a root namespace, a trust boundary and a
  declared dependency direction between canonical `/system` source and external
  material that TOS does not control
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

TOS states that human-readable source text is the canonical installed form of
its non-nucleus executable components. On real hardware this claim meets
material that TOS cannot make textual and cannot rewrite:

- Intel and AMD CPU microcode updates;
- GPU firmware images;
- Wi-Fi, Bluetooth, NIC, storage-controller and embedded-controller firmware;
- device option ROMs and platform firmware payloads.

These are produced, signed and versioned by hardware vendors. They are loaded
by, or on behalf of, the machine, and their internal content is not source in
any sense the owner can act on.

Existing documentation handles this only by omission or by vague phrasing.
`docs/00_PROJECT_CHARTER.md` spoke of "every non-firmware component" without
defining what firmware is architecturally. `docs/17_REPOSITORY_LAYOUT.md` said
"firmware blobs, if supported, are separate and explicitly licensed" without
naming where they live or how `/system` may depend on them.

Two failure modes follow from leaving this undefined. TOS could imply that a
conforming machine contains no opaque binary material, which is false on every
current platform and would make the project dishonest under I-15. Or opaque
vendor material could quietly accumulate inside the canonical textual system
tree, which would erase I-01 component by component while every individual step
looked pragmatic.

The honest position is a stated boundary rather than either denial or drift.

## Decision

### 1. Ownership scope

TOS owns the TOS software layer. TOS does not claim ownership, authorship or
control of vendor-produced material executed by CPUs and peripheral devices.

### 2. Vendor-controlled opaque material

A unit of external material is **vendor-controlled opaque material** when all of
the following hold:

- it is produced and versioned outside the TOS project;
- it is consumed as bytes by hardware or by a hardware-facing loading path;
- TOS cannot express it as canonical source text that the owner may edit,
  rebuild and run;
- it is not the definition of any TOS component.

Vendor-controlled opaque material **MUST NOT** be presented as canonical TOS
source. The system **MUST NOT** display, describe or record it as open,
readable or modifiable material. TOS **MUST NOT** claim to have inspected,
verified or understood its internal behavior. It is identified, located,
version-pinned and hashed; it is not interpreted.

### 3. `/vendor` namespace

External material lives in a dedicated root namespace:

```text
/vendor/
    firmware/
        intel/
        amd/
        nvidia/
        ...
```

`/vendor` is not part of the canonical `/system` tree and **MUST NOT** be
merged into, mounted inside or presented as part of it. Firmware is one class
inside `/vendor`; a separate root `/firmware` namespace is therefore not
introduced.

`/vendor` is its own namespace class, distinct from canonical source, mutable
state and derived cache. It is not derived — deleting it does not regenerate it
from canonical source — and it is not canonical TOS source.

### 4. Declared dependency direction

`/system` **MAY** declare that it requires a vendor object. The declaration is
canonical source text in `/system` and states at least:

- vendor and object identity;
- version;
- content hash;
- expected placement under `/vendor`;
- compatibility constraints;
- policy for absence, mismatch and refusal.

The opaque bytes themselves **MUST** reside under `/vendor`. A declaration is a
reference, never an embedded payload. Dependency flows in one direction only:
canonical source may name external material; external material never names,
selects or alters canonical source.

A TOS component **MUST** behave in a defined way when a declared vendor object
is absent, has a mismatched hash, or is refused by policy. Silent degradation is
not a defined behavior.

### 5. No opaque substitution of textual components

A component that TOS architecture requires to be textual **MUST NOT** be
replaced, shadowed or superseded by vendor-controlled opaque material. A
user-space driver written in TOS Core remains canonical readable source that the
owner can inspect and modify, including when that driver's runtime job is to
hand a firmware image to a device.

Loading vendor firmware is an action performed by a textual TOS component. It is
not a substitute for one.

### 6. Visible boundary

The owner **MUST** be able to determine, for the running system, which
components are canonical TOS source and which are external opaque vendor
material. For each vendor object the system reports vendor, object identity,
version, content hash, provenance record, licence or redistribution status, and
current status (required, present, absent, mismatched, refused).

This report is an ordinary owner-facing system capability, not a debugging
facility. A machine that cannot answer the question does not satisfy this
decision.

### 7. Licence and redistribution

Vendor-controlled opaque material carries its own licence and redistribution
terms and **MUST NOT** be treated as covered by TOS project licences. Its
presence in a TOS installation does not make it a TOS component, and its terms
do not extend to any TOS component. Redistribution requires the review already
required by `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md` and
`docs/27_THIRD_PARTY_COMPONENT_POLICY.md`.

### 8. Scope of this decision

This ADR defines an architectural model. It authorizes no implementation, no
loading path, no storage format and no firmware redistribution. `/vendor` has no
required implementation before the stage that first needs physical-hardware
firmware. Concrete declaration schema, storage format, verification path and
loading mechanism require their own versioned contracts under I-09.

## Relationship to system invariants

This decision does not amend `docs/02_SYSTEM_INVARIANTS.md` and requires no
Level 4 identity amendment.

I-01 governs "every non-nucleus executable component" — that is, every component
*of TOS*. Vendor-controlled opaque material is by this decision's definition not
a TOS component, was never canonical TOS source, and does not become so by being
present on the machine. This ADR states an existing scope boundary explicitly
instead of leaving it to be inferred.

The decision strengthens rather than weakens I-01 in practice: without a named
boundary, opaque material has no defined place and tends to accumulate inside
the canonical tree. Section 5 makes that specific failure a stated violation.

Related invariants:

- **I-15 honest compatibility** — TOS states plainly that opaque vendor material
  exists on real hardware rather than implying a fully textual machine;
- **I-16 source-to-runtime traceability** — traceability continues to apply to
  TOS components; vendor objects are identified, not traced to source;
- **I-17 owner-installable modification** — unaffected, because the textual
  components the owner modifies remain textual under section 5;
- **I-19 external dependency containment** — extended with a class that is
  contained by placement and declaration rather than by review-for-admission,
  since it cannot be reviewed as source;
- **I-20 legal continuity of openness** — section 7 prevents vendor terms from
  bleeding into TOS components.

## Architecture impact statement

- **Change level:** 3.
- **Invariants affected:** none amended; I-01, I-15, I-16, I-17, I-19 and I-20
  are scoped explicitly as described above.
- **Canonical representation after the change:** unchanged. `/system` remains
  canonical text. `/vendor` is explicitly not canonical TOS source.
- **Trusted-base impact:** no dependency enters the loader or nucleus. A new
  trust boundary is named: canonical source to external opaque material.
- **Source-to-runtime impact:** the identity plane gains a second, weaker
  answer class — vendor objects are reported by identity/version/hash, never by
  source path and never as verified behavior.
- **Recovery and rollback impact:** `/vendor` is not part of the system commit,
  so rollback of `/system` does not roll back vendor material. Declarations in
  `/system` carry version and hash, so a rolled-back commit states which vendor
  objects it expects. Absence must be a defined, recoverable state.
- **Stage identity gate:** no stage gate is claimed or closed. The model applies
  from the first stage that touches physical-hardware firmware.
- **Threat-model impact:** TOS does not claim confidentiality or integrity
  against malicious firmware — an existing accepted non-goal in
  `docs/34_THREAT_MODEL.md`. This decision adds the boundary and requires that
  the owner can see it, which is a reporting requirement, not a protection claim.
- **Performance contract:** none applicable; no measured path changes.
- **Compatibility profile:** none claimed. No hardware support is asserted.
- **New dependencies:** none. The decision is documentary.
- **Licence and patent impact:** section 7 keeps vendor terms separate. No
  material is imported by this decision.
- **Tests that enforce the decision:** deferred to the implementing stage. When
  `/vendor` is implemented, architecture conformance tests under
  `docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md` must enforce that no vendor object
  is reachable as `/system` content, that a declared-and-absent object produces a
  defined failure, and that the owner-facing boundary report is complete.

## Consequences

TOS gains a truthful statement about real machines: the TOS layer is textual and
owner-controlled, and material outside that layer is named as external rather
than hidden or denied. A future bare-metal stage can support CPU microcode and
device firmware without either violating I-01 or pretending the material is
open.

The cost is that a TOS machine on real hardware is not fully inspectable by the
owner, and this decision requires TOS to say so rather than obscure it. The
boundary is visible precisely so that its size can be observed and argued about.

## Alternatives considered

**Prohibit all opaque material.** Rejected: it makes TOS unimplementable on
current hardware and would either stop the project at emulation or be quietly
violated later, which is worse than a stated boundary.

**Treat firmware as ordinary third-party components under docs/27.** Rejected:
that policy is built around material TOS can read, evaluate and admit by review.
Opaque blobs cannot be reviewed as source, so applying the same process would
produce approvals with no evidentiary content.

**Place firmware under `/system/firmware`.** Rejected: it puts non-source bytes
inside the canonical source tree, which is the exact drift section 5 forbids.

**A separate root `/firmware`.** Rejected: firmware is one class of external
vendor material. Microcode, option ROMs and future non-firmware vendor material
belong to the same boundary, and a firmware-specific root would need siblings
later.
