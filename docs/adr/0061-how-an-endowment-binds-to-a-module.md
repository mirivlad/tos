<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0061: How a process's endowment binds to the module it runs

- Status: **Proposed**
- Date: 2026-08-20
- Decision level: 2 — it fixes which declaration of a module is matched against
  a grant, what the launch record must carry for the match to be checkable, and
  which of two forms `docs/42` §2 already permits is the one TOS uses; it adds
  no operation, no status and no right
- Project Architect approval: **not given**

## The gap, stated once

ADR-0060 is implemented except for the part that makes it reachable. A module
can declare an operation of an accepted interface, call it, and have the
verifier prove both; the engine leaves at the call boundary and comes back
(`tests/integration/tests/interface_reach.rs`). Every operation
`SYSTEM_INTERFACE_V1` §4 declares takes **the capability as its first argument**
(ADR-0056), and there is no way for a module on the boot path to hold one.

So the runtime image hands its run `Unreachable`, and a module that reaches an
interface is refused by the only party that could have answered it. That is the
honest state, not a cautious one: nothing decides where a capability would have
come from.

## What the accepted documents already decide

More than half of it, and the half they decide is not the half that blocks.

**The unit of the request is the capability import.** `docs/42` §2:
`import capability system.time.Clock as clock` "declares that the module may
receive one opaque value named `clock` whose nominal capability type is
`system.time.Clock`. It is a request, not a grant. The process
launcher/supervisor, not source text, maps the request to a concrete grant after
policy/trust evaluation."

**The decider is the launcher, and it decides before the process runs.**
ADR-0051 §2 has the launcher reading `capability_imports` from the verified IR,
"sees exactly which capabilities the module intends to use, and grants or denies
**each** under policy"; its Consequence section requires that this be readable
"without executing it". ADR-0055 makes the same statement for the boot process,
whose endowment is the launcher's stated constant until `/system/policy/` exists
and which `CAPABILITY_V1` §2 requires to be *named in the audit record rather
than implied*.

**What a grant is.** `CAPABILITY_V1` §3: `object + rights + scope + lifetime +
generation`, never a class of objects. `docs/42` §2: "an explicit finite set of
object-specific rights and resource constraints".

**What a denial is.** `CapabilityDenied` at startup — "not fabricated as an
absence sentinel, a global singleton, an integer, or a successful empty
authority" (`docs/42` §2), and `SYSTEM_INTERFACE_V1` §10.3 makes it conformance
evidence: a module whose request is denied "never reaches the call".

**The transport exists.** The launch record carries a table of initial handles
with object, rights and scope (ADR-0055, `LAUNCH_VERSION` 3), and
`process_create` carries the same shape from a parent to a child, attenuated.

## What they do not decide, and it is three separate things

**1. The correspondence is not carried.** A launch-record entry says *handle,
object, rights, scope*. It does not say **which import it answers**. The
documents say the launcher maps a request to a grant; nothing says how that
mapping reaches the process. So the runtime image cannot know which of the
grants it holds is the one `import capability system.ipc.Endpoint as endpoint`
asked for — and with two endpoint grants it cannot even guess.

**2. An interface path is not connected to an object kind.**
`SYSTEM_INTERFACE_V1` §4 declares `system.ipc.Endpoint`. `CAPABILITY_V1` §3
names object types in prose — "the endpoint, region, process or interface
publication it refers to". `tos-launch` assigns `OBJECT_ENDPOINT = 1`. No
accepted document joins the three, so even "match a request to a grant of the
matching kind" is not derivable; it is a decision.

**3. `docs/42` §2 permits both candidate surfaces and chooses neither.** The
imported name may appear "as a value of its declared opaque type, **a function
parameter/effect name**, or an argument to an operation that requires that same
contract". A module reaching an operation through the imported name directly,
and a module receiving the capability as a parameter of its entry, are both
inside that sentence.

## Options

### A — the entry function's parameters are the delivery

```tos
import capability system.ipc.Endpoint as endpoint;
extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];

pub fn main(cap: system.ipc.Endpoint) -> i64 uses [endpoint] {
    return endpoint_send(cap, 8u64);
}
```

