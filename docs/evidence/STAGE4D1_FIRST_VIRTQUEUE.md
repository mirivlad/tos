<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4D-1 — one real split virtqueue, and what its evidence claims

One canonical textual module configures one real split virtqueue on the Stage 4
reference `virtio-blk-pci` endpoint, from one `DmaRegion<mut u8>`, using three
operation-31 addresses, publishing its initial state through ADR-0086,
connecting one MSI-X source, enabling the queue and reaching `DRIVER_OK` — and
the device reads the programmed state back.

**There is no block request.** No descriptor is exposed, the notification
capability is not mapped, nothing is notified, and `avail.idx` is 0.

## 0. The two claims, kept apart

```text
the device accepted the queue substrate      claimed here
the device performed DMA through it          not claimed, and not shown
```

A device that accepts a descriptor-table address has not read a descriptor. The
first real block request is Stage 4D-2's, and it is what will exercise the chain
this slice only builds.

## 1. The external contract, verified rather than remembered

```text
Virtual I/O Device (VIRTIO) Version 1.4
Committee Specification 01
8 April 2026
docs.oasis-open.org/virtio/virtio/v1.4/cs01/
```

**Read from the OASIS document itself**, not from memory: the title block says
"Committee Specification 01" and "8 April 2026", and the citation line inside it
reads "8 April 2026. OASIS Committee Specification 01."

Every clause this slice depends on was checked against that text:

| Clause | What it says | Where it is used |
|---|---|---|
| §3.1.1 | reset → ACKNOWLEDGE → DRIVER → features → FEATURES_OK → re-read → device-specific setup → DRIVER_OK; FAILED on irrecoverable failure | the driver's whole order |
| §4.1.4.3.2 | "After writing 0 to `device_status`, the driver MUST wait for a read of `device_status` to return 0"; "The driver MUST configure the other virtqueue fields before enabling the virtqueue with `queue_enable`"; "MUST NOT write a 0 to `queue_enable`" | the bounded reset wait, and `DRIVER_OK` last |
| §4.1.4.3.1 | "The device MUST present a 0 in `queue_size` if the virtqueue … is unavailable"; without `RING_PACKED`, a power of 2 | the queue-existence and power-of-two checks |
| §4.1.4.3 | the `virtio_pci_common_cfg` field order | the offsets the module writes |
| §4.1.4.1 | `struct virtio_pci_cap`: `cap_vndr` 0x09, `cfg_type`, `bar`, `offset`, `length`; `VIRTIO_PCI_CAP_COMMON_CFG` = 1 | the capability walk |
| §2.7 | Descriptor Table align 16, `16 * qsz`; Available Ring align 2, `6 + 2*qsz`; Used Ring align 4, `6 + 8*qsz` | the derived layout |
| §6.3 | `VIRTIO_F_VERSION_1(32)` | the one accepted feature |
| §4.1.5.1.2.1/2 | a failed vector mapping is reported as `NO_VECTOR`, and the driver must verify by reading the field back | the MSI-X readback |

**Version 1.4 changed nothing this slice relies on.** The common configuration
gained `queue_notif_config_data` at 0x38 and `queue_reset` at 0x3A, each live
only under a feature this driver does not negotiate, and the administration-queue
fields after them. Nothing here reaches past 0x38, so the existing Stage 4C-1
fixture's reading of the layout stands unchanged.

**What is claimed about the device.** Not that it "implements VirtIO 1.4" — a
device exposes feature negotiation, not a source-visible protocol minor. What is
claimed is that **the TOS Stage 4 reference driver implements the cited modern
PCI / split-virtqueue subset**, and that this device accepted it.

## 2. No new TOS mechanism

The audit found no undecided TOS-owned boundary, and none was added:

```text
no new SYSTEM_ABI_V1 selector       no new platform interface
no new TOS Core minor               no new nucleus knowledge
no physical address exposed         no new authority
```

The queue is built entirely from what Stages 4A–4C accepted: `platform.pci.Bus`,
`platform.pci.FunctionConfig`, `pci_bar_map_write`, `pci_interrupt_claim`,
`dma_region_allocate`, `dma_device_address`, `DmaRegion<mut u8>` indexed access,
and ADR-0086's `dma_publish`. **The launcher constant is the DMA positive's,
unchanged** — which is the evidence that a live queue needs no authority the DMA
slice did not already establish.

Not one VirtIO constant, queue structure, feature bit or descriptor concept
entered the nucleus.

## 3. One friction worth recording, which is not a STOP

**TOS Core V1 has no checked conversion whose destination is `size`.** `docs/40`
§3 fixes the set at `to_i8`..`to_u64`, each accepting "any fixed-width integer or
`size`" — so a `size` converts *out*, and a device-reported `u64` cannot convert
*in*. Every region index and every bounded DMA offset has exact type `size`, and
the queue's geometry is derived from a number the device reports.

The module therefore enters the `size` domain by counting:

```tos
fn scale(unit: size, count: u64) -> size {
    let mut total: size = 0B;
    let mut done: u64 = 0u64;
    while (done < count) { total = total + unit; done = done + 1u64; }
    return total;
}
```

