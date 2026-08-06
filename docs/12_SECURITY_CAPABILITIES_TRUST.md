<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Security, capabilities, and trust

The normative adversary model, assets, trust boundaries and accepted non-goals are defined in `docs/34_THREAT_MODEL.md`. This document defines the intended mechanism architecture.

## Position

TOS does not claim automatic safety merely because programs are readable text or because few attackers initially target it. Text improves inspection and provenance; it does not prevent malicious behavior.

Security is designed into boundaries from the first implementation so it does not require later architectural replacement.

## Threat categories

The summary categories include malicious modules and frontends, defective drivers, repository/boot-control tampering, compromised remotes, secret leakage, vulnerable rollback and supply-chain compromise. Exact adversary powers and required responses are maintained in `docs/34_THREAT_MODEL.md`; duplicate lists here are intentionally non-normative.

## Capabilities

Authority is represented by unforgeable handles. A capability identifies an object and permitted operations.

Examples:

- read a specific repository tree;
- update one protected ref;
- map one PCI BAR;
- bind one interrupt;
- publish one service interface;
- read one secret item;
- open one user-selected file;
- connect to one network endpoint class.

Capabilities can be attenuated and transferred only under explicit rules.

## Manifests and grants

A source module declares requested capabilities. Declaration does not grant them. A launcher policy maps requests to concrete grants based on:

- system commit trust;
- signer;
- module identity;
- user decision;
- machine policy;
- current boot mode;
- service role.

The effective grant is recorded in process identity and logs.

## Signed commits

Production mode may require system commits or protected refs signed by trusted keys. Signature policy is itself versioned and recoverable.

Signatures prove authorization and integrity, not correctness. Tests and health checks remain required.

## Repository protection

Ordinary processes cannot directly rewrite protected system refs or delete protected objects. Repository authority is mediated by a privileged service and nucleus primitives.

Audit records include:

- ref before and after;
- authorizing capability;
- actor process identity;
- commit metadata;
- signature result;
- timestamp source;
- rollback relationship.

## Secret isolation

Secrets are stored outside the system repository. Source modules refer to secret identifiers and request scoped access.

Logs must redact secret values by construction. Secret handles should support non-exportable operations where possible, such as signing without exposing private key bytes.

## Research mode

Research mode permits unsigned branches and broad capabilities after explicit operator selection at boot or trusted local authorization.

Requirements:

- mode is visibly indicated;
- protected recovery remains intact;
- remote push policy prevents accidental publication of secrets;
- audit records identify research mode;
- returning to production mode requires a clean trust evaluation.

## Anti-malware reality

Low popularity may reduce targeted malware temporarily, but cannot be a design control. TOS gains practical resilience from:

- immutable committed system trees;
- visible diffs;
- protected history;
- capability boundaries;
- signed refs;
- transactional activation;
- last-known-good boot;
- deterministic restoration.

The objective is not "no malware exists" but "changes are constrained, attributable, detectable, and reversible."

## Security must not become ownership denial

Official TOS distinguishes trust labels without equating “not signed by the vendor” with “forbidden to the owner.” A protected recovery action can authorize an owner key, one-time unsigned commit or experimental branch. The system records the decision and preserves last-known-good recovery.

Source availability without installation ability is not sufficient for TOS conformance. This architectural rule complements GPLv3 obligations where those legal conditions apply.

## Security evidence levels

Claims are labelled as design intent, implemented mechanism, tested property or formally verified property. Capability terminology does not imply seL4-level proof. Memory-safe implementation language does not eliminate unsafe code, hardware, logic, parser or supply-chain risk.
