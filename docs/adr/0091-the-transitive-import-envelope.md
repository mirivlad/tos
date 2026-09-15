<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0091: `resource imports` is a count of modules, and it is counted

- Status: **Accepted (Project Architect-approved, 2026-09-15)**
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-15, granted before
  implementation
- Date: 2026-09-15
- Decision level: **2**. It repairs enforcement of an existing contract and adds
  one diagnostic code. No source form, no semantic operation, no schema field,
  no language minor
- Related: `docs/41` §6 (the contract this enforces, unchanged), `docs/44` §2
  (the closure ceiling this makes the launch enforce), `docs/42` §1 and §4,
  ADR-0071 §1a and §2, ADR-0090 (the quantity this is **not**)

## 1. The contract was right and nothing enforced it

`docs/41` §6 has said one thing since V1 was accepted:

```text
imports:     integer,     // maximum transitive module dependencies
```

and the only check anywhere on the production path was, in the per-module
verifier:

```text
module.imports.len() > header.resource_envelope.imports
```

Those are different quantities, and the difference runs in both directions.

**Fail-open.** Measured end to end through `execute_set` before this slice:

```text
A imports B;  B imports C;  A declares imports: 1   =>  COMPLETED
```

`A` reaches two modules and declares room for one, and the closure ran. The
verifier saw one direct import and was satisfied. The shape is not bounded by
its depth: under the accepted ceilings a module declaring `imports: 1` may sit
above **255** transitive dependencies, so the enforced bound was short by up to
255×. The one thing this key exists to bound was unbounded.

**False rejection.** Measured on the same path:

```text
A imports B as one;  A imports B as two;  A declares imports: 1
  =>  V2001_LIMIT "more imports than the declared resource envelope allows"
```

One dependency, declared one, refused — and admitted by declaring `imports: 2`,
which is the proof that the quantity actually demanded was the declaration
count. ADR-0090 §2a had just settled that two bindings of one module are one
relationship, and this refused them as two.

