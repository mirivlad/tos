<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0097: The textual surface by which an ordinary `Region<T>` crosses IPC

- Status: **Proposed** (awaiting Project Architect decision, raised 2026-09-23).
  **It blocks the Stage 4D data-path slice**, which cannot be implemented without
  it: §2 is the proof rather than an opinion
- Date: 2026-09-23
- Decision level: **3**, and that is the finding. `SYSTEM_INTERFACE_V1` §4.3's
  representation enumeration is closed, and the document states that "**adding a
  member is a decision of ADR-0085's weight**". ADR-0085 is Level 3,
  Project Architect-approved, TOS Core 1.3. By ADR-0085 §13's own test — a
  capability position a conforming pre-amendment frontend and verifier reject
  becomes valid — this also takes a **language minor**
- Related: **ADR-0085** (capability representation, and the closed enumeration);
  **ADR-0094** §10, which explicitly left region transfer from canonical text
  undecided; **ADR-0037** (`Region`/`DmaRegion` transfer and share rules);
  `IPC_V1` §3, §5; `SYSTEM_ABI_V1` operations 1, 3, 7, 17, 18;
  **ADR-0058** (`MESSAGE_REGIONS`); **ADR-0057** (two regions per message);
  `docs/42` §2 (the seven grant facts a region-originating operation declares);
  `docs/40` §3; `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1, §2

## 0. What this is for, and why it is raised rather than implemented

The Project Architect directed that the next Stage 4 target be the real data
path — 512 bytes of one sector crossing the client/service boundary through the
accepted region mechanism — on the premise that it is "already anticipated by
`IPC_V1`, `MESSAGE_REGIONS`, `STAGE4_DATA_PATH_BOUNDARY.md` and the existing
region machinery."

**That premise is right about the transport and wrong about the textual
surface.** The transport is complete and has been since Stage 3:
`qemu_region_transport` moves a region between two processes today. What does not
exist, and cannot be added without a decision of this weight, is the way a
*canonical textual* module names one. §2 is why.

This ADR is therefore written instead of the slice. Nothing was implemented, no
accepted contract was edited, and the schema, the ABI, the nucleus, the verifier
and the launch machinery are untouched.

## 1. What already exists, so that the gap is small and exact

| | where | state |
|---|---|---|
| a region crosses a message | `MESSAGE_REGIONS`, its own count register and its own bound of two (ADR-0058, ADR-0057, `IPC_V1` §3) | **complete** |
| the nucleus transfers one linearly | `send_transaction`'s region path; the sender's handle and its mappings go atomically (`IPC_V1` §5, ADR-0075 §5a) | **complete** |
| making one | operation 17 `region_allocate`; freezing it, 18 `region_freeze`; sharing it, 7 `region_share` | **complete in the ABI** |
| indexing one from TOS Core | the IR types `Region(T)`/`RegionMut(T)`, and the verifier types an indexed access through them exactly as it does a DMA region | **complete**, and needs no change |
| the host performing the access | `System::access` takes a region *handle* and a mapping the host holds; it is not DMA-specific | **complete** |
| **a textual module naming any of it** | — | **absent, and §2 is why it cannot simply be added** |

**So this is not a mechanism to be built.** It is four schema rows and some bridge
vocabulary, gated behind one closed enumeration.

## 2. Why it needs this decision: the proof

### 2a. A region-producing operation's result is unconstrained

`platform.dma.Region`'s producer is declared
`Result<DmaRegion<mut u8>, i64>` — a *result* type, and §4.3's representation rule
governs **capability positions**, not results. So `region_allocate` declared
`Result<Region<mut u8>, i64>` needs no representation. That part is free.

### 2b. And a produced capability may travel as a value parameter

There is precedent and it is load-bearing: `launch_plan_seal` takes
`Parameter::fixed("system.process.LaunchPlanBuilder")` — a capability the module
*made*, passed as a value rather than as a capability requirement. So a send row
could take the region as a value parameter typed `Region<u8>`, again with no
representation involved.

**Both escapes are real, and together they are not enough.**

### 2c. But one operation cannot avoid a capability position

`IPC_V1` §5: "`Region<mut T>` is neither shareable nor transferable: a writable
region handle may not be delegated or sent at all, and a send that names one is
refused whole." So the region must become immutable before it can travel, by
operation 18 (`region_freeze`) or operation 7 (`region_share`).

**Both take the region as the operation's own capability**, in `rdi`, at the right
the operation requires — 18 needs `write`, 7 needs `share`. And
`SYSTEM_INTERFACE_V1` §4.1 fixes what that means for a schema: the first
capability is the operation's own interface, the one `Signature.effects` records,
and its written parameter type must be that interface's path. There is nowhere
else for the region to go: the ABI puts it in `rdi` and no other register of
either operation accepts it.

So an accepted interface — call it `system.memory.Region` — must exist, and at
that capability position the argument is a `Region<mut u8>` value. By §4.3 that
requires `representation_of("system.memory.Region")` to be a family admitting
`TypeDef::Region(_)` and `TypeDef::RegionMut(_)`. The enumeration has two members
and neither is it:

```text
capability_representation ::= AsInterface | DmaRegionFamily
```

`AsInterface` admits "exactly `TypeDef::Capability(interface_path)`", which cannot
be indexed. `DmaRegionFamily` admits only the DMA families, and §4.3's cardinality
rule 1 — "one representation family belongs to at most one accepted interface" —
forbids sharing it, as does the repository gate that pairs the three tables
against the document.

**Therefore a new member of a closed enumeration is required, and the document
says what that costs.**

### 2d. And it takes a language minor

ADR-0085 §13's test for a minor is whether "a capability position a conforming
pre-amendment frontend and verifier reject becomes valid". This is exactly that.
`LANGUAGE_VERSION` would move to 1.5, after every part of the semantics exists —
the frontend's table, the verifier's own row, the bridge, and the gates — which is
the order ADR-0085 and ADR-0086 each moved it in.

## 3. What is being asked for, minimally

**One representation member, one interface, four rows, and bridge vocabulary.**
Stated so that approval is one step from implementation, and not designed further
than that.

| | proposal |
|---|---|
| representation | one new member of §4.3's enumeration, admitting `TypeDef::Region(_)` and `TypeDef::RegionMut(_)`, any element type. Mirrors `DmaRegionFamily` exactly, including cardinality rule 1 |
| interface | `system.memory.Region`. Object kind `Region` — which the nucleus already has and `granted()` already maps. **Not startup-importable**, by §4.3's derived rule: an import is typed `Capability(I)` and there is nowhere in it for an element type or a mutability to come from, exactly as for `platform.dma.Region` |
| row 1 | `region_allocate` on `system.memory.Authority` with `spend`, `bytes: size`, `Result<Region<mut u8>, i64>`, operation **17**. Must declare `docs/42` §2's seven grant facts; six are as `dma_region_allocate`'s, and **DMA domain is "none, and that is the difference"** — this region is reachable by no device |
| row 2 | `region_freeze` on `system.memory.Region` with `write`, no values, `Result<Region<u8>, i64>`, operation **18**. Consuming: the presented handle goes stale and the result is the immutable form (ADR-0075 §3) |
| row 3 | a send that carries a region: `system.ipc.Endpoint` with `send`, plus the region, over operation **1**, placing the handle in `MESSAGE_REGIONS[0]` and the count in the region-count register. The region may be a **value** parameter (§2b), which keeps the capability-position count at one |
| row 4 | a receive that produces what arrived: the existing `system.ipc.ReceivedCall` gaining a region field, or a sibling record. The nucleus writes `MessageRegion { handle, base, length }` for the receiver, and the bridge records the mapping from it so an indexed access resolves |
| bridge | a `Placed`/`Slot` destination for the region area — the mirror of `Placed::Transfer` for a different area with a different count and bound — and mapping registration for an ordinary region, reading `REGION_ALLOCATE_RECORD` on allocation and `MESSAGE_REGIONS` on receipt |
| nucleus, ABI | **unchanged.** No operation is added and none is re-specified |
| language | TOS Core **1.5**, moved last |

**Why a reply cannot be used, so that nobody proposes it.** `ipc::hand` copies
payload bytes from the replier's argument region into the waiting caller's and
touches neither the transfer table nor the region area. So a service answering with
a region sends it to a channel the asker handed over — the arrangement ADR-0093
P3's lookup already uses, and no new mechanism.

## 4. What it would let Stage 4 prove

One claim, and it is the one the Stage 4 identity gate's word "IPC" refers to
(`docs/37`): a client requests one sector; the service performs the real
VirtIO/DMA read; **all 512 bytes cross the client/service boundary through the
accepted region mechanism**; the client verifies them; and no privileged binary
helper moves the bytes across the textual boundary.

That closes the boundary `docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1 draws,
which the current slice does not reach — it transports one device-derived scalar.

