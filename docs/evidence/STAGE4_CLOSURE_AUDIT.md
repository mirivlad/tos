<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — closure-readiness audit

One map from every Stage 4 closure obligation to the contract that decides it and
the gate that proves it, in the form `STAGE3_CLOSURE_AUDIT.md` took. It duplicates
no ADR: where a decision is stated somewhere else, this points at it.

> **Current audit, reconciled 2026-09-30 on the tree that closes Stage 4.** The
> 2026-09-26 readiness audit it replaces found Stage 4C and Stage 4D ready, and the
> Project Architect closed them on it
> (`source/legal/publication-records/stage4cd-closure-approval-2026-09-26.md`).
> Every verdict below is current. The four that changed since 2026-09-26, and the
> evidence that changed them, are recorded in §13 as superseded history rather
> than kept as active rows.

**Verdicts** are one of four, and nothing else:

- **PASS** — implemented, and proved by a gate that runs on this tree.
- **PARTIAL** — the mechanism exists, and the accepted evidence for it is
  incomplete.
- **MISSING** — the Stage 4 obligation is not demonstrated, or is demonstrated not
  to hold.
- **NONCLAIM** — an accepted document places it outside Stage 4.

**Gate names are the function names `scripts/preflight.sh` declares**, as in the
Stage 3 audit, and `scripts/tests/check-closure-audit.sh stage4` holds every one
against that inventory. `[q]` marks a gate in the `qemu` profile. Rows marked
*new* were exercised for the first time by this closure preparation.

The obligations are `docs/16` §Stage 4's deliverables and exits, `docs/37`
§Stage 4's evidence list, `docs/35` §Stage 4, `docs/34`'s rule that a stage's new
boundary carries a threat entry, negative tests and an evidence level, and the
conformance-evidence sections of every accepted Stage 4 ADR.

## 1. Hardware authority, discovery and device memory

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 1.1 | PCI discovery through a bus capability minted at the launch boundary; a function claimed with an exclusive, generation-tagged assignment | ADR-0079 §14 | `qemu_pci_discovery` `[q]` | PASS |
| 1.2 | Capability effects name interfaces; the schema and its three copies agree | ADR-0080 §10 | `interface_schema`, `qemu_module_operation` `[q]` | PASS |
| 1.3 | Configuration-space reads and writes bounded; nucleus-owned registers refused bit by bit, write-backs and neighbours permitted | ADR-0082 §5a–§5f, ADR-0084 §5c, ADR-0092 R1a | `qemu_pci_placement`, `qemu_virtio_mmio`, `qemu_dma_quarantine` `[q]` | PASS |
| 1.4 | A BAR window derived from the measured BAR; bounds checked before the device transaction; MSI-X table/PBA overlap refused; read-only has no writable mapping; no address accepted anywhere | ADR-0081 §13–§15 | `qemu_virtio_mmio` `[q]` (eight mapping refusals, out-of-window access refused, no-device probe) | PASS |
| 1.5 | The device's own capability list walked from canonical text, bounded | ADR-0081, `docs/34` X4.8 | `qemu_virtio_caps` `[q]` (a `0xFF` chain terminates on its bound) | PASS |
| 1.6 | No device vocabulary in the nucleus, the runtime image or the engine | ADR-0082 §9, `docs/37` §Stage 4 | the `stage4_device_vocabulary_leak` check inside `qemu_virtio_block_write` `[q]`; `device_status_additive` | PASS |

## 2. Interrupts

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 2.1 | A textual driver derives a routed MSI-X source from its function, blocks in `irq_wait` and is woken by the real device | ADR-0082 §13 positives 1–6 | `qemu_irq_routed` `[q]` | PASS |
| 2.2 | Interrupt-authority negatives: no function, wrong function, no numeric authority, missing rights, stale/released sources, re-claimed BDF, death and teardown cancel waits | ADR-0082 §13 negatives 1–11 | `qemu_irq_routed` `[q]` (vectors `irq-authority-negative`, `irq-wait-negative`) | PASS |
| 2.3 | The three ambient paths to interrupt authority closed; BME refused as one bit of one byte | ADR-0082 §13 negatives 12–14 | `qemu_virtio_mmio` `[q]` (vectors `virtio-msix-negative`, `pci-bme-precision`, `pci-msi-reserved`) | PASS |
| 2.4 | Ownership repairs: placement registers, readback, measured window after refusal, neighbours writable, claim state, MSI extent, independent enable predicates | ADR-0082 §13 items 15–21 | `qemu_pci_placement`, `qemu_virtio_mmio`, `qemu_irq_routed` `[q]` | PASS |
| 2.5 | A retired vector is never handed to a second source | ADR-0082 §13 item 22, `docs/34` X4.2 | `qemu_irq_routed` `[q]` (re-claim gets a different vector). The late spurious message itself is E1 | PASS |
| 2.6 | The liveness census counts routed waits and does not cancel them | ADR-0059 realisation, `SYSTEM_ABI_V1` §6 | `qemu_irq_routed` `[q]` (`routed=1 verdict=awaiting-hardware`), `qemu_block_fault` `[q]` (`routed=0 verdict=stalled`) | PASS |

