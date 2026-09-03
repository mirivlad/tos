<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4A — the hardware boundary audit, and why implementation stopped

- Status: **audit, 2026-09-03. Architecture STOP under the Stage 4A round's
  rule 4.** No hardware mechanism was implemented, no contract was amended, and
  no Stage 4 gate is claimed
- Round: Stage 4A — Hardware Boundary and PCI Discovery, opened on `main` after
  the post-Stage-3 UX commit `a3e4975`
- Decision surface: `docs/adr/0079-hardware-authority-origin.md` (**Proposed**)
- Covers: the contract audit, the classification of `docs/11` into normative and
  intent, the implementation audit of the mechanisms Stage 4 would need, one
  finding that is due before any interrupt work, and the proposed Stage 4
  platform extension

## 1. What Stage 4 is building first, and what it is not

The Stage 4 dependency chain, and where the work stands:

```text
platform/device discovery      ← Stage 4A stops here, before the first step
hardware capability contracts
PCI function assigned to a textual driver
BAR / MMIO
IRQ
DMA
VirtIO block queue
real block I/O
persistent object/state storage
capsule → repository-backed handoff
```

Stage 4A's target was the narrowest hardware-facing act in the chain: a
canonical textual TOS Core process reading the real `virtio-blk-pci` function's
configuration space under a capability, with no host fixture, no shadow driver,
no ambient authority and no nucleus-prepared answer.

That target was not reached, and it was not reached for an architectural reason
rather than an engineering one. The audit below is what established that.

## 2. The audit: what is normative, what is intent

The round's brief required this distinction rather than assuming it, and it
turned out to be load-bearing.

| Material | Tier | Status for Stage 4 |
|---|---|---|
| `docs/02` I-07, I-08 | 0 | binding: explicit capabilities, no ambient global privilege, user-space drivers |
| `docs/11` §Driver process (the capability *list*) | 2 | **architectural intent.** It says a driver receives "PCI function configuration; MMIO regions; … interrupt endpoint; DMA allocator with limits", which is a description of a system to be built, not a contract to be conformed to |
| `docs/11` §Driver manifest (the four `import capability platform.*` lines) | 2 | **not accepted interfaces.** The form is accepted V1 form; the paths are not types. `SYSTEM_INTERFACE_V1` §4 does not declare them, `tos-core/src/interfaces.rs`'s `ACCEPTED` table does not contain them, and `types.rs:416` resolves an interface path only against that table. ADR-0051 §4 corrected the previous version of this same passage for the same class of reason |
| `docs/11` §Interrupts | 2 | **normative and constraining**: the nucleus acknowledges and routes low-level interrupts to driver event endpoints; drivers must not block routing indefinitely; shared interrupts are mediated with explicit acknowledgement |
| `docs/11` §DMA | 2 | **normative and constraining**: DMA regions come from a trusted service or nucleus primitive; the driver receives a bounded region and a device-visible address mapping |
| `docs/11` §"Driver interfaces" (`block.device.v1` &c.) | 2 | normative as a *class* rule — drivers publish device-class interfaces — with the publication mechanism already fixed by ADR-0051 §2 and `CAPABILITY_V1` §6 |
| ADR-0037 §2 | 1 | binding: both `DmaRegion` variants are neither shareable nor transferable, and a widening needs "a typed driver or device contract that says what makes it safe" |
| ADR-0049 §"What this deliberately does not do" | 1 | binding: no external device interrupt is routed, and "routing one early to 'test the path' would create an undocumented driver boundary" |
| ADR-0051 §"leaves open" | 1 | binding: device matching is not settled, and is "a query evaluated by a bus manager against hardware, not an authority a launcher grants" |
| `SYSTEM_ABI_V1` §2 | 2 | binding, and the clause the whole round turns on (ADR-0079 §3e) |
| `CAPABILITY_V1` §2 as amended by ADR-0075 §5 | 2 | binding: the capability-origin rule |

**The single most useful result of the audit is the second and third rows.**
`docs/11`'s manifest example reads like a settled interface set — it is written
in valid V1 syntax, with plausible paths, in a Tier 2 document. It is neither
declared nor reachable. Had it been taken at face value, Stage 4A would have
implemented four interfaces nobody accepted.

## 3. The implementation audit

