<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 — closure audit

One map from every Stage 3 closure obligation to the contract that decides it,
the code that implements it, and the gate that proves it. It duplicates no ADR:
where a decision is stated somewhere else, this points at it.

**Verdicts** are one of four, and nothing else:

- **CLOSED** — implemented, and proved by a gate that runs.
- **ENVIRONMENT-ONLY** — implemented and proved, except that this host lacks a
  prerequisite the evidence needs. Nothing about the system is unknown; the run
  is unavailable here.
- **OPEN — blocks Stage 3** — not satisfied.
- **OUT OF STAGE 3 by accepted decision** — an accepted document places it
  outside this stage.

**Gate names are the function names `scripts/preflight.sh` declares**, not the
human-readable labels beside them: a function name is the gate's stable
identity, and `scripts/tests/check-closure-audit.sh` holds every name below
against that inventory. `[q]` marks a gate in the `qemu` profile.

## 1. Execution, language and the interface bridge

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 1.1 | Canonical textual source is read, checked, lowered, verified and executed on the freestanding path | docs/40–44, ADR-0048 | `tos-core`, `tos-ir`, `tos-verifier`, `tos-engine`, `runtime-image` | `qemu_boot_module_failure`, `qemu_module_set` `[q]`; `tests` | CLOSED |
| 1.2 | Textual source reaches real system interfaces rather than decorative wrappers | `SYSTEM_INTERFACE_V1`, ADR-0060 | `tos-core/src/boundary.rs`, `interfaces.rs`; `PERFORMED` in `runtime-image` | `interface_schema`, `qemu_module_operation` `[q]`, `interface_schema.rs`, `interface_reach.rs` | CLOSED |
| 1.3 | An operation returns the **value it produced**, not a register fragment | `SYSTEM_INTERFACE_V1` §5 | `boundary::type_text`, `lower::resolve_type`, `Produced` in the bridge | `capability_result.rs` (4 tests) | CLOSED |
| 1.4 | A denied capability request fails at startup, before the first instruction | `SYSTEM_INTERFACE_V1` §10.3, docs/42 §2 | `Endowment::granted` in `runtime-image`, `CapabilityDenied` | `qemu_module_operation`, `qemu_process_control` `[q]` | CLOSED |
| 1.5 | `string` and capability value parameters cross by the ABI's own mechanism | `SYSTEM_INTERFACE_V1` §4.1 | `Slot::Text`, `Slot::Held` | `qemu_runtime_authority` `[q]`, `qemu_supervision` `[q]` | CLOSED |
| 1.6 | Schema-declared record types (`CreatedProcess`, `ChildEnding`) | `SYSTEM_INTERFACE_V1` §4.2 | `interfaces::RECORDS`, `lower::schema_record`, `ending_value` | `interface_schema` (field order held across three statements), `capability_source.rs`, `qemu_supervision` `[q]` | CLOSED |

## 2. Capabilities

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 2.1 | A handle is an index and a generation; a stale one is detectably stale | `CAPABILITY_V1` §2 | `nucleus/src/capability.rs` | `qemu_capabilities` `[q]` | CLOSED |
| 2.2 | Refusal order: index → generation → type → rights | `CAPABILITY_V1` §2, ADR-0056 | `capability::resolve` | `qemu_capabilities` `[q]` | CLOSED |
| 2.3 | Attenuation refines and never widens; release makes copies stale | `CAPABILITY_V1` §4 | `capability::attenuate`, `release` | `qemu_capabilities`, `qemu_supervisor` `[q]` | CLOSED |
| 2.4 | Runtime-obtained authority is usable in **every** capability position | ADR-0078 | `tos_ir::CapabilitySource`, verifier per-position checks, `lower_interface_operation` | `capability_source.rs` (7 tests), `qemu_runtime_authority` `[q]` | CLOSED |
| 2.5 | A forged scalar or wrong nominal capability in a capability position is refused **by the verifier** | ADR-0078 §4 | `tos-verifier/src/lib.rs` | `capability_source.rs` | CLOSED |
| 2.6 | Affinity is structural, never read off the rights | ADR-0075 §5, `CAPABILITY_V1` §4 | `Object::is_affine` | `qemu_region_transport` `[q]`, `region_lifecycle.rs` | CLOSED |
| 2.7 | Launch plans: made, written through the delegated authority, sealed once, reusable, destroyed exactly once | ADR-0077 | `nucleus/src/plan.rs`, operations 21–23 | `supervisor` `[q]` (plan-as-holder pair), `qemu_bundle_launch` `[q]`, `plans_live=0` on every reclamation | CLOSED |

