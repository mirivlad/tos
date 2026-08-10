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
