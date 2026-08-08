<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0023: Stage 1 exception baseline

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — additive Boot ABI v1 terminal failure contract;
  BootInfo layout and loader-to-nucleus calling convention remain unchanged
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Decision

Before it reads or trusts BootInfo-controlled memory, the x86_64 nucleus
installs a nucleus-owned GDT/TSS foundation and an IDT for every architected
CPU exception vector 0 through 31. Entries are present 64-bit interrupt gates
at DPL 0. Maskable external interrupts remain disabled. Vectors above 31 do
not form an interrupt ABI in Stage 1, and an exception is fatal: Stage 1 does
not resume with `iretq`.

The TSS has a dedicated, fixed-size, nucleus-owned IST stack for #DF (vector
8). Its 16 KiB size is bounded and not derived from loader or capsule input.
This is fault containment, not an IRQ, APIC, scheduler, debugger, user-mode or
recoverable-fault subsystem.

The stable failure event is:

```text
TOS.EXCEPTION vector=<decimal> error=0x<hex> rip=0x<hex> cr2=<none|0x<hex>
```

`vector` is the exact x86_64 vector. `error` is the hardware error code where
the architecture supplies one and otherwise is normalized to zero. `rip` is
the CPU exception-frame RIP. `cr2` is the exact CR2 only for #PF (vector 14),
and literal `none` for every other vector. The handler allocates nothing,
panics nowhere, acquires no lock, emits the event and terminates through the
new stable `RESULT_EXCEPTION = 0x24`. With QEMU `isa-debug-exit`, that is raw
process exit 73. `TOS.PANIC`/`RESULT_PANIC` retain software-panic semantics.

This is an additive Boot ABI v1 failure path. Existing result/event meanings,
success ordering, BootInfo major/minor/size and the default production nucleus
artifact remain unchanged. An unknown non-success result or `TOS.*` failure is
failure, never a successful boot.

Isolated compile-time test features deliberately trigger #UD and #GP only
after the IDT has loaded. They build under distinct `CARGO_TARGET_DIR` paths
and the ordinary loader, BootInfo ABI, capsule/ESP builder, OVMF and QEMU
machine profile run them. The standard production artifact path is never
overwritten by a feature build.

## Architecture impact statement

- **Invariants:** I-02 and I-09 are strengthened by a bounded trusted-base
  containment mechanism and an additive versioned terminal result. No Tier 0
  invariant changes.
- **Canonical representation:** canonical installed source and capsule identity
  are unchanged; the BootInfo layout and success trace are byte-compatible.
- **Trusted base/dependencies:** GDT, TSS, IDT and minimal assembly stubs enter
  the existing no_std nucleus. No dependency is added.
- **Source-to-runtime and recovery:** source provenance and recovery/rollback
  are unchanged. Isolated test artifacts cannot replace the default artifact.
- **Threat/performance:** this contains CPU faults where architecturally
  possible; malicious firmware remains T7. The fixed stack and 32-entry tables
  introduce no attacker-sized allocation or measured-path traversal.
- **Compatibility/licensing/patents:** Boot ABI v1 gains one accepted terminal
  failure identifier/result. Code remains GPL-3.0-or-later; no external code or
  patent claim is introduced.
- **Evidence:** mechanical IDT/TSS/IST checks, ordinary QEMU exit 33, isolated
  #UD and #GP QEMU exits 73, and a production-artifact hash isolation check.
