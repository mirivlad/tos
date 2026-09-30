<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — performance contract report

## R1–R3 under ADR-0103 (2026-09-30)

**All three reference-platform budgets are measured, and all three are missed by
two orders of magnitude.** P1 evidence, clean commit
`3708ea7ee1aa0d67f4d2203a15ee93648e1178a6`, raw record
`docs/evidence/stage4-reference-r1-r3.json`:

| Metric | TOS | Rust oracle | Ratio | ADR-0103 threshold | Verdict |
|---|---|---|---|---|---|
| R1 sequential 512-byte READ throughput, 3 × 1 MiB windows | 37.0 KiB/s | 9 868.7 KiB/s | 0.00375 | ≥ 0.35 | **missed** |
| R2 random 4 KiB operation, nearest-rank p99 of 300 | 129.705 ms | 0.527 ms | 246.1 | ≤ 5 | **missed** |
| R3 whole-QEMU-process CPU per MiB over R1's windows | 28.448 s | 0.127 s | 223.6 | ≤ 8 | **missed** |

No threshold was changed, and nothing was subtracted: the observer's empty-pair
floor (median 564 ns on the TOS side, 351 ns on the oracle side) is reported beside
the result and corrects nothing. R2's medians are 107.0 ms and 0.383 ms.

### How it was measured

- **Machine.** QEMU q35, qemu64, one vCPU, 256 MiB, TCG; Stage 4 VirtIO-block
  profile revision 2; one boot at a time. Host Intel Xeon E5-2680 v4, Debian.
  Scheduler quantum 100 000.
- **Observer.** `build-stage4-observer.sh`: QEMU 10.0.11 from the pinned archive with
  one hash-bound window event (`stage4-observer-patch.py`) carrying
  `CLOCK_MONOTONIC_RAW` and whole-process `CLOCK_PROCESS_CPUTIME_ID` pairs, taken in
  the vCPU thread after OPEN and before CLOSE. Engine
  `153c35365d146bc716f25700dcbea2cb32fdc77f5b4a53b38fc5784fb8dd64ac`. The ADR-0066
  observer of Stage 3 is a separate, untouched build.
- **Oracle.** `tests/virtio-block-oracle`, a dependency-free UEFI application
  (`e2931b8c4631263c722a9bbc46a55cf0dd1b6954b6bb665308ff5b85b9f34b21`). ADR-0103
  leaves its completion mechanism open; it is fixed here to match the service —
  one MSI-X vector and `hlt`, a three-descriptor chain, a queue of at most 128,
  `VIRTIO_F_VERSION_1` only — so that the ratios are about the software above the
  device rather than about polling against interrupts.
- **TOS.** The production service and runtime path. Measurement-only builds, with the
  production nucleus (`574d54d4…`) and runtime image (`8574e30b…`) hashed unchanged
  around them: the nucleus with COM1 in the TSS bitmap (`2a5adc44…`), and a runtime
  that marks calls to `measured_window` and `measured_operation` and nothing else
  (`f420a898…`, the engine identity the boot reports). **Every audit line is
  produced and relayed**; measuring a quieter system would measure a different one.
  Cache state cold: every module is checked, lowered and verified in the boot that
  runs it. Module digests: service `1a5990ad…`, client `5d8387e7…`, init `a20709d8…`.
- **Workload.** ADR-0103's, performed by both sides and checked: 128 warm-up READs
  each verified against the image pattern, 3 × 2048 sequential READs, 303 random
  4 KiB operations of eight READs from a Park–Miller sequence. Both sides report the
  same sector digest (1059771); the TOS audit record carries exactly 8 696 successful
  READ calls; the oracle's interrupt account is 8 696 deliveries and no unexpected
  vector; both disk images equal the 16 MiB pattern
  (`5cb86762501d155ff8c512e66e39180b6077deed3880772d3f7b3c11be7ebfdc`).

Reproduce with `bash source/host-tools/qemu-test/stage4-reference-performance.sh`
after `build-stage4-observer.sh`; exit status 4 means a valid measurement with a
budget missed.

### Where the time goes

`docs/evidence/stage4-reference-decomposition.json` holds one complete boot per
row. Only the last row is evidence; the others are diagnostic variants that exist to
separate causes and were never candidates for the result.