## 3. DMA

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 3.1 | A DMA region needs a function with `dma` **and** a memory authority with `spend`; each missing right refuses on its own; wrong object kind refused at grant | ADR-0084 §8.1, §8.6–8.8, ADR-0085 §16 | `qemu_dma_region` `[q]` | PASS |
| 3.2 | One contiguous extent; an address for a bounded offset; an offset outside refused; no operation accepts an address | ADR-0084 §8.3, §8.9, §8.10 | `qemu_dma_region` `[q]`, `interface_schema`, `dma_region_path.rs` in `tests` | PASS |
| 3.3 | P1 checked: a function with no PCI Express capability gets no DMA authority even when the profile qualified it — *new* | ADR-0084 §8.9a, `SYSTEM_ABI_V1` row 30 | `qemu_dma_quarantine` `[q]` | PASS |
| 3.4 | Relaxed Ordering, No Snoop and both IDO enables refused; their neighbours writable — *new* | ADR-0084 §8.9b | `qemu_dma_quarantine` `[q]` | PASS |
| 3.5 | Released runs stay out of the pool while an interrupt source keeps the function mastering, and return when it goes — *new* | ADR-0084 §8.11, `docs/34` X4.3 | `qemu_dma_quarantine` `[q]` | PASS |
| 3.6 | The churn case: one budget is not spent twice through quarantine — *new* | ADR-0084 §8.12, `docs/34` X4.4 | `qemu_dma_quarantine` `[q]` (a refund-on-release mutation was checked red) | PASS |
| 3.7 | Teardown ordered — mastering stops, the drain is proved, the runs return — on release and on process death | ADR-0084 §8.4, §8.13 | `qemu_dma_quarantine`, `qemu_virtio_block_write` `[q]` | PASS |
| 3.8 | Fail-closed exercised: a device that never clears Transactions Pending keeps its frames, its charge and its assignment, and the BDF cannot be claimed again — *new* | ADR-0084 §8.13a, `docs/34` X4.3 | `qemu_dma_quarantine` `[q]`, second boot (a reclaim-on-`BME=0` mutation was checked red) | PASS |
| 3.9 | DMA publication and consumption ordering, in the language, the IR, the verifier and the backend | ADR-0086 §16 | `dma_ordering_backend`, `dma_ordering.rs` in `tests` | PASS |
| 3.10 | Hardware DMA confinement of a malicious bus master | `docs/34` S5 | stated as not provided on the no-IOMMU profile | NONCLAIM |

## 4. The VirtIO block textual driver

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 4.1 | Initialization, feature negotiation and one split virtqueue from canonical text | Stage 4D-1 | `qemu_virtio_queue` `[q]` | PASS |
| 4.2 | A real `READ` of a real sector, proved against a sentinel | Stage 4D-2 | `qemu_virtio_block_read` `[q]` | PASS |
| 4.3 | Queue reuse: a second request through reclaimed descriptors, ring arithmetic across the wrap | Stage 4D-3 | `qemu_virtio_block_reuse` `[q]` | PASS |
| 4.4 | Two requests outstanding, matched by used id; one delivery completes both | Stage 4D-4 | `qemu_virtio_block_two_inflight` `[q]` | PASS |
| 4.5 | A real `WRITE` proved by independent readback; capacity read under the generation protocol; an undersized device refused before anything is published | Stage 4D-5 | `qemu_virtio_block_write` `[q]` | PASS |

