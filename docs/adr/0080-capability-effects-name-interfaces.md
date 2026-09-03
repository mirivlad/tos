<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0080: Capability effects name interfaces independently of capability origin

- Status: **Accepted (Project Architect-approved)**
- Date: 2026-09-04
- Decision level: **3** — it changes the accepted source language. `docs/39`'s
  `effects` production admits a new form, the source-language version becomes
  **TOS Core 1.1**, and the frontend's effect model is corrected to the one
  `Signature.effects` has always carried. It changes no Tier 0 invariant, no IR
  schema, no ABI operation and no accepted ceiling
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-04
- Related: ADR-0028 (the language contract this amends), ADR-0060 (the interface
  schema, and `Signature.effects` by interface path), ADR-0061 (how an endowment
  binds to a module), ADR-0063 (an operation requiring two capabilities),
  ADR-0078 §4, §6 (capability sources, and the question left open there),
  ADR-0079 (the decision whose implementation reached that question).
  `docs/39` §2, §5, `docs/42` §1–§2, §4, `docs/43` §3, `docs/44`,
  `SYSTEM_INTERFACE_V1` §3, §4.1

## 1. The question, which ADR-0078 §6 left open on purpose

> A capability of an interface a module never imports — one delivered by a
> message, say — is a separate question and is not answered here.

Stage 4A reached it. A module holding the root `platform.pci.Bus` calls
`pci_function_claim` and receives a `platform.pci.FunctionConfig` — lawfully,
from an operation reached through authority it did import. It then cannot use
it. `SYSTEM_INTERFACE_V1` §4.1 makes an `extern`'s `uses` name an
`import capability` **binding of the enclosing module**, so the module must
write `import capability platform.pci.FunctionConfig as f;`, and that request
cannot be answered before the first instruction: the only lawful producer is the
claim, which runs afterwards. A parent cannot place one in a child's plan for the
same reason — `endow_for_launch` on that interface is itself an operation on it.

The recursion has no base case, and it is not about PCI. It is about **every**
authority whose object cannot exist before the process that obtains it.

## 2. Two things that were accidentally the same, and are not

Stage 3 had one way for a module to come to hold authority, so one declaration
did two jobs and nothing forced them apart. They are now separated:

| | **Authority request / startup binding** | **Interface effect declaration** |
|---|---|---|
| written | `import capability P as n;` | `uses [...]` |
| when | before the first instruction | at every call site, statically |
| what it does | requests a capability; creates a binding; introduces a value | states which capability interfaces a function may exercise |
| decided by | launch policy — answered or denied | the module's author, checked against accepted schemas |
| failure | `CapabilityDenied` at startup | a static diagnostic |
| grants authority | **yes**, if answered | **never** |

They coincide whenever a module uses only what it imported, which is every
module written so far. That coincidence is why the frontend could enforce "every
`uses` item is an import binding" without anyone noticing it was a second rule.

**`import capability` is unchanged in every respect.** It still requests, still
binds, still participates in startup identity and the endowment, still produces
`CapabilityDenied` when refused.

## 3. The decision

**`uses` may name an accepted capability interface directly.**

```tos
uses [platform.pci.FunctionConfig]
```

This means: *this function may perform operations whose capability positions
require the accepted nominal interface `platform.pci.FunctionConfig`.*

It does **not** mean, and an implementation that made any of these true would be
wrong: request one at launch; manufacture one; imply a well-known instance;
authorise a missing capability; license a *different* capability; or add
anything to the process's capability table.

**An interface effect with no capability value is no authority at all.** The
operation still requires an actual capability at the call site; the verifier
still proves its exact nominal type against the artifact; and the nucleus still
proves handle bounds, generation, object kind, rights and liveness at every
call. What this admits is a *declaration*, and a declaration has never been a
grant — `docs/42` §2 has said so since it was written.

### The existing form is preserved exactly

```tos
import capability system.ipc.Endpoint as endpoint;

fn f() -> i64 uses [endpoint] { ... }
```

