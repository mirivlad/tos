<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS System Interface Schema — Version 1

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0060 (Project Architect-approved, 2026-08-19), which admits the
interface schema as a class of document and fixes the three things a schema
cannot decide for itself.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, and to
the language half of the model fixed by `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`
§2 under ADR-0028.

## 1. Role

`docs/39` reserves `extern fn` and rejects it as `E1801_FFI_NOT_AVAILABLE`
"until a later accepted FFI contract supplies an interface identifier and
capability rule". `docs/42` §5 lists what such a contract must define. This is
the first one.

It is **not** an FFI. It admits no C ABI, no Rust ABI, no libc, no dynamic
loader and no native extension, and `docs/42` §5's prohibitions are unchanged by
it. What it defines is narrower and is the thing TOS actually needed: how a
module invokes an operation on a capability it was granted.

## 2. A schema is a class of document

This is the first accepted schema and it will not be the last. A Stage 4 driver
interface is another instance of these rules, not a special case of this
document, and the rules below are written to be read that way.

A schema declares **interfaces**. An interface has a path, a capability type,
and a finite set of operations. Nothing else in the system may declare an
operation: an `extern` item naming no accepted schema is rejected exactly as
`docs/44` states, and that rejection is unchanged for everything this document
does not declare.

## 3. How a module reaches one

Three source forms, all of them already in TOS Core V1, and no new syntax:

```tos
import capability system.ipc.Endpoint as endpoint;

extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];
```

- **`import capability`** requests the authority and binds a name to it. It is a
  request, not a grant (`docs/42` §2): the launcher maps it to a concrete grant
  under policy, and a denied request is `CapabilityDenied` at startup.
- **`uses [name]`** on an `extern fn` names that binding. The interface is the
  imported capability's type, so an operation cannot be reached except through a
  capability of the interface that declares it. This is `docs/42` §2's rule
  stated as a mechanism: "the capability type, requested operation/right,
  resource range, and the enclosing `uses` effect all match a declared interface
  contract."
- **The first parameter is the capability**, of the interface's declared type.
  The remaining parameters are values. No parameter is a pointer, because TOS
  Core V1 has none.

A conforming `extern fn` is accepted by checker and verifier. One whose name,
arity, parameter types, result type or effect does not match a declared
operation is `E1801_FFI_NOT_AVAILABLE` with the reason named, which is the same
diagnostic as before for everything that was rejected before.

## 4. The interfaces this version declares

Only operations that already exist, are already reachable through
`SYSTEM_ABI_V1`, and are already evidenced. Nothing speculative: an interface
that declared an operation the system does not perform would be a contract
describing a system that does not exist.

### `system.ipc.Endpoint`

| Operation | Parameters after the capability | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|
| `endpoint_send` | `length: u64` | `i64` | 1 |
| `endpoint_receive` | *(none)* | `i64` | 2 |
| `endpoint_call` | `length: u64` | `i64` | 3 |

### `system.ipc.Reply`

| Operation | Parameters after the capability | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|
| `endpoint_reply` | `length: u64` | `i64` | 4 |

### `system.process.Control`

| Operation | Parameters after the capability | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|
| `process_terminate` | *(none)* | `i64` | 9 |

`process_create` is deliberately absent. It takes a module name and an
endowment, which live in the argument region (ADR-0058) — and TOS Core V1 has no
way to write into that region, because it has no pointers and this schema admits
none. An interface declaring it would be declaring an operation no module could
supply the arguments for. It arrives when there is a typed way to say what it
takes, and not before.

## 5. Results, and what a module may conclude from one

Every operation returns `i64`: the status `SYSTEM_ABI_V1` §4 assigns, unchanged
and unwrapped. A module reads a number the system produced, and this schema adds
no interpretation of its own — a status renamed on the way through would be this
document asserting something about a call it did not make.

Values a call returns beyond its status are **not** available to a module in
this version. `SYSTEM_ABI_V1` returns a second value in `rdx`, and a module has
no second result to receive it into; a tuple result would be a type-surface
change, and this schema does not make one.

## 6. Determinism, and what it costs

ADR-0060 fixes this, and it is the load-bearing sentence of the whole document:

> **The order of effects is deterministic and the verifier proves it. The values
> effects return are not, and nothing may depend on their being reproducible.**

Everything `docs/40` says about evaluation order is unchanged: a module's own
expressions evaluate in the order that document fixes, including the order in
which `extern` calls are made. What is not reproducible is only what came back
across the boundary. Two runs of the same module over the same inputs make the
same calls in the same order; they may receive different answers, and a module
that requires otherwise is wrong about the world rather than about this
contract.

Resource accounting (`docs/41`, ADR-0043) is unchanged: an `extern` call is
charged like any other call before it is made, so a module cannot exceed its
declared budget by leaving the process.

## 7. Blocking and cancellation

An operation of this schema may block, and one that does makes its **process**
not runnable — it is the same block ADR-0059 defines, reached through a
different door. It is therefore subject to the same rules:

- the liveness rule may cancel it, and the operation then returns `E_CANCELLED`
  like any other cancelled call;
- a process holding authority over the caller may end it while it waits;
- nothing waits without a cancellation path, because there is no path here that
  `SYSTEM_ABI_V1` §6 does not already cover.

The engine must therefore be able to leave at a call boundary and be re-entered
there. That is a property of the implementation and is stated here because a
schema whose operations block cannot be implemented by an engine that cannot.

## 8. Target ABI, ownership, and regions

The target ABI is `SYSTEM_ABI_V1` and nothing else: one mechanism, one path to
audit. This schema adds no calling convention of its own — it names which of
that ABI's operations a module may reach and under what authority.

**Ownership.** No parameter transfers ownership. A capability passed to an
operation is borrowed for the call and is still the caller's afterwards, which
is what makes an operation an operation rather than a consumption. Linear
transfer, where an interface declares it, is `CAPABILITY_V1` §4's case and no
operation here declares it.

**Regions.** No operation of this version takes or returns a region.
`docs/42` §2 requires a region grant to originate through an operation whose
interface declares element type, alignment, access, size, DMA domain, lifetime
and transfer rules; none is declared here, so none originates here.

## 9. Provenance and source maps

An `extern` call is an ordinary call in the IR and carries the same source map
entry as any other: source set, path, content id, frontend identity, profile.
The interface it reaches is in `Signature.effects` by interface path, so a
verified module states which interfaces it uses, and a reader of the artifact
learns that without executing it.

Nothing about a capability's concrete representation appears anywhere:
`docs/42` §2 requires authority to appear "in process identity, source maps, IR
imports, audit logs, and cache identity" while "the concrete secret/handle
representation does not", and an interface path is not a handle.

## 10. Conformance evidence

1. An `extern fn` matching a declared operation is accepted by checker and
   verifier; one differing in name, arity, parameter type, result type or effect
   is `E1801_FFI_NOT_AVAILABLE` with the reason named.
2. An `extern fn` whose `uses` names something that is not a capability import
   of the module is rejected, so an operation cannot be reached without
   requesting the authority it belongs to.
3. A module whose capability request is denied fails at startup with
   `CapabilityDenied` and never reaches the call.
4. The interface paths a verified module uses are readable from its IR without
   executing it, and match the `uses` effects of its declared operations.
5. Two runs of one module over one input make the same calls in the same order.
6. An operation that blocks makes its process not runnable, and a cancelled one
   returns `E_CANCELLED` to the module rather than a value resembling a result.
