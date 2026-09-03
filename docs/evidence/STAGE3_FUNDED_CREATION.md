<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 evidence — funded creation, the retirement of ambient funding, and a process built from a bundle

- Status: **evidence, gathered 2026-09-02**
- Covers: the explicitly funded process-construction core, operation 19
  (`process_create_funded`), the retirement of operations 8 and 15, the
  withdrawal of the source-level `process_create`, and operation 20
  (`process_create_from_bundle`) with `Launch` v5
- Related: ADR-0055, ADR-0061, ADR-0067, ADR-0069, ADR-0073, ADR-0074 (Accepted 2026-09-03; Draft when this was written),
  ADR-0075, ADR-0076 §3–§4, §7; `SYSTEM_ABI_V1` §5, §7, `SYSTEM_INTERFACE_V1`
  §4, `CAPABILITY_V1` §3–§4

## 1. What was wrong

`process::create` reached for `memory::root()` and charged it. Every process
this system had ever built spent the boot's accounting anchor, and no caller had
to hold anything to make that happen. ADR-0076 §4 names it: an operation that
spends the root without a presented `MemoryAuthority` *is* the hidden second
counter, and operations 8 and 15 were both such operations.

## 2. The structural invariant, and how it is held

```text
no ring3-reachable process creation path
can allocate one user frame
without an explicit MemoryAuthority argument
```

The construction core is `process::create_funded(program, funding, endowment,
parent, restart_generation)`. It takes the funding node as an argument and has
no way to obtain one: nothing in it names `memory::root`, and there is no
`funding.unwrap_or(root)` anywhere below it.

Exactly one path may pass the anchor, and it is named `create_bootstrap`. It
does not fetch the root either — the boot resolves it once and hands it over —
so every use of the anchor is findable by searching for one identifier. Auditing
the callers after the refactor:

| Caller | Funding |
|---|---|
| `main.rs` boot build closure | `create_bootstrap(entry, boot_root, …)` |
| `main.rs` creation-rollback probes | `create_bootstrap(entry, root, …)` |
| `syscall.rs` operation 19 | the caller's `MemoryAuthority`, resolved from `rsi` |
| `syscall.rs` operation 20 | the same |

There is no fifth.

## 3. The grant is an argument, and a policy figure

`grant_bytes(wanted)` validates against the accepted `RuntimeMemoryGrant`
bounds and rounds up to whole frames; what is charged is the rounded figure
(ADR-0076 §7). There is deliberately no `min(requested, available)`, no
"what is left" and no share of a parent — a size chosen from the pool is the
second counter §1 describes, measured at a moment and spent later.

The charge is the **whole** footprint and not the arena alone:

```text
data 8 192 + grant 56 623 104 + stack 2 097 152 + report 65 536
     + arguments 4 096 + record 4 096  =  58 802 176   (56.08 MiB)
```

Page tables stay outside the tree, in the proved reserve.

Three refusals, told apart, and each proved from ring 3 in one line:

| | condition | status |
|---|---|---|
| impossible | an arena outside the accepted domain | `E_BAD_ARGUMENT` |
| unaffordable | a legal arena a *particular* authority cannot pay for | `E_LIMIT` |
| malformed | a non-canonical `CreateFundedRecord` | `E_BAD_ARGUMENT` |

`TOS.RUN.PROCESS.FUNDING reserved=0 impossible=-3 unaffordable=-6 malformed=-3
distinguished=1`. The unaffordable case is funded from a megabyte reserved out
of what the caller holds, so the same request succeeding through the parent
authority a moment earlier is what says the charge follows the authority that
was presented.

## 4. Absence is not zero

Operation 19 replaces 8 **and** 15. Eight asserted no restart generation and
fifteen asserted one, so the operation replacing both has to keep *absent* and
*present and equal to zero* apart — `PROCESS_IDENTITY_V1` §5's rule that absence
is the true value and a zero would be a claim nobody made. A register cannot
carry that difference, so it travels in a record:

```text
CreateFundedRecord { restart_generation: u64, flags: u64 }
flags bit 0 = HAS_RESTART_GENERATION; every other bit must be zero
flag clear  => restart_generation must be zero
```

One canonical encoding, checked rather than assumed: a caller that leaves
rubbish in the field with the flag clear is refused rather than having the
rubbish ignored, because two byte patterns meaning the same thing is a contract
with an undocumented second spelling. Unknown flag bits are refused for the
reason §7 makes operation numbers permanent — a nucleus that ignored them would
have already accepted, with a different meaning, every record a later version
will write.

Proved in `lifecycle.sh`: a child created with the flag clear reports
`generation_present=0`, beside siblings created with generations 7, 9 and 11.

