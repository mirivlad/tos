<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0079: Where hardware authority comes from, and how a textual process reaches PCI configuration space

- Status: **Accepted (Project Architect-approved)**
- Date: 2026-09-03
- Decision level: **3** — it moves a trust boundary. It admits a root of
  authority that is not memory, not a process and not an endpoint and is derived
  from none of them, and it fixes the general rule under which the nucleus may
  expose a hardware mechanism primitive
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-03
- **What the approval covers**: the reconciled decision in §5–§10 below. The
  alternatives in §5 are retained as history because a decision is not readable
  without the options it was taken against; **they are not approved**, and none
  of them is revived by being written down here.
- Related: ADR-0003 (user-space drivers), ADR-0037 §2 (the DMA region facts),
  ADR-0048 §2 (mechanism, not service policy), ADR-0049 (the interrupt
  baseline, and what it deliberately withheld), ADR-0051 §"What this
  deliberately leaves open" (device matching), ADR-0055 (where a first
  capability comes from), ADR-0060 (a schema is a class of document), ADR-0075
  §2, §5, §6 (region origins, the origin rule, the `RegionObject`), ADR-0076
  (one physical account), ADR-0077 (launch plans), ADR-0078 (capability
  sources). `CAPABILITY_V1` §2–§4, `SYSTEM_ABI_V1` §2, §5, §6,
  `SYSTEM_INTERFACE_V1` §2, §4, `PLATFORM_INTERFACE_V1`, `IPC_V1` §5,
  `BOOT_ABI_V1`, `docs/11`, `docs/37` §Stage 4

## 1. The question, stated once

Stage 4's identity gate asks whether "a canonical textual user-space driver
actually move[s] persistent data through final-style MMIO/interrupt/DMA/IPC
boundaries" (`docs/37` §Stage 4). Every step of that rests on something that did
not exist and could not be derived from anything that did: **a root of hardware
authority**.

Stage 4A does not need a block driver, a queue, a BAR, an interrupt or a DMA
buffer. It needs one textual process to read one real device's configuration
space under a capability. That is the narrowest hardware-facing act there is,
and it was unreachable.

## 2. The smallest unreachable operation

> A canonical textual TOS Core process reads the 32-bit dword at offset `0x00`
> of the configuration space of the PCI function QEMU created for
> `virtio-blk-pci`, and receives `0x1042_1AF4` — the device and vendor IDs the
> real device reports — without holding authority over any other function.

Nothing smaller than this is a hardware-facing act at all.

## 3. Why no accepted mechanism reached it

Four doors, each closed by a clause rather than by an implementation gap. This
section is the STOP that produced this decision, and it is retained because the
decision below is only justified by it.

### 3a. There was no origin for a capability naming a device

`CAPABILITY_V1` §2, as amended by ADR-0075 §5:

> A capability an operation returns must have an explicitly defined normative
> origin: either authority the caller presented to that operation, which bounds
> what is produced, or an explicitly accepted bounded self-only creation rule.
> No operation creates authority over a pre-existing external object out of
> nothing.

A PCI function is a **pre-existing external object**. It could not come from
attenuating what a process holds: ADR-0055 terminates the chain at the boot
process, whose endowment is the launcher's stated constant. And it was not
ADR-0055's Option B self-only creation, whose justification — creating something
nobody else can reach confers no authority over anyone — is false of a device
that exists whether or not anybody names it.

**§9 closes this by admitting a third origin class.**

### 3b. Configuration space cannot be read from CPL 3

`exception.rs` keeps one TSS for the machine with `io_map_base` pointing past
the segment limit, and IOPL stays 0. The one exception,
`IoBitmap::ALLOWING_COM1` under `feature = "test-measurement-port"`, proves the
constraint rather than relieving it: with one TSS a cleared bit is a property of
the machine, not of a process. And the CAM address/data pair is a single global
window, so exposing it would be arbitrary access to every function's
configuration space.

No accepted operation maps a caller-named physical range either.
`SYSTEM_ABI_V1` operation 17 states the opposite — "The nucleus chooses the
address; a caller never supplies one" — and ADR-0075 §6 makes a region's
placement private.

**§6's mechanism operations close this without exposing either.**

### 3c. A device region is not a region ADR-0075 or ADR-0076 describes

