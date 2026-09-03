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

pub fn main() -> i64 uses [endpoint] {
    return endpoint_send(endpoint, 8u64);
}
```

**The imported name is the capability** (ADR-0061). It is what the launcher's
grant was bound to, and it is what a call passes as the operation's first
argument — so the request a module declares and the authority it uses are one
declaration, not two. An entry function's parameters are values; they are not
where authority arrives.

- **`import capability`** requests the authority and binds a name to it. It is a
  request, not a grant (`docs/42` §2): the launcher maps it to a concrete grant
  under policy, and a denied request is `CapabilityDenied` at startup.
- **`uses [...]`** on an `extern fn` names the **interface** the operation
  belongs to, so an operation cannot be reached except through a capability of
  the interface that declares it. This is `docs/42` §2's rule stated as a
  mechanism: "the capability type, requested operation/right, resource range,
  and the enclosing `uses` effect all match a declared interface contract."

  **Two spellings, one effect** (ADR-0080, TOS Core 1.1). A bare identifier is a
  capability import of the module and resolves to the interface that import
  requested; a dotted path is an accepted interface named directly. The second
  form exists because a capability may arrive as the **value an operation
  returned**, and such an interface has no import to name — the object did not
  exist when the process started, so no request could have been answered for it.
  Naming the interface declares which class of authority the function may
  exercise and requests none: the call site still supplies an actual capability,
  and §4.1 is where that is checked.
- **The first parameter is the capability**, of the interface's declared type.
  An operation may require more than one, and §4.1 says how it declares them;
  the remaining parameters are values. No parameter is a pointer, because TOS
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

Each interface declares **which kind of object a capability of it names**
(ADR-0061). `CAPABILITY_V1` §3 says a capability names "the endpoint, region,
process or interface publication it refers to"; this is where an interface path
is joined to one of those kinds, so that a launcher answering a module's request
can refuse a grant of the wrong kind at startup instead of letting the module
discover it at its first call.

**The kind is a check, not the mechanism that chooses a grant.** Which grant
answers which request is decided by the binding the module declared (ADR-0061),
because two imports of one interface are legal and a kind cannot tell them apart.

| Interface | Object kind |
|---|---|
| `system.ipc.Endpoint` | endpoint |
| `system.ipc.Reply` | reply |
| `system.memory.Authority` | memory authority |
| `system.process.LaunchPlanBuilder` | launch plan builder |
| `system.process.LaunchPlan` | launch plan |
| `system.process.Control` | process |

**Every operation declares the right each capability it takes must carry**
(ADR-0063). `docs/42` §2 requires that "the capability type, requested
operation/right, resource range, and the enclosing `uses` effect all match a
declared interface contract", and the right is the half this schema did not
state until an operation needed two capabilities and "which one may I receive
on" stopped having an obvious answer. Stating it for one operation and not the
others would leave the rule true of the newest thing only, so it is stated for
all of them.

### `system.ipc.Endpoint`

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `endpoint_send` | `system.ipc.Endpoint` with `send` | `length: u64` | `i64` | 1 |
| `endpoint_receive` | `system.ipc.Endpoint` with `receive` | *(none)* | `i64` | 2 |
| `endpoint_call` | `system.ipc.Endpoint` with `call` | `length: u64` | `i64` | 3 |
| `endpoint_send_text` | `system.ipc.Endpoint` with `send` | `message: string` (≤ 256) | `i64` | 1 |
| `endow_for_launch` | `system.ipc.Endpoint` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |

### `system.ipc.Reply`

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `endpoint_reply` | `system.ipc.Reply` with `reply` | `length: u64` | `i64` | 4 |
| `endpoint_reply_receive` | `system.ipc.Reply` with `reply`, then `system.ipc.Endpoint` with `receive` | `length: u64` | `i64` | 13 |

`endpoint_reply_receive` answers the call its reply capability names and then
waits for the next message on the endpoint its second capability names, without
running at CPL 3 in between. It is the only operation of this version taking two
capabilities, and the two are **separate authorities throughout**: each is
declared with its own interface and right, each is supplied from its own
`import capability` binding, and neither is derivable from the other. A reply
names one call (`IPC_V1` §4); an endpoint is a different object with different
rights and a different lifetime, and `CAPABILITY_V1` §2 has no operation that
makes one from the other.

It carries no protocol. It does not correlate the answer with the message it
then waits for, does not name a session, and does not know that the two have
anything to do with each other — `IPC_V1` §1 keeps request/reply in textual
libraries above the primitives, and this is a transport primitive that happens to
be what a server loop costs one crossing pair instead of two.

**What it does when something is wrong**, which is the part that had to be
decided rather than described:

- either capability failing to resolve, or lacking its declared right, refuses
  the whole operation: **nothing is delivered and no wait is entered**. A
  half-performed one would leave a caller answered and a server not waiting,
  which is the state this operation exists to make impossible.
- the reply is consumed by a successful delivery, exactly as `endpoint_reply`
  consumes it (`IPC_V1` §4). A second use of the same reply is refused **before**
  any wait is entered.
- a wait cancelled after the answer was delivered returns `E_CANCELLED`, **and
  the answer stands**. Cancellation ends a wait (ADR-0059); it cannot un-answer a
  caller that has already been answered.

### `system.process.Control`

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `process_terminate` | `system.process.Control` with `terminate` | *(none)* | `i64` | 9 |
| `process_wait_child` | `system.process.Control` with `wait_child` | `flags: u64` | `Result<system.process.ChildEnding, i64>` | 14 |
| `launch_plan_create` | `system.process.Control` with `create` | *(none)* | `Result<system.process.LaunchPlanBuilder, i64>` | 21 |
| `launch_plan_seal` | `system.process.Control` with `create` | `plan: system.process.LaunchPlanBuilder` | `Result<system.process.LaunchPlan, i64>` | 23 |
| `process_create_funded` | `system.process.Control` with `create`, then `system.memory.Authority` with `spend` | `plan: system.process.LaunchPlan`, `entry: string` (≤ 256), `grant: u64`, `self_rights: u64` | `Result<system.process.CreatedProcess, i64>` | 19 |
| `endow_for_launch` | `system.process.Control` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |
| `capability_attenuate` | `system.process.Control` with `none` | `rights: u64` | `Result<system.process.Control, i64>` | 5 |
| `capability_release` | `system.process.Control` with `none` | *(none)* | `i64` | 6 |

### `system.memory.Authority`

| Operation | Capabilities | Values after them | Result | `SYSTEM_ABI_V1` |
|---|---|---|---|---|
| `endow_for_launch` | `system.memory.Authority` with `none` | `plan: system.process.LaunchPlanBuilder`, `rights: u64`, `binding: string` (≤ 64) | `i64` | 22 |
| `capability_attenuate_scoped` | `system.memory.Authority` with `spend` | `bytes: u64` | `Result<system.memory.Authority, i64>` | 16 |
| `capability_release` | `system.memory.Authority` with `none` | *(none)* | `i64` | 6 |

### `system.process.LaunchPlanBuilder` and `system.process.LaunchPlan`

Two capability **types** with no operations of their own, and that is the shape
rather than an omission. Everything done to a plan is done *through* another
authority: 22 writes an entry through the capability being delegated, 23 seals
one through the creation authority that was required to make it, and 19 and 20
create from one through the same. There is no operation whose own interface is
either of these, so neither declares any.

Declaring the types is still this schema's job. They are what an operation's
result and a value parameter name, and a path no schema declares is not a type —
so a module could not write `Result<system.process.LaunchPlanBuilder, i64>` if
this section did not say what that path is. They also carry the object kind a
launcher checks a grant against (§4), which is why they are **two** kinds and
not one with a flag: a builder and a sealed plan declare different operations,
and answering a request for a decision that has been made with one that has not
is exactly the startup mistake that check exists to refuse.

**`endow_for_launch` is one operation declared by several interfaces**, and the
declarations differ only in the interface of their first parameter. That is what
`SYSTEM_ABI_V1` operation 22 is: **one** ABI selector for every kind of
authority there is, reached through the capability being delegated. The
alternative — one selector per interface — would make a finite ABI grow a number
every time an interface was added, and an interface set is open-ended while an
ABI is not.

It is declared on every interface whose capabilities may be a startup endowment,
and on no others. Regions, replies and plans themselves are refused by the
nucleus for the reasons `SYSTEM_ABI_V1` §5 gives, so there is no
`endow_for_launch` on `system.ipc.Reply`: declaring an operation the ABI always
refuses would be advertising something that does not work.

A module that endows two kinds of authority declares **two** `extern` items of
this name, differing in their first parameter's interface and in the binding
their `uses` names. Which one a call reaches is decided by the interface of its
first argument, which §4.1 already makes the operation's own capability — so a
call site and the instruction it becomes agree about which interface was
reached, and no capability value is ever erased to a common type.

**`process_create` is withdrawn from this schema, and `process_create_funded`
is what replaces it.**
`process_create` bound to `SYSTEM_ABI_V1` operation 8, which ADR-0076 §4
retires: it funded a process out of the boot's accounting anchor with no caller
presenting a `MemoryAuthority`. This schema could not carry its replacement for
one revision, and the reason was recorded rather than hidden: 19 requires two
capabilities and returns the child's *capability*, and every result this schema
declared was `i64`, so a wrapper would have handed a textual supervisor a number
where a child capability belongs.

Both halves of that gap are closed here. §5 admits a semantic result, so an
operation returns the authority it produced; and a heterogeneous endowment is
expressed by a launch plan, which is itself a capability an operation produced
and a later operation takes. Neither needed a new `tos-ir/v1` variant or a new
TOS Core type constructor: the IR has had `Capability` and `Result` since it was
written, and what was missing was the frontend admitting a constructed type in an
`extern` result and resolving an interface path to the capability type it is.

**Two rows may share one `SYSTEM_ABI_V1` operation.** `endpoint_send` and
`endpoint_send_text` are both operation 1, and differ in how the payload is
declared: one takes a length over bytes the caller placed itself, the other
takes the message as a `string` and lets §4.1's mechanism place it. That is not
two operations wearing one number — it is one operation with two declared
shapes, and a module picks the one whose parameters it actually has. It is also
the only way a TOS Core module can put text into the world in its own words,
which is what a supervisor's journal is made of.

## 4.2 The records this version declares

An operation returns the value it produced (§5), and some of what this system
produces has more than one part. A schema **record** is how those parts are
named.

**Not a new record ABI and not a language change.** A schema record is an
ordinary TOS Core nominal record type — the same type constructor a module's own
`record` declaration produces, carried in the artifact the same way and checked
by a verifier the same way. What is new is only *who declares it*: this schema
rather than a module, exactly as this schema already declares the interfaces and
operations a module may name.

**A module cannot construct one.** Nothing in the language names a schema
record's constructor, so the only way to hold one is to have been given it by
the operation that produces it. TOS Core V1's visibility rules are unchanged;
every field is readable, because a record declared to be returned exists to be
read.

**Field order is part of the contract.** A value's parts are matched to their
names by position, so the order below is what a host builds and what a module
reads, and a gate holds the two together.

### `system.process.CreatedProcess`

| Field | Type |
|---|---|
| `control` | `system.process.Control` |
| `instance` | `u64` |

Two facts, because neither is derivable from the other: a handle is an index in
one table and means nothing in another, and an instance identity is not
authority (ADR-0067 §7).

### `system.process.ChildEnding`

| Field | Type |
|---|---|
| `child_instance` | `u64` |
| `parent_instance` | `u64` |
| `ending_kind` | `u64` |
| `self_reported_status` | `Option<u64>` |
| `ended_by` | `Option<u64>` |
| `restart_generation` | `Option<u64>` |
| `ending_order` | `u64` |
| `ended_tick` | `u64` |

**The three optional facts are `Option`, not a value beside a flag.** ADR-0067
states the rule the other way round from a register-and-offset contract: absence
is the true value, and a zero would be a claim its caller never made. A record
carrying `status` and `has_status` side by side puts that rule in the reader's
hands; `Option<u64>` puts it in the type, and the translation happens once, at
the boundary, rather than in every supervisor that reads one.

## 4.1 What a parameter may be

### Capability parameters

An operation takes **one or more** capabilities, and they come first, in the
order §4 lists them. Each declares the interface it must be of and the right it
must carry; the first is the operation's own interface, which is the one the
instruction records and `Signature.effects` names (ADR-0060).

```tos
import capability system.ipc.Reply    as answer;
import capability system.ipc.Endpoint as inbox;

