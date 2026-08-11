<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0040: the Stage 2 reference platform profile

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — fixes the platform a Stage 2 performance gate is measured
  on, which every later performance claim is stated against
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

`docs/35` gives Stage 2 two budgets for the bootstrap profile — parse,
type-check, lower and verify a 256 KiB canonical module within 500 ms p95, and
execute the standard one-million-operation integer and control-flow benchmark
within ten times "the host reference interpreter time under the same semantic
implementation" — and an evidence ladder where P1 is locally measured and no
stage closes on P0 for its own metric.

It names a reference platform for Stage 1: the mandatory
q35/qemu64/one-vCPU/256-MiB/TCG functional profile. It names none for Stage 2.

Without one, "reference platform" would end up meaning whichever machine
produced an agreeable number, which is measurement chosen after the fact. A
platform has to be fixed before a measurement is taken for the measurement to
mean anything.

## Decision

### 1. The Stage 2 reference platform is the Stage 1 profile

Stage 2 performance evidence is taken on the same profile Stage 1 already
mandates:

```text
machine      q35
cpu          qemu64
vcpus        1
memory       256 MiB
accelerator  TCG (no hardware virtualization)
firmware     the declared OVMF build of the Stage 1 gate
```

One platform for both stages, for three reasons. It already exists and is
already gated, so no second environment has to be kept honest. TCG on one vCPU
is deterministic enough that two runs are comparable. And a single profile keeps
Stage 1 and Stage 2 numbers comparable, which they would not be if each stage
picked its own.

A record taken here demonstrates conformance **on this declared platform**. It
says nothing about performance on other hardware or other emulators, and it is
not evidence that a budget met here is met anywhere: a different CPU, a
different accelerator or a different memory system can be faster or slower for
reasons this profile does not model. The value of a fixed platform is
comparability across runs and across stages, not extrapolation.

### 1a. The measurement must run the real Stage 2 path

The reference measurement executes the actual Stage 2 TOS runtime and recovery
path. Running `tos-engine` inside an arbitrary Linux or host guest under the
profile does **not** satisfy this gate: the guest's libc and host OS would
become a runtime dependency of the measured path, which is exactly the
dependency `docs/44` says is not a recovery or runtime dependency. A number
produced that way would measure the host, and would let a host runtime enter the
Stage 2 story through the performance gate.

Native-host execution remains admissible as the **comparison baseline** of
section 2 — that is what a baseline is for — and is never the production or
reference execution path.

The freestanding runtime this requires is the subject of the runtime-
independence audit; until it exists, the reference half of the pair cannot be
taken, and that is an open gate rather than a number taken elsewhere.

### 2. What "host reference interpreter time" means

`docs/35` states the execution budget as a ratio against "the host reference
interpreter time under the same semantic implementation". That is the **native
host** execution of the **same** reference interpreter — the same
`tos-engine`, the same commit, the same workload — not a second semantic
implementation.

So the execution metric is a pair of measurements and their ratio:

```text
reference   the benchmark under the profile of section 1
native      the same benchmark, same commit, same engine, on the native host
ratio       reference / native            budget: at most 10
```

This is deliberately written down because the sentence admits a second reading —
a different implementation of the same semantics — and building one purely to
satisfy a ratio would be a worse outcome than stating which reading holds.

### 3. Sampling and retention

Unchanged from `docs/35` and restated so a record is checkable: three warmups,
twenty-one samples, median, p95 and p99 retained with the raw samples, the
source commit, the toolchain and build identity, the profile, and the cache
state. Both halves of the ratio are recorded, never just the quotient.

### 4. Evidence level

A record taken under section 1 with section 3's retention is **P2** when it is
produced by the repository's own reproducible gate, and **P1** when produced by
hand. A record from any other machine is P1 and names the machine; it may
support a stage's progress but does not close its gate.

### 5. What this ADR does not decide

It does not set new budgets, change the numbers in `docs/35`, or say anything
about a Full-profile or multicore reference platform. Those need their own
decision when a Full engine exists.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended.
- **Canonical representation:** unchanged. **Trusted-base impact:** none.
- **Threat-model impact:** none directly; a fixed platform makes a resource-
  exhaustion measurement comparable across runs, which the abuse evidence in
  `docs/44` section 3 depends on.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no gate is claimed or closed by this ADR. It states
  where the Stage 2 performance gate is measured, not that it has been.
- **Compatibility profile:** the profile is fixed for Stage 2 and changes only
  through a versioned decision, because changing it silently would invalidate
  every retained comparison.
- **New dependencies:** none. Both halves of the ratio use tooling the
  repository already has.
- **Tests:** the harness records which profile it ran under and refuses to
  present a P2 claim for a record taken elsewhere. A reference-profile record is
  admissible only from the real Stage 2 runtime path of section 1a.

## Consequences

The Stage 2 performance gate has a place to be measured, chosen before the
measurement rather than after it, and the execution budget has one reading. The
remaining work is mechanical: run the harness under the profile, retain both
halves, and compute the ratio.

The cost is that a Stage 2 performance number is a TCG number, so it is slower
than the hardware a developer sits at, and it is a statement about this platform
only. That is the intended trade: the alternative is a number whose platform was
chosen to suit it, which states nothing at all.

## Alternatives considered

**Declare the maintainer's machine the reference platform.** Rejected: it is
choosing the platform after seeing the result, and no one else can reproduce it.

**Define a new Stage 2-specific QEMU profile.** Rejected for now: a second
profile is a second thing to keep gated and honest, and nothing about the Stage 2
metrics needs anything the Stage 1 profile lacks.

**Build a second semantic implementation to satisfy the ratio.** Rejected: it
would exist only to produce a denominator, and a throwaway implementation is a
worse oracle than none. The native-host reading of section 2 measures what the
budget is actually about — how much the reference platform costs — using the
implementation that already exists.
