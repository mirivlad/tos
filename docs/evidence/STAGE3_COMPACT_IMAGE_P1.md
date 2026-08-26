<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# A compact verified module image — what it costs, measured

Evidence level: **P1, locally measured**, on the same instrumented bounded heap
every arena figure in this project is taken through
(`STAGE2_ARENA_BOUND.md`, `STAGE3_PROCESS_GRANT.md`): the production frontend
runs *through* the heap, so a byte figure is the allocator's own accounting
rather than a sum of requests.

Scope: this answers the seven questions **ADR-0070 §6** asks before that ADR can
be accepted. It is the evidence ADR-0070 was **accepted on (2026-08-26)**, and
that acceptance carries ADR-0070 §7's implementation gate: no production engine
integration until a production format covers 100 % of `tos-ir/v1` and closes
docs/43 §1 in full. `RUNTIME_GRANT = 54 MiB` remains **provisional**, and nothing
here is switched into the production engine.

Verdict, stated once: **one ceiling-sized module is `388 329 B` as an image —
`33.13x` smaller than the live `tos_ir::Module` and `14.32x` smaller than the
current canonical stream — and verifying it still costs `28.32 MiB` of peak
arena, because this reader materializes a `Module` before the semantic verifier
traverses it. Artifact density and verification working set are different
quantities, and only the first one improved.**

## What was built, and what it is not

A measurement-only encoder, parser and verifier path
(`source/tests/image-prototype/`). It is **not** a production format:

- the magic is `TOSIMGx0` and the encoding version is `0` — **experimental**,
  chosen so that nothing written here can be mistaken for, or later quietly
  promoted into, the format ADR-0070 decides. The engine never executes it;
- the **container and its security surface are complete** — magic, an encoding
  version independent of `tos-ir/v1`'s semantic version, canonical varints,
  explicit section and table lengths, every bound checked *before* the
  allocation sized from it, an artifact digest over the framed bytes, and
  fail-closed behaviour on an unknown version or an unknown tag;
- the **semantic payload covers only what the ceiling fixture exercises**. Every
  tag outside that coverage fails closed on both sides: the encoder refuses to
  write what it cannot round-trip, and the parser refuses to read a tag it does
  not know. The exact coverage is in §4 below.

So this is evidence for a **density and architecture decision**. It does not
close the completeness obligations of docs/43 §1 — a production encoder must
cover the whole schema, and that work is not started here.

The parser belongs to the **verifier path**: it consumes untrusted bytes and
materializes an internal `Module`, which the existing `tos_verifier::verify`
then traverses. The verifier still reaches its own conclusion from a module
value, exactly as docs/43 §5 requires. A production zero-copy or bounded-view
reader is deliberately **not** designed here; what §3 reports is what this
materializing reader actually costs.

The division between parser and verifier is deliberate. The parser validates
only what the *container* introduces — frame integrity, canonical form, UTF-8,
and references into the string and identity tables. It does **not** check that a
`TypeId` names a type or that a `BlockId` names a block. Those are semantic
references and the verifier's to check; a parser that quietly pre-checked them
would be a second verifier nobody reviewed.

## The frame

```text
0  .. 8    magic                 "TOSIMGx0"
8  .. 12   encoding version      u32 big endian, independent of tos-ir/v1
12 .. 20   payload length        u64 big endian
20 .. 20+n payload               canonical varint encoded sections
20+n ..    artifact digest       sha-256 over bytes 0 .. 20+n
```

Canonical rules, each of them a rule a reader can *check*:

- every integer is a minimal-length varint; a non-minimal spelling is refused
  rather than accepted and normalized;
- the string table is sorted by byte value and free of duplicates;
- the source-map identity table is sorted by its encoded tuple and free of
  duplicates;
- the payload length is exact — trailing bytes after the digest are refused.

The tag space is the digest scheme's tag space here — reusing it costs nothing
and made the encoder easier to review against `digest.rs`. It is **not** a
requirement: ADR-0070 §3 versions the storage encoding independently of the
semantic digest scheme, and identity is computed by the verifier from the
*reconstructed module*, never from the bytes it read or from a tag number the
image chose.

