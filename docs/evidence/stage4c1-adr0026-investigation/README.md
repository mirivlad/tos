<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0026 construct-validity investigation

- Status: **investigation report, 2026-09-05.** Diagnostic only. No conformance
  threshold is changed, no ADR is amended, and nothing here is merged to `main`
- Ordered by: the Project Architect's ruling on the Stage 4C-1 performance STOP
- Subject: does ADR-0026's ratio still measure the quantity its accepted
  decision says it measures?
- Verdict: **no, on the mandatory TCG profile — Case A**
- The Stage 4C ownership repairs remain on `stage4c1-ownership-repairs`
  (`6fc0bf5`) and are **not** merged. `main` remains at `1c3bb49`

## 0. The question, and the answer

ADR-0026 interprets `full_exact_p95 / unavoidable_crypto_p95` as a cap on
non-cryptographic validation overhead at 30% of unavoidable cryptographic cost.

The numerator is the production boot path; the denominator is a **separately
linked** `test-crypto-baseline` artifact. Two images, two layouts, moving
independently.

**A semantically inert change that executes nothing, adds no reachable work, and
leaves the raw image byte-for-byte the same length moves the ratio from ~1.11 to
1.546 — across the 1.30 bound.** The same change moves the native ratio not at
all. The cause is identified exactly, and it is a property of the emulator, not
of the system under test.

## 1. Preserved evidence

| | base | Stage 4C |
|---|---|---|
| tree | `1c3bb490b1e4d688cf208498cda1287a0d5df6ed` | `6fc0bf575f932e6ef0ed495aed59b64d8dee3247` |
| production nucleus sha256 | `4cf4fa35f6ed3e60fb2636b5ba37ccc47c30202df4a8b063f2489c9fc33aef01` | `65a95ae8ed45936e64dd0fb8ede4592df6cdf22d57d8145dbc14197887cdf4aa` |
| production raw bytes | 179312 | 179504 |
| crypto-baseline sha256 | `8ecf7014f3238245694c0d4387a9d27f…` | `5dbebfeed572a6fb6cd97e63e6682301…` |
| crypto-baseline raw bytes | 134216 | 134408 |

Section and segment tables: `elf-comparison.md`. Hot-path symbol and instruction
comparison: `hot-path-comparison.md`. Raw 3+21 sample sets, medians, p95 and p99
for every run are retained under the run directories named in each table below.

**The failing samples are retained as measured.** Nothing was re-run to obtain a
better number, and no successful retry replaces a failing one.

## 2. Did the executed validation work change?

No. Every observable is identical between the base and Stage 4C runs of the
timed Stage 1 workload:

| Property | base | Stage 4C |
|---|---|---|
| capsule bytes | 16882164 | 16882164 |
| capsule sha256 | `6dabadc666f46a75…` | `6dabadc666f46a75…` |
| validation count | `files=1000` | `files=1000` |
| SHA byte count | 101203397 | 101203397 |
| SHA invocation count | 2007 | 2007 |
| loader/nucleus event ordering | sha `90583768709338b5` | sha `90583768709338b5` |
| `TOS.MEM.RESERVE` | identical line | identical line |
| `TOS.MEM.ACCOUNT` | identical line | identical line |
| `nucleus_space_actual_frames` | 23 | 23 |
| `pci_function_claim` occurrences | 0 | 0 |
| `PCI_NORMALISED` / `PCI_ENABLES` / `PCI_ASSIGNED` | 0 | 0 |

The only caller of every Stage 4C ownership operation is the
`pci_function_claim` syscall, which this workload never makes. **No Stage 4C
code executes in the measured path.**

### Machine code

From the `TOS_NUCLEUS_ELF` audit build — same objects, same linker script:

| section | base | Stage 4C | delta |
|---|---|---|---|
| `.text` | addr `0x2000000` size `0x1c4af` | addr `0x2000000` size `0x1cfdf` | size **+2864** |
| `.rodata` | addr `0x201d000` size `0x2211` | addr `0x201d000` size `0x2289` | size +120 |
| `.data` | addr `0x2020000` size `0xbbf8` | addr `0x2020000` size `0xbcb8` | size +192 |
| `.got` | addr `0x202bbf8` | addr `0x202bcb8` | addr +192 |
| `.bss` | addr `0x202bc70` size `0xaf50` | addr `0x202bd30` size `0xaf50` | addr +192 |