| Variant | Service steps / READ | ms / READ | R1 | R2 | R3 |
|---|---|---|---|---|---|
| accepted service before the Level-1 changes | 22 497 | 31.41 | 0.00157 | 426.1 | 529.4 |
| B: sentinel byte converted once per request | 16 360 | 18.31 | 0.00269 | 257.9 | 310.1 |
| A: no sentinel fill or check (−56 defence removed) | 6 625 | 10.87 | 0.00453 | 156.3 | 184.6 |
| C: accepted service, audit lines suppressed | 22 497 | 30.04 | 0.00164 | 403.1 | 506.4 |
| D: A and C together | 6 625 | 8.80 | 0.00559 | 142.9 | 149.3 |
| **accepted service after both Level-1 changes (P1)** | **10 233** | **13.52** | **0.00375** | **246.1** | **223.6** |

- **The service's interpreted steps are the cost.** It is charged about 92 % of all
  process timer ticks (65 331 of ~70 600 in the P1 boot); the client executes 47
  steps per READ. Under TCG one engine step costs roughly 1.2–1.4 µs.
- **The audit trail is not.** Suppressing every interface audit line saves 1.4 ms
  of 31.4 ms per READ; most of the client's own ticks are its audit output, which is
  small beside the service.
- **What remains is per-byte work in interpreted text.** Each READ fills a 512-byte
  sentinel, scans it (now to the first changed byte) and copies 512 bytes out of
  device-visible memory — the one copy H2 permits and ADR-0037 forces. Each is a
  512-iteration loop of engine steps.

**Level-1 changes made** (commit `3708ea7`, the accepted service and every copy that
claims its text): the sentinel byte is converted once per request rather than once
per byte, and the sentinel check stops at the first changed byte. Semantics are
unchanged — the −56 refusal of a device that wrote nothing is exercised by
`block-fault.sh` as before — and together they cut service steps per READ from
22 497 to 10 233 and raised every ratio about 2.3×.

### Why the remainder is not a Level-1 defect

At the oracle's ~48 µs per READ, R1 ≥ 0.35 requires a TOS READ of about 137 µs, which
at the measured step cost is on the order of a hundred engine steps for the whole
service. Any per-byte loop over 512 bytes alone is five times that, and the forced
copy is one. Even the diagnostic variant D, without the sentinel, the defence or the
audit, reaches R1 = 0.0056. The gap is the product of interpreting the driver's data
path byte by byte on the Bootstrap engine under TCG, not of an ordinary defect.

What would close it lies outside ADR-0103's Level-1 remedy, and each is the Project
Architect's to decide (ADR-0103 §Reference measurement):

1. **Execution-engine work** (`docs/35` §Stage 4 names it): a faster or compiled
   execution tier for TOS Core, so that a step costs a small fraction of what it costs
   now. Nothing else on this list alone reaches R1.
2. **A bulk region primitive**: a copy or fill between a `Region` and a `DmaRegion`
   performed by the runtime as one operation instead of 512 interpreted steps — a
   language primitive or system-interface operation.
3. **`BLOCK_DEVICE_V1` requests of more than one sector**, so that R2's 4 KiB operation
   is one request and the per-request cost is amortised — a protocol change (and the
   route to H4 batching).
4. **The −56 defence's shape**: a full 512-byte sentinel per READ is most of what
   remains after the copy; a cheaper proof that the device wrote the buffer is a
   threat-model decision (`docs/34` X4 series), not an optimisation.
5. **The thresholds themselves** for the declared Bootstrap/TCG profile, which
   ADR-0103 allows to be revisited only after measurement — which now exists.

**Closure interpretation (ADR-0104, accepted 2026-09-30).** The original R1–R3
figures stay in this report as pre-measurement research targets and the misses
measured above. For the Bootstrap/TCG profile they are characterization and
regression evidence, not numeric closure thresholds; the P1 result at `3708ea7` is
the retained baseline, and `qemu_stage4_reference_performance` re-measures it on
every full run and applies `docs/35` §Regression policy (more than 15 % worse:
explanation required; more than 30 %: blocks). A later exploratory rerun on the
same host gave R1 0.00384, R2 186.3 and R3 221.6: the R2 ratio moves by about a
quarter between runs because the oracle's p99 does (0.527–0.690 ms across three
runs), and the baseline sits at the low end of the oracle's range. **This is not a
claim that the present performance suffices for production storage.** None of the
five remedies above is undertaken for Stage 4, and the −56 defence stays.

