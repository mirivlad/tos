<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0062: Arguments an operation cannot take in a register

- Status: **Proposed**
- Date: 2026-08-20
- Decision level: 2 — it fixes what an accepted interface schema may declare a
  parameter to be, and therefore who is allowed to place bytes where the system
  reads them; it adds no operation, no status, no right and no syntax
- Project Architect approval: **not given**

## The gap, stated once

`SYSTEM_INTERFACE_V1` §4 says why `process_create` is absent, and the sentence is
the whole of this decision:

> `process_create` is deliberately absent. It takes a module name and an
> endowment, which live in the argument region (ADR-0058) — and TOS Core V1 has
> no way to write into that region, because it has no pointers and this schema
> admits none.

Every operation the schema declares today takes a capability and, at most, one
`u64`. That is not a coincidence of what was needed first: it is the largest
thing the schema knows how to describe. A module that could name a *string* or a
*list* could reach `process_create`; nothing else about the system is in its way.

**What this blocks is the rest of Stage 3.** ADR-0051 §3 puts supervision policy
in `/system/policy/` as canonical text, and the thing that reads it is a
supervisor — a `.tos` module that launches what the policy names, which is
`process_create` with a module path and an endowment. docs/37's identity question
is answered for operations that fit a register (`module-operation.sh`,
`process-control.sh`); the supervisor is where it is answered for the system's
own structure.

## What the accepted documents already decide

**A module never holds a pointer.** `docs/39` gives TOS Core V1 no pointer type,
and `SYSTEM_INTERFACE_V1` §3 states it as a property of the schema: "No parameter
is a pointer, because TOS Core V1 has none." Whatever is decided here cannot
change that.

**The nucleus never walks an address a process chose.** `SYSTEM_ABI_V1` §3:
"Arguments are values and handles, never pointers the nucleus dereferences
without bounds. Where an operation needs a buffer, the buffer is named by a
handle to a region the process already holds." The argument region is at an
address the *nucleus* chose and mapped, which is what makes reading it safe.

**The bytes already have a place.** ADR-0058 fixed the argument region and the
fixed offsets inside it: `CREATE_MODULE`, `CREATE_ENDOWMENT`, `CREATE_SELF_BINDING`.
Nothing new has to be invented to hold a module path — the question is only who
is permitted to put one there.

**The host already stands between the module and the ABI.** ADR-0060's engine
port hands `(interface, operation, arguments)` to a host, and ADR-0061's host is
the runtime image, which already turns each operation into its `SYSTEM_ABI_V1`
call. It is already the party that reads a capability handle out of a value and
puts it in `rdi`. Marshalling is the same act on a longer argument.

**Determinism is already split.** ADR-0060: the order of effects is
deterministic and the verifier proves it; the values effects return are not. A
larger argument travelling *out* changes neither half.

## What they do not decide

Whether an accepted schema may declare a parameter that is **not a scalar**, and
if so, what happens to it between the module's value and the system's read.

`SYSTEM_INTERFACE_V1` §8 says "No parameter transfers ownership" and "No
operation of this version takes or returns a region", both of which are
statements about *this* version rather than rules for the next. `docs/42` §5
requires an accepted schema to define "exact calling/ownership/region/capability
rules" — which is the checklist this would extend, not a prohibition.

## Options

### A — the schema declares `text`, and the host marshals it

An operation may declare a parameter of type `text`. The module passes a TOS Core
`text` value; the engine hands it to the host as `Value::Text`; the host copies
its bytes into the argument region at the offset the ABI fixes and passes the
length in the register the ABI assigns.

```tos
extern fn process_create(cap: system.process.Control, module: text) -> i64 uses [control];
```

The module never names an address. The nucleus reads the region it mapped, at an
offset it chose, for a length it bounds against a constant of the contract — all
three unchanged from today. What is new is only that the bytes got there from a
value rather than from a `set_transferred`-style write by the Rust image.

Costs: the boundary checker's parameter comparison currently admits only what a
`TypeSyntax::Name` resolves to, which `text` already is, so the surface change is
one entry in a table. The real cost is the rule that has to come with it —
**a schema declaring a variable-length parameter must declare its bound**, and
the refusal when a value exceeds it must be the schema's, not the host's
improvisation. Without that, "how long may a module's argument be" is answered by
whichever host runs it.

It does not by itself reach `process_create`, which takes a module name **and an
endowment**. It reaches half of it.

### B — the schema declares a record type, and the host marshals that

The endowment is a list of `(handle, rights, binding)`. A schema could declare a
record type for one entry and a list parameter of it, and the host would lay them
out at `CREATE_ENDOWMENT` exactly as the Rust image does today.

