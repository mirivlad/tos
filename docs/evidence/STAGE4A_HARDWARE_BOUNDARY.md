<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4A — the hardware authority boundary, as built

- Status: **evidence, 2026-09-04.** The authority half of Stage 4A is built,
  green and gated. The configuration-read half is **blocked by a second,
  narrower architecture STOP** that this round found while implementing the
  first — see §7
- Decision: `docs/adr/0079-hardware-authority-origin.md`, **Accepted (Project
  Architect-approved, Vladimir Tomashevskiy, 2026-09-03)**
- Gate: `source/host-tools/qemu-test/pci-discovery.sh`, in preflight as
  *QEMU textual PCI function claim* (`qemu` profile, `full-only`)
- Covers: the platform root, the Bus → `PciFunction` derivation, the nucleus
  mechanism, `PLATFORM_INTERFACE_V1`, the Stage 4 device profile, what the
  textual module proved, and the two things that remain

## 1. What was built

The chain the ruling fixed, end to end, with the last link missing:

```text
boot/platform
    ↓  minted once by the launcher, scope and identity in the record
root platform.pci.Bus                       ← built, gated
    ↓  held by the canonical textual boot process
    ↓  pci_function_claim
platform.pci.FunctionConfig                 ← built, gated
    ↓  pci_config_read / pci_config_write
real configuration space                    ← nucleus side built; NOT reachable
                                              from text (§7), and therefore
                                              NOT EXERCISED at all
```

Everything above the last row is implemented, exercised from canonical TOS Core
on the real QEMU machine, and held by a gate. The last row exists in the nucleus,
in `SYSTEM_ABI_V1` and in `PLATFORM_INTERFACE_V1`, and **no module can declare
it**, for a reason that is not about PCI at all.

**Said plainly, because the distinction is easy to lose: no textual code has
touched hardware yet, and neither has anything else.** A claim is an authority
operation — it takes an exclusive assignment of an *address* within a bus
capability's scope, and it does not probe, read or otherwise consult the device
at that address. The CAM transaction in `pci.rs` is reachable only through
operations 25 and 26, which nothing calls, so it is **implemented and
unexercised**. Stage 4A therefore establishes the authority boundary and not the
hardware-facing act the identity gate is about.

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
| the CAM transaction (`0xCF8`/`0xCFC`) — **implemented, unexercised** | port I/O is unreachable at CPL 3 — IOPL is 0 and the single TSS admits no port. Exposing the pair would be ambient access to **every** function's configuration space, since it is one global window |
| the bus object and its scope | a root cannot be produced by an operation; something outside the ABI has to mint it, and the launcher is the only thing that runs before a process does |
| the assignment table | exclusivity is a property of the machine, not of one process's view of it; a service-held table could be bypassed by a second holder of the same bus |
| the offset/width bound | it is the thing standing between a caller and the hardware, so it lives where the hardware is touched |

**What the nucleus did not gain**, and this is the load-bearing half: no
enumeration, no device matching, no device class, no VirtIO, no driver
selection, no ACPI parser and no boot-ABI change. `pci.rs` cannot tell a block
device from a serial controller. It answers "read offset 8 of the function this
capability names".

## 5. The textual module, and what it proved

`source/tests/vectors/pci/init.tos`, canonical TOS Core carried through the
ordinary source → capsule → lower → TOSIMAGE → independent verifier → runtime
path. Its source identity is in the evidence:

```text
TOS.BOOTTEXT.DIGEST 7e326a4312940e02a964f5c1d75235bfeb1139b654c999d2ba1f5b1852ec14ba
TOS.IDENTITY source_kind=detached
             source_digest=225ec018c2bc87138b1f32b64518f9cb70cb7e4ac339560832a46635daa479fe
             capsule_digest=d71c071e1da75021fc22552e3354a12d44aeb8ceed68640279d56e50004cb4bd
```

It holds exactly one capability and asked for it by name and kind:

```text
TOS.RUN.REQUEST binding=bus interface=platform.pci.Bus object=9 wanted=9
TOS.RUN.CAPABILITY held=1 handle=0x100000000 object=9 rights=2048 binding=bus
```

It took exclusive assignments of two addresses in the real machine's bus scope,
and the nucleus named which. **Neither claim consulted the device**: one of the
two addresses has no device behind it at all, and it was assigned exactly as the
other was, because a claim is about authority over an address and not about what
is there.

```text
TOS.RUN.PCI_ASSIGNED process=0 segment=0 bus=0 device=4 function=0 generation=1 asserted_by=nucleus
TOS.RUN.PCI_ASSIGNED process=0 segment=0 bus=0 device=5 function=0 generation=1 asserted_by=nucleus
```

**The BDF in that record is the nucleus's, from the object it created.** A module
cannot put one there and could not name a different function if it tried.

And it reported four findings, no single answer producing the number:

```text
TOS.RUN.COMPLETED value=i64:15
```

| bit | what it required |
|---|---|
| 1 | an address was claimed out of the bus capability's scope |
| 2 | claiming it again was refused **`E_LIMIT`** — the assignment is exclusive |
| 4 | device 32 was refused **`E_BAD_ARGUMENT`** — outside the architectural range |
| 8 | a *different* function was claimed, so bit 2 was about the function and not about capacity |

Two of the four require a refusal the nucleus decides against its assignment
table and the architectural ranges; two require successes. A module that ignored
the answers could not produce 15.

## 6. Negative authority evidence

