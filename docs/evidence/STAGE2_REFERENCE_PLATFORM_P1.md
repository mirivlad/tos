<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 on the ADR-0040 reference platform — first measurements

Evidence level: **P1** (locally measured, docs/35).
Producer: `source/host-tools/qemu-test/stage2-reference-performance.sh`.
Platform: ADR-0040 — q35, qemu64, 1 vCPU, 256 MiB, TCG, OVMF.
Path: the real one. The workload is the capsule's canonical boot module and it
goes through reader, parser, checker, resolution, lowering, the independent
verifier and the bounded engine inside the guest, exactly as `init.tos` does.

ADR-0040 section 1a requires the reference measurement to be taken through the
Stage 2 runtime path rather than by a host process wearing the platform's name.
That was blocked until the runtime ran on the boot path; it no longer is, and
these are the first numbers from it.

## How the time is taken

Host-monotonic timestamps of the `TOS.RUN.*` events the boot already emits, as
their bytes arrive on the serial line. Serial transport is inside the measured
span. That is stated rather than corrected for: the correction would be a number
nobody measured.

One correction was needed in the harness and is worth recording, because the
first reading it produced was wrong in a way that looked plausible. The result
events — `TOS.RUN.VERIFIED`, `TOS.RUN.ACCOUNTING`, `TOS.RUN.COMPLETED` — are all
emitted *after* the run returns, so the span between them is the cost of
formatting three lines, not of executing anything. Measured that way, a million
operations appeared to take 241 µs. The execution boundary is the last
`TOS.RUN.STAGE` (the one announcing `execute`, emitted before the stage runs) to
the first result event.

## Measured

Workload: `module system.boot.init` with a `while` loop of 1 000 000 iterations,
returning the count so a wrong answer is a failed measurement rather than a fast
one. The guest reported `TOS.RUN.COMPLETED value=i32:1000000` and
`fuel=7000006/40000000`.

```text
frontend   TOS.RUN.BEGIN -> last TOS.RUN.STAGE        69 521 us   (~700-byte module)
execution  last TOS.RUN.STAGE -> TOS.RUN.VERIFIED  6 061 037 us   (1e6 operations)
whole boot TOS.NUCLEUS.ENTRY -> TOS.HALT           6 183 794 us
```

Per operation, execution is ~6 µs on the reference platform against ~0.31 µs for
the same shape of work natively (`docs/evidence/STAGE2_PERFORMANCE_P1.md`) — a
~20x factor, which is an ordinary TCG cost and not a surprise.

## What this does not yet close

**The docs/35 ratio is not computed here.** The native half was taken on the
harness's own fixture and this was taken on a boot module; they are not the same
workload, and dividing one by the other would produce a number that looks like a
ratio and is not one. A like-for-like pair — the same fixture measured both ways
— is the remaining work for that gate.

## The finding that matters more

A **256 KiB module — the published source-unit ceiling — did not finish the
frontend within 900 seconds** on the reference platform. The same module takes
137 ms natively, and the platform's ordinary factor is ~20x, so ~3 s was the
expectation. Four orders of magnitude is not a platform cost; it is a defect in
how the implementation scales.

The cause is identified and is in this repository rather than in QEMU.
`BoundedHeap::try_allocate` is first-fit by walking **every** block from the base
of the arena — free and used alike. A small module lives in 18 blocks and a
ceiling-sized one in many thousands, so allocation cost grows with the number of
live blocks and the frontend, which allocates constantly, becomes superlinear in
module size. The same effect is visible on the host: the arena-bound measurement
takes minutes for work the system allocator does in under a second.

This is an implementation-quality defect, not a contract one. No accepted
document is violated, no semantics change, and every measured result is correct
— it is the cost of reaching them that is wrong. The fix is a free-list or a
rover so the search is not proportional to live blocks, and it is deliberately
**not** attempted in the same change that found it: the heap's reclaim,
coalescing and layout invariants are proved by tests that a rushed rewrite would
be checked against rather than designed for.

Until it is fixed, the docs/35 frontend budget (500 ms p95 on the reference
platform for a 256 KiB module) cannot be met, and this record says so plainly
rather than measuring a smaller module and reporting that instead.