## 1. The fixture

One dependency module at the published `256 KiB` source ceiling — the same
generator `tos-arena-bound` uses, so these figures line up against the ones
`STAGE3_PROCESS_GRANT.md` already published rather than being a new fixture
nobody can compare against.

| | |
|---|---:|
| normalized source | 262 116 B |
| live `tos_ir::Module` | **12 864 160 B (12.27 MiB)** |
| current canonical stream | 5 561 951 B (5.30 MiB) |
| types / exports / functions | 2 268 / 2 268 / 2 268 |
| source-map entries | 11 338 |

## 2. The image, and the two reductions

| | Bytes | MiB | Against the image |
|---|---:|---:|---:|
| live `tos_ir::Module` | 12 864 160 | 12.27 | **33.13x** |
| current canonical stream | 5 561 951 | 5.30 | **14.32x** |
| **image** | **388 329** | **0.37** | 1x |

Of the image, `388 277 B` is payload and `52 B` is frame — magic, version,
length and the artifact digest.

Where the payload goes:

| Section | Bytes | Share |
|---|---:|---:|
| functions | 190 074 | 49.0 % |
| source map | 89 269 | 23.0 % |
| string table | 52 457 | 13.5 % |
| exports | 29 353 | 7.6 % |
| types | 27 081 | 7.0 % |
| constants | 4 | 0.0 % |
| header | 37 | 0.0 % |
| imports, capability imports | 2 | 0.0 % |

`4 546` distinct strings are interned across the module.

**Why the reduction against the canonical stream is so large.** The stream
writes every number as sixteen fixed bytes (`u128::to_be_bytes`) and every
enumerated value as its spelled name — `"i32"`, `"Relaxed"` — with a sixteen-byte
length in front of it. That is what ADR-0044's digest scheme v2 proposes to
replace with canonical varints, and this is the first measurement of what the
replacement is worth. The stream remains correct as a *digest* input; nothing
here proposes hashing something else.

## 3. Verifying the image

Measured in its own process, from a fresh arena, so no earlier phase's
high-water mark is inherited.

| Phase | Arena after | |
|---|---:|---|
| image bytes read as untrusted input | 390 624 B | 0.37 MiB |
| verifier-owned parse complete | 13 182 000 B | 12.57 MiB |
| **peak over read + parse + verify** | **29 697 360 B** | **28.32 MiB** |

The materialized `Module` is `12 781 456 B` (12.19 MiB) of live bytes — within
`0.7 %` of the `12.27 MiB` the frontend produced, which is the expected result:
the reader rebuilds the same value.

**This is the finding that matters most, and it is not the flattering one.**
The artifact shrank `33x`; the memory needed to verify it did not. Of the
`28.32 MiB` peak, `12.19 MiB` is the materialized module and the remainder is
the verifier's own working set. A compact image is a statement about storage and
transport. What a *residency* decision has to bound is this number, and moving
IR into an image does not by itself move it — which is exactly the separate
accounting ADR-0069 §7 records.

## 4. Coverage, exactly

What the ceiling fixture actually exercises, counted rather than asserted, and
every one of them encoded:

| Tagged variant | Occurrences |
|---|---:|
| `TypeDef::Nominal` | 2 267 |
| `TypeDef::Int` | 1 |
| `Constant::Int` | 1 |
| `Op::Read` | 4 534 |
| `Op::Binary` | 2 267 |
| `PlaceStep::Field` | 4 534 |
| `Terminator::Return` | 2 268 |

What the prototype **implements** — 63 tagged variants, the sequential core:

- `TypeDef`: `Unit`, `Bool`, `Int`, `Size`, `Duration`, `Text`, `Bytes`,
  `Option`, `Slice`, `Result`, `Array`, `Tuple`, `Function`, `Nominal`