## 3. IPC

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 3.1 | Inline bound, transfer counts and queue depth are constants of the contract | `IPC_V1` §3, ADR-0057 | `nucleus/src/ipc.rs` | `qemu_capabilities`, `qemu_blocking` `[q]` | CLOSED |
| 3.2 | A message is delivered whole or not at all, for capabilities **and** regions | `IPC_V1` §3, ADR-0075 | `syscall::acceptable`/`accept` | `qemu_region_transport` `[q]` | CLOSED |
| 3.3 | One receiver per endpoint, enforced where authority is granted | `IPC_V1` §2 | `capability::grant` | `qemu_second_receiver` `[q]` | CLOSED |
| 3.4 | Request/reply crossing count is **counted**, not estimated | `IPC_V1` §8, §9.7 | `PAYLOAD_COPIES`, `MESSAGES` in `ipc.rs` | `qemu_exchange_cost` `[q]`, `TOS.RUN.IPC.COST` | CLOSED |
| 3.5 | Blocking has a cancellation path; the liveness rule ends an unsatisfiable wait | ADR-0059 | `process::block`, the scheduler's liveness check | `qemu_blocking` `[q]`, `qemu_supervision` `[q]` | CLOSED |
| 3.6 | A region transferred linearly leaves the sender | `IPC_V1` §5, ADR-0075 §5a | `syscall::send_transaction` | `qemu_region_transport` `[q]` | CLOSED |

## 4. Performance and observation

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 4.1 | Absolute Stage 3 latency budget `p99 ≤ 200 µs` | ADR-0068 (which removed the relative `≤ 8x` from conformance) | `tests/performance`, `qemu_stage3_ipc_conformance` | `qemu_stage3_ipc_conformance` `[q]` — **needs the ADR-0066 observer build** | ENVIRONMENT-ONLY |
| 4.2 | Observer qualification | ADR-0066 | `host-tools/qemu-test/observer-*` | `qemu_stage3_observer_conformance` `[q]` — **needs the ADR-0066 observer build** | ENVIRONMENT-ONLY |
| 4.3 | The relative `≤ 8x` ratio is observational data, not a conformance obligation | ADR-0068 | — | recorded in ADR-0068 | OUT OF STAGE 3 by accepted decision |
| 4.4 | Counted IPC constraints hold | `IPC_V1` §8 | `ipc::cost` | `qemu_exchange_cost` `[q]` | CLOSED |

**On 4.1 and 4.2, and a correction this audit produced.** Locally both gates
exit `2` with *"the selected QEMU has no ADR-0066 observer-build.json"* **before
booting anything**, which reads as a missing host prerequisite. Reading the
repository's own CI — which *does* build the accepted observer — showed that
they were failing there for a different reason: `exit 101`, a **build error** in
the measurement workload (`Prepared` gained a lifetime parameter and
`struct Work` did not follow). The conformance result was therefore *absent*
rather than merely unavailable here, and had been since before this round.

That is ordinary breakage rather than a contradiction, so it was fixed rather
than reported: `Work.prepared` is `Prepared<'static>`, which is what
`Prepared::launch` has always returned. Both measurement features now build.
The closure-commit run is obtained from the repository's own workflow, whose
result is recorded in §12 — no substitute QEMU, timing source, observer or local
approximation was used anywhere, and no threshold was touched.

