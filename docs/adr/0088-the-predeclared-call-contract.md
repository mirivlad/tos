<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0088: the predeclared-call contract

- Status: **Accepted (Project Architect-approved, 2026-09-14)**
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-14, granted before
  implementation
- Date: 2026-09-14
- Decision level: **2**. It adds no source form, no type constructor and no
  artifact schema field; it makes one authority out of knowledge that was
  already accepted and already duplicated
- Related: `docs/39` §2 (the predeclared-function namespace), `docs/40` §3
  (conversion, wrapping arithmetic, index types), `docs/42` §1 (a module
  receives the language its header claims), `docs/43` §4 (a call supplies an
  exact ordered operand list) and §5 (verifier independence and the condition
  on a shared declarative table), ADR-0037 (`share`), ADR-0081 (device memory),
  ADR-0086 (DMA ordering), ADR-0089 (`E1217_CALL_ARITY_MISMATCH`)

## 1. The invariant

> Every TOS Core predeclared callable has one declarative source of its name,
> language-minor availability, arity, parameter type rule and result type rule.
> Frontend checking, lowering and the independent verifier consume that
> authority rather than maintaining semantic copies.

## 2. What was there instead

`docs/39` §2 fixes twenty-two predeclared function names. Four components met
calls of those names and each knew a different part of the contract:

| Component | What it knew | What it did not |
|---|---|---|
| checker | `to_*`'s result; `share`'s, MMIO's and DMA's operand rules | any argument of `to_*` or `wrapping_*`; any arity |
| minor gate | two hand-written name lists, one per feature | that one of them matched nothing for two minors |
| lowerer | a third list, for result types and IR forms | — |
| **verifier** | **nothing** | name, arity, operand types, result, form |

The measured consequences, all on the tree at `a25a784`:

```text
to_u8(1u64, 2u64)        accepted by checker, lowerer and verifier
to_u8()                  accepted by checker, lowerer and verifier
to_u8(true)              accepted by checker, lowerer and verifier
wrapping_add(1u64)       accepted by checker, lowerer and verifier
wrapping_add(1u64, 2i32) accepted by checker, lowerer and verifier
```

and, in the artifact, `CallTarget::Predeclared(_) => {}` — an empty verifier
match arm, so a **forged** artifact could name any string as a predeclared
operation, give it any operands and claim any result.

The minor gate's failure is the sharpest illustration of why a list per
component is not a contract. ADR-0081's device-memory call-site walk read the
wrong accessor and matched no call at all; the gate went on passing for two
minors because the *type* half of the same gate caught every module that could
hold an `MmioRegion`. It surfaced only when ADR-0086 added a feature with no
type of its own, whose gate was therefore the call site or nothing.

## 3. Decision

One declarative table, in its own crate, `tos-predeclared`. It is `no_std`,
data-only, and depends on nothing but the hash primitive and the IR schema
crate that both consumers already depend on. For each of the twenty-two
operations it states:

- the **name**, as `docs/39` §2 spells it;
- the **minimum TOS Core minor**;
- the **feature** it belongs to, as `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`
  names it;
- the **exact ordered parameter rules**, whose length is the exact arity;
- the **result rule**;
- the **IR form** the operation takes.

The rules are declarative, not concrete types. `wrapping_add`'s second operand
is "the same exact integer type as operand 0"; there is no one `TypeId` that
could say so, and a table that pretended there was would be a table for one
module. Each consumer evaluates a rule against its own representation — the
frontend over source types, the verifier over the artifact's own type table —
and neither takes the other's answer.

The table in full:

| Operation | Minor | Parameters | Result | IR form |
|---|---|---|---|---|
| `to_i8` … `to_u64` | 0 | (any exact integer or `size`) | `Result<D, ConversionError>` | `Call` |
| `wrapping_add`/`_sub`/`_mul` | 0 | (any exact integer, same exact integer type) | that type | `Call` |
| `share` | 0 | (satisfies the Shareable rule) | `Shared<T>` | `Op::Share` |
| `mmio_read_u8`/`_le_u16`/`_le_u32`/`_le_u64` | 2 | (`MmioRegion` or `MmioRegionMut`, `size`) | `u64` | `Op::MmioRead` of that width and byte order |
| `mmio_write_u8`/`_le_u16`/`_le_u32`/`_le_u64` | 2 | (`MmioRegionMut`, `size`, `u64`) | `unit` | `Op::MmioWrite` of that width and byte order |
| `dma_publish`, `dma_consume` | 4 | (`DmaRegion<T>` or `DmaRegion<mut T>`) | `unit` | `Op::DmaSync` |

### 3a. What the table is not

**Not behaviour.** It does not convert, wrap, share, touch a device or
establish an ordering edge. It says that `share`'s operand satisfies the
Shareable rule; ADR-0037's traversal still decides what is shareable, in the
checker over source types and in the verifier over the artifact.

**Not a replacement for the specialised operation families.** Operations that
lower to their own IR operation keep every independent check those decisions
gave them — ADR-0081 §8's device-memory obligations, ADR-0086's DMA ones,
ADR-0037's region rules. The table is *source-call contract authority*: the
name, the minor, the arity, the argument rules and the result.