## Current H3 attribution (2026-09-28)

ADR-0103 keeps H3 at four handoffs. The earlier claim of eight unavoidable
non-timer handoffs was incorrect. The `stage4-request-cost.sh` instrument took
differences between consecutive IRQs, which cross the boundary between two
logical READs. More decisively, `Report::line` in the runtime image called
`context_yield` after **every** interface audit line. Those explicit yields,
not `process::wake` or the two-message READ contract, caused the repeated
client/service alternation. The current `process::wake` still only marks a peer
runnable; the scheduler and protocol were not redesigned.

The Level-1 correction keeps each interface audit line visible before the call
returns by entering the nucleus through the self-only non-scheduling
`time_monotonic` operation. Stage/progress lines retain their existing yield,
which preserves their pre-stage visibility. A fixed-size trace buffer in an
isolated test nucleus records scheduler transitions and logical READ markers,
and emits them after all processes end so serial output does not perturb the
request. `source/host-tools/qemu-test/analyze-stage4-read-trace.py` retains
**every** event of all twelve repeated READs in
`docs/evidence/stage4-h3-logical-read-trace.json` and the gate output's
`logical-read-trace.json`. It does not choose a favorable request or subtract
time. It derives the two roles from the boot's own records rather than from
launch order: the service is the one process the nucleus reports
`TOS.RUN.PCI_ASSIGNED` for, and the client is the one context that receives the
Regions the service sends.

The archived run used QEMU 10.0.13, q35/qemu64, one vCPU and TCG; its capsule
digest is `ea524b14aaae3703f13f4ddc2654652eac2a0603a42ea51db51d1fde8a549a60`,
and the reported runtime-engine digest is
`8574e30befeb98e57e82ffc493c86283ed34799455bc4c87e780a4a2f79e3806`.
The trace nucleus was an isolated build with `test-block-protocol` and
`test-request-trace`; the production nucleus artifact was hash-checked against
mutation. The canonical service and client digests are
`cd1a708eaf48078bec1231c66fc87b0ad12618933d7d7988346ae0e034dabffe`
and `7768b17ea850b9bc31be7284ad1c04781fa4f6d72bf364b381cc98d4972b65ce`.

A timer-free logical READ, from client request commit through client receipt of
the owed Region, has exactly these four scheduler transitions. The sequence
numbers are READ 2 of the archived trace (request commit 597, Region received
620, client Region release 622):

| Sequence | From → to | Cause |
|---|---|---|
| 601 | client → service | the caller blocks waiting for the reply (`ENDPOINT_CALL`) |
| 605 | service → idle | the service blocks in `irq_wait`; no context is runnable |
| 608 | idle → service | the real IRQ wakes the service |
| 617 | service → client | after replying and sending the Region, the service blocks waiting for the next request |

Entry into and return from idle each count as one scheduling transition. The
old counter reset `LAST_RUN` to idle but charged only the return, so it also
under-counted one half of that pair. The corrected counter charges both. A
conformance request before the repeated READs may finish before the service
enters `irq_wait`; that IRQ is latched rather than waking a waiter. The
IRQ-to-IRQ cross-check accepts either valid completion form. A READ's protocol
completion is the transfer of the owed Region to the client;
its subsequent byte check, Region release and the terminal fixture's service
teardown are archived but are not block-service work. The client's Region
release closes each retained window. In the archived run, five of twelve
requests had no timer handoff and measured exactly 4/4. Seven had one timer
handoff each, recorded separately; in five of them the tick replaced one of the
four structural transitions (three structural plus one timer). In the other two
(READs 9 and 10) the timer ran the client after the success reply but before
the service sent the Region, so the client's receive blocked and added one
client → service transition. The analyzer accepts that extra transition only in
that exact causal order; it is a consequence of the preemption, not a step of
the unpreempted READ. Timer-free/timer-interleaved proportions vary from run to
run with where the tick lands; the structural skeleton does not.

The gate is a regression check on the correction, not only a record of it:
restoring `context_yield` after interface audit lines (a one-line mutation of
`Report::interface_line`) turns `stage4-request-cost.sh` red with
`READ 1: unexplained non-timer handoff`.