extern fn endpoint_reply_receive(
    reply: system.ipc.Reply,
    on: system.ipc.Endpoint,
    length: u64
) -> i64 uses [answer, inbox];
```

Each capability parameter is supplied from its own `import capability` binding
(ADR-0061), and the enclosing function's `uses` names every binding supplied.
A verifier proves, per capability parameter, that the binding named is an import
of the declared interface and that the enclosing function declares it — the check
it already made for one capability, made for each.

**A requirement may declare no right**, written `none`, and exactly one
operation does: `endow_for_launch` places a capability into a plan at rights the
caller asks for, and the nucleus intersects those with what the caller holds.
There is no right that could be declared — the honest answer would be "the ones
being delegated", which is an argument rather than a requirement, and any fixed
choice would be either too strong (refusing a delegation of `read` because the
caller lacks `write`) or a fiction. What is required is that the caller *hold*
the capability, which resolving it proves.

**They stay separate.** No capability is derived from another, no operation
merges two rights into one object, and an operation requiring two is refused
unless *both* were granted with the rights it declares. `CAPABILITY_V1` §3 admits
an object and rules out a class; two authorities that could not be granted apart
would be a class with two names.

`SYSTEM_ABI_V1` §3 fixes where they go: "Should an operation ever require two
capabilities, this contract assigns their positions in §5 order when that
operation is added" — so the ABI's register assignment follows this table's
order, and this schema does not repeat it.

### Value parameters

An operation's parameters after the capabilities are **values** (§3), and a value
of TOS Core V1 is what `docs/40` says it is. This version admits three:

| Declared type | How it crosses |
|---|---|
| `u64` | in the register `SYSTEM_ABI_V1` §5 assigns the operation |
| `string` | in the argument region, at the offset that ABI fixes, with its length in the register |
| a nominal capability type | in the register that ABI assigns, exactly as a capability parameter crosses |

**The third is new, and it is what a capability-valued result is for.** A
capability an operation *produced* — a launch plan, a child — is a value of the
module's, held in an ordinary binding, matched on like any other `Result`. It is
not an `import capability` and cannot be one: nothing granted it at startup,
because it did not exist at startup. So it is written where a value is written,
and it crosses the boundary the way a capability crosses, which is the same act
the host already performs on an import-supplied handle.

**Nothing about it is erased.** Its declared type is the exact nominal interface
— `system.process.LaunchPlan`, never a common capability type — so a plan cannot
be passed where a child belongs, the artifact records which interface each value
is of, and there is no `AnyCapability` anywhere in TOS Core. The engine carries
it without reading it, exactly as it carries an import-supplied one, and the only
place it becomes a number is the host's own table (`docs/42` §2).

**Every capability position may be supplied either way** (ADR-0078). A
capability parameter is filled from an `import capability` binding, or from a
value of that interface's capability type — one an operation produced. This
holds of the operation's *own* capability as much as of any later one, and it
holds of a schema entry without the schema saying which: what a position
declares is an interface and a right, and where the capability came from is the
call site's.

This section recorded the opposite boundary for one revision, and the record is
worth keeping: an operation acting on a capability of its own interface obtained
at runtime could not be declared here, because `tos-ir/v1`'s `Op::Capability`
named the operation's own capability as an import index and nothing else. That
was the *representation* narrowing accepted TOS Core V1 semantics, which already
admitted capability values and capability-derived authority — not a rule this
schema had chosen. ADR-0078 repaired the representation; this paragraph is the
schema no longer having to work around it.

**A runtime-supplied capability is checked, not trusted.** Its declared type is
the exact nominal interface the position requires; the artifact records that
type; and a verifier proves it against the artifact before anything runs. A
scalar, a constant, a value of a nominal record type, or a capability of a
*different* interface in a capability position is refused there — not by the
frontend that emitted it, which is the point of checking it twice.

**No import is required to license it.** A capability a module obtained at
runtime is reached through itself, not through some other authority declared for
the purpose. There is no capability parameter this schema declares that the ABI
does not act on, and no position filled by one object while another one licenses
it.

**A variable-length parameter declares its maximum, and the maximum is part of
this contract.** `SYSTEM_ABI_V1` §3 bounds every read by a constant of the
contract rather than by a number a caller chose, and a parameter without a
declared maximum would leave "how much of a module's value the system looks at"
to whichever host ran it. A value longer than the declared maximum is refused
**before the call is made**, with `E_BAD_ARGUMENT` — the same status an inline
payload past its own bound receives, because both are constants the caller knew
before it called.

Nothing here is a pointer and nothing here is a region. The module names a value;
the host places its bytes where the ABI already reads them, at an address the
nucleus chose and mapped. That is the same act the host already performs on a
capability handle, over a longer argument.

## 5. Results, and what a module may conclude from one

An operation returns **the value it produced**, and `Result<T, i64>` is the
refusal model. The error is the status `SYSTEM_ABI_V1` §4 assigns, unchanged and
unwrapped: a status renamed on the way through would be this document asserting
something about a call it did not make.

An operation that produces nothing but a status returns `i64` and always did.
One that produces a value returns it: `launch_plan_create` returns the plan it
made, `process_create_funded` returns authority over the child. What a module
receives is a TOS Core value it can match on, not a number it would have to
interpret against a register convention this schema does not describe.

**`T` is the semantic result, not whatever was in `rdx`.** The raw ABI splits a
result across a status register, a value register and fixed offsets in an
argument region; none of that is visible here, and the host bridge is where it
stops. A module names an operation and receives what the operation is *for*.

**This needed nothing new in the language.** `tos-ir/v1` has carried a
capability type and a result type since it was written, the image format encodes
and decodes both, and the engine's boundary has always returned a value rather
than an integer. What changed is the frontend: an `extern` result may now be a
constructed type, and an interface path written as a type resolves to the
capability type it is rather than to a nominal record that merely shares its
name. A reader of the artifact learns from the type table that a value is
authority — which is what `docs/42` §2 admits into provenance, the interface and
never the handle.

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

**Ownership.** Most parameters transfer nothing: a capability passed to an
operation is borrowed for the call and is still the caller's afterwards, which
is what makes an operation an operation rather than a consumption.

**One operation consumes, and says so.** `launch_plan_seal` takes a builder and
gives back a sealed plan naming the same object; the handle passed in stops
resolving, and a module that used it again would be refused rather than acting
on something stale. That is `CAPABILITY_V1` §4's linear case, declared here
because an interface is where it must be declared — a caller cannot see from the
ABI alone that a handle it still holds has gone.

**A creation does not consume its plan.** `process_create_funded` reads the
sealed plan and leaves it whole, which is what makes a restart the same decision
rather than a second one.

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