The launcher grants per import, as `docs/42` §2 requires. The runtime binds the
entry's declared parameters, in order, to the grants, and a parameter with no
grant of the declared type is `CapabilityDenied` at startup.

Costs, and the first is structural. **It needs two correspondences where the gap
is one.** The missing one is imports↔grants; A adds imports↔parameters on top of
it, and nothing in any accepted document says a module's import list and its
entry's parameter list correspond at all. A module could import two capabilities
and declare one parameter, or declare them in a different order, and no rule
would be broken — so the runtime would be reading a correspondence the language
does not assert.

It also puts the authority in the signature. `import capability` then does
nothing but supply a name for `uses`, and the declaration that actually decides
what the module holds is `main`'s parameter list — two places declaring one
fact, which is the drift `docs/45` warns about and ADR-0051 §1 refused to
introduce for manifests.

And it does not scale down the call chain: a capability used four functions deep
travels as a parameter through all four, so every intermediate signature carries
authority it does not use. A supervisor holding four capabilities has an entry of
arity four and four parameters threaded through its body.

### B — the capability import is the binding, and the imported name is the value

```tos
import capability system.ipc.Endpoint as endpoint;
extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];

pub fn main() -> i64 uses [endpoint] {
    return endpoint_send(endpoint, 8u64);
}
```

One list, and it is the list `docs/42` §2 already calls the request. The launcher
grants per import; the runtime binds each import to its grant before the entry
runs; the imported name is in scope as a value of its opaque type wherever the
enclosing function declares the matching `uses` effect. `main()` keeps the
signature it has, so the canonical boot text is unchanged and a module's entry
does not become part of the launch contract.

Costs: the engine gains a per-run binding of capability imports, and the lowerer
gains an operand form for an imported name. `tos-ir/v1` already carries
`CapabilityImport`, so nothing about the closed Stage 2 IR schema changes — but
the engine's `run_set` grows an argument beside the entry arguments, and the
verifier gains a check that a module using an imported name declares the
matching effect (which is `docs/42` §2's rule it already enforces for `extern`
items, applied to one more site).

### C — an operation that fetches the endowment by index

A thirteenth-class operation returning the *n*th capability the process holds.

Costs: it is naming authority by a number, which is exactly what the
confused-deputy gate refuses and what `CAPABILITY_V1` §7.6 exists to prevent. It
also puts the correspondence inside the module — the module would have to know
which index the launcher used, so the mapping the launcher owns would become a
constant compiled into the thing it is supposed to constrain. Rejected on the
same ground as ADR-0056's rejected alternatives, not on cost.

### D — decide nothing yet; write the supervisor first and let it show the shape

Costs: the supervisor is itself a `.tos` module and it must call
`process_create`, which takes a process capability first (ADR-0056). So D asks
the blocked thing to demonstrate the shape of its own unblocking. It is not an
option, it is the gap restated.

## The second half of the decision: how a grant answers a request

Independent of A/B, and both parts are needed before anything can be built.

Three facts decide most of it, and two of them were established by reading the
implementation rather than by argument.

**Two imports of one interface are legal.** A module declaring
`import capability system.ipc.Endpoint as input` and
`... as output` checks clean and verifies. So **the interface path is not a
unique key**, and any rule that matches on it alone cannot tell a process's two
endpoints apart — which is not an exotic case, it is the first realistic one.

**The binding is already part of the module's identity.** `tos-ir/v1` carries
`CapabilityImport { interface, binding, ty }`, and the module digest covers
*both* the interface and the binding. The name a module bound its request to is
therefore already verifier-visible, already in the receipt, and already in cache
identity — it is not a frontend-local convenience, and renaming it is already an
identity change the system notices.

**`PROCESS_IDENTITY_V1` §7.3 requires a key.** It is an accepted Tier 2
contract: "A denied capability appears as a **difference between the requested
and granted sets**, and the process's `CapabilityDenied` startup failure **names
it**." A set difference needs element identity, and a refusal that names the
denied request needs something to name it by.

