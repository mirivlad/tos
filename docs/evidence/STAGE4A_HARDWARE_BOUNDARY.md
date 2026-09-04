<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4A — the hardware authority boundary, as built

- Status: **evidence, 2026-09-04. Stage 4A is formally closed** by Project
  Architect approval for evidence commit `2655aaa`; see §12 and
  `source/legal/publication-records/2655aaa3d7bd0993c5bbfe0da168d2dd1c44641d-stage4a-closure-approval.md`.
  Canonical
  TOS Core reads the real device's configuration space under a capability, and
  all nine authority negatives are gated — eight of them executed rather than
  asserted. The second STOP this round found while implementing the first was
  resolved by ADR-0080; §7 records how
- Decisions: `docs/adr/0079-hardware-authority-origin.md` (**Accepted**,
  Vladimir Tomashevskiy, 2026-09-03) and
  `docs/adr/0080-capability-effects-name-interfaces.md` (**Accepted**,
  Vladimir Tomashevskiy, 2026-09-04)
- Gates: `source/host-tools/qemu-test/pci-discovery.sh`, in preflight as
  *QEMU textual PCI function claim* (`qemu` profile, `full-only`)
- Covers: the platform root, the Bus → `PciFunction` derivation, the nucleus
  mechanism, `PLATFORM_INTERFACE_V1`, the Stage 4 device profile, what the
  textual module proved, and the two things that remain

## 1. What was built

The chain the ruling fixed, end to end:

```text
boot/platform
    ↓  minted once by the launcher, scope and identity in the record
root platform.pci.Bus                       ← built, gated
    ↓  held by the canonical textual boot process
    ↓  pci_function_claim
platform.pci.FunctionConfig                 ← built, gated; a runtime value of
    ↓                                         an interface nothing imported
    ↓  pci_config_read
real configuration space                    ← built, gated, and read
```

Every row is implemented, exercised from canonical TOS Core on the real QEMU
machine, and held by a gate.

**The last row is the one that matters, and it needed a language decision to
reach.** A claim is authority over an *address* and touches no hardware; only a
configuration read does. Until ADR-0080 no module could declare an operation on
`platform.pci.FunctionConfig`, because the frontend required every `uses` item
to be an import binding and no import can answer a request for an object that
does not exist until the claim runs. §7 records that.

## 2. Where hardware authority originates, and its lifecycle

A root `platform.pci.Bus` is minted at the launch boundary by
`pci::endow_root`, which is reachable from the launcher's constant and from no
dispatcher. `SYSTEM_ABI_V1` has no operation that produces one, so a process
cannot ask for one; `CAPABILITY_V1` §2's third origin class (ADR-0079 §9) is
what makes it lawful rather than ambient.

Its scope and identity are on the record rather than implied:

```text
TOS.RUN.PCI_ROOT segment=0 first_bus=0 last_bus=255 rights=claim asserted_by=launcher
```

**One mint per boot.** A second call returns `None`; a second root would be a
second, unattributable ancestry for everything under it.

**The root outlives its users.** A bus object is a table slot for the life of the
boot, like an endpoint: releasing a capability that names it drops a name and not
the object. That is what lets a supervisor retain the root, delegate a name for
it to a PCI service through a launch plan, and re-delegate after that service
crashes — without the root going with it.

**No nucleus rule names a module.** Nothing in ring 0 says who may hold bus
authority. The restriction is the capability flow, the launch policy, the source
identity and the audit record, exactly as the ruling requires.

**One simplification, stated rather than left to be noticed.** The ruling's
lifecycle has a supervisor retaining the root and delegating a scoped name for it
to a separate PCI service. This slice has **one** process, which holds the root
and claims through it. Both properties the two-process shape exists to guarantee
hold structurally and are not merely untested — the bus object outlives every
handle to it, so a holder ending cannot take the root with it, and there is no
module-name policy anywhere to remove. What is *not* exercised is the delegation
itself: `endow_for_launch` on `platform.pci.Bus` is declared, implemented and
reachable, and no gate yet drives a second process through it. That belongs with
the first real PCI service, not with the first claim.

## 3. Bus → `PciFunction`

