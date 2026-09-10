<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — modules, capabilities, and versioning

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language versions: `TOS Core 1.0` (original V1), `1.1` (ADR-0080),
  `1.2` (ADR-0081), `1.3` (ADR-0085), `1.4` (ADR-0086)
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`,
  `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`, and
  `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`

### The minor sequence, recorded whole

It had drifted: this line named only 1.0 and 1.1 while ADR-0081 had accepted 1.2
and the implementation supported it. Repaired here rather than left, and **1.2 is
attributed to the decision that accepted it**:

| Minor | Decision | What it added |
|---|---|---|
| 1.0 | ADR-0027/ADR-0028 | original V1 |
| 1.1 | ADR-0080 | a capability as an operation result, and effects naming interfaces |
| 1.2 | **ADR-0081** | device memory: `MmioRegion`, region and `DmaRegion` indexed access |
| 1.3 | **ADR-0085** | capability representation separated from interface identity |
| 1.4 | **ADR-0086** | DMA publication and consumption ordering |

**A module receives the language its header claims.** A 1.0, 1.1 or 1.2 module
does not acquire a later minor's semantics from the frontend that happens to
compile it — `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR` refuses a later form in an
earlier module — and an implementation that does not support a minor rejects the
module whole by its header with `E1602_UNSUPPORTED_LANGUAGE_MINOR`.

**1.3's frontend and verifier support landed with ADR-0085's rule**, not
before. Until the whole path existed — the schema field and its gate, the
verifier's independent derivation, `E1503`, the release-time mapping
retirement and the feature gate — a module declaring 1.3 was refused whole by
its header, which is the fail-closed direction. The minor was advertised last,
after all of it was implemented and green.

## 1. Module identity and deterministic resolution

Every source begins with exactly one declaration:

```tos
module system.example version 1.0 profile bootstrap;
```

The version is the source-language major/minor version, not a module release
number. For V1 the major MUST be `1`; any other is
`E1601_UNSUPPORTED_LANGUAGE_VERSION`, and a minor the frontend does not
implement is `E1602_UNSUPPORTED_LANGUAGE_MINOR`. The accepted minors are **0, 1,
2, 3 and 4**, and each adds exactly what its decision decided: 1.1 the
direct-interface effect form of docs/39 (ADR-0080), 1.2 device memory and region
indexed access (ADR-0081), 1.3 capability representation separated from interface
identity (ADR-0085), 1.4 the two DMA ordering operations (ADR-0086).

**Accepted and implemented are two different statements**, and they are worth
keeping apart even where they now agree: every accepted minor through 1.3 is
implemented. **1.4 is accepted and is the current example of the difference** —
ADR-0086 was approved on 2026-09-10 and a frontend advertises minor 4 only when
it performs the whole of it, so until that slice is green a 1.4 module is
refused whole by its header with `E1602_UNSUPPORTED_LANGUAGE_MINOR`. A frontend advertises a minor **when it performs it**, not when a
decision naming it is approved — so between ADR-0085's approval and the end of
its implementation slice, a module declaring 1.3 was rejected whole by its
header with `E1602_UNSUPPORTED_LANGUAGE_MINOR`, which is the fail-closed
direction and is what `E1602` is for. A minor a future decision accepts is in
exactly that state until its own slice lands.

**A module receives the language its header declares**, not the newest the
frontend implements. A 1.0 module using a 1.1 form is
`E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`, and the version the header declared is
what the artifact records — so two minors never share one identity in the module
digest, the cache or the provenance record. A resolver maps module name
`a.b.c` to canonical repository path `a/b/c.tos` relative to a declared module
root in the active source set. A source whose path does not match its header is
`E1603_MODULE_PATH_MISMATCH`.

The resolver input is exactly:

- the selected system commit or accepted detached source-set identity;
- a declared ordered list of module roots and dependency source-set identities;
- the importer module name, requested import, language version, and profile;
- the declared dependency lock/manifest; and
- the effective resource import limit.

It MUST NOT inspect an ambient current directory, host filesystem outside those
roots, network, clock, random source, or undeclared environment variable. An
import never triggers a fetch. Any required fetch is a separate,
source-identified system operation outside the language frontend.

The declared module roots are searched in order, and the candidate in the
earliest root resolves the name. That order settles roots and only roots: it is
what layering a private root over a shared one means, and it makes resolution
deterministic and total. It is not permission to paper over a collision between
declared dependencies, which nothing orders against each other.

An import naming no candidate at all is `E1604_IMPORT_NOT_FOUND`. An import is
`E1605_AMBIGUOUS_IMPORT` when either the declared source set holds more than one
module with the requested name inside one root, so nothing in the set orders
them, or more than one reachable declared dependency source set provides that
name. The two conditions are disjoint, and the diagnostic names the identities
that collided. See ADR-0038.

`import a.b as c;` imports exported types, functions, and constants under `c`.
Without `as`, the final segment is the binding name. Imports are explicit; V1
has no wildcard, relative, implicit prelude, or host-standard-library import.

An imported enum's variant is reached through that binding as a qualified path —
`c.Signal.Low` — in both expression and pattern position. A qualified pattern
path always denotes a constructor and never a binding, and a path naming no
reachable variant is an error rather than a catch-all (ADR-0033). Resolution
uses the same deterministic import closure as any other imported name, so a
variant pattern is reproducible from the module's declarations plus its
closure.
An import graph cycle is `E1606_IMPORT_CYCLE`, including a deterministic ordered
cycle path in diagnostic fields. There is no top-level executable initialization:
items declare types, constants, resources, and functions only. This makes
module loading and cache identity independent of initialization order.

The module resource declaration is `resource [ ... ]`, and a function's
capability-effect declaration is `uses [ ... ]`: both are comma-separated
declarative lists, never executable brace blocks. Their meaning and required
keys remain in docs/40–41.

`pub` exports an item. A non-`pub` item is module-private. A public function's
parameter/return types and effect capabilities must be exported/reachable; an
otherwise private ABI type is `E1607_PRIVATE_PUBLIC_TYPE`.

The rule covers the **transitive public type surface**, not just the outermost
name. A type is reachable when it is primitive or predeclared, imported (and so
reachable at the module that declares it), or a `pub` local nominal type whose
own publicly necessary surface is itself reachable. The publicly necessary
surface of an exported record is the types of its fields, and of an exported
enum the payload types of its variants, because a consumer cannot construct or
match one without naming them. So

```tos
pub record Wrapper [ value: PrivateType ]
pub fn get() -> Wrapper
```

is `E1607_PRIVATE_PUBLIC_TYPE` even though `Wrapper` is itself `pub`. A type
used only inside a function body, or only by a module-private item, is an
implementation detail and is not part of that surface.

`pub` states a public **source-level** interface: the importing module must be
able to name and resolve those types. A module has no binary ABI promise merely
because an item is `pub`; source, IR schema, and runtime compatibility are
governed below. The two are separate — the absence of a binary ABI promise does
not weaken the visibility rule. Permitting a private nominal type in a public
signature would require a model of opaque or private type leakage across a
module boundary, which TOS Core V1 does not define and does not introduce
implicitly.

## 2. Capability declarations, grants, and transfer

Capability imports have the exact form:

```tos
import capability system.time.Clock as clock;
```

This declares that the module may receive one opaque value named `clock` whose
nominal capability type is `system.time.Clock`. It is a request, not a grant.
The process launcher/supervisor, not source text, maps the request to a concrete
grant after policy/trust evaluation. An absent/denied request means module
startup returns the typed launch error `CapabilityDenied`; it is not fabricated
as an absence sentinel, a global singleton, an integer, or a successful empty
authority. (`nil` is not a TOS Core V1 value.)

The imported name can appear only as a value of its declared opaque type, a
function parameter/effect name, or an argument to an operation that requires
that same contract. It cannot be a `const`, record field, serialized value,
numeric conversion, equality key, or deserialized replacement. Constructing or
casting one is `E1502_FORGED_CAPABILITY`. A capability operation is valid only
when the capability type, requested operation/right, resource range, and the
enclosing `uses` effect all match a declared interface contract.

**The `uses` effect names an interface, and a request is a separate fact**
(ADR-0080). `import capability` requests authority before the first instruction
and is answered or denied by launch policy; a `uses` effect declares which
interfaces a function may exercise and requests nothing. They coincide whenever a
module uses only what it imported, which is why one declaration did both jobs
until an operation began returning capabilities of interfaces no import could
have been answered for. An effect naming an interface directly therefore grants
nothing: the operation still requires a capability value at the call site, the
verifier proves its exact nominal type against the artifact, and the nucleus
proves the object's rights and liveness at the call.

The effective process grant is an explicit finite set of object-specific rights
and resource constraints. A capability can move to one scoped task only if its
interface declares it transferable. Delegation/attenuation is a typed interface
operation: its output rights MUST be a subset of the input's rights, object
scope, and lifetime. No source operation can widen a right, recreate a consumed
linear capability, or transfer a handle by encoding its bits. Authority appears
in process identity, source maps, IR imports, audit logs, and cache identity;
the concrete secret/handle representation does not.

`Region<T>`/`DmaRegion<T>` grants originate only through a capability operation
whose accepted interface declares element type, alignment, access, size, DMA
domain, lifetime, and transfer/share rules. The language V1 contract defines
the nonforgeability boundary; actual PCI/MMIO/IRQ/DMA interfaces belong to
later stages and must be separately versioned. Thus a Stage 2 example can
declare capability intent without pretending that Stage 3/4 services exist.

## 3. Profile compatibility

`profile bootstrap` is a strict, executable subset of `profile full` source
semantics. A Bootstrap module must conform to every Bootstrap restriction and
may be loaded by a Full engine without changing its meaning. A Full module MUST
NOT be silently accepted by a Bootstrap frontend/engine: it reports
`E1702_PROFILE_NOT_SUPPORTED` with the first forbidden feature.

Bootstrap permits the core scalar/aggregate/Result/ownership/capability syntax,
metered loops, `parallel` scopes, `spawn parallel`, `join`, and `cancel`, but
requires the resource bounds in docs/41 and `workers: 1`. It serializes child
task execution in a deterministic order consistent with source creation order
when more than one order would otherwise be observable. It forbids `async fn`,
`spawn async`, `await`, closures, `defer`, `unsafe`, `extern`, dynamic module
loading, a module graph above its declared import cap, and any interface whose
cleanup/allocation/resource bound is absent.

Full permits these constructs only when their typed interface, effect set,
resource declaration, and verifier-visible IR operation are defined. Full does
not remove safe-language constraints: it adds a true SMP-capable execution
path, not a second memory model. Future Full-only standard libraries use a
declared minimum language/profile version and cannot be implicitly pulled into
Bootstrap recovery.

## 4. Language, IR, runtime, and cache compatibility

Language source declares an accepted TOS Core V1 major/minor version — §1 is
where the accepted minors are listed, and this section states the invariant
rather than repeating them. A frontend declares the exact source-language minors,
profiles, feature set, and conformance revision **it implements**, which is not
the same set: a minor is accepted by a decision and implemented by a frontend,
and until both have happened a module declaring it is rejected whole with
`E1602_UNSUPPORTED_LANGUAGE_MINOR`.

A frontend rejects an unknown language major, and rejects any minor or minor
feature it does not advertise. **A source has no "best effort" downgrade path and
receives no newer minor's semantics implicitly**, whatever the frontend
compiling it happens to implement. Additive V1 minor extensions must have an
accepted contract and cannot reinterpret existing token sequences.

**1.1 is the first such extension, and it meets both requirements.** ADR-0080 is
its accepted contract, and it reinterprets no token sequence: a `uses` item that
was legal under 1.0 is a single identifier, and the form 1.1 adds is a dotted
path, which 1.0 could not parse there at all. A frontend implementing only 1.0
refuses a 1.1 module **whole, by its header**, with `E1602` — never partly, and
never by silently ignoring the form it does not know.

For every declared language version of V1, canonical-source NFC validation uses the
fixed Unicode 17.0.0 / UAX #15 Revision 57 baseline from docs/39 and ADR-0029.
The normalization baseline is selected by language version, never by the host
Unicode database. A future language version that changes it requires an
explicit compatibility decision.

TOS IR has a separate schema ID/version and verifier compatibility range in
docs/43. A runtime reports the language range, IR schema range, verifier ID,
backend ID, target ABI, and execution profile. It MAY accept an older verified
IR cache only when its verifier says the exact schema/source-map/capability
contract is compatible; otherwise it regenerates from canonical source. TOS
does not promise perpetual binary compatibility of IR or native cache objects.

The cache key binds normalized source/dependency identities, source-set
identity, frontend implementation identity, language/profile/feature revision,
IR schema, verifier identity, backend/target ABI, optimization/safety policy,
resource contract, and capability-interface digest. Changing any element
invalidates reuse. Deleting every cache must leave all canonical sources and
their declared dependencies sufficient for recovery/regeneration.

The language version in that key selects its fixed Unicode normalization
baseline. A cache producer cannot substitute a host-dependent normalization
result for the declared source version.

## 5. FFI and external code boundary

V1 reserves `extern` and `unsafe` syntax so the boundary is visible from the
first implementation. It does **not** admit a C ABI, Rust ABI, libc, host
threads, dynamic library loader, or arbitrary native extension as a TOS Core
runtime contract. A frontend written in Rust is an implementation detail; its
Rust FFI is not an FFI available to `.tos` programs.

An accepted future FFI version must define a named interface schema, exact
calling/ownership/region/capability rules, source-map/provenance, target ABI,
resource/cancellation behavior, and safe-call guarantees. An `extern` item
without that accepted interface is rejected by both checker and verifier. It
cannot be enabled by a build flag, host library presence, or unsafe block.

## 6. Module provenance and source maps

The module dependency closure is ordered lexically by canonical module name.
Each member contributes its source-set identity, canonical path, normalized
content ID, declared language/profile version and its Unicode-normalization
baseline, and interface/capability digest
to the frontend/lowering identity. A diagnostic and runtime event identify the
originating source unit and exact byte span. A derived artifact must retain that
mapping across import, lowering, optimization, task spawn/join/cancel, and
runtime failure. Source paths are repository paths, not host paths.

The source set remains canonical even if a derived cache was produced by an
owner-authorized build. An owner may authorize modified source according to
the repository/boot policy; that authorization grants no implicit module
capability and does not make a derived artifact canonical.