- `Constant`: `Unit`, `Bool`, `Int`, `Size`, `Duration`, `Text`, `Bytes`
- `Op`: `Const`, `Aggregate`, `Variant`, `Read`, `Move`, `Write`, `Borrow`,
  `Drop`, `Binary`, `Unary`, `Widen`, `Call`, `Resource`, `RegisterCleanup`,
  `RunCleanups`, `Closure`, `CallValue`
- `Terminator`: `Return`, `Branch`, `BranchIf`, `MatchEnum`, `PropagateError`,
  `Trap`
- `CallTarget`: `Local`, `Imported`, `Predeclared`
- `PlaceStep`: `Field`, `Index(const)`, `Index(unknown)`, `DynamicIndex`
- `Operand`: `Value`, `Constant`
- and the closed small families in full: `IntKind` (8), `BinaryOp` (18),
  `UnaryOp` (2), `BorrowKind` (2), `ResourceKind` (9), `NominalKind` (2),
  `Visibility` (2), `PassMode` (3), `FunctionOrigin` (2), `Profile` (2)

What it **refuses**, and what a production encoder must therefore still cover —
33 tagged variants, the concurrency, region and capability families:

- `TypeDef`: `ConversionError`, `Event`, `Semaphore`, `Barrier`, `Latch`,
  `AtomicBool`, `AtomicU32`, `AtomicU64`, `Task`, `TaskResult`, `Shared`,
  `Region`, `DmaRegion`, `RegionMut`, `DmaRegionMut`, `Mutex`, `RwLock`,
  `MutexGuard`, `ReadGuard`, `WriteGuard`, `Channel`, `Capability`
- `Op`: `Spawn`, `Lock`, `Share`, `Join`, `Await`, `Cancel`, `Atomic`,
  `Capability`
- `AtomicOp` (9), `MemoryOrder` (5), `LockMode` (3)

Every one of those fails closed. An image naming one is refused, not skipped.

## 5. The source-map identity, interned

ADR-0044 proposes module-level identity **referenced rather than repeated**.
Measured here for the first time, on the same fixture:

| | Bytes |
|---|---:|
| identity written per entry, this same encoding | 1 869 322 |
| identity table (`13 B`) + entries (`89 256 B`) | **89 269** |
| saved | **1 780 053 B (1.70 MiB), `20.9x` on the section** |

`11 338` source-map entries carry **one** distinct identity. Logically every
entry still carries the seven docs/43 fields; physically all of them reference
one thirteen-byte record.

The comparison is against the same varint encoding with the identity inline, on
purpose. Comparing against `canonical_stream` instead would confound two
different changes — interning, and the move from sixteen-byte lengths to
varints — into a single number that would not tell you which one paid.

What remains is `89 256 B` for 11 338 entries: `7.87 B` per entry, an identity
index and two spans. The source map is still `23.0 %` of the payload *after*
interning, and that residue is span data, not repetition.

## 6. Time

Single run on the host, release build, same process as the phase it names.

| | |
|---|---:|
| canonical stream build (for comparison) | 5.78 ms |
| **encode** | **39.35 ms** |
| **decode** (verifier-owned parse) | **16.95 ms** |
| **verify** (existing semantic verifier) | **67.06 ms** |

Decoding is about a quarter of what verifying the result costs. A smaller image
bought with a slower verifier is a trade this project states rather than takes,
and on this fixture the parse is not where the time is.

## 7. The invariant

**After `encode` → verifier-owned `parse`, the semantic module digest must equal
the digest of the module that was encoded.**

```text
frontend module   sha256:83954abd4eecf92e3473bd6900b70079362500f90cf64c0170da2bbc10a6b367
parsed module     sha256:83954abd4eecf92e3473bd6900b70079362500f90cf64c0170da2bbc10a6b367
receipt binds to  sha256:83954abd4eecf92e3473bd6900b70079362500f90cf64c0170da2bbc10a6b367
artifact digest   sha256:0352926a8bd146eb56b6d1a702a0b7f46d38961f992879781471947d2daa6b6c
```

Equal. Without this every byte figure above would be a measurement of something
else, and a receipt bound to a digest the source never produced would be a cache
pretending to be a verifier.

The two digests are separate and both travel: the **semantic** digest says which
module, the **artifact** digest says which bytes.