## 5. The funding lifecycle over a child's whole life

`TOS.RUN.PROCESS.LIFECYCLE reserved=0 first=0 still_held=0 second=-6 again=0
released=0 stale=-1 returned=0`

| Field | Claim |
|---|---|
| `first` | a child funded from a reserved authority, and endowed a name for that *same* authority — two names, one budget (ADR-0076 §2b), because its creator said so |
| `still_held` | the creation placed a charge and did **not** consume the capability: the creator can still spend through it |
| `second` | and cannot fund a second child, because the first one's bytes are spent rather than promised |
| `again` | the child ends, is retired, and the same request works again: the exact charge came back to the node that paid |
| `released` / `stale` | the creator lets go of its **last** name for that node while a child it funded still runs; the node stops being nameable |
| `returned` | when that child ends, the bytes travel up the lineage *past* the node nothing names, so the parent authority can reserve the same amount again |

In one sentence: **process funding is an allocation held by the accounting, not
by the continued existence of the funding capability.** Nothing is inherited —
a child receives no name for the node that paid for it unless its creator names
that capability in the endowment like any other.

Failure after the charge is the existing creation transaction, unchanged and
still green: `creation-rollback.sh` drives fifteen named failure points and
compares the pool, the reserve, the authority tree, the charge ledger, the
process table and the capability tables on both sides of each.

## 6. Retirement, and what went with it

Operations 8 and 15 answer `E_NOT_SUPPORTED` forever, **refused before their
arguments are read** — a refusal that first resolved a handle would be reporting
on authority for an operation that is not there to need it. Their numbers stay
assigned and are never reused (`SYSTEM_ABI_V1` §7).

`TOS.RUN.PROCESS.RETIRED create=-7 with_generation=-7`, asked by a process
holding the very authority they used to require, so the answer is about the
operation rather than about the caller.

**The source-level `process_create` went with them.** It bound to operation 8,
and operation 19 cannot be declared in `SYSTEM_INTERFACE_V1` as it stands: two
capabilities, an explicit grant, an explicit endowment, and a *capability*
result where every declared result is `i64`. A schema advertising an operation
the ABI refuses would be worse than one that says plainly it does not carry
this yet.

`process-launch.sh` is now the withdrawal gate. The fixture still declares the
operation, and what is asserted is that it is refused at the **boundary check**,
before its first instruction, with the reason naming exactly what is wrong:

```text
TOS.RUN.DIAGNOSTIC E1801_FFI_NOT_AVAILABLE … item=process_create
                   reason=the interface declares no operation of this name
TOS.RUN.REFUSED stage=check count=1
```

`TOS.RUN.REQUEST` does not appear at all — the check runs before the endowment
is handed over, so a module naming an operation the schema does not declare
never reaches the point where its capabilities would be answered. No module
executed and the boot reported the failure rather than halting as though the
work had been done.

**What is no longer proved, said plainly.** `supervisor-text.sh` used to show a
textual supervisor *starting* the services a textual policy named. It now shows
a textual supervisor *reading* that policy and reaching a real capability
contract once per entry, with a value neither module contains. Creation from
source is not possible for anybody until the typed bridge exists, and that is a
decision rather than an omission: `docs/evidence/STAGE3_CLOSURE_DECISIONS.md`
§A.

## 7. Operation 20, and the trust boundary it does not cross

Three capabilities: process authority with `create`, a `MemoryAuthority` with
`spend`, and a **shared** region with `read`. An immutable *affine* region is
refused — a target gets a window of its own and its creator keeps one, which is
two holders, and `share` (7) is the operation that makes a region able to be in
two places.

There is no module path, no ordinal and no entry. ADR-0076 is explicit that the
bundle declares its own entry, and a caller-supplied one would be a second truth
about which program this is.

**Ring 0 reads none of it.** The nucleus checks capability and lifecycle facts —
the object is live, it is shared, it has a whole-frame length, the target can be
given its own name and window — and not one byte of the artifact. It does not
parse the format, inspect the entry, verify an image or trust a receipt.

`Launch` v5 is a **second record shape**, not a second reading of one shape. The
runtime reads the discriminator first and nothing else until it knows which
record it holds; a record read as the wrong shape is not a record with wrong
values, it is a set of pointers into whatever happened to be laid out there. The
bundle form carries no unit table, no caller entry and no receipt, and the price
of a creation uses the record the selected form actually needs — charging the
source form's while writing the bundle form's would be a frame outside the
authority tree.

The bundle capability is granted by operation 20 itself and not through
`Endowment::Existing`, which correctly refuses regions: that path copies a
handle and cannot build the mapping a region needs in an address space that does
not exist yet.

