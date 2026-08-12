<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 Phase 1: multi-module source sets

> **Scope rule:** this phase implements accepted contracts. It does not change
> TOS Core V1, `tos-ir/v1`, the verifier contract or the Stage 2 closure. If a
> task turns out to need one of those changed, stop at that boundary and say so
> — as Task 0 already did.

**Goal:** a boot module that imports another module from the capsule is
resolved, lowered with real dependency identity, verified, and executed on the
real boot path. Until that works there is nothing for a Stage 3 supervisor to
launch, because a service is a separate module.

**Architecture:** the composition already exists in pieces. `check_module_set`
resolves a set; `tos-ir/v1` represents imports and cross-module calls;
`ResolutionSnapshot` is the verifier's declared view of a source set. What does
not exist is the step that binds them, and the engine's ability to hold more
than one module.

## What was measured before planning

At `7b0847d`, on the production frontend:

| Fact | Where |
|---|---|
| A module import lowers to `tos_ir::Import` with `module_content_id: String::new()` | `lower.rs`, with the comment "the source-set step binds that identity" |
| A cross-module call lowers to `CallTarget::Imported` typed `unit` | `lower.rs`, "a single-module lowering knows the callee's name, not its signature" |
| The engine traps `RUNTIME_UNRESOLVED_IMPORT` on any `Imported` call | `tos-engine`, "a cross-module call needs the imported module's IR" |
| `ResolutionSnapshot::default()` — empty — is what the pipeline passes | `tos-pipeline` |
| `dependency_digest` is the digest of the empty list | `tos-pipeline` |
| `tos-ir/v1` cannot carry a named module constant or import one | `Constant` is a scalar pool; `Import` has no constant form |

The last row is why this phase was scoped to **function imports**. ADR-0052
has since answered the constant half without touching the IR: a constant is a
compile-time value, so it crosses a module boundary by substitution and needs no
representation at run time. Type imports are still out of scope here.

## Global constraints

- The verifier is handed a snapshot and IR, never the frontend's word. A
  multi-module path that verified one module and trusted the rest would give up
  the property Stage 2 closed on.
- Identity is computed over what was actually resolved. `dependency_digest` is a
  digest of the real ordered closure or it is a lie with a hash in front of it.
- Memory stays bounded and measured. The Stage 2 arena bound covers one module
  at the published ceiling; a closure needs its own measurement, not an
  extrapolation.

---

### Task 0: Make the constant gap honest, and stop at its boundary — **done**

**Files:**
- Modify: `source/crates/tos-core/src/lower.rs`
- Modify: `source/crates/tos-pipeline/tests/reference_path.rs`
- Create: `docs/adr/0052-module-level-constants.md`

- [x] Report a use of a module-level `const` as `construct=module-level const`
  rather than as `unbound place` or `unresolved value name`, which describe
  lowering data structures and send a reader looking for a typo in well-formed
  source.
- [x] Record in ADR-0052 that finishing the job needs a decision: what may
  initialize a constant, when it evaluates, and whether `tos-ir/v1` gains a
  named constant — the last being an extension of a closed contract.
- [x] Implement the chosen option. ADR-0052 accepted option A on 2026-08-12: a
  constant is a compile-time value, its initializer is a constant expression,
  and a use is the value substituted in place. `E1224_NONCONSTANT_INITIALIZER`
  refuses an initializer that would execute. Cross-module constant import is the
  same substitution and lands with Task 2's source-set step.

### Task 1: A source set reaches the pipeline

**Files:**
- Modify: `source/crates/tos-pipeline/src/lib.rs`
- Modify: `source/crates/tos-pipeline/tests/reference_path.rs`

- [x] `execute_set` takes `SetRequest { source_set, units, entry_path, entry }`
  and `execute` is its one-unit case, so every existing caller — the boot path
  included — is unchanged and provably so: a test asserts both paths produce the
  same module digest and content id.
- [x] Per-module checks run with each module's identity attached; the set is
  then resolved by `check_module_set`, and a refusal is the existing
  `Diagnosed { stage: Resolve }`. Missing import, cycle and path/name
  disagreement are covered by tests.
- [x] The closure is ordered breadth-first from the entry over imports in source
  order, so the order is the same on every run. Resolution has already refused a
  cycle, and the walk guards anyway rather than resting on that.
- [x] A request naming an entry the set does not contain is **not** a `Run`: it
  is `SetError`, returned before the first stage is announced. Reporting it as a
  `stage=resolve` refusal would have broken the accepted event contract, which
  requires `count=` and diagnostics there — and would have blamed a program for
  the caller's mistake.
