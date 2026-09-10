<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0086: DMA publication and consumption ordering

- Status: **Proposed**, at revision 2 (awaiting final Project Architect review).
  Revision 1's two open questions are decided (§20); revision 1's six review
  corrections are applied (§21)
- Date: 2026-09-10
- Decision level: **3** (§13). It adds two source operations, one
  verifier-visible IR operation, a `TOSIMAGE` encoding version and **TOS Core
  1.4** — and it writes the first ordering relation the language has ever had
  between a TOS Core context and an agent that is not one
- Project Architect approval: *(none — this ADR is not accepted. Nothing in it
  may be implemented before it is: not `Op::DmaSync`, not the encoding change,
  not TOS Core 1.4, not the runtime hook)*
- Related: **ADR-0081** §9, §10, §11 (device memory, observability, and the
  ordering gap this closes), **ADR-0082** §7 and §12 (delivery, and the boundary
  it left), **ADR-0084** §4, §5c, §7 (write-back DMA memory, P2–P4, and
  Stage 4C-3 by name), **ADR-0085** §8 (use, copy and consume are three things),
  ADR-0080 §3/§6/§7 (what a capability effect is), ADR-0037 (transfer and
  share), ADR-0070 (fail-closed unknown container version), `docs/11` §DMA,
  `docs/39` §2, `docs/40` §"Evaluation order", `docs/41` §3–§5, `docs/42` §1,
  `docs/43` §5, `docs/44`

## 0. Chronology, recorded rather than tidied

**This ordering was not overlooked. It was deferred on purpose, three times, by
decisions that each named it and each said why they were not deciding it.**

1. **ADR-0081 §11** (2026-09-04) fixed MMIO-against-MMIO ordering and stopped
   exactly there: "It does not yet assert DMA visibility before a notify, or DMA
   completion visibility after an interrupt." Its stated reason was that
   deciding a cross-domain rule "under pressure from a read-only configuration
   probe would be deciding the portable DMA memory model by accident";
2. **ADR-0082 §12** (interrupt authority) left "the MMIO↔DMA ordering contract"
   in its explicit not-decided list;
3. **ADR-0084 §7** (DMA authority) named it again and gave it a stage: "The
   MMIO↔DMA ordering contract (Stage 4C-3)";
4. `docs/11` §DMA carries the same sentence in the Tier-2 driver model: "The
   MMIO↔DMA ordering contract is still open and is Stage 4C-3's."

5. **Stage 4D opened with an ordering audit** rather than with descriptor rings,
   and the audit reached a STOP: no accepted construct creates either edge. The
   STOP was reported and not worked around;
6. The Project Architect accepted the STOP, directed that the mechanism be a
   language/IR primitive rather than a nucleus service, and directed that
   Stage 4C-3 be taken before Stage 4D. **This document is that proposal.**

So the sequence is the one the preservation policy asks for: the gap was named
by the decision that found it, carried forward without being quietly filled, and
is being decided now because a real queue is the first thing that needs it — not
because a queue was already half-written around an assumption.

7. **Revision 1 was reviewed and returned with corrections**, on 2026-09-10:
   two approvals, one substantive semantic repair to the consume side, and four
   places where the draft claimed more than the accepted documents support. §21
   records each. Revision 2 is what came back.

**Nothing in this ADR has been implemented.** The tree contains no `DmaSync`, no
fence of any kind, no `System::dma_sync`, no encoding change, no TOS Core 1.4 and
no virtqueue.

## 1. The gap, stated exactly

A split virtqueue driver needs two visibility edges, and the accepted corpus
provides neither:

```text
descriptor and ring stores  ->  the device's view of that memory      ABSENT
the device's completion     ->  this context's loads of that memory   ABSENT
```

Four accepted documents were checked independently, and they agree:

| Source | What it establishes |
|---|---|
| **ADR-0081 §11** | the gap, named by the decision that found it |
| **`docs/41` §4–§5** | happens-before relates **TOS Core contexts**. A release "synchronizes-with an *acquire operation* that reads its value"; a device performs no acquire and reads no TOS atomic |
| **ADR-0082 §7** | delivery is specified exhaustively — one waiter, edge with a one-bit latch, coalescing, EOI in the nucleus — and attaches **no memory-visibility meaning** to a wake. "A bit says 'something happened since you last looked'" |
| **`docs/43` §5** | the verifier's `region/DMA` family requires "typed grant, rights, checked range/alignment, transfer/share rule, no physical-address exposure" — no ordering property, and no place for one. The one row that mentions a "memory-order contract" is `atomic`, whose partner must be another atomic |

The ADR-0082 row is the sharpest of the four, because it is the one where a real
synchronising event exists in the machine and the contract still does not use
it: **`irq_wait` returning `OK` is a scheduling fact, not a visibility fact.**

## 2. Why what exists today does not compose into one

Three things a reader might reasonably expect to be enough, and are not:

