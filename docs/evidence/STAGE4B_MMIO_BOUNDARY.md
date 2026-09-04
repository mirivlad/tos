<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4B — BAR/MMIO authority, and canonical text reading real device registers

- Status: **evidence, 2026-09-04. Stage 4B is complete and green.** Canonical
  TOS Core 1.2 discovers the real VirtIO PCI capability structures, derives a
  bounded read-only window on the BAR they name, and reads the device's common
  configuration through a capability-confined MMIO mechanism
- Decision: `docs/adr/0081-region-access-and-device-memory.md`, **Accepted**
  (Vladimir Tomashevskiy, 2026-09-04). It resolved the architecture STOP this
  document previously recorded
- Gates: `virtio-caps.sh` (*QEMU textual VirtIO capability discovery*) and
  `virtio-mmio.sh` (*QEMU textual VirtIO register read*), both `full-only`
- Related: ADR-0079 (PCI authority), ADR-0080 (effects name interfaces),
  ADR-0037/0075/0076 (**not** widened to cover device memory),
  `PLATFORM_INTERFACE_V1`, `SYSTEM_ABI_V1` §5 operation 27

## 1. The chain, end to end

```text
boot/platform
    ↓  minted once by the launcher, scope in the record
root platform.pci.Bus
    ↓  pci_function_claim — BARs measured here, once, under exclusivity
platform.pci.FunctionConfig
    ↓  pci_config_read × N — the capability list, walked in canonical text
the device's own VirtIO capability structures
    ↓  pci_bar_map_read — a bounded window on the BAR those structures named
MmioRegion
    ↓  mmio_read_le_u16 / mmio_read_u8 / mmio_read_le_u32
real VirtIO common configuration registers
```

Every row is exercised from canonical TOS Core on the real QEMU machine and held
by a gate. **No host fixture, binary helper or nucleus-side VirtIO parser
supplies any answer**, and a gate checks the last of those mechanically.

## 2. What a BAR value is, and what an MMIO capability is

| | |
|---|---|
| a **BAR register** | a number the device reports. Read as data through `pci_config_read`. No operation of any accepted schema accepts one, so it cannot become a mapping |
| a **BAR index** | which of six registers to derive from. A caller names one; it is not authority either |
| an **`MmioRegion`** | authority over a bounded, page-granular window whose physical base the *nucleus* took from the live assignment's own measured BAR state. The holder never learns an address |

**The caller never supplies a physical address.** It names a BAR index and a
page-aligned offset and length; a request not entirely inside the measured
extent is refused rather than clamped.

## 3. Where an MMIO capability comes from, and how it ends

`pci_bar_map_read` and `pci_bar_map_write` are two schema operations over one
ABI selector (27), differing in the type they produce. A writable window is
asked for by calling the *other* operation, not by passing a flag — so a module
cannot arrive at one by computing a number.

**Lifetime is a descendant relation, not a handle relation** (ADR-0081 §14). The
assignment stays live while *either* a `FunctionConfig` capability names it **or**
a mapping exists under it:

```text
release the last function handle → the assignment does NOT end
                                   while a window still reaches it
release the last window          → now nothing reaches it: the assignment
                                   ends and its generation advances
```

That is what makes this impossible: release the function, re-claim the same BDF,
and have an old window silently reach the new assignment. And it means a manager
releasing its own handle does not destroy a driver's window.

**Process death** unmaps that process's device pages, frees its mapping slots
and tells each assignment it has one fewer descendant — so an assignment whose
driver died becomes releasable rather than staying live forever, and no
untracked mapping can survive.

## 4. Why it is not ordinary funded RAM

ADR-0075 and ADR-0076 were **not** widened. A device window is not a
`RegionObject`:

- nothing is charged to a `MemoryAuthority` — that account is about pool frames;
- nothing returns to the physical allocator when it is released;
- its backing is pre-existing external hardware state.

So `RAM allocation authority ≠ device mapping authority`: a process holding a
`MemoryAuthority` gains no device access, and a process holding a window gains
no ordinary physical memory. The page tables that map it come from the proved
reserve, as every mapping's do, and `process::device_mapping_bound` puts an
explicit term in it.

## 5. BAR sizing

Performed **once per assignment, at claim time**, under the exclusivity Stage 4A
established — nothing else can hold the function, so nothing else can be probing
it, and a later mapping never repeats a destructive probe on a device somebody
is using.

The standard sequence, with memory decoding disabled for the duration and
restored afterwards including on every refusal path: read the original, write
all-ones, read back which bits the device left clear, restore the original
exactly. 64-bit BARs are handled as pairs and the slot above one is not treated
as a BAR of its own. An I/O BAR is recorded as non-memory and never becomes
authority; an unimplemented BAR reads back zero and never becomes authority.

This teaches the nucleus **PCI BAR mechanics and nothing about any device
class.**

## 6. Mapping and observability are two contracts

**Cache attributes** are the processor's half: a window is mapped user-visible,
non-executable, read-only or read/write per the granted form, with `PCD` and
`PWT` both set — which under the reset-state PAT selects `UC`: no caching, no
write combining, no speculative reads, accesses reaching the device in program
order.

**Observability** is the language's half (ADR-0081 §9): one source access is
exactly one hardware access of the declared width — not elided, coalesced,
duplicated, widened, narrowed or reordered against another. It is carried by
`MmioRead`/`MmioWrite` being their own verifier-visible IR operations and by the
host performing a volatile access per operation.

