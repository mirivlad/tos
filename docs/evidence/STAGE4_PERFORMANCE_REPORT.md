<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — performance contract report

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
