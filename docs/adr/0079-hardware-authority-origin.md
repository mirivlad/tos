<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0079: Where hardware authority comes from, and how a textual process reaches PCI configuration space

- Status: **Proposed** — the Stage 4A decision surface. Nothing here is
  implemented, and no contract is amended by this file's existence
- Date: 2026-09-03
- Decision level: **3** — it moves a trust boundary. It introduces a root of
  authority that is not memory, not a process, not an endpoint and not derived
  from any of them, and under §5's D2 it also decides whether device access is
  an ABI mechanism or a region class. Requires an ADR and Project Architect
  approval (`docs/21` §Level 3)
- Project Architect approval: **not given; this is the request for it**
- Related: ADR-0003 (user-space drivers), ADR-0037 §2 (the DMA region facts),
  ADR-0048 §2 (mechanism, not service policy), ADR-0049 (the interrupt
  baseline, and what it deliberately withheld), ADR-0051 §"What this
  deliberately leaves open" (device matching), ADR-0055 (where a first
  capability comes from), ADR-0060 (a schema is a class of document), ADR-0075
  §2, §5, §6 (region origins, the origin rule, the `RegionObject`), ADR-0076
  (one physical account), ADR-0077 (launch plans), ADR-0078 (capability
  sources). `CAPABILITY_V1` §2–§4, `SYSTEM_ABI_V1` §2, §5, §6,
  `SYSTEM_INTERFACE_V1` §2, §4, `IPC_V1` §5, `BOOT_ABI_V1`, `docs/11`,
  `docs/37` §Stage 4

## 1. The question, stated once

Stage 4's identity gate asks whether "a canonical textual user-space driver
actually move[s] persistent data through final-style MMIO/interrupt/DMA/IPC
boundaries" (`docs/37` §Stage 4). Every step of that rests on something that
does not exist yet and cannot be derived from anything that does: **a root of
hardware authority**.

Stage 4A does not need a block driver, a queue, a BAR, an interrupt or a DMA
buffer. It needs one textual process to read one real device's configuration
space under a capability. That is the narrowest hardware-facing act there is,
and it is already unreachable.

## 2. The smallest unreachable operation

> A canonical textual TOS Core process reads the 32-bit dword at offset `0x00`
> of the configuration space of the PCI function QEMU created for
> `virtio-blk-pci`, and receives `0x1042_1AF4` — the device and vendor IDs the
> real device reports — without holding authority over any other function.

Nothing smaller than this is a hardware-facing act at all, and nothing in the
accepted corpus makes it expressible.

## 3. Why no accepted mechanism reaches it

Five doors, each closed by a clause rather than by an implementation gap.

### 3a. There is no origin for a capability naming a device

`CAPABILITY_V1` §2, as amended by ADR-0075 §5:

> A capability an operation returns must have an explicitly defined normative
> origin: either authority the caller presented to that operation, which bounds
> what is produced, or an explicitly accepted bounded self-only creation rule.
> No operation creates authority over a pre-existing external object out of
> nothing.

A PCI function is a **pre-existing external object**. It cannot come from
attenuating what a process holds: ADR-0055 terminates the chain at the boot
process, whose endowment is the launcher's stated constant, and that constant is
empty on a canonical boot (`nucleus/src/main.rs`, the `test-*` constants grant
endpoints, self-authority and memory and nothing else). And it cannot be
ADR-0055's Option B self-only creation, because Option B's whole justification
is that "creating something nobody else can reach confers no authority over
anyone" — which is false of a device that exists whether or not anybody names
it.

**So a new legitimate origin is required.** That is the sixth item of the Stage
4A STOP list, and it is the load-bearing one: D1 below.

### 3b. Configuration space cannot be read from CPL 3

Both architectural paths are shut, and each by a different clause.

**Port I/O.** `exception.rs` keeps one TSS for the machine with
`io_map_base` pointing past the segment limit — "how the architecture spells
'no port is permitted at CPL 3'" — and IOPL stays 0. The one exception is
`IoBitmap::ALLOWING_COM1` under `feature = "test-measurement-port"`, and it
proves the constraint rather than relieving it: there is **one** TSS, so a bit
cleared in that map is a property of the machine and not of a process. A CAM
path (`0xCF8`/`0xCFC`) opened that way would be ambient for every process that
runs, and because CAM is a single global address/data window, holding it *is*
arbitrary access to every function's configuration space — the first item of the
STOP list, and the thing Stage 4A's brief forbids in terms.