**Not a reserved-word list.** `docs/39` §2 makes reserved words, primitives,
predeclared types and predeclared *values* unshadowable and stops there. A
predeclared function name is an ordinary identifier, so a module that declares
`fn share(a: i64, b: i64)` has declared a function and its calls resolve to it.
The lowerer always resolved it that way; the checker resolved it the other way
and typed such calls as an operation nobody had called. The table is consulted
only for a name the module does not declare.

## 4. The IR form is part of the contract

An artifact that wrote `share`, a device access or an ordering point as an
opaque `Op::Call` would be hiding exactly what three decisions put in the open:
`docs/43` §3 forbids hiding a shared-memory access behind an opaque call,
ADR-0081 §8 requires a device access to be its own verifier-visible operation
because an ordinary call may be eliminated, coalesced or duplicated and a
device access may not, and ADR-0086 §4 requires the same of an ordering point
for the narrower reason that a `Call` is what a backend is free to inline away.

So `Form` is in the table and the verifier refuses a `Call` naming an operation
whose form is its own.

**`Form` carries the device access's width and byte order for the same reason.**
The independent verifier meets an `Op::MmioRead` carrying a width and an order
and must decide whether that pair is one an accepted operation produces — a
three-byte access, a sixteen-byte one or a big-endian one is a device access the
language has no operation for. Asking the table is what keeps a second width
table out of the verifier; the table is data, so the verifier still derives its
own answer.

## 5. What each component does with it

**Checker.** For a call whose name the module does not declare: the name must
be in the table, the arity must be exact (`E1217`, ADR-0089), and each argument
must satisfy its rule. The code reported for a rule is the one the registry
already allocates for that *kind* of position — a byte offset that is not
`size` is `E1211` exactly as an array index is, two exact integer types that
disagree are `E1210`, and everything else is the residual `E1215` with the
fields ADR-0037 gives it. The result type is the result rule's.

**Minor gate.** One walk over the call sites of a module, asking the contract
selected by the module's declared minor whether each named operation exists
there. A later minor that adds an operation brings its gate with it.

**Lowerer.** The same rules, for the same call. Where a rule determines a type —
`size` at an offset, `u64` at a written value, "the same as operand 0" — that
type is the argument's context, so `docs/40` §3's contextual rule puts an
unsuffixed literal in it. The result type is the result rule's, so a call the
checker accepted with one result cannot be lowered with another.

**Verifier.** `CallTarget::Predeclared(name)` is no longer an empty branch. Six
obligations, in this order: the name is in the contract; the form is `Call`; the
operand count is exact; each operand satisfies its rule; the declared result is
the one the contract states; and — **last** — the operation belongs to the
declared minor. The order is ADR-0085 §17.7's: everything above the last is
wrong about the operation at any minor, so only an artifact right about all of
them has its version left as the one thing wrong with it.

Findings use the existing registry: malformed name, form or arity is
`V2011_CFG`, an operand or result type disagreement is `V2010_TYPE`, and the
minor refusal is `V2011_CFG` with the version named — following the
instruction-level precedent ADR-0085 and ADR-0086 set, where a minor refusal is
reported in the operation's own family rather than under the header code
`V2002_SCHEMA`.

## 6. The contract digest

`docs/43` §5 permits this at all, and states the condition:

> A shared declarative type/interface table may be used only if its content
> digest is input to both components; no frontend callback participates in
> verifier acceptance.

The table therefore has a canonical content digest: a domain tag, the selected
minor, and every operation it admits with each field length-prefixed, so no two
different tables encode the same bytes. Integer types are digested by their
**spelling**, so the digest describes the contract rather than a host enum's
discriminant order.

**The digest is an input to both components, and the binding is fail-closed at
compile time.** Each consumer's own source states the accepted digest for every
supported minor; each consumer's build script computes the digests of the table
that build actually links; and a `const` assertion in the consumer's source
compares them. A table edited under either component therefore fails **that
component's compilation** — not its test suite, which could be skipped, and not
a runtime path, which would have to decide what to blame. Neither consumer reads
the other's constant.

A `const fn` digest would let the assertion compute the hash itself and drop the
build script. It is not done here because it would require making `tos-hash`'s
SHA-256 const-evaluable — a change to a trusted-base primitive for a reason
unrelated to hashing. The build script computes; it decides nothing, and the
accepted values live where a reader of each component can see them.

**No artifact field carries it.** No `tos-ir/v1` field is added for it and
`VerifiedModuleRecord` is not enlarged. The module's `language_version` already
selects the applicable contract, and the digest binds the *implementations* to
the content of the contract that version selects — which is what §5 asks for. A
receipt field would be asking a different question (which contract an artifact
was verified under), and that question belongs to whichever decision needs it,
not to this one.

## 7. The MMIO carrier rule, which is ADR-0081's to state

Building the table surfaced one position with no contract at all. ADR-0081 §7
fixed a device write's shape as `(region, offset, value)` and the offset's type
as exact `size`, and fixed a read's result as `u64` at every width — and said
nothing about the written value's type. The frontend consequently checked
nothing there, and a value of any type was accepted.