## 5. What this does not propose

- **No filesystem, partitions, cache, VFS or storage protocol.** One sector, one
  client, one service.
- **No change to ADR-0037.** `DmaRegion` stays non-transferable and non-shareable
  in both modes, so the one forced copy between client memory and device-visible
  memory stays forced (`STAGE4_DATA_PATH_BOUNDARY.md` §1). Zero-copy remains
  unreachable by decision.
- **No widening of the transport.** ADR-0057's two regions per message and
  ADR-0058's offsets stand.
- **No second element type.** `u8`, as ADR-0085 §18 fixes for the DMA family, and
  for the same reason.
- **No writable region crossing anything.** §2c is a rule, not an obstacle to
  route around.
- **No Stage 4 closure**, and nothing about ADR-0096, which stays separate: this
  adds one interface with an object kind of its own, so it creates no pair sharing
  one kind.

## 6. Options

### 6.1 Option R-A — as §3, one new representation member

**Keeps:** the nucleus, the ABI and the object-kind set untouched; the transport,
the counts and the offsets unchanged; ADR-0037 unchanged; `IPC_V1` §5's linearity
enforced where it already is. The new member is the second in an enumeration built
to take one.

**Costs:** a Level 3 decision and a language minor, and the closed enumeration
grows from two to three — which is the thing ADR-0085 made deliberately
expensive. Every later widening still costs the same.

