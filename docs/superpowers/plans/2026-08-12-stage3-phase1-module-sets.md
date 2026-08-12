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

- [ ] Admit a set of modules with one named entry module, keeping the existing
  single-module entry point working unchanged.
- [ ] Resolve the set with `check_module_set` and report a resolution refusal as
  the existing `Diagnosed { stage: Resolve }`, not as a new outcome.
- [ ] Order the closure deterministically, and refuse a cycle by diagnostic
  rather than by recursion.

### Task 2: Real dependency identity

**Files:**
- Modify: `source/crates/tos-pipeline/src/lib.rs`
- Modify: `source/crates/tos-core/src/lower.rs`

- [ ] Bind each `Import.module_content_id` to the content id of the module that
  actually resolved, computed rather than declared.
- [ ] Compute `dependency_digest` over the real ordered closure, so two sets
  that differ in a dependency cannot share a module digest.
- [ ] Type a cross-module call from the callee's exported signature instead of
  `unit`. A wrongly typed call is not a cosmetic defect: it is the frontend
  telling the verifier something untrue.

### Task 3: The verifier sees the set it is judging

**Files:**
- Modify: `source/crates/tos-pipeline/src/lib.rs`
- Modify: `source/crates/tos-verifier/tests/`

- [ ] Build a real `ResolutionSnapshot` from the resolved closure and verify
  every module in it, not only the entry.
- [ ] Negative tests: an import naming a module the snapshot does not provide;
  a snapshot whose content id disagrees with the module lowered; a call to an
  export the callee does not have.

### Task 4: The engine holds a closure

**Files:**
- Modify: `source/crates/tos-engine/src/lib.rs`
- Modify: `source/crates/tos-engine/tests/`

- [ ] Execute with a verified module set, resolving `CallTarget::Imported`
  against the closure and its receipts.
- [ ] Keep the receipt discipline: a module executes because its own receipt
  matches it, never because the module that calls it was verified.
- [ ] Account fuel, depth and allocation across the boundary against the entry
  module's declared envelope, and state in the test what that means.

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