**Neither direction is repaired by reinterpreting the key.** ADR-0071 read it
the strict way twice, in the act of declining to lean on it — "`resource
imports` in docs/41 bounds transitive module dependencies rather than the count
of `import` declarations, so that bound was not proved" — and ADR-0090 §2b
repeated it. Three accepted decisions already depend on the strict reading.

## 2. What the count is

One number per module: **unique resolved module identities reachable from it**.

- direct and indirect together, at any depth;
- the module itself excluded — dependency-first order and `E1606` make that
  structural rather than subtracted;
- two bindings or two declarations resolving to one module count **once**, which
  is the definition of a dependency ADR-0090 §2a settled and which cannot mean
  something else one paragraph away;
- a capability-interface import contributes nothing. It names an interface
  contract, not a module of this source set (`docs/42` §4), and is not resolved
  against the set at all.

Computed from the **resolved** graph, never from source spelling: two names that
resolve to one module are one dependency however they are written, and a name
that resolves to nothing is not a dependency.

**`docs/41` §6 does not change.** It was already correct; what changes is that
something checks it.

## 3. `E1705_IMPORT_ENVELOPE_EXCEEDED`

Stage `resource`. Allocated here:

> the module's exact resolved transitive dependency set contains more unique
> module dependencies than its declared `resource imports` value permits.

Required fields: `declared`, `actual`. **Not the dependency list** — it reaches
255 identities at the ceiling, and a diagnostic is not where a graph belongs.

Reported at the **value token of the module's `imports:` resource declaration**.
Unlike `E1609`, where the edge that crossed a topology quota is a meaningful
place, the breach here is a property of the module's whole reachable set; what
is wrong is the number it declared, and `E1700` and `E1704` already report on
that key.

**Only over a graph that resolved.** An unresolvable import, an ambiguous one, a
cycle, or a closure past the edge ceiling leaves no exact reachable set to
count, so those four own the earlier refusal:

```text
E1604_IMPORT_NOT_FOUND
E1605_AMBIGUOUS_IMPORT
E1606_IMPORT_CYCLE
E1609_IMPORT_EDGE_LIMIT
```

and only those four. A path that disagrees with a header (`E1603`) or a
qualified name a module does not declare (`E1203`) leaves the graph exactly as
resolvable as it was — the modules are the same and the edges are the same — and
suppressing a resource finding for an unrelated one would let a declared
envelope go unchecked because the set happened to get something else wrong.

### 3a. The stage and the pass are different things, deliberately

The diagnostic's **stage is `resource`**, because the semantic class of the
error is a module breaching its declared resource envelope. The **pipeline pass
that discovers it is module-set resolution**, because that is the first phase
holding the resolved graph the count is taken over.

This is the first place in the tree where the two differ, and it is written down
rather than left to be found. `docs/41` §7 defines stage as a property of the
diagnostic — its class, its code family, its position in the precedence order —
not as a claim about which pass had enough information to prove it. A frontend
that reported this at `type` because resolution happens there would be naming
the pass instead of the error.

**No language minor.** This adds no source form and reinterprets none, so
`E1608` does not apply and no module header changes.

## 4. Two checks, proving two different things

### 4a. The per-module verifier proves a necessary condition

A verifier holding one artifact sees a list of import names it cannot
authenticate and no graph at all. It therefore **cannot** prove a transitive
property, and no field added to the artifact would fix that without making a
producer's claim authoritative — which is the hole ADR-0090's slice closed and
which this does not reopen.

What it can prove is that the module's **unique direct** dependencies fit the
envelope, because they are a subset of the transitive set:

```text
unique_direct > declared  ⟹  transitive > declared
```

Deduplicated, so a module that spells one dependency twice is not refused for
how it wrote it. Necessary, never sufficient — a module one level above its
envelope through an indirect dependency passes this check, and the launch is
what catches it.

**Recoded `V2001_LIMIT` → `V2022_RESOURCE`.** A module breaching the envelope it
declared for itself is the class `V2022_RESOURCE` already owns, beside running
more cleanups at one exit than it reserved and declaring `workers > 1` under
Bootstrap. `V2001_LIMIT` belongs to published implementation ceilings, and the
table-count checks beside this one — including `module.imports.len()` against
`limits.modules` — are genuine ceilings and keep it.

### 4b. The launch proves the sufficient one

Walking the dependency-first closure (ADR-0071 §1a), after each module's
artifact has verified and its imports have been authenticated against modules
this same launch verified earlier:

```text
deps(M) = ⋃ over each unique direct dependency D:  {D} ∪ deps(D)
popcount(deps(M)) ≤ M.header.resource_envelope.imports
```

Every `deps(D)` is already final, because D was verified first.

**Every input is authenticated.** The import table comes from the artifact this
launch just parsed and hashed; a name resolves to a closure position only for a
module this launch verified, since `check_imported_calls` already refuses an
import with no evidence and one whose content identity disagrees with that
evidence. **`ResolutionSnapshot` is not consulted.** A declaration cannot shrink
the count: it cannot remove an entry from a signed-for import table, it cannot
make a name resolve to a different position, and omitting a dependency is
refused before the envelope is reached rather than making it stop counting.

## 5. The closure ceiling the launch was not enforcing

Found while auditing this, and repaired first because everything else depends on
it: `launch()` read `source.count()` and immediately sized the record table by
it — 464 bytes per module — before any check. Measured before the repair:

```text
CLOSURE 256 modules => ACCEPTED
CLOSURE 257 modules => ACCEPTED
CLOSURE 400 modules => ACCEPTED
```

`docs/44` §2 publishes `module dependency closure 256 modules` and nothing on
the production launch path enforced it. It is now the first statement of the
launch, before any allocation proportional to the closure, refused with the
existing `V2001_LIMIT` naming the limit in the table's own words — `module
dependency closure`. The effective bound is

```text
min(limits.modules, 256)
```

because a profile may publish a lower limit and honouring it is the point, while
a higher one is a versioned contract change rather than a configuration choice.

**A ceiling checked after the allocation it bounds is not a ceiling**, so the
test does not take the refusal's word for it: the launch is handed a recording
closure source, and the 257-module negative performs **zero** image reads.

The source and build path still has no enforcement of the 256-module ceiling at
all. That is recorded in `PROGRESS.md` as an open item rather than decided here.

## 6. Lifetime and memory

One `[u64; 4]` per closure position, and the ceiling above is what makes 256
bits enough. **Thirty-two bytes whatever a module declares**: a module importing
256 others costs what a leaf costs, so nothing here is sized by an
attacker-supplied count.

| | |
|---|---:|
| raw set storage at the 256-module ceiling | **8 192 B** |
| container cost (`Vec` header) | 24 B |
| permanent state after launch | **0** |

Launch-lifetime only. It reaches neither `VerifiedModuleRecord` nor
`VerifiedClosureManifest`, and it is not in `ResolutionSnapshot`, a bundle
declaration or any runtime process state — the same category as ADR-0090's
`usize`, two orders of magnitude larger and still 4.5 % of ADR-0071's measured
182 KiB of permanent launch state.

### 6a. Why not recompute per module

Measured against a per-module walk of a retained adjacency list, both algorithms
asserted to agree on every count:

| shape | modules | edges | inherited sets | recomputed |
|---|---:|---:|---:|---:|
| chain 64 | 64 | 63 | 1 138 ns | 22 994 ns |
| chain 256 | 256 | 255 | 4 309 ns | 305 876 ns |
| balanced DAG 256 | 256 | 382 | 3 844 ns | 56 775 ns |
| wide fan-in 256 | 256 | 255 | 1 851 ns | 8 910 ns |
| 1024 edges | 136 | 1024 | 5 186 ns | 9 233 ns |
| 256 modules, 1024 edges | 256 | 1024 | 5 457 ns | 12 321 ns |

Up to **71×** slower is the cheap objection. The real one is shape: recomputing
needs each module's import list retained across module turns — variable-length,
attacker-declared state, **512 KiB** at the ceilings — and ADR-0071 §2's
fixed-shape record rule forbids exactly that. A third form retaining nothing and
re-reading dependency artifacts would pay one more authenticated reopen per
edge: ≈7.4 s added to a ceiling launch, doubling it.

## 7. It is not ADR-0090's quantity

| | ADR-0090 | `resource imports` |
|---|---|---|
| what | unique direct caller→dependency edges | unique transitive dependencies |
| whose | the whole closure | one module |
| kind | topology and work ceiling | declared resource envelope |
| bound by | 1024, published | the module's own declaration |

Neither substitutes for the other, and both directions have witnesses:

- **Edge ceiling passes, an envelope fails.** A chain of sixteen is fifteen
  edges — 1.5 % of 1024, admitted without comment — and its last module reaches
  fifteen modules while declaring eight.
- **Every envelope holds, the edge ceiling fails.** Eight providers and 248
  callers of all eight: each caller reaches exactly eight and declares eight, so
  no envelope is breached, and the closure holds **1 984** edges.

## 8. Architecture impact statement

- **Change level**: 2 — enforcement repair plus one diagnostic code.
- **Invariants affected**: none weakened. ADR-0071 §1's flat launch peak and
  §2's fixed-shape record are preserved; a published ceiling that was not
  enforced now is.
- **Canonical representation**: unchanged. No source form, no `tos-ir/v1` field,
  no `TOSBUNDLE` version. Enforcement changes checks and never lowering, so no
  canonical source, IR artifact or Stage 4 digest can move.
- **Trusted base**: the launch gains 8 KiB of launch-lifetime state.
- **Source-to-runtime**: a module that over-declares is unaffected; a module
  that under-declares was already non-conforming and is now told so.
- **Recovery and rollback**: none required.
- **Stage gate**: none.
- **Threat model**: closes a fail-open on a declared resource bound and a
  missing closure-size gate; opens none. A hostile `ResolutionSnapshot` is not
  an input to either new decision.
- **Performance**: ≤ 5.5 µs of set work for the worst admissible closure.
- **Compatibility profile**: unchanged; both profiles.
- **Dependencies, licence, patent**: none.
- **Tests**: the source-side fail-open, duplicate-binding and exact-boundary
  shapes; the per-module necessary condition in both directions; the launch
  transitive check at the exact boundary, over a diamond, and against a declared
  resolution; the 256/257 closure boundary with a recording source; and both
  ADR-0090 separation directions.

## 9. What this ADR does not decide

**It does not enforce the closure ceiling in the frontend or the builder.** §5
repairs the production launch gate, which is the one that must hold against a
provider. Whether the source path should refuse a 257-module set with a
diagnostic of its own is a separate decision, recorded in `PROGRESS.md`.

**It does not change what a module may declare.** The envelope is still the
module's own statement, and a module declaring more than it uses is conforming.

Callable `PassMode`, imported `PassMode` and Stage 4D-4 remain untouched.