Bounded by this driver's own cap and checked at every step. **This is not a
STOP**: the accepted language decides the question rather than leaving it open,
and the slice is implementable without deciding anything new. It is recorded
because the friction is real and will grow when a descriptor index becomes a
runtime value, and because inventing `to_size` inside a device slice is exactly
what §1 of the brief forbids.

## 4. What the positive boot proves

`TOS.RUN.COMPLETED value=i64:16383` — fourteen facts, one bit each, composed so
no partial run can produce it:

| Bit | Fact |
|---|---|
| 1 | reset written and observed, bounded |
| 2 | `VERSION_1` offered by the device |
| 4 | `FEATURES_OK` accepted and read back |
| 8 | queue 0 exists — a nonzero, power-of-two maximum |
| 16 | the selected size was written and read back |
| 32 | the derived layout fits one 4 KiB region |
| 64 | all three device addresses are correctly aligned |
| 128 | `queue_desc` reads back the descriptor address |
| 256 | `queue_driver` reads back the available-ring address |
| 512 | `queue_device` reads back the used-ring address |
| 1024 | the device accepted the MSI-X table entry |
| 2048 | `queue_enable` reads 1 |
| 4096 | `DRIVER_OK` reads set |
| 8192 | `avail.idx` reads 0 — no buffer is exposed |