The ruling asks for nine. Six are proved; three depend on the blocked half and
are stated as unproved rather than quietly dropped.

| # | Case | State |
|---|---|---|
| 1 | no PCI authority → no configuration operation | **proved**, and more strongly than a refused call: the same module under the canonical constant that endows nothing is `capability-denied` before its first instruction, and no `TOS.RUN.PCI_ASSIGNED` appears |
| 2 | function A cannot read function B | **structural, not yet exercised.** No configuration operation accepts a BDF, so there is no parameter through which to try; the exercise needs §7 |
| 3 | `config_read` without `config_write` cannot mutate | **not proved** — blocked by §7 |
| 4 | stale/released function capability refuses | **not gated.** The path exists — `pci::release` advances the generation and `object_is_live` refuses — but the ring-3 exercise needs §7 |
| 5 | offset outside conventional space refuses | **not gated.** `pci.rs` carries an in-tree unit test, which no gate executes: the nucleus is `no_std` with its own panic handler and has no host test target, so its `#[cfg(test)]` modules are unrun — the same as `region.rs`'s. The gated exercise needs §7 |
| 6 | malformed alignment/width refuses | **not gated**, as row 5 |
| 7 | forged scalar in a capability position refused | **proved** by the existing verifier gate (ADR-0078 §7.7), which is interface-agnostic |
| 8 | a BAR value cannot be used as MMIO authority | **structural**: no operation of any accepted schema takes one. There is nothing to present it to |
| 9 | without the device, the positive proof fails | **not proved, and cannot be at this stage.** A claim succeeds for any address in scope whether or not a device is behind it, so nothing this gate asserts would change if the `virtio-blk-pci` were removed. The differential this case asks for needs a configuration read, which is §7 |

Handle and refusal ordering is unchanged: index bounds, generation, type, rights.

## 7. The remaining STOP: a runtime capability of an interface the module never imports

**Found while implementing, verified against the checker, and not worked
around.**

### The smallest unreachable operation

> The same textual module that claimed the function calls `pci_config_read` on
> it and receives the vendor and device identifiers the device reported.

### Why it is unreachable

`SYSTEM_INTERFACE_V1` §4.1 and ADR-0061 make an `extern`'s `uses` name an
`import capability` **binding of the enclosing module**, and the frontend
enforces it (`tos-core/src/boundary.rs`, `unavailable`): the first effect must
resolve to an import whose interface is the one the operation requires. So a
module that calls `pci_config_read` must write

```tos
import capability platform.pci.FunctionConfig as function;
```

That request must be answered before the first instruction or the process is
refused with `CapabilityDenied` (`tos-engine`: "Every request answered before the
first instruction, or none of them run"). **Nothing can lawfully answer it.** The
only producer of a function capability is `pci_function_claim`, which runs after
startup; and a launcher that pre-claimed one would be the nucleus choosing a
device, which ADR-0079 §5 names to be rejected.

The recursion has no base case. A parent cannot place a function into a child's
launch plan either: `endow_for_launch` on `platform.pci.FunctionConfig` is
itself an operation on that interface, so the parent needs the same
unanswerable import.

Verified empirically against the checker: with the import absent the declaration
is `E1801_FFI_NOT_AVAILABLE`; with it present the module checks clean and dies at
startup instead.

### This is the question ADR-0078 §6 recorded as open

> **It does not admit an interface a module never requested.** … A capability of
> an interface a module never imports — one delivered by a message, say — is a
> separate question and is not answered here.

Stage 4A is the first case to reach it. Every previous runtime-obtained
capability escaped it for a reason that does not generalise: a child
`system.process.Control` and a scoped `system.memory.Authority` are of interfaces
the module already imports for its own authority, and `LaunchPlanBuilder` and
`LaunchPlan` **declare no operations at all**, so nothing is ever reached
*through* one.

### The smallest decision surface

| Option | Consequence |
|---|---|
| **A — a runtime-sourced position needs no import, and `uses` may name the interface directly** | closest to ADR-0078 §4's own words, which already contemplate "the exact nominal interface check that has no import declaration to compare against". Requires deciding what `uses` means when it names an interface rather than a binding, and docs/42 §2's "enclosing `uses` effect" has to be read against that |
| **B — a fourth operation: place a function into a plan *through the bus*** | needs no language change. The bus service claims, then endows the child through its Bus capability; the driver imports `platform.pci.FunctionConfig` and is granted one. Cost: only a bus holder may delegate a function, so a driver cannot re-delegate — a policy asymmetry driven by a frontend rule rather than by the authority model |
| **C — declare the config operations on `platform.pci.Bus`, function as a value** | works today with no change at all. **Rejected here rather than recommended**: it makes reading require the Bus, which contradicts the ruling's "an ordinary device driver receives a `PciFunction`, never the root Bus authority" |

**Not chosen.** A is a change to an accepted language-boundary rule and B adds an
operation whose shape encodes a policy the Architect did not decide. Either is a
Level-2/3 decision, and the round's rule is to report rather than pick.

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
| *QEMU textual PCI function claim* (new) | **green** |
| every Stage 1–3 gate | **unchanged and green**; no harness, budget or profile was modified, and the Stage 4 device is opt-in |
| Stage 4 identity gate | **not claimed, and not partly claimed.** No persistent data moved, no configuration was read, and no textual code has yet performed a hardware-facing act. What is gated is the authority boundary that such an act will cross |
