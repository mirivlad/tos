<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 evidence — the Region round: three states, two transitions, one transport

- Status: **evidence, gathered 2026-09-02**
- Covers: the Region state refactor, per-process mapping ownership, operation 18
  (`region_freeze`), operation 7 (`region_share`), IPC receive atomicity, the
  send queue preflight, and Region transfer in both its affine and shared forms
- Related: ADR-0037 §2–§4, ADR-0075 §3, §5a, §6, ADR-0076 §7, `CAPABILITY_V1`
  §3–§4, `IPC_V1` §3, §5–§7, §9, `SYSTEM_ABI_V1` §4–§5

## 1. What is claimed

A Region has three states and two consuming transitions between them, and it
only ever moves forward:

```text
MutableAffine    one capability, read|write, one exclusive holder,
                 one writable mapping, neither transferable nor shareable
ImmutableAffine  one capability, read|share, one exclusive holder,
                 read-only mapping, transferable linearly, consumable by share
SharedImmutable  capability refs may exceed one, read only, read-only mappings
                 in several processes, no exclusive holder, copyable
```

Affinity is a property of the **object**, not of the rights on any handle to it.
An immutable affine region and a shared one carry the same absence of `write`;
a rule that read affinity off the rights mask would treat them as the same thing
and let an attenuation that dropped a bit turn one into the other. So the
capability table carries two internal variants — `Object::Region` and
`Object::SharedRegion` — that both describe themselves to a process as
`OBJECT_REGION`, and the region itself is what answers "may a second capability
name this?".

## 2. Per-process mapping ownership, and why counters could not do it

The region object records **which** address spaces map it, as a bitset over
process slots, and not how many. Three questions have to be answerable and a
pair of counters answers none of them:

| Question | A count says | The set says |
|---|---|---|
| is region `R` mapped in process `P`? | nothing | yes or no |
| in which mode? | nothing about `P` | writable, readable, or neither |
| does this handle's release take the window? | nothing | only if it is `P`'s last |

The last row is the one that decides correctness. A process may hold several
`SharedRegion` capabilities for one region and still have exactly one physical
window at one address; a count that grew with the handles could never say when
the window should go. `map` therefore refuses an incompatible duplicate rather
than counting it, `unmap` refuses a process that did not map it rather than
clamping, and the aggregate counts are derived by popcount so the two cannot
disagree.

Process death clears exactly that process's bit in every region, and gives up
that process's ownership of anything affine it still held. Capability references
are **not** swept: `capability::clear` walks the table entry by entry and drops
one reference per entry actually destroyed, which is the only shape that works
once a region can be shared — a sweep asking "which regions did this process
hold?" cannot tell one alias from three, nor this process's share of a region
from another process's.

`MAX_MAPPING_SPACES` is restated in `region.rs` rather than imported from
`process.rs`, because the state machine is compiled and exercised on its own by
`source/tests/integration/tests/region_lifecycle.rs`; `process.rs` asserts at
compile time that the two numbers agree.

## 3. The two consuming transitions

Both keep the capability's table slot and advance its generation. Granting a
fresh capability and releasing the old one would take a **second** reference the
affine region refuses and a **second table slot** a full table may not have,
with a window in between where the region is named twice. Rewriting the entry
where it stands takes neither: the presented handle is stale by exactly the rule
that makes any released handle stale, the returned one is the only live name,
and the region's reference count is one throughout.

**Operation 18, `region_freeze`.** The whole lane is judged before the first bit
of it moves — the branch is there, it has exactly the committed page count,
every leaf names the frame the backing index names for that page, every leaf is
present, user-accessible and not executable, every leaf is currently writable,
and nothing at all is mapped past the region's length. Below that line the
window is demoted in place at the same addresses over the same frames, one bit
per leaf, and `CR3` is reloaded once. No frame moves, no page table moves, no
account moves. A failure before the line leaves the region completely mutable.

**Operation 7, `region_share`.** The same in-place replacement. The region stops
having an exclusive holder; the caller's mapping does not move. `RIGHT_SHARE` is
what the caller presents and never what it receives — carrying it onto the
result would make a shared region shareable again, which is a transition with
nothing left to consume.

## 4. Acceptance is a transaction

The receive path was `dequeue, then grant` — and a grant that failed wrote a
zero handle where authority should have been. That is a partial delivery wearing
a success status, and it becomes unacceptable the moment a region can travel:
a message dequeued and then found unacceptable is a message nobody has and
nobody can retry.

It is now `peek → preflight → commit → copy → pop → release transit refs`.
`ipc::peek` answers what the oldest message carries without moving it and
without touching the payload — copying the 256 bytes there would be a third
payload copy for a message `IPC_V1` §8 budgets two. The preflight asks
everything that can refuse, in one place:

