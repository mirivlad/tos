<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS security policy

TOS is pre-implementation research software and currently offers no production security guarantee.

The normative threat model is `docs/34_THREAT_MODEL.md`. `docs/12_SECURITY_CAPABILITIES_TRUST.md` defines the intended security architecture. Both must be read before implementing privileged code, parsers, repository authority, language frontends or drivers.

## Reporting

Until a dedicated private channel is published, do not submit live credentials, exploit payloads against third parties or personal data. A future implementation repository must publish a private vulnerability contact before any network-capable release.

## Security claims

The project distinguishes:

- design intent;
- implemented mechanism;
- tested property;
- formally verified property.

No document or release may call TOS “secure,” “verified,” “memory safe” or “sandboxed” without naming the exact property, adversary assumptions and evidence.

## Owner control

Security mechanisms must protect recovery, provenance and capability boundaries without permanently preventing the owner from booting an experimental branch. Official distributions must make the difference between trusted, signed and owner-authorized code visible.

Vendor control over owner-installed commits is itself a threat considered by the project, not an automatic security feature.

## Supply chain

A release must include source identity, dependency inventory, licence inventory, build provenance, artifact digests and reproducibility status. Generated code and AI-assisted code are subject to the same review and provenance requirements as human-written code.

## Required updates

A change that introduces a new trust boundary, input parser, privileged service, remote protocol, DMA path, key store, boot-control mutation or executable cache must update the threat model or explicitly explain why existing entries cover it.
