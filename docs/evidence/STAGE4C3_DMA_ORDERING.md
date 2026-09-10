<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0086 implementation — Stage 4C-3, and what its evidence proves

ADR-0086 (Accepted, Project Architect-approved 2026-09-10, revision 2) decides
the MMIO↔DMA ordering contract that ADR-0081 §11, ADR-0082 §12, ADR-0084 §7 and
`docs/11` §DMA each named and each left open. It is implemented whole, and its
§16 conformance set is green.

**This record exists mainly to keep two claims apart**, because the second is
the one a green run is most likely to be mistaken for:

```text
semantic, compiler and backend conformance      proved here
real-device functional behaviour                a Stage 4D question
```

**QEMU on x86-64 cannot demonstrate that the ordering primitive was needed.**
The architecture does not permit the executions the primitive forbids, so no run
on this profile distinguishes a correct implementation from an absent one. A
green virtqueue would not prove the memory-ordering theorem either, and this
document does not claim it would.

## 1. The obligations, and where each is

| Obligation | Where |
|---|---|
| source surface | `PREDECLARED_FUNCTIONS` and `dma_direction` in `source/crates/tos-core/src/{checker,typing}.rs`; `docs/39` §2 |
| type rule over the closed family | `dma_sync_type` in `source/crates/tos-core/src/typing.rs` |
| non-consuming use | `source/crates/tos-core/src/ownership.rs` |
| lowering to its own operation | `lower.rs`, beside the MMIO arm and for the same reason |
| `Op::DmaSync` and its direction | `source/crates/tos-ir/src/lib.rs` |
| canonical digest | `source/crates/tos-ir/src/digest.rs` (tag 40) |
| `TOSIMAGE` v6 | `source/crates/tos-image/src/{lib,write,parse}.rs` |
| verifier obligations | `source/crates/tos-verifier/src/lib.rs` |
| engine boundary | `System::dma_sync` in `source/crates/tos-engine/src/lib.rs` |
| x86-64 backend | `Endowment::dma_sync` in `source/runtime-image/src/main.rs` |
| backend asymmetry, mechanically | `scripts/tests/check-dma-ordering-backend.sh` |
| language minor 4 | `LANGUAGE_VERSION`, `DMA_ORDERING_MINOR`, `tos_ir::LANGUAGE_VERSIONS` |

**The minor was advertised last**, after every row above existed and was green.
Until then a 1.4 module was refused whole by its header with `E1602`.

## 2. The tag, verified rather than assumed

ADR-0086 §14 requires the next free operation tag to be re-verified immediately
before it is assigned. At the commit this landed on:

```text
allocated        0 .. 24        the original operation set
allocated        38, 39         MmioRead, MmioWrite      (ADR-0081)
unallocated      25 .. 37       historically unallocated legacy gap
assigned         40             Op::DmaSync              (ADR-0086)
```

Read out of the encoder, the digest writer and the parser, not out of the ADR.
Nothing in 25..37 was back-filled and no existing tag was renumbered.

## 3. What the built image proves

`check-dma-ordering-backend.sh` disassembles the text range of the freestanding
runtime image and reads the accepted asymmetry out of it:

```text
330 139 instructions
      1 LFENCE            Consume's execution barrier
      0 SFENCE / MFENCE   Publish is compiler-only
      0 MOVNT* / MASKMOV* no DmaRegion write is a non-temporal store
```

**A source grep would not have proved any of this.** It would prove nobody wrote
a non-temporal store; it would say nothing about what the compiler emitted, and
"LLVM would never" is the kind of plausible platform statement ADR-0084
revision 3 was wrong about. The image has no symbol table, so the single LFENCE
is attributed by arithmetic: there is exactly one, and `dma_sync` is the only
place that emits one. A second one fails the gate and has to be re-justified.

## 4. Conformance, row by row

