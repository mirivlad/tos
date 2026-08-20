<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0064: Which diagnostic a nonconstructible type in value position gets

- Status: **Accepted (option B, with the boundary of §"The decision" below)**
- Date: 2026-08-21
- Decision level: 2 — it settles which accepted diagnostic a source form
  produces; the docs/44 registry, conformance vector R070 and the frontend all
  depend on the answer, and before this decision the registry said one thing
  while the frontend and the vector said another
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-21
- Carried out by: ADR-0039 revision 4 (the operations and the boundary),
  `docs/44`'s `E1213` row, and conformance vectors R070 and R081. **ADR-0036 is
  not amended and needs no amendment:** under option B its §1 sentence and its §7
  evidence item are true as written, which is the dangling reference closing from
  the other end
- Relates to: ADR-0036 §1 and §7, ADR-0039 revision 3 §1 and §4

## The decision

Option **B**, and the boundary is part of it rather than a note on it. Not every
nonconstructible type name in value position is `E1213`:

| form | code | `operation` |
|---|---|---|
| a nonconstructible predeclared type applied to arguments — `Event()`, `Task(…)`, `Mutex(…)`, `MutexGuard(…)` | `E1213_NONCONSTRUCTIBLE_TYPE` | `construct` |
| the same name written alone in value position — `Event` | `E1202_UNKNOWN_VALUE_NAME` | *(none)* |
| `as` with a nonconstructible type, either side | `E1213_NONCONSTRUCTIBLE_TYPE` | `as` |
| a capability, in any of those forms | `E1502_FORGED_CAPABILITY` | — |

**Why B.** Revision 3's factual premise — that reaching the diagnostic would
require widening the grammar — is disproved by the working frontend: the call
form already exists, and it tells a fabrication attempt from an ordinary
unresolved value name without any grammar change. And it keeps `docs/40` §3's
rule symmetric: an opaque runtime handle may not be made out of data, whether the
handle is authority or a lock.

**What the decision requires of an implementation.** The finding must follow from
the *position* the name is written in — a callee of a call or construction —
never from the name alone. A special case keyed on the spelling is what produced
`operation=construct` for a bare `Event`, and it is excluded by this decision
rather than merely discouraged.

Everything below is the analysis this decision was taken on, kept as written.

## Why this exists

Two accepted Tier 1 decisions say different things about one source form, and
the implementation follows neither of them exactly. `docs/38` says a silent
contradiction between accepted ADRs is invalid and that an agent must not
resolve one by choosing an implementation, so this decision is written before
anything is changed.

Nothing here proposes new functionality. It proposes to make three texts and one
implementation say the same thing, and it records which of them would have to
move under each option.

## The passages, quoted

**ADR-0036 §1** (Accepted, Project Architect-approved 2026-08-11), on the three
guard types:

> They are **not constructible from source**. There is no constructor syntax for
> a guard; a guard value exists only as the result of a lock operation. Writing
> one as a constructor is the nonconstructible-type error of ADR-0039.

**ADR-0036 §7**, in its conformance-evidence list:

> … and a negative applying a constructor to a guard type.

**ADR-0039 revision 3 §1** (Accepted, Project Architect-approved 2026-08-11),
listing the operations `E1213_NONCONSTRUCTIBLE_TYPE` covers:

> - an `as` conversion whose target type is one of the nonconstructible types;
> - an `as` conversion whose operand type is one of them.
>
> That is the whole list, and it is short for a reason. A predeclared type is not
> an expression primary or callee in V1, so `Event()`, `Task(1i32)` and
> `Mutex(1i32)` are not fabrication attempts this code has to catch — they are
> names that resolve to nothing in value position, and the frontend already
> reports each as `E1202_UNKNOWN_VALUE_NAME`. Verified against the reference
> frontend, not assumed.

**ADR-0039 revision 3 §4**:

> A vector for `Event()` is deliberately absent: R-vectors record the code a form
> actually produces, and that form produces `E1202_UNKNOWN_VALUE_NAME`.

**`docs/44` diagnostic registry** (Tier 2), the `E1213` row:

> … `TaskResult<T>` is not among them: `Completed` and `Cancelled` build one. A
> predeclared type in value position is `E1202`, not this (ADR-0039)

**`docs/44` diagnostic registry**, the `E1502` row — the same rule's other half:

> a capability interface is constructed or cast into existence rather than
> received through its declared import; the `interface` field names it and
> `operation` says which

**Conformance expectation R070**:

> | R070 | `reject/forged-guard.tos` | Bootstrap | `E1213_NONCONSTRUCTIBLE_TYPE` |
> ADR-0036/0037 negative |

