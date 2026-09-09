<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0085 implementation — what is closed, and what is blocked

ADR-0085 (Accepted, Project Architect-approved 2026-09-08) separates an
interface's identity from the class of TOS Core values that represents it. Its
six acceptance obligations are implemented and green. **Its §16 conformance set
is not complete**, and this record says exactly which rows are not and why,
because "conformance complete" is a claim the repository has to be able to
check.

**ADR-0085 §16 conformance is complete.** Every row below is closed by direct
evidence, and the four that were open until 2026-09-10 are closed against the
**real nucleus** on the real reference machine — not by a stand-in, and not by
the analogous tests other interfaces have.

They were blocked on one thing, and **§6 records what it turned out to be**: the
Stage 4 reference machine's function had no PCI Express Capability, so ADR-0084
§5c's P1 could not hold and the nucleus refused DMA authority outright. Nothing
was missing from the language, the schema, the verifier, the bridge or the boot.
P1 did not give way — the machine did, as profile revision 2 under ADR-0084
revision 5.

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
| 16.5 wrong **runtime object kind at grant** | CLOSED | `dma-region.sh`, `test-dma-wrong-kind`: a real granted object of the wrong kind under the name the module imports, refused before the first instruction |
| 16.5 wrong **right**: the function's `dma` | CLOSED | `test-dma-unqualified`: `express=1 dma=0`, so P1 holds and P5 does not — the same endpoint, one right short, refused by the real nucleus |
| 16.5 wrong **right**: the authority's `spend` | CLOSED | `test-dma-no-spend`: `express=1 dma=1`, a fully qualified function, and the refusal is the authority's own right and nothing else |
| 16.6 one binding, one handle, one bridge mapping | CLOSED | `dma_region_path.rs` records the handle each call carried and compares the numbers rather than inferring sameness from both succeeding |
| 16.6 one **nucleus capability-table entry** | CLOSED | `capability_delta=1 aliases=0`, reported by the nucleus itself: one allocation, one entry, no auxiliary authority and no alias |
| 16.7 a second use of the same binding is accepted | CLOSED | corpus A013; `one_region_binding_may_be_used_more_than_once` |
| 16.8 stale **indexed-access** path | CLOSED | `a_successful_release_closes_both_paths`; `an_access_after_a_release_is_refused_before_an_address_exists` — refused before an address is formed, never by a fault |
| 16.8 stale **operation** path | CLOSED | the live boot returns `-1` (`E_NO_CAPABILITY`) from `dma_device_address` through a released handle, decided by the real nucleus's generation check |
| 16.9 a failed release preserves the mapping | CLOSED | `a_failed_release_leaves_both_paths_alive`, and the bridge retires only on `OK` |
| 16.10 version gating; a 1.2 module does not acquire the rule | CLOSED | corpus R083; `a_one_two_artifact_does_not_receive_the_representation_rule` |
| 16.11 an implementation without a minor rejects the module whole, by its header | CLOSED | `an_unadmitted_minor_is_refused_by_the_header_alone` |
| 16.12 no representation outside the closed enumeration | CLOSED | the gate reads the enumeration from §4.3; `the_capability_representation_relation_is_closed_and_one_to_one` matches exhaustively |

## 3. How the four remaining rows were closed

**The whole path, with nothing replaced.** Since profile revision 2 the boot
runs end to end against the real nucleus:

```text
source -> frontend representation rule -> lowering -> IR
       -> independent verifier -> engine -> runtime bridge -> nucleus
```

`host-tools/qemu-test/dma-region.sh` is five boots of the same source, differing
only in what the launcher decided:

| Boot | What it establishes | Asserted by |
|---|---|---|
| `test-dma-region` | P1 `express=1` and P5 `dma=1`, **separately** | the nucleus |
| | allocation, an indexed write and read, a device address, a release | the nucleus and the runtime workload |
| | `capability_delta=1 aliases=0` | nucleus test instrumentation |
| | `dma_device_address` through the released handle answers `E_NO_CAPABILITY` | the nucleus |
| `test-dma-region` (stale fixture) | `region[0B]` after the same release is `RUNTIME_DEVICE_REFUSED` **before any memory access** | the runtime bridge |
| `test-dma-wrong-kind` | a real object of the wrong kind, refused before the first instruction | the runtime image, against the accepted schema |
| `test-dma-unqualified` | `express=1 dma=0`: operation 30 refused for the function's missing right | the nucleus |
| `test-dma-no-spend` | `express=1 dma=1`: operation 30 refused for the authority's missing right | the nucleus |