### i — by the name the module bound the request to

The requested set is `{binding → interface}`, read from the verified IR without
executing it (ADR-0051). The granted set is a subset of it, and each launch-record
entry says which binding it answers. The difference is what was denied, computed
rather than inferred, and `CapabilityDenied` names it — which is
`PROCESS_IDENTITY_V1` §7.3 satisfied by construction rather than approximated.

Bindings are unique inside a module because they are names in scope, so
`(module, binding)` is a key. `docs/42` §2's restrictions on the imported name —
not a `const`, not a record field, not a serialized value, not an equality key —
are restrictions on what the **module** may do with it; the launcher naming a
request by the module's own declaration of that request is the sentence
immediately above them: "The process launcher/supervisor, not source text, maps
the request to a concrete grant."

Costs: a source-level identifier becomes part of the launch contract, so renaming
`endpoint` to `ep` is a change policy must follow. That cost is smaller than it
looks, because the rename already changes the module digest and is therefore
already a different module to every other part of the system.

### ii — by object kind, in the order the imports are declared

The *n*th import is answered by the *n*th grant, refused unless the grant's
object kind is the kind the import's interface declares.

Costs: the kind check is necessary under **every** option and is not a matching
rule — the first fact above is that it does not discriminate between two imports
of one interface. Order as the key makes source order semantically load-bearing,
which no accepted document says it is, and makes reordering two import lines a
silent change of which authority each name receives. And to satisfy
`PROCESS_IDENTITY_V1` §7.3 the launch record must additionally carry the *denied*
positions as holes, so that the two sets stay alignable — an addition that exists
only to recover an identity the binding already has.

### iii — an explicit request identifier declared in source

Costs: it needs either new grammar (`docs/39`) or a new IR field (`tos-ir/v1`,
closed by ADR-0028) — a Level 3 change — and it is redundant with the binding,
which is already a declared, unique, digest-covered identifier for exactly this
request.

### iv — `/system/policy/`

Not a rival: policy is **who decides**, not what the arrow is drawn with. A
policy still has to name the import it grants, by binding, by position or by an
identifier. ADR-0051 §3 already places it in `/system/policy/` as canonical text,
and it arrives when there is a supervisor to write it — which needs this decision
first.

**The kind check, the key and the decider are three axes, not three options.**
The kind check is required under all of them and needs the path→kind mapping that
gap 2 names. The key is i, ii or iii. The decider is the launcher's stated
constant now (`CAPABILITY_V1` §2) and policy later, over whichever key is chosen.

## Recommendation

**B for the surface, i for the key**, with the kind check under both, the
launcher's stated constant as the decider now and `/system/policy/` as the
decider later.

B, because the gap is imports↔grants and B closes exactly that gap, while A
closes it and invents a second one. Because `docs/42` §2 calls the import the
request, and a design in which the request is not what receives the grant leaves
`import capability` as decoration. And because authority that travels as a
parameter through functions that do not use it is authority spread by the
calling convention rather than by delegation, which is the opposite of what
`CAPABILITY_V1` §4 makes delegation cost.

i, because it is the only key that satisfies `PROCESS_IDENTITY_V1` §7.3 without
adding anything: the requested and granted sets become comparable and a denial
becomes namable, using an identifier the artifact already carries and the digest
already covers. Position needs denial holes added to the launch record to
recover an identity the binding already has, and cannot distinguish two imports
of one interface without making source order load-bearing. An explicit
identifier needs a closed contract reopened to add what is already there.

The correspondence travels in the launch record **explicitly**, because a record
that says which import an entry answers can be read by a policy layer later
without a version change, and one that says only where in the list cannot.

**One thing this costs nothing.** `tos-ir/v1` already declares
`Op::Capability { import: usize, right, operands }`, and the verifier already
bounds-checks `import` against `capability_imports.len()`. The operand form B
needs is inside the closed Stage 2 IR schema, so ADR-0028's contract is not
touched.

If B and i are accepted:

1. `SYSTEM_INTERFACE_V1` §4 gains the object kind each interface's capability
   type is, which closes gap 2 in the document that owns it and supplies the
   check every key needs.