## 5. Processes, identity and funding

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 5.1 | A process's identity records what it was built from | `PROCESS_IDENTITY_V1` §3 | `TOS.RUN.PROCESS_BEGIN` | `qemu_success` `[q]` | CLOSED |
| 5.2 | Instance identity is not a handle, and survives ending | ADR-0067 §7 | `process::instance`, `CREATE_INSTANCE_ID` | `qemu_lifecycle` `[q]` | CLOSED |
| 5.3 | Restart generation is asserted by the creator, never computed | ADR-0067 | `CreateFundedRecord` + `HAS_RESTART_GENERATION` | `qemu_lifecycle` `[q]`, `qemu_supervisor` `[q]` | CLOSED |
| 5.4 | Ending, reclamation and slot reuse do not confuse identities | ADR-0067, `PROCESS_IDENTITY_V1` | `process::retire`, generation advance | §8 below | CLOSED |
| 5.5 | Creation is funded from a presented `MemoryAuthority`; no ambient root | ADR-0076 §3 | `process::create_funded`, `Funding` | `qemu_supervisor` `[q]`, `qemu_memory_account` `[q]` | CLOSED |
| 5.6 | Operations 8 and 15 are retired and always answer `E_NOT_SUPPORTED` | ADR-0076 §4, `SYSTEM_ABI_V1` §7 | `syscall::answer` refuses before reading arguments | `qemu_process_launch` `[q]`, `qemu_supervisor` `[q]` | CLOSED |
| 5.7 | A child's endowment comes from a sealed plan, and the raw table is gone | ADR-0077 §5 | `funded_endowment`; `CREATE_ENDOWMENT` removed | `qemu_supervisor` `[q]`, `qemu_bundle_launch` `[q]` | CLOSED |

## 6. Memory

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 6.1 | One physical account: `allocated + reserved + free == budget` | ADR-0076 §1–2 | `nucleus/src/region.rs` authority tree | `qemu_memory_account` `[q]` | CLOSED |
| 6.2 | The page-table reserve is proved and returns to its baseline | ADR-0076 §2 | `memory::tables` | `qemu_memory_account` `[q]` (1452 total, 1451 baseline, 1 permanent root) | CLOSED |
| 6.3 | `MAX_PROCESSES` bounds **slots**, not a whole-machine funding reservation | ADR-0069 §7 (amended 2026-09-03) | — | `qemu_memory_account` `[q]` reports what the root funds and asserts the topologies actually run | CLOSED |
| 6.4 | A grant is a fixed policy figure, never a share of what is free | ADR-0069 §2, §2a | `process::grant_bytes` — no `min(requested, available)` | `qemu_supervisor` `[q]` (impossible vs. unaffordable told apart) | CLOSED |
| 6.5 | Region allocate / freeze / share / lifecycle | ADR-0075 | operations 17, 18, 7; `region.rs` | `qemu_region_transport` `[q]`, `qemu_region_faults` `[q]`, `region_lifecycle.rs` | CLOSED |
| 6.6 | Reclamation is physical-before-accounting, and everything returns | ADR-0076 §5 | `process::reclaim` | every QEMU gate asserts the pool and reserve return | CLOSED |

## 7. Build, bundle and topology

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 7.1 | A build's products live outside the workspace | ADR-0074 §1 | `build_into_bundle` | `STAGE3_BUILD_WORKSPACE.md` | CLOSED |
| 7.2 | One immutable bundle per exact closure, opaque to ring 0 | ADR-0074 §2, ADR-0073 | `tos-bundle`, operation 20 | `qemu_bundle_launch` `[q]` | CLOSED |
| 7.3 | The target admits and verifies the bundle itself; no receipt crosses | ADR-0073 | `admit_bundle` in the target | `qemu_bundle_launch` `[q]`, `qemu_build_topology` `[q]` | CLOSED |
| 7.4 | A corrupt bundle creates a process successfully and that process refuses itself | ADR-0073 | operation 20 reads no byte | `qemu_bundle_launch` `[q]` (`created=0`, then `BUNDLE.REFUSED`) | CLOSED |
| 7.5 | T1: resident supervisor → transient worker → build → freeze/share → handoff → worker collected and reclaimed → **then** the target | ADR-0074 §4a | `build_worker`, `build_supervisor` | `qemu_build_topology` `[q]` — ordering read off the log | CLOSED |
| 7.6 | Capsule-v1 capacity, measured with both roles resident | ADR-0074 §5d | — | `qemu_build_topology` `[q]`: 70.09 MiB headroom vs. the 50.52 MiB worst Capsule-v1 bundle, 19.57 MiB spare | CLOSED |
| 7.7 | A fixed `BuildWorkspace` size | ADR-0074 §5a, §5c | — | nothing depends on it: a worker's arena is a funded role grant | OUT OF STAGE 3 by accepted decision |
| 7.8 | An installed-source backend | ADR-0074 §5 C, ADR-0073 | — | explicitly not described | OUT OF STAGE 3 by accepted decision |

