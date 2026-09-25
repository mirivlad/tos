<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0102: Stage-4 capsule-to-repository linkage

- Status: **Proposed** (**not accepted, and nothing in the tree implements it**)
- Date: 2026-09-26
- Decision level: **3** — architectural, **requiring Project Architect approval**.
  It creates a **second persistent format** on the reference block device, which is
  `docs/21`'s explicit Level-3 test and the same one that made ADR-0099 Level 3. It
  also mints a **new capability kind at the trusted boot boundary** and adds one
  `SYSTEM_ABI_V1` operation. §15 is the architecture impact statement `docs/21`
  requires
- Project Architect approval: **not granted; this is a draft for review**
- Research basis: `docs/research/STAGE4_CAPSULE_REPOSITORY_HANDOFF.md`, whose §17–§20
  established that no accepted operation lets canonical text read the boot's source
  identity. That note's §18b-C recommendation, its D1 wording, its `Template` field
  proposal, its "authorizes nothing" phrasing and its D5-as-separate-Level-2 framing
  are **superseded by this decision** and are marked so there
- Related: **ADR-0016** (the capsule carries the raw OID); **ADR-0018** (detached
  source-set identity); **ADR-0031** and `docs/45` (the `source/system/` ↔ `/system`
  mapping); **ADR-0055** (endowment is a launcher decision); **ADR-0085**
  (capability representation); **ADR-0093**/**ADR-0095** (who holds
  `block.device.v1`); **ADR-0098** and `BLOCK_DEVICE_V1`; **ADR-0099** and
  `STATE_STORE_V1` §4 (which reserves sectors `0..64` and names this decision as the
  separate one); **ADR-0101**; `CAPSULE_FORMAT_V1` §4.2, §5, §6; `BOOT_ABI_V1` §6;
  `PROCESS_IDENTITY_V1` §5, §7.5; `SYSTEM_ABI_V1`; `SYSTEM_INTERFACE_V1` §4, §8;
  `docs/04`, `docs/08`, `docs/11`, `docs/16`, `docs/34`, `docs/35`, `docs/36`,
  `docs/37`

## 0. What this decides, and why it is one decision

`docs/16` gives Stage 4 a deliverable named **capsule-to-repository handoff** and no
accepted clause defines it. This decision defines the Stage-4 portion of it, and
everything that portion needs in order to exist:

| | |
|---|---|
| §2 | what the Stage-4 deliverable **means**, and what it deliberately is not |
| §3 | `system.boot.Identity` — a boot-lifetime capability over the verified boot source identity |
| §4 | one read operation and the record it produces |
| §5 | a **specialized** read-only Git profile under `docs/36` |
| §6 | a second bounded raw extent on the same device |
| §7 | that extent's persistent format |
| §8 | the bounds the reader refuses past |
| §9 | object location and the one traversal |
| §10 | who provisions the extent |
| §11 | the conformance evidence acceptance carries |

**One decision rather than five.** A repository extent with no object-location format
is not implementable; an object-location format with no reader authority is
unreachable; a reader that cannot read the boot identity can only prove the answer it
was compiled with. The five pieces have no independent existence, and five ADRs would
cross-reference each other for every field — the same argument that kept ADR-0099's
layout and its protocol in one decision.

**Nothing here is implemented.** No sector is reserved in production code, no ABI
operation exists, no capability kind exists, and no repository code is written.

## 1. What is already decided, and must not be re-decided

Cited because this decision is built on them and changes none of them.

| Clause | Fact |
|---|---|
| `CAPSULE_FORMAT_V1` §5 | the boot-canonical bit *"is set for exactly one file, the system boot text at `/system/boot/init.tos`"*, and a path/file flag inconsistency is rejected |
| `CAPSULE_FORMAT_V1` §4.2 | every file entry carries `content_digest`, a 32-byte SHA-256 the parser validates against the exact content bytes |
| `CAPSULE_FORMAT_V1` §6, ADR-0016 | for a git identity the header holds `source_oid_alg`, `source_oid_length` and the **raw commit object id** |
| `BOOT_ABI_V1` §6 | the loader copies all four identity fields into BootInfo and the nucleus re-verifies the capsule digest |
| ADR-0031 §2, `docs/45` | *"the repository subtree `source/system/` is the canonical input for the runtime `/system` tree, mapped directly and without renaming or generation"* |
| `STATE_STORE_V1` §4 | sectors `0..64` are the state store's; the rest are *"undecided"*; *"the capsule-to-repository handoff is a separate decision and must not overlap this extent"*; and there are **no partitions at Stage 4** |
| `BLOCK_DEVICE_V1` §10 | one sector per request; no batching; no filesystem, partition, cache or object store |
| `PROCESS_IDENTITY_V1` §5, §7.5 | the system commit id is **absent** until Stage 5, asserted by test *"so that a later stage cannot make it present by accident"* |
| `SYSTEM_INTERFACE_V1` §8 | *"the target ABI is `SYSTEM_ABI_V1` and nothing else: one mechanism, one path to audit"* |
| `docs/36` §Purpose | *"the profiles are cumulative unless an ADR explicitly defines a specialized profile"* |
| ADR-0085 | the capability representation is a closed enumeration; `AsInterface` is one of its members |

## 2. The meaning of the Stage-4 deliverable

**The `docs/16` name does not change.** The roadmap item stays
*"capsule-to-repository handoff"*. What this decision fixes is which **half** of it
Stage 4 owes.

### 2a. The decision

**The Stage-4 portion of the handoff is linkage and verification.** Canonical text
verifies that the repository extent contains the Git commit the boot capsule names,
and that this commit resolves `source/system/boot/init.tos` to bytes identical to the
capsule's boot-canonical `/system/boot/init.tos`:

```text
capsule source commit OID
        ↓                       (from system.boot.Identity, §3, §4)
repository commit/tree traversal (§9, verified object by object)
        ↓
source/system/boot/init.tos     (the one path, fixed normatively)
        ↓
SHA-256(blob payload)
        ==
capsule boot-canonical file SHA-256
```

**That equality is the Stage-4 linkage witness**, and it is the whole claim.

### 2b. What Stage 4 therefore does not do

Throughout this slice, and asserted rather than assumed:

```text
the system commit id remains absent
the capsule remains the installed source set
/system is not repository-mounted
the repository-backed init does not take over
```

**Stage 4 does not transition execution to the repository.** The transition half of
the handoff is Stage 5's.

### 2c. Reconciliation with `docs/04` and `docs/11`

Neither document is wrong, and neither is edited. Both describe the **completed**
sequence:

- `docs/04` boot phase 3: the capsule's `/system/boot/init.tos` *"launches
  boot-critical driver services, discovers repository storage, verifies the selected
  commit, and transitions to the repository-backed system tree"*;
- `docs/04` boot phase 4: *"the repository-backed `/system/boot/init.tos` takes over.
  It may differ from the capsule copy only through a defined handoff protocol"*;
- `docs/11` §Bootstrapping step 5: *"repository-backed versions replace capsule
  versions through a versioned handoff"*.

**The development-stage boundary runs through the middle of that sentence.** Stage 4
delivers *discovers repository storage* and *verifies the selected commit*. The clause
after it — *transitions to the repository-backed system tree* — is word for word
`docs/37` Stage 5's identity exit (*"commit tree is the installed `/system`"*) and
`docs/16` Stage 5's *"immutable `/system` mount by commit"*. Reading it as a Stage-4
obligation would make Stage 4 deliver Stage 5.

**And `docs/04`'s phase numbering is not `docs/16`'s stage numbering.** `docs/04`
describes four phases of one boot; `docs/16` describes development stages. That the
word "Stage 4" appears in both is a collision of vocabulary, not a shared claim, and
§14 records the reconciliation so a later reader does not have to rediscover it.

### 2d. The landing point is fixed, not chosen

`CAPSULE_FORMAT_V1` §5 makes `/system/boot/init.tos` the **only** boot-canonical file
a capsule may have, and `docs/04` names that same module as the component that
discovers and verifies repository storage. So the path this slice traverses to is
fixed by an accepted contract before this decision is made, and ADR-0031 §2 fixes its
repository spelling. There is no choice here to get wrong, and no reader can be
pointed at a different file.

**Stage 4 proves linkage of the boot-canonical source, not of every capsule unit.**
That is a smaller claim than "the repository contains the source set", and it is the
one the capsule contract can support on its own.

## 3. `system.boot.Identity`

One new capability kind and one new interface.

### 3a. What it is

```text
interface   system.boot.Identity
kind        a singleton object for the boot
minted      only at the trusted boot boundary, by the launcher
```

- **One immutable object for the boot.** It names a fact established before the first
  process ran and never changes while the machine is up.
- **Ordinary processes cannot create one.** There is no operation that mints it; it
  exists because the nucleus established the boot, and it reaches a process only by
  endowment (`ADR-0055`: an endowment is what a launcher decided).
- **Non-affine.** It is not consumed by use, and `capability_attenuate` may refine a
  name for it exactly as it does for an endpoint (ADR-0100).
- **Delegable and attenuable normally.** A helper receives it only through explicit
  delegation or endowment; nothing about it is ambient.
- **Lifetime is the boot.** `capability_release` releases a *name*, as it does for
  every non-affine object; the object outlives every name.
- **Representation `AsInterface`** (ADR-0085), so `LANGUAGE_VERSION` does not move
  and no representation-family member is added.

### 3b. The right

**`RIGHT_READ`, the existing generic right**, and no identity-specific right is
invented. A right whose only holder is one interface is a right that says nothing a
capability type does not already say.

### 3c. What it authorizes, stated exactly

This capability **does** authorize something, and the draft says so rather than
calling it harmless:

```text
authorizes:  READ of the immutable, already-verified boot source identity
```

and nothing mutable. Holding it does **not** allow: selecting a commit; modifying any
identity; reading any capsule file; enumerating the capsule; launching anything;
controlling a process; allocating memory; or changing boot policy.

**It is nevertheless authority, and it is minted at the trusted boot boundary.** A
process that holds it can learn which source the machine booted from; a process that
does not hold it cannot. That difference is exactly what a capability is for, and
calling it "authorizes nothing" would be the kind of description that makes a
boundary stop being examined.

### 3d. Who gets one

The bootstrap launch policy may endow the boot-canonical init with it. Every other
process receives it, if at all, through explicit endowment or delegation. A module
that does not ask for it is not given one, which is `ADR-0055` working normally.

## 4. One read operation

### 4a. The row

```text
boot_identity_read(cap: system.boot.Identity)
    -> Result<system.boot.IdentityRecord, i64>
```

One operation, not a family. There is **no separate caller-identity query**, no path
lookup, no enumeration and no second question. `ADR-0093` §3a's discipline applies
here too: a surface that answers one question about one fact is the whole of it, and
growing it is a later decision with its own reasons.

### 4b. The record

Only facts the boot chain already verified:

```text
system.boot.IdentityRecord
    source_kind           1 = git commit, 2 = detached source set
    oid_algorithm         0 = none, 1 = SHA-1, 2 = SHA-256
    oid_length            0, 20 or 32
    oid                   the raw value, as the capsule header holds it
    boot_content_sha256   the boot-canonical file's validated SHA-256
```

**No pathname field.** `/system/boot/init.tos` is normative in `CAPSULE_FORMAT_V1` §5
and `source/system/boot/init.tos` is normative in ADR-0031 §2; a field carrying either
would be a second place for a fact that already has one, and a place where the two
could disagree.

**No system-commit field**, and none is added by any later edit to this record without
its own decision. `PROCESS_IDENTITY_V1` §5 makes that field Stage 5's.

**No process self-identity query.** §18b-C of the research note proposed one; §2d
replaces it with a landing point fixed by contract, which needs no such query and does
not drift when a second canonical module appears.

### 4c. How the record is carried

`SYSTEM_INTERFACE_V1` §4.2's record fields are `u64`, `Option<u64>`, capability types
and `Option<Region<u8>>` — **there is no array field type**, and this decision does
not add one. A new language representation invented to make one record look tidy would
be a language change bought for cosmetics.

So the two 32-byte values are carried as **four `u64` chunks each**, with a normative
rule: *chunk `i` holds bytes `[8i, 8i+8)` of the value in ascending address order, as
a little-endian `u64`.* Eleven `u64` fields in this order:

```text
source_kind  oid_algorithm  oid_length
oid_0  oid_1  oid_2  oid_3
boot_content_0  boot_content_1  boot_content_2  boot_content_3
```

**A SHA-1 OID occupies bytes `[0, 20)`; bytes `[20, 32)` are zero**, as
`CAPSULE_FORMAT_V1` §6 already requires of the header field, and `oid_length` is what
says so. A reader that ignored `oid_length` and compared all 32 bytes would still be
correct for this profile; one that ignored it and compared 32 bytes against a 32-byte
SHA-256 OID would not, which is why the field is present.

### 4d. Where it is written, and by whom

`SYSTEM_ABI_V1` already has the shape: *the nucleus writes a record at a fixed offset
of the caller's own argument region*, as operations 14, 17, 19, 27 and 30 do. A new
operation follows it, and the nucleus walks no pointer a caller supplied
(`SYSTEM_ABI_V1` §3).

```text
SYSTEM_ABI_V1 operation 32   the next free number; 31 is the highest assigned
BOOT_IDENTITY_RECORD         MMIO_MAP_RECORD + 16 = 592 in the argument region
record size                  88 bytes, ending at 680 of the 4096-byte frame
```

The existing `const` assertions that keep fixed results from overlapping gain one more
row, so the layout stays checked rather than counted.

### 4e. Why this cannot be answered without the ABI

The value is already in the process's own read-only `Launch` record, so the runtime
image could in principle answer from memory it already holds, with no syscall — the
shape ADR-0100 and ADR-0101 had. **`SYSTEM_INTERFACE_V1` §8 forbids it**: *"the target
ABI is `SYSTEM_ABI_V1` and nothing else: one mechanism, one path to audit"*, and every
runtime-image row carries an ABI operation number. A row answered without crossing the
boundary would be a second mechanism and a second path to audit.

### 4f. What the nucleus needs, and what it does not

- **`launch::Template` already retains `pub bi: &'static BootInfo`**, which already
  carries `source_identity_kind`, `source_oid_alg`, `source_oid_length` and
  `source_identity_value`, all validated. **No duplicate fields are added.** The
  research note's §19c proposal to add `oid_algorithm` and `oid_length` to `Template`
  is withdrawn: it would have created a second copy of a fact already there.
- **The boot-canonical digest is retained, not recomputed.** `Capsule::boot_file()`
  returns a `File` whose `digest` field is the file entry's `content_digest`, and
  `parse()` has already checked it against the exact content bytes. `Template` gains
  **one** field — that 32-byte digest — copied at `establish`. The nucleus does no
  cryptographic work for this operation, then or later.
- **No capsule catalog is retained.** One digest of one normatively fixed file, not a
  table.

## 5. The specialized Stage-4 Git profile

`docs/36` §Purpose requires a specialized profile to be defined by an ADR. This is
that definition. It is a **read-only subset of G1** and is not G1.

### 5a. Scope

```text
supported     commit, tree and blob objects
              bounded traversal of one path
              object-integrity recomputation
              comparison against an independent Git oracle

unsupported   tag objects; refs of any kind; object writing; packfiles;
              network; merge and diff; garbage collection; activation
```

**No refs**, and that is not a shortcut. The starting object id comes from the capsule
through `system.boot.Identity`; a ref is a *name for an object id*, and this slice
already has the id from a source the accepted boot chain verified. Refs arrive with
G2, where there is something to select between.

### 5b. Hash family: SHA-1

**The Stage-4 reference profile verifies SHA-1 Git object ids**, because that is what
the repository and the capsule OID actually are. `git rev-parse --show-object-format`
reports `sha1`, and every capsule this tree builds therefore carries
`(source_oid_alg = 1, source_oid_length = 20)`.

**The repository is not migrated to Git's SHA-256 object format** to save one hash
implementation. Migration would change every existing capsule identity and every
provenance record already published, which is a far larger price than a second hash
in canonical text.

Canonical text therefore implements two hashes:

```text
SHA-1      Git object identity
SHA-256    the capsule boot-content comparison
```

**The security limitation, stated rather than implied.** Recomputing a SHA-1 object id
proves corruption and substitution *under this conformance profile*. It is **not**
claimed as collision resistance against a motivated adversary who can produce a
collision; `docs/34` T4 names *"hash collisions where feasible"* and this profile does
not defend against that threat. §12 carries the same statement where a threat reader
will meet it.

**An unsupported `source_oid_alg` fails closed.** A capsule declaring SHA-256 while
this profile verifies SHA-1 is refused, not reinterpreted. A later ADR may extend or
migrate the profile; this one does not leave room for a silent reinterpretation.

### 5c. Loose-object encoding: ordinary Git, stored blocks only

The object stream is ordinary Git loose-object semantics:

```text
zlib( "<type> <size>\0" || payload )
```

and the object id is over the **uncompressed** `"<type> <size>\0" || payload`.

**This profile accepts only DEFLATE stored blocks (`BTYPE = 00`)**, and a **bounded
sequence** of them rather than a single block: a stored block carries at most 65535
bytes, so an object larger than that is several blocks with `BFINAL` on the last. A
reader that accepted only one block would be a reader that worked until an object grew.

Validated, in order, and every failure is a refusal:

1. the zlib header: `CM = 8`, a valid `CINFO`, `(CMF*256 + FLG) % 31 == 0`, and
   **`FDICT = 0`** — a preset dictionary is refused;
2. stored-block framing: `BTYPE == 00`, and the bit position padded to a byte
   boundary before `LEN`/`NLEN`;
3. `LEN` and `NLEN` are one's complements of each other;
4. the total inflated size equals the object's declared `uncompressed_length` (§7b)
   **and** the size in its own `"<type> <size>\0"` header;
5. the inflated total never passes `MAX_OBJECT_UNCOMPRESSED_BYTES` — checked *before*
   the bytes are produced, from the table entry, not after;
6. the Adler-32 trailer over the inflated bytes.

**Fixed and dynamic Huffman blocks (`BTYPE = 01`, `BTYPE = 10`) are refused as
unsupported in this profile.** That is a declared restriction on the *producer*, not a
claim about Git: the host provisioner (§10) must emit the supported representation, and
**ordinary Git must still read the generated object stream successfully** — which
`core.looseCompression 0` already produces and `git cat-file` already reads.

`BTYPE = 11` is reserved and is refused as malformed.

## 6. The repository extent

### 6a. The decision

```text
the same block device as the state store
STATE_STORE_V1              sectors [0, 65)      unchanged
repository extent           sectors [65, 2113)
    REPOSITORY_FIRST_SECTOR = 65
    REPOSITORY_SECTORS      = 2048               1 MiB
everything at or above 2113 stays unowned
```

**No partition table.** `STATE_STORE_V1` §4 already refused a base-offset field
*"because it would have exactly one possible value"*, and a two-entry table whose
values are constants is that argument again with a parser and a threat surface
attached. **No filesystem, no allocator, no second device**: a second device would need
enumeration, a multi-device registry and discovery, all three of which `ADR-0093` §3a
names as what Stage 4 does not get.

### 6b. How overlap is prevented

By a constant the reader checks, exactly as `STATE_STORE_V1` prevents running past its
own end — and by the same kind of evidence: an address outside `[65, 2113)` is refused
**by the reader, before the block service sees it**, and the counted device requests of
the gate show it never did (§11).

The boundary is stated once, in this decision, and asserted from both sides: this ADR
names 65 as the state store's exclusive upper bound and the repository's first sector,
and §14's documentation reconciliation records it in both contracts so the two cannot
drift apart silently.

### 6c. The static accounting

Required by the review before the bound is frozen.

```text
extent                     2048 sectors            1 048 576 bytes
  header                      1 sector       §7a
  object table                3 sectors      §7b   32 entries × 48 bytes = 1536
  object data              2044 sectors            1 046 528 bytes
```

The chain the Stage-4 traversal actually needs, measured at `4600a8f`, with the stored
-block stream size `N + 5k + 6` where `N` is the Git object bytes and `k` the number of
stored blocks:

| object | content | `N` | stream | sectors |
|---|---|---|---|---|
| commit | 6724 | 6736 | 6747 | 14 |
| root tree | 1159 | 1169 | 1180 | 3 |
| `source` tree | 668 | 677 | 688 | 2 |
| `source/system` tree | 31 | 39 | 50 | 1 |
| `source/system/boot` tree | 75 | 83 | 94 | 1 |
| `init.tos` blob | 2047 | 2057 | 2068 | 5 |
| | | | **total** | **26** |

So the whole extent in use is `1 + 3 + 26 = 30` of 2048 sectors — **68× headroom**, and
the commit object is the largest only because of a long commit message.

**The aggregate bound and the per-object bound are independent, and the extent is the
binding one.** `MAX_OBJECTS` × `MAX_OBJECT_UNCOMPRESSED_BYTES` is 32 × 128 KiB = 4 MiB,
which does not fit 1 MiB — and it is not meant to. The per-object bound says what one
object may be; the extent says what all of them may be together. A table whose derived
placement (§7c) would run past `extent_sectors` is **refused at header validation**,
before any object is read. The arithmetic is consistent and no bound needs changing.

## 7. The repository extent format

A persistent format, and therefore the reason this decision is Level 3. All
multi-byte integers are **little-endian**; all reserved bytes **must be zero** and a
non-zero reserved byte is a refusal.

### 7a. The header — extent sector 0, 512 bytes

| Offset | Size | Field | Rule |
|---|---|---|---|
| 0 | 8 | `magic` | the bytes `54 4F 53 47 49 54 52 31` (`TOSGITR1`), in ascending address order, so no endianness applies |
| 8 | 4 | `format_version` | `u32` = 1 |
| 12 | 4 | `oid_algorithm` | `u32` = 1 (SHA-1) at v1 |
| 16 | 4 | `oid_length` | `u32` = 20 at v1 |
| 20 | 4 | `object_count` | `u32`, `1 <= object_count <= MAX_OBJECTS` |
| 24 | 4 | `entry_size` | `u32` = 48 |
| 28 | 4 | `table_sectors` | `u32` = 3 |
| 32 | 4 | `data_start` | `u32` = 4 — the first data sector, as an index inside the extent |
| 36 | 4 | `extent_sectors` | `u32` = 2048 |
| 40 | 472 | `reserved` | every byte zero |

**There is no root-commit field, and that is deliberate.** The root comes only from
`system.boot.Identity`. An extent that could name the commit a reader should trust
would be an extent that gets to choose what the system believes about itself — the
whole point of the linkage is that the *capsule* names the commit and the *repository*
is checked against it.

A header whose `oid_algorithm` or `oid_length` disagrees with the record from
`system.boot.Identity` is a refusal, not a reinterpretation.

### 7b. The object table — extent sectors 1..3

`MAX_OBJECTS = 32` entries of `entry_size = 48` bytes = 1536 bytes = exactly three
sectors. Entries `[object_count, MAX_OBJECTS)` must be entirely zero.

| Offset | Size | Field | Rule |
|---|---|---|---|
| 0 | 32 | `oid` | the object id, bytes `[0, oid_length)`; bytes `[oid_length, 32)` **must be zero** |
| 32 | 4 | `stored_length` | `u32`, the bytes of the zlib stream, non-zero |
| 36 | 4 | `uncompressed_length` | `u32`, the bytes of `"<type> <size>\0" || payload`, `<= MAX_OBJECT_UNCOMPRESSED_BYTES` |
| 40 | 8 | `reserved` | zero |

**Sorted by `oid`, ascending by byte value, and strictly increasing.** An unsorted
table, a duplicate id, or an all-zero id inside `[0, object_count)` is a refusal. Sorted
and unique is what makes lookup bounded and makes "which entry names this object"
have exactly one answer.

### 7c. Object placement is derived, not stored

```text
first_sector(i) = data_start + Σ ceil(stored_length(j) / 512)   for j < i
```

in table order. **No per-object sector offset is stored**, so two objects cannot be
placed on top of each other: overlap is not a representable state rather than a state
that is checked for. The validation is the total:

```text
data_start + Σ ceil(stored_length(j) / 512)  <=  extent_sectors     for all j
```

checked once at header validation, before any object is read. An extent whose objects
would run past its own end is refused whole.

### 7d. What a v1 reader refuses

Bad magic; a `format_version` other than 1; an `oid_algorithm` or `oid_length` this
profile does not support, or that disagrees with the identity record; an `entry_size`,
`table_sectors` or `data_start` inconsistent with `MAX_OBJECTS` and the layout above;
`object_count` of zero or past `MAX_OBJECTS`; any non-zero reserved byte; a table that
is unsorted, has a duplicate id, or has a non-zero entry past `object_count`; a
`stored_length` of zero; an `uncompressed_length` past the bound; and a placement total
past `extent_sectors`.

## 8. Bounds

Declared, not measured. A bound set to what today happens to need is a bound that
breaks on the next commit message.

| Bound | Value | Why |
|---|---|---|
| `MAX_OBJECTS` | 32 | the minimum chain is 6; a `MAX_TREE_DEPTH` path needs at most 18; 32 leaves room without making the table span more than three sectors |
| `MAX_OBJECT_UNCOMPRESSED_BYTES` | 128 KiB | ~19× the largest object in the measured chain |
| `MAX_TREE_DEPTH` | 16 | `source/system/boot/init.tos` is depth 4 |
| `MAX_TREE_ENTRIES` | 256 | the root tree has 29 entries today, `source` has 17 |
| `MAX_PATH_BYTES` | 256 | `source/system/boot/init.tos` is 28 bytes. Distinct from `CAPSULE_FORMAT_V1`'s `MAX_PATH_BYTES` of 1024, which bounds a *capsule* path; this one bounds the *repository* path this profile traverses |
| aggregate stored bytes | `extent_sectors` (§6c) | the extent is the binding limit, and the per-object bound does not imply every object may be maximum-sized at once |

**Refuse before crossing, never after.** Every bound is checked against a declared
number before the work it bounds is done: `uncompressed_length` from the table before
inflating, depth before descending, entry count while parsing a tree, path length
before matching. A bound checked afterwards is a bound an attacker has already spent.

## 9. Object location and traversal

The reader, in order:

1. reads and validates the repository header (§7a, §7d);
2. reads the bounded object table (§7b);
3. looks up the commit OID **from `system.boot.Identity`**, never a compiled constant;
4. reads exactly that Git commit object;
5. verifies SHA-1 over its uncompressed canonical object bytes;
6. obtains its `tree`;
7. traverses only the path `source/system/boot/init.tos`;
8. verifies every intermediate tree object against its own OID;
9. verifies the final blob against its Git OID;
10. computes SHA-256 over the blob payload;
11. compares it with `boot_content_sha256` from `system.boot.Identity`.

**Step 11 is the Stage-4 linkage witness.**

Explicitly not done: **no commit parent is traversed**; **no history is enumerated**;
**no unrelated tree subtree is entered**. At each level the reader matches one path
component and descends into one entry; the other entries of that tree are parsed only
far enough to be skipped, within `MAX_TREE_ENTRIES`.

A missing path component, a tree entry of the wrong kind for its position (a blob
where a tree must be, or the reverse), or an object id absent from the table is a
refusal with its own code.

## 10. Provisioning

### 10a. The decision: host-provisioned, TOS-verified

A deterministic host tool creates the extent as a **derived fixture artifact**, in this
order, and the order is the decision:

```text
build or parse the capsule
        ↓
take the source OID from the capsule
        ↓
resolve that exact Git commit in the host repository
        ↓
collect only the bounded object chain that
    source/system/boot/init.tos needs
        ↓
encode supported stored-block loose objects (§5c)
        ↓
write the deterministic repository extent (§7)
```

**The capsule is the authority for which commit the fixture provisions.** The tool must
**not** independently use host `HEAD` after the capsule has been built: a fixture that
took its root from `HEAD` would agree with the capsule by coincidence of timing, and
would stop agreeing the moment anything committed between the two steps — which is
exactly the class of failure the linkage exists to detect.

### 10b. Obligations on the tool

Held to `tos-capsule-tool`'s standard: deterministic output; no timestamps; no absolute
host paths in the artifact; an **independent checker** that verifies the boot OID and
the object chain without reusing the writer's code; ordinary Git used as an oracle,
which `docs/08` §External implementations permits; and the generated object streams
readable by ordinary Git (§5c).

### 10c. The host-path discipline

**After QEMU boot begins, no host Git command and no host file path participates in the
result.** The host writes bytes to a disk image exactly as it already writes the capsule
to an ESP; what `docs/37` Stage 4's identity exit forbids is a hidden host path *in the
running system*, and the controls are the ones Stage 4 already uses — the reader holds
no device authority but `block.device.v1`, and the counted device requests of the gate
show the reads happened.

### 10d. Why not the alternatives

**A textual provisioner writing the objects** is `docs/36` **G2** — *"deterministic
blob/tree/commit creation"* and *"crash-safe local object publication"* — assigned to
the Stage 5 exit gate, and it needs a deflate **compressor** and tree/commit
construction in canonical text. It steals the stage boundary. It is the right answer at
Stage 5.

**Repository material inside the capsule** would make reading the repository a second
reading of the capsule: no device, no persistence, no second representation, nothing the
block driver did — and it would push the capsule toward `ADR-0021`'s bounds for no gain.

## 11. Conformance evidence this decision requires

Acceptance carries these obligations, and none is met today.

### 11a. Boot identity

1. Canonical text holding `system.boot.Identity` reads the **exact** `source_kind`,
   `oid_algorithm`, `oid_length`, OID bytes and `boot_content_sha256` that came from the
   boot capsule.
2. **A mutation to each relevant boot field makes the linkage evidence fail** —
   separately for the OID and for the boot-content digest, so neither is carried by the
   other.
3. **A process not endowed with `system.boot.Identity` cannot read the record**, refused
   by exact status.

### 11b. Positive linkage

4. The repository reader holds only what it needs:

   ```text
   system.boot.Identity   with read
   block.device.v1        send | call
   ```

   and **no PCI, MMIO, IRQ or DMA authority** — asserted from the boot journal's
   capability records, as `state-store.sh` asserts topology today.
5. It starts from the OID the identity operation returned, **not a compiled constant**.
6. It reaches `source/system/boot/init.tos`, verifies every object against its own OID,
   and proves `SHA-256(repository blob payload) == boot_content_sha256`.

### 11c. Required negatives

Each by exact refusal code, and each its own case:

| | |
|---|---|
| substituted object byte | the OID no longer matches its bytes |
| malformed zlib header | bad `CM`, bad check, or `FDICT = 1` |
| malformed stored-block `LEN`/`NLEN` | not one's complements |
| bad Adler-32 | trailer disagrees with the inflated bytes |
| object SHA-1 mismatch | recomputation differs from the table id |
| missing OID | an id the traversal needs is not in the table |
| duplicate OID table entry | two entries name one object |
| unsorted table | entries not strictly ascending |
| object count past bound | `object_count > MAX_OBJECTS` |
| object length past bound | `uncompressed_length > MAX_OBJECT_UNCOMPRESSED_BYTES` |
| traversal depth past bound | a path deeper than `MAX_TREE_DEPTH` |
| malformed Git object header | not `"<kind> <decimal>\0"`, or a size disagreeing with the payload |
| wrong object kind | a blob where a tree must be, or the reverse |
| missing path component | `source/system/boot/init.tos` does not resolve |
| a request that would cross the extent | an address below 65 or at/above 2113 |
| unsupported OID algorithm | a capsule declaring an algorithm this profile does not verify |
| a **different valid commit** than the capsule's | the extent holds a well-formed commit the capsule does not name: refused, not preferred |
| the correct commit but the **wrong boot blob** | traversal succeeds, the SHA-256 comparison fails |
| the state store is unaffected | a `GET` of an object the state store holds still succeeds after the repository reads, in the same boot |

**Where "no lower block request" is part of a refusal, it is proved from the real block
service's journal**, not from the reader's own assertion — the derived-count discipline
`block-protocol.sh` and `state-store.sh` already use.

### 11d. The restart shape

7. **A second repository-reader process in the same boot**, created after the first has
   ended and been collected, re-reads the extent from the device and reaches the same
   witness. That is the shape `state-store.sh` already proves for a service generation.

8. **Cross-reboot persistence is not claimed.** The harness re-creates the disk image
   for every `run.sh` invocation, so no gate in this repository has ever shown a byte
   surviving a reboot. The evidence says the in-boot re-read and says plainly that the
   other claim is unproved, rather than implying it.

## 12. Threat statement

`docs/34` requires that a stage's new boundary have a threat entry, negative tests and a
stated evidence level. This decision adds two boundaries to `docs/34` §Trust boundaries'
existing list, one of which is already there as boundary 4:

```text
verified boot identity   ->  canonical textual repository verifier   (new)
raw repository bytes     ->  canonical textual object parser         (boundary 4)
```

Stated explicitly:

- **`system.boot.Identity` reveals an immutable verified fact.** It does not select
  boot state, mutate it, or make any commit more trusted than another. A holder learns
  what the machine booted from; it cannot change it.
- **SHA-1 collision resistance is not claimed** (§5b). Recomputation proves corruption
  and substitution under this profile; a motivated collision-producing adversary is
  outside what this profile defends against, and a later ADR that migrates the hash
  family is the answer to that threat, not this one.
- **Raw repository bytes are attacker-controlled input to a textual parser.**
  `docs/34` S2 requires bounded recursion, allocation and work before such input is
  processed; §8's bounds are that, and §5c's ordering — bound checked from the table
  *before* inflating — is what makes a decompression bomb unreachable rather than
  survivable.
- **Every size, depth and count bound fails closed**, with no fallback and no partial
  answer.
- **Repository reading can never address a sector below 65 or at or above 2113**, and
  §11c proves it from the block service's journal rather than from the reader's word.
- **Repository linkage does not create system commit identity.** The X3.9 control
  stands: the system commit id remains absent, asserted by the existing test.

Evidence level: **E2** for every row — automated positive and negative tests. E3 is not
claimed.

## 13. Non-claims

This decision explicitly excludes, and nothing in it may be read as delivering:

```text
repository-backed /system          the system commit id becoming present
refs                               protected refs
repository writes                  Git object creation
commit creation                    a working overlay
status, diff                       candidate / current / LKG / recovery
activation                         rollback
packfiles                          network
cross-reboot persistence           Stage 4 closure
tag objects                        history traversal or enumeration
a filesystem, partition or allocator
```

**Stage 5 owns them**, and `docs/36` G2 is where the first group is assigned.

## 14. Documentation reconciliation

On acceptance, and not before:

1. **`docs/16`'s line is not renamed.** *"Capsule-to-repository handoff"* stays; this
   ADR is what defines its Stage-4 portion.
2. **The stage boundary is recorded explicitly**, in this ADR and referenced from the
   contracts it touches:

   ```text
   docs/16 Stage 4  "capsule-to-repository handoff"  =  the linkage/verification half
   docs/04 phases 3-4 and docs/11 step 5
       transition / replacement                      =  completed in Stage 5
   ```

   **The older architectural description is not erased.** `docs/04` and `docs/11`
   describe the finished boot sequence correctly; what was missing was the
   development-stage boundary running through it, and that is what is added.
3. **`STATE_STORE_V1` §4 gains the other half of its own sentence.** It already says the
   handoff *"must not overlap this extent"*; it gains the fact that the repository extent
   begins at sector 65 and ends at 2113, so the boundary is stated from both sides.
4. **`docs/36` gains the specialized-profile reference**, as its §Purpose requires.
5. **`docs/34` gains the boundary and the SHA-1 limitation** of §12.
6. **`docs/19` R3's open question narrows**: the *"exact initial object/hash/ref
   profile"* is answered for Stage 4 by §5, and remains open for the G1→G2 promotion.

## 15. Architecture impact statement (`docs/21`)

- **Which invariants are affected?** None is amended. **I-02** is the one to justify:
  the permanently binary trusted base may contain what is required to *"verify
  repository state"*, and this adds the smallest readable form of an already-verified
  fact — not repository verification itself, which stays in canonical text. **I-07** is
  served: the new authority is explicit, endowed, and refusable. **I-09** is served: a
  new persistent format is versioned from its first implementation (`format_version`),
  and so is the record. **I-15** is the reason §5b states the SHA-1 limitation instead
  of calling the profile "Git-compatible".
- **What becomes canonical after the change?** That a process may learn, through an
  explicit capability, which source the machine booted from; and that sectors
  `[65, 2113)` of the reference device hold a bounded read-only Git object extent in the
  format §7 fixes.
- **What enters or leaves the trusted base?** One object kind and one read-only ABI
  operation enter. Nothing leaves. **No repository parsing, verification or traversal
  enters the nucleus**: `docs/08` scopes its nucleus list to *"mechanisms required for
  trusted boot and immutable mounting at the selected profile"*, and at Stage 4 the boot
  text is the capsule's and there is no mount, so no item on that list fires.
- **Can the active runtime still identify its exact source?** Yes, and better: the
  identity it already reported becomes one it can also read.
- **Can all derived artifacts be discarded and regenerated?** Yes. The repository extent
  is a derived fixture artifact (§10a) and is rebuilt from the capsule and the host
  repository.
- **Can the owner still recover and boot a previous commit?** Unchanged. Nothing here
  selects a commit or affects boot selection.
- **Does the change create a hidden host dependency?** No — §10c. The host builds the
  extent as it builds the capsule; after boot begins, nothing host-side participates,
  and the evidence proves it from the journal.
- **Does it alter licensing or patent exposure?** No. The reader is an independent
  interoperability implementation of a published format; `docs/08` §Licence boundaries
  permits Apache-2.0 for independent readers and test vectors, and the activation
  services this decision does not build remain GPL-3.0-or-later.
- **How is the behavior tested?** §11's obligations: three identity, three positive and
  nineteen negative, plus the in-boot restart shape.

## 16. What this does not decide

- **The G1→G2 promotion**, and the evidence it requires — `docs/19` R3's open question
  in its remaining half;
- **a hash-family migration**, which is a later ADR with its own reasons;
- **packfiles, refs, remotes** and everything in §13;
- **what happens to the extent when the repository grows past 2048 sectors**, which is a
  bound this decision freezes for Stage 4 and a later stage may revisit with a reason;
- **Stage 4 closure.** Stage 4C, Stage 4D and Stage 4 do not close here, and **Stage 4E
  remains mandatory after formal Stage 4 closure and before Stage 5.**
