<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0085 implementation — what is closed, and what is blocked

ADR-0085 (Accepted, Project Architect-approved 2026-09-08) separates an
interface's identity from the class of TOS Core values that represents it. Its
six acceptance obligations are implemented and green. **Its §16 conformance set
is not complete**, and this record says exactly which rows are not and why,
because "conformance complete" is a claim the repository has to be able to
check.

**Do not read this as ADR-0085 conformance complete.** Three §16 rows are
recorded `BLOCKED` below and one is `PARTIAL`. They are not waived, not N/A, and
not satisfied by the analogous tests other interfaces have.

Every one of them is blocked on the same single thing, and **§6 records what
that turned out to be**: the accepted Stage 4 reference machine's function has
no PCI Express capability, so ADR-0084 §5c's P1 cannot hold and the nucleus
refuses to grant DMA authority at all. Nothing is missing from the language, the
schema, the verifier, the bridge or the boot — the machine cannot answer the
question. That is a profile decision, and it is a STOP rather than something to
route around.

## 1. The obligations, and where each is

| Obligation | Where |
|---|---|
| schema field and version | `SYSTEM_INTERFACE_V1` §4.3 (Version 2); `PLATFORM_INTERFACE_V1` §4.4 (Version 3), and operation 30 at Version 4 |
| mechanical schema-gate invariants | `scripts/tests/check-interface-schema.sh` |
| verifier-owned representation mapping | `source/crates/tos-verifier/src/representation.rs` |
| derivation arms and forged-IR negatives | `source/crates/tos-verifier/src/lib.rs`; `source/tests/integration/tests/capability_representation.rs` |
| `E1503_NONIMPORTABLE_CAPABILITY` | `source/crates/tos-core/src/boundary.rs`; `docs/44` §7; corpus R082 |
| runtime mapping retirement on release | `source/crates/tos-launch/src/lib.rs`; `source/runtime-image/src/main.rs` |
| C′ feature gate | `check_features_against_minor` in `source/crates/tos-core/src/checker.rs` |
| implemented minor 3 | `LANGUAGE_VERSION` in the same file; `tos_ir::LANGUAGE_VERSIONS` |

The gate proves one fact in three places — the accepted schema, the frontend's
table and the verifier's own — and the verifier keeps no dependency on the
frontend (§7a).

## 2. §16, row by row

| § | State | Evidence |
|---|---|---|
| 16.1 existing interfaces and artifacts unchanged | CLOSED | `artifact_compatibility.rs` pins the module digest, the image length and the image hash of four modules against the values read at `28c661f`; `interface_schema.rs` proves every interface but one is `AsInterface` |
| 16.2 an `AsInterface` position refuses a region, a device window and a scalar | CLOSED | `capability_representation.rs` (frontend and verifier) |
| 16.3 `DmaRegion<T>` and `DmaRegion<mut T>` satisfy the position; `Capability("platform.dma.Region")` does not | CLOSED | both suites |
| 16.4 `import capability platform.dma.Region` refused in source | CLOSED | `E1503`, corpus R082 |
| 16.5 wrong **effect** refuses independently | CLOSED | `the_right_representation_without_the_effect_is_refused` |
| 16.5 wrong **representation** refuses independently | CLOSED | `the_interfaces_own_capability_type_is_refused_at_its_position`, `nothing_outside_a_representation_fills_a_capability_position` |
| 16.5 wrong **runtime object kind at grant** | **BLOCKED — a real-nucleus boot** | §3 |
| 16.5 wrong **right** on operation 30 (`dma`, `spend`) | **BLOCKED — a real-nucleus boot** | §3 |
| 16.6 one binding, one handle, one bridge mapping | CLOSED | `dma_region_path.rs` records the handle each call carried and compares the numbers rather than inferring sameness from both succeeding |
| 16.6 one **nucleus capability-table entry** | **BLOCKED — a real-nucleus boot** | §3 |
| 16.7 a second use of the same binding is accepted | CLOSED | corpus A013; `one_region_binding_may_be_used_more_than_once` |
| 16.8 stale **indexed-access** path | CLOSED | `a_successful_release_closes_both_paths`; `an_access_after_a_release_is_refused_before_an_address_exists` — refused before an address is formed, never by a fault |
| 16.8 stale **operation** path | PARTIAL | closed at the bridge and against a stand-in; the `E_NO_CAPABILITY` itself is the nucleus's generation check and wants the boot of §3 |
| 16.9 a failed release preserves the mapping | CLOSED | `a_failed_release_leaves_both_paths_alive`, and the bridge retires only on `OK` |
| 16.10 version gating; a 1.2 module does not acquire the rule | CLOSED | corpus R083; `a_one_two_artifact_does_not_receive_the_representation_rule` |
| 16.11 an implementation without a minor rejects the module whole, by its header | CLOSED | `an_unadmitted_minor_is_refused_by_the_header_alone` |
| 16.12 no representation outside the closed enumeration | CLOSED | the gate reads the enumeration from §4.3; `the_capability_representation_relation_is_closed_and_one_to_one` matches exhaustively |