**Current hard-budget verdict:** H1 meets ADR-0103 (one funded ordinary Region,
zero per-request DMA Regions); H2 remains one payload copy; **H3 meets four**
under the corrected logical-request attribution; H4 and H5 are unchanged.
R1–R3 were unmeasured when this section was written; they are measured and missed
in §R1–R3 above, so Stage 4 stays open.

Reproduce the H1/H3 evidence with:

```sh
bash source/host-tools/qemu-test/stage4-request-cost.sh
```

## Historical report before attribution correction

The following pre-decision and first post-decision observations are retained to
show the error's provenance. Their H1/H3 verdicts and P0 measurement-method
claim are superseded by ADR-0103 and the current section above.

`docs/16` lists a *"Stage 4 performance contract report"* among Stage 4's
deliverables and `docs/37` lists it among the Stage 4 identity evidence. This is
that report. It measures what can be measured under the accepted contracts,
counts what `docs/35` states as counts, and says exactly which budgets are **not
met** and which are **not measurable without a decision the accepted documents do
not contain**. Nothing below is turned into a pass by choosing a reading.

**Verdict:** the Stage 4 performance contract is **not satisfied**. Two hard
architectural budgets are exceeded by the accepted design, and the three
reference-platform budgets are **P0**, because their measurement method is not
defined anywhere a stage could close on. §6 is the decision surface.

## 1. What `docs/35` §Stage 4 requires

```text
hard, after queue initialization
  H1  zero dynamic allocation per completed block request on the steady-state path
  H2  no more than one payload copy between client memory and device-visible memory
  H3  no more than four address-space/scheduler handoffs per unbatched request
  H4  one interrupt wakeup may complete a batch; no scheduling cycle per descriptor
      when batching is available
  H5  no global driver lock serializes independent queues in the long-term contract

reference platform, against a separately isolated Rust VirtIO-block oracle
  R1  sequential throughput >= 35% of the reference, same queue depth and image
  R2  random 4 KiB p99 latency <= 5x the reference
  R3  CPU time per MiB <= 8x the reference
  R4  results include textual-runtime engine identity and cache state
```

`ADR-0099` §11 fixes the endpoints of H3 — *immediate block client ↔ block
service ↔ device*, one completed block request — and requires `state.store`
end-to-end cost to be kept separate. That clarification is applied here.

## 2. Environment