- enough capability slots for the capabilities **and** the regions, counted as
  objects rather than as positions (a call reserves the last slot for its answer
  whatever else it carries, so its transfer table has gaps);
- the one-receiver rule of `IPC_V1` §2, checked exactly as `grant` checks it —
  a preflight stricter than its commit refuses legal messages and one looser
  fails half way;
- every authority's name count, summed, because one message naming one authority
  twice costs it two names;
- an address space to map into;
- each region in the state its transfer requires — an affine one owned by
  nobody, a shared one still shared;
- a free lane for each window that has to be built, deduplicated so that two
  records naming one shared region build one window;
- and enough reserve to build them.

If it cannot be satisfied the answer is `E_LIMIT`, the message is still queued,
every transit reference still stands and the receiver gets nothing partial. This
is fixed for generic capabilities too, not only for regions.

**A preflight stricter than its commit is a defect, and this one found it.** The
first version also asked whether each object was still *usable*, which `grant`
does not ask — a capability's lifetime is bounded by its object
(`CAPABILITY_V1` §3) and `resolve` is where that is enforced, once, when the
holder tries to act. The case is not hypothetical: `endpoint_call` hands the
receiver the right to answer **before** the caller blocks, so at the instant of
delivery the reply capability names a call that has not begun to wait yet. Every
request/reply boot deadlocked until the extra question was removed. The rule the
preflight is written to is now stated where it lives: ask exactly what the
commit asks, entry by entry, and nothing more.

## 5. A failed send leaves the sender everything

The queue's room is now a pure question asked **before** anything is consumed. A
linear region taken from its sender and then discovered to have nowhere to go
belongs to nobody, and there is no rollback that can be relied on to put it
back: rebuilding the sender's window needs page tables from the reserve and can
fail on its own. So `ipc::has_room` is asked first, and below the commit line
nothing may refuse.

A sender that blocks for room has given up nothing — not its handles, not its
mappings, not a transit reference. When room appears, what runs is the **same**
transaction again, with the arguments the suspended frame still holds. The frame
*is* the record of what the call was, so re-resolving in the sender's table is
not resolving it twice.

## 6. The Region transfer area, and the ABI amendment

Regions travel in the separate area `IPC_V1` §3 already reserved for them, never
in the generic capability slots — spending a capability slot on a region would
make one message able to consume the other's bound. The record is fixed:

```text
MessageRegion { handle: u64, base: u64, length: u64 }
```

A sender fills in `handle` and nothing else; the base and length it might write
are its own address and mean nothing in another address space, so the nucleus
ignores them rather than validating a number it is about to overwrite. A
receiver is given all three: its own new handle, the address the nucleus chose
in its address space, and the charged and mapped length. Unused records are
zeroed, and a handle of all zeros names nothing in any table.

`SYSTEM_ABI_V1` §5 now assigns `r10` = transferred capability count and `r8` =
transferred region count on `endpoint_send` (1) and `endpoint_call` (3). `r9`
stays unused. `r10` was already the capability count and is unchanged; `r8` was
previously unassigned on both, so a nucleus reading it from an older caller
would be reading a register nobody wrote — the same safety argument that made
`r8` admissible on operation 15. The runtime image writes every argument
register an operation reads, including the zeros.

**Nothing was widened on `endpoint_reply` (4) or `endpoint_reply_receive` (13),
and no accepted document requires it.** `IPC_V1` §4 describes the answer to a
call and states only that a reply capability is single-use; §5's region transfer
is stated over the message path. A reply that transferred authority would need
its own preflight, its own bound and its own refusal semantics, and inventing
them would be this contract making a decision rather than carrying one. No STOP
is raised because no normative requirement exists to cite.

## 7. Evidence

### 7.1 Host — the state machine on its own

`cargo test -p tos-tests-integration --test region_lifecycle` — 31 tests over
the nucleus's own `region.rs`, compiled directly rather than copied.

| Test | What it establishes |
|---|---|
| `a_region_is_allocated_frozen_transferred_shared_and_reclaimed` | the whole sequence: writable in one space and no other, freeze moving the writable bit to the readable one in place, linear detach/adopt with the message's internal reference bridging the gap, share, a second holder, and reclamation only when the last of everything goes |
| `the_mode_is_one_way` | neither transition may be reached out of order, neither has an inverse, and each needs the holder's own window to be exactly what the state says |
| `a_shared_region_survives_the_death_of_one_of_its_readers` | one reader's death clears one bit and one entry; the other holder is untouched, and the backing goes exactly once |
| `several_shared_names_in_one_process_are_one_mapping` | three names, one address space, one window; a second window in the same space is refused rather than counted |
| `a_process_ending_reclaims_what_only_it_could_reach` | per-entry capability release plus per-slot mapping destruction |
| `an_unmapping_of_nothing_is_a_defect` | `mapped_by` answers which space and which mode, which a count cannot |
| `the_negatives_are_refusals_and_not_surprises` | a mutable region is neither transferable nor shareable; only the sole holder may freeze; a second capability naming an affine region is refused; an out-of-bound process slot names no address space |
| `a_full_table_refuses` | the sixty-fifth region is refused, and the tree's free bytes, committed bytes, allocated bytes and live-region count are identical on both sides of the refusal |
| `a_stopped_tree_refuses_to_grow_and_still_gives_back` | a poisoned tree answers `Refusal::Stopped` to every path that grows, changes not a byte, and still lets everything shrink |