**A memory window.** No accepted operation maps a caller-named physical range.
`SYSTEM_ABI_V1` operation 17 states the opposite — "The nucleus chooses the
address; a caller never supplies one" — and ADR-0075 §6 makes a region's
placement private: "backing — the pages; nothing about where they are is
public". Confirmed against the implementation: nothing in `region.rs` accepts a
physical address from anywhere.

### 3c. A device region is not a region ADR-0075 or ADR-0076 describes

Even granted an operation that mapped one, ECAM frames are not what the accepted
region model is about:

| Accepted clause | What a device window would need |
|---|---|
| ADR-0076: one physical account, every region charged to a `MemoryAuthority` | MMIO frames are not pool bytes; there is nothing to charge and nothing to reclaim |
| ADR-0075 §2 B: allocation "reserve[s] unique backing pages" out of the pool | the pages already exist and belong to a device |
| ADR-0075 §6: `charged to — the MemoryAuthority the allocation spent from` | no authority funded it |
| operation 17: the nucleus chooses the address | the address *is* the identity of the thing |
| nothing in the corpus | device memory must be mapped uncacheable on x86_64; no accepted clause mentions caching at all |

So a device-memory region is a **new region class with a new origin**, not a use
of the existing one. That is the second item of the STOP list.

### 3d. No accepted schema declares a platform interface

`docs/11` §"Driver manifest" now shows accepted V1 form, and that is exactly why
it must not be mistaken for an accepted interface:

```tos
import capability platform.pci.FunctionConfig as pci;
import capability platform.mmio.RegionMap as mmio;
import capability platform.irq.Binding as irq;
import capability platform.dma.Allocator as dma;
```

None of those four paths is declared by `SYSTEM_INTERFACE_V1` §4 or present in
`tos-core/src/interfaces.rs`'s `ACCEPTED` table, and `types.rs:416` resolves an
interface path only against that table. The form parses; the paths are not
types. `docs/11` is Tier 2 architectural intent here, and ADR-0051 §4 already
corrected the previous version of this same passage for the same reason.

`SYSTEM_INTERFACE_V1` §2 anticipates the fix — "A Stage 4 driver interface is
another instance of these rules, not a special case of this document" — but
ADR-0060 accepted *that* schema. A `PLATFORM_INTERFACE_V1` needs its own
accepting decision.

### 3e. `SYSTEM_ABI_V1` §2 names devices as services

> It carries mechanism the nucleus alone can provide: address spaces, execution
> contexts, capability handles, IPC transport, and the small amount of time a
> scheduler needs. Filesystems, **devices**, repositories, networks and consoles
> are services reached through IPC, not operations added here. **If an operation
> could be a service, it is a service.**

This is the clause D2 turns on, and it cannot be satisfied by both available
mechanisms at once. An ABI operation that reads configuration space is a device
operation added to the ABI, against the sentence's letter. A region that maps
ECAM avoids the sentence entirely and lands on §3c instead. **One of the two
must be chosen, and each costs an amendment somewhere.** Choosing quietly is
what `docs/38` §Conflict protocol forbids.

## 4. What Stage 4 must decide because nothing has

Recorded so the open questions are not re-derived later, and so that no part of
this ADR is read as settling one it does not:

| Question | State |
|---|---|
| PCI enumeration ownership | open — D1 |
| configuration-space access mechanism | open — D2 |
| platform facts (ECAM discovery) | open — D3 |
| function capability shape and rights | open — D4 |
| who may hold bus authority | open — D5 |
| device matching | **deliberately open**, ADR-0051: "matching is a query evaluated by a bus manager against hardware, not an authority a launcher grants" |
| BAR → MMIO mapping | open, and deliberately *not* opened here (§6) |
| interrupt binding and acknowledgement | open, and constrained by `docs/11` §Interrupts and ADR-0049 |
| DMA authority | open, and constrained by ADR-0037 §2 and `docs/11` §DMA |
| reset authority | open, and deliberately deferred (§6) |
| publication of `block.device.v1` | shape already fixed by ADR-0051 §2 and `CAPABILITY_V1` §6 — a request, never a claim |