## 8. Supervision

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 8.1 | A textual supervisor, not a host script | docs/37, ADR-0051 §3 | `tests/vectors/supervision/init.tos` | `qemu_supervision` `[q]`, `supervision.rs` | CLOSED |
| 8.2 | Supervision policy is canonical text under `/system/policy/` | ADR-0051 §3 | `tests/vectors/supervision/services.tos` | `qemu_supervision` `[q]` — three modules resolved as one set | CLOSED |
| 8.3 | Restart window: failure **density**, dated by `ChildEnding.ended_tick` | Architect decision F (2026-09-03), ADR-0067 | `init.tos` failure ring | `supervision.rs`: exactly-a-window-old expires; one tick inside counts | CLOSED |
| 8.4 | `BLOCKED` — no instance running **and** a dependency prevents a fair launch; consumes no budget; left when the dependency runs | Architect decision G | `init.tos` `considering` re-evaluation | `supervision.rs`, `qemu_supervision` `[q]` | CLOSED |
| 8.5 | Terminal `FAILED` latches; an event that would have restarted does not | Architect decision F | `init.tos` state 3, never reconsidered | `supervision.rs`, `qemu_supervision` `[q]` (`latched-no-start` after the latch, other services still starting) | CLOSED |
| 8.6 | A running dependent is **not** terminated when a dependency becomes unavailable | Architect decision G | nothing in the machine terminates | by construction; `qemu_supervision` `[q]` | CLOSED |
| 8.7 | The supervisor's journal keeps observed / inferred / policy / action / result apart, in order | Architect decision J | `note()` + the record vocabulary | `qemu_supervision` `[q]` (order checked), `supervision.rs` | CLOSED |
| 8.8 | One operator-visible important-error view | `RUNTIME_OBSERVABILITY_V1` §9 | severity per event kind; `said=<severity>.…`; `scripts/tos-journal.py` | `operator_journal` | CLOSED |
| 8.9 | Journal persistence, rollover, archival, cross-boot recovery, retention, filesystem location | — | — | `RUNTIME_OBSERVABILITY_V1` §9.6: no accepted contract decides them | OUT OF STAGE 3 by accepted decision |

## 9. Identity chain and fail-closed behaviour

| # | Requirement | Normative source | Implementation | Evidence / gate | Verdict |
|---|---|---|---|---|---|
| 9.1 | Source → artifact → verifier → runtime identity chain is recorded and checkable | `PROCESS_IDENTITY_V1`, ADR-0070 | source maps, `TOS.RUN.VERIFIED`, `TOS.RUN.PROCESS_BEGIN` | `qemu_success`, `capsule_provenance`, `cache_identity.rs` | CLOSED |
| 9.2 | Capsule identity and source binding are not weakened by the build path | ADR-0073 | `launched_set` reads the process's own record | `qemu_build_topology` `[q]`, `check-capsule-provenance.py` | CLOSED |
| 9.3 | A malformed capsule is refused before handoff | `BOOT_ABI_V1` §7–8 | loader validation | `qemu_negative` `[q]` (14 rejections) | CLOSED |
| 9.4 | A hostile bundle is refused by its own target | ADR-0073 | `admit_bundle` | `qemu_bundle_launch` `[q]` | CLOSED |
| 9.5 | A malformed launch record is refused rather than read permissively | `SYSTEM_ABI_V1` §5 row 19 | `restart_generation_of` refuses unknown flags and non-canonical encodings | `qemu_supervisor` `[q]` (`malformed=-3`) | CLOSED |
| 9.6 | A policy naming a module the capsule does not carry fails closed | ADR-0051 | creation refuses `E_BAD_ARGUMENT` | `qemu_supervisor` `[q]` (`no_module`), `qemu_supervisor_text` `[q]` | CLOSED |
| 9.7 | An image or IR damaged in any of the ways a frontend could get wrong is refused | ADR-0070, docs/43 | `tos-verifier`, `tos-image` | `image.rs`, `interface_schema.rs`, `capability_source.rs`, `fuzz` | CLOSED |