## 5. The block service, its protocol and the capability plumbing

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 5.1 | A capability carried in a message from canonical text | ADR-0094 | `qemu_capability_transfer` `[q]` | PASS |
| 5.2 | Publication needs the dedicated publish endpoint, and granting it is what changes the outcome | ADR-0095 §5, ADR-0093 §10.1 | `qemu_publication_authority` `[q]` | PASS |
| 5.3 | A client holding only an endpoint reaches the service, looked up through the registry | ADR-0093 §10.2 | `qemu_name_service`, `qemu_block_service` `[q]` | PASS |
| 5.4 | An ordinary immutable region crosses IPC from canonical text; a sector crosses as a region | ADR-0097 §9 | `region_ipc_payload`, `qemu_block_data_path` `[q]`, `stage4_data_path_claims` | PASS |
| 5.5 | `block.device.v1`: READ, atomic WRITE, CAPACITY, every refusal a reply, the ordering assertion, both mutations | ADR-0098 §4 | `qemu_block_protocol` `[q]` | PASS |
| 5.6 | `BLK_DEVICE`: a device failure is a refusal, and the service serves the next request — *new* | `BLOCK_DEVICE_V1` §7 | `qemu_block_fault` `[q]` (QEMU `blkdebug` fails a real WRITE and a real READ; the first run found and this closure preparation fixed the ring-count defect) | PASS |
| 5.7 | A send-only name for an endpoint, by intersection | ADR-0100 §5 | `qemu_endpoint_attenuation` `[q]` | PASS |
| 5.8 | A carried call reads its reply | ADR-0101 §4 | `qemu_carried_call_answer` `[q]` | PASS |
| 5.9 | Nominal interface identity under delegation and endowment | ADR-0096 (Accepted 2026-09-30, option N1) | the injectivity invariant — no two accepted interfaces declare one object kind — stated in `SYSTEM_INTERFACE_V1` §4 and checked over both schemas by `interface_schema` and over the frontend table by `accepted_interfaces_declare_distinct_object_kinds` in `tests` (a mutation giving `platform.irq.Source` the PCI bus kind turns it red) | PASS |

## 6. Persistent state and the capsule-to-repository handoff

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 6.1 | `state.store.v1`: format once, refuse twice, two store generations, a reader after the writer ended, every refusal | ADR-0099 §13, `docs/16` deliverable | `qemu_state_store` `[q]` | PASS |
| 6.2 | Persistent storage works through a textual user-space driver | `docs/16` engineering exit | `qemu_state_store`, `qemu_repository_linkage`, `qemu_block_lifecycle` `[q]` | PASS |
| 6.3 | Capsule-to-repository **linkage**: the commit the capsule names read from the device, every object verified against its id, `init.tos` resolved to the capsule's bytes | ADR-0102 §11, `docs/16` deliverable | `qemu_repository_linkage` `[q]` | PASS |
| 6.4 | `system.boot.Identity` reports and never selects | ADR-0102 §3–§4, `docs/34` X4.5 | `qemu_repository_linkage` `[q]` | PASS |
| 6.5 | `ST_BLOCK` and `REPO_BLOCK` reached against a failing device | ADR-0099 §13, ADR-0102 §11 | both ADRs accept them as implemented and not exercised; `qemu_block_fault` `[q]` now reaches the layer below them | NONCLAIM |
| 6.6 | The **transition** half: a repository-backed `/system`, a present system commit id | ADR-0102, `docs/16` | Stage 5's by accepted decision | NONCLAIM |
| 6.7 | Power-loss durability, `VIRTIO_BLK_F_FLUSH`, crash consistency, exactly-once writes | ADR-0092 §0, ADR-0098 §3, `STATE_STORE_V1` §11 | outside Stage 4 by decision | NONCLAIM |