## 5. The decision surface

Five decisions. D1 and D2 are the ones that cannot be deferred; D3, D4 and D5
follow from D2 and are stated so that accepting D2 does not leave them implied.

### D1 — where hardware authority originates

**A — a launcher-minted root bus authority.** A `platform.pci.Bus` capability is
part of the boot endowment, minted by the launcher from platform facts and named
in the launch and audit record, exactly as ADR-0075 §2b mints the root
`MemoryAuthority`: "Its size and its identity are written into the launch and
audit record, so the root of every later allocation can be named rather than
assumed." Authority over one function is derived from it by a **scoped**
narrowing that produces a new object, never by generic attenuation, which
`CAPABILITY_V1` §3 already distinguishes.

- *For*: one new root, and it is the shape the corpus already uses for the only
  other authority that cannot be derived from anything. The chain still
  terminates at a constant that can be named. Nothing becomes ambient.
- *Against*: a second kind of root is a second thing to audit, and the bus
  capability is a real confused-deputy surface (D5).

**B — the nucleus enumerates and endows each driver with its function.** No
root; ring 0 walks the bus and decides who gets what.

- *Against*: it makes the nucleus the PCI policy engine and the matcher. ADR-0048
  §2 gives the nucleus "mechanism and explicitly no service policy", ADR-0055
  refuses "a capability the nucleus decides on", and Stage 4A's brief names this
  outcome as a failure. **Named to be rejected.**

**C — a self-only creation rule for device authority.** Rejected in §3a: a
device is not an object only its creator can name.

**Recommended: A.**

### D2 — how configuration space is actually read

**M — a mechanism operation on the function capability.** Two operations, whose
first argument is the function capability; the nucleus performs the access, takes
the BDF **from the capability's object** and never from the caller, and bounds
the offset by a constant of the contract.

- *For*: no physical mapping ever crosses into ring 3. "Authority for function A
  cannot reach function B" becomes structural rather than checked, because the
  caller never names a function. Read and write are separate rights on separate
  operations, so "a read-only capability cannot mutate configuration" is a
  rights fact. No new region class, no caching question, no charge, no
  reclamation. The whole surface is two ABI numbers and one schema.
- *Against*: it puts a device operation in the ABI, against §3e's letter. The
  defence is that §2's own first sentence is "mechanism the nucleus alone can
  provide", and configuration access at CPL 3 with IOPL 0 and no mapping is
  precisely that: the PCI *bus service* remains a textual service reached through
  IPC, and this is the primitive underneath it as `endpoint_send` is the
  primitive underneath every service. **That reading is the Architect's to make,
  not this file's.** If it is made, §2's sentence should gain a clause
  distinguishing a device *service* from a device *access primitive*, rather than
  being left to be read against the table.
- *Cost*: one boundary crossing per dword. Enumeration of one function's header
  is tens of crossings and is not on any budgeted path (§7).

**R — a device-memory region over the function's ECAM page.** The nucleus maps
that function's 4 KiB uncacheable, and the driver does ordinary loads and stores.

- *For*: `SYSTEM_ABI_V1` §2 is untouched — nothing device-shaped is added to the
  ABI. The ECAM layout gives each function exactly one 4 KiB page, so the
  confinement wanted falls out of the hardware. And BARs will need a
  device-memory region anyway, so one decision would serve both.
- *Against*: it amends ADR-0075 and ADR-0076 rather than fitting them (§3c), and
  it moves two properties out of the capability model into page permissions: a
  write to a BAR register or a reset bit is a store the nucleus never sees, so
  BAR programming and reset stop being mediated at the moment config space
  becomes a window.
- *And*: it decides the device-region class under the pressure of a config-space
  read, which is the wrong evidence to decide it on.

**H — reads through a mapped read-only page, writes through an operation.**
Cheaper than it sounds, but the negative-proof set then splits across two
mechanisms and there are two things to audit instead of one.

