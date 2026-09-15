<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0090: the resolved-closure import-edge ceiling

- Status: **Accepted (Project Architect-approved, 2026-09-15)**
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-15, granted before
  implementation
- Date: 2026-09-15
- Decision level: **2**. It adds one topology hard limit and one diagnostic
  code. No source form, no semantic operation, no schema field
- Related: `docs/44` §2 (the hard-limit table this joins), `docs/41` §6
  (`resource imports`, which this is **not**), `docs/42` §1 (module resolution),
  ADR-0071 §1a and §5 (dependency-first launch, the exact-byte reload rule this
  bounds the use of), ADR-0073 §4

## 1. What is bounded, and why it needed bounding

An imported call may be checked only against the export signature of the exact
dependency artifact this launch already verified. The launch therefore reopens
that artifact — authenticates its bytes against the trusted artifact digest,
reconstructs its export prefix, checks the caller's calls, and drops the
reconstruction. Nothing is retained: that is the property ADR-0071 §2's
fixed-shape record exists to protect, and it is not negotiable.

The cost of that discipline is one authenticated reopen per
caller-to-dependency relationship, **measured at approximately 7.1 ms** for a
ceiling-sized module. What was missing was any bound on how many such
relationships a conforming closure may contain.

It was not small. Under the ceilings already accepted — 256 modules, graph depth
64 — the densest admissible closure reaches **32 256** edges, which is
**≈229 s** of reopen work. Measured, not estimated: 1 920 edges over 64 modules
cost `18 287 ms` against a `4 662 ms` no-import baseline, and the per-edge cost
is stable across closure sizes.

## 2. Decision

`docs/44` §2 gains one line:

```text
resolved closure direct dependency edges    1024
```

It sits beside `module dependency closure 256 modules` and
`module/import graph depth 64`, because it is the same kind of statement: a
property of the closure's shape, checked before work proportional to it begins.

### 2a. What an edge is

One **unique pair**:

```text
(caller module identity, resolved dependency module identity)
```

in the exact resolved closure. Not an import declaration:

```tos
import up as first;
import up as second;
```

is **one** edge once both resolve to the same dependency, because it is one
relationship to every consumer of the graph — a launch authenticates that
dependency once and checks every call site in that caller against the single
reopened surface. Counting declarations would bill the source for a shape it
does not have.

An unresolvable or ambiguous import contributes no edge: `E1604` and `E1605`
own those, and an edge that does not exist may not consume the budget.

A capability-interface import contributes no edge. It introduces no module
dependency, and this decision does not invent a relation under which it would.

### 2b. Why not `resource imports`

`docs/41` §6 defines that field as **"maximum transitive module
dependencies"** — a count of modules reachable from one module. This is a count
of edges in a whole closure. One does not bound the other in either direction:
a closure of 256 modules each within a small transitive bound can still carry
tens of thousands of edges, and a single module with a large transitive bound
may declare one. Reusing the field would have bounded neither quantity while
appearing to bound both.

### 2c. Why 1024

Measured, from the tree this decision was made in:

| | |
|---|---:|
| modules in the repository (`.tos`) | 182 |
| direct dependency edges across all of them | **49** |
| largest single real closure | **3 modules / 4 edges** |
| largest direct-import count of any real module | **4** |
| densest previously admissible synthetic closure | 32 256 edges |

1024 is **256× the largest real closure** in the tree and bounds the reopen
amplification to ≈7.3 s before any optimisation of the reopen itself. 2048 was
considered and rejected: it doubles the bound on a measured expensive operation
to buy headroom nothing in the tree, or anything resembling it, is near.

**No language minor.** This adds no source form and reinterprets none, so
`E1608` does not apply and no module header changes.

## 3. `E1609_IMPORT_EDGE_LIMIT`

Stage `type`. Allocated here:

> the exact resolved module closure contains more than 1024 unique direct
> module-dependency edges.

Required fields: `limit`, `actual`. The first refusal carries `limit=1024`,
`actual=1025`.

It is reported on the source `import` declaration whose successfully resolved
**new unique** dependency edge first crosses the ceiling, walking the modules in
the order the resolver produced and each module's imports in source order, so
the same set always names the same declaration. A duplicate caller-to-dependency
relation is never counted twice, so it can never be the declaration reported.

It belongs to the `E16xx` family because the condition is a property of module
resolution and topology, beside import-not-found, ambiguity and cycles — not a
property of a module's declared resource envelope.

## 4. The launch enforces it independently

`docs/43` §5 makes a frontend's success no input to verifier acceptance, and
that applies here exactly: a launch does not admit a closure because a frontend
says it counted the edges.

Walking the dependency-first closure (ADR-0071 §1a), the launch deduplicates
each module's imported module identities, adds them to **one closure-wide
`usize`**, and refuses the first total above the ceiling with the existing
`V2001_LIMIT`, whose subject names the breached limit — `resolved closure direct
dependency edges`. No new verifier code is allocated.