Costs: this is the option that actually unblocks `process_create`, and it is much
larger than A. A schema that declares a record type is declaring a **layout**,
and `docs/42` §5's list then has to be answered for it: alignment, size,
ownership, and what happens when a future version adds a field. It also puts a
type declaration in a schema, so the language's type surface and the schema's
begin to overlap — which is exactly the coupling ADR-0060 refused when it kept
`SYSTEM_ABI_V1` out of TOS Core V1.

The narrower form is worth naming separately: a record whose fields are all
scalars and whose layout is the ABI's, declared by the schema and *never* named
as a type by the module — the module writes a call with several arguments and the
host groups them. That form declares no type and admits no layout into the
language, and it reaches an endowment of a fixed arity.

### C — a region capability, and the module writes through it

`docs/42` §2's real answer: `Region<T>` originating from an operation whose
interface declares element type, alignment, access, size, DMA domain, lifetime
and transfer rules. The module holds a region capability and writes into it.

Costs: it is the largest option and the one the accepted documents point at for
the *general* case, but it is not the smallest thing that unblocks the
supervisor, and it drags in the region questions `IPC_V1` §9.6 is already blocked
on — the transfer mode of a region in a message is declared by no accepted
document. Doing C first means answering the region contract before there is a
service that needs one, which is the order this project has refused elsewhere.

### D — the supervisor stays a Rust component

Write the supervisor in the runtime image, reading `/system/policy/` and calling
`process_create` directly, and leave TOS Core unable to launch anything.

Costs: it is docs/37's failure condition restated as a plan. "Textual processes
exercise real capability contracts rather than running as decorative scripts
around privileged binary services" is the Stage 3 identity question, and a system
whose *launcher of everything* is the privileged binary answers it *no* for the
part that matters most. It is also not cheaper for long: Stage 4's driver
manifests want the same mechanism.

## Recommendation

**A now, with the bound as part of the decision; then B's narrow form when a
supervisor exists to need it.** Not C first, and not D.

A, because it is the smallest change that is on the path rather than beside it: a
variable-length argument travelling out, marshalled by the host that already
marshals every other argument, with the module still naming no address and the
nucleus still reading only memory it mapped. Every property the accepted
documents state about the boundary survives it verbatim.

The bound belongs in this decision rather than in the schema that follows,
because it is the one part a schema cannot choose freely: `SYSTEM_ABI_V1` §3
bounds a read by a constant of the contract, and a schema that declared a
parameter without one would be asking a host to decide how much of a module's
value the system will look at. The rule proposed: **a schema declaring a
variable-length parameter declares its maximum, and a value exceeding it is
refused before the call is made, with the same status an over-long inline payload
receives.** That makes the refusal the schema's, deterministic, and identical
across hosts.

B's narrow form — several scalar arguments the host groups into the layout the
ABI fixes — rather than B's general form, because it reaches `process_create`
without putting a layout or a type declaration into a schema. A schema that
declared a record type would be declaring something the language also declares,
and two declarations of one shape drift.

If A is accepted:

1. `SYSTEM_INTERFACE_V1` gains a section fixing which parameter types a schema
   may declare, and the bound rule above.
2. The frontend's mirror of the schema gains the type; the boundary checker
   compares it as it compares `u64` today.
3. The host marshals `Value::Text` into the argument region at the offset the ABI
   fixes, refusing an over-long value before the call.
4. Nothing about the engine changes: it already carries `Value::Text` and already
   hands arguments to a host it does not interpret.

## What each option costs to build

| | A — `text` | B — record/list | B narrow — grouped scalars | C — region | D — Rust supervisor |
|---|---|---|---|---|---|
| Module names an address | no | no | no | no | — |
| Schema declares a layout | no | **yes** | no | yes | — |
| Language and schema type surfaces overlap | no | **yes** | no | partly | — |
| Reaches `process_create` | half | yes | yes | yes | n/a |
| Region contract must be settled first | no | no | no | **yes** | no |
| docs/37's identity question | advanced | advanced | advanced | advanced | **answered no** |

## Boundary

This decides what an operation may *take*, and nothing about what one returns:
`SYSTEM_INTERFACE_V1` §5 keeps every result an `i64` status, and a second result
remains a type-surface change no schema makes. It decides nothing about regions —
`IPC_V1` §9.6 stays blocked on the separate question of what mode a region
travels in, which no accepted document fixes.

Nothing already built changes under any option. The nucleus, `SYSTEM_ABI_V1`, the
argument region and its offsets, the engine's port and the binding rule of
ADR-0061 stand as they are; what is decided here is whether a module may be the
one whose value ends up in a place the ABI already reads.