Answering the four questions separately:

1. **Instruction bytes changed?** No. `check_path`, `decode_file_entry`,
   `decode_path_entry`, `rd_u64`, `update_detached_identity` and
   `Sha256::finalize` are instruction-identical. `Sha256::update` and
   `Sha256::compress_block` differ in **2 instructions of 84 and 179**, and
   every one of those is a rip-relative displacement (`call *0x1e812(%rip)` →
   `call *0x23e5a(%rip)`). Same opcodes, same registers, same counts.
2. **Only addresses/alignment changed?** Yes. Every hot symbol moved by exactly
   **−21904** bytes.
3. **Unrelated code insertion shifted the hot symbols?** Yes — that is the whole
   of the change to the timed path.
4. **ELF segment/page layout changed?** `.text` is padded to a 4 KiB boundary
   and both builds occupy the same page count, which is why the memory account
   is identical. Within `.text`, everything after the insertion point moved.

## 3. Controlled inert-layout experiment

Two families, both starting from the known-green pre-Stage-4C tree, both
introducing an explicitly retained object that performs no work the workload
reaches.

### 3a. Appended pad — image grows, existing code does not move

| variant | `.text` bytes | raw bytes | TCG ratio | native ratio |
|---|---|---|---|---|
| baseline | 115887 | 179312 | 1.174 | 0.999 |
| text +64 | 115951 | 179312 | 1.113 | 1.016 |
| text +128 | 116015 | 179312 | 1.136 | 0.988 |
| text +192 | 116079 | 179312 | 1.095 | 0.997 |
| text +256 | 116143 | 179312 | 1.089 | 1.008 |
| text +512 | 116399 | 179312 | 1.134 | 1.006 |
| text +4096 | 119983 | 183408 | 1.145 | 0.986 |

**No movement**, including the +4096 case which crosses a page boundary and
grows the image. Growing the binary is not the operative variable.

### 3b. Repeatability control — identical binary, measured seven times

The first attempt's pads were garbage-collected by LLD, so those runs measured
the **same binary** (sha `4cf4fa35…`) repeatedly. Retained because it is the
metric's own noise figure:

```
1.168  1.117  1.191  1.174  1.113  1.096  1.095
```

n = 7, range 1.095–1.191, spread ≈ ±3% against a 30% budget.

### 3c. Displacing pad — existing code moves, image length unchanged

The pad is emitted into the first `.text` group, so it moves every existing
function instead of being appended after them. For every size below 4096 the raw
image length does not change at all.

| shift | raw bytes | `compress_block` | straddles a 4 KiB page? | TCG ratio | nucleus validation | native |
|---|---|---|---|---|---|---|
| baseline | 179312 | `0x200d400` | no | 1.174 | 1182.4 ms | 0.999 |
| +512 | 179312 | `0x200d600` | no | 1.126 | 1097.9 ms | 1.002 |
| +1024 | 179312 | `0x200d800` | no | 1.086 | 1094.9 ms | 0.992 |
| +2048 | 179312 | `0x200dc00` | no | 1.115 | 1155.5 ms | 0.992 |
| **+2864** | **179312** | `0x200df30` | **YES** | **1.546** | **1987.7 ms** | 0.999 |
| +4096 | 183408 | `0x200e400` | no | 1.120 | 1179.5 ms | 1.003 |
| *Stage 4C* | *179504* | `0x2007e70` | **YES** | *2.09–2.27* | *3263.9 ms* | *0.997–1.001* |

## 4. The mechanism, identified

`Sha256::compress_block` is **576 bytes**. Every passing build places it inside
one guest 4 KiB page. **Both failing builds — and only those — place it across a
page boundary:**

```
baseline    0x200d400 .. 0x200d640   one page
+512        0x200d600 .. 0x200d840   one page
+1024       0x200d800 .. 0x200da40   one page
+2048       0x200dc00 .. 0x200de40   one page
+4096       0x200e400 .. 0x200e640   one page
+2864       0x200df30 .. 0x200e170   CROSSES 0x200e000
Stage 4C    0x2007e70 .. 0x20080b0   CROSSES 0x2008000
```