The counter is fixed size and exists only for the launch. It does not enter
`VerifiedModuleRecord`, `VerifiedClosureManifest` or any execution state, so
ADR-0071 §1's flat launch peak and §2's fixed-shape record are untouched.

**It refuses where the ceiling is crossed**, before the dependencies of the
remaining modules are opened — the point of a topology quota is that the work it
bounds is not performed first.

## 5. The architecture this completes

> Imported-call authentication retains no variable-size export state across
> module turns. Each used dependency is reopened from the exact artifact bytes
> previously verified by this launch, checked against its trusted artifact
> identity, prefix-parsed transiently, and dropped. The total number of such
> caller-to-dependency relationships is bounded by the resolved-closure
> direct-edge ceiling of 1024.

That last sentence is what this decision adds, and it is what closes the
32 256-edge amplification.

### 5a. Why the retained frontier was rejected

The alternative was to verify dependencies in closure order and **retain** a
compact typed signature surface for each until its last importing caller had
been verified. It was rejected because it violates ADR-0071's lifetime rule:
after a module is verified only its fixed-shape record survives, and a
variable-length export surface living across module verifications is exactly
what that rule forbids. Measured, a wide fan-in would have retained 255 such
surfaces at once.

### 5b. Why a bounded cache was rejected

A fixed-size LRU of already-reconstructed surfaces was measured against every
shape, simulated over the exact reopen sequence. It does not work, and the
reason is structural: the access pattern is a scan, not a working set. At 256
modules a cache of sixteen surfaces removes **0.5 %** of the parses in the dense
case and **none at all** in a chain or a wide fan-in. It would have cost
retained state for nothing.

## 6. Architecture impact statement

- **Change level**: 2 — one topology limit, one diagnostic code.
- **Invariants affected**: none weakened. ADR-0071 §1's flat launch peak and
  §2's fixed-shape record are preserved; this is what makes preserving them
  affordable.
- **Canonical representation**: unchanged. No source form, no `tos-ir/v1` field,
  no `TOSBUNDLE` version.
- **Trusted base**: unchanged. The launch gains one `usize`.
- **Source-to-runtime**: unchanged for every closure within the ceiling, which
  is every closure in the tree by a factor of 256.
- **Recovery and rollback**: none required.
- **Stage gate**: none.
- **Threat model**: closes a resource-amplification path — a conforming closure
  demanding ≈229 s of authenticated reopen work — and opens none.
- **Performance**: bounds a measured expensive operation. The ceiling's own cost
  is recorded rather than assumed.
- **Compatibility profile**: unchanged; both profiles.
- **Dependencies, licence, patent**: none.
- **Tests**: the exact 1024/1025 source boundary, duplicate-binding
  deduplication, and the re-measured reopen and launch costs. On the launch
  side the boundary is exact on both sides and the closure's edges are
  **counted from the artifacts**, never taken from a fixture's label: a closure
  of exactly 1024 unique pairs launches, and one of exactly 1025 is refused
  `V2001_LIMIT`. The refusal is proved to land *before* the crossing edge's
  work by the reopen ledger rather than by position — the launch is handed a
  recording closure source, and after the crossing caller's own image is read
  nothing is read at all, so 1024 authenticated reopens were paid and the
  1025th was not.

## 7. What the exact boundary test corrected (2026-09-15)

The exact launch-side boundary above found §2a stated but not implemented. The
ADR's reason for counting two bindings of one dependency as one edge is that
"a launch authenticates that dependency once and checks every call site in that
caller against the single reopened surface" — and the verifier's reopen loop
walked `module.imports` by **declaration**, authenticating the same dependency
again for each binding of it. The quota counted relationships while the work was
paid per declaration, so one edge of budget could buy up to 255 authenticated
reopens and the amplification this ADR exists to close was not closed: 256
modules each binding one dependency 255 times is 255 edges — comfortably inside
the ceiling — and 65 025 reopens.

The implementation was corrected to the decision, not the decision to the
implementation: the reopen loop now opens each distinct dependency once per
caller and checks every call site reaching any binding of it against that one
surface. No acceptance decision changes — the same call sites are checked
against the same exports — and edges and reopens are now the same number, which
is what makes the ceiling a bound on the work.

## 8. What this ADR does not decide

**It does not repair `resource imports`.** A separate discrepancy was found
while measuring: the verifier checks `module.imports.len()` against that
envelope value, while `docs/41` §6 defines it as maximum *transitive* module
dependencies. That is a real and separate correctness item, recorded in
`PROGRESS.md`, and it is not redefined, reused or silently corrected here.

Imported `PassMode`, callable `PassMode` and Stage 4D-4 remain untouched.