**Recommended: M for Stage 4A, with R re-opened at the BAR/MMIO slice on its own
evidence.** BARs force the device-region question regardless; taking it now buys
nothing and spends the decision badly.

### D3 — where the platform facts come from

Only D2-M makes this cheap, which is part of why M is recommended.

**P1 — the nucleus parses ACPI MCFG** from `BootInfo.acpi_rsdp`. A new total
parser in the trusted base over firmware bytes: AGENTS §8 requires it be total,
structured and panic-free, and §10 requires threat-model coverage and negative
tests. MCFG is small, and this is still nucleus growth for a Stage 4A that does
not otherwise need it.

**P2 — the loader finds MCFG and `BootInfo` carries an ECAM descriptor.**
`BOOT_ABI_V1` goes to v2. The parse happens where firmware data is already
handled — `uefi-loader/src/main.rs` already walks the configuration table and
validates the RSDP — and the nucleus receives a validated descriptor. A
versioned boundary change, which that contract exists to absorb.

**P3 — CAM (`0xCF8`/`0xCFC`), and no ACPI at all.** Under D2-M the window lives
entirely in ring 0, where it is owned and serialised and never exposed, so its
being global costs nothing. No parser, no boot-ABI change.

- *Limit, stated rather than discovered later*: CAM cannot reach offsets ≥
  `0x100`. Stage 4A does not need them — the VIRTIO PCI capability list is in
  standard config space, reached through the capability pointer at `0x34` — but
  MSI-X and any PCIe extended capability will, and that is when P2 becomes due.

**Recommended: P3 for Stage 4A, P2 when extended config space or MSI-X is
actually needed.** It is the smallest thing that works, and its expiry date is
known in advance.

### D4 — what a function capability is

Stated as a proposal because the brief asks for it, and because a rights set
allocated loosely is harder to narrow later than to widen.

```text
object     PciFunction { segment, bus, device, function, generation }
           — held in the nucleus's object table; never in the handle
scope      exactly one function. Derived from the bus capability by a scoped
           narrowing that makes a new object (CAPABILITY_V1 §3), because generic
           attenuation does not consume its input and would leave two names for
           one claim
rights     config_read, config_write — separate, per the brief
lifetime   bounded by the bus capability's and by the holder's; a claimed
           function returns to the bus on release or holder death
staleness  by generation, as everything else
```

**A BAR is data in Stage 4A, not authority.** Reading `BAR0..5` through
`config_read` yields the numbers the device reports, and holding those numbers
maps nothing. That is what keeps the BAR/MMIO decision genuinely open instead of
pre-decided by a header read.

**No reset right is allocated.** `SYSTEM_INTERFACE_V1` §4's own rule is that a
contract must not declare "an operation the system does not perform"; a right
with no operation is the same fault one layer down. Reset is deferred explicitly
(§6), not represented and left unused.

**Not a forgeable integer.** The BDF is in the nucleus's object. A caller that
fabricates a scalar where a function capability belongs is refused by the
verifier (ADR-0078 §4) and by the nucleus's kind check, and a caller cannot name
a BDF at all — there is no parameter for one.

### D5 — who may hold bus authority

The bus capability can claim every function, so it is the confused-deputy
surface `CAPABILITY_V1` §7.6 names as the test that "fails quietly in systems
that pass the other five".

Proposed: it is endowed to exactly one textual PCI bus service; a driver never
receives one, and what travels to a driver in a launch plan is the **function**
capability. Whether the nucleus should refuse a bus capability as a plan entry —
as `SYSTEM_ABI_V1` operation 22 already refuses regions, replies and plans — is
a decision, and refusing it would make "a driver cannot hold bus authority" a
mechanism rather than a policy.

## 6. What this ADR does not decide

- **BAR → MMIO mapping.** Deferred to the next slice on its own evidence
  (D2). Nothing here maps device memory.
- **Interrupts.** `docs/11` §Interrupts is Tier 2 and already constrains the
  answer — the nucleus acknowledges and routes to driver event endpoints — and
  ADR-0049 withheld external routing deliberately. One consequence is due before
  any interrupt work, and it is a finding rather than a decision: see
  `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md` §5 on the liveness rule.