| ADR-0086 §16 row | Evidence |
|---|---|
| `dma_publish` over both mutabilities | `both_directions_over_both_mutabilities_are_accepted` |
| `dma_consume` over both mutabilities | the same test |
| element type is irrelevant | `every_element_type_of_the_family_synchronises` |
| wrong operand family refused in source | `a_wrong_operand_family_is_refused_in_source`; corpus R085 |
| forged wrong operand refused by the verifier | `a_forged_operand_of_any_other_type_is_refused` — fifteen types, including ones no source can write |
| forged result type refused | `a_forged_result_type_is_refused` |
| malformed direction refused, never defaulted | `a_malformed_direction_is_refused_by_the_parser` |
| 1.3 source refused by the feature gate | `a_1_3_module_using_the_feature_is_refused`; corpus R084 |
| forged 1.3 artifact refused independently | `a_forged_1_3_artifact_carrying_the_operation_is_refused` |
| encode/decode round trip | `both_directions_survive_encode_and_decode` |
| Publish and Consume differ in digest and bytes | `publish_and_consume_differ_in_digest_and_in_bytes` |
| tag 40 in the stream | `the_operation_is_encoded_as_tag_forty` |
| old artifacts unchanged | `an_existing_module_keeps_its_digest_and_its_exact_image` — see §5 |
| non-consuming, region usable afterwards | `the_region_is_usable_after_both_operations`; `the_region_is_still_usable_after_three_ordering_points` |
| stale/retired region refused at the runtime boundary | `a_synchronisation_after_release_is_refused`; `a_synchronisation_on_an_unknown_handle_is_refused` |
| exactly one backend synchronisation per operation | `each_ordering_point_reaches_the_host_exactly_once` |
| no `SYSTEM_ABI_V1` selector added | `check-abi-operations.sh` |
| no accepted schema operation added | `check-interface-schema.sh` |
| no address source-visible | the surface has no offset, length or address parameter |
| ordering litmus vectors | `the_ordering_vectors_lower_in_the_order_they_are_written`; corpus C028 |
| the standalone consume | `a_standalone_consume_before_a_status_read_is_accepted` |

## 5. The one compatibility claim that had to be restated

ADR-0086 §14 says a container version bump cannot leave a re-encoded old image
byte-identical, and it does not. What is stable is stated exactly and pinned:

- **module digests** of every 1.0–1.3 module are unchanged;
- **the instruction stream and every table** are unchanged;
- **version 5 remains readable**;
- **only the container's own version field differs**, which the pinned test
  proves by putting it back to 5, resealing, and comparing with the hash taken
  before this slice.

## 6. Three defects this slice surfaced, and fixed

Two of them were **not** ADR-0086's. They came from the Stage 4C-2 DMA slice and
had been on `main` since, because that slice ran its own QEMU script and not the
whole QEMU profile. Running the profile for this slice is what found them, and
they are recorded here rather than fixed quietly:

| Defect | What it was |
|---|---|
| `TOS.MEM.RESERVE` was missing a line | `table_reserve` gained a `dma_mapping_bound()` term with the DMA slice and the decomposition the nucleus prints did not. The parts summed to 48 frames less than the reserve they decompose — `MAX_PROCESSES * 3 * MAX_DMA_REGIONS` exactly — and the gate that exists to catch precisely this said so the first time it ran afterwards |
| two `$`-anchored assertions | `TOS.RUN.PROCESS_RECLAIMED` gained `dma_quarantined=` (ADR-0084 §5f), and two harness greps ended at `plans_live=0$`. They stopped matching a line that was still true, so a green property read as a failure. Both now ask about the plan table rather than about the end of the line |

**The lesson is about the profile, not about the fields.** An event line is a
public boundary, and both defects are the same mistake seen twice: a producer
grew a line and a consumer was pinned to its old shape. The gates were right
both times; nothing had run them.

## 6a. A defect that was this slice's

The language-minor gate's walk read a call's callee through the wrong accessor
and therefore **matched no call at all**. It was written for ADR-0081's
device-memory gate and had been passing since, because an MMIO access needs an
`MmioRegion` value and the *type* half of the same gate caught every module that
could have one — so the broken half never showed.

ADR-0086's feature has no type of its own (§13): a `DmaRegion` parameter is a
1.2 form and stays legal, and the ordering point is the whole of what 1.4 adds.
Its gate is therefore the call site or nothing, which is what surfaced this. The
fix is one accessor, and it repairs ADR-0081's gate at the same time.

## 7. What is not here

No virtqueue. No descriptor table, no available or used ring, no block request.
ADR-0086 §18's list is untouched, and Stage 4D begins from this state.
