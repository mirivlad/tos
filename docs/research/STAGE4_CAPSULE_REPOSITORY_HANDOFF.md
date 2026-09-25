<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 4 — capsule-to-repository handoff: the boundary before the decision

- Status: **note, not a decision.** Tier 4 under `docs/38`: research and
  explanatory material, incorporated by no ADR. Nothing here is accepted, and
  nothing here is implemented.
- Date: 2026-09-25
- Audience: the Project Architect, before any repository implementation is begun
  and before the decision surface in §15 is ruled on.
- Scope: the last `docs/16` Stage 4 deliverable before the performance report.
  `docs/08_GIT_NATIVE_SYSTEM.md` already exists and is not restated here; this
  note is about **where Stage 4 stops**.

## 0. What this is, and the one question it answers

> **What is the smallest honest meaning of "capsule-to-repository handoff" that
> Stage 4 must prove, without implementing Stage 5 early?**

**It recommends without deciding.** Where an answer is forced by an accepted
clause, the clause is cited. Where it is a choice, the alternatives are given
with a recommendation and the reasons on both sides. Four places are **gaps in
the accepted text** rather than choices this note may make; they are named as
gaps in §7 and §15 instead of being filled in by intuition.

**One finding shapes everything else.** The capsule header already carries the
**raw Git commit object id** of the source it was built from (`ADR-0016`,
`CAPSULE_FORMAT_V1` §6), the Boot ABI already copies it to the nucleus
(`BOOT_ABI_V1` §6), and the nucleus already reports it
(`TOS.IDENTITY source_kind=git source_digest=…`). So the entry point into a
repository is **already in the machine**, verified, before any repository exists.
That removes refs, ref storage, ref ambiguity and misleading-ref threats from the
Stage-4 slice entirely, and it makes the strongest honest Stage-4 claim available
without inventing any new identity mechanism.

## 1. Current facts from accepted contracts

Only clauses, with citations. Nothing inferred.

### 1a. What the stage plan actually says

| Clause | Text |
|---|---|
| `docs/16` Stage 4 deliverables | *"persistent object/state storage; **capsule-to-repository handoff**; crash/reset and adversarial-device tests; Stage 4 performance contract report"* |
| `docs/16` Stage 4 engineering exit | *"persistent storage works through a textual user-space driver"* |
| `docs/16` Stage 4 identity exit | *"the textual driver performs actual I/O from canonical source; no binary shadow driver or hidden host path exists"* |
| `docs/16` Stage 5 deliverables | *"declared compatibility profile at least G2; bounded object store and commit/tree/blob traversal; immutable `/system` mount by commit; writable source overlay; status, diff, commit and branch services; protected refs and transition audit; candidate/last-known-good activation and rollback"* |
| `docs/16` Stage 5 identity exit | *"commit tree is the installed `/system`, not metadata around another package/image authority"* |

**Stage 4's own identity exit says nothing about a repository.** `docs/37` Stage 4
asks *"does a canonical textual user-space driver actually move persistent data
through final-style MMIO/interrupt/DMA/IPC boundaries?"* and lists driver
evidence only. Repository identity is `docs/37` Stage 5's question.

### 1b. Where the repository work is staged

`docs/08` §Work decomposition, verbatim:

```text
1. G0 source identity in capsule/runtime provenance;
2. G1 bounded loose-object reading;
3. G2 deterministic local object writing and protected refs;
4. G3 packed-object reading;
5. G4 remote transport;
6. G5 history manipulation;
7. G6 maintenance and scale.
```

`docs/08` §Compatibility is profiled: *"Stage 1 requires G0 provenance identity.
Stage 5 requires at least G2 deterministic local history."* `docs/36` G2 is
*"Required by: Stage 5 exit gate."* **G1 is assigned to no stage by name.** It
sits between a Stage-1 requirement and a Stage-5 requirement, and the only
deliverable between them that could carry it is this one.

### 1c. What G1 is, and what it excludes

`docs/36` G1 capabilities, verbatim: *"parse and verify the selected
loose-object profile; read blob, tree and commit objects; traverse a tree through
a bounded object-store interface; read explicitly supported refs; reject
unsupported algorithms, malformed objects and ambiguous names; compare results
against independent Git test oracles."* Excluded: *"object writing; packfiles;
network protocols; merge/diff semantics; garbage collection."*

`docs/36` §Purpose: *"The profiles are cumulative unless an ADR explicitly
defines a specialized profile."*

### 1d. Where the handoff is named outside `docs/16`

| Clause | Text |
|---|---|
| `docs/04` boot phase 3 | *"The nucleus executes `/system/boot/init.tos` from the capsule. This component launches boot-critical driver services, **discovers repository storage, verifies the selected commit, and transitions to the repository-backed system tree**."* |
| `docs/04` boot phase 4 | *"The repository-backed `/system/boot/init.tos` takes over. **It may differ from the capsule copy only through a defined handoff protocol.**"* |
| `docs/11` §Bootstrapping, step 5 | *"Repository-backed versions replace capsule versions through a versioned handoff."* (step 4 is *"Text driver initializes persistent storage"*) |
| `docs/45` §`boot/` | *"The capsule copy of `/system/boot/init.tos` and the repository-backed copy are related through the handoff protocol defined there; **the capsule remains a transport and recovery seed, never a second installed system.**"* |

**These describe a protocol whose completion is Stage 5's exit.** "Transitions to
the repository-backed system tree" and "repository-backed versions **replace**
capsule versions" are the same statement as `docs/16` Stage 5's *"immutable
`/system` mount by commit"* and `docs/37` Stage 5's *"commit tree is the installed
`/system`"*. Stage 4 therefore owes a **part** of the handoff protocol, not its
completion — §3 says which part.

### 1e. What forbids the strongest reading at Stage 4

