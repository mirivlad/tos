<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0041: `RuntimeMemoryGrantV1` — the nucleus-to-runtime memory contract

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — introduces a versioned interface between the nucleus and
  the Stage 2 reference runtime, and fixes how implementation memory is
  distinguished from a module's declared resource envelope
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

The runtime-independence audit found one genuine gap. The Stage 2 crates use
only `core` and `alloc` facilities, and the freestanding target already builds
and is gated — but `alloc` needs a `#[global_allocator]`, the nucleus has none
and does not use `alloc` at all, and no accepted document says who owns memory
in Stage 2 before the Stage 3 process substrate exists.

Until that is settled the `docs/44` claim — that libc, the C ABI and host
threads are not runtime or recovery dependencies — cannot be discharged, the
ADR-0040 reference measurement cannot be taken on the real path, and Stage 2
cannot be candidate-complete.

## Decision

### 1. The nucleus grants; the runtime never discovers

The nucleus already owns the physical and virtual memory mechanism before Stage
3. It hands the Stage 2 reference runtime **one bounded region**, and that
region is the runtime's only heap backing store.

```text
RuntimeMemoryGrantV1 {
  version           the grant contract version
  base              start of the granted region
  length            bytes granted
  alignment         guaranteed alignment of `base`, a power of two
  identity          which nucleus build produced the grant
}
```

The runtime does **not** probe a memory map, walk firmware tables, or acquire an
ambient allocator. It receives a base and a length or it does not run. Discovery
is the nucleus's job and stays there.

The granted memory comes from memory the nucleus already legitimately owns or
has reserved from a validated memory topology. Explicitly not: host `malloc`,
libc, UEFI allocation after runtime handoff, a hidden C ABI, or a Stage 3
process allocator.

### 2. `BootInfo v1` is not touched

`BootInfo v1` is the loader-to-nucleus contract. Its size, version and reserved
rules, and the Stage 1 evidence that pins them, stay exactly as they are.

`RuntimeMemoryGrantV1` is a **different** interface — nucleus to Stage 2
runtime — with its own version. Widening `BootInfo v1` to carry it would change
a contract Stage 1 closed on, and would do it for a consumer that did not exist
when it was written. Two interfaces with two versions is the honest shape.

### 3. Two limits that must never be confused

**Implementation heap capacity** is `RuntimeMemoryGrantV1.length`: the physical
memory the parser, lowerer, verifier and interpreter have as an implementation.

**A module's resource envelope** is `resource [allocation: ...]`: the semantic
budget of the TOS program being executed, already enforced by the engine before
the effect.

These are separate quantities with separate failures, and neither may stand in
for the other:

- a module declaring `allocation: 4KiB` gets 4 KiB of semantic budget, never a
  claim on the whole arena;
- exhausting the implementation arena must **not** be reported as that module's
  `RUNTIME_ALLOCATION_LIMIT`, because it is not a fact about the module;
- exhausting a module's declared budget must not be reported as an
  implementation failure, because the program is the thing at fault.

### 4. Allocation failure discipline

A module that spends its declared budget gets a defined resource refusal before
the effect. That is settled and already implemented.

Exhausting the **implementation** arena on valid input inside the published
`docs/44` hard limits is a different matter, and it may **not** be an ordinary
panic or halt. One of the following, or both:

- fallible allocation — `try_reserve` and its equivalents — so the runtime
  refuses the work rather than dying; or
- a proved upper memory bound for the published limits, and an arena at least
  that large.

`alloc_error_handler` may halt, but only as an implementation-invariant failure
— the equivalent of an assertion — never as the ordinary response to
attacker-controlled input that is valid and within bounds.

### 5. The allocator itself

A bump allocator that leaks irreversibly between ordinary operations is **not**
accepted unless a lifetime or reset contract proves bounded long-term behaviour.
A reference runtime that must be restarted to reclaim memory is not a recovery
oracle.

The preferred shape is a small, auditable, bounded allocator with real reclaim,
or an equivalent scheme whose long-term behaviour is proved. It should remain
useful as the permanent recovery and reference-runtime allocator after Stage 3
arrives, rather than being a shim to discard.

Every new `unsafe` site is minimized, documented with its SAFETY invariants,
entered in the unsafe inventory, and covered by adversarial tests.

### 6. Evidence this decision owes

- the grant is a declared input: a runtime given no grant runs nothing;
- repeated executions do not grow arena use without bound;
- a reset or recovery path returns the allocator to a documented state;
- arena exhaustion on valid bounded input is a refusal, not corruption and not a
  halt;
- module-envelope exhaustion and arena exhaustion are distinguishable in the
  diagnostic record;
- the freestanding artifact has no dynamic dependency.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. I-15 is served:
  who owns Stage 2 memory stops being unstated.
- **Canonical representation:** unchanged. **Trusted-base impact:** the nucleus
  gains one grant responsibility; it already owns the mechanism.
- **Threat-model impact:** positive. An arena with a declared length bounds what
  a malicious module can make the implementation consume, and separating the two
  limits stops one from masking the other.
- **Recovery impact:** positive, provided section 5 holds — a reference runtime
  that reclaims is usable as a recovery oracle.
- **Stage identity gate:** none claimed. This unblocks the Stage 2
  runtime-independence evidence; it does not supply it.
- **Compatibility profile:** `RuntimeMemoryGrantV1` is versioned from the start
  and changes only through a versioned decision.
- **New dependencies:** none. No libc, no WASI, no C ABI, no host shim.
- **Tests:** section 6, plus the freestanding build gate and the unsafe
  inventory.

## Consequences

The `no_std` conversion and the freestanding runtime become ordinary bounded
work, and the ADR-0040 reference measurement gets a real path to run on. The
cost is one new versioned interface and one allocator to write and audit — both
things the system needs regardless of Stage 2.

## Alternatives considered

**Widen `BootInfo v1`.** Rejected: it changes a contract Stage 1 closed on, for
a consumer that did not exist when it was written.

**No allocator; fixed-capacity storage everywhere.** Rejected: it rewrites the
components that most need to stay reviewable, and makes worst-case memory the
always case.

**Wait for Stage 3 to own memory.** Rejected: Stage 2's identity question is
whether actual language semantics execute, and with a host runtime underneath,
that execution is a host execution.
