<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS threat model

## Status and scope

This is the normative architectural threat model for TOS. It describes assets, adversaries, trust boundaries, required properties and accepted non-goals. It does not claim that any property is implemented until a stage report names evidence.

The model applies first to the x86_64 UEFI/QEMU profile and expands through ADRs for physical hardware, networking and multi-user deployment.

## Security objective

TOS aims to make system behavior constrained, attributable, inspectable and recoverable while preserving the owner's authority to run modified source.

Readable source is an observability property, not a security boundary. Git history is an attribution and recovery mechanism, not proof of correctness. Signatures prove authorization and integrity, not safety.

## Protected assets

### A1 — Canonical system identity

The selected commit, its `/system` tree and the mapping from running modules to source objects must not be silently substituted.

### A2 — Recovery authority

The owner must retain a protected path to select last-known-good or recovery state after a failed or malicious candidate.

### A3 — Capability integrity

A process must not forge, widen or transfer authority outside explicit rules.

### A4 — Nucleus integrity

The binary trusted base, boot ABI and protected primitives must not be modified or driven into undefined behavior by untrusted input.

### A5 — Repository integrity

Objects, refs, activation records and retention roots must resist corruption, ambiguity, rollback confusion and unauthorized mutation.

### A6 — Source-to-runtime provenance

Derived IR, bytecode, native cache and capsule content must be bound to exact source and toolchain identities.

### A7 — Mutable state and secrets

`/state`, `/home`, `/secrets`, `/cache` and `/run` must not be confused with canonical `/system`, leaked through commits or rolled back without explicit policy.

### A8 — Owner control

A vendor, signer or update service must not convert trust policy into permanent denial of owner-authorized boot.

### A9 — Availability within declared limits

The system should contain faults and resource exhaustion according to declared budgets. Absolute denial-of-service resistance is not promised.

## Adversary classes

### T0 — Accidental defect

Malformed input, buggy source, interrupted writes, driver errors, operator mistakes and incompatible state migrations.

### T1 — Unprivileged application

Controls its own source and data, sends arbitrary permitted IPC, attempts capability abuse, resource exhaustion or information disclosure.

### T2 — Malicious textual service or driver

Possesses its granted capabilities and may intentionally misuse them, crash, lie about health, corrupt shared buffers or exploit nucleus interfaces.

### T3 — Malicious language frontend or derived cache producer

Attempts incorrect lowering, source-map forgery, verifier confusion, cache substitution or hidden behavior absent from canonical source.

### T4 — Malicious repository or remote

Supplies crafted object graphs, hash collisions where feasible, excessive recursion/delta chains, misleading refs, rollback commits, bad signatures or resource-exhaustion inputs.

### T5 — Local attacker with mutable-storage access

Can rewrite ordinary disk blocks or boot-control storage but does not initially possess trusted signing keys or arbitrary firmware execution.

### T6 — Supply-chain adversary

Compromises compiler, builder, dependency, firmware, signing process, generated source or release infrastructure.

### T7 — Physical/firmware adversary

Controls firmware, DMA-capable hardware outside isolation, debug interfaces or physical memory. Early TOS does not claim full protection against this class.

### T8 — Nucleus compromise

Arbitrary execution in the trusted nucleus. This is outside containment guarantees; recovery and independent verification may still detect or repair persistent consequences.

### T9 — Vendor or project authority acting against owner control

Uses signing, update, trademark or recovery policy to prevent the owner from running modified source. Official TOS architecture must resist this as a governance and design threat.

## Trust boundaries

1. firmware to loader;
2. loader to boot protocol and nucleus;
3. arbitrary capsule bytes to capsule parser;
4. repository bytes to object parser/verifier;
5. source text to language frontend;
6. frontend output/cache to IR verifier;
7. nucleus to user-space service through capability and IPC boundary;
8. driver to device through MMIO, interrupt and DMA grants;
9. active commit to writable overlay;
10. system repository to mutable state and secrets;
11. recovery authority to candidate activation;
12. local system to remote repositories and time/signature services;
13. canonical `/system` source to external vendor-controlled opaque material in
    `/vendor`.