## What the frontend actually does

Measured against the reference frontend at `00664f8`, whose frontend crates are
byte-identical to `850f1b3`. Checker output for a `bootstrap` module body:

| source | code | fields |
|---|---|---|
| `let e = Event();` | `E1213_NONCONSTRUCTIBLE_TYPE` | `type=Event`, `operation=construct` |
| `let t = Task(1i32);` | `E1213_NONCONSTRUCTIBLE_TYPE` | `type=Task`, `operation=construct` |
| `let m = Mutex(1i32);` | `E1213_NONCONSTRUCTIBLE_TYPE` | `type=Mutex`, `operation=construct` |
| `let g = MutexGuard(0i32);` | `E1213_NONCONSTRUCTIBLE_TYPE` | `type=MutexGuard`, `operation=construct` |
| `let e = Event;` | `E1213_NONCONSTRUCTIBLE_TYPE` | `type=Event`, `operation=construct` |
| `let n = Nowhere(0i32);` | `E1202_UNKNOWN_VALUE_NAME` | `name=Nowhere` |
| `let x = 1i32 as Task<i32>;` | `E1213_NONCONSTRUCTIBLE_TYPE` | `operation=as`, `from=i32`, `to=Task<i32>` |

Three facts follow, and the third is not in either ADR.

1. The three forms ADR-0039 revision 3 states produce `E1202` produce `E1213`.
2. `MutexGuard(0i32)` produces `E1213`, which is what ADR-0036 §1 asks for and
   what R070 records.
3. **A bare `Event` in value position, where nothing is constructed at all, also
   produces `operation=construct`.** No accepted text asks for that, and the
   field is inaccurate on its face: nothing was applied to anything.

The finding is produced by `Resolver::resolve` in the name-resolution slice, not
by the type slice.

## How the texts came apart

- `19d784a` proposed ADR-0036 through ADR-0039. ADR-0039 revision 2 covered
  constructor and aggregate forms, so ADR-0036 §1's cross-reference was accurate
  when it was written.
- `b3832fe` accepted ADR-0036 **as written** and, in the same commit, revised
  ADR-0039 to revision 3, removing exactly the forms ADR-0036 §1 points at. The
  cross-reference became dangling at that instant, and nothing said so.
- `98533e9` added the conformance vectors both ADRs require, including R070
  recording `E1213` for `forged-guard.tos` — ADR-0036 §7's evidence item, written
  against ADR-0036 §1's reading.
- `b16cc6c` changed `Resolver::resolve`. Its own message records the change and
  its reasoning: *"Implementing this exposed a gap in ADR-0039's landing: a
  nonconstructible type applied as a constructor was reported as
  `E1202_UNKNOWN_VALUE_NAME` … It is `E1213` with `operation=construct`."* That
  is not a gap in a landing. It is the sentence revision 3 decided, reversed in
  code without a decision — and the premise revision 3 was accepted on
  ("verified against the reference frontend") stopped being true afterwards.

## Why the hierarchy does not settle this by itself

`docs/38` Tier 1 holds accepted ADRs. ADR-0036 and ADR-0039 revision 3 are both
accepted, carry the same date and the same approval, were accepted in the same
commit, and neither supersedes the other. The tier rule therefore names no
winner; what it says instead is that **"silent contradiction is invalid"**, and
the conflict protocol requires stopping at the boundary and either correcting the
lower-authority document or explicitly superseding the higher decision.

`docs/44` is Tier 2. Its `E1213` row agrees with ADR-0039 revision 3, and a Tier
2 document must conform to Tier 1 — but a Tier 2 document cannot settle which of
two Tier 1 texts it should be conforming to.

One asymmetry does narrow the reading without deciding it, and it belongs on the
record. ADR-0036's own decision-level line says it "adds type constructors and
**one** diagnostic code" — and that code is `E1402_INVALID_GUARD_LIFETIME`.
ADR-0036 therefore does not allocate `E1213` or extend its operation set; §1
*refers* to whatever ADR-0039 provides. Under that reading the two decisions do
not conflict at all: ADR-0036 asserts the fact (a guard may not be constructed)
and delegates the code, and the delegate's accepted revision says `E1202`. That
reading is available, it is coherent, and `docs/38` still does not permit an
agent to adopt it unilaterally — which is why it is an option below rather than a
conclusion here.

## What must not be done

**Leaving it as it is.** The frontend and R070 say one thing; the registry that
describes the frontend, and the ADR that allocated the code, say another. An
implementation that answers a form differently from the published registry makes
the registry advisory, and `docs/44` is the document an alternate implementation
is measured against.