| Accepted clause | What a device window would have needed |
|---|---|
| ADR-0076: one physical account, every region charged to a `MemoryAuthority` | MMIO frames are not pool bytes; nothing to charge, nothing to reclaim |
| ADR-0075 §2 B: allocation "reserve[s] unique backing pages" out of the pool | the pages already exist and belong to a device |
| ADR-0075 §6: `charged to — the MemoryAuthority the allocation spent from` | no authority funded it |
| operation 17: the nucleus chooses the address | the address *is* the identity of the thing |
| nothing in the corpus | device memory must be mapped uncacheable on x86_64 |

**This is not closed by this ADR and is deliberately left to Stage 4B.** No
device-memory region is introduced here, and no configuration access is a
mapping.

### 3d. No accepted schema declared a platform interface

`docs/11` §"Driver manifest" shows accepted V1 *form*, and that is exactly why it
must not be mistaken for an accepted interface:

```tos
import capability platform.pci.FunctionConfig as pci;
import capability platform.mmio.RegionMap as mmio;
import capability platform.irq.Binding as irq;
import capability platform.dma.Allocator as dma;
```

None of those four paths was declared by any schema, and `types.rs` resolves an
interface path only against the accepted table. The form parses; the paths were
not types. ADR-0051 §4 already corrected the previous version of this same
passage for the same class of reason.

**§8 declares the two that now exist, and no others.** `docs/11` is corrected so
the remaining illustrative names cannot be read as accepted.

## 4. What Stage 4 still leaves open

| Question | State after this ADR |
|---|---|
| PCI enumeration ownership | **decided** — §5 |
| configuration-space access mechanism | **decided** — §6 |
| platform facts / ECAM discovery | **decided for this stage** — §7 |
| function capability shape and rights | **decided** — §10 |
| who may hold bus authority | **decided** — §5, and it is not a nucleus rule |
| device matching | **open**, ADR-0051: a query a bus manager evaluates, not an authority a launcher grants |
| BAR → MMIO mapping | **open — Stage 4B.** A BAR value is data here |
| device-memory region semantics | **open — Stage 4B** |
| interrupt binding and acknowledgement | **open**, constrained by `docs/11` §Interrupts and ADR-0049 |
| DMA authority | **open**, constrained by ADR-0037 §2 and `docs/11` §DMA |
| IOMMU semantics | **open** |
| reset authority | **open**, and no right is allocated for it |
| VirtIO negotiation, queues, block I/O | **open** |
| publication of `block.device.v1` | **open**; its shape needs nothing new (ADR-0051 §2, `CAPABILITY_V1` §6) |

## 5. D1 — the root of PCI authority, and who holds it

**Accepted: a launcher-minted root PCI Bus authority.**

A pre-existing hardware platform needs an explicit root of authority. It cannot
be derived from memory, process or endpoint authority, and it cannot be created
by an ordinary capability-returning operation. The root is:

- a boot/platform fact, minted at the launch boundary and nowhere else;
- explicitly scoped;
- named, with its scope, in the launch and audit record;
- unacquirable ambiently, and unmanufacturable by any process operation;
- the traceable ancestor of every later PCI authority.

This is analogous to the root `MemoryAuthority` of ADR-0075 §2b **in authority
provenance and not in resource accounting**: a bus is not a quantity, nothing is
reserved out of it, and no accounting node exists.

### The root is held by the boot supervisor, not by the PCI service

```text
boot/platform
    ↓  minted once, named in the launch record
named root Bus authority
    ↓  retained
canonical textual boot supervisor / launch-policy process
    ↓  delegated under /system/policy/, scoped
derived Bus authority
    ↓
canonical textual PCI bus service
    ↓  claimed per function
PciFunction authority
    ↓
eventual textual device driver
```

The supervisor **retains** the root and delegates a scoped Bus capability to the
PCI service. If the PCI service crashes the root does not go with it: the
supervisor revokes or re-derives the service's authority and restarts it through
the ordinary Stage 3 lifecycle.

**An ordinary device driver receives a `PciFunction`, never the root Bus
authority.**

### There is no nucleus rule naming the PCI service

Explicitly rejected: a ring-0 rule of the form "only the PCI service may hold
Bus authority". That would encode service policy in the nucleus, against
ADR-0048 §2, and would put a module name in a place that has no business knowing
one. The restriction is expressed by **explicit capability flow, textual launch
policy, source identity and audit evidence** — and by nothing else.

A supervisor that deliberately delegates its explicit root authority wrongly is
a compromise of the authority-granting policy principal. It is not ambient
authority, and the nucleus must not carry a hard-coded module-name policy to
compensate for it. Stage 3 already fixed the distinction this preserves:
**mechanism verifies that a grant is lawful; textual policy decides that the
grant should happen.**

