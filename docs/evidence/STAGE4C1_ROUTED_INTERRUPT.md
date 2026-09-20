<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4C-1 — a real device interrupt reaches a textual driver

A canonical textual module holding one PCI function derives
`platform.irq.Source` **from that assignment**, blocks in `irq_wait`, and is
woken by a **real MSI-X message from the real device**. The blocked-context
census answers `routed=1` for the first time in this system's history.

```text
claim function -> derive Source from the assignment -> program the device's
own MSI-X entry -> irq_wait -> LIVENESS blocked=1 routed=1 awaiting-hardware
-> real MSI-X on vector 48 -> the waiter wakes -> ... -> source released,
vector retired
```

## 0. What this document is, and what it is not

**It is post-implementation evidence, written on 2026-09-17 for behaviour that
has been green since 2026-09-07.** The Stage 4C/4D closure-readiness audit found
that Stage 4C-1 was the only sub-stage with no positive evidence document: its
basis was ADR-0082, the `irq-routed` gate and a progress-log entry, while every
other sub-stage of 4A, 4B, 4C and 4D has a document a closure ruling can cite.
This fills that form.

**It rewrites no history.** `STAGE4C1_REVIEW_FINDINGS.md` remains exactly what
it is and says so in its own header: a **pre-implementation** report from
2026-09-05, written when ADR-0082 was still *Proposed*, when items 2–4 of it
were proposals only, and when Stage 4C-1b — the source object, the vector
allocator, MSI-X programming, `irq_wait` and the positive proof — was **not
begun**. That document is the review; this one is the result. Neither replaces
the other.

**Everything below was re-run against this tree**, not transcribed from the
logs of the day it was built. The figures are from that run.

## 1. Authority — a source is derived, never ambient

`platform.irq.Source` is **descended from a live PCI function assignment**
(ADR-0082). There is no interrupt-controller capability, no vector namespace a
module can name, and no path from "I am a driver" to "I may receive
interrupts". A module that does not hold the function cannot obtain a source
for it, and a source does not outlive the assignment it came from.

```text
TOS.RUN.IRQ_SOURCE process=0 segment=0 bus=1 device=0 function=0
                   entry=0 transport=msix generation=1 asserted_by=nucleus
```

`generation=1` is the assignment's, so the source is bound to *this* claim of
this BDF; a later assignment of the same function is a different generation and
an older source does not reach it.

**The entry is the device's, and the vector is the machine's.** The module names
an MSI-X **entry index of its own function** — never a CPU vector. The vector
(48 here) comes from a spent allocator inside the nucleus and is never a value
ring 3 chooses or learns as authority.

## 2. Delivery — a real device, a real message

The driver owns the function, derives the source, programs the device's own
MSI-X entry, and blocks:

```text
TOS.RUN.LIVENESS      blocked=1 routed=1 verdict=awaiting-hardware
TOS.RUN.IRQ_DELIVERED source=0 entry=0 vector=48 deliveries=1 woke=1 latched=0
TOS.RUN.LIVENESS      blocked=1 routed=1 verdict=awaiting-hardware
TOS.RUN.IRQ_DELIVERED source=0 entry=0 vector=48 deliveries=2 woke=1 latched=0
TOS.RUN.COMPLETED     value=i64:7
```

The interrupt is caused by **the outside world acting on the device**, not by
the guest: the harness waits for the guest's own announcement that it is blocked
with nothing runnable and then asks QEMU to resize the disk, which is an
ordinary configuration change a device reports to its driver. Nothing is written
to the guest and no value it can read is supplied. `TOS.RUN.IRQ_DELIVERED` is
written by the handler that took the message and counts with the nucleus's own
counter, so a module can neither forge it nor suppress it.

`woke=1 latched=0` on both: each message found a waiter rather than a latch.

## 3. Liveness — a routed wait is not a deadlock

Before Stage 4C-0 the census answered a boolean "something is blocked". The
rule's second half (`SYSTEM_ABI_V1` §6, `STAGE4C_LIVENESS.md`) computes *why*
each context is blocked and what could wake it. Here it answers `routed=1` —
a wait whose wake source is a live routed capability — and the verdict
`awaiting-hardware` rather than a stall.

The gate asserts all three: that a routed source was seen, that the wait was not
judged stalled, and that it was **not cancelled**. Without this, the first real
device interrupt would have raced a liveness policy that had no way to tell a
driver waiting for hardware from a deadlocked pair — which is why 4C-0 was a
mandatory prerequisite to 4C-1 rather than a convenience.

## 4. The latch, and what a wake means

**One bit, not a count** (ADR-0082 §7). The nucleus keeps one `pending` bit per
source: an interrupt arriving with a waiter wakes it and leaves the bit clear;
arriving with nobody waiting sets it; a wait with the bit set returns
immediately and clears it; a second interrupt while it is set coalesces. So the
only completion event cannot be lost by racing the wait call.

**A wake is a delivery fact and carries no memory-visibility meaning.** That is
why the completion side became `dma_consume` in ADR-0086 rather than an ordering
silently attached to `irq_wait`. A driver's obligation after a wake is the one
every real queue driver has: drain until empty.