- [x] The dependency digest is over the entry's real reachable closure. Two sets
  differing only in a dependency produce different module digests; an
  unreachable module changes nothing.

### Task 2: Real dependency identity

**Files:**
- Modify: `source/crates/tos-pipeline/src/lib.rs`
- Modify: `source/crates/tos-core/src/lower.rs`

- [x] `lower_module_in_set(source, schema, context, &[ResolvedImport])` binds
  each `Import.module_content_id` to the identity of the module that actually
  resolved. Without a dependency the field stays **empty** rather than
  plausible: the verifier can then tell "unresolved" from "resolved to that".
- [x] `dependency_digest` is over the real closure (Task 1), and each module of
  a set now gets its own — a dependency's digest is over its own closure, not
  the entry's.
- [x] A cross-module call carries the callee's declared result type, re-interned
  into the caller's type table. Nominal identity survives the crossing, because
  a nominal carries the content id of the module that declared it. A call to a
  name the dependency does not export is a named lowering gap rather than a
  `unit` the verifier would have to take on trust.
- [x] The closure is lowered dependencies-first, in a deterministic depth-first
  post-order, so every module is lowered after everything it imports.

### Task 3: The verifier sees the set it is judging

**Files:**
- Modify: `source/crates/tos-pipeline/src/lib.rs`
- Modify: `source/crates/tos-verifier/src/lib.rs`
- Modify: `source/tests/integration/tests/pipeline.rs`

- [x] The snapshot is built from the modules that were actually **lowered**,
  never from the request: one assembled from what a caller asked for would let
  the verifier confirm the caller's own assumption.
- [x] Every module of the closure is verified, not only the entry. A dependency
  whose IR the verifier never saw would be executing on its caller's receipt,
  and a receipt is a statement about one module.
- [x] `ResolutionSnapshot` gains `exports` — the function names each module
  provides. Not signatures: comparing those means comparing types across two
  modules' tables, which is a larger question, and claiming to have done it
  would be worse than not doing it.
- [x] Negative tests, all on hand-forged IR so the verifier is not merely
  agreeing with the frontend: an import the snapshot does not provide; an import
  claiming an identity the snapshot denies; a call to a name the resolved module
  does not export; and an empty snapshot leaving resolution unjudged, because
  silence is not acceptance of a claim it was never given the means to check.

### Task 4: The engine holds a closure

**Files:**
- Modify: `source/crates/tos-engine/src/lib.rs`
- Modify: `source/crates/tos-pipeline/src/lib.rs`
- Modify: `source/tests/integration/tests/execution.rs`

- [x] `run_set(&[Verified], entry, name, arguments)` resolves
  `CallTarget::Imported` against the set and nothing else: the engine never
  loads, searches for or fabricates a module. `run` is its one-module case.
- [x] Every receipt is checked **before anything runs**, not when a call reaches
  it — otherwise a program could choose which modules get checked by choosing
  which branch it takes. A module runs because its own receipt matches it.
- [x] The right name is not enough: a set holding another revision of the module
  under the same name is refused, because the caller was lowered and verified
  against a particular identity and running against another executes code it was
  never checked with.
- [x] One run, one budget. docs/41 section 6 admits a call only when the
  callee's declared contract fits the caller's envelope, so the entry's envelope
  governs and a boundary is not a way to obtain a second one. The test asserts
  the fuel limit is the entry's and that the callee's work is charged to it.
- [x] A trap carries its own resolved source-map entry across a boundary. An
  index is only meaningful in its own module's table, so a trap resolved against
  the caller's map would name a real line in the wrong file — which is worse
  than naming none. Tested: a divide-by-zero inside a dependency is located in
  the dependency.

### Task 5: The boot path runs a set

**Files:**
- Modify: `source/nucleus/src/runtime.rs`
- Modify: `source/host-tools/qemu-test/`
- Modify: `source/system/boot/`

- [ ] Read every module the capsule carries, not only the canonical boot file,
  and hand the pipeline a set.
- [ ] A boot module that imports a library module from the capsule runs on the
  real boot path and returns its answer.
- [ ] The serial contract is unchanged: the same `TOS.RUN.*` events, with the
  closure visible in `TOS.RUN.BEGIN` or an additive event under the delegated
  namespace rather than a new vocabulary.

### Task 6: Measure the closure

**Files:**
- Modify: `source/tests/arena-bound/`
- Modify: `PROGRESS.md`, `docs/evidence/`

- [ ] Measure the arena bound for a multi-module closure and record it as its
  own number. The existing single-module bound is not restated as a set bound.
- [ ] State the closure size measured and what is still unmeasured, so the next
  reader knows which claim is which.