A QEMU TCG translation block **cannot span a guest page boundary**. When the
hot compression loop straddles one, it is split into additional translation
blocks and pays extra block lookups on every iteration of a loop that runs
2007 times over 101 MB. That is entirely a property of the emulator: the guest
executes the same instructions in the same order either way, which is why the
native ratio never moves.

The 30% budget is therefore being spent, or not, according to where the linker
happened to put one function relative to a 4 KiB boundary.

## 5. Segment decomposition

Medians over all 21 samples. The effect is confined to one phase:

| phase | base | Stage 4C | delta |
|---|---|---|---|
| loader validation | 1366.6 ms | 1364.3 ms | −2.3 |
| loader post-validation | 79.0 ms | 79.4 ms | +0.4 |
| handoff | 0.3 ms | 0.3 ms | 0.0 |
| **nucleus validation** | **1194.4 ms** | **3263.9 ms** | **+2069.5** |
| canonical lookup | 0.4 ms | 0.4 ms | 0.0 |
| post-validation tail | 2112.1 ms | 2164.5 ms | +52.4 |
| isolated crypto baseline (total) | 2337.1 ms | 2322.0 ms | −15.1 |

The denominator artifact also changed (134216 → 134408 bytes) and did **not**
move. The difference is not "non-cryptographic validation work"; it is the same
cryptographic work translated differently.

## 6. Cross-checks

| profile | base | Stage 4C | inert +2864 |
|---|---|---|---|
| **TCG** (mandatory) | 1.174 | 2.09 / 2.10 / 2.27 | **1.546** |
| **native** (research) | 1.014 / 0.999 | 1.001 / 0.997 | 0.999 |
| **KVM** (research) | not obtainable | not obtainable | not obtainable |

**KVM was attempted and is not obtainable on this host.** `/dev/kvm` is present,
but the nucleus does not boot under it — identically for both builds:
`TOS.RUN.UNSTARTABLE reason=no-address-space`, `TOS.MEM.FAIL`, result 71. That
is a pre-existing property of the system under that accelerator and not a
difference between the two builds; it is recorded rather than worked around.

**The movement is TCG-only, with native stable.** Under the ruling's own
interpretation that indicates a TCG/layout measurement artifact.

## 7. Does ADR-0026 still measure what its accepted text claims?

**No, on the mandatory TCG profile.**

The accepted interpretation is that the ratio caps non-cryptographic validation
overhead. For that reading to hold, the ratio must move when and only when
non-cryptographic validation work changes. It does not:

- executed validation work is provably identical (§2), yet the ratio doubles;
- an inert perturbation that changes no image length and executes nothing moves
  it across the threshold (§3c);
- the cause is a guest-page-boundary interaction in the emulator (§4);
- the metric's own repeat noise is ±3% while the effect is 40–100%;
- numerator and denominator are separately linked images, so their layouts —
  and therefore their translation behaviour — move independently, which is the
  structural reason the quotient does not cancel the artifact.

## 8. Recommendation — Case A

`ADR-0026 metric construct validity is falsified on the mandatory TCG profile.`

**Do not widen 1.30.** Widening would encode the artifact rather than remove it,
and the artifact is not bounded by 1.546: it is bounded by wherever the linker
puts one function.

### Proposed direction — same-artifact paired ratio (proposal only, not implemented)

One measurement-only nucleus artifact provides both series, so executable
layout, linker result and translation environment are shared and cancel in the
quotient:

- full exact validation mode using the production validation implementation and
  semantics;
- unavoidable-crypto mode;
- runtime selection between the two, so **the same binary hash produces both
  measured series**;
- same fixture, same firmware, same QEMU, same profile;
- 3 warmups + 21 samples each;
- no reused validation or hash result.

The production nucleus keeps its separate functional boot gate. The paired
artifact's full mode must be **mechanically shown** to execute the same required
logical validations and canonical lookup as production — otherwise the repair
trades one construct-validity problem for another.

**The threshold must not be carried over unexamined.** `1.30` was chosen against
a metric now shown to be confounded; the repaired metric should be measured
first and the evidence brought back for review before any number is fixed.

This amendment is **not implemented**, per the ruling.

## 9. What was not done

No IRQ work: no `platform.irq.Source`, no vector allocator, no MSI-X routing, no
`irq_wait`, no DMA, no Stage 4C-3, no Stage 4D. `Assignment` was not repacked,
reordered, padded or shrunk. No threshold was changed. The ownership-repair
branch is not merged.