**That ambiguity is resolved by ADR-0081 §7a, not here.** The carrier rule
belongs to the decision that owns the operations: every V1 MMIO read returns
exact `u64`, every V1 MMIO write takes exact `u64`, the offset stays exact
`size`, and the hardware transaction's width and byte order are decided by the
operation and its verifier-visible IR form rather than by the integer type of
the carrier value. No minor moves, because MMIO is already the 1.2 feature and
this states the type contract of operations 1.2 already has.

This table then encodes that rule **as authority**, which is the difference
between a contract and a guess: the frontend checks against it, the lowerer
gives an unsuffixed literal at either position the type it requires, and the
independent verifier proves it of an artifact.

**It has a visible consequence, and it is a repair.** A device offset or
written value spelled as an unsuffixed literal used to lower as `i32` — the
default of `docs/40` §3, because no position stated a type for it to take:

```text
mmio_write_le_u32(window, 4, 7)   before   Int(I32, 4), Int(I32, 7)
                                  after    Size(4),     Int(U64, 7)
```

An `i32` was standing where the contract requires a `size` and where a register
takes a `u64`. Since the position now states a type, the literal takes it.

## 8. Conformance

`docs/language/conformance/v1/reject/` gains vectors for a predeclared call of
the wrong arity, a conversion of a non-integer, and a wrapping operation over
two different integer types (which must report `E1210` and not `E1215`, the
precedence being conformance-visible). The existing device-memory and DMA
vectors keep their codes and their fields unchanged, which is the check that
the authority reproduces the accepted diagnostics rather than replacing them.

The table's namespace is bound to `docs/39` §2 by a test that reads the
document's own machine-readable inventory, so the two cannot drift.

## 9. Architecture impact statement

- **Change level**: 2 — one new data-only crate, no syntax, no semantics that
  were not already accepted, no schema field.
- **Invariants affected**: none weakened. `docs/43` §5's independence condition
  is met explicitly rather than by a component holding no knowledge at all.
- **Canonical representation**: unchanged. No source form is added, removed or
  respelled.
- **Trusted base**: the verifier gains a dependency on a `no_std`, data-only
  crate with no behaviour, permitted by `docs/43` §5 under the digest condition
  met in §6. It gains no dependency on the frontend.
- **Source-to-runtime**: unchanged for every program that was already valid,
  with one measured exception recorded in §7 — an unsuffixed literal at a
  device offset or written value now lowers as the type the contract requires
  instead of `i32`. Programs the new checks reject had no defined lowering.
- **Derived artifacts**: a module whose type table did not already hold `size`
  or `u64` gains that entry, because a predeclared argument position now states
  its type. Measured over the whole tree, exactly two artifacts move and neither
  moves an instruction: `tests/vectors/virtio-msix-wait/init.tos`, whose
  lowered module digest goes
  `sha256:4ecb8375bc3255d60a0c76a919d3b56f70be5251f60177cf5d0d3786037c1e82` →
  `sha256:e2fb96859105209ac35d411cdc340a07e069cf19eb506ea36108982142dd2d7c`
  as `Size` enters its type table (no evidence document pins that digest and no
  gate compares it; the source declares nine `size`-typed items and the table
  did not name the type), and `artifact_compatibility.rs`'s 1.2 fixture, which
  is repinned in place with the reason. **The Stage 4D-2 and Stage 4D-3 block
  vectors are byte-identical**, digest for digest, so the evidence that pins
  them is unaffected.
- **Recovery and rollback**: none required.
- **Stage gate**: none. Contract completeness, not stage work.
- **Threat model**: narrows accept paths — five measured forged-IR acceptances
  and five measured source acceptances — and opens none.
- **Performance**: no measured path is touched. The contract is a static table;
  no digest is computed at verification time.
- **Compatibility profile**: unchanged; both profiles.
- **Dependencies, licence, patent**: one new first-party crate,
  `GPL-3.0-or-later`, no third-party code.
- **Tests**: the table's own unit tests (namespace, rule well-formedness, minor
  cumulativity, digest), each consumer's independent digest statement — proved
  at compile time and again over all five minors in tests — the source and
  forged-IR suites of `predeclared_contract.rs`, and the conformance vectors of
  §8.

## 10. What this ADR does not decide

**Imported named-call signatures.** A call to another module's function still
carries no parameter types across `LoweringInterface`, `VerificationSurface`,
the bundle declaration or `ResolutionSnapshot`, so neither the frontend nor the
verifier can check one. That is a separate architecture question with its own
open sub-questions — source-diagnostic timing, snapshot authentication, and
persistence across the bundle boundary — and is deliberately untouched here.

**Callable and imported `PassMode`.** A function type carries no parameter
mode, so `Owned`, `SharedBorrow` and `MutableBorrow` remain indistinguishable in
a callable's type. Recorded, unchanged, not decided here.

**Nothing about the device-memory operation family's own obligations** — those
were measured while building this table beside them, found unmet, and closed in
the same slice against ADR-0081 §8's existing text rather than by any new
decision here. What this table contributed was the declarative form the
verifier's width and byte-order check reads.
