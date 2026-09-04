<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4B — VirtIO capability discovery, and an architecture STOP on MMIO access

- Status: **evidence, 2026-09-04.** The textual half that needs no new decision
  is built, green and gated: canonical TOS Core discovers the real modern VirtIO
  PCI capability structures of the QEMU `virtio-blk-pci` device. **BAR and MMIO
  authority stopped** on a language question the round's own rule 8 and rule 20
  name as STOP conditions — see §4
- Gate: `source/host-tools/qemu-test/virtio-caps.sh`, in preflight as
  *QEMU textual VirtIO capability discovery* (`qemu` profile, `full-only`)
- Related: ADR-0079 (the PCI authority model this extends), ADR-0080 (the effect
  model that made a runtime capability usable), ADR-0037 §2, ADR-0075, ADR-0076,
  `PLATFORM_INTERFACE_V1`, `docs/39` §2, `docs/40` §"Region", `docs/11` §Interrupts

## 1. What was built

```text
root platform.pci.Bus            ← Stage 4A
    ↓  pci_function_claim
platform.pci.FunctionConfig      ← Stage 4A
    ↓  pci_config_read × N
the device's PCI capability list ← Stage 4B, built and gated
    ↓  textual VirtIO parser
common / notify / ISR / device cfg, each with BAR, offset and length
    ↓
bounded BAR authority            ← STOPPED (§4)
    ↓
bounded MMIO mapping/access      ← STOPPED (§4)
    ↓
real VirtIO registers            ← not reached
```

`source/tests/vectors/virtio-caps/init.tos` is a TOS Core 1.1 module holding one
capability — the root PCI bus authority. It claims 00:04.0, walks the device's
capability list through `pci_config_read`, identifies the vendor-specific
entries, and reads each one's `cfg_type`, BAR index, offset and length.

**Everything VirtIO-specific in this system is in that file.** The gate checks
that mechanically: it strips comments from `nucleus/src/*.rs` and fails if the
word appears in ring-0 code at all.

## 2. What the device reported

```text
found            common | notify | ISR | device   (all four, from cfg_type)
BAR index        4, for every one of the four
common config    offset 0x0, length 0x1000
well-formed      every one had a BAR index in range and a non-zero length
```

That is QEMU's modern `virtio-blk-pci` layout, and the module contains none of
it: no capability identifier, no `cfg_type` constant compared against a literal
this document could have supplied, and no BAR number.

**The traversal is bounded in the module**, at 64 entries — no well-formed chain
can exceed that in 256 bytes of configuration space — so a device that returned a
self-looping list could not hang the process reading it.

### Negatives, executed

| Case | Result |
|---|---|
| wrong transport | the same device class over the **transitional** transport reports device 0x1001 and carries no modern structures. The parser reports **none found** rather than falling back on defaults |
| no device at all | an absent function reads all-ones, so the capability pointer is `0xFF` — outside the range a capability may begin at. The traversal refuses it and reports nothing found and not well-formed |
| unbounded chain | every run terminated on the module's own bound, not on a harness timeout |
| trusted-base boundary | no VirtIO identifier appears in nucleus code |

## 3. What a BAR value is, and is not

The module read BAR *indices* out of the capability structures, and Stage 4A's
module reads BAR *registers* as data. Neither is authority:

- a BAR register holds a number the device reported;
- no operation of any accepted schema accepts one;
- there is nowhere to present it, so it cannot become a mapping.

Deriving authority over the memory a BAR describes is the decision this slice
stopped on.

## 4. The STOP: TOS Core cannot read through a region

**Found by audit, verified in three places, and not worked around.**

The round's preferred model (§7 of the ruling) is a bounded **mapped**
device-memory capability rather than a syscall per register access. The
mechanism for mapping it exists — §5 records that. What does not exist is any
way for canonical text to *read* through a mapping.

### The smallest unreachable operation

> A textual module holding a mapped device-memory capability loads the 32-bit
> value at offset 0 of the VirtIO common configuration.

### Why it is unreachable

Three layers, each checked rather than inferred:

1. **`docs/39` §2 declares no access operation.** The `predeclared-function`
   inventory is `to_i8 … to_u64`, `wrapping_add`, `wrapping_sub`,
   `wrapping_mul`, `share`. There is no `read`, no `write`, no `slice`.
2. **Region indexing is untyped.** `crates/tos-core/src/typing.rs`'s
   `index_type` yields an element type for `Array` and `slice` and
   `Type::Unknown` for everything else, so `region[i]` has no type.
   `E1211_INDEX_TYPE_MISMATCH` in `docs/44` mentions "an array, slice or region
   index", which is the registry describing a capability the language does not
   have.
3. **No operation produces a region either.** `SYSTEM_INTERFACE_V1` §8: "No
   operation of this version takes or returns a region."

So `Region<T>` is, today, entirely unreachable from TOS Core source: nothing
creates one and nothing reads one.

### The accepted contract already describes what does not exist

`docs/40` §"Region" says safe code "may obtain it only from an authority-bearing
typed service operation, access it only with checked `read`, `write`, or `slice`
contracts". **Those contracts are named and nowhere defined** — not in `docs/39`'s
grammar, not in its predeclared list, not in the implementation.

