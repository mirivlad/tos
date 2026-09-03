<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0060: How a TOS Core module invokes an operation on a capability

- Status: **Accepted (option A)** (Project Architect-approved)
- Date: 2026-08-19
- Decision level: 3 — it fixes how the language reaches the system, admits the
  first accepted interface schema as a class of document, and touches the
  determinism TOS Core V1 was closed on (ADR-0028)
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-19

## The gap, stated once

`import capability system.time.Clock as clock;` parses, checks, and lowers into
`tos_ir::CapabilityImport`. A module can **request** authority.

There is no way for it to **use** any. `extern fn` is reserved by the grammar
and rejected by both checker and verifier as `E1801_FFI_NOT_AVAILABLE`, and
docs/44 states why in one line: "an `extern` item names no accepted FFI
interface schema; V1 accepts none, so every `extern` item is rejected". No other
form reaches a capability: docs/42 §2 permits the imported name to appear "as a
value of its declared opaque type, a function parameter/effect name, or an
argument to an operation" — a value, never a callee.

So the language half of the capability contract exists as a declaration and the
half that does anything does not exist at all.

**This is the Stage 3 identity gate, not a convenience.** docs/37 asks: "do
textual processes exercise real capability/IPC contracts rather than running as
decorative scripts around privileged binary services?" Everything built in
Phase 4 — endowment, delegation, request and reply, the confused-deputy
property — is exercised by the **Rust runtime image**, which is a privileged
binary. The textual module computes a number. Today the honest answer to
docs/37's question is *no*, and no amount of further nucleus work changes it.

## What the accepted documents have already decided

This is not a blank page. Four documents already fix the shape, and reading them
together leaves one thing missing rather than a design space.

**docs/42 §2 describes the call.** "A capability operation is valid only when the
capability type, requested operation/right, resource range, **and the enclosing
`uses` effect** all match a declared interface contract." So an operation takes
the capability as an argument, is declared by an interface, and is enclosed in a
`uses` effect. It also says delegation and attenuation are "a typed interface
operation", and that `Region<T>` grants "originate only through a capability
operation whose accepted interface declares element type, alignment, access,
size, DMA domain, lifetime, and transfer/share rules".

**docs/39 reserves exactly that form.**

```ebnf
extern_decl = "extern" "fn" identifier "(" parameter_list? ")" "->" type effects? ";" ;
effects     = "uses" "[" identifier ( "," identifier )* ","? "]" ;
```

A named operation, typed parameters, a return type, and an effect list. It is
the shape docs/42 §2 requires, written down before anything could use it.

**The IR already carries the binding.** `Signature.effects: Vec<String>` —
"Declared capability effects, **by interface path**". The seam between a
module's declared effects and an interface's identity is in the artifact the
verifier reads.

**docs/42 §5 lists what is missing, as a checklist.** An accepted future FFI
version "must define a named interface schema, exact calling/ownership/region/
capability rules, source-map/provenance, target ABI, resource/cancellation
behavior, and safe-call guarantees. An `extern` item without that accepted
interface is rejected by both checker and verifier. It cannot be enabled by a
build flag, host library presence, or unsafe block."

The missing piece is a document, and its table of contents is already written.

## What must not be done about it

**A build flag, a host library, or an `unsafe` block that enables `extern`.**
docs/42 §5 forbids all three by name. The rejection is not a switch waiting to
be flipped; it is the absence of the thing that would make the call meaningful.

**A Rust FFI reaching `.tos` programs.** "A frontend written in Rust is an
implementation detail; its Rust FFI is not an FFI available to `.tos` programs."

**Making the Stage 3 system ABI part of the language.** `SYSTEM_ABI_V1` is
versioned separately for a reason, and docs/42 §2 says the concrete interfaces
"belong to later stages and must be separately versioned". An operation that is
part of TOS Core V1 is an operation that can never be versioned apart from it.

## Options

### A — supply the interface schema `extern` is waiting for

Write the first accepted interface schema, with the contents docs/42 §5
enumerates, and let `extern fn` bind to it. A module declares the operations it
will use, names the interface in `uses`, and the checker and verifier accept the
item because — and only because — an accepted schema declares it.

Nothing in the grammar changes. Nothing in the type system changes. What changes
is that `E1801_FFI_NOT_AVAILABLE` stops being unconditional: it becomes the
answer for an `extern` item naming *no accepted schema*, which is what docs/44
already says it means.

Costs: it is the largest single document this project has written, because every
item on docs/42 §5's list is load-bearing and the determinism question below is
genuinely hard. It also commits the shape before Stage 4's drivers exist, so the
schema must be written such that a driver interface is another instance of it
rather than a special case.

### B — methods on capability types