What exists, checked in the tree rather than inferred from documents:

| Mechanism Stage 4 needs | Present today |
|---|---|
| PCI enumeration, anywhere | **none.** No occurrence of PCI in the nucleus, the loader, the runtime image or any crate |
| ACPI MCFG / ECAM discovery | **none.** `BootInfo` carries `acpi_rsdp` and `acpi_version` (`boot-protocol/src/lib.rs:127`), validated by the loader (`uefi-loader/src/main.rs:110`, `:168`); nothing parses a table beyond the RSDP prefix |
| port I/O from CPL 3 | **structurally absent.** One TSS, `io_map_base` past the segment limit, IOPL 0 (`exception.rs:158`). The single exception is `IoBitmap::ALLOWING_COM1` under `feature = "test-measurement-port"` — and with one TSS for the machine that is a machine property, not process authority |
| mapping a caller-named physical range | **none.** Operation 17 chooses the address itself; nothing in `region.rs` accepts a physical address |
| a device-memory region class | **none.** `RegionObject` (ADR-0075 §6) has backing, an access mode, reference counts and a funding authority — and no physical identity, no caching mode |
| a capability object kind for a device | **none.** `Object` in `capability.rs` names eight kinds beside `None`, which is "the state of a slot nobody has been given" rather than a kind; the public `OBJECT_*` space in `tos-launch` ends at 8 |
| a platform interface schema | **none.** `ACCEPTED` declares six interfaces, all `system.*` |
| routed device interrupts | **none, deliberately** (ADR-0049) |
| a `DmaRegion` origin | **none.** ADR-0037 fixes the type facts; no operation produces one |

Two facts worth stating positively, because they are what makes the rest of
Stage 4 reachable once the boundary is decided: the capability, region, launch
plan and process machinery is complete and evidenced, and ADR-0078 already
repaired the one representation gap that would otherwise have blocked a driver
from acting on a capability an operation handed it at runtime. **Nothing in the
textual side of the boundary is missing. What is missing is the hardware side's
right to exist.**

## 4. The STOP report

In the five parts the round's rule 4 requires.

### 4.1 Exact accepted clauses involved

1. `CAPABILITY_V1` §2, as amended by ADR-0075 §5 — "No operation creates
   authority over a pre-existing external object out of nothing."
2. ADR-0055 (Accepted, option A) — a process's table is written before it is
   entered, from the launcher's endowment; the recursion terminates at the boot
   process's stated constant, which is empty on a canonical boot.
3. `SYSTEM_ABI_V1` §2 — "Filesystems, **devices**, repositories, networks and
   consoles are services reached through IPC, not operations added here. If an
   operation could be a service, it is a service."
4. `SYSTEM_ABI_V1` §5 operation 17 — "The nucleus chooses the address; a caller
   never supplies one."
5. ADR-0075 §6 — "backing — the pages; nothing about where they are is public";
   and `charged to — the MemoryAuthority the allocation spent from`.
6. ADR-0076 — one physical account: every region is charged out of the pool.
7. `SYSTEM_INTERFACE_V1` §4 — an interface a module may name is one an accepted
   schema declares; §2 anticipates a Stage 4 schema but ADR-0060 accepted only
   this one.

### 4.2 The smallest unreachable operation

A canonical textual TOS Core process reads the dword at offset `0x00` of the
`virtio-blk-pci` function's configuration space and receives `0x1042_1AF4`,
holding authority over no other function.

### 4.3 Why existing mechanisms cannot express it

- **No origin.** A PCI function is a pre-existing external object. It is not
  derivable by attenuation from anything the boot process holds (clause 2, and
  the constant is empty), and it is not ADR-0055's Option B self-only creation,
  whose justification — creating something nobody else can reach confers no
  authority over anyone — is false of a device that exists whether or not
  anybody names it. Clause 1 refuses the remaining path.
- **No access path from CPL 3.** Port I/O is shut by one TSS at IOPL 0, and
  opening it would be a machine-wide grant of a single global window that *is*
  arbitrary access to every function — ambient authority by construction. A
  memory window is shut by clause 4.
- **No region that could be the window.** Clauses 5 and 6: device frames are not
  pool bytes, cannot be charged, cannot be reclaimed, and must be named by the
  physical address the accepted model keeps private. A device region is a new
  class with a new origin, not a use of the existing one.