**Deciding it in code.** `docs/38`: "Agents must not resolve conflicts by
choosing the easiest implementation." Either answer is a Level 2 act because
conformance evidence depends on it.

## Options

### A — the delegation reading: a nonconstructible type in value position is `E1202`

ADR-0039 revision 3 stands as accepted. `Resolver::resolve` stops reporting
`E1213`; the form is `E1202_UNKNOWN_VALUE_NAME`, which is exactly what the
registry's `E1202` row describes ("a value name … resolves to no predeclared
value, module item, parameter or in-scope binding"). R070 keeps its vector — it
is ADR-0036 §7's required evidence and remains so — with the recorded code
corrected to `E1202`, which is what ADR-0039 §4's own rule for R-vectors demands.
ADR-0036 gains a revision 4 correcting §1's dangling sentence and §7's evidence
item to name the code the form actually produces. `docs/44` does not move: it
already says this.

Costs: `MutexGuard(0i32)` is reported as an unknown value name, which points a
reader towards a declaration that must never exist — the objection `b16cc6c`
raised, and it is a real one about diagnostic quality. An accepted conformance
expectation changes its recorded code, which is a change to accepted evidence
even though it is a correction towards Tier 1.

That cost is smaller than it looks, and the reason belongs in the option rather
than in the recommendation. **The code is the contract; the message is not.**
`docs/44` fixes what `E1202` means and what fields it carries, and nothing in any
accepted document fixes the human-readable text. A frontend reporting `E1202` for
a predeclared type may say so — that `MutexGuard` is a type, that V1 has no
constructor for one, and that a guard comes from `lock()` — without changing the
code an alternate implementation is measured against. The reader's problem is
answerable inside A; the machine-facing disagreement is not answerable inside the
status quo.

### B — ADR-0039 revision 4: the value-position form rejoins `E1213`

`E1213` covers a third operation: a nonconstructible type name in value
position. The implementation and R070 stay as they are. `docs/44`'s `E1213` row
loses its final sentence and gains the operation. ADR-0036 §1 becomes accurate as
written.

This option carries one fact revision 3 did not have. Revision 3 rejected these
forms because "promising `E1213` for those forms would mean widening the grammar
to let them through to the type stage purely so a diagnostic could fire".
**That premise is false for a resolution-stage implementation**, and the running
frontend is the proof: `Resolver::resolve` produces the finding with no grammar
change whatever, because the callee name is resolved before any type exists. The
rejected alternative in revision 3's own list — "widen the grammar so `Event()`
reaches the type stage" — is not the only way to reach the diagnostic, and it is
not the way the implementation took.

It also restores a symmetry the registry currently breaks: `docs/40` §3 pairs
`E1502_FORGED_CAPABILITY` with "the corresponding nonconstructible-type error"
as two halves of one rule, and the capability half already covers construction
(`docs/44`: "constructed or cast into existence … `operation` says which"). Today
`system.time.Clock()` is a forgery with `operation=construct` and `Mutex(1i32)`
is an unknown name.

Costs: it reverses an explicit decision taken with reasons, and amends a Tier 2
registry row. It must also fix what the implementation gets wrong today: a bare
`Event` is not a construction, so either the operation field distinguishes the
call form from the bare-name form, or the bare-name form stays `E1202`.

### C — a third code for the form

`E1213` keeps revision 3's two `as` operations, `E1202` keeps "no such name", and
a new code — `E1214_PREDECLARED_TYPE_IN_VALUE_POSITION` — names the form
precisely: this is a type, it is not a value, and no declaration will ever make
it one.

The number is free, and that was checked rather than assumed: `E1214` appears in
ADR-0037 §5 as `E1214_INVALID_SHARE`, which is option 1 of that ADR's list and
was **not** the option accepted — §5 allocates `E1215_ARGUMENT_TYPE_MISMATCH`
instead. A number named in a rejected alternative is unassigned.

Costs: one more code fixed for TOS Core 1.0, one more registry row, and a new
precedence edge against `E1202` and `E1502`. It is the most accurate and the most
expensive in contract surface, and V1's diagnostic surface is fixed at 1.0.

## What each option costs to move

| | A | B | C |
|---|---|---|---|
| Frontend behaviour changes | yes | no | yes |
| R070's recorded code changes | yes (`E1202`) | no | yes (`E1214`) |
| `docs/44` registry changes | no | yes | yes |
| An accepted ADR is revised | ADR-0036 §1/§7 (dangling reference) | ADR-0039 (decision reversed) | both, by reference |
| A code is added to TOS Core 1.0 | no | no | yes |
| `MutexGuard(0i32)` reads as | an unknown name | a forged handle | a type in value position |
| Capability/other-opaque symmetry | stays broken | restored | named explicitly |