### Rejected alternatives, retained as history and not approved

**B — the nucleus enumerates and endows each driver.** It makes the nucleus the
PCI policy engine and the matcher, against ADR-0048 §2 and ADR-0055, and fails
the Stage 4 identity gate even if every read works.

**C — a self-only creation rule for device authority.** Refused by §3a: a device
is not an object only its creator can name.

## 6. D2 — configuration access is a nucleus mechanism primitive

**Accepted: mechanism operations over a `PciFunction` capability.**

Configuration-space access is a privileged hardware mechanism the nucleus must
mediate, because CPL 3 cannot perform it safely under the accepted isolation
boundary. **This does not move PCI, VirtIO or device policy into the nucleus.**
The nucleus knows how to perform a privileged configuration transaction; it does
not know which device is a VirtIO block device or what should drive it.

### The general rule, amending `SYSTEM_ABI_V1` §2

This is not a PCI exception. It is the Stage 4 interpretation of the existing
"nucleus provides mechanism, no service policy" boundary, and it is stated
generally so that the next device class is judged by a rule rather than by this
precedent:

> Device discovery policy, matching, class behaviour and device services remain
> textual user-space services reached through IPC. The nucleus **MAY** expose a
> narrowly capability-gated hardware mechanism primitive when the operation:
>
> 1. cannot safely be performed directly at CPL 3 under the accepted isolation
>    boundary;
> 2. operates only on an exact object and scope already named by a capability
>    the caller holds;
> 3. does not choose devices, drivers, matching policy or service behaviour;
> 4. does not grant authority the caller did not already obtain through a
>    normative origin;
> 5. is the minimum mechanism required for a textual service to perform the real
>    work.

`SYSTEM_ABI_V1` §2 is amended to carry this rule. The amendment is explicit and
is made in that document; this ADR does not silently override its old wording.

### Rejected alternatives, retained as history and not approved

**R — a device-memory region over the function's ECAM page.** It amends ADR-0075
and ADR-0076 rather than fitting them, and it moves two properties out of the
capability model into page permissions: a write to a BAR register or a reset bit
would be a store the nucleus never sees. **H**, the hybrid, splits the negative
proofs across two mechanisms.

## 7. D3 — the configuration backend for the Stage 4 reference platform

**Accepted: PCI Configuration Mechanism #1 (`0xCF8`/`0xCFC`), inside ring 0.**

The ports are completely inaccessible from CPL 3. The address/data pair is owned
and serialised by the nucleus-side mechanism. **No IOPL, no process-specific TSS
I/O bitmap, and no exposure of either port to a process.**

**This is an implementation backend, not the public authority model**, and the
distinction is load-bearing:

- the public contract names a **function** and an **offset**, never a port, a
  mechanism or a physical address;
- the `PciFunction` capability model does not change when the backend changes
  from CAM to ECAM;
- CAM's physical mechanism is not part of `PLATFORM_INTERFACE_V1` and never
  appears in a textual interface.

**Scope: conventional configuration space, the first 256 bytes.** That is what
CAM reaches and it is what Stage 4A needs — the VIRTIO PCI capability list is
reached through the capability pointer at `0x34` and lives in standard config
space. The contract declares that bound honestly rather than promising a space
the backend cannot serve.

Later PCIe extended configuration space may version the platform interface and
replace the backend with ECAM obtained through a boot/ACPI platform contract.
**No ACPI MCFG parser and no `BOOT_ABI_V1` version change are required in Stage
4A**, and P3 is not permission to build a throwaway public architecture around
legacy I/O ports.

## 8. `PLATFORM_INTERFACE_V1`

A separately versioned Stage 4 platform schema, accepted by this ADR as
`SYSTEM_INTERFACE_V1` §2 anticipated — "A Stage 4 driver interface is another
instance of these rules, not a special case of this document."

**It declares only what exists**: `platform.pci.Bus` and
`platform.pci.FunctionConfig`, with their operations and rights. It declares no
MMIO, IRQ or DMA interface. Declaring one because `docs/11` shows an
illustrative name would be a contract describing a system that does not exist,
which is the fault `SYSTEM_INTERFACE_V1` §4 names in its own terms.

`docs/11` is corrected so that its remaining illustrative interface paths cannot
be mistaken for accepted ones. Future platform interfaces are added when their
mechanisms are actually decided.