| Clause | Text |
|---|---|
| `PROCESS_IDENTITY_V1` §5 | *"Stage 3 has no repository… its **system commit id is absent, not guessed**. Writing the capsule's build commit there would report a commit the system never read… **Stage 5** replaces the capsule source set with the selected commit's tree, and the field becomes present. Until then, absence is the true value."* |
| `PROCESS_IDENTITY_V1` §7.5 | *"The system commit id is absent for every capsule-launched Stage 3 process — **asserted by a test, so that a later stage cannot make it present by accident**."* |
| `docs/34` X3.9 | *"A commit identity the system never read… Controls: the system commit id is **absent** for capsule-launched processes and is asserted absent by test, not left to convention."* |

So **Stage 4 must not make the running system's identity a repository identity.**
That is not a preference; it is a named threat with a named control.

### 1f. Where the trust boundary is drawn

| Clause | Text |
|---|---|
| I-02 | the permanently binary trusted base contains only what is required to *"start the machine, isolate execution, access the boot capsule, expose primitive hardware mechanisms, **verify repository state**, and launch textual components"* — and *"features must not move into the nucleus merely for convenience"* |
| `docs/08` §Nucleus versus userspace | the nucleus implements *"only mechanisms **required for trusted boot and immutable mounting at the selected profile**"*: content-ID parsing, object-integrity verification, bounded commit/tree traversal, reference selection from boot control, immutable tree exposure, protected transactional ref primitives |
| `docs/36` §Nucleus boundary | *"The nucleus implements only the minimum bounded reading/verification mechanism **justified by the active stage**."* |
| `docs/34` X3.10 | privileged policy migrating into the nucleus because IPC is inconvenient is a named failure |
| I-08 | *"Drivers run as isolated services unless operation before process isolation is technically unavoidable."* |

### 1g. Where the threats and the budgets live

`docs/34` §Stage mapping: *"Stage 4: interrupt, MMIO, DMA and **storage-corruption**
threats; Stage 5: **repository, refs, protected candidate/current/last-known-good/
recovery selection, rollback, garbage collection and state migration threats**."*
And: *"A stage cannot close if its new boundary lacks a threat entry, negative
tests and stated evidence level."*

`docs/35` gives Stage 4 the block-driver budgets and Stage 5 *"Repository and
activation"* budgets (commit-root resolution, candidate switch, `status`,
rollback). **No repository budget is assigned to Stage 4.**

### 1h. What the storage contract already says about this deliverable

`STATE_STORE_V1` §4, verbatim: *"This contract gives meaning to sectors `0..64`
and to no others… It assigns no meaning and no owner to the remaining sectors.
They are not free, not reserved and not this store's: they are undecided. **The
capsule-to-repository handoff is a separate decision and must not overlap this
extent without explicitly revisiting the layout.**"* And: *"There are no
partitions at Stage 4 and **no partition abstraction is introduced**; a
base-offset field would have exactly one possible value, so there is none."*

### 1i. What identity the capsule carries

`CAPSULE_FORMAT_V1` §6 and `ADR-0016`: for `source_identity_kind = 1` (git),
the header holds `source_oid_alg` (1 = SHA-1, 2 = SHA-256), `source_oid_length`
(20 or 32) and `source_identity_value` = **the raw commit object id**, stored
directly *"so a capsule can be resolved back to its commit with
`git show <oid>`"*. For kind 2 (detached source set) it holds the ADR-0018
digest over the canonical path/digest sequence. `CAPSULE_FORMAT_V1` §4.2 gives
every file a validated 32-byte SHA-256 content digest and a canonical path.

`CAPSULE_PROVENANCE_V1` §3 already asserts, host-side, that *"a Git-mode
`repository_path` names a blob in `source_commit` with the same bytes"* — but §1
is explicit that it is *"a sidecar only: a UEFI loader and nucleus MUST NOT
consume it"*. **The linkage exists today as a host claim and has never been
checked by TOS.** That gap is exactly what this deliverable is for.

## 2. Current implementation facts

Read from code and from a real boot, not from prose.

1. **The boot path.** UEFI loader → nucleus (`source/nucleus/src/main.rs`) →
   ring-3 runtime image → canonical `/system/boot/init.tos` **from the capsule**.
   `source/system/` in the repository contains exactly one module today:
   `source/system/boot/init.tos`.
2. **The capsule identity is real and is Git.** `run.sh` builds the boot capsule
   with `--git-commit HEAD` over `source/system/boot/init.tos`, and
   `check-capsule-provenance.py` binds capsule bytes, Git blobs and the retained
   notice before the ESP is made. The nucleus re-derives the kind from the parsed
   header and emits `TOS.IDENTITY source_kind=git source_digest=<64 hex>
   capsule_digest=<64 hex>`.
3. **This repository is SHA-1.** `git rev-parse --show-object-format` → `sha1`.
   So every capsule this tree can build carries `(alg = 1, length = 20)`, and a
   reader that verified only SHA-256 object ids could not read this repository's
   objects at all.
4. **The object chain for the one thing the capsule carries is tiny and bounded.**
   At `ccc10e2`:

   | object | id | uncompressed bytes | entries |
   |---|---|---|---|
   | commit | `ccc10e2…` | 6724 | — |
   | root tree | `5b48241…` | 1159 | 29 |
   | `source` tree | `9dcf88a…` | 668 | 17 |
   | `source/system` tree | `a9ae3f2…` | 31 | 1 |
   | `source/system/boot` tree | `a196e18…` | 75 | 2 |
   | `init.tos` blob | `575502a…` | 2047 | — |

   Six objects, tree depth 4, 10 704 bytes in total — 21 sectors. **The commit is
   the largest object and it is large only because of a long commit message**; a
   bound must be a declared bound, not this measurement.
5. **A loose Git object is a zlib stream.** `zlib(<type> SP <size> NUL payload)`,
   and the object id is over the **uncompressed** `<header || payload>`. Measured
   in this repository: with `core.looseCompression 0`, Git writes the stream as
   `78 01` followed by a **stored** deflate block (`BFINAL = 1`, `BTYPE = 0`) and
   `git cat-file` reads it back normally. So a stored-block-only inflater is a
   real, ordinary-Git-readable subset — see §7c.
