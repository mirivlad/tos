<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Platform Interface Schema — Version 1

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0079 (Project Architect-approved, 2026-09-03), which fixes the
authority model this schema declares operations over.

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
Its target ABI is `SYSTEM_ABI_V1`, operations 24–26, and nothing else.

## 2. What this version declares, and why so little

Two interfaces. `docs/11_DRIVER_MODEL.md` illustrates four more —
`platform.mmio.RegionMap`, `platform.irq.Binding`, `platform.dma.Allocator` and
a class publisher — and **none of them is declared here.** ADR-0079 §11 leaves
MMIO, interrupts and DMA open, and `SYSTEM_INTERFACE_V1` §4's rule applies to
this schema as much as to that one:

> Nothing speculative: an interface that declared an operation the system does
> not perform would be a contract describing a system that does not exist.

An interface arrives here when its mechanism is decided, not when a document
first shows its name.

| Interface | Object kind |
|---|---|
| `platform.pci.Bus` | pci bus |
| `platform.pci.FunctionConfig` | pci function |

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

Two, and each declares which kind of object a capability of it names, exactly as
`SYSTEM_INTERFACE_V1` §4 does — so a launcher answering a module's request can
refuse a grant of the wrong kind at startup rather than letting the module
discover it at its first call.

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
| `endow_for_launch` | `platform.pci.FunctionConfig` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |
| `capability_attenuate` | `platform.pci.FunctionConfig` with `none` | `rights: u64` | `Result<platform.pci.FunctionConfig, i64>` | 5 |
| `capability_release` | `platform.pci.FunctionConfig` with `none` | *(none)* | `i64` | 6 |

**No operation here takes a BDF.** A configuration access names an offset and a
width; which function it reaches is decided by the capability. So a holder
cannot address a different function — not because it is forbidden to, but
because there is no parameter through which to say so, and a fabricated device
number is a value with nowhere to go.

**`config_read` and `config_write` are separate rights.** A capability carrying
only `config_read` refuses operation 26 with `E_NO_CAPABILITY`. This is the
attenuation a manager performs before handing a function to something that
should only look at it.

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

- **A BAR is data.** Offsets `0x10`–`0x27` return base-address registers. No
  operation of any accepted schema takes one, so a BAR value grants no mapping,
  no physical memory access, and cannot be presented where authority is
  required. Mapping device memory is not in this contract version.
- **Nothing read here is authority.** A vendor identifier, a class code and a
  capability pointer are facts about hardware. Deciding which driver should own
  a function is policy, evaluated by a bus manager, and ADR-0051 deliberately
  leaves it open.

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
| the assignment lives | from a successful claim to the loss of its last name |
| a handle resolves | one process's name for the assignment, with its own handle generation |

**The assignment carries a generation.** Releasing a function and claiming the
same one again produces a new assignment at a new generation, so a handle held
across that gap resolves to nothing rather than to the new occupant — the same
rule `CAPABILITY_V1` §2 states for every other object, applied to the one thing
here that can be released and re-made.

## 5. What this version does not declare

No MMIO interface, no interrupt interface, no DMA interface, no reset operation
and no device-class publisher. Each is open under ADR-0079 §11 and arrives when
its mechanism is decided.

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
8. The interface paths a verified module uses are readable from its IR without
   executing it, and match the `uses` effects of its declared operations.
