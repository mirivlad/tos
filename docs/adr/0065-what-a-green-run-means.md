<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0065: What a green local run means, and what a green CI run means

- Status: **Accepted (option A′)** (Project Architect-approved)
- Date: 2026-08-21
- Decision level: 2 — it fixes what the project's own evidence runs assert, and
  every stage gate is reported through one of them
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-21
- Follows: the CI/preflight parity audit of 2026-08-21, which found that the
  local gate set and the CI gate set had drifted in both directions at once

## The problem this decides

`docs/30` states the release gates as outcomes — "unit, integration, QEMU, fuzz
and conformance suites pass" — and does not say where they run. Nothing was
wrong with that until two runs existed that could disagree, and then it left the
one question nobody had answered: **what does green mean?**

It had already gone wrong twice, in opposite directions:

- a completeness check lived only in CI, so `preflight --full` reported "PASS, 58
  gates" for eleven commits while `main` was red on GitHub;
- the ADR-0026 performance conformance run lived only in CI, so a green local
  run never included it;
- and the same rule had two implementations — an inline copy in a workflow and a
  script — which is how the first of those went unnoticed.

## Decision

### 1. Parity, stated as an obligation on the sets

`preflight --full` is the **canonical local set of mandatory repository gates**.
The repository-conformance CI jobs on one commit MUST between them cover every
gate in it. CI MAY run environment-specific checks beyond it.

Therefore a green required CI on a commit means **no less** than a green
`preflight --full` on the same tree. The converse is not claimed: CI may prove
more, and does.

### 2. One inventory, and CI names profiles

Every mandatory gate is declared exactly once, in the inventory in
`scripts/preflight.sh`, with four fields: **profile**, **local scope**, label,
and the function that proves it.

- **profile** — the environment class the gate needs, and the unit a CI job runs:
  `docs` (text only), `provenance` (full git history), `source` (the Rust
  toolchain), `qemu` (firmware and an emulator), `selftest` (fixtures).
- **local scope** — `default` runs in a bare `preflight`; `full-only` needs
  `--full`. Scope is local pacing and has no effect on what CI must cover:
  `--profile X` runs the whole of X regardless of scope.

`scripts/preflight.sh --list` prints the inventory and runs nothing. It is the
only source of the composition, and every reader — the parity gate included —
takes it from there.

**A workflow names a profile and never a gate.** This is the part that makes the
rule hold rather than be maintained: a workflow cannot omit a gate because a
workflow never mentions one. The only thing a workflow can get wrong is failing
to run a profile, which is one assertion to check instead of sixty-nine.

### 3. What is not a repository gate

Installing firmware, adding a rustup target, uploading evidence — these are
environment, they have no local counterpart, and requiring one would make every
workflow lie about its setup. A step declares itself environment **in YAML**:

```yaml
      - name: Firmware and tools
        env:
          GATE_PARITY: environment
```

Not in a comment. A comment is not part of the document a parser sees, and a
marker a parser cannot see means whatever the next reader decides it means. A
step that only `uses:` an action carries no command and is environment by
construction.

### 4. The parity gate

`scripts/check-gate-parity.py` is itself a gate in the inventory. It reads the
inventory from `--list` and the workflows structurally, and asserts:

1. every profile the inventory declares is run by some job;
2. every profile a workflow runs exists in the inventory — a typo names a
   profile that selects nothing, and a step that runs nothing passes;
3. every command step of such a job is either a profile invocation or declares
   itself environment.

The third is the guard against the failure that started this: a rule
reimplemented in YAML beside the script that already implements it.

### 5. Jobs stay separate

`docs`, `source`+`selftest`, `qemu`, `provenance` remain four jobs, so the kind
of failure is visible without opening a log. Within a job, `preflight` runs every
gate of the profile even after one fails and prints each as `FAIL: <label>`,
which is more than a fail-fast step list gave.

### 6. Gate self-tests are a profile, not a claim about the repository

A regression test that runs a checker against a fixture proves the checker still
detects what it is for. That is a different claim from the checker's own, and
mixing them lets a repository assertion and a tool assertion pass for each other.
They are the `selftest` profile. `check-unsafe-safety` is split accordingly: the
checker over the trusted base is a `source` gate; its fixture test is a
`selftest` gate.

## What this does not decide

**Whether a hosting platform requires those jobs.** Branch protection is
configured outside the repository and is not visible from it, so this decision is
about the repository-conformance jobs declared in `.github/workflows`, and the
parity gate claims exactly that. If the set of platform-required checks is ever
made visible to the repository, binding it to this inventory is a further
decision and a small one.

**How long a local run may take.** Adding the ADR-0026 conformance run to
`--full` roughly doubles it, measured at 7m12s on the development machine. That
is a consequence of parity, not a preference, and if it becomes intolerable the
answer is a decision about that gate rather than about parity.

## Consequences

A gate added to the inventory is covered by CI or the parity gate is red. A rule
written into a workflow instead of a script is red. A profile silently dropped
from a job is red. None of these depends on anyone remembering.

The local default run gains one thing and is otherwise unchanged: the `selftest`
profile, whose gates were previously either not run at all or bundled inside
another gate. That is a deliberate widening and is named here so that "the same
class of gates" stays a checkable statement rather than an impression.

## Alternatives considered

**Both sides list the gates, and a checker compares the lists.** Rejected: two
lists that agree today are two lists, and the drift they permit is exactly the
one being fixed.

**One physical run — CI invokes `preflight --full` in a single job.** Rejected:
it satisfies parity and destroys the property `docs/30` implicitly relies on,
that the *kind* of failure is visible. A boot failure and a licence-header
failure would arrive as the same red mark.

**Leave it as it was and rely on review.** Rejected by the evidence: the rule was
already written down, and eleven commits went past a red CI without it being
noticed locally.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-15 is served —
  what a green run asserts is now stated instead of assumed.
- **Canonical representation:** unchanged; no artifact, digest or boot path
  depends on this.
- **Trusted-base impact:** none. **Threat-model impact:** positive: a gate that
  silently stops running is the failure mode this makes loud.
- **Compatibility profile:** unaffected.
- **Stage identity gate:** every stage gate is reported through one of these
  runs, which is why what they assert is a Level 2 question.
- **Performance contract:** ADR-0026's conformance run becomes part of the local
  full set; its cost is recorded above.
- **Tests:** the parity gate, its regression test covering five refusals and two
  acceptances, and the injections recorded in `PROGRESS.md`.