## 9. The platform root, as an origin class

`CAPABILITY_V1` §2 is amended to recognise platform roots explicitly. The
existing rule is unchanged:

> an ordinary operation cannot manufacture authority over a pre-existing
> external object.

Stage 4 adds a third legitimate root class beside it:

> Authority over a **pre-existing platform resource** may originate only at the
> boot/platform boundary, under an accepted platform contract, with explicit
> finite scope and identity and an attributable launch and audit record.

**This is not an exception permitting arbitrary hardware capabilities to be
minted later.** No ordinary process operation creates a root Bus authority, and
the only Stage 4A instance of this class is the first PCI Bus authority.

## 10. D4 — what a `PciFunction` is

Nucleus-owned state, unreachable except through a capability:

```text
segment
bus
device
function
generation / assignment epoch
```

**None of these is encoded into a forgeable capability handle.** The BDF lives in
the object, so it is part of the authority rather than a caller-supplied address.

| | |
|---|---|
| scope | exactly one PCI function |
| rights | `config_read`, `config_write`, separate — a holder of `config_read` alone cannot mutate configuration |
| reset | **no right allocated.** A right with no operation would be a contract describing a system that does not exist |
| lifetime | bounded by the Bus authority it was claimed through and by its holders |
| staleness | by the assignment generation |

### A BAR value is data, not authority

A BAR read from configuration space is a number the device reported. It does not
grant a mapping, does not grant physical memory access, and cannot be presented
where an MMIO capability is required — there is no operation that accepts one.
**BAR → MMIO is a Stage 4B decision** and this ADR does not prejudge it.

### Assignment is exclusive, and three lifetimes stay separate

**At most one live `PciFunction` assignment object exists for a BDF under one
root.** A second claim of an already-assigned function is refused. Several
capabilities may name that one assignment object — attenuation produces another
name, which is what later management/driver separation will need — so the object
is not affine, and its assignment being exclusive is a property of the claim
rather than of the capability model.

Three facts that are not the same fact:

| | |
|---|---|
| physical device existence | the device is there whether or not anything names it |
| assignment lifetime | the claim, from `pci_function_claim` to the loss of its last name |
| capability-handle lifetime | one process's name for it, with its own handle generation |

The assignment carries a **generation** so that releasing a function and later
claiming the same BDF does not make a stale capability valid again.

### Derivation is its own operation

**`capability_attenuate_scoped` (operation 16) is not reused.** It is a
`MemoryAuthority` reservation with accounting semantics — a parent's remainder
falls by what the child may spend — and a PCI function is not a quantity of
memory. Inheriting those semantics accidentally is the error that would be
saved by reusing the number, and an ABI number is not worth it.

```text
Bus authority + bus/device/function
        ↓  pci_function_claim
validated, exclusively assigned PciFunction object
        ↓
config operations take only PciFunction + offset + width
```

The Bus holder names a BDF when deriving, because possession of the Bus
capability **is** the authority to address functions within that bus scope. After
derivation the holder of the `PciFunction` cannot choose another BDF: no
configuration operation accepts one.

Three operations, not two. Operation-count minimisation is not an architectural
requirement, and the earlier estimate of two is superseded.

## 11. What this ADR does not decide

Unchanged from the STOP, and restated because acceptance must not be read as
widening: BAR → MMIO mapping, device-memory region semantics, interrupt routing
and acknowledgement, DMA authority, IOMMU semantics, reset, VirtIO feature
negotiation, VirtIO queues, block reads and writes, device matching policy,
`block.device.v1`, persistent state and repository handoff.

**Device matching remains deliberately open.** Reading identifiers is discovery;
deciding which driver should own them is policy, and it comes later.

## 12. The liveness finding, and where it belongs

`SYSTEM_ABI_V1` §6 states the liveness rule as "nothing runnable **and nothing
routed can change that**". `nucleus/src/process.rs` implements only the first
half and says so in its own comment. It is correct for a stage that routes no
device interrupt and becomes wrong at the first one: a driver blocked on its
interrupt, alone in the system, would be cancelled the instant it blocked.

**It is not part of Stage 4A**, which routes no interrupt and needs none. It is
recorded as a **mandatory prerequisite to the first routed device interrupt** and
is a blocking item for the IRQ slice, not for configuration access.

## 13. Performance

`docs/35` §Stage 4's hard budgets are per completed block request, and this ADR
introduces no request path. Checked against each anyway:

