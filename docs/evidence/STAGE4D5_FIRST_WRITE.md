<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4D-5 — the first real block write

One canonical textual module **writes 512 bytes it composed to a real VirtIO
block device** with `VIRTIO_BLK_T_OUT`, scrubs the buffer it wrote them from,
and reads the same sector back into a **different** poisoned buffer with
`VIRTIO_BLK_T_IN` — obtaining exactly those bytes.

```text
capacity read from the device        32768 sectors, admits sector 5
T_OUT sector 5, 512 bytes            payload descriptor device-readable
  -> used.id = chain head, used.len = 1, status VIRTIO_BLK_S_OK, chain reclaimed
payload buffer scrubbed to zero      the pattern now exists only on the disk
T_IN sector 5, a different buffer    poisoned 0x3C
  -> used.id = chain head, used.len = 513, status OK
  -> all 512 bytes recomputed from the index and exact
```

## 0. The five claims, kept apart

```text
the device accepted the queue substrate            Stage 4D-1
the device performed DMA through it, once          Stage 4D-2
the queue survives a request and serves another    Stage 4D-3
two chains are outstanding together                Stage 4D-4
the device consumed bytes TOS composed, and the
  block device's observable state changed          claimed here
crash durability, host-storage persistence         none of them, and not designed
```

## 1. Why the status byte is not the evidence

Every Stage 4D boot so far has **read**. The device wrote into memory this
driver granted it and the witness was a sentinel the device had to replace.
Nothing the guest did changed a byte on the disk.

A write inverts the direction, and the obvious witness — `VIRTIO_BLK_S_OK` — is
the wrong one. §2.7.5.1 says a device *"MUST NOT write to a device-readable
buffer, and SHOULD NOT read a device-writable buffer"*, so a payload descriptor
wrongly marked device-writable is a payload the device is told not to read: the
disk keeps its old content and the status still says nothing was wrong.
**Measured on the real device**, with the module's own direction guard and the
gate's source check suspended for the measurement: the run reported `-59`
because QEMU's `used.len` became 513 instead of 1 — and had it passed that, the
read-back would have found zeros. The status was never the discriminator.

That is why this stage's claim needs the read-back, and why Stage 4D-5 is not
"the driver sent a write".

## 2. Clauses audited

Read from the OASIS document during this slice:

> Virtual I/O Device (VIRTIO) Version 1.4, Committee Specification 01,
> 8 April 2026, OASIS, docs.oasis-open.org/virtio/virtio/v1.4/cs01/

| Clause | What it fixed here |
|---|---|
| **§2.5** | "Each transport also provides a generation count for the device configuration space, which will change whenever there is a possibility that two accesses … can see different versions" |
| **§2.5.1** | "Drivers MUST NOT assume reads from fields greater than 32 bits wide are atomic, nor are reads from multiple fields", with the `do { before = generation; read; after = generation } while (after != before)` protocol; and "drivers SHOULD only check that device configuration space is large enough to contain the fields necessary for device operation" |
| **§2.7.4.2** | "The driver MUST place any device-writable descriptor elements after any device-readable descriptor elements" |
| **§2.7.5** | a descriptor is "read-only for the device ('device-readable') or write-only for the device ('device-writable')" |
| **§2.7.5.1** | "A device MUST NOT write to a device-readable buffer, and a device SHOULD NOT read a device-writable buffer" |
| **§2.7.8.2** | "The device MUST write at least len bytes to descriptor, beginning at the first device-writable buffer, prior to updating the used idx" — which is what makes a write's `len` 1 and a read's 513 |
| **§2.7.13** | the seven steps and the barriers at 4 and 6 |
| **§2.7.14** | consuming a used entry |
| **§4.1.4.1** | `#define VIRTIO_PCI_CAP_DEVICE_CFG 4` |
| **§4.1.4.1** | "The driver SHOULD use the first instance of each virtio structure type they can support"; and "The drivers SHOULD only map part of configuration structure large enough for device operation" |
| **§4.1.4.2** | "If the device presents multiple structures of the same type, it SHOULD order them from optimal (first) to least-optimal (last)" — the **device's** side of the rule above |
| **§4.1.4.3** | the common configuration layout: `u8 config_generation` immediately after `device_status` |
| **§4.1.4.6**, **§4.1.4.6.1** | "The device MUST present at least one VIRTIO_PCI_CAP_DEVICE_CFG capability for any device type which has a device-specific configuration"; "The offset for the device-specific configuration MUST be 4-byte aligned" |
| **§5.2.4** | `struct virtio_blk_config { le64 capacity; … }` |
| **§5.2.6** | `#define VIRTIO_BLK_T_OUT 1`; `VIRTIO_BLK_S_OK 0` |
| **§5.2.6.1** | "A driver MUST NOT submit a request which would cause a read or write beyond capacity"; "The length of data MUST be a multiple of 512 bytes for VIRTIO_BLK_T_IN and VIRTIO_BLK_T_OUT requests" |
| **§5.2.6.2** | the stability rules — quoted in §7 below, and quoted there rather than paraphrased because that is the clause it would be easiest to overclaim from |