6. **Canonical text has no hash and no decompressor.** `SYSTEM_INTERFACE_V1` has
   no digest operation and no inflate operation, and no interface exposes one.
   `tos-hash` is a Rust crate inside the binary base, reachable by the nucleus and
   the runtime image and **not** by a module. A textual repository reader must
   therefore implement its hash and its inflater **in TOS Core**. The language has
   `&`, `|`, `^` and checked shifts (`parser.rs`; `docs/40` §Evaluation makes
   integer arithmetic and shifts checked operations with a stable trap), so both
   are expressible; neither exists.
7. **The device and what is on it.** One VirtIO block function on a 16 MiB image =
   32 768 sectors. `state.store.v1` owns sectors 0–64. Sectors 1–4 are seeded by
   the harness with 512 copies of `0xC0 + n`. **Sectors 65–32767 are undecided
   territory** — 32 703 sectors, 15.97 MiB.
8. **Nothing persists across boots today.** `run.sh` does `rm -f "$STAGE4_IMAGE"`
   and re-creates the image for every invocation, and each gate passes its own
   `--out` directory. `block-lifecycle` proves data survives a **service**
   restart inside one boot; `state-store` proves it survives a service
   **generation** inside one boot. **No gate has ever demonstrated that a byte
   survives a reboot**, because the harness has no option to retain the image.
9. **`block.device.v1` is one sector per request.** §10: *"no more than one sector
   per request; no batching; no multiple outstanding requests from one client"*.
   So the 6724-byte commit above is **14 separate `READ` exchanges**, each
   answered with its own immutable region.
10. **The bounds a repository reader has to fit inside.** `MAX_PROCESSES = 4`,
    `MAX_ENDPOINTS = 6`, `MAX_ENDOWMENT = 4`, `MAX_PLANS = 4`,
    `MAX_CAPABILITIES = 16`, `QUEUE_DEPTH = 4`, `MAX_INLINE_BYTES = 256`,
    `MAX_TRANSFERRED_REGIONS = 2`. The `state-store` fixture already uses **five
    of six endpoints** and peaks at **four of four processes**.
11. **A canonical service serves a constant number of requests.** `block.tos`,
    `state.tos` and every other fixture service loop to a compile-time count,
    because a child is told nothing at launch but its endowment. Any repository
    fixture inherits that arithmetic.
12. **The system commit id does not exist in the implementation at all.**
    `PROCESS_IDENTITY_V1` §3 lists the field; no launch record carries it.
    Absence is the implementation, not a convention.

## 3. The exact Stage-4 question, answered

### 3a. What exists before the handoff

```text
BEFORE
──────────────────────────────────────────────────────────────────────────────
  ESP (host-provisioned)
    ├─ loader            binary, verified by firmware
    ├─ nucleus           binary, digest in the handoff record
    ├─ runtime image     binary, digest re-verified by the nucleus
    └─ capsule           ┌─────────────────────────────────────────────┐
                         │ whole_capsule_digest      SHA-256, verified │
                         │ source_identity_kind = 1  git               │
                         │ source_oid_alg = 1        SHA-1             │
                         │ source_identity_value     RAW COMMIT OID ●  │
                         │ file table: /system/boot/init.tos           │
                         │             + SHA-256 content digest        │
                         │ bytes:      the module's exact source text  │
                         └─────────────────────────────────────────────┘

  running system
    source set            "git:<64 hex of the OID field>"   (nucleus-asserted)
    system commit id      ABSENT                            (asserted by test)
    /system               does not exist as a mount; modules come from the capsule

  block device (32 768 sectors)
    sectors 0..64         state.store.v1          owned, formatted, in use
    sectors 65..32767     UNDECIDED               no meaning, no owner
```

The `●` is the whole point: **the commit id is already in the machine, already
verified, and already reported** — and the system has never read the commit.

### 3b. What must exist after the handoff for Stage 4 to have fulfilled it

```text
AFTER  (the Stage-4 claim, and not one byte more)
──────────────────────────────────────────────────────────────────────────────
  block device
    sectors 0..64         state.store.v1          unchanged, still owned
    sectors R..R+n        repository extent       bounded, reserved by contract
                          ┌──────────────────────────────────────────────┐
                          │ a bounded set of Git objects, locatable by   │
                          │ object id, each verifiable against its id    │
                          └──────────────────────────────────────────────┘
    everything else       still undecided

  a canonical textual process (no PCI, MMIO, IRQ or DMA authority; reaches the
  device only through block.device.v1) has, from inside TOS:

    1. taken the commit OID the capsule carried            ● from §3a
    2. located that object in the repository extent
    3. verified it: recomputed its object id over its own uncompressed bytes
    4. walked commit -> root tree -> source -> system -> boot -> init.tos
       verifying every object the same way
    5. compared the blob's bytes with the capsule file's bytes  BYTE-IDENTICAL
    6. refused, by exact code, a malformed object, a missing object, a
       substituted object and an out-of-extent address

  running system
    source set            UNCHANGED   still the capsule's
    system commit id      STILL ABSENT
    /system               still not a mount; modules still come from the capsule
```

### 3c. What exact authority or identity changes hands

**Authority: none.** No capability moves, no process gains a right it did not
have, and the nucleus gains nothing. The repository reader is one more client of
`block.device.v1` under `ADR-0093`/`ADR-0095`, holding no part of the machine.

**Identity: nothing changes hands, and something becomes *checkable*.** Before,
the sentence *"the capsule carries the source of commit X"* was a **host claim**
in a sidecar the loader and nucleus are forbidden to read
(`CAPSULE_PROVENANCE_V1` §1). After, it is a sentence **TOS itself has
verified**, from bytes on a device it drove, against an object id it recomputed.
The installed system's identity does not move — that move is Stage 5's exit.

### 3d. So which of the four is it?

| Candidate reading | Verdict |
|---|---|
| copying capsule source **into** repository objects | **no.** That is deterministic object *writing*, which `docs/36` puts in **G2** and assigns to Stage 5. It also needs deflate and crash-safe publication (§9B) |
| **reading a repository provisioned by something else** | **yes, this is the operation** |
| **associating capsule identity with repository identity** | **yes, this is the claim** the reading exists to establish |
| selecting repository objects for later boot | **no.** That is *"reference selection from boot control"* and candidate/current semantics — `docs/36` G2, `docs/16` Stage 5 |
| something else | no |