**The region-table bound is host evidence on purpose.** Sixty-four simultaneous
ring-3 regions would need a fixture built to hold them and would prove the same
thing about the same fixed table. What ring 3 proves instead is the bound a
process actually runs into first — a full *capability* table, in §7.2.

**The poisoned tree is host evidence plus one mapping.** `Refusal::Stopped` has
no word in the closed status space of `SYSTEM_ABI_V1` §4, so operations 16 and
17 answer the nearest accepted resource refusal: everything but `Empty` and
`BadArgument` becomes `E_LIMIT`. The state machine proves the refusal and the
dispatcher's `Err(_) => E_LIMIT` arm is the whole of the mapping.

### 7.2 QEMU — `memory-authority.sh`

Operations 16, 17, 18 and 7 asked from CPL 3 on a real machine, by a process the
launcher endowed a child of the root authority.

```text
state share_mutable=-1  freeze=0  rehandled=1  stale_mutable=-1  refreeze=-1
      kept=1  share=0  reshaped=1  stale_frozen=-1  reshare=-1
      freeze_shared=-1  after_share=1  alias=0  dropped_alias=0
      survived=1  last_name=0
table aliases=10  full=-6  freed=0  after=0
```

- `share_mutable=-1` — a mutable region carries no share right;
- `rehandled=1`, `stale_mutable=-1` — the freeze returns a **different** handle
  and the presented one is stale by generation;
- `refreeze=-1` — no inverse and no repeat;
- `kept=1`, `after_share=1` — the bytes written before the freeze are the bytes
  read after it *and* after the share, at the same address: nothing moved;
- `alias=0`, `survived=1`, `last_name=0` — generic attenuation admits a second
  name for a shared region, dropping one leaves the memory readable, and only
  the last one takes the window;
- `full=-6`, `after=0` — a full capability table refuses operation 17 before
  anything is charged, and one freed slot makes the same request succeed.

Then: every frame back to the root's count, every table back to the reserve's
baseline, and no `TOS.NUCLEUS.INVARIANT` line.

### 7.3 QEMU — `region-transport.sh`

Two processes and one endpoint. The worker holds a child authority and `send` on
two endpoints — one the peer drains and one **nobody receives on at all**,
because a queue nothing can drain is the only way to ask what a full queue does
to a linear transfer. The peer holds `receive` and nothing else, so every region
it comes to hold arrived in a message.

```text
worker  delegated_sent=0 froze_shared=0 shared=0 alias=0 shared_sent=0
        shared_kept=1 alias_dropped=0 after_alias=1 mutable_refused=-1
        still_writable=1 overcount=-3 frozen=0 filled=4 queue_full=-6
        refused_full=-6 intact=1 sent=0 stale=-1
peer    refused=-6 freed=0 delegated=0 handle=0x300000001 first=0 second=0
        shared_read=0x5348415245445f31 moved_read=0x4d4f5645445f5f31
        moved_tail=0x4d4f5645445f5f32 distinct=1 alone=-5
        shared_after=0x5348415245445f31 moved_after=0x4d4f5645445f5f31
```

Three messages, in that order: an ordinary capability delegation, the shared
region, the affine one. The **first** is deliberately not a region — what the
full table refuses and then accepts is a plain message, which is what makes the
all-or-nothing property one of every message rather than of region transport.

