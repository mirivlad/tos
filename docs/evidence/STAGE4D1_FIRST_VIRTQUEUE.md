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