`pci_function_claim` (operation 24) is its own operation and deliberately not
`capability_attenuate_scoped` (16): that one is a memory reservation whose
semantics are a parent's remainder falling by what a child may spend, and a PCI
function is not a quantity of memory.

The Bus holder names a bus, a device and a function — three registers, not a
packed word, so each has its own architectural range to be refused against and
there is no canonical form to argue about. The **segment is the capability's**
and is never a caller argument.

After derivation the BDF lives in nucleus state. **No configuration operation
takes one**, so a holder of a function capability cannot address a different
function — not because it is forbidden to, but because there is nowhere to say
so.

Assignment is **exclusive** (the ruling's preference, §6): at most one live
assignment per function under one root, a second claim refused with `E_LIMIT`
while the first lives. It conflicts with no capability invariant, because the
exclusivity is a property of the claim rather than of the capability: several
capabilities may name one assignment, since `capability_attenuate` makes another
name and a later manager/driver split needs exactly that. The assignment carries
a generation, so releasing a function and claiming it again does not revive a
handle to the first claim.

## 4. What the nucleus gained, and why each piece cannot be user-space

`source/nucleus/src/pci.rs`, 535 lines, plus three dispatch arms in
`syscall.rs` and two `Object` variants in `capability.rs`.

| Piece | Why not user-space |
|---|---|
| the CAM transaction (`0xCF8`/`0xCFC`) | port I/O is unreachable at CPL 3 — IOPL is 0 and the single TSS admits no port. Exposing the pair would be ambient access to **every** function's configuration space, since it is one global window |
| the bus object and its scope | a root cannot be produced by an operation; something outside the ABI has to mint it, and the launcher is the only thing that runs before a process does |
| the assignment table | exclusivity is a property of the machine, not of one process's view of it; a service-held table could be bypassed by a second holder of the same bus |
| the offset/width bound | it is the thing standing between a caller and the hardware, so it lives where the hardware is touched |

**What the nucleus did not gain**, and this is the load-bearing half: no
enumeration, no device matching, no device class, no VirtIO, no driver
selection, no ACPI parser and no boot-ABI change. `pci.rs` cannot tell a block
device from a serial controller. It answers "read offset 8 of the function this
capability names".

## 5. The textual module, and what it read

`source/tests/vectors/pci/init.tos`, canonical TOS Core carried through the
ordinary source → capsule → lower → TOSIMAGE → independent verifier → runtime
path. Its source identity is in the evidence:

```text
TOS.BOOTTEXT.DIGEST 7e326a4312940e02a964f5c1d75235bfeb1139b654c999d2ba1f5b1852ec14ba
TOS.IDENTITY source_kind=detached capsule_digest=800a469e…
```

It is a **TOS Core 1.1** module. It holds exactly one capability and asked for
it by name and kind — and for nothing else:

```text
TOS.RUN.REQUEST binding=bus interface=platform.pci.Bus object=9 wanted=9
```

There is one request in the whole run. It never imported
`platform.pci.FunctionConfig`; it declares that interface as an **effect** and
acts on the value the claim returned (ADR-0080).

The nucleus named the function it assigned — a BDF from the object it created,
which a module cannot put there:

```text
TOS.RUN.PCI_ASSIGNED process=0 segment=0 bus=0 device=4 function=0 generation=1 asserted_by=nucleus
```

And the module reported what the device said about itself, five fields packed
into one value:

```text
TOS.RUN.COMPLETED value=i64:42784201027754740     (0x0098000110421AF4)

  vendor           0x1AF4     offset 0x00
  device           0x1042     offset 0x02   modern VirtIO, not the transitional 0x1001
  class            0x01       offset 0x0B   mass storage
  subclass         0x00       offset 0x0A
  capability ptr   0x98       offset 0x34   a list the device laid out
```

**The module contains none of those numbers.** It has no vendor identifier, no
device identifier and no class code to compare against; it reports what it read
and the harness decides what it means. The nucleus cannot decide either — it
performs a configuration transaction against the function a capability names and
has no idea what a VirtIO device is.

**And the values are the hardware's.** The same module, same nucleus, same
machine *minus the device* reports `vendor=0xFFFF` — an absent function reads
all-ones — so not one assertion above would hold without the real device present.

## 6. Negative authority evidence

All nine, and eight of them executed by canonical text against the real machine
rather than asserted from the shape of the contract.

`source/tests/vectors/pci-negative/init.tos` holds the same single bus
capability and then tries, one at a time, every way of reaching further than it
was granted. Each refusal sets one bit; the module reports **255**, and every
bit is a status the nucleus decided:

| # | Case | Evidence |
|---|---|---|
| 1 | no PCI authority → no configuration operation | **executed**: the same module under the canonical constant that endows nothing is `capability-denied` before its first instruction — stronger than a refused call, since there is no authority to refuse |
| 2 | function A cannot access function B | **executed** (bit 64): two claims, one device. There is no parameter through which to name another function, so each capability reads its own — the one with a device behind it and the one without return different values |
| 3 | `config_read` cannot `config_write` | **executed** (bit 16): the capability is attenuated to `config_read` alone and the write is refused `E_NO_CAPABILITY` |
| 4 | stale/released capability refuses | **executed** (bit 32): released, then used; refused by generation |
| 5 | offset outside conventional space refuses | **executed** (bit 2): offset 256 is `E_BAD_ARGUMENT`, and nothing is read |
| 6 | malformed width or alignment refuses | **executed** (bits 4, 8): width 3, and offset 1 at width 2 |
| 7 | forged scalar in a capability position refused | **by the independent verifier**, over the artifact, in `tests/integration/tests/interface_effects.rs` — including under a direct interface effect, which is where it matters most |
| 8 | a BAR value is not authority | **structural**: no operation of any accepted schema takes one. The module reads BARs as data and there is nothing to present them to |
| 9 | without the device, the positive proof fails | **executed**: the device-absent run reports `vendor=0xFFFF`, a different observation entirely |

Two more, added because ADR-0080 made them possible to get wrong: a claim of an
already-live assignment is refused `E_LIMIT` (bit 128), and a device outside its
architectural range is refused `E_BAD_ARGUMENT` (bit 1).

Handle and refusal ordering is unchanged: index bounds, generation, type, rights.

## 7. The second STOP, and how it was resolved

Found while implementing the first, verified against the checker, reported
rather than worked around — and then decided.

**The wall.** `SYSTEM_INTERFACE_V1` §4.1 and ADR-0061 made an `extern`'s `uses`
name an `import capability` binding of the enclosing module, and the frontend
enforced it. So a module calling `pci_config_read` had to write
`import capability platform.pci.FunctionConfig as f;` — a request nothing can
answer, because the only lawful producer is the claim, which runs after startup.
A parent could not place one in a child's plan either: `endow_for_launch` on that
interface is itself an operation on it. The recursion had no base case, and it
was not about PCI.

**The decision.** ADR-0080 separates two things that had been accidentally
identical while an import was the only way to hold authority:

```text
import capability   requests a capability, binds a name, is answered or denied
uses [...]          declares which interfaces a function may exercise
```

TOS Core **1.1** admits `uses [interface.path]`. It requests nothing, implies no
instance, and adds nothing to the capability table; the operation still needs a
capability value at the call site, the verifier proves its exact nominal type
against the artifact, and the nucleus proves rights and liveness at the call.

**What was rejected**, and stayed rejected: an "endow a function through the Bus"
operation, and moving the configuration operations back onto `platform.pci.Bus`.
Both would have made PCI a special case in a frontend that should not know what
PCI is.

**The consequence worth naming.** Effect identity is now the interface path
everywhere — which it already was in the IR, since ADR-0060. A function
declaring `uses [a]` may therefore exercise a different binding `b` of the same
interface, which was previously refused. That **widens** what is accepted and
reinterprets nothing: the artifact could never tell the two apart, so the old
refusal was a frontend rule with nothing below it to enforce it.

**1.0 is unchanged.** Every 1.0 module keeps its meaning and its digest, a 1.0
module using a 1.1 form is `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`, and a
frontend implementing only 1.0 refuses a 1.1 module whole by its header with
`E1602`. The artifact records the version the module *declared*, so two minors
never share one identity.

## 8. The liveness prerequisite, unchanged

`SYSTEM_ABI_V1` §6 states the rule as "nothing runnable **and nothing routed can
change that**"; `nucleus/src/process.rs` implements only the first half and says
so in its own comment. Stage 4A routes no interrupt and needed none, so nothing
here touches it. It is a **mandatory prerequisite to the first routed device
interrupt** and a blocking item for the IRQ slice.

## 9. The Stage 4 platform profile, as built

An extension of the ADR-0040 base platform, never a change to it: it is reached
only through `run.sh --stage4-block-device`, and every Stage 1–3 gate runs the
same profile it was measured on.

```text
machine        q35                       unchanged from ADR-0040
cpu            qemu64                    unchanged
vcpus          1                         unchanged
memory         256 MiB                   unchanged
accelerator    TCG                       unchanged
firmware       the declared OVMF build of the Stage 1 gate

device         virtio-blk-pci
transport      disable-legacy=on, disable-modern=off — modern VIRTIO PCI, the
               transport Stage 4 intends to continue with rather than the one
               whose configuration space is easier to read
pci location   addr=0x4, so evidence names 00:04.0 rather than whichever slot
               enumeration happened to find
backing        raw, 16 MiB, zero-filled and deterministic
num-queues     1, recorded because feature negotiation is part of the surface
               QEMU exposes
iommu_platform off, and recorded because the later DMA contract must not change
               when it becomes on (docs/11 §DMA)
observed on    QEMU 10.0.11 (Debian 1:10.0.11+ds-0+deb13u1)
```

## 10. One defect this round found in an existing boundary

`LAUNCH_VERSION` and `BUNDLE_LAUNCH_VERSION` are **one discriminator space**, not
two: `runtime_entry` reads the first word of the launch record and decides *which
record shape it is holding* from that word alone. Bumping `LAUNCH_VERSION` from 4
to 5 collided with `BUNDLE_LAUNCH_VERSION`, and the failure was not a version
disagreement failing closed — every ordinary launch record was read as a bundle,
producing a page fault at the entry stub and a runtime image 890 KiB smaller than
it should be, because the unreachable path had been optimised away.

`LAUNCH_VERSION` is therefore **6**, and both constants now carry the rule: a
number must be free in *both* sequences. The trap is worth naming because the
symptom pointed away from the cause — a size change and a fault in the entry
stub, from a constant that looked like ordinary versioning.

## 11. Gates

| Gate | State |
|---|---|
| *QEMU textual PCI function claim* | **green** — the real read, the device-absent differential, eight executed negatives and the startup denial |
| `interface_effects` (new) | **green** — the ADR-0080 chain through the independent verifier |
| every Stage 1–3 gate | **unchanged and green.** No harness, budget or profile was modified, and the Stage 4 device is opt-in |
| Stage 4 identity gate | **not claimed.** Discovery is not persistent data. What Stage 4A establishes is the authority boundary and the first hardware-facing act across it |

## 12. Closure

Stage 4A was formally closed by the Project Architect on **2026-09-04**, for
evidence commit **`2655aaa3d7bd0993c5bbfe0da168d2dd1c44641d`**. The ruling is
archived verbatim in
`source/legal/publication-records/2655aaa3d7bd0993c5bbfe0da168d2dd1c44641d-stage4a-closure-approval.md`.

The ruling accepts this document and its two decisions — ADR-0079 and ADR-0080 —
as the evidence basis: root Bus authority originating at the platform/launcher
boundary and unmintable by ordinary runtime code, `PciFunction` naming exactly
one live assigned function, BDF scalars as data rather than authority, exclusive
generation-bound assignment, configuration access requiring the exact live
capability and rights, the real values read from the device, the device-absent
differential, the gated negatives, the forged scalar refused by the independent
verifier, and TOS Core 1.1's separation of startup capability requests from
interface effect declarations. It states that **no Stage 1–3 gate was weakened**.

It closes the authority boundary and configuration access only. It approves and
implies **no BAR/MMIO mapping, device-memory semantics, IRQ, DMA, IOMMU, device
reset, VirtIO queue setup, block I/O, persistent storage or repository handoff.**
BAR/MMIO and device memory were decided separately and later, by ADR-0081 under
the Stage 4B closure.

The §8 liveness prerequisite crosses this closure unchanged and remains
mandatory before the first routed device interrupt, and the Stage 4 identity gate
above remains unclaimed.