With ADR-0082, ADR-0084 and ADR-0086 unchanged.

## 3. Capacity, read from the device before anything is published

This is the first Stage 4 vector to derive a request bound from the device
rather than from the reference profile's known disk. §5.2.6.1 makes it a MUST,
and a write is where it first has teeth.

The module walks the real capability list for `VIRTIO_PCI_CAP_DEVICE_CFG` and
validates what it finds **before reading a byte of it**: the vendor capability's
own length must cover a sixteen-byte `struct virtio_pci_cap`; the BAR index must
be in range; the offset must be 4-byte aligned (§4.1.4.6.1); and the structure
must be at least eight bytes long, which is the mandatory `capacity` and nothing
after it.

The walk returns the **first** such capability because that is the driver's own
rule — §4.1.4.1: *"The driver SHOULD use the first instance of each virtio
structure type they can support"* — and what makes it a good rule rather than an
arbitrary one is the device's side of it, §4.1.4.2: *"If the device presents
multiple structures of the same type, it SHOULD order them from optimal (first)
to least-optimal (last)."* Two clauses, two parties. Nothing assumes there is
exactly one, or that it sits at any particular offset.

**The same-BAR requirement is this vector's, not VirtIO's.** The capability
carries its own `bar`, and nothing in the specification requires the
device-specific configuration to share one with the common configuration. The
reference Stage 4 endpoint places the selected structure in the VirtIO BAR this
driver has already mapped; the vector verifies that profile fact and refuses a
different BAR (`-45`) rather than adding a second mapping for one evidence
slice. A driver meant for arbitrary devices would map the BAR the capability
names, and that is deliberately not built here.

`capacity` is then read under §2.5.1's protocol — two 32-bit little-endian reads
between two reads of `config_generation`, accepted only when the generation did
not move:

```text
tries < MAX_GENERATION_TRIES:
    before = config_generation
    low    = device_cfg[0..4]
    high   = device_cfg[4..8]
    after  = config_generation
    if before == after: capacity = low | (high << 32)
```

**Bounded, because a retry without a bound is not a bound.** `MAX_GENERATION_TRIES`
is 8, a constant of this driver's, and a generation that will not settle is a
refusal (`-42`), never a loop. Observed: **capacity 32768 sectors in 1 attempt**.

The predicate is written so it cannot wrap: a capacity below the request length
is refused first, then `TARGET_SECTOR > capacity - SECTORS_IN_REQUEST` is the
comparison. **Nothing is published before it holds** — no descriptor is written,
`avail.idx` is not advanced and the device is not notified.

### 3a. The negative, on a device too small to hold the sector

The same module and the same capsule on a **4-sector** disk, built per-run by an
opt-in harness flag that leaves the reference profile untouched:

```text
completion  -44        the capacity refusal
interrupts  0          counted by the nucleus
```

**Zero deliveries is the proof that nothing reached the device.** No
notification means no completion means no MSI-X message, and that line is
written by the nucleus's own handler — a module can neither forge it nor
suppress it.

## 4. Two independent buffers, and a scrub between them

The write's payload and the read-back's buffer are different memory, and the
extents are checked disjoint before either request is built. That alone is not
quite enough, so after the write completes and its chain is back in the pool the
**payload buffer is overwritten with zeros and the overwrite is verified**.

From that moment the pattern exists nowhere in this machine's memory. A
read-back that returns it cannot have been answered by a buffer that still
happened to hold it, and a mutation that checked the wrong buffer finds zeros.

The expected byte is **recomputed from its index** at both ends rather than
compared against a saved copy, so the check is against the rule and not against
the bytes that were sent.

## 5. The payload, and why it is not a constant

```text
payload[i] = (0x5C + 7 * i) mod 256      i = 0 .. 511
```