A bare identifier in `uses` still resolves to a capability import binding under
the existing rules. **After resolution its semantic effect is that binding's
interface path**, so these two denote the same effect:

```tos
uses [endpoint]                 // endpoint imports system.ipc.Endpoint
uses [system.ipc.Endpoint]
```

The first additionally corresponds to a startup request, because the import
exists. The second does not. **There are not two effect identities.**

## 4. Effect identity is the interface path, and always was

ADR-0060 fixed `Signature.effects: Vec<String>` as "declared capability effects
**by interface path**", and the lowerer has resolved bindings to paths since it
was written. The artifact has therefore never been able to tell `uses [a]` from
`uses [b]` when both import one interface — the distinction existed in the
frontend and nowhere else, and no verifier could enforce it.

This decision makes the frontend agree with the representation rather than the
other way round. **No IR schema change**: `tos-ir/v1` already carries exactly the
resolved semantics, which §9 records was checked rather than assumed.

Authority *source* stays where ADR-0078 put it, and the two stay orthogonal:

```text
effect interface     which class of authority may be exercised
capability source    which actual capability value fills this position
```

Collapsing them again — in either direction — is the mistake this ADR exists to
prevent.

### One consequence, stated rather than discovered

Under the resolved model, a function declaring `uses [a]` may also exercise a
*different* binding `b` of the **same** interface. That was previously
`E1501_UNDECLARED_CAPABILITY_EFFECT`.

This **widens** what is accepted; it reinterprets nothing. Every program valid
under the old rule is valid now and means exactly what it meant, because the
artifact it lowers to is byte-identical — the old rule refused programs the
representation could not have distinguished anyway. A rule enforceable in one
frontend and in no verifier is not a property of the language.

## 5. TOS Core 1.1

**This changes accepted source syntax, so it is a language version.** Unlike
ADR-0078 it is not a consistency correction that happens to be invisible, and
calling it one would be the opposite of what `docs/44` requires of a new syntax
feature.

- **TOS Core major version remains 1.** `docs/42` §1's major rule is untouched.
- **TOS Core 1.0 remains supported, unchanged.** Every 1.0 module keeps its
  meaning, its diagnostics and its digest.
- **TOS Core 1.1 adds the direct-interface effect form and nothing else.**

```ebnf
effects     = "uses" "[" effect_ref ( "," effect_ref )* ","? "]" ;
effect_ref  = identifier | interface_path ;
interface_path = identifier ( "." identifier )+ ;
```

The dotted form is unambiguously an interface path: a bare identifier is a
binding and cannot contain a dot, so no source valid under 1.0 changes meaning.
It uses `docs/39`'s existing qualified-name production rather than a second
dotted-name parser.

**The version gate is the existing mechanism, not a new one.** The module header
already declares the source-language version, and `docs/44` already assigns
`E1602_UNSUPPORTED_LANGUAGE_MINOR` to "a minor version the frontend does not
implement". So a 1.0-only frontend rejects a 1.1 module *whole*, by its header,
before any of its syntax is read — never partly accepting the new form.

**A module gets what it declared.** A module whose header says `version 1.0` and
whose body uses a direct interface effect is refused with
`E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`, naming the feature and the minor it
needs. Without that, 1.1 syntax would work in a module that told every reader it
was 1.0, and the header would stop being a fact about the source.

**The artifact records the version the module declared**, not the newest the
frontend implements. With one supported minor those were the same string; with
two they are not, and a header that always said "1.0" would put two languages
under one identity.

## 6. Static checks

For `uses [P]` where `P` is dotted, the checker proves:

- `P` is declared by an accepted interface schema;
- it denotes a **capability interface** — not a record, a module or any other
  nominal type;
- the function's operations are permitted by that schema;
- each capability parameter has the exact required nominal interface;
- ordinary effect propagation is unchanged: a caller still declares every effect
  its callees require.