- **DMA.** ADR-0037 §2 makes both `DmaRegion` variants neither shareable nor
  transferable and says a widening may come "later through a typed driver or
  device contract that says what makes it safe". No such contract is proposed
  here, and no `DmaRegion` origin exists yet.
- **Device matching.** ADR-0051 left it open on purpose; nothing here narrows it.
- **Reset authority.** Deferred, per D4.
- **`block.device.v1`.** Not proposed. Its shape needs nothing new:
  `CAPABILITY_V1` §6 and ADR-0051 §2 already make publication an authority
  requested and granted, never claimed.

## 7. Performance pre-check

`docs/35` §Stage 4's hard budgets are per **completed block request**, and
nothing in D1–D5 touches a request path — there is no request path. Checked
against each anyway, because the brief requires an early choice that would
structurally violate one to be recorded before it is implemented:

| Budget | D1–D5 |
|---|---|
| zero dynamic allocation per steady-state request | unaffected; a function capability is a table slot written once at claim |
| ≤1 payload copy | unaffected; no payload |
| ≤4 address-space/scheduler handoffs per request | **watch**: D2-M is one crossing per dword. That is acceptable only because config access is an initialisation path. A later design that read config space per request would violate this, and must not |
| batching-capable interrupt handling | not decided here |
| no global driver lock across independent queues | unaffected; no queue. D5's single bus service is not a driver lock — it serialises *claims*, not I/O |

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. I-07 (explicit
  capabilities, no ambient global privilege) and I-08 (user-space drivers) are
  the two this is *for*; both are strengthened by D1-A and D2-M and would be
  weakened by D1-B.
- **Canonical representation:** unchanged. The PCI service is canonical TOS Core
  text carried through the ordinary source → lower → TOSIMAGE → verifier →
  runtime path.
- **Trusted-base impact:** under D2-M plus D3-P3, the nucleus gains a bounded
  configuration accessor and an object table, and **no parser**. Under D3-P1 it
  gains a parser over firmware bytes; under D3-P2 the loader does instead and
  `BOOT_ABI_V1` versions.
- **Source-to-runtime impact:** none. No new artifact class, no new digest.
- **Recovery and rollback impact:** none yet. It arrives with persistent
  storage, not with discovery.
- **Stage identity gate:** `docs/37` §Stage 4. **No gate is claimed or closed by
  this ADR.** It states what must be decided before the first hardware-facing
  textual act can honestly exist.
- **Threat-model impact:** a new authority root and, under D2-R, a new physical
  mapping path — both requiring coverage under AGENTS §10. D2-M's surface is a
  bounded offset and a nucleus-held BDF; D2-R's is a page permission.
- **Compatibility profile:** a new `PLATFORM_INTERFACE_V1`, new `OBJECT_*` kinds
  (the space currently ends at 8), a `LAUNCH_VERSION` bump (currently 4) and new
  `SYSTEM_ABI_V1` operation numbers (next free 24) — every one a public boundary
  versioned from its first commit under I-11.
- **New dependencies:** none.
- **Tests, if accepted:** §8.

## 8. Conformance evidence required if this is accepted

Positive, on the real device and from canonical text:

1. a textual TOS Core process reads offset `0x00` of the `virtio-blk-pci`
   function and reports `vendor=0x1AF4 device=0x1042`, with its own source
   identity in the evidence;
2. class and subclass read as mass storage, and the capability pointer at `0x34`
   leads to the VIRTIO capability list — enough for the next step, and no more.

Negative, and a successful read alone is not sufficient Stage 4 evidence:

3. a process holding no PCI authority cannot read configuration space, and is
   refused by handle rather than by policy;
4. authority for function A cannot reach function B;
5. a `config_read`-only capability refuses `config_write`;
6. a released or stale function capability refuses by generation;
7. an out-of-range configuration offset is refused rather than wrapped or
   truncated;
8. a fabricated scalar in the capability position is refused by the independent
   verifier, and a fabricated BDF is unexpressible because no parameter carries
   one;
9. the harness configures QEMU, observes the log and asserts — and performs none
   of the guest's discovery. With the device absent, the proof fails rather than
   passing from a fixture.