**Stage 4C-1 does not prove batching, and does not claim to.** The model permits
one wake to cover any number of completions; this stage's two deliveries each
woke a waiter. That the model actually holds on real hardware was observed later
and elsewhere — Stage 4D-4 recorded one MSI-X message exposing two completed
used-ring entries — and is cited here as corroboration of the model, not as
something 4C-1 established.

## 5. Lifetime

- **One waiter.** At most one context may be inside `irq_wait` on a source; a
  second concurrent wait is `E_LIMIT`. Several capabilities may name the source;
  one wait is outstanding.
- **Claiming turns bus mastering on.** An MSI-X message *is* a memory write the
  device issues, so a routed source necessarily makes its function a bus master
  (ADR-0082 §5d). A mapped window turns only memory decoding on; claiming a
  source turns both on, and the gate asserts the difference.
- **Release retires the vector.** The last descendant going takes both enables
  back, and the vector is **retired rather than returned** — a late message on a
  retired vector is acknowledged, counted and wakes nobody:

```text
TOS.RUN.PCI_ENABLES  memory_decoding=0 bus_mastering=0 memory_space=0 bus_master=0
TOS.RUN.IRQ_RELEASED source=0 entry=0 vector=48 deliveries=2
                     cancelled_waiter=0 vector_retired=1 asserted_by=nucleus
```

- **The entry is claimable again afterwards, with a different vector**, which
  the gate checks — because a retired vector never comes back.
- **Assignment relationship.** The source is a descendant; when the assignment
  ends, so does it. Stage 4D-5's gate now asserts the same release on the
  **process-death** path, where nothing released anything deliberately.

## 6. Negatives, as they actually run today

All of these are in the `irq-routed` gate and all ran green on this tree.

| Negative | Where |
|---|---|
| a fabricated MSI-X entry index | `tests/vectors/irq-authority-negative` |
| a function capability without the `interrupt` right | same |
| a second source for one entry | same |
| a source name that cannot wait and cannot be made | `tests/vectors/irq-wait-negative` |
| a released source that no longer resolves | same |
| the entry claimable again, with a **different** vector | `irq-routed.sh` |
| a module with no PCI root cannot start at all | `irq-routed.sh` |
| no interrupt is delivered in a run that never armed the device | `irq-routed.sh` |
| invalid MSI-X setup against the real device | `tests/vectors/virtio-msix-negative`, run by `virtio-mmio.sh` |
| reserved/invalid MSI-X vector conditions | `tests/vectors/pci-msi-reserved` |
| Bus Master Enable precision — one bit of one byte | `tests/vectors/pci-bme-precision` |

The source-side run reports **31** wait-negative observations and the
authority-side run its own set; the gate requires the exact counts rather than
"at least some". **Only negatives that run today are listed**: nothing here is
claimed from a proposal.

## 7. The nucleus boundary

Ring 0 programs an MSI-X table entry, allocates and retires a vector,
acknowledges through the local APIC and matches a message to a source. It does
**not** know what a queue is, that entry 0 is a configuration vector, or that
the function behind it is a block device — and it interprets no VirtIO
completion semantics whatever.

Checked mechanically, with comments stripped, and now over a token list extended
for the protocol Stage 4D completed. The current result on this tree:

```text
nucleus/src, runtime-image/src, crates/tos-engine/src   no device-protocol vocabulary
```

The `irq-routed` gate has carried its own inline version of this check since
Stage 4C-1; the shared, extended list lives in `stage4-profile.sh` and the Stage
4D-5 gate runs it, so a future leak turns the ordinary `qemu` profile red.

**One narrow, documented exception**: PCI Express' own Device Status register.
ADR-0084 §5b makes its Transactions Pending bit ring 0's business — it is how
the nucleus proves a function has no non-posted request outstanding before that
function's memory returns to the pool. The token is blanked rather than the line
dropped, so a genuine leak sharing a line with it still matches.

## 8. Current results

Re-run against this tree rather than narrated from the logs of the day:

```text
irq-routed        PASS   value=i64:7, 2 deliveries on vector 48, routed=1,
                         vector retired, 31 wait-negative observations
negative suite    PASS   13 fixtures failed closed with the declared rule
virtio-mmio       PASS   includes the MSI-X negative vector
virtio-block-write (4D-5) PASS  now also asserting the process-death teardown
                         and the extended vocabulary guard
```

## 9. What this does not claim

Not DMA, not ordering, not a virtqueue, not block I/O — those are Stage 4C-2,
4C-3 and 4D, each with its own evidence. Not the Stage 4 identity gate: no
persistent data moves here. Not batching, per §4. And not a claim that a
malicious bus-mastering device is confined: `docs/34` S5 and ADR-0082 §5 record
the no-IOMMU reference profile as a **weaker security profile**, stated rather
than hidden.

## 10. Reproduction

```sh
bash source/host-tools/qemu-test/irq-routed.sh
./scripts/preflight.sh --profile qemu
```