## 3. Why three rows are blocked, and what unblocks them

**The producer exists now.** `PLATFORM_INTERFACE_V1` version 4 declares
operation 30, the frontend lowers it, the verifier accepts it and the bridge
serves indexed access to what it returns — so the path ADR-0085 specifies runs
end to end:

```text
source -> frontend representation rule -> lowering -> IR
       -> independent verifier -> engine -> runtime bridge -> nucleus
```

`dma_region_path.rs` exercises all of it **with only the last box replaced**. The
stand-in mints a handle for a successful allocation and records the handles it is
asked to act on; the bridge's mapping table in it is `tos_launch::DeviceMappings`
— the real one the runtime image uses, not a model of it. That is what closes
§16.6's handle identity and §16.8's indexed-access half.

**What a stand-in cannot close**, and why these three rows stay blocked: they are
refusals the **nucleus** makes, and a double that made them would be evidence
about the double.

- **wrong runtime object kind at grant.** The nucleus refuses a handle naming
  the wrong object with `E_NO_CAPABILITY`. Nothing above it decides that;
- **wrong right on operation 30.** `dma` on the function and `spend` on the
  authority are checked by `capability::resolve`, and each refuses
  independently. The relevant rights are operation 30's: **`platform.dma.Region`
  legitimately requires no right beyond possession**, and inventing one on the
  region to make a row green would be inventing authority to test for it;
- **§16.6's capability-table half.** That one allocation occupies *one* entry of
  the caller's table is a fact about the nucleus's table, which no host above it
  can see.

**A test-only minting path is not an acceptable substitute**, and none is
introduced. Existing object-kind and right refusals for other interfaces are
inherited **mechanism** evidence, never closure evidence for this one.

**The boot was built**, and it does not close them — §6 says why. All of this
exists and is green:

1. four launcher constants endowing a process with the PCI bus root and a
   memory authority, one positive and three negatives differing in one fact
   each;
2. `tests/vectors/dma-region` and `dma-region-stale`, running the accepted
   operation-30 source path;
3. `host-tools/qemu-test/dma-region.sh`, which asserts each dimension
   independently;
4. nucleus instrumentation reporting the capability-table delta, the alias
   count, and which of P1/P5 holds — counts and verdicts, never a handle.

What it reports instead is the STOP. These rows are replaced with direct
evidence when the reference machine can answer.

## 4. Operation 30's V1 concrete surface (ADR-0085 §18, resolved)

§18 left the gap between ADR-0084's abstract result and the concrete type texts
an accepted schema declares. **It is not a STOP**: no new language or schema
mechanism is required, and none is introduced. Project Architect-approved
resolution, 2026-09-09:

```text
dma_region_allocate(...) -> Result<DmaRegion<mut u8>, i64>
```

Why `u8`, stated so it is not mistaken for something more general:

- ABI operation 30 allocates a contiguous run measured in **bytes** (`rdx`);
- `u8` is the only concrete element type for which every ABI-valid byte length
  has an exact representation, with no invented divisibility, alignment, tail or
  element-count rule;
- a driver encodes its protocol-defined structures into bytes explicitly, which
  is preferable to implying that a TOS nominal or layout type automatically has
  a device-visible binary layout;
- a family of `_u16`, `_u32`, `_u64` rows would add surface and unanswered
  semantics with no demonstrated driver requirement, so none is pre-allocated.

**ADR-0084's `DmaRegion<mut T>` is the abstract semantic shape.** The V1
concrete surface instantiates it at `T = u8` and nothing more:

> **V1 has no generic DMA allocation.** No operation of any accepted schema
> produces a `DmaRegion<mut T>` for a caller-chosen `T`.

If a later driver genuinely needs typed DMA allocation or typed views, that
requirement must first define the missing element-count, alignment and layout
semantics, and is considered separately.

## 5. The Stage 4C-2 path, and where each rule is enforced

ADR-0081 §2's indexed access is **ordinary checked CPU memory**, and it is
implemented as such: `Op::Read`/`Op::Write` over a `Place` with an index step,
which is the encoding the IR has always had. No IR operation, TOSIMAGE tag,
canonical encoding, source construct or address representation was added, and
none was needed — the STOP condition of the brief was evaluated and not reached.

**It is deliberately not `Op::MmioRead`/`Op::MmioWrite`.** Those mean an
observable device transaction and carry obligations DMA RAM must not inherit:
exactly one hardware access, non-elision, non-coalescing, explicit width and
byte order, MMIO ordering. A DMA region is coherent memory on the accepted
reference profile, and giving it those properties would be a semantic no ADR
grants. The host boundary is a second method, `System::access`, whose contract
says so.