And from the nucleus's own journal: `express=1` and `dma=1` asserted separately
(ADR-0084 §5c's P1 and P5 stay two facts), `capability_delta=1`, `aliases=0`, and
exactly one `TOS.RUN.DMA_REGION` for the whole boot — three device addresses from
three bounded offsets in **one** object.

For the reference device the geometry lands at `queue_size = 128`:

```text
desc   0     .. 2048     16 * 128
avail  2048  .. 2310      6 + 2 * 128
used   2312  .. 3342      6 + 8 * 128   (align 4)
end    3342  <= 4096
```

recorded as evidence, and derived by the source rather than written into it.

## 5. What the negative boot proves, and what it does not

Two refusals are the reference device's own answer or this driver's own checked
arithmetic:

| Refusal | Evidence class |
|---|---|
| a queue index the device does not have presents `queue_size` 0 | **device-exhibited** |
| a queue larger than one region is refused by the layout guard | **device-exhibited** (the device's own maximum, this driver's cap raised past what fits) |
| giving up adds FAILED and preserves ACKNOWLEDGE, DRIVER, FEATURES_OK | **device-exhibited** — a conformance fact rather than a refusal, and named as one (§4a) |
| no `VERSION_1` offered | static |
| device refuses `FEATURES_OK` | static |
| device refuses the MSI-X queue vector | static — **and attempted** |
| device refuses `queue_enable` or its readback | static |
| device refuses `DRIVER_OK` | static |

**The MSI-X refusal was attempted and withdrawn, which is worth recording.** With
no interrupt source claimed the nucleus leaves MSI-X disabled at claim time, so
the expectation was that the device would answer `NO_VECTOR` per §4.1.5.1.2.1.
The reference device **accepted the mapping anyway**, answering 0. The positive
boot's readback check is still exactly what §4.1.5.1.2.2 requires; this device
simply cannot be made to fail it, and **no fake device was built to manufacture
the branch**.

## 4a. A conformance defect, found in review and repaired

**The first version of this slice violated VIRTIO §2.1.1**, and so did the
Stage 4C-1 witness it inherited its shape from. The clause is not ambiguous:

> The driver MUST update device status, setting bits to indicate the completed
> steps of the driver initialization sequence specified in 3.1. **The driver
> MUST NOT clear a device status bit.**
> — VIRTIO 1.4 CS01 §2.1.1

Every non-zero status write in every canonical fixture replaced the byte:

```tos
fn give_up(borrow mut window: MmioRegionMut) -> unit {
    mmio_write_u8(window, DEVICE_STATUS, STATUS_FAILED);   // clears three bits
}
```

After a successful negotiation that turned `ACKNOWLEDGE|DRIVER|FEATURES_OK`
into `FAILED` alone. The ACKNOWLEDGE and DRIVER writes had the same shape, and
the FEATURES_OK and DRIVER_OK writes rebuilt the byte from the sequence the
driver believed it had performed.

**Why rebuilding is wrong even when it computes the right answer.** The byte
carries bits the *device* sets — `DEVICE_NEEDS_RESET` among them (§2.1.2) — so
a driver that reconstructs it from its own history erases a bit it never wrote,
and then goes on relying on a device that asked to be reset. The repair reads
the register:

```tos
fn add_status(borrow mut window: MmioRegionMut, bits: u64) -> u64 {
    let current: u64 = mmio_read_u8(window, DEVICE_STATUS);
    mmio_write_u8(window, DEVICE_STATUS, current | bits);
    return mmio_read_u8(window, DEVICE_STATUS);
}
```

### Every `DEVICE_STATUS` writer in the tree

A bounded whole-tree search, recorded so the 4C-1 witness is known not to have
been the only inherited instance:

| File | Writes | What changed |
|---|---|---|
| `tests/vectors/virtio-queue/init.tos` | 6 | 1 reset kept; 5 replacements → `add_status` |
| `tests/vectors/virtio-queue-refused/init.tos` | 5 | 1 reset kept; 4 replacements → `add_status` |
| `tests/vectors/virtio-msix-wait/init.tos` | 6 | 2 resets kept; 4 replacements → `add_status` |
| `nucleus/src/pci.rs` | 1 | **not this register.** `EXPRESS_DEVICE_STATUS` is the PCI Express capability's Device Status, and is untouched |
| `host-tools/qemu-test/irq-routed.sh`, `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md` | 0 | mentions only; the gate blanks the Express token before matching |

Seventeen mentions, sixteen VirtIO writes, four legitimate resets, twelve
replacements repaired. Nothing else in the tree writes the register.

### And one fatal exit that said nothing to the device

`virtio-msix-wait`'s modern-only path returned on a device that does not offer
`VERSION_1` **without setting FAILED**. §2.2.1: a driver that does not go into
backwards compatibility mode "MUST set the FAILED device status bit and cease
initialization". That module has no legacy path, so this is exactly the branch
the clause is about. Its other irrecoverable exits — a refused `FEATURES_OK`, a
refused MSI-X vector, a refused `DRIVER_OK` — now give up the same way, which is
§3.1.1 applied consistently with the fixture's existing policy and not generic
error-recovery work.

### Proved at run time, not asserted

The refusal boot reaches a successful negotiation and then gives up, so it can
demonstrate the repair against the real device without a fake one:

```text
TOS.RUN.COMPLETED value=i64:7
```

which is two refusals **plus one status-preservation fact**: after `FAILED` is
added, the byte the device reports still has ACKNOWLEDGE, DRIVER and FEATURES_OK
set. **Checked by mask, never by equality** — a device may set bits of its own,
and a driver asserting an exact byte would be asserting that the device said
nothing.

### And it cannot come back silently

`scripts/tests/check-device-status-additive.sh` reads every write to the VirtIO
`DEVICE_STATUS` register in every fixture that declares one, and requires each
value to be either the literal `0u64` — explicit reset, the one legitimate
non-additive write — or an OR whose enclosing function reads the register.
A value naming a `STATUS_` constant directly is refused, which is what a
replacement looks like in every form it took here.

**It is structural, not a spelling**, and it was tested against the defect: the
bare-constant form and a fabricated `previous | 128u64` that never read the
register are both caught, and neither depends on the helper being called
`add_status`.

## 5a. A boundary the gates caught, and it was right to

The launcher constant that runs this boot was first called `test-virtio-queue`,
and three Stage 4B/4C-1 gates failed at once:

```text
virtio-caps: FAIL: the nucleus code mentions VirtIO
virtio-mmio: FAIL: the nucleus code mentions VirtIO
irq-routed:  FAIL: ring 0 mentions device vocabulary
```

**A feature name is code.** Those gates strip comments and then refuse any
mention of VirtIO in ring 0, precisely so that the boundary is a mechanical fact
rather than a habit — and a launcher constant named after a device protocol
tells a reader of the nucleus which device class the module drives, which the
nucleus does not know and must not appear to.

The constant is `test-dma-driver`: named for what the launcher decided — two
authorities for a driver — and not for what the driver happens to drive. The
rename is the whole fix, and the gates were doing their job.

## 5b. One flake, and why it is one

The first complete QEMU profile after the §4a repair failed
`QEMU textual service supervision`. It is recorded rather than dropped, because
a new failure is a regression until it is shown not to be:

```text
the supervisor reported   value=i64:1303
the gate expects          value=i64:1302
```

That number is `1000 + created×10 + latched×100 + blocked`, so the difference is
**one extra service observed blocked** — a scheduling observation, not a
decision. Three consecutive re-runs reported 1302, and the causal argument is
independent of the count: this corrective slice changed **nothing** under
`source/nucleus` or `source/crates` — the diff against `616c3bd` there is empty
— and the supervision gate boots `tests/vectors/supervision`, which this slice
does not touch. The nucleus, the runtime image and that fixture are
byte-identical to the runs that passed. The second complete profile passed it.

## 6. Cleanup and reset, deliberately not decided

The slice performs the initial VirtIO device reset §3.1.1 step 1 requires, and
nothing more. It does **not** decide Stage 4's crash or restart policy, and
**nothing about DMA safety depends on that reset**: process death and release
remain ADR-0084's descendant, bus-mastering and drain mechanism, unchanged and
untouched. A live queue with no exposed buffer revealed no contradiction in that
teardown proof.

## 7. What is not here

No descriptor in the available ring. No notification capability mapped. No
notify. No `VIRTIO_BLK_T_IN` or `VIRTIO_BLK_T_OUT`. No sector. No `irq_wait` on
the queue source. No used-ring consumption. No indirect descriptors, no
`EVENT_IDX`, no packed queues, no multi-queue.

Stage 4D-2 is the smallest real block request, and it is what will exercise the
whole chain: descriptors → publish → `avail.idx` → publish → notify → `irq_wait`
→ consume → used, status and data.
