<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0085 implementation — what is closed, and what is blocked

ADR-0085 (Accepted, Project Architect-approved 2026-09-08) separates an
interface's identity from the class of TOS Core values that represents it. Its
six acceptance obligations are implemented and green. **Its §16 conformance set
is not complete**, and this record says exactly which rows are not and why,
because "conformance complete" is a claim the repository has to be able to
check.

**Do not read this as ADR-0085 conformance complete.** Three §16 rows are
recorded `BLOCKED` below. They are not waived, not N/A, and not satisfied by the
analogous tests other interfaces have.

## 1. The obligations, and where each is

| Obligation | Where |
|---|---|
| schema field and version | `SYSTEM_INTERFACE_V1` §4.3 (Version 2); `PLATFORM_INTERFACE_V1` §4.4 (Version 3) |
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
| 16.5 wrong **runtime object kind at grant** | **BLOCKED — Stage 4C-2** | §3 |
| 16.5 wrong **right at the call** | **BLOCKED — Stage 4C-2** | §3 |
| 16.6 `region[i]` and `dma_device_address(region, …)` over one binding are one handle and one capability-table entry | **BLOCKED — Stage 4C-2** | §3 |
| 16.7 a second use of the same binding is accepted | CLOSED | corpus A013; `one_region_binding_may_be_used_more_than_once` |
| 16.8 after a successful release the two stale paths refuse differently | PARTIAL | the bridge half is closed by `tos-launch`'s `retire`; the nucleus half is its existing generation check |
| 16.9 a failed release preserves the mapping | CLOSED | `retiring_something_that_is_not_there_disturbs_nothing`, and the bridge retires only on `OK` |
| 16.10 version gating; a 1.2 module does not acquire the rule | CLOSED | corpus R083; `a_one_two_artifact_does_not_receive_the_representation_rule` |
| 16.11 an implementation without a minor rejects the module whole, by its header | CLOSED | `an_unadmitted_minor_is_refused_by_the_header_alone` |
| 16.12 no representation outside the closed enumeration | CLOSED | the gate reads the enumeration from §4.3; `the_capability_representation_relation_is_closed_and_one_to_one` matches exhaustively |

## 3. Why three rows are blocked, and what unblocks them

`platform.dma.Region` is **not startup-importable** (§4a), and
`SYSTEM_ABI_V1` operation 30 — the one that makes a region — has no interface
schema row yet. So no module can obtain a live DMA region through the accepted
path, and three claims have nothing to be made about:

- **wrong runtime object kind at grant.** There is no grant: the launcher never
  answers a request for this interface, because none can be written;
- **wrong right at the call.** There is no live region handle to present at the
  wrong right;
- **§16.6's identity claim.** It is specifically that `region[i]` and
  `dma_device_address(region, …)` *over the same source binding* reach the ABI
  as one handle and occupy one entry of the caller's capability table. Without a
  producer and without the indexed-access lowering path, that identity cannot be
  exercised.

**A test-only minting path is not an acceptable substitute**, and none is
introduced. Calling the nucleus directly would exercise a lower-layer mechanism
and skip the path ADR-0085 specifies:

```text
source -> frontend representation rule -> lowering -> IR
       -> independent verifier -> runtime bridge -> nucleus
```

Existing object-kind and right refusals for other interfaces are inherited
**mechanism** evidence. They are not closure evidence for this one.

**The unblock condition is concrete**, because operation 30's V1 surface is
decided (§4). When Stage 4C-2 lands the schema, source and lowering path:

1. create the region through the ordinary accepted operation-30 surface;
2. exercise the wrong-object-kind and wrong-right runtime negatives;
3. exercise `region[i]` and `dma_device_address(region, …)` from one binding;
4. prove they carry the same handle and one capability-table entry;
5. replace these rows with direct evidence.

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