**The honest name.** What Stage 4 can prove is not a handoff; it is the
**first half** of one. Nothing is handed over: the capsule stays the installed
system and the repository stays inert. What is established is that the repository
on the device **contains, names and can serve** the same source the capsule
carried, and that TOS can read and verify it without a host. A precise name for
the Stage-4 deliverable is **capsule-to-repository linkage**, with the
*replacement* half — `docs/04`'s *"transitions to the repository-backed system
tree"* — remaining Stage 5's. Whether to rename the `docs/16` line or to keep the
name and narrow its meaning is §15's first decision; this note does not rename
anything.

## 4. Explicit non-goals

Not proposed for this slice, each with the clause that puts it elsewhere:

| Not this slice | Where it belongs |
|---|---|
| writable `/system` | `docs/09` §`/system` — *"read-only"*; `docs/16` Stage 5 |
| `/work/system` overlay | `docs/16` Stage 5 *"writable source overlay"* |
| status, diff, commit | `docs/36` G2; `docs/16` Stage 5; `docs/08` §Nucleus versus userspace puts them in textual services |
| branches | `docs/36` G2 |
| protected candidate / current / LKG activation | `docs/36` G2; `docs/16` Stage 5 |
| rollback | `docs/16` Stage 5; I-05 |
| repository mutation, of any kind | `docs/36` G2 |
| ordinary local commit creation | `docs/36` G2 |
| packfiles, pack index, delta chains | `docs/36` G3 |
| remotes, fetch, push, clone | `docs/36` G4 |
| shell integration | `docs/16` Stage 4E and Stage 6 |
| immutable `/system` mount | `docs/16` Stage 5; `docs/37` Stage 5 identity exit |
| garbage collection, retention | `docs/36` G6 |
| refs, of any kind | **not needed at all** — §7d |
| the system commit id becoming present | **forbidden** — `PROCESS_IDENTITY_V1` §5, §7.5; `docs/34` X3.9 |
| the Stage 4 performance contract report | a separate `docs/16` deliverable |
| closing Stage 4C, Stage 4D or Stage 4 | not this note's |

**No accepted clause forces any of them into Stage 4.** The one that comes
closest is `docs/04` boot phase 3 — *"discovers repository storage, verifies the
selected commit, and transitions to the repository-backed system tree"* — and it
is a description of the **finished** boot sequence, whose last clause is
word-for-word Stage 5's exit condition. Reading it as a Stage-4 obligation would
make Stage 4 deliver Stage 5.

## 5. The before/after model

§3a and §3b are the diagram. Three statements make it precise:

1. **The arrow is one-directional and it is read-only.** Capsule → repository is
   a *comparison*, not a copy. Nothing in the slice writes a repository object.
2. **The two identities meet at bytes.** The capsule's file table has a validated
   SHA-256 content digest per path; the repository's blob has a Git object id over
   the same bytes with a different framing and a different algorithm. The linkage
   is not "two hashes agree" — they cannot, being different functions of
   different inputs. It is **"both name the same byte string, and TOS checked
   both"**.
3. **The capsule remains the installed system throughout.** `docs/45`: *"the
   capsule remains a transport and recovery seed, never a second installed
   system."* After this slice it is still the installed system, and the
   repository is a verified second description of it.

## 6. Storage-placement alternatives

`STATE_STORE_V1` §4 requires an explicit decision here: sectors 65+ are
*"undecided"* and the handoff *"must not overlap this extent without explicitly
revisiting the layout."* **Simply starting to write at sector 65 because it is
unused is exactly what that clause forbids.**

Answers to the placement questions:

- **Does repository storage share the same block device?** At Stage 4 there is
  one device and `ADR-0093` gives Stage 4 *"one interface, one publisher, one
  lookup"* with *"no enumeration API, multi-device registry, metadata framework,
  discovery protocol"*. So yes, unless a second device is introduced — which
  pulls in exactly the machinery `ADR-0093` refused (option C below).
- **What survives a service restart?** The sectors. Nothing else: the store
  proves that a successor reads the device and is handed nothing in memory, and
  the repository reader would be held to the same rule.
- **Who owns the lower block endpoint?** `ADR-0093`/`ADR-0095`: the block service
  publishes `block.device.v1` and a client holds a `send | call` name for it. The
  repository reader is such a client. It holds **no** PCI, MMIO, IRQ or DMA
  authority, exactly as the state store does not.

### A — one more explicitly reserved bounded raw extent (recommended)

A contract names a first sector `R` and a sector count `n`; the repository extent
is `[R, R+n)` and the reader **never** addresses a sector outside it. `R > 64` by
construction, stated as a constant in canonical text and in the contract.

For: it is the same shape `STATE_STORE_V1` already uses and already proved — a
bounded extent with a header, a validated format, and a service that refuses to
address outside it. Overlap is prevented the way `STATE_STORE_V1` prevents
running off its own end: by a constant the service checks, with the counted
device requests of the gate as the evidence. It introduces no abstraction that
does not exist.

Against: two constants in two contracts must not drift. That is answerable by
making the boundary a single stated fact — *"the state store owns `[0, 65)`, the
repository extent begins at `R ≥ 65`"* — asserted in both contracts and checked
by a documentation gate, which is how this repository already keeps operation
numbers and bounds from drifting.

### B — a partition table

For: it is the familiar answer, and it would make the extents self-describing.

Against: `STATE_STORE_V1` §4 already refused a base-offset field *"because it
would have exactly one possible value"*, and a partition table at Stage 4 would
have exactly two entries whose values are constants. It is a persistent format,
a parser, a threat surface (a crafted table) and a new abstraction, bought to
express two numbers that are already constants. **Gratuitous by the same
argument the accepted contract already made.** If a third owner ever appears with
a genuine need to relocate, a table becomes a real decision then.

### C — a second block device

For: overlap becomes impossible rather than forbidden.

Against: it needs a second PCI function claimed, a second driver instance or a
driver serving two functions, and a way for a client to ask for *which* device —
which is enumeration, multi-device registry and discovery, all three named by
`ADR-0093` §3a as what Stage 4 does **not** get. It buys a property that a
constant already buys.