A typo or an undeclared path fails **statically**. A direct interface effect
cannot make an unavailable `extern` available: the operation must be declared by
an accepted schema, and `E1801_FFI_NOT_AVAILABLE` is unchanged for everything
else. No `AnyCapability`, no raw handle, no implicit coercion, and no effect
inferred because a value happened to arrive at runtime.

## 7. The verifier proves two dimensions, independently

**Effect.** The operation's required interface is in the enclosing function's
resolved effect set.

**Authority source**, per capability position (ADR-0078):

- `Import(index)` — the index names one of the module's own capability imports,
  and the interface matches, exactly as before;
- `Value(operand)` — the operand's type is `TypeDef::Capability(interface)` with
  the interface **equal** to the schema-required one for that position, under
  ordinary ownership and dominance rules.

**A `Value` position requires no import.** The two alternatives ADR-0078 §3
rejected stay rejected: no dummy startup import, and no unrelated import
reinterpreted as a licence.

## 8. Delegation becomes generic, and PCI is not special

A process holding a runtime `platform.pci.FunctionConfig` may use every
operation that interface declares, `endow_for_launch` included:

```text
Bus → pci_function_claim → runtime FunctionConfig value
    → endow_for_launch(function, plan, …) → child receives a startup binding
```

The parent needs no startup `FunctionConfig` import to delegate a runtime value.
**No "endow a function through the Bus" operation is added**, and the
configuration operations stay on `platform.pci.FunctionConfig` where ADR-0079
put them.

The same shape answers the general case ADR-0078 §6 named, without a second
decision: a capability of interface `X` that arrives by IPC is usable by a
function declaring `uses [X]`, provided the transfer rules admit it. Nothing in
the frontend or the verifier mentions PCI.

## 9. What was checked rather than assumed

- `Signature.effects` already carries interface paths, and the lowerer already
  resolves bindings to them — so no `tos-ir` version change is required, and
  none is made. Had the IR been unable to represent the resolved semantics, this
  ADR would have stopped and said so.
- The module header's version is already the **source-language** version, and
  `E1601`/`E1602` already gate major and minor. The 1.0/1.1 range needs no new
  version mechanism, only a second supported minor and the feature gate of §5.
- `E1608` is the next free code in `docs/44`'s `E16xx` module/version band.

## Architecture impact statement

- **Change level:** 3 — the accepted source language. **Invariants affected:**
  none amended. I-07 is strengthened: a declaration that never grants is now
  distinguishable in the language from a request that does.
- **Canonical representation:** unchanged for every 1.0 module, byte for byte.
  A 1.1 module records `1.1` as its language version.
- **Trusted-base impact:** none. No nucleus change.
- **Source-to-runtime impact:** the artifact records the declared language
  version rather than a constant, which is what makes two supported minors
  distinguishable in identity, cache and provenance.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** none claimed. This unblocks Stage 4A's remaining half.
- **Threat-model impact:** neutral by construction — §3's list of what an
  interface effect does *not* do is the threat statement, and §7 is where it is
  enforced twice.
- **Compatibility profile:** TOS Core **1.0 and 1.1**. A 1.0-only frontend
  refuses a 1.1 module by its header.
- **New dependencies:** none.

## 10. Conformance evidence

1. 1.0 positives: existing vectors are unchanged and still pass, including
   `uses [binding]`.
2. 1.1 positives: a direct interface effect on an ordinary function and on an
   `extern`; a runtime-obtained capability used through one.
3. A 1.0 module using a direct interface effect is
   `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`.
4. A 1.1 module naming an undeclared interface path in `uses` is refused
   statically; so is one naming a record or module path.
5. A direct interface effect grants nothing: a module declaring one and holding
   no capability of that interface cannot perform the operation, and fails where
   a module holding one succeeds.
6. `uses [a]` and `uses [system.ipc.Endpoint]` produce the **same**
   `Signature.effects`, and a 1.0 module's artifact is unchanged.
7. A frontend supporting only 1.0 refuses a 1.1 module with `E1602`, whole and
   by its header.
8. The declared language version reaches the module header, the image and the
   identity record.