This is the same shape as the gap ADR-0037 §5 found for `share`: a companion
document describing an operation the grammar could not express. That one was
closed by adding `share` to the predeclared list, through an accepted decision.

### And MMIO needs more than an access operation

Even given region access, MMIO is not ordinary memory. **No accepted document
uses the word "volatile"** — `docs/40`'s evaluation-order rules are about a
module's own expressions, and say nothing about loads that must not be
coalesced, elided, reordered or speculatively repeated. A device register read
twice is two hardware events; a compiler or engine free to treat it as one would
be correct under every rule TOS Core currently states.

### Both are the round's own STOP conditions

Rule 8: *"If textual MMIO access requires a new TOS Core memory primitive or
volatile type whose semantics are not already accepted, STOP and present that as
an explicit Architect decision."* Rule 20: *"the only viable model requires a new
TOS Core memory/type primitive."*

### This blocks Stage 4D regardless of what Stage 4B chooses

The load-bearing point. A VirtIO **queue** lives in DMA memory the driver writes
descriptors into — a `DmaRegion<mut T>`, which has exactly the same problem: no
accepted way to read or write through it from source. So the region-access
primitive is required for Stage 4D whichever way MMIO is decided, and choosing
ABI operations for MMIO would defer the question rather than remove it.

## 5. What is *not* blocked, and is ready

Recorded so the decision is taken against what exists rather than against an
unknown.

**Mapping mechanics are present.** `nucleus/src/paging.rs` already has
`WRITE_THROUGH` (PWT) and `CACHE_DISABLE` (PCD), and `map_page` takes an explicit
physical address — so an uncacheable, non-executable, user-visible device mapping
is expressible today with no new paging work.

**Page tables are already outside the memory account.** ADR-0076 §2 keeps them in
the proved reserve, and `process::region_mapping_bound` bounds what one process's
mappings can cost. A device aperture is another bounded consumer of that reserve,
not of any `MemoryAuthority`.

**The object model is clear and does not conflict.** A device-memory object is
*not* an ADR-0075 `RegionObject`: nothing is charged, nothing returns to the
allocator, and its backing is pre-existing external hardware state named by a
physical address the model deliberately keeps private for pool memory. It needs
its own class, which is a system-level decision and not a language one.

**Exclusivity gives BAR sizing a home.** Stage 4A made a function assignment
exclusive under its root, so a sizing probe performed once at claim time, under
that exclusivity, has no concurrent holder to disturb. That was the other thing
the round asked to be checked before implementation (rule 5), and it is not a
blocker.

**Nothing here needs interrupts.** Rule 19's boundary is intact: the liveness
mismatch remains a Stage 4C prerequisite and this slice does not approach it.

## 6. The decision surface

| Option | What it needs | Consequence |
|---|---|---|
| **A — mapped device region** (the stated preference) | a new device-memory object class *and* a TOS Core region-access primitive *and* volatile/ordering semantics | the performant model, and the one Stage 4D needs anyway. Two accepted-contract additions: a language operation and its memory semantics |
| **B — MMIO mechanism operations** | nothing new: the shape of `pci_config_read` | works today. Defers the language question rather than answering it, and Stage 4D reopens it for DMA |
| **A′ — decide the language primitive now, for regions generally** | the same additions as A, decided once for `Region`, `DmaRegion` and device memory together | closes `docs/40`'s existing gap, unblocks 4B and 4D at once, and is a larger decision than MMIO alone |

**Not recommended by this document**, because the choice is the Architect's and
the round's rule 8 asks for it to be presented rather than taken. What this
document does claim is that B is not a way of avoiding the question.

## 7. Performance pre-check

Asked before choosing, as rule 18 requires. The steady-state VirtIO block
request path touches MMIO far less than it might appear:

| Step | Where it lives | Cost under B (ABI ops) |
|---|---|---|
| descriptor preparation | **DMA memory**, not MMIO | no MMIO crossing |
| available-ring update | **DMA memory** | no MMIO crossing |
| notify doorbell | **MMIO write** | one crossing, **per batch** rather than per request |
| interrupt completion | ISR read (MMIO), or none with MSI-X | at most one crossing per interrupt |
| used-ring read | **DMA memory** | no MMIO crossing |

So option B costs roughly **one crossing per batch and one per interrupt**, which
does not by itself breach the ≤4 handoffs per request budget, and batching
amortises it further. The budget pressure is not where it first appears.

**The real pressure is on descriptor memory**, which is DMA rather than MMIO —
and that is precisely what §4 shows is blocked either way. A design that required
a syscall per descriptor field *would* breach the budget, and neither option here
proposes one.

Nothing decided in this slice introduces a global lock, a per-request allocation
or a per-descriptor crossing.

## 8. What the nucleus gained in this slice

**Nothing.** No ring-0 code was added. The VirtIO capability walk is entirely
textual, over an operation Stage 4A already accepted, and the gate enforces that
the word VirtIO does not appear in nucleus code.

## 9. What remains before Stage 4C

- the §4 decision, which gates BAR authority, MMIO access and — through
  `DmaRegion` — Stage 4D;
- the device-memory object class and its lifecycle against the function
  assignment's generation, which is designed but not built because it has
  nothing to serve until §4 is answered;
- the `SYSTEM_ABI_V1` §6 liveness rule, unchanged and still a mandatory
  prerequisite to the first routed device interrupt.
