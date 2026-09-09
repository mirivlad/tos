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
not satisfied by the analogous tests other interfaces have. Every one of them is
now blocked on the same single thing — a boot against the real nucleus — rather
than on anything missing from the language, the schema, the verifier or the
bridge.

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

**The unblock condition is a boot**, and it is the only thing left:

1. a nucleus feature endowing a process with an assigned function carrying `dma`
   and a memory authority carrying `spend`;
2. a runtime-image workload running the accepted operation-30 source path;
3. the negatives beside it — the same call with the wrong object kind, without
   `dma`, and without `spend` — each refused independently and not conflated
   with a wrong representation, a missing effect or a stale generation;
4. the released region's `dma_device_address` answering `E_NO_CAPABILITY` from
   the nucleus itself;
5. replace these rows with that evidence.

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