### 6.2 Option R-B — defer the data path

Leave Stage 4's data claim where it is: a device-derived scalar crosses IPC, the
512 bytes do not, and the boundary of §1 is not reached.

**Keeps:** everything. **Costs:** the Stage 4 identity question — "does a
canonical textual driver actually move persistent data through final-style
MMIO/interrupt/DMA/**IPC** boundaries?" (`docs/37`) — stays unanswered on its IPC
term, and Stage 4 cannot honestly close on it.

### 6.3 Option R-C — a non-region path for the payload

512 bytes as two inline messages of 256 (`IPC_V1` §3), with no region at all.

**Keeps:** no new representation, no new interface, no minor. Two existing rows
would nearly do it.

**Costs:** it is **not** the accepted data path. `IPC_V1` §5 and
`STAGE4_DATA_PATH_BOUNDARY.md` §2 both make the payload a region transferred as a
capability, "the nucleus maps and unmaps; it does not copy the payload through
itself" — and this copies it through the nucleus twice per 256 bytes. It would
prove that bytes can cross, and it would prove the wrong mechanism. Recorded so
that its rejection is on the record rather than assumed.

## 7. Recommendation

**R-A**, as a recommendation and not a decision. R-C would produce a green gate
and a false claim, which this project has just spent three rounds removing; R-B is
honest but leaves the Stage 4 identity question open on the term the stage is named
for. R-A is the accepted mechanism, and its cost is a decision the Project
Architect has priced before.

**What should not happen is R-A being taken quietly.** A new member of that
enumeration is exactly what ADR-0085 built a gate around, and adding one as a
convenience of a storage slice would be the failure that ADR forestalls.