## Recommendation as proposed — not the decision taken

Kept because a recommendation that is overruled is part of the record, and the
reasons on both sides were what the choice was made between. The decision is B,
stated at the top; §"Why B" there records the ground it was taken on.

**A**, and the reason is fidelity rather than taste.

Revision 3 is the later and narrower of the two decisions, it was accepted on a
stated factual premise about what the frontend does, and `docs/44` — the document
an alternate implementation is measured against — was written to match it and
still says so. The change that broke the agreement was made in code, described in
a commit message as fixing a gap, and never went through a decision. Restoring it
returns the system to the state the Project Architect actually approved.

The amendment A needs to ADR-0036 is a correction of a reference that its own
scope line already tells us is a reference: ADR-0036 adds one diagnostic code and
it is `E1402`. B's amendment is a reversal.

The diagnostic-quality argument is serious, but it is an argument about a
*message*, and A can answer it without moving a code: the finding stays `E1202`
and its text says that `MutexGuard` is a type, that V1 has no constructor for
one, and where a guard actually comes from.

If the Project Architect nevertheless holds that the machine-facing code must
distinguish a forged handle from a misspelled name — which is a defensible
position, and the one the capability half of the rule already takes — then **B**
should be chosen deliberately, with revision 4 recording that a resolution-stage
implementation needs no grammar widening, and with the bare-name form given an
`operation` value that is true of it. What should not happen is B by default,
which is where the repository is now.

## Evidence, as built

`docs/38`'s conflict protocol asks for a test when the conflict was mechanically
detectable, and this one was. The gate that should have caught it is named by
ADR-0039 itself, in its own architecture impact statement: "checker unit tests …
**for a predeclared type in value position still being `E1202`**". That test was
required by the accepted decision and never written, which is why `b16cc6c` could
change the answer without anything turning red.

1. A checker unit test asserting the code and fields the accepted option gives
   each of `Event()`, `Task(1i32)`, `Mutex(1i32)`, `MutexGuard(0i32)` and a bare
   `Event` — the five forms, not one, because the drift lives in the difference
   between them.
2. A conformance vector for the form, recorded in `EXPECTATIONS.md` with the code
   the accepted option assigns, so that the corpus an alternate implementation is
   measured against contains the answer.
3. A vector on **each** side of the boundary, which is what makes the drift
   unrepeatable rather than merely corrected. The binding from a form to a code
   exists and works — that is what a conformance vector is, and the corpus driver
   holds the frontend to every recorded code. What it cannot do is notice a form
   that has **no** vector: `check-stage2-language-contract.py` binds codes to
   stages and checks that cited codes are registered, so a registry sentence
   naming a code for a form nobody wrote a vector for is prose no gate reads.
   ADR-0039 §4 excluded that vector deliberately, on the reasoning that the
   form's answer was settled; the effect was that the one sentence recording the
   answer became untested, and `b16cc6c` changed the answer with nothing to turn
   red. That is the structural reason a contradiction between two accepted texts
   and the implementation passed 58 gates.

   Built: R070 (`reject/forged-guard.tos`, constructor form, `E1213`) and R081
   (`reject/predeclared-type-in-value-position.tos`, bare name, `E1202`). A
   frontend that keys the finding on the name instead of the position now fails
   R081; one that drops the constructor form fails R070. Neither half can move
   without the corpus saying so.

   Not built, and named so it is not mistaken for built: a lint that reads a
   registry *condition* and holds the corpus to it. That would generalize beyond
   this row, and it is a separate decision about how much of the registry's prose
   is machine-readable.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-15 is served —
  one form gets one documented answer instead of three.
- **Canonical representation:** unchanged under every option. No accepted source
  becomes valid or invalid; only the code reported for an already-rejected form
  moves.
- **Trusted-base impact:** none. **Threat-model impact:** none directly; the form
  is rejected under every option, and what differs is what the rejection is
  called.
- **Source-to-runtime impact:** none. No artifact, digest or runtime behaviour
  depends on the diagnostic code.
- **Recovery and rollback impact:** none.
- **Compatibility profile:** TOS Core 1.0. Under C the diagnostic surface of 1.0
  gains a code, which is why C is the expensive option.
- **Stage identity gate:** Stage 2 conformance evidence (docs/37), which is what
  R070 belongs to.
- **Performance contract:** none applies.
- **Dependencies, licence, patent impact:** none.
- **Tests that enforce the decision:** the three items above.