## 8. Evidence — `bundle-launch.sh`

One supervisor builds a real `TOSBUNDLE/v1` over this boot's own closure into a
region it allocated, freezes it, shares it, and creates targets from it.

```text
BUNDLE.WRITTEN   bytes=1147 modules=1
BUNDLE.SHARED    allocate=0 freeze=0 share=0
BUNDLE.TARGETS   not_shared=-1 unheld=-2 first=0 second=0 distinct=1
                 kept=0x4c444e42534f54 collected=0/0
BUNDLE.PARSED    modules=1 entry_position=0 entry_path=system/boot/init.tos   (×2)
BUNDLE.HOSTILE   shared=0 created=0
BUNDLE.REFUSED   stage=parse reason=bundle-bad-magic
```

| Claim | Evidence |
|---|---|
| the artifact goes through the whole state machine | allocated mutable, written, frozen, shared — all `0` |
| an affine region is not a shared one | `not_shared=-1`, asked **before** anything is shared |
| a handle nobody holds names nothing | `unheld=-2` |
| two targets from one capability over one backing | `first=0`, `second=0`, `distinct=1` — no rebuild, no refreeze, no copy |
| the supervisor keeps everything | `kept=0x4c444e42534f54` — its own window still reads `"TOSBNDL"` after both |
| the target admits the artifact itself | two `BUNDLE.PARSED`, two `TOS.RUN.VERIFIED`, two `TOS.RUN.COMPLETED` |
| the entry is the bundle's | `entry_path` comes from the artifact; the creator supplied none |
| **a corrupt bundle creates a process that refuses itself** | `hostile created=0`, then `BUNDLE.REFUSED stage=parse` — creation succeeding and admission failing are two outcomes of two components |
| and everything came back | pool to the root's frame count, reserve to its baseline |

The corrupt artifact is the valid one with **one byte of the magic flipped**:
the region is legal in every way a nucleus can check, and what is wrong is the
one thing it never looks at.

**One implementation addition to `tos-bundle`, and no format change.**
`Bundle::parse_prefix` reads the declared total, bounds it by what was actually
handed over, and then runs `Bundle::parse` over that prefix. A region is a
container and an artifact is a prefix of one — a bundle arrives in whole frames
because that is what memory is handed out in. No field was added and no byte of
`TOSBUNDLE/v1` changed; what was added is a way to *read* one out of a
container, which the format already describes and had no entry point for.

## 9. The physical account

```text
1452 total reserve      1451 runtime baseline      1 permanent backing root
```

Unchanged. Operation 20's target mapping is an ordinary region mapping, already
covered by `process_region_mapping_frames = 163`. No legal topology in this
round needs frame 1453.

**The margin on four ordinary processes is now zero**, and that is worth stating
rather than passing over: four ordinary processes need 57 424 frames and the
root holds 57 424. It was 4 two commits ago. `admitted_frames` falls as the boot
artifacts grow, and this round's production runtime image grew by the
bundle-target launch path.

What recovered it was not shaving: **every evidence-only workload moved behind a
feature**, so the image a canonical boot runs no longer carries the supervision
probes, the region-state probes or the funding-lifecycle probes. None of them is
reachable on a canonical boot — `system.boot.init` is endowed nothing, so no
process ever holds the authority they need — and code a canonical boot carries
and cannot reach is memory every process pays for. The gates that drive them now
build their own image, as the region gates already did.

The same reasoning applies one level down. `lifecycle.sh`'s children are given a
16 MiB arena rather than the 54 MiB runtime policy, named by their creator —
which is exactly the flexibility operation 19 introduced. Their measured peak is
under 128 KiB; what that change buys is that the arrangement is about the
lifecycle rather than about arena size, on a platform that funds four 54 MiB
processes and not one frame more.

Any further growth of the nucleus or the production runtime image takes that
margin negative. The next thing that does should be paid for rather than
absorbed — see `STAGE3_CLOSURE_DECISIONS.md` §B.

## 10. Environment-only failures

`bash scripts/preflight.sh` — 36 of 36.

`bash scripts/preflight.sh --profile qemu` — the two that do not run are
`stage3-observer-conformance` and `stage3-ipc-conformance`, both exiting 2 with
`the selected QEMU has no ADR-0066 observer-build.json` **before booting
anything**. ADR-0066 fixes the measurement boundary at one qualified external
observer, and the gates check for its provenance manifest beside the
`qemu-system-x86_64` on `PATH`. This host has the distribution's QEMU. Neither
gate reaches a boot, so neither executes a creation, a region or a bundle; the
refusal is a property of the machine and would be identical at any commit.