- **No interface to declare it.** Clause 7.
- **And the two available mechanisms are refused by different clauses**, so
  there is no design that satisfies all of them: an ABI accessor is against
  clause 3's letter, and a mapped ECAM window is against clauses 4, 5 and 6.
  This is the conflict, and `docs/38` §Conflict protocol requires it be reported
  rather than resolved by choosing the easier implementation.

### 4.4 The smallest Project Architect decision surface

Two decisions cannot be deferred; three follow from the second and are stated so
that accepting it does not leave them implied. All five are in ADR-0079 §5.

```text
D1  where hardware authority originates
      A  a launcher-minted root bus authority, named in the launch record,
         the shape ADR-0075 §2b already uses for the root MemoryAuthority
      B  the nucleus enumerates and endows            [named to be rejected]
      C  a self-only creation rule                    [refused by clause 1]
    recommended: A

D2  how configuration space is read
      M  a bounded ABI mechanism operation on the function capability
      R  a device-memory region over the function's ECAM page
      H  reads by region, writes by operation
    recommended: M for Stage 4A, R re-opened at the BAR/MMIO slice

D3  where the platform facts come from
      P1 the nucleus parses ACPI MCFG
      P2 the loader parses it; BOOT_ABI_V1 → v2
      P3 CAM, and no ACPI at all
    recommended: P3 now, P2 when MSI-X or extended config space is needed

D4  what a function capability is — object, scope, rights, lifetime, staleness;
    read and write as separate rights; a BAR as data and not authority; no reset
    right allocated until reset has an operation

D5  who may hold bus authority, and whether the nucleus refuses it as a launch
    plan entry the way operation 22 already refuses regions, replies and plans
```

**The narrowest accepting decision is D1-A + D2-M + D3-P3.** Its whole cost is
one new authority root, one new capability object kind, one new interface schema
with two operations, two new `SYSTEM_ABI_V1` numbers and a `LAUNCH_VERSION` bump
— and **no new parser in the trusted base, no physical mapping path, no new
region class and no change to ADR-0075 or ADR-0076.** It also requires the
Architect's reading of `SYSTEM_ABI_V1` §2, and if that reading is given, §2's
sentence should gain a clause distinguishing a device *service* from a device
*access primitive* rather than being left to be read against the operation
table.

### 4.5 Alternatives and their architectural consequences

| Path | Consequence |
|---|---|
| D1-A + D2-M + D3-P3 | smallest surface; config access stays mediated, so BAR programming and reset remain mediated when they arrive; one crossing per dword, on an initialisation path only; requires a §2 reading |
| D1-A + D2-R | `SYSTEM_ABI_V1` §2 untouched, and the device-region class arrives once for both config and BARs — but ADR-0075 and ADR-0076 are amended, and BAR/reset writes become stores the nucleus never sees, moving two properties out of the capability model into page permissions |
| D1-A + D2-H | both mechanisms, both audits, negative proofs split across two surfaces |
| D1-B (any D2) | the nucleus becomes the PCI policy engine and the matcher; fails ADR-0048 §2, ADR-0055 and the Stage 4 identity gate even if every read works |
| do nothing | Stage 4 cannot begin. Every later step in §1's chain depends on a function capability existing |

**Not taken, and recorded so it stays not taken:** a Rust helper in the nucleus
or the runtime image that enumerates PCI and hands a textual module a
"VirtIO block found" record. It would produce a working read this week and fail
`docs/37` §Stage 4 permanently — "text merely configures an in-kernel driver" is
a named failure condition, and the round's rule 6 names the same shape four ways.

## 5. One finding that is due before any interrupt work

Not part of the STOP, and not a decision — an accepted clause whose second half
the implementation does not yet honour, found while auditing D-adjacent
mechanisms and reported here so it is not discovered by a hanging driver.

`SYSTEM_ABI_V1` §6 states the liveness rule as:

> when no context is runnable and some context is blocked, **and nothing routed
> can change that**, every block is cancelled at that instant

and then says what that costs: "in a stage that routes no device interrupt, 'no
runnable context' and 'a state nothing can leave' are the same thing, and in a
stage that routes one they are not. An implementation whose rule reads only
'nothing runnable' becomes wrong at the moment a driver exists."