| Rule (§6) | Where |
|---|---|
| mapping exists for the handle | `DeviceMappings::extent` |
| index has exact `size` type | the frontend, `E1211_INDEX_TYPE_MISMATCH` |
| element-size multiplication does not overflow | `extent`, checked |
| byte-offset arithmetic does not overflow | `extent`, checked |
| access byte range within `mapping.length` | `extent` |
| write requires a mutable `DmaRegion` | the frontend (`E1201`) **and** the verifier (`V2021_REGION`), so a forged artifact is refused too |
| write requires a writable mapping | `extent` |
| the read's width is the region's element | the verifier (`V2010_TYPE`) — the host takes the width from the declared type, so it is not the artifact's to claim freely |
| unknown or retired handle refuses before memory | `extent` answers `None` before an address is formed |

**CPU address and device address stay distinct.** The mapping base is
runtime-private and is used only to perform an indexed access; it is never
returned, never a value, and never accepted as authority. The device-visible
address is data operation 31 returns, and no operation of any accepted schema
takes one back — nor a physical address, a mapping base or a frame number.

## 6. STOP — the reference machine cannot grant DMA authority

**Reported rather than worked around**, under the rule the Stage 4C-2 brief §2
states: if the existing reference-machine qualification cannot actually grant
DMA authority through the accepted mechanism, that is a STOP, and a profile
requirement must not become a test-only assumption inside production logic.

### What was observed

The real nucleus, the accepted Stage 4 reference machine, and the ordinary
source path. One boot, one line, and the two halves of ADR-0084 §5c side by
side:

```text
TOS.RUN.PCI_NORMALISED ... msix=disabled_masked msi=absent ...
TOS.RUN.PCI_ASSIGNED   ... generation=1 express=0 dma=1 asserted_by=nucleus
TOS.RUN.COMPLETED      value=i64:-101
```

- **`dma=1`** — P5 holds. The compatibility profile qualified the function for
  TC0-only requester traffic, so the capability the claim produced carries the
  `dma` right;
- **`express=0`** — **P1 does not hold.** The function has no PCI Express
  capability, so `dma_region_allocate` refuses with `E_NO_CAPABILITY`
  (`Refused::OutOfScope`), and `-101` is that status as the module reports it;
- the capability walk is **not** at fault, and the same line proves it: it found
  MSI-X (`0x11`) on the same function on the same boot, which is the capability
  `irq-routed` has been claiming interrupts through since Stage 4C-1b. The chain
  is walked correctly and `0x10` is not in it.

### Why it is a decision and not a defect

The reference machine attaches its device as

```text
-machine q35 -device virtio-blk-pci,drive=stage4blk,addr=0x4,
             disable-legacy=on,disable-modern=off,num-queues=1
```

which places it on q35's root-complex bus. It is not a PCI Express endpoint
there, and no register the nucleus may read makes it one.

**P1 is not a formality.** ADR-0084 §5c makes the Express capability the thing
that lets a *reclaim* be proved — Device Status' Transactions Pending bit is how
the nucleus establishes that a function has no non-posted request outstanding
before its memory returns to the pool. A function without it is a function whose
DMA memory could never be proved safe to reclaim, which is exactly why operation
30 refuses it. Granting DMA anyway would be granting authority the system cannot
end.

So the fix is to the **machine**, not to the nucleus: the device must be behind
a `pcie-root-port` to be an Express endpoint. That is a change to the accepted
Stage 4 reference profile, and it is not one to make in passing —

- **every existing Stage 4 gate hard-codes the BDF `0:0:4.0`**: the profile's
  `qualify_dma(0, 0, 4, 0)`, `pci-discovery`, `pci-placement`, `pci-bme-precision`,
  `pci-bar-relocation`, `pci-msi-reserved`, `irq-routed`, `virtio-caps` and
  `virtio-mmio`. Behind a root port the device moves to another bus, and every
  one of those numbers moves with it;
- the root port is itself a device with its own configuration space, its own
  BAR placement and its own interrupt routing, all of which the Stage 4A/4B
  placement and precision gates measure.

### What was **not** done

- P1–P5 are unweakened, and no qualification was bypassed, relaxed or made
  conditional on a test feature;
- no fake DMA device and no synthetic production path was introduced;
- `supports_dma` still answers from the function's own configuration space;
- nothing in the nucleus was taught to assume an Express capability it did not
  find.

### What is ready and waiting on the decision

Everything except the machine:

| Piece | State |
|---|---|
| `platform.dma.Region` schema, lowering, verifier, bridge | landed and green |
| the four launcher constants, one positive and three negatives | landed, each buildable, each excluded from every other constant |
| `tests/vectors/dma-region`, `dma-region-stale` | check clean and lower |
| `host-tools/qemu-test/dma-region.sh` | written, and refuses with the STOP named |
| nucleus instrumentation for §16.6 and for P1/P5 | landed, test-feature-gated, reporting counts and never a handle |

The script is deliberately **not** registered in the preflight inventory or CI:
a gate that cannot pass is not a gate. It runs on request, and the first thing
it reports is this STOP.

**The four §16 rows therefore stay open**, and ADR-0085 §16 conformance stays
incomplete. They are blocked on one decision — whether the Stage 4 reference
machine gains a PCI Express root port, and what that costs the gates that
measure the device where it is today.