Every implementation crossing a boundary names its input format, validation, authority, resource limits and failure behavior.

## Required security properties

### S1 — Fail closed on identity ambiguity

Unknown hash algorithms, unsupported format versions, duplicate normalized paths, ambiguous source mappings or unverifiable caches are rejected rather than guessed.

### S2 — Bounded parsing

Boot, repository, IPC, language and IR parsers must have bounded recursion, allocation and work or must enforce explicit quotas before processing attacker-controlled input.

### S3 — No ambient privilege

Authority originates from explicit capabilities. Configuration text may request authority but cannot grant it to itself.

### S4 — Capability attenuation

Delegation cannot create greater authority than the delegator possesses. Rights and object identity are both checked.

### S5 — DMA confinement

Drivers receive only explicitly mapped DMA regions and device resources. IOMMU absence or limitations are reported as a weaker security profile, not hidden.

**On the no-IOMMU reference profile, said plainly rather than implied**
(ADR-0082 §5). A routed interrupt necessarily makes its function a bus master —
an MSI-X message *is* a memory write the device issues — so this is reachable
from Stage 4C onwards and not only from a future DMA stage:

> **With no IOMMU, TOS cannot claim hardware-enforced confinement of a malicious
> bus-mastering device.** The capability model controls which sanctioned DMA
> objects and device-visible addresses software may **obtain**; it does not
> physically prevent a malicious driver from programming a bus-mastering device
> with some other address.

Two consequences, which are stated together and never separately:

- **sanctioned DMA authority and device-visible address issuance** require
  *both* memory funding authority and the live device assignment, and that is
  mechanically enforced;
- **hardware DMA confinement** is not provided by this profile, and no sentence
  of any contract may imply that it is.

It is therefore false, on this profile, to say that possession of only a
`PciFunction` makes arbitrary RAM physically invisible to the device. What the
capability model bounds is which addresses a driver can obtain *legitimately*,
not which addresses the hardware will accept. An IOMMU backend later strengthens
confinement **without changing the public DMA object model**, which is why that
model must not be written in terms of identity-mapped physical addresses.

### S6 — Verified derived execution

No IR or executable cache runs solely because it has a plausible filename or local origin. Identity, schema and verifier checks are mandatory.

### S7 — Transactional protected state

Candidate, current, last-known-good and recovery selection cannot enter an unrecorded half-updated state after expected interruption.

### S8 — Recovery independence

A failed active system must not be required to repair itself. Recovery has separately protected code, boot selection and minimum repository inspection.

### S9 — Mutable-state separation

Ordinary runtime writes cannot silently alter `/system`. Rollback of source does not silently reinterpret incompatible state without migration policy.

### S10 — Observable trust state

Production, community, owner-authorized and research modes are distinguishable in process identity, boot records and user-visible diagnostics.

### S11 — Owner-authorized boot

Official profiles provide a documented local recovery path for owner keys or explicitly authorized unsigned experimental commits.

### S12 — Audit without secret disclosure

Security-relevant events identify actors, source and capabilities while redacting secret material by construction.

## Threats by subsystem

### Boot and capsule

Threats include corrupted lengths, integer overflow, duplicate paths, fake source commit, capsule rollback and mismatch between nucleus ABI and source. Controls include deterministic format, whole-object digest, bounded parser, explicit compatibility fields, protected boot record and corruption tests.

### Language and runtime

Threats include parser differentials, nondeterministic lowering, type confusion, unbounded compile time, source-map forgery and malicious frontend behavior. Controls include normative grammar/semantics, sandboxed frontends, deterministic inputs, independent verifier, resource accounting and cross-engine conformance.

### IPC and capabilities

Threats include handle forgery, confused deputy, stale-handle reuse, schema confusion, queue exhaustion and unauthorized delegation. Controls include typed generation counters, explicit transfer, schema versions, quotas and audit identity.

### Drivers

Threats include malicious MMIO, DMA outside granted memory, interrupt storms, malformed device descriptors, stale completion and service starvation. Controls include user-space isolation, IOMMU profile where available, bounded queues, device reset, watchdogs and performance/resource contracts.

### Repository and activation