`nucleus/src/process.rs:1091` implements only the first half, and says so in its
own comment: "**Stage 4 must revisit this.** … the second half stops being free
the day a device interrupt can wake a driver." The code is correct for Stage 3
and the comment is accurate.

**The consequence for Stage 4:** the first routed device interrupt makes
`cancel_every_block` cancel waits that the device *would* have satisfied — a
driver blocked on its interrupt, alone in the system, is cancelled the instant
it blocks. So the rule needs a routed-source term before an interrupt object
exists, not after. This is implementation of an accepted clause rather than a
new decision, and it belongs to the IRQ slice.

## 6. The proposed Stage 4 platform extension

Recorded now because the round's rule 9 asks for it, and because a platform
chosen after a measurement means nothing. **Proposed, not applied** — no harness
was changed in this round.

A Stage 4 *extension* of the accepted base profile, never a global change to it:
ADR-0040 fixes the Stage 1/Stage 2 reference platform, and Stage 1–3 gates
continue to run the profile they were measured on, unmodified.

```text
machine        q35                      unchanged from ADR-0040
cpu            qemu64                   unchanged
vcpus          1                        unchanged
memory         256 MiB                  unchanged
accelerator    TCG                      unchanged
firmware       the declared OVMF build of the Stage 1 gate
                                        unchanged

added for Stage 4, and only for Stage 4 gates:
device         virtio-blk-pci
transport      modern VIRTIO PCI — disable-legacy=on, disable-modern=off,
               so the function reports device 0x1042 and not the
               transitional 0x1001. The transport profile is chosen because
               it is the one Stage 4 intends to continue with, not because
               its configuration space is easier to read
identity       vendor 0x1AF4, device 0x1042, class 0x01 (mass storage)
pci location   deliberately fixed by addr=, so evidence names one function
               rather than whichever slot enumeration happened to find
backing image  raw, fixed size, deterministic content
num-queues     1 for the first slice; the property is recorded because
               feature negotiation is part of the surface QEMU exposes
iommu_platform off for the first slice, and this is the choice ADR-0079's
               DMA note has to survive: the textual driver contract must not
               change when it becomes on
observed       QEMU 10.0.11 (Debian 1:10.0.11+ds-0+deb13u1), the emulator
               this audit's device-property check was run against
```

The `iommu_platform` row is the one to read twice. `docs/11` §DMA requires that
"IOMMU support should later enforce hardware isolation without changing the
driver contract", so the first DMA decision has to be shaped for a future in
which that property is on — which is a constraint on ADR-0079's deferred DMA
work, recorded here before anything is built rather than after.

## 7. What a new reader should be able to answer

- **What Stage 4 is building first**: not storage. The authority boundary in §1's
  first two rows, and nothing below them.
- **Where PCI authority originates**: undecided. ADR-0079 D1 is the question;
  a launcher-minted root bus authority, named in the launch record, is the
  recommendation.
- **Which component discovers devices**: undecided, and the answer must not be
  the nucleus (ADR-0079 D1-B, named to be rejected). Discovery, matching,
  granting and using are four authorities, and ADR-0051 already holds matching
  open.
- **What the nucleus does and deliberately does not do**: it may provide a
  primitive hardware isolation mechanism where one is unavoidable; it must not
  become the PCI policy engine, the matcher, or an ordinary device driver.
- **What textual code touched the real device**: **nothing.** No hardware was
  touched in this round, and no proof is claimed.
- **What authority it held**: none was minted, because none can be minted
  lawfully yet (§4.3).
- **What negative authority cases were proved**: none. ADR-0079 §8 lists the
  seven that the first slice must prove, and records that a successful read
  alone would not be sufficient Stage 4 evidence.
- **What remains before BAR/MMIO/IRQ/DMA and VirtIO block**: ADR-0079's D1 and
  D2 first — everything else in the chain depends on a function capability
  existing — then the §5 liveness term before the IRQ slice, then the
  device-region class, then a `DmaRegion` origin that ADR-0037 §2 admits.

## 8. Gates

No Stage 4 gate is claimed. No Stage 1, 2 or 3 gate was weakened: no harness, no
nucleus source, no interface contract and no accepted document was modified in
this round. What this round produced is two documents and the bookkeeping that
carries them.
