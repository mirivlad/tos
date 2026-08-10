<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Glossary

**Active commit** — commit whose tree is mounted as the current immutable `/system`.

**Boot capsule** — immutable archive loaded by firmware/loader and containing text components required before persistent repository access.

**Boot-control record** — redundant transactional metadata selecting current, candidate, last-known-good, and recovery states.

**Bootstrap profile** — strict subset of TOS Core usable during early boot and recovery.

**Candidate commit** — commit staged for the next transactional activation but not yet promoted.

**Canonical source** — human-readable source text that defines a component; derived executable artifacts do not replace it.

**Capability** — unforgeable handle granting explicit authority over an object or operation.

**Content ID** — algorithm-qualified digest identifying immutable content.

**Derived artifact** — bytecode, native code, index, boot image, or other generated data reproducible from canonical source and declared inputs.

**Frontend** — module that translates a programming language into verified TOS IR.

**Last-known-good** — protected commit confirmed by required boot health checks.

**Nucleus** — minimal binary trusted base implementing isolation, processes, capabilities, IPC, boot substrate, and runtime bootstrap.

**Overlay** — writable layer containing proposed changes without modifying the active immutable tree.

**Protected ref** — system reference whose modification requires dedicated authority and audit.

**Recovery nucleus** — independently bootable trusted environment able to inspect, fetch, select, and restore system commits.

**Service** — supervised process publishing versioned interfaces.

**System commit** — repository commit representing a complete canonical system source state.

**TOS Core** — initial native textual language of TOS.

**TOS IR** — typed capability-aware intermediate representation shared by language frontends and execution engines.

**Transactional activation** — replacement procedure that either completes and becomes healthy or leaves/returns to the previous working version.

**Trusted base** — components whose compromise can subvert fundamental isolation or boot verification.

**Architecture conformance** — Evidence that a system preserves the active TOS invariants, not merely wire or syntax compatibility.

**Canonical source** — Human-readable installed representation that legally and technically defines a non-nucleus component.

**Defensive publication** — Public enabling disclosure intended to become prior art and reduce later patentability by others; not a freedom-to-operate opinion.

**DCO** — Developer Certificate of Origin, a contributor sign-off certifying the right to submit a contribution.

**Derived artifact** — Boot image, capsule, IR, bytecode, native cache or index generated from canonical source and tied to it by provenance.

**Identity-affecting ADR** — An architecture decision that changes an invariant or the defining relationship between source, runtime, history, owner control or trust.

**Installation Information** — GPLv3 term for methods, procedures, authorization keys or other information required in applicable User Products to install and execute modified covered software.

**Project Architect** — Official role responsible for protecting TOS architectural identity during foundational development.

**Reference oracle** — External implementation used to compare behavior or generate test vectors without becoming a TOS runtime dependency.

**Source-to-runtime traceability** — Ability to map a running process and executable derivative back to exact source, commit, frontend, verifier and capability grant.

**Compatibility profile** — precisely declared subset of an external format or ecosystem, such as Git G0–G6, with an associated conformance suite.

**Generated specification** — non-normative concatenated view built deterministically from listed source documents; never edited directly.

**Identity gate** — stage-specific proof that implemented functionality still expresses TOS architecture rather than a conventional substitute.

**Language foundation decision** — Stage 1.5 evidence-based selection of the parser/type/runtime basis fulfilling the TOS Core role.

**Performance contract** — versioned metric, workload, environment and threshold attached to an architectural path.

**Threat evidence level** — E0–E4 label distinguishing design intent, implementation, tests, adversarial evidence and formal argument.

**Vendor-controlled opaque material** — externally produced bytes consumed by hardware that TOS cannot express as editable source, such as CPU microcode or device firmware; identified by vendor, version and hash, never presented as TOS source.

**`/vendor`** — root namespace holding vendor-controlled opaque material, outside the canonical `/system` tree and outside the system commit.

**Vendor declaration** — canonical source text in `/system` naming a required `/vendor` object by vendor, identity, version, hash, placement and behavior on absence or mismatch; a reference, never an embedded payload.