### D — repository objects inside `state.store.v1`

**Refused by the accepted contract, not merely unattractive.**
`STATE_STORE_V1` §5: *"An object is exactly `OBJECT_BYTES` of opaque payload, and
there is no per-object header."* `OBJECT_BYTES` is 512 and `MAX_ID` is 64, so the
store can hold 64 objects of exactly 512 bytes — it cannot hold the 6724-byte
commit of §2.4 at all, and it would make the store's `schema_identifier` the
authority over repository content. It also inverts the layering: the repository
would become a client of a service whose whole purpose is that it interprets no
payload byte.

**Recommendation: A**, with the boundary stated once and asserted in both
contracts.

## 7. The G1/G2 boundary, and what G1 must contain here

### 7a. Can Stage 4 stop at G1?

**Yes, and it must.** `docs/36` assigns G2 to the Stage 5 exit gate; every G2
capability — deterministic creation, branches, overlay status, commit creation,
candidate/current/LKG, reflog, crash-safe publication — is a `docs/16` Stage 5
deliverable. Nothing in Stage 4's deliverable list, engineering exit or identity
exit requires writing an object or moving a ref. **Stage 4 stops at reading.**

### 7b. But not at all of G1 either

Of G1's six capabilities, this slice needs four:

| G1 capability | Stage 4 |
|---|---|
| parse and verify the selected loose-object profile | **yes** |
| read blob, tree and commit objects | **yes** — and not tag |
| traverse a tree through a bounded object-store interface | **yes** |
| reject unsupported algorithms, malformed objects and ambiguous names | **yes** |
| read explicitly supported refs | **no** — §7d |
| compare results against independent Git test oracles | **yes**, host-side, as `docs/08` §External implementations permits |

`docs/36` §Purpose: *"The profiles are cumulative unless an ADR explicitly
defines a specialized profile."* A four-of-six subset is therefore **a
specialized profile and requires an ADR** — §15.

### 7c. What the profile must fix, item by item

The directive's list, answered or named as a gap.

1. **Object hash family — UNDECIDED, and this is a gap.** This repository is
   SHA-1 (§2.3), so every capsule it builds carries `alg = 1`. `docs/36`'s
   allowed-claim example is *"G1 for loose SHA-256 repositories"*. There is **no
   clause anywhere** in the accepted text choosing a family for repository
   objects, and `docs/19` R3 lists *"exact initial object/hash/ref profile"* as
   an open question. Three sub-options, none of which this note may pick:
   (i) verify SHA-1, matching the repository that exists — and state honestly
   that a recomputed SHA-1 is integrity against corruption, not against a
   motivated adversary (`docs/34` T4 names *"hash collisions where feasible"*);
   (ii) convert this repository to Git's SHA-256 object format, which changes
   every existing capsule identity and every provenance record;
   (iii) verify whichever family the capsule header's `source_oid_alg` declares,
   which is self-describing and matches `ADR-0016`'s design, at the cost of two
   hash implementations in canonical text.
2. **Loose-object representation.** `zlib(<type> SP <size> NUL payload)`, the id
   over the uncompressed `<header || payload>`. The real choice is how much of
   deflate the reader implements:
   (i) **full inflate** — reads any Git loose object, and brings Huffman
   decoding, dynamic tables and a decompression-bomb surface into canonical text;
   (ii) **stored blocks only** — measured working in §2.5: `core.looseCompression
   0` produces `BFINAL = 1, BTYPE = 0` and ordinary `git cat-file` reads it back.
   A reader needs the 2-byte header, `LEN/NLEN` framing, the Adler-32 trailer and
   nothing else. It constrains the *producer*, so TOS could not read an
   arbitrary repository — which at Stage 4 it never needs to, because the extent
   is provisioned for it (§9).
   (ii) is the smaller honest surface and the one this note would recommend,
   **stated as a declared profile restriction** rather than pretended to be
   general G1.
3. **Supported kinds.** `commit`, `tree`, `blob`. Not `tag`: nothing in the chain
   needs one, and accepting a kind the slice does not traverse is surface
   without a claim.
4. **Bounds.** Declared, not measured: maximum object bytes, maximum objects
   traversed per lookup, maximum tree depth, maximum entries per tree, maximum
   path length, maximum inflate output per object. §2.4's six objects / depth 4 /
   10 704 bytes is what today needs, and a bound set to today's measurement is a
   bound that breaks on the next commit message.
5. **Integrity verification.** Recompute the object id over the object's own
   uncompressed bytes and compare with the id it was *located by*. This is the
   only check that makes a substituted object detectable, and it is the reason the
   hash family cannot be deferred.