Threats include crafted object graphs, malicious packs, unauthorized ref movement, rollback to vulnerable commit, garbage collection of recovery objects and state/source incompatibility. Controls include compatibility profiles, bounded traversal, protected refs, signed or owner-authorized policy, retention roots, candidate health and migration declarations.

### Remotes

Threats include credential theft, malicious server data, downgrade, replay, time confusion and partial fetch. Network support must add transport-specific threat entries before Stage 7 closes.

### External vendor material

Threats include substitution of a declared vendor object, downgrade to a
vulnerable firmware version, silent acceptance of a missing or mismatched
object, opaque material shadowing a component required to be textual, and vendor
material being presented to the owner as inspectable TOS source.

Controls are identity-level only: declaration in canonical source with vendor,
version and content hash; hash verification before use; defined behavior on
absent, mismatched or refused objects; the placement rule keeping `/vendor` out
of `/system`; and the owner-facing boundary report required by ADR-0030.

TOS does not analyze what a vendor object does. The controls constrain which
bytes are loaded and whether the owner can see that they were loaded — not their
behavior once running. This limit is stated rather than mitigated, and T7 remains
the governing adversary class.

## Stage 3 — the capability, IPC and process boundary in detail

Trust boundary 7 — nucleus to user-space service through the capability and IPC
boundary — is created by Stage 3 under ADR-0048…0051. The subsystem paragraph
above names the threat families; this section is the detail the change rule
requires before Stage 3 can close. Adversary classes, assets and required
properties are the existing ones; nothing here adds a class or an asset.

### X3.1 — Authority acquired without a grant (T1, T2 → A3, S3)

A process obtains authority it was never granted: forging a handle, guessing an
index, re-encoding a handle's bits into a value, reusing a released handle after
its slot is recycled, or presenting a handle of one type where another is
expected.

Controls: handles are process-local indices into a nucleus-owned table with
generations; validation checks range, generation, type and rights
(`interfaces/system/CAPABILITY_V1.md` §2). Negative tests: that contract §7.1–3.
Evidence level target at closure **E3** — the guessing and staleness cases are
fuzzable and must be fuzzed, not argued.

### X3.2 — Authority widened by attenuation (T2 → A3, S4)

Attenuation returns a capability with more rights, wider scope or longer
lifetime than its input, through an arithmetic or subset-check defect.

Controls: the nucleus computes and checks the subset relation in all three
dimensions, and no operation adds a right (`CAPABILITY_V1` §4). Negative test:
§7.4, generated over right/scope pairs rather than a hand-picked few. **E3**.

### X3.3 — Confused deputy (T1, T2 → A3, A8)

A weak client persuades a broker holding a strong capability to act on an object
the client cannot name. This is the failure that survives when the mechanical
capability tests all pass.

Controls: a broker acts only on objects named by capabilities the client passed;
refusal is attributable to the client in the audit record (`CAPABILITY_V1` §7.6;
docs/37 Stage 3 evidence). **E2** at Stage 3 close, **E3** once a second broker
service exists to test against.

### X3.4 — Isolation breach between processes (T1, T2 → A4, A9)

A process reads or writes another process's memory or the nucleus's: addressing
outside its grant, racing a region transfer, or reading frames that belonged to
a dead process.

Controls: hardware address spaces (ADR-0048); grants bounded and
generation-tagged with frames cleared before reuse (ADR-0050 §3); linear region
transfer unmaps at the sender (`interfaces/system/IPC_V1.md` §5). **E2**, with
the frame-reuse case at **E3** because it is the one that fails silently.

### X3.5 — Denial of service through the nucleus (T1, T2 → A9)

A process fills queues to grow nucleus memory, holds a receiver blocked forever,
spins without entering the ABI, or makes capability validation expensive by
holding many capabilities.

Controls: bounded queues with visible backpressure and no allocation to accept a
message (`IPC_V1` §7); timer preemption independent of process cooperation
(ADR-0049); constant-time validation in the holder's capability count
(`CAPABILITY_V1` §5); every blocking operation cancellable
(`interfaces/system/SYSTEM_ABI_V1.md` §6). **E2**.