`gcd(7, 256) = 1`, so the pattern strides every byte value and repeats only
after 256 — the written sector contains all 256 values twice. 512 copies of one
byte could not tell a sector write from the first byte written 512 times, nor
catch an offset slip inside the sector, nor a truncated request. Measured on the
backing image afterwards: **256 distinct byte values** in sector 5, first bytes
`5c 63 6a 71 78 7f 86 8d`, last `24 2b 32 39 40 47 4e 55`, exactly the rule.

Sector **5** is beyond the profile's seeded range (`STAGE4_SEEDED = 4`), so its
initial content is the zero fill the image was created with, known to the host
independently, and no other Stage 4 vector reads it. The read-back buffer is
poisoned `0x3C` and the status bytes `0xFF` — neither is zero, neither is any
seeded sector's content (`0xC1`..`0xC4`), and neither is a poison any earlier
Stage 4 vector used (`0xA5`, `0x5A`).

## 6. Descriptor directions and the two used lengths

```text
T_OUT   header  16 B   NEXT                device-readable
        payload 512 B  NEXT                device-readable    <- the new direction
        status   1 B   WRITE               device-writable
        used.len must be exactly 1

T_IN    header  16 B   NEXT                device-readable
        buffer  512 B  WRITE | NEXT        device-writable
        status   1 B   WRITE               device-writable
        used.len must be exactly 513
```

The module does not merely *build* the write's payload descriptor without
`VIRTQ_DESC_F_WRITE` — it **reads the flags back out of the descriptor table it
just wrote** and refuses (`-80`) if the write bit is set. §2.7.4.2's ordering
requirement is satisfied by construction: the only device-writable element is
last in both chains.

The two admissible lengths are different numbers and both are exact, which
follows from §2.7.8.2: a `T_OUT` chain's only device-writable buffer is the one
status byte. A write reporting 513 is describing a chain this driver did not
build.

## 7. Persistence: what the contract says, and what this boot does not measure

Measured from the guest on the reference profile, the device offers:

```text
SEG_MAX(2) GEOMETRY(4) BLK_SIZE(6) FLUSH(9) TOPOLOGY(10) CONFIG_WCE(11)
DISCARD(13) WRITE_ZEROES(14) INDIRECT_DESC(28) EVENT_IDX(29) VERSION_1(32) RING_RESET(40)
```

`VIRTIO_BLK_F_RO` is **not** offered, so the device is writable. `FLUSH` **is**
offered, and this driver negotiates only `VERSION_1` — neither `FLUSH` nor
`CONFIG_WCE`. §5.2.6.2's first stability case is exactly that shape:

> "A write becomes stable once it is completed and one or more of the following
> conditions is true: 1. neither VIRTIO_BLK_F_CONFIG_WCE nor VIRTIO_BLK_F_FLUSH
> feature were negotiated, but VIRTIO_BLK_F_FLUSH was offered by the device"

and

> "If the device is backed by persistent storage, the device MUST ensure that
> stable writes are committed to it, before reporting completion of the write"

**So VirtIO 1.4 classifies this completed write as stable under case 1.** That
is what the contract says about it.

**Stage 4D-5 does not independently measure the host's persistence
implementation, and claims nothing about it.** The backing drive is configured
`cache=writeback` — measured over QMP on a drive configured exactly as the
profile configures it: `{"no-flush": false, "direct": false, "writeback": true}`
— and whether QEMU disables that cache for a driver that negotiates neither
feature was not established here. Nor was what the `isa-debug-exit` path does
with pending writes. Neither question needs answering for this claim, and
neither is answered by it.

In particular: **the post-run image contents do not prove commit-at-completion**,
and a live read of the raw file would not either, because the host page cache
can show bytes that are not durably committed. A durability or flush claim needs
a different evidence design and belongs to a slice of its own.

## 8. The geometry, in one 8 KiB region

```text
desc        0 ..  2048   2048 B   align 16    16 * 128
avail    2048 ..  2310    262 B   align  2     6 + 2 * 128
used     2312 ..  3342   1030 B   align  4     6 + 8 * 128     queue end 3342

header W 3344 ..  3360     16 B   align 16    device-readable
payload  3360 ..  3872    512 B   align 16    device-readable
status W 3872 ..  3873      1 B               device-writable
header R 3888 ..  3904     16 B   align 16
buffer R 3904 ..  4416    512 B   align 16    device-writable, poisoned 0x3C
status R 4416 ..  4417      1 B               device-writable
                                              request end 4417 <= 8192
```