**Neither implies the other**, and "PCD/PWT means volatile" is precisely the
confusion that separation exists to refuse.

A read-only grant is read-only in **both**: the type refuses the write at check
time, and the page table has no `WRITABLE` bit if it somehow got there.

### Ordering, bounded honestly

> Stage 4B's device ordering contract orders MMIO observations with each other.
> It does not yet assert DMA visibility before a notify, or DMA completion
> visibility after an interrupt.

A cross-domain rule between CPU memory, DMA memory and device MMIO belongs to
the queue slice. Deciding it under pressure from a read-only probe would be
deciding the portable DMA memory model by accident.

## 7. What the module read

`source/tests/vectors/virtio-mmio/init.tos`, TOS Core 1.2, holding one
capability: the root PCI bus authority.

```text
TOS.RUN.PCI_ASSIGNED  process=0 segment=0 bus=0 device=4 function=0 generation=1
TOS.RUN.MMIO_MAPPED   process=0 ... bar=4 offset=0 length=4096 access=read_only
TOS.RUN.COMPLETED     value=i64:72340168526266369

  num_queues         1        matches the profile's num-queues=1
  device_status      0x00     untouched: this probe sets no status bit
  config_generation  0
  queue 0 size       256      a real, power-of-two queue size
  device_feature     0x0101   non-zero, so the window is live and not zeros
```

**The module contains none of those numbers**, and neither does the harness
except as expectations to compare against. The BAR index came from the device's
own capability structure, the physical base from the nucleus's measurement of
that BAR, and the register values from the device.

**Read-only throughout.** No configuration write, no BAR programmed, no queue
created or configured, `DRIVER_OK` never set, the device never notified, no
interrupt and no DMA.

## 8. Negative authority evidence

**Executed**, by canonical text against the real machine.

Eight mapping refusals in one module, reported as 255, with **no window produced
by any of them**:

| bit | case | status |
|---|---|---|
| 1 | BAR index outside the architectural range | `E_BAD_ARGUMENT` |
| 2 | an unimplemented BAR is not authority | `E_NO_CAPABILITY` |
| 4 | an offset that is not page-aligned | `E_BAD_ARGUMENT` |
| 8 | a length that is not page-aligned | `E_BAD_ARGUMENT` |
| 16 | a zero-length window | `E_BAD_ARGUMENT` |
| 32 | a window reaching past the BAR's extent | `E_NO_CAPABILITY` |
| 64 | an offset plus length that overflows | `E_BAD_ARGUMENT` |
| 128 | a function with no BARs cannot be mapped — which is also why authority over one function reaches no other, since the base comes from *that* assignment's measured state | `E_NO_CAPABILITY` |

And separately, because a refused access ends the process rather than returning
a status a module could have handled:

- **an access past the end of its own window** refuses with
  `RUNTIME_DEVICE_REFUSED`, naming the bound it broke, **before the device is
  touched**.

**Statically refused**, in `tests/integration/tests/device_memory.rs`: a write
through a read-only `MmioRegion`; an ordinary `Region` where a device window is
required; a non-`size` offset; and the feature used by a module declaring 1.0 or
1.1.

**Structural**: a numeric BAR value cannot become authority, because no
operation of any accepted schema accepts one.

**Differential**: with the device removed, the same probe reports a refusal
rather than a reading, and maps nothing.

## 9. What the nucleus gained, and what it still does not know

Added: BAR sizing and the measured extents (`pci.rs`), a device-memory object
with its assignment-descendant lifecycle (`device.rs`), a page-granular
uncacheable mapping into a process and its own aperture (`process.rs`), and ABI
operation 27.

Not added, and enforced by a gate that strips comments from ring-0 code and
fails on the word: **VirtIO**. The nucleus does not know what
`VIRTIO_PCI_CAP_COMMON_CFG` means, where `device_status` lives, what a queue is,
or that this is a block device. All of that is in canonical text.

## 10. Performance pre-check

The steady-state path the mapping model makes possible:

```text
driver writes descriptors    direct mapped memory     no crossing
driver updates avail ring    direct mapped memory     no crossing
driver performs MMIO notify  direct mapped MMIO write no crossing
device completes
driver reads used ring       direct mapped memory     no crossing
```

**No nucleus crossing per descriptor field and none per MMIO register access.**
Mapping is a one-time act at initialisation. The later interrupt path will add
scheduler/IPC crossings, and Stage 4B consumes none of the ≤4-handoff budget
before IRQ exists.

## 11. Gates

| Gate | State |
|---|---|
| *QEMU textual VirtIO register read* (new) | **green** |
| *QEMU textual VirtIO capability discovery* | green |
| *QEMU textual PCI function claim* | green |
| every Stage 1–4A gate | unchanged and green |
| Stage 4 identity gate | **not claimed.** No persistent data has moved |

## 12. Before Stage 4C

- the `SYSTEM_ABI_V1` §6 liveness rule — "nothing runnable **and nothing routed
  can change that**" — remains implemented as its first half only, and remains a
  **mandatory prerequisite to the first routed device interrupt**. Stage 4B
  needed no interrupt and did not approach it;
- DMA allocation and origin, device-visible address representation, coherency
  and IOMMU are untouched, and ADR-0081 §11 deliberately did not decide the
  cross-domain ordering they need;
- reset, feature negotiation beyond read-only observation, queues and block I/O
  are untouched.

**No architecture STOP remains open before Stage 4C.**