Explicit Stage 3 non-goal: fair-share scheduling and priority-inversion control.
Round-robin within one band is what ADR-0049 fixes.

### X3.6 — The system ABI as an attack surface (T1, T2 → A4, S2)

A process drives the ABI with out-of-domain arguments, unknown operation
numbers, or addresses it hopes the nucleus will dereference.

Controls: a closed status space; no operation dereferences a process-supplied
address; buffers are named by region handles; an unknown operation returns
`E_NOT_SUPPORTED` rather than being ignored (`SYSTEM_ABI_V1` §3, §4, §7).
**E3** — the ABI is the one Stage 3 surface taking wholly untrusted input in
registers, and it is fuzzable exactly as the capsule parser was.

### X3.7 — Escalation by process creation (T2 → A3, A8)

A service creates a process to obtain authority it does not hold, or grants a
child more than it holds itself.

Controls: `process_create` requires a process-authority capability ordinary
services do not hold; a launcher cannot install a capability it does not itself
hold, and the granted set is asserted by the nucleus rather than by the
launcher's claim (`SYSTEM_ABI_V1` §5,
`interfaces/system/PROCESS_IDENTITY_V1.md` §3). **E2**.

### X3.8 — Identity forged by the process it describes (T2 → A6, A1, S12)

A process reports a module digest, source set or capability set it does not
have, and the false claim reaches the audit record.

Controls: the audit record is the launch record, asserted by the holder of
`process_create` from the bytes it passed; self-reports are separately
identified and never the audit record; disagreement is itself an event
(`PROCESS_IDENTITY_V1` §2, §6, negative test §7.2). **E2**.

### X3.9 — A commit identity the system never read (T2, T6 → A1, A6, S1)

A Stage 3 process is recorded as belonging to a system commit although Stage 3
has no repository and read no commit, and the false record propagates into
activation and rollback reasoning later.

Controls: the system commit id is **absent** for capsule-launched processes and
is asserted absent by test, not left to convention (`PROCESS_IDENTITY_V1` §5,
§7.5). **E2**.

This is the Stage 3 form of the failure Stage 1 was built to prevent — an
invented official commit. It is cheap to introduce by accident and expensive to
detect later.

### X3.10 — Privileged policy migrating into the nucleus (T0 → A1, A8)

Service logic moves into the nucleus because IPC is inconvenient, and the system
becomes a conventional microkernel with textual decoration.

Controls: a design threat, checked as docs/31 checks it — a dependency and
surface inventory at Stage 3 close showing that no service logic entered the
nucleus and that every privileged behaviour is exercised by a source-identified
textual process. **E1**, honestly: a reviewable property, not a tested one.

### X4.1 — Interrupt routing taken by writing a number (T2 → A3, A8, S3)

A process that may map a device's memory or write its configuration space
programs the structure that decides where an interrupt goes: an MSI-X table
entry carries a message address and a message data word, so writing one is
choosing which interrupt is delivered to which vector — authority nobody
granted, taken by writing a number. The same holder could relocate the BAR the
table lives in, moving it out from under any rule stated over the old address.

Controls (ADR-0082 §5, §5a–§5f): interrupt authority is a capability derived
from a live function assignment and from nothing else; a window overlapping the
MSI-X table or its pending-bit array is refused in both map forms; a
configuration write touching the MSI-X or the conventional MSI capability is
refused; a write that would change a resource-placement register of the reported
header type, or Memory Space Enable, or Bus Master Enable, is refused, judged
byte by byte and bit by bit so that writing back an unchanged value proceeds. No
operation of any accepted contract takes a vector, a GSI, an MSI address/data
pair or a BDF, so the escalation has nowhere to be expressed. **E2**: exercised
against the reference device, in both directions — each refusal, and each
neighbouring access that must still work.

### X4.2 — A device vector reused under a stale message (T7 → A9, S3)

A message emitted before an entry was masked arrives after that vector has been
given to another source, and is delivered to a process that never asked for it.