**Eight KiB because two independent buffers are a witness requirement**, not
because Stage 4D-4 used it. The blocks need 1075 bytes after 3342 and a 4 KiB
region has 754 — short by 321. Sharing the header or the status byte still lands
at 4401 and 4400, and shrinking the queue to fit would change the substrate away
from Stage 4D-3's, which is the opposite of what this slice wants.

Six device addresses from six bounded offsets, one region, `capability_delta=1`
(ADR-0084 §6b). Nothing adds to an address and no scalar address is in the
source.

## 9. The substrate is Stage 4D-3's, deliberately

One initialized queue, a four-descriptor pool spending three per request, the
two persistent ring counters, the FIFO free list, one MSI-X source, one DMA
region. The write is completed, consumed and reclaimed before the read-back is
built, and the read-back is served by the write's reclaimed descriptors —
exactly what Stage 4D-3 proved.

Stage 4D-4's two-outstanding substrate is not used. A first write does not need
concurrency, and one new claim per slice.

## 10. Runtime evidence

```text
TOS.RUN.COMPLETED value=i64:36081574113968127

  proof mask   536870911   the complete set of 29; no partial run can produce it
  capacity     32768       sectors, as the device reported them
  attempts     1           generation reads before it settled
  queue size   128
```

| Fact | Bit |
|---|---|
| reset observed, `VERSION_1` offered, `FEATURES_OK` accepted | 1, 2, 4 |
| queue 0 exists; size accepted; layout fits; addresses aligned | 8, 16, 32, 64 |
| `queue_desc`, `queue_driver`, `queue_device` all read back | 128 |
| MSI-X entry accepted; `queue_enable` 1; `DRIVER_OK` set | 256, 512, 1024 |
| the notification capability was found and validated | 2048 |
| the 16-bit wrap arithmetic holds across 65535 → 0 | 4096 |
| the pool started whole | 8192 |
| **the device-specific configuration capability was found and validated** | 16384 |
| **`capacity` was read under a generation that did not move** | 32768 |
| **the capacity admits sector 5**, before anything was published | 65536 |
| **the write payload and the read-back buffer are disjoint** | 131072 |
| **the write's payload descriptor is device-readable**, read back from the table | 262144 |
| the write's `used.id` is its chain head | 524288 |
| **the write's `used.len` is exactly 1** | 1048576 |
| the write's status is `VIRTIO_BLK_S_OK` | 2097152 |
| the write's chain was reclaimed | 4194304 |
| **the payload buffer was scrubbed and the scrub verified** | 8388608 |
| the read's `used.id` is its chain head | 16777216 |
| **the read's `used.len` is exactly 513** | 33554432 |
| the read's status is `VIRTIO_BLK_S_OK` | 67108864 |
| **all 512 bytes are the payload, recomputed from the index** | 134217728 |
| the pool is whole again | 268435456 |

```text
TOS.RUN.PCI_ASSIGNED  ... generation=1 express=1 dma=1
TOS.RUN.DMA_REGION    ... bytes=8192 contiguous=1 capability_delta=1
TOS.RUN.IRQ_DELIVERED ... deliveries=1 woke=1 latched=0      the write
TOS.RUN.IRQ_DELIVERED ... deliveries=2 woke=1 latched=0      the read-back
TOS.RUN.ACCOUNTING fuel=105432/4194304 depth=7/16 allocation=448/4096
```

Three consecutive runs, each a fresh boot: identical value, identical fuel.

## 11. Host evidence, and exactly what it is for

The gate reconstructs the reference image the profile builds — 16 MiB of zeros
with sectors 1..4 seeded — writes the expected pattern into sector 5, and
compares the post-run backing file against it byte for byte. Measured: **exact
match**, sector 0 still zero, sectors 1..4 still `0xC1`..`0xC4`, sector 6 still
zero, 16 777 216 bytes.

**This is a target-and-location corroboration, not a durability claim.** What it
catches is a write that landed on another sector, any collateral change, and one
case the guest cannot see at all — and that case is why it exists:

> **Measured.** With the target sector changed to 9 in the vector and the gate
> still expecting 5, the guest reported `36081574113968127` — the complete,
> correct value, byte-identical to a passing run, because it wrote and read back
> sector 9 self-consistently. Only the host's image check refused it.

What it does **not** say is anything about when or whether the bytes were
committed to host storage. The guest evidence in §10 stands alone as proof that
a correct VirtIO request was issued and answered; the host never substitutes for
it.

## 12. Negative evidence