## 7. Crash, restart, reset and the adversarial device

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 7.1 | A service dies, its publication is withdrawn, a successor claims the function at a new generation, resets and reinitializes the device, republishes; a fresh lookup reaches it; the stale name is cancelled, not repaired (case C) | ADR-0093 §10.3, ADR-0092 R1a | `qemu_block_lifecycle` `[q]`, on the fixture encoding ADR-0098 §3 keeps in force | PASS |
| 7.2 | Case D as a boundary: the caller's observation is identical whether or not the device wrote — *new* | ADR-0093 §5a, §10.4 | `qemu_block_fault` `[q]` (identical `E_CANCELLED`; 0 vs 1 completion; zeros vs pattern on the image) | PASS |
| 7.3 | An incomplete `READ`: success replied, the region never sent, the receive cancelled, nothing queued — *new* | `BLOCK_DEVICE_V1` §6a | `qemu_block_fault` `[q]` | PASS |
| 7.4 | A CPL-3 write of Initiate FLR refused, a write-back permitted | ADR-0092 §11.1 | `qemu_pci_placement` `[q]` | PASS |
| 7.5 | A failing device: refusals, and service continued — *new* | `docs/34` X4.8 | `qemu_block_fault` `[q]` | PASS |
| 7.6 | A lying device: wrong completion id, impossible lengths, a read that wrote nothing — each refused at its own step — *new* | `docs/34` X4.8 | `qemu_block_fault` `[q]` (`test-hostile-device`) | PASS |
| 7.7 | A device that never quiesces — *new* | `docs/34` X4.3 | `qemu_dma_quarantine` `[q]` | PASS |
| 7.8 | Availability against a device that stops completing | `docs/34` §What Stage 4 does not claim | the driver waits on a live routed source; stated | NONCLAIM |