**Each dimension refuses on its own.** The two right negatives are separate
boots because they are two independent authority requirements; a test that
removed both would prove only that removing something refuses. And each asserts
the *other* conditions still hold, so a refusal cannot be mistaken for a wrong
object kind, a wrong representation, a missing effect, a stale generation, an
invalid length, or a failed qualification.

**P1 and P5 stay two facts.** They refuse identically, and a boot reporting only
the refusal could not say which it was — which is exactly how revision 1's
missing Express Capability hid behind an `E_NO_CAPABILITY` for as long as it
did. The instrumentation reports them side by side.

**§16.5's enforcement layer, stated precisely.** ADR-0085 §16.5 says *wrong
runtime object kind **at grant***, and that is where it is enforced: the runtime
image compares the kind the launch record reports against the kind the accepted
schema declares for the interface, before the module's first instruction. The
nucleus's own object-kind check is defence-in-depth for a handle that never came
from a grant — a forged artifact rather than a boot — and is not the mechanism
of this row. Nothing bypasses the grant check to drive a malformed grant deeper.

**Nothing is minted, forged or mislabelled to make any of this happen.**
`Endowment` has no way to misreport an object's kind and gains none; the
wrong-kind boot grants authority over a real process under the name the module
imports its bus under, and the record reports the kind that object actually is.

**No handle value is ever printed.** `docs/42` §2 admits an interface path into
the record and keeps the concrete handle representation out of it, so the
instrumentation reports a count and an identity verdict — which is what the
claim is about.

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

## 6. The STOP, and how it was resolved

**Recorded rather than deleted**, because the shape of the error is the useful
part and ADR-0084 revision 5 turns on it.

Stage 4C-2's first real-nucleus boot was refused DMA authority outright. The
instrumented line said which of ADR-0084 §5c's conditions failed:

```text
TOS.RUN.PCI_ASSIGNED ... generation=1 express=0 dma=1 asserted_by=nucleus
TOS.RUN.COMPLETED    value=i64:-101
```

- **`dma=1`** — P5 held. The compatibility profile qualified the function;
- **`express=0`** — **P1 did not.** No PCI Express Capability, so no Transactions
  Pending bit, so no way to prove a reclaim, so `E_NO_CAPABILITY`;
- the capability walk was not at fault, and the same boot proved it by finding
  the function's MSI-X.

**P1 was not weakened, and nothing was special-cased.** Quiescence is not
inferred from BME, no timeout substitutes for it, Transactions Pending is not
fabricated, no Express Capability is assumed, `supports_dma` still answers from
the function's own configuration space, and QEMU is nowhere named in the
nucleus. Operation 30 still fails closed when P1 does not hold — which is
exactly what the `test-dma-unqualified` boot now exercises deliberately.

**The reference machine changed instead** (ADR-0084 revision 5, Project
Architect-approved 2026-09-10): the endpoint moved behind an explicit q35 PCIe
root port, and profile revision 2 is recorded in
`docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md` §9a with every Stage 4 invariant
re-measured on it rather than assumed to carry over.

On revision 2 the same boot reports `express=1 dma=1`, and §3's five boots close
the rows that were open.

## 7. ADR-0085 §16 conformance complete

Every row of §2 is CLOSED, by direct evidence, at the layer the decision names.
The four that needed the real nucleus have it; the ones the host could prove
keep their host evidence; and no row is closed by a test-only construction path,
a synthetic object, a bypassed check or an analogous test for another interface.

**ADR-0085 implementation and conformance work is complete.**

What this slice does *not* claim is first-virtqueue readiness. It converts the
plumbing into a real-system fact — two authorities, one region, one nucleus
entry, one bridge mapping, indexed CPU access, a bounded device-visible address,
a release that closes both stale paths — and stops there.
