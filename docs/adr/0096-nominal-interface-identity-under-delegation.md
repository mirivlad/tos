<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0096: Must a nominal interface survive delegation and endowment?

- Status: **Proposed** (raised 2026-09-23 on Project Architect direction; **not
  scheduled, and it does not block Stage 4**)
- Date: 2026-09-23
- Decision level: **2 or 3, depending on the answer.** Preserving interface
  identity through the generic launch machinery touches `SYSTEM_ABI_V1`
  operation 22, the launch-plan entry and the launch record; putting it in the
  object kind touches the nucleus's object set. Either is a decision about
  Stage 3's capability model
- Related: **ADR-0061** (a binding answers a request; the object-kind check is a
  compatibility check and "is not a matching rule"); **ADR-0077** §2–§5 (launch
  plans, operation 22); **ADR-0085** (an interface is not its own value type);
  **ADR-0095** (Stage 4's publication authority, which deliberately does not
  depend on the answer); `CAPABILITY_V1` §2, §3; `docs/42` §2;
  `SYSTEM_INTERFACE_V1` §4, §4.1, §4.3;
  `docs/research/PUBLICATION_AUTHORITY_CONFLICT.md` §21–§24, which is the
  research this is raised from and is authority for nothing

## 0. Why this exists as its own decision

It was found while implementing Stage 4 publication, and Stage 4 publication no
longer depends on it (ADR-0095). Recording it here is what keeps a real generic
finding from either being lost or being allowed to hold up a storage slice. **It
is not to be solved from Stage 4D.**

## 1. The question

**When two accepted interfaces declare the same object kind, may launch policy
answer a request for one with a capability that reached the launcher as the
other?** Equivalently: is a capability's nominal interface a property that must
survive delegation and endowment, or is it a property of the receiving artifact
alone?

## 2. What is true of the tree today, as facts rather than as a position

- **The question is unreachable.** interface → object kind is **injective** over
  all ten accepted interfaces: `system.ipc.Endpoint`, `system.ipc.Reply`,
  `system.memory.Authority`, `system.process.LaunchPlanBuilder`,
  `system.process.LaunchPlan`, `system.process.Control`, `platform.pci.Bus`,
  `platform.pci.FunctionConfig`, `platform.irq.Source` and
  `platform.dma.Region` each have a kind of their own. So "a grant of the
  matching kind" determines the interface uniquely, and no pair exists to retype
  between. **That is why nobody has had to answer this.**
- **If such a pair existed, both directions would succeed.**
  `launch_plan_endow` (operation 22) resolves the delegated capability at no
  particular right and records `{binding, object, rights, scope}`; the launch
  record carries `{handle, object, rights, scope, binding}`; and the launcher's
  startup check compares the endowed object's kind against the kind the requested
  interface declares, and makes no other comparison. No interface identity exists
  anywhere along that chain to compare.
- **Stage 4 creates no such pair.** ADR-0095 represents publication authority as
  a dedicated endpoint object rather than as a second interface over
  `OBJECT_ENDPOINT`, so the injectivity above is intact after it.
- **Resolving this is not required to close Stage 4.** Nothing in the concrete
  Stage 4 path — correctness, isolation, authority or recoverability — depends on
  the answer, which is the test `docs/21` and the Stage 4 process rule apply.

## 3. What the accepted corpus says, both ways

**Neither reading is unsupported, which is why this is a decision.**

**That it is permitted.** `docs/42` §2: an import "is a request, not a grant.
The process launcher/supervisor, not source text, maps the request to a concrete
grant after policy/trust evaluation." `CAPABILITY_V1` §3: a capability is
`object + rights + scope + lifetime + generation` — the interface is not one of
the five. `SYSTEM_INTERFACE_V1` §4: "the kind is a check, not the mechanism that
chooses a grant." ADR-0061, most explicitly: "even 'match a request to a grant of
the matching kind' is not derivable; **it is a decision**", and of the check it
adopted, "the kind check … **is not a matching rule**".

**That it was meant to be refused.** The model met this case once and removed the
possibility rather than granting the power: `system.process.LaunchPlanBuilder`
and `system.process.LaunchPlan` are one object in two states and were given
**two object kinds**, because "a launcher answering
`import capability system.process.LaunchPlan` with a builder would be answering a
request for something that has been decided with something that has not."
`SYSTEM_INTERFACE_V1` §4 gives the check's purpose as refusing "a grant of the
wrong kind at startup", which is vacuous for a shared kind. And the prohibitions
on widening, on conversion between capability interfaces and on recreating a
linear capability are all stated of **source** operations (`docs/42` §2), so they
do not reach a launcher's mapping — which leaves it unguarded rather than
permitted by anyone's decision.

**So the position this ADR takes is only that the two are different claims.**
*The launcher decides who holds authority* is accepted many times over. *The
launcher decides which accepted operation set a child may exercise over one
object at one right* has never been decided, because it has never been
reachable.

## 4. What an answer would have to settle

1. Whether two accepted interfaces may declare the same object kind at all. If
   not, the rule belongs in `SYSTEM_INTERFACE_V1` §4 as an invariant on the
   schema, and it is cheap: today's set already satisfies it.
2. If they may, whether interface identity must travel with a delegation and an
   endowment — and if so, where it is recorded, where it is compared, and what an
   entry carrying none means. An absent identity treated as a wildcard is the
   retyping with extra steps.
3. Where the identity may come from. It cannot be a string a module supplies:
   that is `docs/37`'s "textual manifest grants itself authority". The candidate
   source is the verified operation row, since `endow_for_launch` is declared per
   interface and the verifier already proves a call site's exact interface.
4. Whether the nucleus must be able to tell such capabilities apart, which is a
   different and larger question — a nominal type that only the runtime image
   compares is an artifact-level gate, not a check on a handle.

## 5. Options, sketched and not weighed

Kept short deliberately; a real weighing belongs to whoever schedules this.

- **N1 — an injectivity invariant.** Forbid two accepted interfaces from sharing
  an object kind. Nothing changes today; a future interface pair that wants to
  share one must add a kind instead, as the launch plan pair did.
- **N2 — type-preserving endowment.** Interface identity in the plan entry and
  the launch record, taken from the verified operation row and compared at
  startup. Generic, and it extends operation 22.
- **N3 — status quo, stated.** Record that launch policy may retype, so that
  nobody designs an authority that depends on the nominal type surviving.
- **N4 — identity in the capability itself.** The nucleus holds an interface per
  handle. The largest, and it changes what a capability is (`CAPABILITY_V1` §3).

`docs/research/PUBLICATION_AUTHORITY_CONFLICT.md` §24 has the fuller analysis of
the middle two, written when this was still a publication question.

## 6. What this ADR does not do

- It does not answer its own question, and nothing in the tree waits on it.
- It does not amend `CAPABILITY_V1`, `SYSTEM_INTERFACE_V1`, ADR-0061, ADR-0077 or
  ADR-0085.
- It does not reopen ADR-0095. Stage 4 publication does not depend on the answer,
  by construction rather than by luck.
- It does not block Stage 4C, Stage 4D or Stage 4 closure. If a future slice
  creates an interface pair sharing an object kind, that slice inherits this
  decision as a prerequisite — and saying so now is the whole point of raising it.
