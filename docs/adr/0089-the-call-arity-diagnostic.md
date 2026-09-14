<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0089: the call-arity diagnostic

- Status: **Accepted (Project Architect-approved, 2026-09-14)**
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-14, granted before
  implementation
- Date: 2026-09-14
- Decision level: **2**. It allocates one diagnostic code and changes no source
  syntax, no semantic rule and no artifact schema
- Related: `docs/43` §4 (a call supplies an exact ordered operand list),
  `docs/44` §7 (the diagnostic registry), ADR-0037 (`E1215`), ADR-0087
  (`E1216`, and its §"What this is not" which left this code deliberately
  unallocated), ADR-0088 (the predeclared-call contract)

## 1. The gap, stated exactly

`docs/43` §4 has always required it:

> A call names a declared imported or local function signature and supplies an
> **exact ordered operand list**.

An operand list that is exact is exact in both directions. Nothing in the
accepted registry says what a source call that supplies a different number of
arguments receives.

The registry owns four type mismatches and each is about a *position* — an
argument, an index, a return, a binding. A call with three arguments to a
two-parameter function is not a wrong value at a position. It is a call with no
complete positional correspondence at all: its third argument corresponds to
nothing, and any diagnostic pairing it with a parameter would be describing a
correspondence the source does not have.

The consequence, measured on the tree at `a25a784`: the checker compared
arguments to parameters only when the counts already agreed, and said nothing
when they did not. `add(1i64)` against `fn add(a: i64, b: i64)` checked clean,
lowered, and was refused by the independent verifier as `V2011_CFG` — a
malformed *artifact*, reported against an artifact the source was entitled to
be refused before producing.

**This is a hole in the registry, not in the language.** The rule such a
program breaks is already accepted.

## 2. Decision

Allocate **`E1217_CALL_ARITY_MISMATCH`**:

> a source call supplies a different number of arguments than the resolved
> callee declares.

Required fields: `callee`, `expected`, `actual`, `context`.

`context` names the kind of callee, because the four are resolved differently
and a reader needs to know which resolution produced the count:

| `context` | The callee |
|---|---|
| `local named` | a function this module declares, or an `extern` operation it declares |
| `imported named` | a function another module exports |
| `predeclared` | an operation of `docs/39` §2's namespace (ADR-0088) |
| `callable value` | an in-scope binding, parameter or capture of a function type |

### 2a. Precedence

`E1217` is determined **before** positional argument-type diagnostics. With the
wrong number of arguments there is no complete positional correspondence to
type-check, so a per-position code would be reporting about pairs the source
never formed.

Once arity agrees, the existing precedence is unchanged:

```text
E1217_CALL_ARITY_MISMATCH       the counts disagree — and then nothing else
E1210_INTEGER_TYPE_MISMATCH     two exact numeric types disagree
E1211_INDEX_TYPE_MISMATCH       an index or byte offset is not `size`
E1215_ARGUMENT_TYPE_MISMATCH    a call or predeclared argument, otherwise
E1216_VALUE_TYPE_MISMATCH       the residual value-type code (ADR-0087)
```

A resolution finding still takes precedence over all of them: an unresolved
callee has no declared count to disagree with, and `E1202_UNKNOWN_VALUE_NAME`
owns that.

## 3. What this is not

**Not new language semantics.** `docs/43` §4 already required an exact ordered
operand list, and the independent verifier already refused an artifact that
lacked one. What was missing is the code that names the breach in *source*.

**Not a new source form**, so `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR` does not
apply and **no TOS Core minor is required**. `docs/44` §7 fixes that code to a
module using "a source form added in a later minor"; nothing here adds or
reinterprets one. No canonical module header changes on account of this
decision. The rule was checked against the accepted versioning text before
allocation; no existing versioning rule says otherwise.

**Not a replacement for the verifier's obligation.** The independent verifier
refuses a malformed local or value call's operand list as `V2011_CFG` and
continues to, because a frontend's refusal is not an input to verifier
acceptance (`docs/43` §5). The forged-IR negatives that prove it stay, and the
source fixtures that used to reach them have been rewritten to assert the
source refusal instead — a fixture the frontend now rejects cannot demonstrate
anything about the verifier.

## 4. What is implemented now, and what owns the code later

Implemented for three of the four contexts:

- **local named** — the declared signature is in reach;
- **callable value** — the function type carries its parameter list, which
  nothing in source checked before this;
- **predeclared** — the contract states the exact arity (ADR-0088).

**`imported named` is registered as an intended owner and is not checkable
yet.** A call to another module's function carries no parameter list anywhere
the frontend can reach at check time, and `LoweringInterface` — where an exact
signature does exist — is built during lowering, after checking. Pretending to
check it would mean either inventing a count or reporting a lowering `Gap`
where a source diagnostic belongs. It waits for the imported-signature
architecture decision.

## 5. Conformance

`docs/language/conformance/v1/reject/` gains one vector per checkable context:
a local named call, a callable-value call and a predeclared call, each with a
recorded `callee`, `expected`, `actual` and `context`.

## 6. Architecture impact statement

- **Change level**: 2 — one registry entry, no syntax, no semantics, no schema.
- **Invariants affected**: none. `docs/43` §4's requirement is served in source
  rather than only in the artifact.
- **Canonical representation**: unchanged.
- **Trusted base**: unchanged. The diagnostic is a frontend refusal; the
  verifier's independent obligations are untouched and continue to hold.
- **Source-to-runtime**: unchanged for every program that was already valid.
  Programs this rejects were refused by the verifier and never ran.
- **Recovery and rollback**: none required.
- **Stage gate**: none.
- **Threat model**: narrows an accept path — a malformed call reaching lowering
  — and opens none.
- **Performance**: no measured path is touched.
- **Compatibility profile**: unchanged; both profiles.
- **Dependencies, licence, patent**: none.
- **Tests**: the conformance vectors of §5, the source cases in
  `call_boundary.rs` and `predeclared_contract.rs`, and the forged-IR negatives
  that keep the verifier's independent refusal bound.

## 7. What this ADR does not decide

Imported named-call signatures, their source diagnostics, imported and callable
`PassMode`, and the authentication and persistence of a typed export surface
across the bundle boundary all remain open and are all deliberately untouched.
