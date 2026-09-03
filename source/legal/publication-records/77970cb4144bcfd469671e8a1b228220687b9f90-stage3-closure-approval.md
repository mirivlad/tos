<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Project Architect approval — TOS Stage 3 closure

Project Architect Vladimir Tomashevskiy closed Stage 3 on 2026-09-03 for
evidence commit `77970cb4144bcfd469671e8a1b228220687b9f90`.

## Decision, as given

> Project Architect ruling, 2026-09-03:
>
> **ADR-0074 — APPROVED.** I approve ADR-0074 in the reconciled form present on
> closure commit `77970cb`. The approval covers the surviving normative decision
> after reconciliation, including: build products outside the build workspace;
> one immutable bundle region per exact closure; the performed T1 lifecycle in
> §4a; the funded build-worker role rather than a fixed BuildWorkspace
> allocation; the measured Capsule-v1 T1 account in §5d; operation 20 as
> actually built in §6a; the answers collected in §7a.
>
> Historical and superseded sections remain historical/superseded exactly as the
> reconciled document says. Their presence does not revive their old semantics.
> The still-open installed-source backend and the absence of a fixed
> BuildWorkspace size remain outside the Stage 3 closure claim exactly as the
> document states.
>
> **RUNTIME_OBSERVABILITY_V1 §9 — APPROVED AMENDMENT**, dated 2026-09-03. The
> accepted Stage 3 operator-visible error-view semantics are: the diagnostic
> transport is the single converged operator-visible view; the important-error
> view is a selection of that transport, not a duplicated second log; `WARN`,
> `ERROR` and `FATAL` form the important-error selection; severity of
> contract-defined events is fixed per event kind; process-owned journal records
> carry their own severity in the accepted textual form; all components converge
> on one ordered transport; `scripts/tos-journal.py` is a reader of that accepted
> view, not a production subsystem or second source of truth; the human-readable
> textual operator interface is part of the Stage 3 result; existing
> IPC/report/transport bounds remain the bounds; no new unbounded queue or store
> is introduced.
>
> I also explicitly approve the boundary in §9.6: persistence, rollover,
> archival, retention, filesystem location and cross-boot journal recovery are
> not Stage 3 closure requirements. Stage 3 requires that the consequential
> events exist, are attributable to their producer, carry severity, converge in
> one order, and can be read by an operator. It does not require an on-system
> persistent log store. This approval does not choose the eventual persistence
> mechanism or assign it to a particular later stage. It merely states that those
> questions do not block Stage 3. Do not infer from this approval that losing all
> diagnostic history across a real production reboot is an acceptable final TOS
> operator experience. That remains a future design obligation, not a Stage 3
> one.
>
> **Stage 3 — FORMAL CLOSURE APPROVED.** I approve formal closure of Stage 3
> based on the closure audit and the evidence on commit `77970cb`.
>
> Stage 3 is therefore CLOSED.

## The closure basis, as the ruling states it

- `docs/evidence/STAGE3_CLOSURE_AUDIT.md`: **60 audited obligations**;
- **56 `CLOSED`**;
- **0 `ENVIRONMENT-ONLY`**;
- **0 `OPEN — blocks Stage 3`**;
- **4 `OUT OF STAGE 3 by accepted decision`**;
- P2 observer qualification;
- P2 absolute IPC latency result: `p99 = 39.147 µs` over 300 samples, within the
  accepted `≤ 200 µs` conformance bound;
- relative `7.957x` retained only as observational data under ADR-0068;
- complete identity-exit audit;
- all four CI jobs green on `77970cb`;
- QEMU workflow profile 45/45;
- **no conformance threshold changed to obtain closure.**

## What this approval covers

The evidence tree is commit `77970cb4144bcfd469671e8a1b228220687b9f90`, and the
artifacts that commit's `MANIFEST.txt` and `SHA256SUMS` pin. The audited obligations are the numbered
rows of `STAGE3_CLOSURE_AUDIT.md` §1–§9 at that commit.

This record is written in a later commit because a record cannot contain its own
hash. **The reviewed tree is the one named here.** The commit that adds this
record performs closure bookkeeping only — status lines, approval records and
current-status prose — and changes no nucleus behaviour, no TOS Core semantics,
no IR or image format, no ABI operation, no supervisor behaviour, no restart
policy, no memory figure or ceiling, and no performance threshold.

## What this approval does not cover

It closes Stage 3 only. It does not authorize Stage 4, and it does not decide
any of the questions Stage 3 explicitly placed outside itself:

- **an installed-source backend** (ADR-0074 §5 C) — open, and no residency is
  attributed to a backend that has not been chosen;
- **a fixed `BuildWorkspace` size** (ADR-0074 §5a, §5c) — still a measurement
  rather than a bound, and nothing in the implemented system depends on one,
  because a build worker's arena is a funded role grant;
- **journal persistence, rollover, archival, retention, filesystem location and
  cross-boot recovery** (`RUNTIME_OBSERVABILITY_V1` §9.6) — not Stage 3
  requirements, with no mechanism chosen and no later stage assigned. The
  ruling is explicit that this is not a statement that losing diagnostic history
  across a production reboot is acceptable in the end;
- **the relative `≤ 8x` latency ratio** — removed from conformance by ADR-0068
  and retained as observational data only.

One representation boundary recorded during Stage 3 also stays where it is:
`SYSTEM_INTERFACE_V1` §4.1 requires the interface an operation reaches to be one
the module requested. Capability delivery of an entirely unrequested interface
was named a later question by the Architect's ADR-0078 ruling and is not part of
this closure.