## 8. Negatives

Nineteen cases, each refused, each for the stated reason.

**The frame:**

| Input | Refused with |
|---|---|
| wrong magic | `BadMagic` |
| encoding version 1 | `UnknownVersion(1)` |
| shorter than a frame | `Truncated("frame")` |
| body cut short | `Truncated("payload")` |
| declared length `u64::MAX` | `Oversized { declared: 18446744073709551615 }` |
| declared length past the bytes | `Truncated("payload")` |
| trailing bytes after the digest | `TrailingBytes(1)` |
| payload byte altered | `WrongDigest` |
| digest byte altered | `WrongDigest` |

**The payload, each one sealed with a *valid* artifact digest** — because an
attacker who controls the bytes controls the digest too, so a digest is
integrity and never authenticity, and the payload parser has to stand on its
own:

| Input | Refused with |
|---|---|
| repeated string-table entry | `NonCanonicalTable("string table")` |
| unsorted string-table entries | `NonCanonicalTable("string table")` |
| `0x80 0x00` — zero in two bytes | `NonCanonicalVarint` |
| varint past 128 bits | `VarintOverflow` |
| count past the declared limit | `CountExceedsLimit { count: 4294967295, limit: 4194304 }` |
| count past the bytes that remain | `Truncated("string table")` |
| empty payload | `Truncated("varint")` |
| string reference out of range | `OutOfRange { what: "string table" }` |
| string that is not UTF-8 | `BadUtf8` |
| unknown `TypeDef` tag `0xfe` | `UnknownTag { family: "TypeDef", tag: 254 }` |

The last one is the rule that makes partial payload coverage safe: a variant the
prototype does not implement is **refused**, never skipped.

`count past the declared limit` and `count past the bytes that remain` are the
two halves of "bounds before allocation". Every table entry costs at least one
byte, so a count larger than the bytes remaining cannot be honoured whatever the
declared limit says — and the reader learns that before it reserves anything.

**Totality.** The parser must return on every input, not merely on the ones
someone thought of:

| Sweep | Result |
|---|---|
| 4 096 resealed single-byte payload mutations | 3 018 refused, 1 078 parsed to some module, **0 panics** |
| 2 055 sampled prefixes of a valid image | **2 055 refused, 0 panics** |

The 1 078 that parsed are not a failure: a mutated byte can leave a well-formed
image of a *different* module, and deciding whether that module is admissible is
the verifier's job, not the parser's. What the sweep proves is that the parser is
total. The crate builds with `panic = "abort"`, so a panic would be a dead
process rather than a caught error — reaching the end of the loop is the
assertion.

## 9. What this does not settle

- **Not the production format.** Coverage is partial by declaration, the magic
  is experimental, and docs/43 §1's completeness obligations are open.
- **Not the engine.** No integration, and none before ADR-0070 is accepted.
- **Not residency.** §3 says what verifying one module costs; how many may be
  resident at once is the separate decision ADR-0070 §5 requires. Extrapolating
  one module's image to 256 is arithmetic, not a measurement, and the number
  that binds is the `28.32 MiB` verification peak rather than the `0.37 MiB`
  artifact.
- **Not ADR-0044's status.** Canonical varints and module-level source-map
  identity are used here as an **experimental candidate** for digest scheme v2
  and are now measured. That ADR remains Proposed and is not advanced by this
  document. ADR-0070 §3 versions the storage encoding independently of the
  digest scheme, so neither waits on the other.
- **Not the grant.** `RUNTIME_GRANT = 54 MiB` stays provisional.

## Reproduction

From `source/`, on the host. Each mode runs in its own process, because the
arena's frontier never falls and a second measurement in the same process would
inherit the first one's high-water mark:

```sh
cargo run --release -p tos-image-prototype -- --encode target/ceiling.tosimg0
```

```sh
cargo run --release -p tos-image-prototype -- --verify target/ceiling.tosimg0
```

```sh
cargo run --release -p tos-image-prototype -- --negatives target/ceiling.tosimg0
```