**Source evaluation order** (`docs/40`: "TOS evaluates expressions
left-to-right") is *sequenced-before* over the operations of one context. It
constrains the order of the program's own steps; it says nothing about when a
third party that is not a TOS context can see their effects. A backend that
preserves evaluation order while holding a store in a write buffer has violated
nothing.

**`System::observe` does not order `System::access`**, by construction and on
purpose — the engine's own contract says so where the two are defined:

```text
observe   one hardware transaction of the declared width, not elided,
          coalesced, repeated, or reordered against another such access
access    "a load or a store in coherent memory, with none of those
          obligations and none of that cost"
```

Widening `observe` to cover `access` would give ordinary DMA RAM MMIO
transaction semantics — refused in §17 for the same reason ADR-0084 §4 refused
mapping DMA memory `UC`. What is missing is a **bridge between two domains**,
not a merger of them.

**Atomics cannot reach the far side.** `Op::Atomic` exists, `docs/41` §5 gives it
a real memory-order contract, and every partner in that contract is another
atomic operation performed by a TOS Core context. A device reads bytes, not
`AtomicU32` objects, and performs no acquire. (In the reference engine
`Op::Atomic` is additionally `RUNTIME_OPERATION_NOT_IMPLEMENTED`, but that is an
implementation fact and not the argument.)

## 3. D1 — two source operations, named for their directions

```tos
dma_publish(region);
dma_consume(region);
```

Both:

- return `unit`;
- take exactly one operand;
- are **non-consuming uses** of the affine region (ADR-0085 §8: use, copy and
  consume are three different things, and this is the first). The same region
  value is usable afterwards, which is the whole point — a driver publishes the
  same ring many times;
- expose no address, no offset, no length, and no physical or device-visible
  quantity;
- accept the existing sealed family and nothing else;
- introduce no allocation, no pointer and no range mechanism.

**They are predeclared functions of the language** (`docs/39` §2), joining the
twenty that are there already — `share`, the conversions, and ADR-0081's eight
MMIO accesses. They are **not** operations of an accepted interface schema, they
consume no `SYSTEM_ABI_V1` selector, and §17 records why that was the choice.

**The element type is irrelevant to synchronisation**, so the rule is over the
family and not over a type:

```text
DmaRegion<T>
DmaRegion<mut T>
```

That operation 30 currently produces only `DmaRegion<mut u8>` is a fact about
today's single schema row (ADR-0085 §18), not a property of ordering. Writing
`u8` into this contract would make a synchronisation primitive depend on an
allocation surface it has nothing to do with, and would have to be widened again
by the first ADR that adds a second element type. The check is nominal — the
operand's exact type is one of those two constructors — never structural, and
never over "some type that looks region-like".

**Both mutabilities are accepted, including `dma_publish` on `DmaRegion<T>`** —
and the reason is narrower than the one revision 1 gave. That draft argued from
a region "narrowed from a mutable name, or shared from a context that wrote it",
and **the review was right that this is not a true statement about the accepted
language.** ADR-0037 §2 fixes both DMA variants as neither shareable nor
transferable, and TOS Core has no source-level

```text
DmaRegion<mut T>  ->  DmaRegion<T>
```

attenuation. Those examples are withdrawn; the correct argument does not need
them:

```text
mutability   current CPU write authority over the region
DmaSync      a visibility operation, and not a write
```

`dma_publish` writes no byte, grants no authority, creates no alias, consumes no
ownership and changes no access right. Nothing it does requires the authority
that `mut` names, so requiring `mut` would be a type rule with no obligation
behind it. On an immutable region with no CPU write ordered before the
operation, the publication set is simply **empty** — well-defined, and costing
nothing on any profile.

**This ADR introduces no DMA sharing, no transfer and no mutability
attenuation.** ADR-0037's four facts are exactly as they were.

**No `unsafe` marker.** `docs/44`'s unsafe surface exists for operations that
suspend a safe-caller guarantee; this one adds an ordering obligation and
removes none. Nothing about it can produce a value, an address, or an access
that the safe language did not already permit.

## 4. D2 — one IR operation, two directions

```text
enum DmaSyncDirection {
    Publish,
    Consume,
}

Op::DmaSync {
    region: Operand,
    direction: DmaSyncDirection,
}
```

**One form, because the two operations differ in exactly one closed field.**
Their operand shape, result type, ownership behaviour, verifier obligations and
backend hook are identical. Two unrelated tags would duplicate eight verifier
rules to express one bit, and every later reader would have to prove they had
stayed in step. `Op::Atomic` already carries its `MemoryOrder` this way, and
`Op::Lock` its mode.

**It is verifier-visible for MMIO's reason, and it does not have MMIO's
semantics.** ADR-0081 §8 gave MMIO its own operations because "an ordinary read
is free to be optimised, and a rule that depended on nobody doing so would not be
a rule". The same sentence applies here and only that sentence does:

```text
the backend must not be allowed to treat the ordering point
as an ordinary call that may disappear or move
```

What follows for MMIO — exactly one hardware transaction, no coalescing, no
repetition, a declared width and byte order — **does not follow here.** A
`DmaSync` implies no device transaction at all, and §9 says explicitly that a
backend may emit zero machine instructions for one.

## 5. D3 — the operand is the authority, and no interface effect is required

**This ADR does not require the enclosing function to declare
`uses [platform.dma.Region]`** for `dma_publish` or `dma_consume`.

ADR-0080 §3 defines a capability effect precisely: `uses [P]` means "this
function may perform operations whose capability positions require the accepted
nominal interface `P`", and §6 requires `P` to be "declared by an accepted
interface schema". `DmaSync` is deliberately not such an operation — it has no
capability position and no schema row. Requiring an effect for it would mean one
of two things, and both are worse than the problem:

- either the effect model now covers operations no schema declares, which
  silently widens `docs/43` §5's `capability` family while trying to solve an
  ordering problem;
- or a fake schema row is added to justify the effect, which is the syscall
  design §17 rejects, wearing a different hat.

**Authority is established directly, by the operand:**

```text
the operand is an actual DmaRegion value
```

and the verifier proves its exact type independently (§15). Holding the value
*is* the authority — it was produced by an operation that required two
capabilities neither of which is sufficient (ADR-0084 §4), and it cannot be
forged from scalar data (`docs/43` §5).

**This is the existing rule for this family, not a new one.** ADR-0081 §2's
indexed access already works exactly this way, and the accepted conformance
surface says so in one line:

```tos
pub fn get(d: DmaRegion<mut u8>) -> u8 { return d[0B]; }     // no `uses` at all
```

A function that may already read and write every byte of a region without
declaring an effect is not restrained by making it declare one before saying
*when* those bytes become visible. In the Stage 4C-2 fixture the effect on
`lifecycle` is required by `dma_device_address` and `capability_release` —
schema operations with capability positions — and not by `region[0B]`.

**Unchanged:** every actual `platform.dma.Region` interface operation still
requires the effect. `dma_device_address` and `capability_release` are
operations of an accepted schema and this ADR touches neither.

**No separate architecture question is raised.** Writing the proposal did not
find a compelling reason to widen the effect model for language-level device
synchronisation, so none is proposed; if a later decision finds one, it should
argue it on its own and not inherit it from here.

## 6. D4 — `dma_publish` is CPU → device visibility

> **When `dma_publish(region)` completes, writes to that region that
> happen-before the operation are visible to the device the region is associated
> with, before any device transaction that follows the ordering point.**

It is a **directional publication point for one region**, not a memory fence. It
says nothing whatever about:

- unrelated ordinary memory;
- atomics, or any `docs/41` §5 memory order;
- another `DmaRegion`;
- another TOS Core context;
- mutex, channel, event or task happens-before;
- MMIO-against-MMIO ordering, which stays ADR-0081 §9's.

### 6a. Which writes are published, and what that set contains today

The general definition is stated by **composing two existing relations and
inventing neither**:

```text
published = { writes to this region that happen-before this operation
              under docs/41 §3-§4 }
```

**In the accepted language that set has exactly one possible source, and the ADR
says so rather than implying otherwise.** ADR-0037 §2 fixes both DMA variants:

| Type | `Copy` | mutable | Shareable | `Transferable` |
|---|---|---|---|---|
| `DmaRegion<T>` | no | no | **no** | **no** |
| `DmaRegion<mut T>` | no | yes | **no** | **no** |

A DMA region therefore never crosses a task boundary and never reaches a second
TOS Core context — which is the crossing ADR-0037 exists to forbid, and the
reason it refused to make `DmaRegion<T>` shareable even though the ordinary
`Region<T>` is. So:

```text
Current TOS Core: DmaRegion is neither shareable nor transferable, so the
current publication set cannot contain writes from another TOS context.
```

For the one context that can hold the region, `happens-before` is exactly
`docs/40`'s evaluation order, and nothing else contributes.

**The general form is kept anyway, and deliberately.** It is written as a
composition rather than as "writes sequenced-before in this context" so that a
future decision which deliberately adds DMA sharing or transfer makes the
composition non-vacuous *without amending this ordering definition*. What must
not be read into it is a claim that ADR-0037 permits such a writer today. It
does not, and no example in this ADR depends on one.

### 6b. One barrier before the notify is not the whole problem

The ADR states this because the obvious reading of "publish before you notify"
is wrong for a queue, and a primitive designed around the wrong reading would be
too weak the day it was used:

```tos
// descriptor contents first
write_descriptor(region);

dma_publish(region);

// now publishing the ownership marker cannot expose it while the
// descriptor bytes it points at are still unpublished
write_avail_idx(region);

dma_publish(region);

// and the marker itself is published before the device is told to look
mmio_write_le_u16(notify_window, QUEUE_NOTIFY_OFFSET, queue_index);
```

**A device may observe an ownership marker without being notified.** VirtIO
permits a device to poll the available ring, and a notification is a hint that
may be suppressed. So there are two distinct handoff points — the marker and the
notification — and the primitive has to be able to order writes before *either*.
An implicit barrier attached to the notifying MMIO write could only ever order
the second (§17).

**The contract is not queue-specific.** Nothing above appears in §6's normative
sentence; the litmus is here to show why that sentence has the shape it has, and
§16 turns it into a source vector rather than a story.

## 7. D5 — `dma_consume` is device → CPU visibility

> **When `dma_consume(region)` completes, device writes to that region that were
> complete before the synchronisation point are visible to reads of that region
> that follow the operation in this context.**

Directional and region-scoped, like its partner. It is **not** an atomic
acquire: it creates no `docs/41` happens-before between two TOS Core contexts,
and it is not a partner for any release.

```tos
let woken: i64 = irq_wait(source);

dma_consume(region);

let index: u8 = region[used_index_offset];
```

A polling driver may also synchronise its own view *before* examining a
device-written indicator:

```tos
dma_consume(region);
let flag: u8 = region[used_flags_offset];
```

**This second form is legal only because `dma_consume` is a standalone
synchronisation point, and the distinction matters enough to state here rather
than in a footnote.** There is no device observation preceding it — no status
read, no MMIO read, no wake — so nothing anchors the ordering except the
operation itself. A *compiler-only* barrier could not implement this form: it
constrains what the compiler may move and emits no instruction, so a later
region load could still execute before the conceptual point. §11 is what makes
the form legal: on x86-64 `Consume` lowers to a compiler barrier **and an
execution barrier**, so no later load executes until the point completes.

The more typical shape is the ownership pattern, where a device-written
indicator is read first and the barrier separates it from the payload it
governs:

```tos
let completed: u8 = region[status_offset];

if (completed != 0u8) {
    dma_consume(region);
    let payload: u8 = region[data_offset];
}
```

Here the status load is itself the hardware-side observation, and the barrier
only has to keep the payload loads behind it. **This is the shape Linux's
`dma_rmb()` serves, and it is why Linux can implement that barrier as a compiler
barrier on x86 without implying that a free-standing TOS `dma_consume` can.**
The two are different obligations: Linux's is anchored by a preceding load, and
`dma_consume`'s first form is anchored by nothing. TOS keeps the stronger,
standalone meaning — §17 records why weakening it into a protocol-dependent
operation was refused — and pays for it in the backend rather than in the
contract.

**`dma_consume` does not invent completion**, and this is the sentence that keeps
it honest. Which device write constitutes completion — a used-ring index, a
status byte, an interrupt-status register read — is the device protocol's to
define, and stays entirely outside this ADR. The primitive says only: *whatever
the device had finished writing before this point, you will see.*

It pairs naturally with ADR-0082 §7's wake without being defined by it. A wake
means "something happened since you last looked", so the driver's obligation is
still to drain until empty; `dma_consume` is what makes each look at the ring
see the device's writes rather than a cached view of them.

## 8. D6 — whole-region semantics for V1

The operations apply to **the whole region**. The V1 source surface gains no
`offset`, no `length`, no cache-line, no physical range and no device address.

The region is already bounded, already typed, and already the unit every other
part of this model works in. A range parameter would be an optimisation
hypothesis about platforms none of which are in front of us, and on the
reference profile — where the implementation is a compiler barrier — it would
optimise nothing at all while adding arithmetic the verifier would have to check
and the nucleus-free path would have to bound.

**The trade is deliberate and stated:** whole-region semantics may cost more on a
future non-coherent platform, where `consume` could imply cache maintenance over
a large region. A range-scoped variant is **additive** and can be proposed by
the decision that has such a platform and a measurement. Buying that flexibility
now, in the dark, would enlarge the first contract for a benefit nobody can
demonstrate.

## 9. D7 — the optimisation rule

**The semantic operation may not be treated as absent.** It is not a call the
backend may inline away, not a no-op it may delete because its lowering happens
to be empty, and not an ordinary function whose ordering effect can be inferred
away.

But:

```text
semantic operation exists   does not imply   machine instruction exists
```

A backend **may emit no machine instruction** for a `DmaSync` when its
target and profile prove the visibility relation already holds in hardware
(§11 is precisely such a proof) — provided it still preserves the ordering
boundary in everything it is allowed to reorder. Specifically it may not:

- move a `DmaRegion` access of that region across the point in the direction the
  operation forbids;
- move the device notification or the device observation that the point exists
  to order across it;
- merge, duplicate, or hoist the point out of a loop that contains accesses it
  orders.

**And it does not acquire MMIO's semantics** (ADR-0081 §9): no
exactly-one-transaction rule, no non-coalescing rule, no non-repetition rule, no
width or transaction ordering. Those remain `System::observe`'s and are not
extended by this ADR.

**The permission is per direction and per target, and the proof can fail.** §11
is a worked case of both outcomes on one machine: `Publish` needs no instruction
on the reference profile and `Consume` does. "The profile proves it already
holds" is a proof obligation, not a default, and a backend that cannot discharge
it emits the instruction.

## 10. D8 — the engine and backend boundary

A dedicated semantic host hook, conceptually:

```text
System::dma_sync(region_handle, Publish | Consume) -> Result<(), Trap>
```

distinct from all three that exist, and the separation is load-bearing:

| Hook | What it is |
|---|---|
| `System::reach` | an accepted interface operation — a `SYSTEM_ABI_V1` selector, a ring transition |
| `System::observe` | one MMIO hardware transaction (ADR-0081 §9) |
| `System::access` | one ordinary load or store in coherent region memory (ADR-0081 §2) |
| **`System::dma_sync`** | **a visibility boundary for one region. No transaction, no access, no selector** |

**The runtime proves liveness before it synchronises.** It must first establish
that the handle still names a live mapping it holds; a retired or unknown
mapping is refused **deterministically, before any synchronisation and before
any memory is touched** — the same rule and the same mechanism as the stale
indexed-access path already uses. A refusal is a `Trap`, for `System::access`'s
stated reason: the operation did not happen, and there is no status standing for
one that did.

**No address of any kind crosses this boundary** — not the CPU mapping address,
not the device-visible address. The hook takes the handle the module already
holds.

## 11. The current x86-64 reference profile

```text
Publish  ->  compiler ordering barrier, no hardware fence instruction
Consume  ->  compiler ordering barrier  AND  an execution barrier (LFENCE)
```

**Revision 1 proposed a compiler barrier for both, and the review found that
wrong.** The two sides are not symmetric, and the asymmetry is not a detail:

```text
Publish   orders this CPU's stores against each other before a handoff.
          x86 does not reorder stores with stores in WB memory, so once the
          compiler is forbidden to move them, nothing else can.

Consume   must prevent a later region load from executing — including
          speculatively — before the synchronisation point. A compiler barrier
          establishes no architectural execution point. It emits no instruction,
          so it constrains the compiler and nothing in the machine.
```

`dma_consume` is a **standalone** synchronisation point (§7): in its first form
nothing precedes it that the hardware could anchor an ordering on. An
implementation that emitted no instruction would therefore be weaker than the
contract it claims to implement — the exact failure this ADR exists to prevent,
committed in the ADR itself.

### The premises, each with the document that establishes it

1. **DMA memory is write-back and coherent on this profile** — ADR-0084 §4: "on
   x86-64 the platform is DMA-coherent for write-back memory, so a DMA region is
   mapped **write-back**". So no cache maintenance is required in either
   direction;
2. **No Snoop is held disabled** — ADR-0084 §5c P4, *enforced* by the nucleus and
   nucleus-owned so CPL 3 cannot re-enable it. This is what makes premise 1 hold
   for device writes as well as CPU writes: a non-snooping device write "may
   leave the CPU reading stale data";
3. **Relaxed Ordering and ID-Based Ordering are disabled** — ADR-0084 §5c P2 and
   P3, both *enforced*, so the fabric does not reorder the device's transactions
   against each other in ways the profile's ordering argument assumes away;
4. **MMIO is strong UC** — ADR-0081 §10: `PCD` and `PWT` set, which with the
   reset-state PAT selects UC, "no speculative reads, and accesses reaching the
   device in program order". So the notify store is not held behind the CPU's
   store buffer relative to the DMA stores in a way that inverts them;
5. **x86-64 reorders neither stores with stores nor loads with loads in
   write-back memory** — cited exactly below;
6. **LFENCE is an execution barrier** — cited exactly below;
7. **The compiler is not permitted to move region accesses across the point** —
   §9 of this ADR, a requirement this ADR imposes rather than a premise it
   borrows.

### Premises 5 and 6, cited from the manual rather than from memory

Read from the vendor PDFs, at the revision named:

> **Intel® 64 and IA-32 Architectures Software Developer's Manual, Volume 3A:
> System Programming Guide, Part 1.** Order Number **253668-092US**, **June
> 2026**. §**11.2.2**, "Memory Ordering in P6 and More Recent Processor
> Families", page **Vol. 3A 11-7**. For a single-processor system and memory
> regions **defined as write-back cacheable**:
>
> - "Reads are not reordered with other reads."
> - "Writes to memory are not reordered with other writes, with the following
>   exceptions: — streaming stores (writes) executed with the non-temporal move
>   instructions (MOVNTI, MOVNTQ, MOVNTDQ, MOVNTPS, and MOVNTPD); and — string
>   operations."
> - "Reads cannot pass earlier LFENCE and MFENCE instructions."
> - "LFENCE instructions cannot pass earlier reads."
>
> The same volume states the rule again as §**11.2.3.2**, "Neither Loads Nor
> Stores Are Reordered with Like Operations", page **Vol. 3A 11-10**: "The
> Intel-64 memory-ordering model allows neither loads nor stores to be reordered
> with the same kind of operation."

> **Intel® 64 and IA-32 Architectures Software Developer's Manual, Volume 2A:
> Instruction Set Reference, A-L.** Order Number **325383-092US**, **June 2026**
> (combined volumes 2A–2D). **LFENCE—Load Fence**, page **Vol. 2A 3-552**:
>
> - "LFENCE does not execute until all prior instructions have completed
>   locally, and **no later instruction begins execution until LFENCE
>   completes**."
> - "Instructions following an LFENCE may be fetched from memory before the
>   LFENCE, but they will not execute (even speculatively) until the LFENCE
>   completes."

**The chapter number is the reason this had to be read.** Memory ordering is
§11.2.2 in revision 092; in older revisions of this manual it was §8.2.2. An ADR
that cited the remembered number would have been wrong about the current
document while sounding precise, which is ADR-0084 revision 3's failure exactly.

**Two obligations the manual creates, which revision 1 did not know about:**

- **the store-store rule has exceptions.** Non-temporal stores (MOVNT*) and
  string operations are outside it. A backend must therefore not lower
  `DmaRegion` writes to non-temporal stores, or `Publish` must additionally emit
  `SFENCE`. This is now a stated backend obligation rather than an accident that
  happens to hold because nobody emits MOVNT today;
- **LFENCE does not order speculative cache fills.** The same page says data
  "can be brought into the caches speculatively just before, during, or after
  the execution of an LFENCE instruction". That is harmless *here*, and only
  because of premises 1 and 2: the line is coherent, so a device write
  invalidates it. **Neither premise alone carries the consume edge** — coherence
  without LFENCE leaves the load free to execute early, and LFENCE without
  coherence leaves it reading a stale line. They are load-bearing together.

The point is semantic, not the spelling of the inline assembly: any backend
sequence that prevents a later region load from executing before the point,
including speculatively, satisfies `Consume`. `LFENCE` is named as the
established way to get it, not as part of the contract (§12).

### Corroboration, narrowed to what it actually demonstrates

Linux on x86 implements its DMA read barrier as a compiler barrier and no
instruction. The chain was read on this machine, Linux 6.5.0 as packaged
(`linux-headers-6.5.0-1mx-ahs-common`):

```text
arch/x86/include/asm/barrier.h:54   #define __dma_rmb()  barrier()
arch/x86/include/asm/barrier.h:55   #define __dma_wmb()  barrier()
include/asm-generic/barrier.h:46    dma_rmb() -> kcsan_rmb(); __dma_rmb()
include/asm-generic/barrier.h:50    dma_wmb() -> kcsan_wmb(); __dma_wmb()
include/linux/compiler.h:85         #define barrier() __asm__ __volatile__("": : :"memory")
```

**What this does and does not corroborate**, stated because revision 1 cited it
past its evidence:

- it **does** corroborate `Publish`. A store-store publication point on this
  architecture needs no instruction, and Linux's `dma_wmb()` agrees;
- it **does not** corroborate a compiler-only `Consume` with the semantics §7
  gives it. `dma_rmb()` serves the *ownership* shape — a device-written status
  or index is read first, and the barrier separates that read from the payload
  reads it governs (§7's second litmus). The preceding load is the hardware-side
  anchor, and it is what lets the barrier be free. TOS's `dma_consume` has no
  such anchor in its first form, so Linux's implementation is evidence about a
  **narrower** obligation than the one being implemented here.

Linux is corroborating implementation evidence and is not the TOS normative
contract. If the citation were wrong, premises 1–7 would still have to stand on
their own.

**Second-vendor corroboration is absent, and that is recorded rather than
approximated.** AMD64 APM Volume 2 §§7.1.1 and 7.1.2 would be a reasonable
second source for the x86-64 ordering model; the document could not be retrieved
in this environment, and citing section numbers for a manual nobody opened is
precisely what this section refuses to do. The proof rests on the Intel
citations above, which were read.

**And the honest consequence.** `Publish` costs nothing on this profile, which is
exactly why it must be written now: the first virtqueue would work without it,
the omission would be invisible in every QEMU run, and it would surface on the
first machine where the edge is not free — as a data corruption with no failing
test behind it. `Consume` costs one `LFENCE`, which is the price of a
synchronisation point that means what it says without a protocol wrapped around
it.

## 12. Weaker and non-coherent platforms

The normative contract is a **visibility relation**, and it stays one. This ADR
deliberately does **not** state that a future AArch64 backend "uses `DSB`" or
"uses `DMB`": no accepted AArch64 compatibility profile has been analysed, and
naming an instruction for a target nobody has qualified would be revision 3's
mistake in a new register.

What can be said architecturally: a backend for a weaker or non-coherent target
may need architecture memory barriers, cache clean and invalidate operations,
privileged assistance, or some combination of them. **Whatever mechanism it uses
must implement these same two semantics.** A platform on which they cannot be
implemented cannot claim support for the language feature — which is `docs/41`
§3's existing rule for a stated ordering contract, applied here: "An execution
engine that cannot implement a stated atomic/happens-before rule must reject the
module."

A privileged backend helper is permitted, and this is where §17's rejected
alternative would return legitimately: a target that genuinely needs privileged
cache maintenance may implement `dma_sync` through one. **That does not make the
source or IR operation a `SYSTEM_ABI_V1` operation** — it makes one backend's
lowering privileged, which is a property of that backend and not of the
language.

## 13. TOS Core 1.4, and the level

New source operations and a new verifier-visible IR semantic. By `docs/42` §1
and `docs/44` that is a language minor:

| Minor | Decision | What it added |
|---|---|---|
| 1.4 | **ADR-0086** | DMA publication and consumption ordering |

- **1.0 through 1.3 are unchanged**, in meaning, diagnostics and digest;
- a **1.3 module** using either operation is
  `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR` with `feature = "DMA ordering"`,
  `declared = 3`, `requires = 4` — the existing machinery of `checker.rs`, one
  more constant beside `DEVICE_MEMORY_MINOR` and
  `CAPABILITY_REPRESENTATION_MINOR`, and no new mechanism;
- an implementation that has not implemented 1.4 rejects a **1.4 module whole**
  by its header, with existing `E1602_UNSUPPORTED_LANGUAGE_MINOR`;
- **acceptance does not advertise implementation.** Exactly as `docs/42` records
  for 1.3, the frontend advertises minor 4 only when it performs it — after the
  whole path exists and is green. Between acceptance and that moment a 1.4
  module is refused whole, which is the fail-closed direction;
- **`tos_ir::LANGUAGE_VERSION` does not move** because a new maximum minor
  exists. It is the baseline a module gets when it says nothing, and nothing
  about this decision changes what an existing module means.

**Level 3** for the reason ADR-0081 was: it changes accepted source syntax, the
IR, the container encoding and the engine boundary at once, and it decides a
memory-visibility relation that every later device contract will build on.

## 14. `tos-ir/v1`, `TOSIMAGE`, tags and identity

**The IR schema version stays `tos-ir/v1`.** The operation is additive and
language-minor gated — the precedent ADR-0081 set for the additive MMIO
operations, where the semantic major did not move because no existing program
changes meaning.

**The `TOSIMAGE` encoding version moves**, because an instruction stream that can
carry a new operation is different bytes, and ADR-0070's fail-closed
unknown-version rule is what makes that safe: `ENCODING_VERSION` 5 → 6, with 5
joining 3 and 4 in `READABLE_ENCODING_VERSIONS`. A version-5 image cannot contain
the new tag, so decoding one stays unambiguous.

### Tag assignment, audited rather than assumed

At `e32c04a` the operation tag space is, mechanically:

```text
allocated   0 .. 24        the original operation set
allocated   38, 39         MmioRead, MmioWrite      (ADR-0081)
never allocated  25 .. 37
```

**The gap is recorded as a fact and given no explanation.** ADR-0081 assigned 38
and 39 while 25..37 stood empty, and no accepted document says why. Any reason
this ADR offered would be a reconstruction, so it offers none:

```text
25..37 = historically unallocated legacy gap
```

It is **not** available space merely because the current writer has no arm there.

**Approved assignment** (Project Architect, 2026-09-10):

```text
Op::DmaSync   ->  operation tag 40
direction     ->  a nested closed discriminator: 0 = Publish, 1 = Consume
```

40 is the next tag above every tag allocated in either space. **Nothing in
25..37 is back-filled by this decision**, and **no existing tag is renumbered.**

The implementation slice must **mechanically re-verify, immediately before
landing**, that 40 is still the next tag above every actually allocated
operation tag — in both the image writer and the digest writer — rather than
trusting this paragraph.

### What stays identical, stated precisely

- **module digests of every existing 1.0–1.3 module are unchanged.** The
  canonical digest stream hashes the module's own declared language version and
  its structure; it does not hash the container's encoding version, and a module
  containing no `DmaSync` emits no new tag;
- **the instruction-stream bytes of such a module are unchanged**, for the same
  reason;
- **previously written images still decode**, because 5 stays readable;
- **and one thing does change, which must not be papered over:** a *re-encoded*
  old module's container header carries 6 where it used to carry 5, so its
  whole-image bytes differ in that field. That is what moving a container version
  means, it is what happened at 4 → 5 under ADR-0081, and the conformance
  requirement in §16 is written against the identity that is actually stable —
  the module digest — rather than against a byte equality that a version bump
  cannot preserve.

**The digest must distinguish the two directions.** `DmaSync(Publish, r)` and
`DmaSync(Consume, r)` are different programs, and the discriminator is written
into the canonical stream so they hash differently. `docs/43` §6's identity rule
requires nothing less.

## 15. Verifier obligations

For every `Op::DmaSync`, proved independently of the frontend and of any
producer-supplied string:

1. the artifact's declared language minor admits the operation (≥ 4);
2. `region` **dominates** the operation under the ordinary dominance rule;
3. its exact type is `TypeDef::DmaRegion(_)` or `TypeDef::DmaRegionMut(_)`;
4. every other type **refuses** — a scalar, an ordinary `Region`/`RegionMut`, an
   `MmioRegion`/`MmioRegionMut`, a `Capability("platform.dma.Region")`, an
   aggregate, a `Shared`, a task or a guard;
5. `direction` is one value of the closed discriminator, and an out-of-range
   discriminator is a malformed artifact rather than a default;
6. the operation's result type is `unit`;
7. it **consumes no ownership**: the region's affine state is unchanged across
   it, and the same value is usable afterwards;
8. it is not representable as, and is not accepted as, `Op::Call`,
   `Op::Capability`, `Op::Read`, `Op::Write`, `Op::MmioRead` or `Op::MmioWrite`.

Obligation 4 is the one that matters most in practice: the forged-IR negatives
of §16 exist to prove the verifier derives the operand's type from the artifact
rather than believing a producer that says "this is a DMA region".

## 16. Conformance set the proposal requires

**Frontend and verifier**

- `dma_publish(DmaRegion<T>)` accepted;
- `dma_publish(DmaRegion<mut T>)` accepted;
- `dma_consume` accepted in both forms;
- a wrong operand family refused by the frontend — `Region`, `MmioRegion`,
  `Capability("platform.dma.Region")`, and a scalar;
- **forged IR** with each of those operand types refused by the verifier
  independently, and a forged out-of-range direction refused;
- a **1.3 source** using either operation refused with
  `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`, `requires = 4`;
- a **forged 1.3 artifact** carrying `DmaSync` refused by the verifier
  independently of the frontend gate;
- a 1.4 module refused whole with `E1602` by a frontend that has not implemented
  1.4.

**Encoding and identity**

- encode/decode roundtrip for both directions;
- `Publish` and `Consume` produce **different** canonical digests and different
  image bytes;
- every existing 1.0–1.3 module's **module digest is byte-identical** to its
  pinned value, and its instruction-stream bytes are unchanged (§14 states what
  the container header does instead);
- a version-5 image still parses.

**Semantics and runtime**

- the operation is **non-consuming**: the same region is used after it, in both
  directions, and the ownership checker accepts it;
- a **stale or retired** region refuses deterministically at the runtime
  boundary, before any synchronisation;
- **exactly one** semantic backend synchronisation is issued for one `DmaSync` —
  observed through a recording `System`, not inferred;
- on the x86-64 reference backend, **`Consume` emits an execution barrier and
  `Publish` emits none** — the asymmetry §11 argues for, checked against the
  emitted sequence rather than assumed;
- `DmaRegion` writes are **not** lowered to non-temporal stores, which §11 makes
  a standing backend obligation;
- **no `SYSTEM_ABI_V1` selector is added**, proved by the existing ABI gate;
- **no accepted schema operation is added**, proved by the existing schema gate;
- **no address becomes source-visible**, which is ADR-0084 §6c's standing rule.

**Ordering litmus vectors**, as canonical source:

```text
descriptor writes        -> publish -> ownership marker write
ownership marker write   -> publish -> MMIO notification
completion observation   -> consume -> DMA data reads
```

**And an honest separation of what each kind of evidence proves.** These vectors
are *semantic and compiler* evidence: they prove the operation survives the
whole pipeline, appears where the source put it, and orders what it claims to
order in the artifact and in the backend's output. They are **not** weak-memory
falsification, and the ADR must not pretend otherwise — QEMU on x86-64 cannot
exhibit an execution the architecture forbids, so no green run on the reference
machine is evidence that the barrier was needed. *Functional* device evidence —
a real queue moving real data — is separate, belongs to Stage 4D, and proves the
driver works, not that the ordering rule holds.

## 17. Alternatives, and why each loses

**A general memory fence.** Too broad: it would decide an ordinary-memory
visibility model TOS has not written and does not need here, and every future
question about ordinary memory would then have to be answered consistently with
an accident.

**Existing atomics with acquire/release.** Their synchronizes-with relation pairs
a release with an *acquire operation that reads its value* (`docs/41` §5). A
device performs no acquire and reads no TOS atomic, so the far side of the edge
can never exist. The relation would be stated and unsatisfiable.

**Widening `System::observe` to cover region access.** It would collapse ordinary
DMA memory into MMIO transaction semantics — exactly-one-transaction,
non-coalescing, non-repetition — for memory that ADR-0084 §4 deliberately maps
write-back. It would make every ring access pay a device transaction's price and
would make "a DMA region is RAM" false.

**An implicit barrier inside the MMIO notify.** It cannot express the consume
side at all — there the ordering point is a wake, not a store — and §6b shows it
is too weak even on the publish side, because a device may observe an ownership
marker without any notification. It would also tie a memory-visibility rule to
one transport action, so any device that hands off differently would be
unserved.

**Schema operations on `platform.dma.Region`, consuming `SYSTEM_ABI_V1`
selectors.** Wrong layer. Their semantics are a compiler, backend and platform
visibility constraint over a region the caller already holds; on the reference
profile the correct implementation is *no privileged transition and no hardware
fence at all*, so every barrier would pay a ring-0 round trip for an operation
that emits nothing. It would also bind a portable language memory-ordering
primitive permanently to one ABI mechanism — and a future target that needs
privileged cache maintenance is served by §12's backend helper without any of
that.

**Relying on x86 ordering, QEMU behaviour, or source evaluation order.** Not a
portable TOS contract, not checkable by a verifier, and — for evaluation order —
not even a statement about visibility to a non-TOS agent (§2).

**Weakening `dma_consume` into a protocol-anchored barrier**, whose meaning holds
only when a device-written status read, an MMIO read or an interrupt return
precedes it. That is the obligation Linux's `dma_rmb()` actually carries (§11),
and it would make `Consume` free on x86 — by making the language contract depend
on a pattern the language cannot see. The verifier cannot prove that a preceding
load was a device observation; the type system does not distinguish one region
byte from another; and a driver that got the pattern wrong would have written a
program whose ordering silently did not hold. TOS keeps the standalone meaning
and pays one `LFENCE` for it.

## 18. What this ADR does not decide

Virtqueue structures, descriptor rings, VirtIO feature negotiation, device reset,
block I/O, scatter-gather beyond one contiguous region, IOMMU domain management,
a range-scoped synchronisation variant (§8), an ordinary-memory visibility model,
any change to the capability effect model (§5), any AArch64 or non-coherent
profile (§12), and the interrupt-to-visibility relation as anything other than
what §7 states — ADR-0082 §7's wake keeps exactly the meaning it has.

## Architecture impact statement

- **Change level:** 3. **Invariants affected:** none amended. I-07 is unchanged;
  the operation carries no authority of its own and produces nothing. I-08 is
  unaffected: on the reference profile the primitive costs no ring transition.
- **Canonical representation:** one new IR operation and one closed
  discriminator, both additive and language-minor gated.
- **Trusted-base impact:** **none.** The nucleus learns nothing, gains no
  operation, and is not on the path. This is the property that distinguishes this
  design from the rejected syscall alternative.
- **Source-to-runtime impact:** `TOSIMAGE` encoding version 5 → 6, with 5
  readable; `tos-ir/v1` unchanged and additive; one new engine hook. Module
  digests of existing modules unchanged (§14).
- **Threat-model impact:** none new. No authority, no address, no mapping and no
  reachable object is created; the operand must already be held.
- **Compatibility profile:** **TOS Core 1.0–1.4**; `SYSTEM_INTERFACE_V1`,
  `PLATFORM_INTERFACE_V1` and `SYSTEM_ABI_V1` **unchanged** — no interface, no
  operation, no selector, no object kind and no right is added.
- **Bounded-resource impact:** none. The operation allocates nothing, blocks
  never, and consumes fuel as one instruction.
- **New dependencies:** none.

## 19. Cross-document reconciliation, on acceptance

Recorded here so the edits are reviewed as part of the decision rather than
discovered afterwards. **None of the normative edits happens before approval.**
The only thing this slice writes into other documents is a *forward pointer* in
each of the four places that name the open boundary — one sentence saying a
proposed decision exists and is not accepted, which decides nothing and lets a
reader of ADR-0081 §11 find this document instead of concluding the gap is
untouched.

| Document | Change |
|---|---|
| `docs/39` §2 | two names in the machine-checked predeclared-function inventory: `dma_publish`, `dma_consume` (20 → 22), with the EBNF terminals the Stage 2 gate compares against |
| `docs/40` | the visibility relation of §6 and §7, alongside evaluation order, stated as the one relation that reaches outside a TOS context |
| `docs/41` §4 | a note that these operations create **no** happens-before between TOS Core contexts, so the table stays exhaustive for what it covers |
| `docs/42` §1 and the minor table | minor 4, attributed to this ADR, with the "accepted is not implemented" paragraph 1.3 already has |
| `docs/43` §5 | the `region/DMA` family gains the ordering property it currently lacks, and the `DmaSync` verifier obligations of §15 |
| `docs/44` | `E1608`'s feature name for this gate, and the conformance expectations of §16 |
| `docs/11` §DMA | "still open and is Stage 4C-3's" becomes the decision, once there is one |
| ADR-0081 §11 | the boundary it named is closed by this decision — recorded as a forward reference, without rewriting what §11 decided |
| ADR-0082 §12, ADR-0084 §7 | the same forward reference in each not-decided list |
| `docs/SPECIFICATION_SOURCES.txt` | the ADR is added **on acceptance**, as `docs/38`'s release check requires of an accepted ADR and does not require of a proposed one |

## 20. What writing this proposal did not require

The instruction was to stop and report if the semantics above turned out to need
a broader ordinary-memory model, a new capability or effect rule, or an
unmentioned authority mechanism. Each was checked, and none is needed:

- **no ordinary-memory model.** Both sentences name `DmaRegion` accesses on the
  CPU side and one device on the other. Ordinary memory, atomics and other
  regions are explicitly outside them (§6, §7);
- **no new capability or effect rule.** Authority is the operand, which is
  ADR-0081 §2's existing rule for this family, demonstrated by an accepted
  conformance function that takes a `DmaRegion<mut u8>` with no `uses` clause at
  all (§5). The effect model is untouched, and no reason to widen it was found;
- **no new authority mechanism.** No object, no right, no selector, no interface,
  no mapping. The runtime's liveness check reuses the stale-mapping rule that
  already exists;
- **one composition, not one invention.** §6a's publish set composes `docs/41`
  §4's happens-before with the single new edge to the device. Nothing in
  `docs/41` is restated, widened, or given a new partner.

The one place where the direction as given cannot be met literally is recorded
rather than quietly satisfied: **a container version bump cannot leave a
re-encoded old image byte-identical**, so §14 states what is stable — the module
digest and the instruction stream — and what is not.

Revision 1 put two questions to the Project Architect. **Both are decided**, on
2026-09-10, and are recorded in the sections they belong to rather than left
here as open items:

1. **operation tag 40 — approved.** 25..37 is recorded as a historically
   unallocated legacy gap, with no reason invented for it and no back-filling
   (§14);
2. **`dma_publish` on an immutable `DmaRegion<T>` — approved**, with the
   rationale replaced: mutability is current CPU write authority, `DmaSync` is a
   visibility operation and not a write (§3).

## 21. What revision 2 changed

Revision 1 was returned with six corrections. Each is applied, and two of them
were substantive errors rather than wording:

| | What revision 1 said | What revision 2 says |
|---|---|---|
| **§3** | an immutable region "may have been narrowed from a mutable name, or shared from a context that wrote it" | **Wrong about the accepted language.** ADR-0037 §2 makes both DMA variants non-shareable and non-transferable, and no `DmaRegion<mut T> -> DmaRegion<T>` attenuation exists. The examples are withdrawn; the argument is now that `DmaSync` is a visibility operation and not a write, so `mut` names an authority it does not need |
| **§6a** | "incomplete for a region that has been shared or transferred (ADR-0037)" | the general happens-before composition is kept, and the current fact is stated with it: **a DMA region reaches no second TOS context today**, so the publication set has one possible source. The general form is retained so a future sharing decision makes it non-vacuous without amending the ordering definition |
| **§11** | `Consume -> compiler barrier` | **The proof did not support the semantics.** A compiler barrier establishes no architectural execution point, so a later WB load could still execute — speculatively — before a standalone consume. `Consume` now lowers to a compiler barrier **and** `LFENCE`; `Publish` is unchanged |
| **§11** | premise 5 cited "the specific architecture-manual rule" as an outstanding obligation | discharged, from the actual PDFs: SDM Vol. 3A **253668-092US, June 2026, §11.2.2 p. 11-7** and §11.2.3.2 p. 11-10; SDM Vol. 2A **325383-092US, June 2026, LFENCE p. 3-552**. Reading them found two obligations nobody had stated: the store-store rule's non-temporal and string-operation exceptions, and that LFENCE does not order speculative cache fills — harmless only because premises 1 and 2 hold |
| **§11, §17** | Linux's `dma_rmb()` cited as corroboration for both directions | narrowed to what it demonstrates: it corroborates `Publish`, and it does **not** corroborate a compiler-only standalone `Consume`, because Linux's barrier is anchored by a preceding device-written status read. §17 now records the protocol-anchored `Consume` as a refused alternative |
| **§14** | "very likely why ADR-0081 began the device operations at 38" | the speculation is removed. `25..37 = historically unallocated legacy gap`, with tag 40 approved and mechanical re-verification required immediately before landing |

**What did not change:** the source surface, the single IR form and its
discriminator, the authority model, whole-region semantics, the engine boundary,
TOS Core 1.4, the encoding move, the verifier obligations, and every normative
sentence of §6 and §7. The consume *contract* was kept exactly as written and
its *implementation* was strengthened to match it — not the other way round.