| | |
|---|---|
| source commit | `793ac36002831bf7127bf8efe635787ee71b812c` (the counting instrument's commit); rerun by the closure-preparation commit that carries this report |
| stage | 4, closure preparation |
| machine | QEMU `q35`, `qemu64`, 1 vCPU, 256 MiB, **TCG** — the ADR-0040 profile — plus the Stage 4 device profile revision 2 (`virtio-blk-pci` behind a PCIe root port, one queue, `iommu_platform=off`, 16 MiB raw image) |
| emulator | QEMU 10.0.13 (Debian `1:10.0.13+ds-0+deb13u1`) |
| firmware | OVMF `2025.02-8+deb13u1`, `OVMF_CODE_4M.fd` |
| host | Intel Xeon E5-2680 v4 @ 2.40 GHz, Linux 6.5.0; KVM present and **not** used |
| engine | `runtime_engine=sha256:269f008873d626f2943f664a438bda04b7043bf3bce3b129d3667169166b77ce` (from `TOS.RUN.PROCESS_BEGIN`) |
| verified modules | `system.service.block` `sha256:cd1a708e…dabffe`, `system.client.block` `sha256:7768b17e…2b65ce`, verifier `tos-verifier-reference/0.1.0` |
| cache state | **none** — every module is read, checked, lowered and verified from canonical text in the boot that runs it; no derived executable is carried in or kept |
| workload | `block-protocol`'s accepted boot: ten conformance requests, then **twelve identical 512-byte READs** of one sector through `block.device.v1`, one client, queue depth 1 |
| instrument | `source/host-tools/qemu-test/stage4-request-cost.sh` — gate `qemu_stage4_request_cost` |

## 3. The hard budgets, counted

`test-request-cost` adds one thing to the accepted protocol boot's nucleus: the
scheduler counts its dispatches, the handoffs among them (the processor given to a
different context than last had it; an idle wait counts as nobody, so a context
woken out of one is a handoff), its idle waits and the handoffs the timer made,
and prints the running totals on every routed delivery. Each repeated READ
completes with exactly one delivery, so consecutive deliveries bracket one request.
The runtime's own per-operation lines between them say what the request asked for.

| | Measured per completed READ (11 steady-state intervals) | Budget | Verdict |
|---|---|---|---|
| **H1** | **1** `region_allocate` in the service (the answer region), 0 `dma_region_allocate` | 0 | **EXCEEDED** |
| **H2** | **1** payload copy (`copy_out`: device-visible → the answer region); the region then moves linearly, and IPC copies only the 8-byte request/reply words. `WRITE` is the mirror: **1** (`copy_in`) | ≤ 1 | MET — counted from the canonical service text, which has exactly one 512-byte copy loop per direction; not an instrumented byte count |
| **H3** | **8** handoffs before the timer's, **9–11** measured with 1–3 timer preemptions; 17–19 dispatches; 1 idle wait | ≤ 4 | **EXCEEDED** |
| **H4** | batching is not available: `BLOCK_DEVICE_V1` is one sector per request and one outstanding READ per answer endpoint (ADR-0098 §3). The driver's completion drain already completes more than one request per wake — Stage 4D-4: **1 delivery, 2 completions** | "when batching is available" | MET where it applies; the batching case does not exist in v1 |
| **H5** | one queue, one single-threaded service, no lock of any kind | no global lock across independent queues | MET vacuously — there are no independent queues |

### Why H1 is exceeded, and why it is structural

`BLOCK_DEVICE_V1` §6a answers a READ with **one immutable region** sent to the
answer endpoint the request carried. A region crosses `IPC_V1` §5 linearly
(ADR-0075 §5a): after the send the service holds neither the handle nor the
mapping, and only a frozen region may cross. So the service cannot reuse an answer
region, and **every completed READ allocates one** — `region_allocate`, operation
17: frames from the pool charged to the service's authority, a mapping, and a
capability. A WRITE is the mirror: the region the client composes crosses to the
service, which releases it after the copy, so the **client** allocates one per
write. The DMA side allocates nothing per request.

This is the accepted protocol (ADR-0098, ADR-0097) and not an implementation
choice this report could correct. `docs/35` states that exceeding a hard budget
"requires an ADR because it indicates an architectural path change", and the
accepted ADRs that fix the READ shape do not mention H1. Under `docs/38` a Tier 1
ADR outranks the Tier 2 budget, **but silent contradiction is invalid** — so this is
a conflict for the Project Architect to resolve, not one this report may resolve
by reading "dynamic allocation" narrowly.

### Why H3 is exceeded, and why it is structural

The trace of one steady-state READ, one line per handoff (from a diagnostic build,
not committed):

```text
idle -> service      the device's completion wakes it
service -> client    endpoint_reply_word wakes the client; round-robin hands over
client -> service    the client, answered, blocks receiving the owed region
service -> client    endpoint_send_region wakes it
client -> service    ...
                     while both are runnable, each system call returns through a
                     round-robin turn and alternates them
client -> service    the client's next call blocks it
```

Two properties of accepted contracts produce it. The READ is **two messages** —
a reply, then the region — because `BLOCK_DEVICE_V1` §6a fixes that order
(ADR-0098 §2d says why) and states that a reply never carries a region: the IPC
reply path copies payload bytes and moves no region. And ADR-0049's round-robin
gives the processor to the next runnable context whenever the scheduler runs,
which after a wake is the context just woken. A request of this shape therefore
costs eight handoffs before the timer adds any. Meeting four would take a READ
answered in one message — a change to ADR-0098 — or a scheduler that does not hand
the processor to a context it has just woken. Whether that second change is
within ADR-0049's round-robin or a change to it is itself a question this report
does not answer by making the change.

## 4. The reference-platform budgets: not measured

**R1, R2 and R3 are P0.** `docs/35` §Reporting status: *"No stage closes on P0
for a metric assigned to that stage."* They are not measured because a measurement
that could close them needs choices that no accepted document makes, and making
them here would be choosing the method that decides the verdict:

1. **The clock.** ADR-0066 fixes an external observer **for Stage 3's IPC
   budgets**, and fixes it as the vCPU thread's `CLOCK_THREAD_CPUTIME_ID`. That is
   the right clock for R3's CPU time and the wrong one for R2's latency: while a
   request is at the device the vCPU is halted and the device model runs on
   another host thread, so thread CPU time omits exactly the wait a block latency
   is made of. A wall clock brings in host descheduling, which ADR-0066 kept out
   on purpose. No decision says which applies to Stage 4, or whether R3 counts the
   device model's thread.
2. **The oracle.** `docs/35` admits *"a minimal, separately isolated Rust
   VirtIO-block benchmark implementation … only as a host/reference oracle"*. On
   this machine the device exists only inside the guest, so the oracle must run
   there — in ring 0 of a measurement nucleus (which the Stage 4 device-vocabulary
   guard forbids in `nucleus/src` today), as a native ring-3 process under the same
   capability ABI, or in a separate bare-metal image. The ratio depends on which,
   by more than the budgets' margins, and nothing chooses.
3. **The workload.** R2 is *random 4 KiB*; `BLOCK_DEVICE_V1` is **one 512-byte
   sector per request**. Whether the textual side is eight sequential requests
   against an oracle's one 4 KiB descriptor chain, or both do eight, is the
   difference between measuring the protocol and measuring the driver. R1's
   *same queue depth* is 1 on the textual side by construction.
4. **The boundary.** This report would measure through the accepted boundary —
   client ↔ `block.device.v1` ↔ service ↔ device — and a measured client needs
   measurement-only markers (ADR-0066 §4's form, for Stage 3). Whether the
   markers sit in canonical text or in a measurement runtime image, as Stage 3's
   did, is part of the same decision.

### What is known without a method, recorded as observational data only

Not a conformance number, not a ratio, and not P1 of R1–R3 — it is the host clock
at which serial event lines were received, which includes serial transport:

```text
host wall-clock between consecutive READ completions, TCG, n = 33 (three boots)
  median 39.14 ms   min 38.66 ms   max 44.38 ms
  => about 13 KiB/s sequential at queue depth 1, 512 bytes a request
```

Inside one READ, the time goes to byte-at-a-time loops over 512-byte buffers in
interpreted canonical text: the driver fills a sentinel before the request and
checks it after, copies the 512 bytes into the answer region, and the benchmark
client checks all 512 on arrival — each on the order of 10 ms under TCG. Only the
copy is required by the accepted contracts (ADR-0037); the sentinel is the
evidence harness's, and a client need not check its data. Even the copy alone
would be milliseconds against what a native driver under the same TCG profile
would take per request. **No reading of R1–R3 is likely to be met by the current
engine without either execution-engine work or a bulk-copy primitive**, and
`docs/35` names exactly those as the response to a miss — not moving the driver
into the nucleus.

## 5. What this report does not do

- It does not weaken, reinterpret or re-scope any budget.
- It does not build an oracle, choose a clock or define a workload for R1–R3.
- It does not count `state.store.v1` end-to-end cost as the block budget
  (ADR-0099 §11); that remains P0 observational.
- It does not claim H2 from an instrumented byte count; it is counted from the
  service's text.

## 6. Decisions this requires of the Project Architect

1. **H1 against ADR-0097/ADR-0098.** Either a decision that a funded, linear
   answer region per READ is not "dynamic allocation" in `docs/35`'s sense — an
   amendment of the budget's wording, by ADR — or a change to the READ shape that
   lets an answer region be reused (for example, the client supplying the region a
   READ fills).
2. **H3 against the READ shape and the scheduler.** Either an amendment of the
   budget (including what an idle wait counts as), or a one-message READ, or a
   scheduling rule that does not hand over on wake — with a ruling on whether that
   last one is within ADR-0049.
3. **The Stage 4 measurement method for R1–R3:** clock (and for R3, which host
   threads), oracle placement and isolation, workload shape for 4 KiB against a
   one-sector protocol, and where the markers live — in the form ADR-0040,
   ADR-0066 and ADR-0068 took for Stages 2 and 3.
4. **Whether a miss on R1–R3 closes Stage 4 as an accepted gap or opens engine
   work first.** The observational data in §4 says what to expect.

## 7. Reproduction

```sh
bash source/host-tools/qemu-test/stage4-request-cost.sh
./scripts/preflight.sh --profile qemu
```