`clock.now()`. The grammar parses it as a field access followed by a call, but
resolution has no method dispatch: it would need either function-typed fields on
a capability — which docs/42 §2 forbids, a capability "cannot be a **record
field**" — or a new dispatch rule in accepted semantics.

Costs: a change to ADR-0028-accepted semantics, for a surface that expresses
nothing `extern fn` does not, and which **hides** the boundary docs/42 §5 says
must be visible "from the first implementation".

### C — predeclared intrinsics

Built-in `endpoint_send`, `process_create` and the rest, as functions the
language knows.

Costs: it makes the Stage 3 ABI part of TOS Core V1, so the two version together
forever. Stage 4's drivers then either become intrinsics too — the language
growing a function per device operation — or arrive as a second mechanism beside
the first. It is the fastest option to build, which under this project's stated
priorities is not an argument.

### D — the imported capability name as a callable value

Costs: docs/42 §2 admits the imported name as a *value*, not a callee, and one
import would then be one operation — a process authority carrying `create` and
`terminate` would need two imports of the same grant, splitting one capability
into two names against "the effective process grant is an explicit finite set of
object-specific rights".

## Recommendation

**A**, and the choice is not close: B, C and D each invent a second invocation
surface where the first is already reserved, specified in outline, and waited
for by name.

Three things belong in the decision rather than in the schema that follows it,
because they are what make A a decision rather than a work item.

**1. The schema is a document class, not a document.** `extern` binds to *an*
accepted interface schema. Stage 3's system operations are the first; a Stage 4
driver interface is the second, written to the same rules. If the first schema
is written as "the system ABI, in TOS Core", the second will not fit it.

**2. What remains deterministic must be said in this decision.** docs/40 fixes
evaluation order and ADR-0043 measures a budget against it, both closed while a
program's observable behaviour was a function of its inputs. An `extern` call
introduces the outside world. The rule proposed here: **the order of effects is
deterministic and the verifier proves it; the values effects return are not, and
nothing may depend on their being reproducible.** A module's own evaluation
order, its resource accounting, and its diagnostics stay exactly as docs/40 and
docs/41 fix them — what becomes non-deterministic is confined to values that
crossed the boundary, and it is visible in the source because the call sites are
`extern` and the function is marked `uses`.

**3. A blocking `extern` call is a blocking process.** ADR-0059 now defines what
waiting is, and docs/42 §5 requires "resource/cancellation behavior" of any
accepted schema. An `extern` operation that blocks makes its process not
runnable, is subject to the liveness rule, and returns the cancellation its
interface declares. The engine must therefore be able to leave and be re-entered
at a call boundary — which is the one genuinely new thing the engine has to
learn, and it should be settled here rather than discovered.

If A is accepted, the first schema is deliberately **narrow**: an interface over
the operations that already exist and are already evidenced — `endpoint_send`,
`endpoint_receive`, `endpoint_call`, `endpoint_reply`, `capability_attenuate`,
`capability_release`, `process_create`, `process_terminate` — and nothing
speculative. It is not "an FFI"; it is the first interface, and the FFI question
docs/42 §5 frames stays open for whatever else ever needs it.

## What each option costs to build

| | A — interface schema | B — methods | C — intrinsics | D — callable import |
|---|---|---|---|---|
| Grammar changes | none | new dispatch rule | none | none |
| Accepted semantics changed | `E1801`'s condition, as docs/44 already words it | ADR-0028 semantics | TOS Core V1 gains the Stage 3 ABI | docs/42 §2's "value, not callee" |
| Boundary visible in source | yes, `extern` + `uses` | no | no | partly |
| Stage 4 drivers | another instance of the schema | another dispatch case | a function per device operation | one import per operation |
| Separately versionable from the language | yes | yes | **no** | yes |
| Determinism question | must be answered | must be answered | must be answered | must be answered |

The last row is the one that matters most: no option avoids it, so it is not a
cost of A. It is a cost of letting a program reach the world at all, and it is
paid once.

## Boundary

Everything downstream of it in Stage 3 waits on this: a supervisor that reads
`/system/policy/` and launches what it says is a textual module that must call
`process_create`, and docs/37's identity question cannot be answered *yes* while
the only thing exercising capability and IPC contracts is a binary. Nothing
already built changes under any option — the nucleus, the ABI and the evidence
of Phase 4 stand as they are, and what is decided here is who else may reach
them.

## Superseded in part by ADR-0080

`Signature.effects` by interface path is unchanged and is what ADR-0080 builds
on. What that decision corrects is narrower: this ADR's mechanism assumed every
`uses` item was an `import capability` binding, because an import was the only
way a module could come to hold authority when it was written. TOS Core 1.1
admits an interface named directly, for capabilities that arrive as the value an
operation returned. The effect identity, the schema rules and the `extern` form
are otherwise as written here.
