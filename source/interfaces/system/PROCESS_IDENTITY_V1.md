<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Process Identity — Version 1

Status: **Accepted Tier 2 interface contract.**

Accepted by ADR-0048 (Project Architect-approved, 2026-08-12), which fixes the
boundary this contract describes.

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs.

## 1. Role

docs/03 calls the identity plane part of the operating-system model rather than
optional debugging metadata, and docs/10 lists the fields a process identity
carries. docs/02 I-16 requires that every running non-nucleus component be able
to report the source it came from.

This contract fixes the part that decides whether any of that is evidence:
**who asserts each field.**

## 2. Why the asserter is the contract

A process that reports its own module digest is reporting a claim. If the
process is defective or hostile, the claim is worth nothing, and an audit record
built from it is worth less than nothing because it looks like evidence.

So identity has two kinds of record, and they are never merged:

- the **launch record**, made by whoever held the `process_create` capability,
  from the bytes and the grant it actually passed;
- **self-report**, produced by the process about itself, available for
  debugging, always labelled, never the audit record.

Where the two disagree, the launch record governs and the disagreement is itself
an event worth emitting.

## 3. Fields

| Field | Asserted by | Notes |
|---|---|---|
| process instance id | nucleus | unique for the life of the boot; never reused |
| module name | launcher | read from the verified module header |
| source content id | launcher | `sha256:` over the normalized source it passed |
| source set | launcher | capsule or commit; §5 |
| system commit id | launcher | absent in Stage 3; §5 |
| language/frontend id | launcher | from the module header |
| IR schema id | launcher | from the module header |
| verifier identity | launcher | the verifier that issued the receipt |
| runtime engine id | launcher | ADR-0048: the engine is a per-process artifact and is named |
| system ABI version | nucleus | `SYSTEM_ABI_V1` §7 |
| granted capability set | nucleus | what was actually installed in the table, not what was requested |
| requested capability set | launcher | from the module's `capability_imports` |
| memory grant | nucleus | base, length, generation (ADR-0050) |
| parent supervisor | nucleus | the creating process's instance id |
| start time | nucleus | monotonic tick (ADR-0049) |
| restart generation | supervisor | §4 |

Two entries deserve their reason stated. The **granted** set is asserted by the
nucleus and kept separate from the **requested** set, because the gap between
them is the only durable record that policy did something; a single merged field
would hide every denial. The **runtime engine id** is a consequence of ADR-0048:
once the engine runs per process, "which engine executed this" stops being a
property of the system and becomes a property of the process.

## 4. Restart

A restart produces a new process instance id and increments the restart
generation, keeping the same module and supervisor lineage. Identity is not
reused: an instance id that came back would make two different executions
indistinguishable in the log.

docs/37 requires that service restart preserve identity and audit records. That
means the lineage — module, source content id, supervisor, generation sequence —
survives, not that the instance is the same one.

## 5. What Stage 3 honestly cannot say

Stage 3 has no repository. A process launched from the capsule has a source set
naming the capsule and its digest, and its **system commit id is absent, not
guessed**. Writing the capsule's build commit there would report a commit the
system never read, which is exactly the failure Stage 1 was built to prevent.

Stage 5 replaces the capsule source set with the selected commit's tree, and the
field becomes present. Until then, absence is the true value.

## 6. Observability

Process identity is reported through the existing delegated runtime vocabulary
rather than a competing one: identifiers under `TOS.RUN.*`, one event per line,
same discipline as `RUNTIME_OBSERVABILITY_V1`. Extending that contract to a new
producer is a versioned change to it, not a new namespace, because two event
vocabularies describing one system eventually disagree.

Self-reports, when emitted, are distinguishable from launch records by
identifier. A reader must never have to guess which kind of claim it is holding.

## 7. Conformance evidence

1. The launch record for a process is reproducible from the artifact: recompute
   the source content id from the capsule bytes and get the same value.
2. A process that lies about itself changes only its self-report; the audit
   record is unchanged and the disagreement is emitted.
3. A denied capability appears as a difference between the requested and granted
   sets, and the process's `CapabilityDenied` startup failure names it.
4. Restart increments the generation, changes the instance id, and preserves the
   module and supervisor lineage.
5. The system commit id is absent for every capsule-launched Stage 3 process —
   asserted by a test, so that a later stage cannot make it present by accident.
6. Every field in §3 has exactly one asserter in the implementation, checked by
   a test that no field is written from two sources.
