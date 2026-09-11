<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4D-2 — one real block read, and what proves it happened

One canonical textual module reads sector 0 of the Stage 4 reference backing
image — 512 bytes, one `VIRTIO_BLK_T_IN`, through the queue Stage 4D-1 built —
and the bytes come back from the device.

```text
3 descriptors -> publish -> avail.idx=1 -> publish -> real PCI notify
              -> irq_wait -> dma_consume -> used.idx=1 -> used.id=0
              -> status=VIRTIO_BLK_S_OK -> 512 device-written bytes
```

## 0. The two claims, still kept apart

```text
the device accepted the queue substrate      Stage 4D-1
the device performed DMA through it          claimed here
a client-facing block interface              neither, and not yet designed
```

## 1. Clauses audited

Read from the OASIS document, not from memory:

| Clause | What it fixed here |
|---|---|
| **§2.7.4** | message framing: a device-readable header, a device-writable status tailer |
| **§2.7.5** | `virtq_desc` layout; `NEXT`=1, `WRITE`=2, `INDIRECT`=4 |
| **§2.7.6** | `virtq_avail`: `flags`, `idx`, `ring[]`; "A driver MUST NOT decrement the available idx" |
| **§2.7.7** | used-buffer notification suppression; **"The driver MUST handle spurious notifications from the device"** |
| **§2.7.8** | `virtq_used`, `virtq_used_elem { le32 id; le32 len; }` |
| **§2.7.8.2** | "The device MUST set len prior to updating the used idx"; "**The device MAY write more than len bytes**" |
| **§2.7.8.3** | "The driver MUST NOT make assumptions about data in device-writable buffers beyond the first len bytes" |
| **§2.7.9** | `VIRTIO_F_IN_ORDER` — **not negotiated**, so no ring-order assumption is made |
| **§2.7.10.1** | "The driver MUST initialize flags in the used ring to 0"; if `flags` is 1 the driver **SHOULD NOT** notify; **"The device MUST handle spurious notifications from the driver"** |
| **§2.7.13** | the seven steps, including the two barriers at steps 4 and 6 |
| **§2.7.13.3.1** | "The driver MUST perform a suitable memory barrier before the idx update" |
| **§2.7.13.4.1** | a barrier is required **before reading `flags` or `avail_event`** — which this driver never reads |
| **§2.7.14** | receiving used buffers |
| **§2.9** | notification contents; a 16-bit virtqueue index absent `VIRTIO_F_NOTIFICATION_DATA` |
| **§4.1.4.1** | `virtio_pci_cap`; `VIRTIO_PCI_CAP_NOTIFY_CFG` = 2 |
| **§4.1.4.3** | the common configuration layout |
| **§4.1.4.4** | `virtio_pci_notify_cap`; **Queue Notify = `cap.offset + queue_notify_off * notify_off_multiplier`** |
| **§4.1.4.4.1** | two branches, selected by whether the device **offers** `VIRTIO_F_NOTIFICATION_DATA` — see §2a |
| **§4.1.5.1.2** | MSI-X vector configuration and its readback |
| **§4.1.5.2**, **§4.1.5.2.1** | "If VIRTIO_F_NOTIFICATION_DATA is not **negotiated**, the driver notification MUST be a 16-bit notification", and with `VIRTIO_F_NOTIF_CONFIG_DATA` not negotiated "the driver MUST set the notification value to the virtqueue index" |
| **§4.1.5.3.1** | with `queue_msix_vector` = NO_VECTOR the device MUST NOT deliver an interrupt |
| **§5.2.6** | `virtio_blk_req { le32 type; le32 reserved; le64 sector; u8 data[]; u8 status; }`; `VIRTIO_BLK_T_IN` = 0; `VIRTIO_BLK_S_OK` = 0 |

