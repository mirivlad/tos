<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 threat entries — proposed text for docs/34

Status: **Proposed.** This is the text to be merged into
`docs/34_THREAT_MODEL.md` when the Project Architect accepts ADR-0048…0051. It
is kept separate until then because `docs/34` is an accepted Tier 2 document and
must not describe a boundary that has not been accepted.

docs/34's own change rule requires it: "A stage cannot close if its new boundary
lacks a threat entry, negative tests and stated evidence level." Stage 3's stage
mapping line already exists — "capability, IPC and process isolation threats" —
and this is the detail behind it.

Adversary classes (T0–T9), protected assets (A1–A9) and required security
properties (S1–S9) are the existing ones; nothing here adds a class or an asset.

## Trust boundary 7, now real

Boundary 7 — "nucleus to user-space service through capability and IPC boundary"
— is listed in docs/34 and does not exist in the implementation yet. Stage 3
creates it. Everything below is about the moment it starts carrying traffic.

## Stage 3 threat entries

### X3.1 — Authority acquired without a grant (T1, T2 → A3)

A process obtains authority it was never granted: by forging a handle, guessing
an index, re-encoding a handle's bits into a value, reusing a released handle
after its slot is recycled, or presenting a handle of one type where another is
expected.

Response: handles are process-local indices into a nucleus-owned table with
generations; validation checks range, generation, type and rights
(`CAPABILITY_V1` §2). Negative tests: `CAPABILITY_V1` §7.1–7.3.
Evidence level target at closure: **E3** — the guessing and staleness cases are
fuzzable and must be fuzzed, not argued.

### X3.2 — Authority widened by attenuation (T2 → A3, S4)

A process attenuates a capability and receives one with more rights, wider
scope or longer lifetime than the input, through an arithmetic or subset-check
defect.

Response: the nucleus computes and checks the subset relation in all three
dimensions; there is no operation that adds a right (`CAPABILITY_V1` §4).
Negative test: `CAPABILITY_V1` §7.4, generated over right/scope pairs rather
than a hand-picked few. Evidence level: **E3**.

### X3.3 — Confused deputy (T1, T2 → A3, A8)

A weak client persuades a broker holding a strong capability to act on an
object the client cannot name. This is the failure that survives when the
mechanical capability tests all pass.

Response: a broker acts only on objects named by capabilities the client
actually passed; refusal is attributable to the client in the audit record
(`CAPABILITY_V1` §7.6, docs/37 Stage 3 evidence). Evidence level: **E2** at
Stage 3 close, **E3** when a second broker service exists to test against.

### X3.4 — Isolation breach between processes (T1, T2 → A4, A9)

A process reads or writes another process's memory or the nucleus's, by
addressing outside its grant, by racing a region transfer, or by reading frames
that belonged to a dead process.

Response: hardware address spaces (ADR-0048); grants bounded and generation-
tagged, frames cleared before reuse (ADR-0050 §3); linear region transfer
unmaps at the sender (`IPC_V1` §5). Negative tests: ADR-0050 evidence list and
`IPC_V1` §9.6. Evidence level: **E2**, with the frame-reuse case at **E3**
because it is the one that fails silently.

### X3.5 — Denial of service through the nucleus (T1, T2 → A9)

A process exhausts a shared resource: filling queues to grow nucleus memory,
holding a receiver blocked forever, spinning without ever entering the ABI, or
making capability validation expensive by holding many capabilities.

Response: bounded queues with visible backpressure and no allocation to accept
a message (`IPC_V1` §7); timer preemption independent of process cooperation
(ADR-0049); constant-time validation in the holder's capability count
(`CAPABILITY_V1` §5); every blocking operation cancellable
(`SYSTEM_ABI_V1` §6). Evidence level: **E2**.

Explicit non-goal at Stage 3: fair-share scheduling and priority inversion
control. Round-robin within one band is what ADR-0049 fixes, and a service that
needs more is a later stage's decision, not an undeclared property of this one.

### X3.6 — The ABI as an attack surface (T1, T2 → A4)

A process drives the system ABI with out-of-domain arguments, unknown operation
numbers, or addresses it hopes the nucleus will dereference.

Response: a closed status space; no operation dereferences a process-supplied
address; buffers are named by region handles; an unknown operation returns
`E_NOT_SUPPORTED` rather than being ignored (`SYSTEM_ABI_V1` §3, §4, §7).
Negative tests: `SYSTEM_ABI_V1` §8. Evidence level: **E3** — the ABI is the one
Stage 3 surface that takes wholly untrusted input in registers, and it is
fuzzable in exactly the way the capsule parser was.

### X3.7 — Escalation by process creation (T2 → A3, A8)

A service creates a process to obtain authority it does not hold, or grants a
child more than it holds itself.

Response: `process_create` requires a process-authority capability that ordinary
services do not hold; a launcher cannot install a capability it does not itself
hold, and the granted set is asserted by the nucleus, not by the launcher's
claim (`SYSTEM_ABI_V1` §5, `PROCESS_IDENTITY_V1` §3). Evidence level: **E2**.

### X3.8 — Identity forged by the process it describes (T2 → A6, A1)

A process reports a module digest, source set or capability set it does not
have, and the false claim reaches the audit record.

Response: the audit record is the launch record, asserted by the holder of
`process_create` from the bytes it passed; self-reports are separately
identified and never the audit record; disagreement is itself an event
(`PROCESS_IDENTITY_V1` §2, §6). Negative test: `PROCESS_IDENTITY_V1` §7.2.
Evidence level: **E2**.

### X3.9 — A commit identity the system never read (T2, T6 → A1, A6)

A Stage 3 process is recorded as belonging to a system commit, although Stage 3
has no repository and read no commit. The false record then propagates into
activation and rollback reasoning at later stages.

Response: the system commit id is **absent** for capsule-launched processes and
is asserted absent by test, not left to convention
(`PROCESS_IDENTITY_V1` §5, §7.5). Evidence level: **E2**.

This entry exists because it is the Stage 3 form of the failure Stage 1 was
built to prevent: an invented official commit. It is cheap to introduce by
accident and expensive to detect later.

### X3.10 — Privileged policy migrating into the nucleus (T0 → A1, A8)

Service logic moves into the nucleus because IPC is inconvenient, and the system
becomes a conventional microkernel with textual decoration.

Response: this is a design threat, not a runtime one, and it is checked the way
docs/31 checks it — a dependency and surface inventory at Stage 3 close showing
that no service logic entered the nucleus, and that every privileged behaviour
is exercised by a source-identified textual process. Evidence level: **E1**,
honestly: it is a reviewable property, not a tested one.

## What Stage 3 does not claim

- No protection against T7 (physical/firmware) or T8 (nucleus compromise); both
  remain outside containment, as docs/34 already states.
- No timing or cache side-channel protection between processes. Stage 3 clears
  reused frames, which closes the direct-disclosure path, and makes no
  micro-architectural claim.
- No time source a process can trust. Monotonic ticks exist for scheduling
  (ADR-0049); trusted time is Stage 7.
- No revocation of authority already delegated beyond what an owning service
  implements (`CAPABILITY_V1` §4).