| Budget | This decision |
|---|---|
| zero dynamic allocation per steady-state request | unaffected; a claim writes one table slot |
| ≤1 payload copy | unaffected; no payload |
| ≤4 address-space/scheduler handoffs per request | **watch**: one crossing per configuration access. Acceptable because configuration access is an initialisation path. A later design that read configuration space per request would violate this and must not |
| batching-capable interrupt handling | not decided here |
| no global driver lock across independent queues | unaffected. The nucleus serialises the CAM window, which is a hardware register pair and not a driver lock; it is on no request path, and a later ECAM backend removes even that |

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. I-07 (explicit
  capabilities, no ambient global privilege) and I-08 (user-space drivers) are
  strengthened: hardware authority gains a named root and a traceable ancestry
  where it previously had neither.
- **Canonical representation:** unchanged. The PCI service is canonical TOS Core
  text carried through the ordinary source → lower → TOSIMAGE → independent
  verifier → runtime path.
- **Trusted-base impact:** the nucleus gains a bounded configuration accessor
  over a serialised port pair, a bus object and an assignment table. **No
  parser**, no physical mapping path, no ACPI, no boot-ABI change.
- **Source-to-runtime impact:** none. No new artifact class and no new digest.
- **Recovery and rollback impact:** none. It arrives with persistent storage,
  not with discovery.
- **Stage identity gate:** `docs/37` §Stage 4. This ADR closes no gate; §14 is
  what a gate would have to show.
- **Threat-model impact:** a new authority root and a new privileged mechanism,
  both requiring coverage under AGENTS §10. The surface is a bounded offset, a
  bounded width and a nucleus-held BDF; the negative cases are §14.
- **Compatibility profile:** `PLATFORM_INTERFACE_V1` at version 1, two new
  `OBJECT_*` kinds, three new rights, three new `SYSTEM_ABI_V1` operations and a
  `LAUNCH_VERSION` bump — each a public boundary versioned from its first commit
  under I-11.
- **New dependencies:** none.

## 14. Conformance evidence

Positive, on the real device and from canonical text:

1. a textual TOS Core process reads the `virtio-blk-pci` function and reports
   `vendor=0x1AF4`, `device=0x1042`, class/subclass mass storage, and a
   capability list that is genuinely device-provided, with its own source
   identity in the evidence;
2. the host configures QEMU and checks what the guest reported, and supplies
   none of those answers.

Negative, and a successful read alone is not sufficient Stage 4 evidence:

3. a process holding no Bus or `PciFunction` authority cannot perform a
   configuration operation;
4. authority for function A cannot read function B;
5. `config_read` without `config_write` cannot mutate;
6. a stale or released function capability refuses;
7. an offset outside conventional configuration space refuses;
8. a malformed alignment or width refuses;
9. a forged scalar in the capability position is refused;
10. a BAR value cannot be used as MMIO authority;
11. without the actual QEMU VirtIO device, the positive proof fails rather than
    passing from a fixture.

Existing handle and refusal ordering is unchanged: index bounds, then
generation, then type, then rights.

## 15. Realisation, and one thing this decision does not by itself reach

Recorded here because a decision whose implementation state is invisible invites
being re-derived. Full evidence:
`docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md`.

**Built, green and gated**: the platform root and its lifecycle (§5, §9), the
Bus → `PciFunction` derivation with exclusive assignment (§10), the nucleus
mechanism and its CAM backend (§6, §7), `PLATFORM_INTERFACE_V1` (§8), and
`SYSTEM_ABI_V1` operations 24–26. A canonical textual module holds the root,
claims real functions of the Stage 4 machine, and is refused in three distinct
ways it cannot itself decide.

**Not reached**: the configuration read *from text*. Operations 25 and 26 exist
and the nucleus performs them; no module can **declare** them, because
`SYSTEM_INTERFACE_V1` §4.1 requires an `extern`'s `uses` to name an
`import capability` of the enclosing module, and a `platform.pci.FunctionConfig`
import cannot be answered at startup — the only lawful producer is operation 24,
which runs afterwards.

That is **not a defect in this decision** and does not reopen D1–D5. It is the
question ADR-0078 §6 explicitly left open — "a capability of an interface a
module never imports … is a separate question and is not answered here" — reached
for the first time by the first authority whose object cannot exist before the
process that claims it. Its decision surface is
`STAGE4A_HARDWARE_BOUNDARY.md` §7, and it belongs to whoever settles ADR-0078's
remainder rather than to this ADR.