6. **How an object is located — UNDECIDED, and this is the largest gap.**
   `docs/36` G1 says *"bounded loose-object reading"*, and "loose" in Git means
   `.git/objects/ab/cdef…` — **a filesystem path**. Stage 4 has no filesystem and
   will not get one (`BLOCK_DEVICE_V1` §10: *"no filesystem, partition, cache,
   VFS or object store"*). So the documented G1 presumes a naming mechanism this
   stage does not have, and the extent needs an explicit one: a fixed object
   table in the extent header (id → sector, length); or placement derived from
   the id; or a chain of object records scanned linearly. **Each is a persistent
   format.** This note deliberately does not choose: it is a Level-3 decision and
   §15's second.
7. **Malformed, missing, substituted.** Fail closed with distinct codes, never a
   fallback and never a partial answer: a bad zlib header, a bad `LEN/NLEN`
   complement, a header that is not `<kind> SP <decimal> NUL`, a declared size
   that disagrees with the payload, an id that does not match the bytes, an id
   not present in the extent, a sector outside the extent, a tree entry whose
   mode or name is not admissible, depth or count past the bound.
8. **Do refs exist at this point?** No — §7d.

### 7d. Why no refs, and why that is a finding rather than a shortcut

The entry point is the **capsule's own `source_identity_value`** (§1i, §2.2): a
raw commit object id that the loader verified, the Boot ABI carried, the nucleus
re-derived from validated bytes and the nucleus reported. A ref is a *name for an
object id*; Stage 4 already has the id, from a source whose integrity is already
part of the accepted boot chain.

Consequences: no ref storage format, no ref profile, no ambiguous-name rules, and
none of `docs/34` T4's *"misleading refs"* or `docs/08`'s protected-ref
mechanisms. Refs arrive with G2, where they are needed because there is then
something to select *between*.

**This is also the sharpest argument that the slice is honest.** The commit the
reader starts from is not one it chose; it is the one the capsule says it was
built from. A reader that could pick its own starting commit would be proving
that it can read *a* repository, not that it can read *this system's*.

## 8. Trust-boundary alternatives

### Which Stage-4 handoff operations must enter the nucleus?

**None.** For each nucleus responsibility `docs/08` lists, the clause that would
require it and why it does not fire at Stage 4:

| `docs/08` nucleus item | Required at Stage 4? |
|---|---|
| algorithm-qualified content-ID parsing | **no.** `docs/08` scopes the nucleus list to *"mechanisms required for trusted boot and immutable mounting at the selected profile"*. At Stage 4 the boot text is the capsule's and there is no mount, so no content-ID parsing is boot-critical |
| object-integrity verification | **no**, same clause. The verification here is a *claim about a device*, not a precondition of executing anything |
| bounded commit/tree traversal | **no**, same clause |
| reference selection from boot control | **no** — there are no refs (§7d) and no boot selection (§4) |
| immutable tree exposure | **no** — no mount at Stage 4 |
| protected transactional ref primitives | **no** — G2 |

And the clauses that push the other way: `docs/36` §Nucleus boundary (*"only the
minimum … justified by the active stage"*), I-02 (*"features must not move into
the nucleus merely for convenience"*), I-08, and `docs/34` X3.10.

**So the whole of this slice is canonical text.** That is not a preference chosen
for elegance; it is what the scoping clause produces when the stage has no
trusted-boot dependency on the repository. The moment a later stage makes a
repository object boot-critical — Stage 5's mount — the nucleus gets the minimum
that stage justifies, and the textual reader of this slice becomes the thing
whose behaviour the nucleus's minimum is checked against.

### The six operations the directive asks to keep apart

| Operation | Stage 4 | Where |
|---|---|---|
| repository object **storage** (bytes on a device) | **yes**, read-only, textual | this slice |
| repository object **parsing/verifying** | **yes**, textual | this slice |
| commit/tree **traversal** | **yes**, bounded, textual | this slice |
| **ref selection** | no | G2 / Stage 5 |
| immutable **`/system` exposure** | no | Stage 5 |
| repository **mutation** | no | G2 / Stage 5 |

## 9. Provisioning alternatives

### A — host-provisioned repository extent, read and verified by TOS (recommended)

A host tool — a sibling of `tos-capsule-tool`, with the same auditability — takes
the commit the capsule was built from, emits the bounded object set the slice
traverses into a disk image at the reserved extent, and the harness attaches that
image. TOS reads it, verifies it and compares it with what the capsule carries.

- **Does it deserve the word "handoff"?** Not on its own. It deserves
  *"linkage"* (§3d). What it does establish is the half of the handoff that is
  about the repository being **readable and verified by TOS**, which is the half
  Stage 4 can honestly own.
- **Trust implications.** The host writes bytes to a disk image exactly as it
  already writes the capsule to an ESP. What matters is that **after boot no host
  path participates**, which is `docs/37` Stage 4's own discipline for the block
  driver. `docs/08` §External implementations explicitly permits *"Command-line
  Git and libgit2 … as host-side oracles and tooling"*.
- **Hidden-host-path risk.** Real and answerable. The risk is not that the host
  wrote the bytes; it is a fixture that *appears* to read them while the answer
  comes from somewhere else. The controls are the ones Stage 4 already uses: the
  reader holds no device authority but `block.device.v1`, the counted device
  requests of the gate show the reads happened, and a substituted-object negative
  shows the verification is real.
- **Stage-5 machinery pulled in.** None: no writing, no refs, no mount.
- **Can it prove the Stage-4 identity requirement?** Yes — §10.

### B — a canonical textual provisioner writes the objects

A TOS process converts capsule content into repository objects and writes them
through `block.device.v1`.

- **Deserves "handoff"?** More than A does: it is a real transfer of content from
  one representation to another, inside TOS.
- **Against, decisively.** Writing a Git object deterministically is
  `docs/36` **G2** — *"deterministic blob/tree/commit creation"* and
  *"crash-safe local object publication"* — assigned to the Stage 5 exit gate. It
  also needs a **deflate compressor** in canonical text (not just an inflater),
  and it would have to construct tree and commit objects, which means encoding
  modes, names, sorting and parent links — i.e. most of what Stage 5 owes. It
  steals the stage boundary this note exists to protect.
- Keep it as **Stage 5's** provisioning story, where it is the right answer.

### C — the capsule or boot artifact carries the repository material

- **Deserves "handoff"?** No. If the repository arrives in the capsule, reading
  it is reading the capsule again: no device, no persistence, no second
  representation, nothing the block driver did. It would also make the capsule
  carry both a source set and a description of that source set, and push it
  toward `ADR-0021`'s bounds for no gain.
- **Refuse.**

**Recommendation: A**, with the provisioning tool held to `tos-capsule-tool`'s
standard — deterministic output, an independent checker, no timestamps or host
paths — and with the gate proving that the reading and the verifying happened
inside TOS.

## 10. Recommended minimal Stage-4 design

Stated as a recommendation, not a decision. Five parts.

1. **One reserved bounded raw extent** beginning above sector 64, with a
   validated header of its own, and a service that never addresses a sector
   outside it (§6A). The boundary is stated once and asserted in both contracts.
2. **A declared specialized read-only profile** — commit, tree and blob;
   one hash family; a stated loose-object encoding subset; an explicit
   object-location mechanism; declared bounds; distinct refusal codes; **no
   refs** (§7).
3. **All of it canonical text**, reaching the device only through
   `block.device.v1`, holding no PCI, MMIO, IRQ or DMA authority (§8).
4. **The starting object is the capsule's own commit id** — not chosen by the
   reader, not a ref, not a constant in the reader's text (§7d).
5. **The claim is linkage, not replacement.** The system commit id stays absent;
   `/system` is not mounted; the capsule stays the installed system (§1e, §3b).

**What it would prove, in one sentence.** *A canonical textual TOS process, with
no authority over the machine beyond a block-device endpoint, read the commit the
capsule says it was built from out of a bounded extent of a real device, verified
every object in the chain against its own object id, walked to the path the
capsule's file table names, and found the same bytes the capsule carries — and
refused, by exact code, a substituted object, a malformed object, a missing
object and an address outside its extent.*

## 11. Required conformance evidence

The smallest set that makes the claim real, each item with what it defends
against.

1. **Canonical text exercising the accepted boundary.** The reader is a module
   under the ordinary process and capability model, identified by source, holding
   `block.device.v1` and nothing of the machine. *Defends: `docs/37` Stage 4's
   "no hidden host path"; I-08.*
2. **Real persisted bytes through `block.device.v1`.** Counted device requests,
   derived and not observed, as `state-store.sh` counts them. *Defends: a fixture
   that appears to read a device.*
3. **Bounded traversal actually bounded.** The declared depth, object-count and
   byte bounds are asserted, and a negative proves a graph past the bound is
   refused rather than followed. *Defends: `docs/34` T4 recursion exhaustion.*
4. **Object integrity checked by recomputation**, not by trusting the location.
   *Defends: substitution.*
5. **A substituted-object negative.** Flip one byte of the blob in the extent:
   the id no longer matches, the read is refused by exact code, and the linkage
   claim **fails** rather than quietly succeeding. *Defends: the whole point.*
6. **A malformed-object negative and a missing-object negative**, by exact code.
7. **An out-of-extent negative.** A request for a sector below the boundary is
   refused **by the repository reader**, before it reaches the block service, and
   the counted device requests show it never did. *Defends: `STATE_STORE_V1` §4's
   non-overlap requirement — and this is the assertion that makes non-overlap a
   fact rather than an intention.*
8. **The state store is still there afterwards.** A `GET` of an object the state
   store holds still succeeds after the repository reads, in the same boot.
   *Defends: overlap, from the other side.*
9. **Identity linkage to something the capsule actually carried.** The blob bytes
   equal the capsule file's bytes; the starting commit id is the capsule header's
   `source_identity_value` and not a constant in the reader. *Defends: reading
   *a* repository instead of *this system's*.*
10. **A host oracle.** Ordinary `git` produced the extent and can read it back —
    `docs/36` G1's *"compare results against independent Git test oracles"* and
    `docs/08`'s host-oracle permission.
11. **No host shortcut after boot.** The capability topology read from the
    journal: which process requested which binding, as `state-store.sh` already
    asserts it.
12. **Explicitly not claimed**, in the gate's own voice: no writing, no refs, no
    mount, no `/system`, no activation, no rollback, no packfiles, no system
    commit id, and **no Stage 4 closure**.

**On restart and re-read.** If the claim includes *"the objects survive"*, the
honest scope must be stated. §2.8 is the constraint: **the harness re-creates the
disk image for every boot**, so a *reboot* claim needs a harness that retains the
image between two `run.sh` invocations — a harness capability that does not exist
and would be a new one. Within one boot, a second reader process re-reading the
extent after the first has ended is exactly the shape `state-store.sh` already
proves, and is available now. **Recommendation: claim the in-boot re-read, and
state plainly that across-boot persistence is unproved by any gate in this
repository** rather than implying it.

## 12. Bounds and resources

| Bound | Value | What it costs this slice |
|---|---|---|
| `MAX_PROCESSES` | 4 | `init + block service + reader` = 3, leaving one. The `state-store` fixture already peaks at 4 |
| `MAX_ENDPOINTS` | 6 | `block-serve` + a reader inbox = 2 if the slice runs alone; the `state-store` topology already uses 5 |
| `MAX_ENDOWMENT` | 4 per plan | a reader needs `budget`, `block` (send\|call), `inbox` (send\|receive) = 3 |
| `MAX_CAPABILITIES` | 16 per process | one region per sector read must be released as it is consumed, not accumulated |
| `block.device.v1` granularity | one sector per request | a 6724-byte object is **14 exchanges**, each answered with its own region |
| object assembly | — | the reader needs a buffer region of its own and **one copy per sector** out of each arriving region. `docs/35`'s *"no more than one payload copy between client memory and device-visible memory"* binds the **driver**; this copy is above it and must be stated rather than hidden |
| fuel | measured: ~4 700 per 512-byte pass in `state.tos`; 162 k for the whole block service | a SHA-1 pass over 10 704 bytes is ~168 compression blocks; an **estimate**, not a measurement, puts a full chain verification in the low hundreds of thousands of fuel — inside a declared envelope of a few million, which existing modules already declare |
| request count | a canonical service serves a compile-time count | the block service's constant must be re-derived for the slice, as it was for `state-store` (§2.11) |
| extent size | 32 703 sectors are undecided | the slice needs ~21 sectors of objects plus its index; a bound of a few hundred sectors would be generous |

## 13. Security and threat obligations

`docs/34` §Stage mapping assigns *repository* threats to Stage 5 — but *"a stage
cannot close if its new boundary lacks a threat entry, negative tests and stated
evidence level."* This slice introduces a new boundary — *"repository bytes to
object parser/verifier"*, which `docs/34` §Trust boundaries already lists as
boundary 4
— so it owes entries of its own, even though the *activation* threats stay
Stage 5's.

Negatives the slice owes, each traceable to a `docs/34` threat:

| Negative | Threat |
|---|---|
| a substituted object (one byte flipped) is refused | T4, A5 |
| a malformed object header or zlib frame is refused | T4; S2 *"Bounded parsing"* |
| an object id not present in the extent is refused | T4; S1 *"Fail closed on identity ambiguity"* |
| a graph past the declared depth or object-count bound is refused | T4 *"excessive recursion"* |
| an inflate output past the declared bound is refused | T4 resource exhaustion |
| a read addressed outside the extent is refused **before** the block service sees it | A5, and `STATE_STORE_V1` §4 |
| the state store's objects are unaffected | A6 *"/state … must not be confused with canonical /system"* |
| no process gains an authority it was not endowed | T0, A8 |
| the system commit id remains absent | **X3.9 directly** — this is the threat this slice is most likely to trip, and the one the accepted control already exists for |

**And one honest limitation to record.** If the hash family is SHA-1 (§7c.1),
recomputing an object id detects corruption and accidental substitution but is
**not** a defence against an adversary who can compute a collision. `docs/34`
T4 names *"hash collisions where feasible"*. Whatever family is chosen, the
threat entry must say which of the two it defends.

Evidence level: **E2** (automated positive and negative tests) is achievable for
every row. E3 is not claimed.

## 14. Performance obligations

`docs/35` gives Stage 4 the **block-driver** budgets and Stage 5 the
**repository** budgets (§1g). So this slice owes **no repository budget**, and
proposing one would be inventing a contract.

What it must not do is break Stage 4's own budgets: *zero dynamic allocation per
completed block request on the steady-state path*, *no more than one payload copy
between client memory and device-visible memory*, *no more than four
address-space/scheduler handoffs per unbatched request*. Those bind the driver,
and a repository reader is a client — but a reader that allocated a fresh region
per sector would put pressure on the same path, and the slice should state its
per-object allocation shape rather than leave it to be discovered.

The Stage 4 performance contract report is a **separate** `docs/16` deliverable
and is untouched here.

## 15. Decisions that require Project Architect approval

Four questions are genuinely undecided. Three need an ADR; one is a naming
ruling. Nothing else here does: everything in §4, §8 and §7d is settled by
clauses already accepted.

### D1 — What Stage 4's `capsule-to-repository handoff` means (ruling, and possibly an ADR)

`docs/16` names the deliverable; no accepted clause defines it. §3d proposes:
reading and verifying, linkage and not replacement, with the system commit id
staying absent. The alternative readings — and the reason each is rejected — are
in §3d and §4.

Needs: a ruling on whether §3d's reading is accepted, and whether the `docs/16`
line is renamed (*linkage*) or kept with a narrowed meaning. **Level 2** if
recorded as an ADR; a ruling may be enough, but the reading is load-bearing
enough that a recorded decision is worth more than a note.

### D2 — The repository extent's placement on the block device (**ADR, Level 3**)

Required by `STATE_STORE_V1` §4's own words: *"a separate decision"*. Fixes the
first sector, the sector count, the non-overlap rule and how the boundary is kept
from drifting between two contracts. §6 compares four options and recommends A.

Level 3 because it reserves persistent device space and creates a second
persistent extent — `docs/21`'s *"changes persistent formats"* test, the same one
that made ADR-0099 Level 3.

### D3 — The Stage-4 repository object profile (**ADR, Level 3**)

The largest of the four, and the one with two genuine gaps rather than choices:

- **the hash family** (§7c.1) — this repository is SHA-1, `docs/36`'s example
  says SHA-256, and `docs/19` R3 lists it as open;
- **how an object is located inside a raw extent** (§7c.6) — "loose object" means
  a filesystem path, and this stage has no filesystem, so the documented G1 is
  underspecified for the storage this stage actually has. **This is the missing
  decision, and it is a persistent format.**

Plus the choices: the loose-object encoding subset (§7c.2), the supported kinds,
the declared bounds, the refusal codes, and the fact that the result is a
*specialized* profile, which `docs/36` §Purpose says requires an ADR.

D2 and D3 could be one ADR. **Recommendation: one.** A placement without an
object-location format is not implementable, and two ADRs would cross-reference
each other for every field — the same argument that kept ADR-0099's layout and
protocol in one decision.

### D4 — Who provisions the initial extent, and the host-path discipline (**ADR, Level 2**)

§9 compares three and recommends A, with the tool held to `tos-capsule-tool`'s
standard. The decision is Level 2 rather than 3 because it fixes no persistent
format — D3 does — but it fixes what *"no hidden host path"* means for a
repository whose bytes a host wrote, which is exactly the claim `docs/37`
Stage 4's identity exit turns on.

### Explicitly **not** requiring an ADR

- **Nucleus responsibility.** §8 answers *none* from accepted clauses. There is
  nothing to decide; if a later stage needs a minimum, that stage decides it.
- **Refs.** §7d shows the slice needs none, because the capsule already carries
  the object id. A ref profile is G2's decision.
- **Packfiles, remotes, G2 promotion, `/system` mount, activation, rollback.**
  All assigned elsewhere by accepted clauses (§4).
- **A partition abstraction.** `STATE_STORE_V1` §4 already refused one for this
  device, on an argument that has not changed (§6B).

## 16. What remains explicitly for Stage 5

| Stage 5 owes | Clause |
|---|---|
| G2: deterministic blob/tree/commit **creation** | `docs/36` G2, *"Required by: Stage 5 exit gate"* |
| crash-safe local object publication | `docs/36` G2 |
| refs, branches, protected refs, reflog | `docs/36` G2; `docs/08` §Branch and protected-ref model |
| immutable `/system` mount **by commit** | `docs/16` Stage 5; `docs/09` §`/system` |
| the writable `/work/system` overlay, status, diff, commit | `docs/16` Stage 5 |
| candidate / current / last-known-good / recovery activation and rollback | `docs/16` Stage 5; I-05, I-06 |
| **the system commit id becoming present** | `PROCESS_IDENTITY_V1` §5 |
| *"commit tree is the installed `/system`"* | `docs/37` Stage 5 identity exit |
| repository and activation performance budgets | `docs/35` §Stage 5 |
| repository, ref, rollback, GC and migration threats | `docs/34` §Stage mapping |
| the second half of `docs/04`'s handoff protocol — the **transition** | `docs/04` boot phase 3–4; `docs/11` step 5 |

And between them, `docs/16` Stage 4E — the interactive console — which this note
touches not at all.

**Nothing in this note closes Stage 4C, Stage 4D or Stage 4.**