With ADR-0082 (delivery, and that a wake carries no memory meaning), ADR-0084
(the two authorities, bounded offsets, no address exposure) and ADR-0086 (the
two publication points, and consume before reading the device's writes).

**No undecided TOS-owned boundary was found.** Nothing was added: no ABI
operation, no interface, no Core minor, no authority, no nucleus device
knowledge, no physical-address operation. The endowment is `test-dma-driver`,
unchanged.

## 2. The notification capability, measured

Measured from the reference endpoint before anything was designed around it,
rather than predicted from what QEMU usually does:

```text
VIRTIO_PCI_CAP_NOTIFY_CFG
    bar                    4
    cap_len                20     (16-byte virtio_pci_cap + le32 multiplier)
    cap.offset             0x3000
    cap.length             0x1000
    notify_off_multiplier  4
    queue_notify_off (q0)  0

Queue Notify = 0x3000 + 0 * 4 = BAR4 + 0x3000
```

`cap.length` 0x1000 satisfies §4.1.4.4.1's `>= queue_notify_off * multiplier + 2`,
and the derived address is 2-byte aligned as that clause requires.

**One window covers both structures.** The common configuration is at BAR4+0 and
the Queue Notify at BAR4+0x3000, so the module maps **one 16 KiB window** through
the accepted `pci_bar_map_write` path — measured to map successfully — and
refuses if the two capabilities name different BARs.

The module derives all of this at run time from the capability structure; the
numbers above are the record of what the reference device presented.

## 2a. Which branch of §4.1.4.4.1 applies, and why that is a different question

**`VIRTIO_F_NOTIFICATION_DATA` — feature bit 38 — is NOT offered by the
reference device.** Measured from the endpoint the Stage 4 profile selects, not
inferred from QEMU's source or from the fact that this driver does not negotiate
it:

```text
device_feature[ 0..31] = 0x30006e54
device_feature[32..63] = 0x00000101

bit 38 VIRTIO_F_NOTIFICATION_DATA     not offered
bit 39 VIRTIO_F_NOTIF_CONFIG_DATA     not offered
```

**Two clauses turn on this feature and they ask different questions**, which is
the distinction an earlier revision of this record collapsed:

| Clause | Predicate | Answer here |
|---|---|---|
| §4.1.4.4.1 | does the device **offer** it? | **no** → the 2-byte branch |
| §4.1.5.2.1 | did the driver **negotiate** it? | **no** → a 16-bit notification |

The two happen to agree on this device, and they are still asked separately: a
device may offer a feature a driver declines, and then the capability is
governed by the stricter branch while the notification stays 16-bit.

**The branch is now implemented rather than assumed.** The module reads the
offered high dword from the device's own register and validates:

| Check | Not offering (this device) | Offering |
|---|---|---|
| `cap.offset` alignment | **2-byte** | 4-byte |
| `notify_off_multiplier` | 0, **or a power of two that is even** | 0, or a power of two that is a multiple of 4 |
| `cap.length` | `>= queue_notify_off * multiplier + 2` | `… + 4` |

Three things that were wrong or missing before: the branch was hard-coded to
`+2`; `cap.offset`'s own alignment was never checked, only the derived Queue
Notify address; and the multiplier rule was never checked at all. The reference
device presents `4`, which satisfies **both** branches — and observing one
conforming value is not the clause being enforced.

**The refusal paths are source-level evidence.** A conforming device cannot
exhibit a bad multiplier or a misaligned `cap.offset`, and no fake device was
built to manufacture them.

**The notification stays 16-bit**, and for §4.1.5.2.1's reason rather than
§4.1.4.4.1's: this driver negotiates only `VIRTIO_F_VERSION_1`, so
`VIRTIO_F_NOTIFICATION_DATA` is not negotiated whatever the device offered, and
the value written is the virtqueue index 0 because `VIRTIO_F_NOTIF_CONFIG_DATA`
is not negotiated either.

## 3. The layout inside the one 4 KiB region

Queue size the driver settled on, reported by the boot: **128** descriptors.
Every offset is derived by the module's own checked arithmetic — `align_up` and
a counting `scale` — not copied from Stage 4D-1's evidence:

```text
desc       0 ..  2048   2048 bytes   align 16     16 * 128
avail   2048 ..  2310    262 bytes   align  2      6 + 2 * 128
used    2312 ..  3342   1030 bytes   align  4      6 + 8 * 128
                                                   queue end 3342

header  3344 ..  3360     16 bytes   align 16     virtio_blk_req head
data    3360 ..  3872    512 bytes   align 16     device-writable
status  3872 ..  3873      1 byte                 device-writable

request end 3873 <= 4096, with 223 bytes to spare
```

**Six device addresses, six bounded offsets, one region.** Each comes from
`dma_device_address(region, offset)` — three for the ring at 0, 2048 and 2312,
three for the request at 3344, 3360 and 3872. No address is added to, and there
is no second allocation: the boot journals exactly one `TOS.RUN.DMA_REGION` with
`capability_delta=1`.

## 4. The chain

```text
desc[0]  addr=header  len=16   flags=NEXT            next=1   device-readable
desc[1]  addr=data    len=512  flags=WRITE|NEXT      next=2   device-writable
desc[2]  addr=status  len=1    flags=WRITE           next=0   device-writable
avail.ring[0] = 0                                             the chain head
```

## 5. Runtime evidence

`TOS.RUN.COMPLETED` carries a twenty-bit proof mask plus the values the device
reported. The boot returns the complete mask — **1048575** — which no partial run
can produce:

| Fact | Bit |
|---|---|
| reset observed, `VERSION_1` offered, `FEATURES_OK` accepted | 1, 2, 4 |
| queue 0 exists; size accepted; layout fits; addresses aligned | 8, 16, 32, 64 |
| `queue_desc`, `queue_driver`, `queue_device` read back | 128, 256, 512 |
| MSI-X entry accepted; `queue_enable` 1; `DRIVER_OK` set | 1024, 2048, 4096 |
| the request storage fits the same region | 8192 |
| the notification capability was found and validated | 16384 |
| **`used.idx` advanced 0 → 1** | 32768 |
| **`used.ring[0].id` = 0**, the chain head this driver made available | 65536 |
| **`used.len` covers the data and the status byte** | 131072 |
| **status = `VIRTIO_BLK_S_OK`** | 262144 |
| **all 512 bytes are sector 0's, and the sentinel is gone** | 524288 |

and alongside the mask: **`used.len` = 513** — 512 data bytes plus the one
device-written status byte, which is what the fact is about. Recorded, not
asserted as an equality: §2.7.8.2 permits the device to write more than it
reports and §2.7.8.3 forbids assuming anything past `len`, so the module
requires only `len >= 513` before reading the data and the status, and bounds it
above by the region size so a nonsense report is refused rather than recorded.

**Both publication points are in the artifact**, not just in the source: the
module lowers to four `Op::DmaSync` sites — the initial ring publish, the publish
before `avail.idx`, the publish before the notification, and the consume after
the wake.

### Why the sentinel is the evidence

`VIRTIO_BLK_T_IN` is 0, `VIRTIO_BLK_S_OK` is 0, and the reference backing image
is explicitly zero-filled. So "the status is 0 and the data is 0" is **exactly
what untouched memory looks like**, and a driver that checked only that would
have proved nothing. The module writes 0xA5 into all 512 data bytes and 0xFF
into the status byte before publishing, and what it checks afterwards is that
the device replaced them. The backing image was not modified to make a prettier
pattern, and nothing was written to the device.

### Spurious wakes

A wake is a delivery fact and not a completion (ADR-0082 §7), and §2.7.7.1
requires a driver to handle spurious notifications. Every wake is followed by
`dma_consume` and a look at `used.idx`; a wake that did not advance it is waited
past, bounded at sixteen.

## 6. Notification, deliberately unsuppressed

This driver **always notifies**. `VIRTIO_F_EVENT_IDX` is not negotiated,
`used.flags` is never read, and `avail_event` is ignored as §2.7.10.1 requires
when the feature is absent.

**This is a correctness-over-optimisation choice, not an omission.** §2.7.10.1
makes the no-notify case a *SHOULD NOT* when `used.flags` is 1, while §2.7.10
requires the device to handle a spurious notification from the driver — so
always notifying is conformant and always correct. And because nothing is read
to decide it, §2.7.13.4.1's barrier-before-reading-`flags` does not apply:
**Stage 4D-2 justifies no new CPU store-to-load barrier**, and none was added.

## 7. Features, still minimal

Only `VIRTIO_F_VERSION_1`. **What the device actually offers was measured**, and
an earlier revision of this record listed features it does not:

```text
device_feature[ 0..31] = 0x30006e54
device_feature[32..63] = 0x00000101
```

| Offered and declined | Not offered at all |
|---|---|
| `INDIRECT_DESC` (28), `EVENT_IDX` (29), `RING_RESET` (40) | `ACCESS_PLATFORM` (33), `RING_PACKED` (34), `IN_ORDER` (35), **`NOTIFICATION_DATA` (38)**, `NOTIF_CONFIG_DATA` (39) |
| `VIRTIO_BLK_F_` SEG_MAX (2), GEOMETRY (4), BLK_SIZE (6), FLUSH (9), TOPOLOGY (10), CONFIG_WCE (11), DISCARD (13), WRITE_ZEROES (14) | |

Offered-but-unselected features are not an error (§2.2.1), and the queue's
layout and notification rules depend on none of them. The three that are *not
offered* are listed separately because one of them selects a clause: see §2a.

The request is non-destructive: one read, no write, no flush, no `GET_ID`, and
no QEMU device property was added.

## 8. Stage 4 performance accounting

```text
initialization          claim, walk, map, negotiate, one dma_region_allocate,
                        zero the region, write the ring, program common config,
                        claim one MSI-X source, enable, DRIVER_OK

the request path        write 3 descriptors + header + sentinels, publish,
                        avail.idx, publish, one MMIO notify, irq_wait,
                        dma_consume, read used/status/data
```

**The completed request adds no dynamic allocation.** The DMA region is
allocated once during initialization and the request is written into space
inside it; no second region, no per-request allocation, and no growth of the
capability table — `capability_delta=1` for the whole boot.

Ring-0 crossings on the request path, against ADR-0082 §11's stated budget of
two per unbatched request: the descriptor and ring writes are ordinary stores,
the notify is one mapped store, `irq_wait` is one crossing in and the device's
wake is one out. `dma_publish` and `dma_consume` cost **no** crossing — they
are compiler and execution barriers in ring 3 (ADR-0086 §11), which is the
property that design was chosen for.

**What this does not yet demonstrate.** There is no client-facing block
interface, so the client-payload-copy budget of `docs/35` is not exercised and
is not claimed. A steady-state service — many requests, a queue that wraps,
batching, and a client on the other side — is what later evidence must measure;
this is one request, once, with the ring at index 1.

## 9. What is not here

No second request. No write, flush, discard or `GET_ID`. No indirect
descriptors, no `EVENT_IDX`, no packed ring, no multi-queue, no ring wrap, no
batching, no used-buffer suppression, and no block service interface.