**Against the host judgement.** Seven crafted values, including a correct one
that must be accepted; a run that never proved the read-back matched; a
device-writable payload; a write reporting the read's `used.len`; an unscrubbed
payload buffer; a capacity that excludes the target sector; and the module's own
refusal shape.

**Against the module, on the real device.** Each mutation reverted:

| Mutation | Result | Kind |
|---|---|---|
| `T_IN` left in the write header | `-62` — the read-back's 512 bytes are not the pattern. `used.len` and the status both passed | hardware |
| write payload descriptor marked device-writable | refused **before the boot** by the source check; with that suspended, `-59` — QEMU reported `used.len` 513 for a chain whose writable extent had grown | static, then hardware |
| write lands on sector 9, read-back asks sector 5 | `-62` | hardware |
| **write and read-back both use sector 9** | the guest **passes** with the correct value; the host image check refuses it | host |
| read-back aliased onto the write payload buffer | refused **before the boot**: "the read branch does not read into the independent buffer" | static |
| `dma_consume` removed before reading the used ring and status | the boot **passes** — see below | static |
| a 4-sector device | `-44` before publication, 0 interrupts | hardware |

**Two negatives are static, and the reason is measured rather than asserted.**

- **The missing `dma_consume` cannot be witnessed on this profile.** Removing the
  call leaves the boot reporting a byte-identical correct result, because
  ADR-0086 §11 lowers it to a compiler and execution barrier in ring 3 and x86
  under QEMU makes the device's writes visible anyway. A hardware race here
  would be nondeterministic and is not valid evidence, so the gate checks the
  source: `dma_consume(region)` must precede every read of the used ring and of
  the status byte.
- **A payload modified after publication** is enforced the same way. Once the
  chain is device-owned the driver may not touch its memory; the gate requires
  the write payload's extent to be referenced exactly twice — the scrub's store
  and the check that it took — both after the chain came back.

## 13. Architectural invariants

**The nucleus learned nothing.** No ABI operation, interface version, capability
type, Core minor, authority, endowment, `tos-ir/v1` field or `TOSBUNDLE`
version, and no ADR. `VIRTIO_BLK_T_OUT`, sectors, block direction, write
buffers, flush and persistence are words that occur only above the boundary. The
device-specific configuration is reached through the same generic PCI capability
reads and the same mapped BAR window that Stage 4B established; reading it
needed no new mechanism. That the selected structure is *in* that window is a
fact about the reference endpoint which the vector checks rather than assumes.

**The Stage 4 profile is unchanged.** The harness gained one opt-in
`--stage4-block-sectors` flag used only by this gate's capacity negative; with
it absent, every other gate builds the same 16 MiB image with the same seeded
sectors as before, byte for byte.

**Stage 4D-1, 4D-2, 4D-3 and 4D-4 are untouched**, source and module digest
alike.

## 14. What this is not

- **not** a durability, persistence, flush or crash-consistency claim;
- **not** a second queue, an indirect descriptor, an event index, a packed
  virtqueue, a queue reset or a queue resize — none of those features is
  negotiated;
- **not** a general block-configuration reader: eight bytes of the mandatory
  prefix, and the optional fields after it are neither read nor required to
  exist;
- **not** a driver for arbitrary VirtIO endpoints: a device-specific
  configuration in a BAR other than the already mapped one is refused rather
  than mapped, because a second mapping for one evidence slice is scope this
  claim does not need;
- **not** a block service, a filesystem, a cache, a scheduler or a driver
  framework, none of which is designed.

## 15. One derived digest moved, and why

The two corrections in this document — the capability-order citation and the
same-BAR qualification — are comments, and comments are source bytes. The
canonical source therefore changed and so did the module digest derived from it:

```text
tests/vectors/virtio-block-write/init.tos
  module digest   before  sha256:6b1522f695aeed20c026ab312fb693492d735f1a83c71e8a296c6f4c32382ae4
                  after   sha256:2a9204b1467e5bb465bb327e190c578ddc18c3bd8ebabd405660e584f817dd60
```

**No behaviour moved with it.** The gate was re-run against the corrected source
and the device-visible witness is byte for byte the one recorded above —
`value=36081574113968127`, `fuel=105432`, capacity 32768 in one generation
attempt, both used lengths, both statuses, all 512 bytes. Stage 4D-1, 4D-2,
4D-3 and 4D-4 are untouched, source and module digest alike.

## 16. Reproduction

```sh
bash source/host-tools/qemu-test/virtio-block-write.sh
./scripts/preflight.sh --profile qemu
```