| Claim | Evidence |
|---|---|
| a shared region transfers without being given up | `shared_sent=0`, `shared_kept=1` — the sender keeps handle and window |
| several local names, one window | `alias_dropped=0`, `after_alias=1` |
| a mutable region is refused **whole** | `mutable_refused=-1`, `still_writable=1` |
| a count past the bound is malformed, not limited | `overcount=-3` (`E_BAD_ARGUMENT`) |
| a full queue refuses and takes nothing | `queue_full=-6`, `refused_full=-6`, `intact=1`, and then `sent=0` with the **same handle** |
| a successful linear send consumes the handle | `stale=-1` |
| …and takes the window with it | the worker's next read of that address faults: exactly one `TOS.RUN.PROCESS_FAULT … cpl=3`, and its `TOS.RUN.REGION.WORKER.UNREACHED` line never appears |
| acceptance is all-or-nothing, for **any** message | the peer fills its own table on purpose: `refused=-6` on a message carrying a capability and no region, the message stays queued, and one freed slot later `delegated=0` delivers it whole with a non-zero `handle` — never the zero handle a partial delivery used to write |
| the receiver gets the bytes, at an address the nucleus chose | `shared_read`, `moved_read`, `moved_tail` are exactly what the worker wrote, at both ends of the charged length |
| region identity is not an address | `distinct=1` — two regions in two lanes with two handles |
| transit lifetime works across the sender's death | `alone=-5` is a blocking receive **cancelled** by ADR-0059's liveness rule, which in a boot of two processes where the other never blocks fires exactly when the other has ended; `shared_after` and `moved_after` are read after it, and the nucleus's own `TOS.RUN.BLOCK_CANCELLED … reason=no-runnable-context` is on the log beside them |
| the backing is reclaimed exactly once | every frame back to the root's count, every table back to the baseline |

### 7.4 QEMU — `region-faults.sh`

Two dedicated processes, each of which ends in a fault, beside an ordinary first
process that completes its work — a fault is the evidence here rather than a
failure, and one in a process that was also doing something else would be a
fault nobody could attribute.

| Case | Fault |
|---|---|
| NX: write an instruction into a region and jump to it | `vector=14 error=0x15 cpl=3` — present, user, **instruction fetch**, at the base the nucleus gave it. A leaf that is mapped and readable refusing to be executed, not an address that is not there |
| use-after-release: release the only handle, then read | `vector=14 error=0x04 cpl=3` — not present, user, read, at the base the nucleus gave it |

Neither `UNREACHED` line appears. All three processes are reclaimed, the pool
returns to the root's frame count and the reserve to its baseline.

## 8. The handoff failure is structurally unreachable, and is not manufactured

Operation 17's fault-injection harness drives six failure points
(`creation-rollback.sh`). It does **not** drive the capability handoff, and the
reason is that the handoff cannot fail — which is a claim to prove rather than a
convenience.

`capability::grant` refuses for exactly two reasons. `NotGranted::NoRoom` needs
the caller's table to be full or the object's own retain to refuse;
`NotGranted::ReceiverExists` needs an endpoint object with `RIGHT_RECEIVE`, and
this grant names a region with `read | write`. At the point the handoff runs:

- `capability::has_room(caller)` was answered **true** in the preflight, before
  the authority was charged, and nothing between there and here writes a
  capability entry for this process — the intervening steps are the region
  table, the frame pool, the backing index and the caller's page tables;
- the region was created by `allocate_rounded` with `capabilities: 0` and
  nothing has retained it, so `retain_capability` takes its one reference by the
  same rule that refuses a second.

So the branch is a fail-closed backstop for a state that would already be a
defect, and it says so: it records `region-handoff` as a divergence and undoes
the transaction whole. Manufacturing a rollback test for it would mean forcing
the table and the region to disagree, which would be testing the injection
rather than the nucleus. The branch is kept rather than removed because removing
it would replace a reported divergence with an unreported one; what is removed
is any suggestion that it is a reachable refusal.

## 9. What the reserve did not do

The accepted reference-machine figures are unchanged, and every gate above
asserts them rather than assuming:

```text
1452  total reserve
1451  runtime free baseline
   1  permanent RegionBackingSpace root
```

Freeze consumes no page table: it clears one bit per leaf of a lane that already
exists. Share consumes none: it moves no mapping at all. Affine transfer builds
the receiver's lane out of the same reserve every mapping has always come from,
and the receiver's preflight asks the reserve for the cost of that lane before
committing to it. No legal state in this round needs frame 1453.

## 10. Environment-only failures

Recorded here so that a red line in a local run is classified rather than
tolerated. None of the gates in §7 depends on any of them, and each is
independent of this slice:

`bash scripts/preflight.sh --profile qemu` — **39 of 41**. The two that did not
run are `stage3-observer-conformance` and `stage3-ipc-conformance`, and both
exit 2 with the same line before booting anything:

```text
the selected QEMU has no ADR-0066 observer-build.json
```

ADR-0066 fixes the measurement boundary at one external observer, and the gates
check for its provenance manifest beside the `qemu-system-x86_64` on `PATH`
(`stage3-observer-conformance.sh:46`, `stage3-ipc-conformance.sh:30`). This host
has the distribution's QEMU rather than the one
`host-tools/qemu-test/build-simple-observer.sh` installs, so the gates refuse to
begin rather than measuring through an instrument nobody qualified — which is
the refusal working.

**Independent of this slice, and demonstrably so:** neither gate reaches a boot,
so neither executes a region, a freeze, a share or a region transfer. The
refusal is a property of the machine the gate was run on and would be identical
at any commit.

`bash scripts/preflight.sh` — **36 of 36**, with no threshold weakened.