Controls (ADR-0082 §5f): **a CPU vector allocated to a routed device source is
retired for the rest of the boot and never returned to the allocator.** Every
other stale-authority property in this system is proved by a generation, and
this one cannot be — a stale MSI carries a vector and does not carry the
source's generation — so the conservative rule stands in for the proof. The
supply is finite and exhaustion is `E_LIMIT` rather than recycling. An interrupt
on a retired vector keeps its IDT gate, is acknowledged, is counted as spurious
and wakes nobody. **E2**.

### X4.3 — Memory returned to the pool while a device can still write it (T7 → A4, A9, S5)

A driver releases a DMA region; the frames go back to the allocator and are
handed to another process; the device still holds the address in a register and
writes into memory that is now somebody else's. Clearing Bus Master Enable is not
by itself a defence: it blocks new requests and says nothing about requests
already issued.

Controls (ADR-0084 §5b–§5d): frames are **quarantined** rather than returned, and
return only when the assignment is provably quiescent — no live bus-mastering
descendant, and a configuration read of the function's PCI Express Capability
observing **Transactions Pending = 0**, whose value proves no outstanding
non-posted request and whose completion flushes earlier posted writes. The
function's ordering and coherency bits are nucleus-owned so a driver cannot
weaken that proof. **Fail-closed**: if the proof cannot be established the frames
stay out, the charge stays outstanding and the assignment does not end, so the
device cannot be handed to a second driver. There is no timeout and no reset
fallback. **E1** until Stage 4C-2's evidence exists; the residual risk is
ADR-0084 §5c.1's TC0-only profile requirement, which is qualified about the
platform rather than checked in ring 0.

### X4.4 — One memory budget spent twice through quarantine churn (T2 → A9, S5)

A holder allocates a DMA region, releases it into quarantine, and allocates
again from a refunded budget — accumulating physically occupied frames above the
`MemoryAuthority` that was supposed to bound them.

Controls (ADR-0084 §5f): the allocation charge **stays outstanding** while the
backing is quarantined, and the funding lineage is refunded at the same moment
the frames actually return. This is ADR-0075's existing rule — physical
reclamation before accounting refund — with a longer interval, so
`allocated + reserved + free == budget` holds throughout. **E2** once the churn
case of ADR-0084 §8.12 is exercised, which a refund-on-release implementation
fails and every other item passes.

### What Stage 3 does not claim

- no protection against T7 or T8, which remain outside containment;
- no timing or micro-architectural side-channel protection between processes;
  clearing reused frames closes the direct-disclosure path and nothing more;
- no time source a process can trust — monotonic ticks exist for scheduling
  (ADR-0049) and trusted time is Stage 7;
- no revocation of already-delegated authority beyond what an owning service
  implements (`CAPABILITY_V1` §4).

## Accepted non-goals for early stages

- confidentiality or integrity against malicious firmware;
- verification of the internal behavior of vendor-controlled opaque material;
- protection from all physical attacks;
- availability against an attacker controlling granted device or CPU resources;
- formal verification of the complete system;
- secure multi-user isolation before the corresponding stage defines it;
- anonymous operation or traffic-analysis resistance;
- compatibility with arbitrary unsigned third-party binaries.

Non-goals must not be advertised as solved and must not weaken recovery or owner control silently.

## Security evidence levels

- **E0 design:** property exists only in documents;
- **E1 implemented:** code path exists and is reviewable;
- **E2 tested:** automated positive and negative tests exercise it;
- **E3 adversarially tested:** fuzzing/fault injection/red-team evidence exists;
- **E4 formally argued:** machine-checked proof or equivalently rigorous artifact exists for a named property.

Release notes state the evidence level for security claims.

## Stage mapping

- Stage 1: boot/capsule boundaries and source identity;
- Stage 1.5–2: parser, language, verifier, resource and source-map threats;
- Stage 3: capability, IPC and process isolation threats;
- Stage 4: interrupt, MMIO, DMA and storage-corruption threats;
- Stage 5: repository, refs, protected candidate/current/last-known-good/recovery
  selection, rollback, garbage collection and state migration threats;
- Stage 7: remote, network, credential and time threats.

A stage cannot close if its new boundary lacks a threat entry, negative tests and stated evidence level.

## Change rule

Any Level 2 or higher change must either update this document or identify the exact existing section that covers the new threat. “No security impact” is a claim requiring explanation.