## 8. The threat model

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 8.1 | Every Stage 4 boundary has a threat entry, negative tests and a stated evidence level | `docs/34` §Stage mapping | X4.1–X4.12, reconciled on this tree: two identifier collisions resolved (ADR-0102's entries are X4.5/X4.6), X4.3 raised from E1 and X4.4 from a conditional E2 by `qemu_dma_quarantine`, six entries added for MMIO, the device, the block client, service lifetime, reset and stored state, and a Stage 4 non-claims list | PASS |

## 9. Performance

Stated in full in `docs/evidence/STAGE4_PERFORMANCE_REPORT.md`. The hard budgets
are counted by `qemu_stage4_request_cost` `[q]`; the reference ratios are measured
by `qemu_stage4_reference_performance` `[q]`. What performance closure requires is
ADR-0104's (accepted 2026-09-30), which amends ADR-0103's reading of a miss.

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 9.1 | H1 — no per-request DMA, ring, descriptor, endpoint, plan or driver-private allocation; at most one funded ordinary payload Region | `docs/35` §Stage 4 as amended by ADR-0103 | `qemu_stage4_request_cost` `[q]`: one ordinary `region_allocate` and zero `dma_region_allocate` per READ, every one of twelve | PASS |
| 9.2 | H2 — at most one payload copy client ↔ device-visible memory | `docs/35` | one per direction, counted from the service's text | PASS |
| 9.3 | H3 — at most four handoffs per unbatched request | `docs/35`, ADR-0099 §11, ADR-0103 | `qemu_stage4_request_cost` `[q]`: the causal logical-READ trace requires exactly four structural transitions for a timer-free READ; timer-induced transitions are recorded separately and the one extra transition admitted is the causal consequence of a tick. Restoring the per-line yield turns it red | PASS |
| 9.4 | H4 — one wakeup may complete a batch | `docs/35` | batching unavailable in v1; one delivery completed two requests in `qemu_virtio_block_two_inflight` `[q]` | PASS |
| 9.5 | H5 — no global driver lock across independent queues | `docs/35` | one queue, no lock | PASS |
| 9.6 | R1–R3 — throughput, 4 KiB p99 latency and CPU per MiB against the reference oracle | `docs/35`, ADR-0103, ADR-0104 | **measured at P1 and missed**: R1 0.00375 (original target ≥ 0.35), R2 246.1 (≤ 5), R3 223.6 (≤ 8), clean commit `3708ea7`, `stage4-reference-r1-r3.json`. The original figures are research targets and remain missed; the closure requirement is ADR-0104's and is met: a valid measurement, checked workload equivalence, the production textual-driver path with its audit trail, nothing subtracted, the Level-1 defects fixed, the decomposition retained (`stage4-reference-decomposition.json`) | PASS |
| 9.7 | R4 — engine identity and cache state reported | `docs/35` | the report's §R1–R3: engine, cache state, observer, QEMU command, oracle, artifacts, disk image, quantum and host | PASS |
| 9.8 | The measured Bootstrap/TCG baseline is not degraded silently | ADR-0104 §4, `docs/35` §Regression policy | `qemu_stage4_reference_performance` `[q]` re-measures and blocks a ratio more than 30 % worse than the `3708ea7` baseline, and reports one more than 15 % worse | PASS |

## 10. The identity exit

`docs/16`: *"the textual driver performs actual I/O from canonical source; no
binary shadow driver or hidden host path exists"*. `docs/37` asks for the driver
loaded from identified source, device capabilities only, and no binary shadow
driver doing the real I/O.

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 10.1 | The driver is canonical text, and its exact identity is observable | `docs/37` | every Stage 4 boot records `TOS.RUN.BEGIN path=…`, the module digest in `TOS.RUN.VERIFIED` and the engine digest in `TOS.RUN.PROCESS_BEGIN`; the capsule's provenance is checked by `check-capsule-provenance.py` in every Stage 4 gate, and `qemu_repository_linkage` `[q]` ties the boot text to its commit | PASS |
| 10.2 | The real device I/O is performed by that textual process | `docs/37` | the device's own effects are what the gates read: a sentinel replaced by device data (`qemu_virtio_block_read`), a sector read back through a different buffer and by a successor (`qemu_virtio_block_write`, `qemu_block_lifecycle`), the image changed or not as the boot claimed (`qemu_block_fault`), each completion a routed MSI-X delivery to the process's own source | PASS |
| 10.3 | Hardware authority only by capability | `docs/37` | the clients of `qemu_block_service`, `qemu_block_protocol`, `qemu_state_store` and `qemu_repository_linkage` `[q]` hold no bus, function, window, source or DMA region; only the block service requests them | PASS |
| 10.4 | No binary shadow driver | `docs/37` | the nucleus, the runtime image and the engine carry no device-protocol vocabulary (1.6); the runtime bridge performs single MMIO and DMA accesses a text asks for and interprets none; no production artifact changes in any evidence build — every Stage 4 gate hashes the production nucleus before and after its test builds | PASS |
| 10.5 | No host path after boot | `docs/16` | the disk is a QEMU device; the harness writes the image before the boot and, in `qemu_block_fault` alone, reads one sector after QEMU has exited, as a judge; `blkdebug` is QEMU's device model and not a TOS path | PASS |
| 10.6 | Persistent state and repository reads pass through `block.device.v1` | ADR-0099, ADR-0102 | `qemu_state_store`, `qemu_repository_linkage` `[q]` count the block service's requests and the readers hold no hardware authority | PASS |
| 10.7 | Derived artifacts disposable and regenerable | `docs/02`, `docs/37` Stage 2 | no derived executable is carried into or kept out of any Stage 4 boot; every module is checked, lowered and verified in the boot that runs it | PASS |
| 10.8 | Test-only paths that could satisfy a gate | hostile self-review | every test feature changes a launcher constant or adds an observation or an injected device observation; none reaches a dispatcher or adds an ABI operation, and `feature_builds` type-checks them all. Named in §11 | PASS |
| 10.9 | A long-lived textual driver keeps its whole audit trail | `RUNTIME_OBSERVABILITY_V1` §7, found by the R1–R3 preparation | `qemu_report_reclaim` `[q]`: a process relays 3.4 report regions' worth of lines, all of them, then its account and completion; without the reclaim it loses 2912 of 4096 lines | PASS |

## 11. Cross-stage gates, open decisions and the hostile self-review

| # | Requirement | Normative source | Evidence / gate | Verdict |
|---|---|---|---|---|
| 11.1 | Review user-space interrupt, DMA and interpreted-driver **patent/security** mechanisms before Stage 4 closes | `docs/16` §Cross-stage gates, `docs/24` | security: `docs/34` X4.1–X4.12. Patent: the engineering review of `docs/research/PATENT_LANDSCAPE.md` §Stage 4, `docs/24` steps 1–6, and **step 8, the Project Architect's decision** of 2026-09-26, `source/legal/publication-records/stage4-patent-engineering-review-2026-09-26.md`: the engineering review is accepted, no freedom-to-operate or non-infringement conclusion is made, qualified counsel is deferred to the `docs/24` triggers, and the flagged families stay in the risk register | PASS |
| 11.2 | Open ADRs do not block Stage 4 | `docs/38` | ADR-0044 remains Proposed by Project Architect decision (2026-09-30): the current digest scheme is correct and it is revisited on a real operational need, not before; it does not block Stage 4. ADR-0096 is Accepted (N1). ADR-0094 is resolved. `open_decisions` holds the list to the ADR files | PASS |
| 11.3 | No gate weakened, no threshold lowered to make a result pass | `docs/21` | every earlier gate still runs; operation 30's P1 status and the ring count of a device-failed request weaken no gate. R1–R3 were **not** lowered: ADR-0104 records them as missed research targets and changes their role for the Bootstrap/TCG profile explicitly, holding the measured baseline against regression. The Level-1 changes to the service keep the −56 defence, proved by `qemu_block_fault` | PASS |

**What the hostile self-review found and fixed before this audit**, rather than
listing: the ring-count defect after a device-failed request (every copy of the
accepted service); operation 30's P1 status disagreeing with its contract since
the row was written; two pairs of colliding threat identifiers; X4.3's evidence
level still reading "E1 until Stage 4C-2's evidence exists"; a gate comment
describing profile revision 1's machine as the current one; `run.sh --help`
truncating its own usage; comments calling `BLK_DEVICE` unreachable. And, in the
2026-09-28–30 performance work: the H3 count of eight was an attribution error — a
runtime yield after every audit line and an IRQ-to-IRQ instrument — not the
protocol; a long-lived process silently lost its report after 64 KiB; the service
converted its sentinel byte once per byte and scanned all 512 bytes where the first
change decides.

**Test-only constructions this closure preparation added**, each named so a
reader can judge it: `test-dma-quarantine` (a launcher constant that qualifies a
second function), `test-device-never-quiescent` (one device observation injected
at the Transactions Pending read), `test-hostile-device` (a DMA-writing adversary
at a named delivery), `test-request-cost` (counters), `test-request-trace` (a
buffered causal trace emitted after every process has ended), the runtime's
`test-block-reference` (marks around the reference client's measured calls, on a
nucleus with `test-measurement-port`), the `blkdebug` option of `run.sh` (QEMU's own
device failure), and the separately built Stage 4 observer and Rust oracle, neither
linked into TOS. None changes a production artifact; every one hashes the
production nucleus, and the reference run the runtime image too, around its builds.

## 12. Summary and readiness

Counted over the numbered requirement rows of §1–§11, of which there are 72.

| Verdict | Count |
|---|---:|
| PASS | 67 |
| PARTIAL | 0 |
| MISSING | 0 |
| NONCLAIM | 5 |

**Stage 4C and Stage 4D are closed** (2026-09-26) on the obligations of §2, §3, §4
and §7, all PASS or NONCLAIM.

**Stage 4 — READY TO CLOSE on the evidence.** Every row is PASS or a NONCLAIM that
an accepted decision places outside Stage 4 (3.10 hardware DMA confinement without
an IOMMU, 6.5 the store's and repository's block refusals as implemented but not
exercised, 6.6 the repository transition that is Stage 5's, 6.7 power-loss
durability, 7.8 availability against a device that stops completing). The
performance contract report shows H1–H5 met and R1–R3 measured at P1, missed as
research targets and closed under ADR-0104 with a regression-held baseline; this is
not a production-performance claim. The patent engineering review is closed without
an FTO conclusion. ADR-0044 stays open by decision and blocks nothing. Closure
itself is the Project Architect's act and is recorded separately.

## 13. Superseded verdicts of the 2026-09-26 readiness audit

Kept so that the record says what was believed and why it changed. None of these is
current.

- **9.1 H1 — was MISSING** ("one `region_allocate` per READ, structural"). ADR-0103
  (2026-09-26) amended H1 to admit one funded ordinary payload Region as an
  accounted IPC ownership transfer; measured, it is exactly one per READ.
- **9.3 H3 — was MISSING** ("eight handoffs before the timer's, structural"). The
  eight were a runtime yield after every audit line and an IRQ-to-IRQ instrument;
  corrected on 2026-09-28, the causal trace measures four
  (`STAGE4_PERFORMANCE_REPORT.md` §Current H3 attribution).
- **9.6 R1–R3 — was MISSING** ("P0: no accepted method"). ADR-0103 defined the method;
  the measurement was made on 2026-09-30 and missed every original target; ADR-0104
  made the ratios characterization and regression evidence for Bootstrap/TCG.
- **11.1 patent/security — was PARTIAL** ("step 8 not taken"). The step-8 decision
  already existed from 2026-09-26 in
  `source/legal/publication-records/stage4-patent-engineering-review-2026-09-26.md`;
  the audit had not been brought up to it.
- **5.9 nominal interface identity — was NONCLAIM** under a Proposed ADR-0096; the
  Project Architect accepted option N1 on 2026-09-30.