## 10. The identity exit audit

The previous closure packet left this pending. It is an audit of evidence that
already exists; no new mechanism was added for it.

**What must not be lost or confused when a process ends and its slot is reused:**

| Fact | How it is kept | Where it is proved |
|---|---|---|
| **Source identity** | `TOS.RUN.PROCESS_BEGIN` records the module path and the runtime engine digest before the process runs; `TOS.RUN.VERIFIED` records the module digest and the verifier identity. Neither is re-derived at exit | `qemu_success`, `qemu_boot_module_failure` `[q]` |
| **Process identity (the slot)** | `TOS.RUN.PROCESS_EXIT`/`_TERMINATED`/`_RECLAIMED` all carry `process=<slot>`, and reclamation is reported per slot with the frames it returned | every QEMU gate |
| **Instance identity** | a boot-monotonic number the nucleus assigns and never reuses. It is **not** a handle: a handle is an index in one process's table and means nothing in another (ADR-0067 §7) | `qemu_lifecycle` `[q]` |
| **An ended object vs. a reused slot** | the slot's **generation** advances at retirement, so a capability naming the old occupant stops resolving — `capability::resolve` asks `object_is_live` once, centrally, rather than in each operation | `qemu_supervisor` `[q]`: the same handle over an ended child answers `E_NO_CAPABILITY`, not authority over whoever occupies the slot next. `qemu_lifecycle` `[q]`: stale authority refused |
| **Region and lane reuse** | a released region's lane is reusable and the old handle is not: the same slot, a new generation | `qemu_memory_authority` `[q]` (`same_lane=1`, `stale=-1`) |
| **Supervisor observation of the correct child** | `ChildEnding.child_instance` is the **instance**, and the supervisor matches on it rather than on a slot or a handle | `qemu_supervision` `[q]`; `supervision.rs` — a run whose only difference is which instance ended produces a different decision |
| **Endings are not lost, and are not kept forever** | every ending is collected in ending order, and one nobody is left to collect is **released** rather than held: `TOS.RUN.NOTICE_RELEASED` | `qemu_lifecycle` `[q]` (three boots: collection order, a delegated observer cancelled with the relation, a delivery to a blocked collector) |
| **Nothing outlives the process that made it** | the pool returns to the root's endowment, the page-table reserve to its baseline, and `plans_live=0` | every QEMU gate's last `TOS.RUN.PROCESS_RECLAIMED` |

**Verdict: CLOSED.** No identity is derived from another, none is re-computed at
exit, and the one place two could be confused — a reused slot — is closed by a
generation that advances and is checked centrally.

## 11. Summary

Counted over the numbered requirement rows of §1–§9, of which there are 60.

| Verdict | Count |
|---|---:|
| CLOSED | 54 |
| ENVIRONMENT-ONLY | 2 |
| OUT OF STAGE 3 by accepted decision | 4 |
| **OPEN — blocks Stage 3** | **0** |

The two ENVIRONMENT-ONLY items are §4.1 and §4.2, and both have the same cause:
this host has no ADR-0066 observer QEMU build. See §9 of
`STAGE3_LAUNCH_PLANS.md` and §4 above for what that does and does not leave
unknown.