2. The launch record's capability entry gains **the binding it answers**.
   `LAUNCH_VERSION` becomes 4; a nucleus and an image that disagree do not run
   together, which is what ADR-0053 established that field for.
3. The launcher's endowment becomes a set of `(binding, object, rights, scope)`
   decided from the verified IR's `capability_imports` before the process starts,
   and named in the audit record as `CAPABILITY_V1` §2 requires.
4. The engine binds capability imports per run, beside the entry arguments, and
   the lowerer lowers an imported name to `Op::Capability`. The verifier checks
   that a function naming an import declares the matching effect — the rule it
   already enforces for `extern` items, at one more site.
5. The runtime image implements `System` over `SYSTEM_ABI_V1`, which is the
   mapping `SYSTEM_INTERFACE_V1` §8 already fixes and which nothing can exercise
   until this decision lands.
6. A boot module reaching a real operation on a real endpoint becomes the
   Stage 3 identity evidence `docs/37` asks for, and the answer to its question —
   "do textual processes exercise real capability/IPC contracts rather than
   running as decorative scripts around privileged binary services?" — stops
   being *no*.

## Evidence required

1. A launcher decides the full capability grant of a process from its verified
   module image alone, before that process's first instruction runs, and the
   decision names each request by the binding the module declared.
2. A module holding two imports of one interface receives two different grants,
   and each name reaches the object the launcher chose for it — the case that
   makes the key a key rather than a label.
3. A module whose request is denied fails at startup with `CapabilityDenied`
   that **names the denied binding**, and never reaches the call
   (`SYSTEM_INTERFACE_V1` §10.3, `PROCESS_IDENTITY_V1` §7.3).
4. A grant whose object kind is not the kind the import's interface declares is
   refused at startup rather than at the first call.
5. Reordering two `import capability` lines in a module changes nothing about
   which grant each name receives.
6. A `.tos` module performs a real operation on a real endpoint, and the other
   side of that endpoint observes it — the whole point, and the thing that is
   impossible today.

## What each option costs to build

| | A — entry parameters | B — capability imports | C — fetch by index | D — defer |
|---|---|---|---|---|
| Correspondences that must be invented | **two** | one | one, and it lives in the module | — |
| `main()`'s signature is part of the launch contract | yes | no | no | — |
| Canonical boot text changes | yes | no | no | — |
| Authority travels through uninvolved signatures | yes | no | no | — |
| `import capability` still means something | as a label for `uses` | as the request | as a label | — |
| Refused by an accepted document | no | no | `CAPABILITY_V1` §7.6 in substance | it is the gap restated |
| Stage 4 drivers | one parameter per device authority | one import per device authority | index constants in driver source | — |

And for the key, over the three axes it is easy to conflate:

| | i — binding | ii — order | iii — explicit id | iv — policy |
|---|---|---|---|---|
| Is a key at all | yes | only with denial holes | yes | no — it is the decider |
| Distinguishes two imports of one interface | yes | by source order | yes | over whichever key |
| `PROCESS_IDENTITY_V1` §7.3 set difference | by construction | needs holes added | by construction | — |
| Names the denied request | the binding | the position, resolved by the module | the id | — |
| Already in the artifact and the digest | **yes** | order is, identity is not | no | — |
| Closed contract touched | none | none | `tos-ir/v1` or `docs/39` | none |
| Survives a source reorder | yes | **no, silently** | yes | — |
| A policy can be written against it | yes | fragile | yes | — |

## Boundary

Everything downstream of ADR-0060 waits on this: the supervisor that reads
`/system/policy/` is a textual module that must call `process_create`, and
`docs/37`'s Stage 3 identity question cannot be answered *yes* while the only
thing exercising capability and IPC contracts is the Rust runtime image.

Nothing already built changes under any option. The nucleus, `SYSTEM_ABI_V1`,
the capability table, the endowment chain and the engine's interface port stand
as they are; what is decided here is which declaration of a module a grant is
matched against, and what the launch record must say for that match to be
checkable rather than assumed.
