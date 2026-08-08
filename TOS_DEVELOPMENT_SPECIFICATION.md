<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS — consolidated development specification

> **GENERATED FILE — DO NOT EDIT.**  
> This file is a non-normative convenience view. Individual source documents and accepted ADRs govern according to `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.

Version: 0.2.1  
Source-manifest SHA-256: `1d18aa40a43b17be7590f3832caba5e7470f5b9cec06eb0ab01ed4874da9992b`  
Generator: `tools/build-specification.py`

---

<!-- BEGIN README.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS — TextOS

                                                                                   aW              
                                                                                IWWWl              
                                                                               WWTYWl              
                                                                              aa   Wl              
                                                                             fW    nWn WWI         
                                                                          FJ pW      WTWkWz        
            lll  llCWWWll;                                               WWWupW       WW  aW       
          WWr  oMk      !kWWW;                                          Wo  oWk       w    MW      
         WW       rWWWWW     WWw                                       aW      ;W   W,      Wk     
        aWw      tCzaI  f   wWWWWn                                     ak     ak     ak     aM     
      aWl        tWWiW      WWk  CWi                                   Wk     ak     ak      W     
     Wv           WWt   Y   lWWWmttbWC                                 ok    WW       WW     W     
     WF  .         !rrr              uWWqJJ     nJJJv                  jW     ok     aa     aa     
     tWWY            uY                  fuuWuuuuuuuuuuuhWWWWWY;        WW    ak     ak    uW      
       xWccLWWWWWWWWc:      :      xWWw fchW    jWWu lcbW,    jcWWc      JWm   WW   WW    LW,      
         LWWr;              c      pWWWWq       LWWWWWT         rFIWWY     JWWr          WW        
            xLWWWWwF:        j       LLLL!        LWWWd!      LWWo L vWWT     CmWWW     rW!        
                   uWWWh     ak               lll.    ;        oWWWQ    dWWl    WWj     Wh         
                     pW;      Wq       .    ;WWW.;WWWWr    ;:     !        ckMkk       Wo          
                       WW      tWW      WF  ;WWWWW      pWk             ;WWW          WW           
                       ;WWWf     UW     .Wl    ,,,     MW               WWWa        wWk            
                       ;W  wWWpU  Wb     WW           YW.       wdi       ,l,     pWW              
                       Ww    ahtWWWW     Wl           YWi     LWf             wWWWt                
                   jWWnF    aW   jWl    dWWWWWWWWWWWWWWWQ     WcruWWWWWWWWWWWrr                    
                  WL YM    bX ,WWLi    mW            YWWW.    Wl                                   
                  !pWhWcWWwI ao IuJ  ,Wd           cWj fu.   tWl                                   
                              LWwWrWWJ             :mWWWcjWWWJ                                                        


**Architecture documentation version: 0.2.1 — 2026-08-06**

TOS is a text-centric operating system in which the canonical installed form of services, applications, language frontends, configuration and device drivers is human-readable source text. The unavoidable binary foundation is a deliberately small bootable nucleus and reproducible derived boot artifacts.

The name expands to **TextOS** and also carries the internal joke of the Russian abbreviation “ТОС”: a system intended to set conventional operating-system assumptions on fire. The public project name remains provisional pending trademark clearance.

## Quick start

The supported Stage 1 reference environment is x86_64 Linux with:

- QEMU system emulation for x86_64;
- a QEMU GTK backend (SDL is an automatic fallback; Debian/MX package:
  `qemu-system-gui`) for the interactive window;
- a matching OVMF CODE/VARS firmware pair;
- `mtools` (`mformat`, `mcopy` and `mmd`) and GNU `timeout`;
- `rustup` and the toolchain declared by `source/rust-toolchain.toml`, with the
  `x86_64-unknown-uefi` and `x86_64-unknown-none` targets.

The launcher reports any missing command, firmware file or Rust target; it does
not install software. From the repository root, start the human-facing boot:

```sh
./run-tos.sh
```

This builds the Stage 1 release artifacts, prepares the capsule and ESP through
the same harness used by CI, opens a GTK (or SDL fallback) QEMU display and
streams serial boot events in the terminal. A successful boot reaches
`TOS.HALT ok=0x10`, then the production nucleus stays halted so the visual
Stage 1 Pyro diagnostic and verification panel remain visible until you close
QEMU or press Ctrl+C. Its serial log is retained alongside the image
preparation evidence.

For a headless automated check, run:

```sh
./run-tos.sh --check
```

Serial and filtered event evidence is retained under
`source/target/run-tos/interactive/` or `source/target/run-tos/check/`, in
`serial.log` and `events.log`. `--check` is the self-judging mode: it enables
`isa-debug-exit`, returns raw QEMU exit 33 on success and prints
`QEMU-TEST PASS`. The interactive display is a human-facing representation of
the already-validated boot state; serial events remain the machine-readable
evidence.

The screen is not a desktop, shell or GUI subsystem. It is a best-effort
Stage 1 diagnostic drawn directly to the validated RGBX8/BGRX8 framebuffer;
it renders the separately identified CC-BY-SA-4.0 Pyro artwork only after a
successful validation. Its checked source/provenance relationship is recorded
in `assets/mascot/pyro-stage1-provenance.json`. When no framebuffer is
available, boot evidence remains the serial log.

Stage 1 is a bootable TOS foundation with source-bound capsule identity and
fail-closed validation. It is not yet a user shell, application environment or
desktop operating system, and Stage 1 is not declared closed while the formal
closure findings remain open.

## Core thesis

A conventional open-source operating system may publish source while installing binaries built elsewhere. TOS reverses the authority:

> The source tree is the installed system. Parsed IR, bytecode, native code, indexes, capsules and boot images are disposable derivatives with verifiable provenance.

The active system is identified by a commit. A machine can boot a known-good commit, branch its system, merge upstream changes, bisect regressions, push its system history to a remote and restore itself from a recovery nucleus plus repository.

## What makes TOS distinct

TOS is not merely:

- a microkernel with a scripting language;
- an immutable Linux distribution;
- source packages stored beside executables;
- Git used for developer configuration;
- a natural-language agent OS;
- a VM that happens to run drivers.

It is the conjunction of canonical installed text, source-to-runtime traceability, commit-addressed system identity, capability-confined textual services and drivers, owner-installable modification, transactional activation and repository-native recovery.

## Non-negotiable development rule

TOS is never developed as an MVP. It may be paused after any coherent stage, but foundations must not be intentionally temporary, fake or throwaway.

A narrow first platform is acceptable. A disposable architecture is not.

Every stage has both an engineering exit gate and a **TOS identity gate**. A stage does not close merely because conventional OS functionality works; it must produce evidence that the functionality still expresses the TOS model.

## Initial platform

- x86_64;
- UEFI;
- QEMU;
- deterministic preloaded boot capsule;
- serial and framebuffer diagnostics;
- VirtIO block first, followed by network/input/GPU;
- one active CPU initially, with interfaces designed not to block later SMP.

This is a platform boundary, not permission to bypass final trust and identity contracts.

## Documentation authority

The normative source is the set of individual documents and accepted ADRs defined by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.

`TOS_DEVELOPMENT_SPECIFICATION.md` is a **generated, non-normative convenience view**. It must never be edited manually. `tools/build-specification.py` regenerates it from `docs/SPECIFICATION_SOURCES.txt`, and CI rejects drift.

## Licensing and governance

- core OS implementation: `GPL-3.0-or-later`;
- public SDK/ABI/schema material explicitly designated: `Apache-2.0`;
- documentation: `CC-BY-SA-4.0`;
- contributions: DCO 1.1, no mandatory copyright assignment;
- patent strategy: defensive publication plus risk review;
- foundational governance: architect-led, with Vladimir Tomashevskiy as initial Project Architect.

See `LICENSE.md`, `GOVERNANCE.md`, `PATENTS.md`, `CONTRIBUTING.md` and `TRADEMARKS.md`.

## Required reading order

### Foundation and authority

1. `docs/00_PROJECT_CHARTER.md`
2. `docs/01_MANIFESTO.md`
3. `docs/02_SYSTEM_INVARIANTS.md`
4. `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`
5. `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`
6. `docs/37_STAGE_IDENTITY_GATES.md`
7. `docs/03_ARCHITECTURE_OVERVIEW.md`

### System design

8. `docs/04_BOOT_AND_RECOVERY.md`
9. `docs/05_TOS_CORE_LANGUAGE.md`
10. `docs/06_EXECUTION_AND_IR.md`
11. `docs/07_LANGUAGE_FRONTENDS.md`
12. `docs/08_GIT_NATIVE_SYSTEM.md`
13. `docs/36_GIT_COMPATIBILITY_PROFILES.md`
14. `docs/09_FILESYSTEM_AND_STATE.md`
15. `docs/10_PROCESS_SERVICE_IPC.md`
16. `docs/11_DRIVER_MODEL.md`
17. `docs/12_SECURITY_CAPABILITIES_TRUST.md`
18. `docs/34_THREAT_MODEL.md`
19. `docs/13_UPDATE_MERGE_PACKAGE_MODEL.md`
20. `docs/14_OBSERVABILITY_DEBUGGING.md`
21. `docs/15_TESTING_AND_VERIFICATION.md`
22. `docs/35_PERFORMANCE_CONTRACTS.md`
23. `docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md`

### Development and operations

24. `docs/16_DEVELOPMENT_STAGES.md`
25. `docs/17_REPOSITORY_LAYOUT.md`
26. `docs/18_CODING_STANDARDS.md`
27. `docs/19_RISKS_AND_OPEN_QUESTIONS.md`
28. `docs/20_GLOSSARY.md`
29. `docs/28_RELEASE_PROVENANCE_AND_REPRODUCIBILITY.md`
30. `docs/30_COMPLIANCE_AND_RELEASE_GATES.md`

### Legal, governance and ecosystem

31. `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`
32. `docs/23_CONTRIBUTION_PROVENANCE.md`
33. `docs/24_PATENT_POLICY.md`
34. `docs/25_DEFENSIVE_PUBLICATION_PROTOCOL.md`
35. `docs/26_NAME_TRADEMARK_AND_CONFORMANCE.md`
36. `docs/27_THIRD_PARTY_COMPONENT_POLICY.md`
37. `docs/29_PROJECT_GOVERNANCE.md`
38. `docs/32_EXTERNAL_IMPLEMENTATION_POLICY.md`
39. `docs/33_LEGAL_AND_RESEARCH_SOURCES.md`
40. `docs/research/`
41. `docs/adr/`

`AGENTS.md` contains mandatory instructions for all coding agents. `CODEX_START.md` is the first implementation task packet.

## Status

Version 0.2.1 closes the documentation gaps found in the first external architecture review: normative-document drift, missing threat model, deferred language-foundation decision, unmeasured driver performance, underspecified Git compatibility and the risk that early conventional OS work could lose TOS identity.

The package is the accepted architecture and policy baseline for beginning Stage 1. No implementation decision may silently contradict it. Invariant changes require an identity-affecting ADR. Legal documents are project policy, not jurisdiction-specific legal advice.

<!-- END README.md -->

---

<!-- BEGIN CHANGELOG.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Changelog

## 0.2.1 — 2026-08-06

External architecture-review corrections.

### Added

- normative documentation hierarchy and deterministic consolidated-spec generator;
- CI guard preventing manual or stale `TOS_DEVELOPMENT_SPECIFICATION.md`;
- full threat model with adversary classes, assets, trust boundaries and stage mapping;
- quantitative performance contracts for runtime, IPC, drivers and repository operations;
- explicit Git compatibility profiles G0 through G6;
- TOS identity gates for every development stage;
- Stage 1.5 language-foundation decision gate and evaluation matrix;
- ADR-0015 establishing that no parser/runtime foundation is implemented before comparative architecture review.

### Changed

- the consolidated specification is now explicitly generated and non-normative;
- Stage 1 requires actual repository identity and source-commit provenance;
- Stage 2 no longer begins until the language foundation is selected by ADR;
- Stage 4 requires measured driver-path budgets rather than qualitative performance claims;
- Stage 5 requires a declared Git compatibility profile instead of an undefined promise of “Git support”;
- all stages now require identity evidence proving that TOS has not become a conventional microkernel with scripts;
- agent and pull-request instructions now require threat, performance, compatibility and identity impact review.

### Removed

- no project files removed; manual authority of the consolidated specification is explicitly rejected.

## 0.2.0 — 2026-08-05

Architecture and legal-governance baseline revision.

### Added

- GPLv3-or-later / Apache-2.0 / CC BY-SA 4.0 licence matrix and full licence texts;
- DCO 1.1 contribution model without copyright assignment;
- architecture-preservation policy and architect-led governance;
- patent policy, preliminary patent landscape and defensive-publication protocol;
- provisional-name, trademark and conformance policy;
- third-party dependency and external-oracle policy;
- release provenance and reproducibility grades;
- architecture conformance and legal release gates;
- source-to-runtime, owner-installability and derived-provenance invariants;
- GitHub pull-request architecture checklist and third-party inventory scaffold;
- ADR-0007 through ADR-0014.

### Changed

- boot capsule now has explicit source-commit and builder provenance;
- TOS Core and external language/runtime selection now require architecture review;
- Git implementations default to host tools/oracles rather than nucleus dependencies;
- driver porting policy distinguishes hardware knowledge from copyrighted implementation;
- Linux GPL-2.0-only code is explicitly excluded from direct copying into GPLv3 TOS components;
- Stage 0 now includes governance, licensing, defensive publication and naming readiness;
- Codex Stage 1 task includes SPDX, DCO, provenance and architecture-conformance requirements.

### Supersedes

This package supersedes architecture documentation version 0.1.0. It does not claim implementation completion or legal freedom to operate.

<!-- END CHANGELOG.md -->

---

<!-- BEGIN ARCHITECTURE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS architecture authority

The normative hierarchy is defined by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.

In summary, architecture authority descends through:

1. active system invariants;
2. accepted ADRs and architecture-preservation policy;
3. normative subsystem specifications;
4. stage, testing and release policies;
5. explanatory root documents;
6. generated convenience views.

`TOS_DEVELOPMENT_SPECIFICATION.md` is generated from normative sources and is never independently authoritative. If it differs from an individual source document, the source document governs and the generated file must be rebuilt.

The project does not use an MVP phase. A narrow implementation is valid only when it exercises the intended long-term contract. A quick demonstration that requires replacement of its trust boundaries, source model, driver placement, object identity or recovery path is not a TOS milestone.

Architecture-preserving substitutions are possible. For example, a different hash algorithm, scheduler or parser implementation may be accepted if it preserves the relevant contracts. Architecture-erasing substitutions—such as replacing canonical text with canonical binaries, embedding a general third-party runtime into the trusted nucleus for convenience, or making Git merely a developer tool—require rejection or an explicit identity-affecting ADR.

Every stage must satisfy `docs/37_STAGE_IDENTITY_GATES.md`; conventional feature completion alone is insufficient.

<!-- END ARCHITECTURE.md -->

---

<!-- BEGIN LICENSE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS licensing map

TOS uses established licenses rather than a project-specific license. This repository is intentionally multi-licensed by component class.

## License matrix

| Material | Default license | SPDX identifier |
|---|---|---|
| Nucleus, boot code, reference runtime, official system services, official drivers, activation and recovery implementation | GNU General Public License version 3 or later | `GPL-3.0-or-later` |
| Public SDKs, ABI definitions, IPC schemas, conformance harness libraries, independent integration libraries and reusable test vectors explicitly marked as such | Apache License 2.0 | `Apache-2.0` |
| Architecture documents, specifications, tutorials, diagrams, governance and policy documents | Creative Commons Attribution-ShareAlike 4.0 International | `CC-BY-SA-4.0` |
| Code fragments embedded in documentation, unless a fragment says otherwise | dual licensed | `GPL-3.0-or-later OR Apache-2.0` |
| Network services intentionally designated in their own directory | GNU Affero General Public License version 3 or later | `AGPL-3.0-or-later` |

No directory becomes AGPL-licensed merely because it communicates over a network. An AGPL component requires an explicit ADR and SPDX declaration.

## Why GPLv3-or-later for the operating system

TOS is designed so that the owner can inspect, modify and boot the actual source identity of the installed system. GPLv3 is selected because reciprocal source obligations alone are not enough for this project: when its conditions apply to a User Product, GPLv3 also addresses the information necessary to install and execute modified versions. That aligns with TOS invariant I-17: official TOS must not expose source while technically locking the owner out of loading it.

The project uses the `or-later` form to permit migration to a future GNU GPL version if the project governance later accepts it through an ADR. A distributor may always use GPLv3 under the current grant; no future migration may retroactively remove rights already granted.

## Why Apache-2.0 for interfaces and SDK material

TOS should permit independent applications, tools, language frontends and compatible implementations. Stable public interfaces therefore use a permissive license with an express patent grant. Apache-licensed interface material may be combined into the GPLv3 TOS implementation, while independent projects can use it without becoming GPL-covered merely by copying an interface library or schema.

This boundary must not be abused to move operating-system implementation into an Apache directory. The architecture-preservation policy decides whether material is an interface or part of TOS itself.

## Documentation license

Documentation is licensed under CC BY-SA 4.0. Implementing a documented protocol or idea does not automatically copy the wording of its specification. Modified copies of TOS documentation must retain attribution and ShareAlike obligations.

## File-level declarations

Every source file added by the project must carry an SPDX identifier in the conventional comment syntax for its format. Generated artifacts must carry provenance metadata identifying the licenses of their canonical sources; generated artifacts are not a way to remove license notices.

Examples:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
```

```text
# SPDX-License-Identifier: Apache-2.0
```

```markdown
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
```

## Full texts

The corresponding license texts are stored in `LICENSES/`:

- `LICENSES/GPL-3.0-or-later.txt`
- `LICENSES/Apache-2.0.txt`
- `LICENSES/CC-BY-SA-4.0.txt`

If an AGPL component is accepted, the repository must add the official AGPLv3 text before that component is merged.

## Copyright and contributions

Copyright remains with contributors. TOS does not require assignment of copyright to the project architect or a foundation. Contributions are accepted under the Developer Certificate of Origin 1.1 in `DCO`; every commit must contain a valid `Signed-off-by` trailer.

## No extra field-of-use restriction

TOS does not add clauses forbidding particular industries, military use, commercial use, AI use, or other fields of endeavor. Such clauses would no longer be conventional open-source licensing and would create incompatible custom terms. Ethical positions may be stated as non-binding project values, but they are not license restrictions.

## No legal warranty

This file documents project policy and is not legal advice. Before a commercial hardware distribution, jurisdiction-specific counsel should review the release, third-party notices, installation-information obligations and patent exposure.

<!-- END LICENSE.md -->

---

<!-- BEGIN CONTRIBUTING.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Contributing to TOS

TOS welcomes contributions that preserve its architectural identity. The project deliberately accepts narrow progress more readily than broad shortcuts.

## Before contributing

Read, in order:

1. `README.md`;
2. `docs/02_SYSTEM_INVARIANTS.md`;
3. `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`;
4. `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`;
5. `docs/23_CONTRIBUTION_PROVENANCE.md`;
6. the subsystem specification and accepted ADRs.

Automated contributors must also follow `AGENTS.md`.

## Contribution classes

### Class A — implementation-preserving

Bug fixes, tests, diagnostics, documentation corrections and implementations that clearly follow accepted contracts. These may be reviewed normally.

### Class B — contract-extending

New public API fields, wire-format versions, capability types, on-disk structures, language semantics or new top-level components. These require a design note and normally an ADR.

### Class C — identity-affecting

Any proposal that changes canonical-source semantics, nucleus boundaries, owner boot control, Git-native identity, licensing boundaries, driver placement, architecture governance or an invariant. These require an ADR and explicit approval by the Project Architect. A pull request may not smuggle a Class C decision inside implementation code.

## Required commit sign-off

All commits must include:

```text
Signed-off-by: Real Name <email@example.com>
```

The sign-off certifies the Developer Certificate of Origin 1.1 in `DCO`. It is not a transfer of copyright.

Create commits with Git's sign-off option so the trailer is not forgotten:

```sh
git commit -s
```

Before pushing, run the local repository gates from the repository root:

```sh
./scripts/preflight.sh
```

Use `./scripts/preflight.sh --full` when the change touches boot, capsule parsing
or QEMU-visible behavior; it additionally runs fuzzing and both QEMU suites.
Preflight reports all selected gate results and does not install missing tools.

## AI-assisted contributions

AI tools may be used, but the human submitter remains responsible for:

- the origin and license compatibility of the contribution;
- reviewing every material change;
- ensuring no third-party code was reproduced without permission;
- disclosing any known generated-code provenance concern;
- running the required tests;
- signing the DCO personally.

An AI system cannot provide a DCO sign-off and cannot be listed as the legal author.

## Third-party code

Do not paste code merely because it is publicly visible. Record source, exact license, version or commit, modifications and compatibility in `THIRD_PARTY.toml` or the future equivalent inventory.

In particular, the Linux kernel is generally GPL-2.0-only. GPL-2.0-only code cannot simply be copied into a GPL-3.0-or-later TOS component. Linux drivers are valuable sources of hardware knowledge, register behavior and references to specifications, but direct copying requires file-level license review and may be prohibited. Prefer public hardware specifications, permissively licensed code, GPL-2.0-or-later code, or a documented clean-room reimplementation.

## Repository assets

Text assets carry an SPDX identifier in the first five lines, using the
existing `LICENSE.md` component class that applies to the material. Do not
select a new licence merely because a file extension is new.

Binary artwork cannot use a normal source comment. Its directory therefore
contains a tracked `README.md` that lists each binary path, its licence under
the existing matrix, its origin and the Git contribution that introduced it.
The SPDX gate checks the record path-by-path. Adding a blanket extension
exemption is not an acceptable substitute for provenance.

Imported or adapted assets additionally follow
`docs/23_CONTRIBUTION_PROVENANCE.md` and are recorded in `THIRD_PARTY.toml` when
applicable. If origin or licensing cannot be established, the asset is blocked.

## Completion standard

A contribution is complete only when:

- implementation and tests agree with the specification;
- changed contracts are documented in the same change;
- formats have versions and stable test vectors;
- failure behavior is tested;
- required license and provenance metadata is present;
- the architecture conformance checklist passes;
- no known limitation is hidden.

<!-- END CONTRIBUTING.md -->

---

<!-- BEGIN GOVERNANCE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS governance

TOS begins as an architect-led open project.

## Project Architect

The initial Project Architect is Vladimir Tomashevskiy. The role owns the architectural intent, accepts or rejects invariant-changing ADRs, defines coherent stage boundaries and may refuse changes that make TOS more conventional at the cost of its identity.

This role does not own contributor copyrights and cannot revoke open-source rights already granted.

## Maintainers

Subsystem maintainers may accept implementation-preserving changes inside accepted contracts. They may not redefine project invariants, licensing boundaries or the owner-control model without an accepted ADR.

## Decision hierarchy

1. applicable law and third-party licence obligations;
2. accepted system invariants;
3. accepted ADRs;
4. normative subsystem specifications;
5. implementation and tests;
6. informal discussion.

A lower level cannot silently override a higher level.

## Architecture amendments

An invariant amendment requires:

- a dedicated ADR labelled `Identity-affecting`;
- an explicit explanation of why TOS remains TOS after the change;
- rejected alternatives;
- migration and rollback consequences;
- approval by the Project Architect;
- a release-note entry.

## Succession

If the Project Architect formally steps down, a signed governance commit may appoint a successor or architecture council. Mere inactivity does not authorize rewriting the invariants on the official branch; forks remain free to choose different rules under the software licences.

See `docs/29_PROJECT_GOVERNANCE.md`.

<!-- END GOVERNANCE.md -->

---

<!-- BEGIN PATENTS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS patent policy

TOS follows a defensive, disclosure-first patent policy.

## Project position

- The TOS Project does not plan to seek software patents on the core architecture.
- Significant original architecture should be published in a dated, durable and searchable form so that it can serve as prior art.
- Publication is not a freedom-to-operate opinion. Existing patents can still create implementation risk even when the project independently invents a system.
- No contributor is expected to perform a worldwide patent search for ordinary patches.
- A contributor must disclose any patent claim they actually know is intentionally required by their contribution.
- The project does not accept a contribution accompanied by a private patent licence that cannot extend to downstream recipients on compatible terms.

## Patent review triggers

A focused review is mandatory before accepting designs for:

- content-addressed update and rollback mechanisms;
- verified native or bytecode caches tied to source identity;
- user-space interrupt delivery and DMA isolation;
- remote restoration and fleet activation;
- hardware-distributed textual drivers;
- a commercial appliance or User Product;
- any implementation deliberately modelled on a patented technique.

## Public records

The preliminary landscape is maintained in `docs/research/PATENT_LANDSCAPE.md`. It is a risk register, not legal advice. Each entry records jurisdiction, family, apparent status, relevant independent-claim concepts, TOS intersection and design response.

## Assertions against TOS

Patent demands, threats or licence offers must be preserved unmodified and escalated to maintainers. Developers should not admit infringement, promise payment or publicly speculate about claim construction. The project will prefer design-around, prior-art evidence, community defence and qualified counsel.

See `docs/24_PATENT_POLICY.md` and `docs/25_DEFENSIVE_PUBLICATION_PROTOCOL.md`.

<!-- END PATENTS.md -->

---

<!-- BEGIN SECURITY.md -->

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

<!-- END SECURITY.md -->

---

<!-- BEGIN TRADEMARKS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS name and trademark policy

`TOS` and `TextOS` are provisional project identifiers until a professional clearance search is completed in the jurisdictions relevant to public distribution.

The short name `TOS` has extensive prior use, including Atari TOS and the generic abbreviation “terminal operating system” in logistics. The project therefore makes no claim in this repository that the name is registrable, exclusive or legally cleared.

## During the provisional period

- Use the combined form **TOS — TextOS** in public-facing material.
- Do not copy Atari visual identity, iconography, historical logos or product presentation.
- Do not imply affiliation with Atari, EmuTOS, FreeMiNT, port terminal software vendors or military equipment manufacturers.
- Repository namespaces and package identifiers should support a later rename without changing protocol semantics.
- Protocol magic values should not depend solely on the letters `TOS`.

## Future mark policy

If the name is cleared and adopted, code freedom will remain separate from brand identity. Compatible unmodified releases may use the official name. Modified releases may accurately describe themselves as “based on TextOS,” but materially non-conforming forks must use a distinct product name.

Conformance is never used to forbid forks. It only prevents confusion about which systems preserve the published TOS invariants.

<!-- END TRADEMARKS.md -->

---

<!-- BEGIN docs/00_PROJECT_CHARTER.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS project charter

## Purpose

TOS explores a different relationship between an operating system, its source code, its installed state, and its history.

The project exists to build an operating system where:

- the installed system is inspectable source text;
- changing source text changes the system without a separate package-build-install cycle;
- executable caches are derived and disposable;
- the system is identified by a repository commit;
- rollback, branching, merging, cloning, and bisecting are ordinary system operations;
- device support can be delivered as textual driver modules;
- multiple programming languages can be added as textual frontend modules targeting one common execution model;
- the owner retains the right to inspect, modify, replace, and recover every non-firmware component.

## Success definition

TOS succeeds when a user can perform the following sequence on a supported machine:

1. boot a trusted nucleus and select a system commit;
2. run the system whose canonical components are text files from that commit;
3. inspect the exact source currently responsible for a service or driver;
4. modify the source in the running system;
5. validate and activate the new module transactionally;
6. commit the system change with tests and metadata;
7. push the history to a standard remote;
8. restore another machine from the nucleus plus that repository;
9. boot an earlier commit after a regression;
10. use automated bisect and health checks to locate the first bad system commit.

## Development stance

TOS does not pursue an MVP. The term obscures an important distinction:

- **Limited platform support** is acceptable.
- **Limited architectural integrity** is not.

A milestone may implement only one architecture and a small device set, but the interfaces created at that milestone must be intended to survive.

The project may be paused when time, energy, or resources run out. A paused coherent system is preferable to a quickly demonstrated pile of shortcuts that must later be discarded.

## Initial supported use

The first target is a research and enthusiast operating system running under QEMU. The initial goal is not desktop replacement, application compatibility, or mass adoption. The goal is to prove and mature the TOS model under controlled hardware while retaining a path to physical machines.

## Governance

Governance is architect-led during the foundational phases. The decision hierarchy is:

1. applicable law and third-party licence obligations;
2. system invariants;
3. accepted Architecture Decision Records;
4. normative subsystem specifications;
5. conformance tests and implementation;
6. informal design discussion.

The initial Project Architect is Vladimir Tomashevskiy. The role protects the project thesis, accepts identity-affecting ADRs and decides whether a stage has reached a coherent architectural boundary. Subsystem maintainers may decide implementation details inside accepted contracts, but they cannot waive invariants silently.

This authority applies to the official project only. The open-source licences preserve the right to fork. Copyright remains with contributors under the DCO model.

See `GOVERNANCE.md`, `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`, and `docs/29_PROJECT_GOVERNANCE.md`.

## Licensing

TOS uses a component-based established licence model accepted by ADR-0007:

- operating-system implementation: `GPL-3.0-or-later`;
- SDK, ABI, schemas and designated independent interface libraries: `Apache-2.0`;
- documentation: `CC-BY-SA-4.0`;
- documentation code samples: `GPL-3.0-or-later OR Apache-2.0`;
- a future official hosted network service may use `AGPL-3.0-or-later` only through its own ADR.

Contributors retain copyright and certify contributions through Developer Certificate of Origin 1.1 sign-off. No mandatory assignment of copyright is required.

The licensing strategy is part of the architecture: official TOS should not be distributable as a source-visible but owner-locked appliance. See `LICENSE.md` and `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`.

## Public-interest and patent stance

TOS is intended to remain a public technical commons. The project does not plan to seek patents on its core architecture by default. Original enabling architecture is prepared for defensive publication. Patent risk is tracked honestly, and a professional freedom-to-operate review is required before material commercial distribution.

The project does not claim that publication or independent invention prevents infringement of earlier valid claims.

<!-- END docs/00_PROJECT_CHARTER.md -->

---

<!-- BEGIN docs/01_MANIFESTO.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS manifesto

## The installed program should not hide from its owner

Most operating systems separate the code a person may inspect from the artifact the computer actually executes. Even open-source systems commonly install compiled binaries whose relationship to available source code depends on external build infrastructure, package metadata, and trust.

TOS treats this separation as optional rather than fundamental.

The authoritative form of a service, driver, shell, application, language frontend, and system policy is human-readable text in the active system commit. The runtime may parse, lower, optimize, cache, or translate that text, but those products do not replace the source as the definition of the program.

## A machine should have history, not merely state

A conventional machine accumulates state through installations, upgrades, scripts, package hooks, manual edits, and undocumented recovery actions. The final disk contents often do not explain how the machine arrived there.

TOS makes the system tree a commit graph. Every durable change can have:

- a parent state;
- an author;
- a reason;
- a diff;
- test results;
- hardware context;
- signatures;
- boot-health records.

A machine can therefore answer not only "what is installed?" but "which change made it so?"

## Recovery is a first-class operation

Reinstallation should not be the standard answer to uncertainty. A TOS machine must always know:

- its current commit;
- the previous commit;
- the last commit that passed boot health checks;
- a protected recovery commit or recovery source;
- whether the working overlay contains uncommitted changes.

Restoring a machine is repository reconstruction, not archaeological package management.

## Text does not mean slow by decree

TOS does not fetishize character-by-character interpretation. The source is canonical; execution strategy is an implementation detail.

The runtime may create:

- abstract syntax trees;
- typed intermediate representations;
- bytecode;
- native code;
- persistent caches keyed by source hash and runtime version.

All such artifacts must be reproducible, invalidatable, and deletable without losing the program.

## Extensibility must not enlarge the trusted core without limit

New languages, drivers, services, and applications should normally be installed as textual modules outside the binary nucleus. The nucleus provides mechanisms: memory isolation, scheduling, capabilities, IPC, object access, and boot selection. Policy belongs in textual system components.

## The owner is allowed to break the machine

TOS should provide safe defaults, signatures, capability boundaries, transactional activation, and recovery. It must not convert those protections into ownership denial.

An explicit research mode may permit unsigned commits, experimental drivers, and unrestricted system branches. The system should make risk visible and recovery easy rather than pretending the owner is an attacker.

## Open here, not somewhere else

A system is not fully open merely because a source archive exists on a developer server. TOS should expose the exact source identity of what is running on the owner’s machine, the commit that selected it, the derived artifact that executes it, the capabilities it holds and the history that introduced it.

The desired chain is visible:

```text
source path and content hash
    -> system commit
    -> validated IR/cache identity
    -> running process
    -> granted capabilities
```

The owner should be able to branch that chain and boot the result. Open source that cannot be installed by the owner is incomplete for TOS.

## Openness is architectural and legal

Architecture provides inspectability, source identity, modification and recovery. Copyleft provides downstream legal continuity. Neither substitutes for the other. A readable system under a closing licence can be enclosed; an open licence over an opaque installed binary can remain practically inaccessible. TOS requires both layers.

<!-- END docs/01_MANIFESTO.md -->

---

<!-- BEGIN docs/02_SYSTEM_INVARIANTS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# System invariants

These statements define TOS. Violating one requires an explicit ADR that amends this document.

## I-01 — Canonical source

For every non-nucleus executable component, human-readable source text is the canonical installed representation.

Generated bytecode, native code, indexes, and optimized images are caches. Deleting them must not remove functionality, only force regeneration or slower execution.

## I-02 — Minimal binary trusted base

The permanently binary trusted base contains only what is required to start the machine, isolate execution, access the boot capsule, expose primitive hardware mechanisms, verify repository state, and launch textual components.

Features must not move into the nucleus merely for convenience.

## I-03 — Repository-addressed system

The immutable `/system` tree visible to a booted system corresponds to a specific commit identity. A boot record must be able to name that commit unambiguously.

## I-04 — Separation of source and mutable state

Runtime state, logs, caches, secrets, user data, and transient files do not modify the active system commit implicitly.

Durable system source changes require an explicit working overlay and commit operation.

## I-05 — Transactional activation

A new system commit, service revision, driver revision, or language frontend must be validated before replacing the currently active version. Activation either completes atomically or leaves the prior version active.

## I-06 — Recoverable boot

The boot path must support a protected last-known-good state and an independent recovery environment. A failed candidate boot must not destroy the ability to select an older commit.

## I-07 — Explicit authority

Processes and drivers receive explicit capabilities. Ambient global privilege is avoided. A module declares required capabilities, and the launcher grants a concrete subset.

## I-08 — User-space drivers by default

Drivers run as isolated services unless operation before process isolation is technically unavoidable. Bootstrapping hardware access is passed through narrow nucleus primitives or preloaded capsules.

## I-09 — Versioned boundaries

Boot ABI, IPC schemas, repository metadata, capsule format, driver contracts, language frontend contracts, and cache formats are versioned from their first implementation.

## I-10 — Deterministic identity

Content identity, commit selection, source normalization, module dependency resolution, and generated cache keys must be deterministic.

## I-11 — Observable execution

The system can identify which source file, content hash, module version, commit, and granted capabilities produced a running component.

## I-12 — No hidden build requirement at runtime

A restored TOS installation requires only the trusted nucleus, recovery shell, repository data, and documented runtime services. It must not require an undocumented host compiler or developer workstation to execute ordinary textual components.

## I-13 — Architecture before demonstration

No project milestone may be declared complete by bypassing an intended subsystem with a known throwaway path. Demonstrations must exercise the real contract for the functionality they claim.

## I-14 — Owner-controlled experimentation

The system provides a clearly marked mode in which the owner may boot unsigned or experimental branches. Safety controls remain visible and recovery remains available, but policy does not permanently lock the owner out.

## I-15 — Honest compatibility

Language compatibility, Git compatibility, hardware support, and source portability are stated precisely. A subset is called a subset. A translation is called a translation. TOS must not claim to run an ecosystem merely because it accepts superficially similar syntax.

## I-16 — Source-to-runtime traceability

Every running non-nucleus component can report its canonical source path, content hash, system commit, frontend version, IR/cache identity, execution engine and granted capabilities. An executable derivative with no verifiable source relationship is not an official TOS textual component.

## I-17 — Owner-installable modification

An official TOS distribution must provide a documented path for the owner to authorize, validate and boot modified system source. Security policy may distinguish vendor-trusted, community-trusted and owner-authorized branches, but it must not permanently require an undisclosed vendor secret to run owner modifications.

## I-18 — Derived-artifact provenance

Boot images, capsules, bytecode and native caches record enough provenance to identify canonical inputs, source commit, builder/runtime identity, target ABI and output digest. They remain disposable derivatives.

## I-19 — External dependency containment

Existing implementations default to references, host tools or test oracles. A language runtime, Git library, driver framework or filesystem implementation may enter the runtime or trusted base only through explicit architecture, licence and dependency review.

## I-20 — Legal continuity of openness

Official TOS components use licences that preserve the intended downstream freedoms. Licence boundaries are explicit, SPDX-identified and cannot be moved merely to avoid reciprocal obligations.

## I-21 — Architectural identity is not temporary debt

A stage may omit breadth, but it may not close while relying on a known identity-erasing substitute. No roadmap promise that “the real TOS will replace this later” converts a conventional shortcut into an accepted foundation.

<!-- END docs/02_SYSTEM_INVARIANTS.md -->

---

<!-- BEGIN docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Normative document hierarchy

## Purpose

TOS documentation is large enough that duplicated text can drift. This document defines authority, conflict resolution and generated views.

## Authority tiers

### Tier 0 — Project identity

- `docs/02_SYSTEM_INVARIANTS.md`

An active invariant overrides every lower-tier document. Changing an invariant requires the Level 4 process.

### Tier 1 — Accepted architectural decisions and preservation rules

- accepted files under `docs/adr/`;
- `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`;
- `docs/29_PROJECT_GOVERNANCE.md` for decision authority.

A later accepted ADR may supersede an earlier ADR explicitly. Silent contradiction is invalid.

### Tier 2 — Normative subsystem and policy specifications

Numbered documents under `docs/` that define boot, language, execution, repository, state, IPC, drivers, security, stages, testing, legal policy and release gates.

Accepted versioned interface contracts under `source/interfaces/` are also
Tier 2 only when all of the following are true:

- their status explicitly says `Accepted Tier 2 interface contract`;
- they are listed in `docs/SPECIFICATION_SOURCES.txt`;
- they explicitly reference this hierarchy; and
- they acknowledge Tier 0 invariant and accepted Tier 1 ADR precedence.

Listing a path in `docs/SPECIFICATION_SOURCES.txt` does not by itself grant
Tier 2 authority to any other listed material. Directory placement, generated
view inclusion and a contract's own “normative” claim are insufficient.

A subsystem document must conform to Tier 0 and Tier 1. Where two Tier 2 documents overlap, the more specific subsystem contract governs only if it cites the general document and does not violate higher tiers.

### Tier 3 — Root operational documents

`README.md`, `ARCHITECTURE.md`, `AGENTS.md`, `CODEX_START.md`, `CONTRIBUTING.md`, `SECURITY.md`, `GOVERNANCE.md`, `PATENTS.md`, `LICENSE.md` and similar entry points.

They summarize or operationalize normative sources. They do not silently amend them.

### Tier 4 — Research and explanatory material

Files under `docs/research/`, examples, evaluations and historical notes unless an ADR explicitly incorporates a result.

### Tier 5 — Generated views

`TOS_DEVELOPMENT_SPECIFICATION.md` and other generated bundles.

Generated views are never independent authority. Their purpose is transport, review and model ingestion.

## Consolidated specification rule

`TOS_DEVELOPMENT_SPECIFICATION.md`:

- is built only by `tools/build-specification.py`;
- takes ordered inputs from `docs/SPECIFICATION_SOURCES.txt`;
- contains a generated-file warning and source-manifest digest;
- must not be manually edited;
- is checked by CI for byte-for-byte reproducibility;
- is replaced whenever any listed source changes.

If a generated view differs from a source file, the source file governs and the generated view is stale.

## Conflict protocol

When a conflict is found:

1. stop implementation at the affected boundary;
2. identify the authority tier and exact passages;
3. open an architecture issue or ADR as required;
4. correct the lower-authority document or explicitly supersede the higher decision;
5. regenerate derived views;
6. add a test or lint rule if the conflict was mechanically detectable.

Agents must not resolve conflicts by choosing the easiest implementation.

## Amendment rules

- invariant changes: Level 4 identity amendment;
- accepted ADR changes: new superseding ADR, except spelling/link fixes;
- subsystem contract changes: Level 2 or 3 according to impact;
- generated view changes: never direct; regenerate from sources;
- research notes: may evolve but must not be cited as accepted architecture without promotion.

## Normative language

The terms **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **MAY** and **OPTIONAL** are normative when capitalized. Ordinary prose remains binding when clearly stated as an invariant, requirement, exit gate or accepted decision.

## Release check

A documentation release is invalid if:

- the generated consolidated specification is stale;
- a listed source is missing;
- an accepted ADR is absent from the source manifest;
- document version metadata disagrees;
- an unresolved normative conflict is known.

<!-- END docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md -->

---

<!-- BEGIN source/interfaces/boot/BOOT_ABI_V1.md -->

<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Boot ABI — Version 1

Status: **Accepted Tier 2 interface contract.**

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs.

## 1. Role

The loader constructs one `BootInfo` block in memory, then transfers control to
the nucleus entry point exactly once. The block is a versioned standalone ABI:
no Rust layout is authoritative; this byte layout is.

## 2. Constants

| Name | Value | Meaning |
|---|---|---|
| `MAGIC` | `u64::from_le_bytes(b"TOSBOOT1")` = `0x31544F4F42534F54` | ABI magic |
| `PROTOCOL_UUID` | `e2e8c15a-6c4b-4d11-9a2c-8f3b1a2c4d5e` (16 bytes, RFC order) | ABI identity |
| `MAJOR` / `MINOR` | 1 / 0 | ABI version |
| `STRUCT_SIZE` | 224 | size of BootInfo v1 |
| `ARCH_X86_64` | 1 | architecture id |
| `BOOT_MODE_NORMAL` | 0 | boot mode |
| `FB_FORMAT_NONE` | 0 | framebuffer absent |
| `FB_FORMAT_RGBX8` | 1 | bytes `R,G,B,X`; X ignored |
| `FB_FORMAT_BGRX8` | 2 | bytes `B,G,R,X`; X ignored |
| `MEM_DESC_SIZE` | 24 | memory-range descriptor size |
| `RESULT_PORT` | `0x501` | QEMU `isa-debug-exit` I/O port |
| `RESULT_HALT_OK` | `0x10` | clean halt |
| `RESULT_PANIC` | `0x20` | nucleus panic |
| `RESULT_CAPSULE_INVALID` | `0x21` | capsule rejected |
| `RESULT_ABI_INVALID` | `0x22` | boot ABI rejected |
| `RESULT_MEMORY_INVALID` | `0x23` | memory map rejected |
| `RESULT_EXCEPTION` | `0x24` | caught CPU exception |

Result codes are written to `RESULT_PORT` as one `u8`; QEMU exits with
`(value << 1) | 1` when `isa-debug-exit` is configured.

## 3. Calling convention

- Entry: 64-bit long mode, paging enabled (identity mapping covering the kernel
  image, capsule, boot info and memory-map regions), GDT valid.
- Argument: `rdi = physical address of BootInfo`.
- `rsp`: valid stack configured by the loader.
- No other register contents are part of the ABI.
- The nucleus must not return; it either halts via `RESULT_PORT` or panics.

Before reading or trusting BootInfo-controlled memory, the nucleus MUST install
its Stage 1 exception foundation: a nucleus-owned GDT/TSS and a present,
DPL-0, 64-bit interrupt-gate IDT for CPU exception vectors 0 through 31.
Maskable external interrupts remain disabled; vectors above 31 are not an
interrupt ABI in v1. Every Stage 1 exception handler is fatal and MUST NOT
resume through `iretq`. Vector 8 (#DF) MUST use a dedicated bounded
nucleus-owned IST stack.

## 4. BootInfo layout (224 bytes, little-endian, 8-aligned)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 8 | `magic` | must equal `MAGIC` |
| 8 | 16 | `protocol_uuid` | must equal `PROTOCOL_UUID` |
| 24 | 2 | `major` | must be 1 |
| 26 | 2 | `minor` | 0; unknown minor with same major is rejected |
| 28 | 4 | `total_size` | `>= STRUCT_SIZE`; extra bytes are reserved, must be zero |
| 32 | 4 | `architecture_id` | must be `ARCH_X86_64` |
| 36 | 4 | `boot_mode` | `BOOT_MODE_NORMAL` |
| 40 | 8 | `memory_map_phys` | physical address of memory-range array |
| 48 | 8 | `memory_map_length` | byte length of the array |
| 56 | 8 | `memory_desc_size` | must equal `MEM_DESC_SIZE` |
| 64 | 8 | `framebuffer_phys` | 0 if absent |
| 72 | 4 | `framebuffer_width` | 0 if absent |
| 76 | 4 | `framebuffer_height` | 0 if absent |
| 80 | 4 | `framebuffer_pitch` | 0 if absent |
| 84 | 4 | `framebuffer_format` | `FB_FORMAT_NONE` if absent |
| 88 | 8 | `capsule_phys` | physical address of the capsule |
| 96 | 8 | `capsule_length` | byte length of the capsule |
| 104 | 32 | `capsule_digest` | SHA-256 of capsule bytes |
| 136 | 1 | `capsule_identity_kind` | mirrors capsule header field |
| 137 | 1 | `capsule_oid_alg` | mirrors capsule header field (0 none, 1 SHA-1, 2 SHA-256) |
| 138 | 1 | `capsule_oid_length` | mirrors capsule header field (20/32, or 0 when no OID) |
| 139 | 5 | `reserved` | zero |
| 144 | 32 | `capsule_source_identity` | mirrors capsule header field |
| 176 | 8 | `acpi_rsdp` | physical address of RSDP, 0 if absent |
| 184 | 8 | `smbios` | physical address of SMBIOS table, 0 if absent |
| 192 | 8 | `next` | 0; reserved extension pointer |
| 200 | 24 | `reserved` | zero |

## 5. Memory-range descriptor (24 bytes)

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `phys_start` |
| 8 | 8 | `phys_length` |
| 16 | 4 | `ty` (`1` usable, `2` reserved, `3` ACPI reclaimable, `4` ACPI NVS, `5` MMIO) |
| 20 | 4 | `flags` (`bit0` executable, `bit1` writable; reserved bits zero) |

The loader converts the UEFI memory map to this descriptor set: contiguous
usable ranges are merged; the array is sorted by `phys_start`; entries do not
overlap; `phys_length` is non-zero.

### Platform handoff (ADR-0022)

`framebuffer_pitch` is bytes per scanline. A present framebuffer has one of
the two 32-bit formats above, non-zero base/width/height/pitch, pitch at least
`width * 4`, and checked `pitch * height` bytes backed by GOP. The all-zero
tuple with `FB_FORMAT_NONE` means GOP is absent. A present but malformed,
PixelBitMask or PixelBltOnly GOP mode fails closed; it is never reported absent.

`acpi_rsdp` is the physical selected RSDP, preferring the ACPI 2.0+ UEFI
configuration table and falling back to ACPI 1.0 only when the preferred GUID
is absent. `smbios` is the physical selected SMBIOS entry point, preferring
SMBIOS 3 and falling back to SMBIOS 2 on the same condition. The loader
validates selected anchors, lengths and checksums; a malformed preferred entry
fails closed. Consumers revalidate firmware-owned data before use.

## 6. Capsule identity binding

`capsule_digest`, `capsule_identity_kind`, `capsule_oid_alg`,
`capsule_oid_length` and `capsule_source_identity` are copied from the
verified capsule header (`whole_capsule_digest`, `source_identity_kind`,
`source_oid_alg`, `source_oid_length`, `source_identity_value`). The nucleus
re-verifies the capsule digest against the bytes at `capsule_phys`.

## 7. Stable diagnostic events

Every serial event line begins with one stable identifier matching
`^TOS\.[A-Z0-9_.]+`. Human-facing console text is outside this ABI.

### Success order

An ordinary successful boot emits the following identifiers in this exact
order. `TOS.CAPSULE.OK` occurs exactly twice: first after loader validation,
then after the nucleus independently validates the capsule.

```text
TOS.BOOT.ENTRY
TOS.CAPSULE.OK files=<decimal>
TOS.BOOT.HANDOFF nucleus=0x<hex> stack=0x<hex> bootinfo=0x<hex>
TOS.NUCLEUS.ENTRY
TOS.CAPSULE.OK files=<decimal>
TOS.BOOTTEXT.PATH <canonical-absolute-path>
[TOS.BOOTTEXT.LINE <first-logical-source-line>]
TOS.BOOTTEXT.DIGEST <64-lowercase-hex-digits>
TOS.IDENTITY source_kind=<git|detached> source_digest=<64-lowercase-hex-digits> capsule_digest=<64-lowercase-hex-digits> arch=0.2.1 builder=1
TOS.HALT ok=0x10
```

`TOS.BOOTTEXT.LINE is optional`: it is emitted only when the canonical boot
text has a first logical line. The other success identifiers and the shown
fields are mandatory. A consumer may rely on the listed identifier order,
including both `TOS.CAPSULE.OK` events.

### Failure vocabulary and extension rule

The following identifiers are stable Boot ABI v1 failures:

| Identifier | Required payload / fields | Meaning |
|---|---|---|
| `TOS.BOOT.FAILC` | `capsule_err=<CapsError>` | Loader rejected capsule bytes before handoff. |
| `TOS.BOOT.FAILI` | `<reason-token>` | Loader infrastructure failure before handoff. |
| `TOS.ABI.FAIL` | none | Nucleus rejected BootInfo ABI bytes. |
| `TOS.MEM.FAIL` | none | Nucleus rejected memory-map data. |
| `TOS.CAPSULE.FAIL` | none | Nucleus rejected capsule data after handoff. |
| `TOS.IDENTITY.MISMATCH` | `bootinfo-vs-capsule-header` | Nucleus rejected the mirrored identity. |
| `TOS.PANIC` | `<component>` | Trusted component stopped by panic. |
| `TOS.EXCEPTION` | `vector=<decimal> error=0x<hex> rip=0x<hex> cr2=<none\|0xhex>` | Nucleus caught a CPU exception and terminates with `RESULT_EXCEPTION`. |

`TOS.BOOT.FAILI` is a stable identifier. Existing reason tokens retain their
meaning: `no-boot-services`, `no-loaded-image`, `no-fs`, `no-volume`,
`no-capsule`, `no-nucleus`, `alloc-nucleus`, `alloc-stack`, `memmap-probe`,
`memmap-descsize`, `alloc-map`, `alloc-ranges`, `alloc-bootinfo`,
`memmap-fill`, `memmap-toomany`, `map-overflow`, `unsorted-map`, and `exit-bs`.
An implementation MAY add a reason token in Boot ABI v1, but it MUST NOT change
the meaning of an existing token.

Mandatory fields and raw payloads above are a stable prefix. An implementation
MAY append optional fields in `key=value` form after that prefix; optional
fields must not alter, remove or reinterpret mandatory fields, so parsers that
consume the v1 prefix remain compatible.

`TOS.EXCEPTION` has a fixed field order. `vector` is the exact x86_64 exception
vector; `error` is the hardware-provided error code or normalized zero when the
architecture supplies none; `rip` is the exception-frame instruction pointer;
and `cr2` is the exact CR2 only for vector 14 (#PF), otherwise literal `none`.
The terminal result is `RESULT_EXCEPTION`. A consumer MUST treat an unknown
non-success `TOS.*` failure or result as failure, not as a successful boot.

### Identity record

`TOS.IDENTITY` carries the machine-readable Stage 1 identity record. Its
required fields are, in this order:

```text
source_kind=
source_digest=
capsule_digest=
arch=
builder=
```

`source_kind` is `git` or `detached`; `source_digest` is the exact 32-byte
header identity rendered in lower-case hexadecimal (with zero padding for a
raw SHA-1 OID); `capsule_digest` is the validated whole-capsule SHA-256;
`arch` is the capsule architecture-spec version; and `builder` is the capsule
builder version. Their spelling and semantics are part of Boot ABI v1.

## 8. Validation summary (reject conditions)

1. magic mismatch;
2. protocol UUID mismatch;
3. unsupported major version;
4. `total_size < STRUCT_SIZE` or reserved trailing bytes non-zero;
5. wrong architecture id;
6. memory map region outside addressable bounds, unsorted, overlapping or with
   zero-length entries;
7. `memory_desc_size != MEM_DESC_SIZE`;
8. capsule region outside memory-map bounds or digest mismatch.

The nucleus halts with the corresponding `RESULT_*` code instead of continuing
when validation fails.

<!-- END source/interfaces/boot/BOOT_ABI_V1.md -->

---

<!-- BEGIN source/interfaces/boot/CAPSULE_FORMAT_V1.md -->

<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Boot Capsule Format — Version 1

Status: **Accepted Tier 2 interface contract.**

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs, including
ADR-0016 through ADR-0021 where they decide capsule v1 semantics.

This document defines capsule v1 before any implementation exists. The parser is
total over arbitrary bytes: every rule below has an exact rejection behaviour.

## 1. Role

The capsule is a deterministic, immutable, read-only archive that transports the
first textual system content from the loader into the nucleus. It is **not** the
installed system: it is a transport and recovery seed. Deleting or replacing it
must not claim to be a system update.

## 2. Constants

| Name | Value | Meaning |
|---|---|---|
| `MAGIC` | bytes `54 4F 53 43 41 50 53 55` (`"TOSCAPSU"`) | format magic |
| `FORMAT_UUID` | `2c4f78b3-9d1e-4b0a-9f2c-1a5c8e0d6f71` (16 bytes, RFC order) | format identity |
| `FORMAT_VERSION` | 1 | version of this document |
| `HEADER_SIZE` | 184 | fixed header length |
| `ALIGNMENT` | 8 | structural alignment unit of the header and the fixed entry sizes (see §2.1) |
| `PATH_ENTRY_SIZE` | 16 | fixed path-table entry size |
| `FILE_ENTRY_SIZE` | 64 | fixed file-table entry size |
| `DIGEST_BYTES` | 32 | SHA-256 digest length |
| `ARCH_SPEC_VERSION` | `0x000201` | packed `0.2.1` (`major<<16|minor<<8|patch`) |
| `BUILDER_VERSION` | 1 | capsule builder contract version |

Byte order: all multi-byte integers are **little-endian**.

### 2.2 Resource bounds (ADR-0021)

All maxima are inclusive. `KiB = 1024` bytes and `MiB = 1024 * 1024` bytes.

| Constant | Value |
|---|---:|
| `MAX_CAPSULE_BYTES` | 32 MiB |
| `MAX_FILE_COUNT` | 4096 |
| `MAX_PATH_BYTES` | 1024 bytes per path |
| `MAX_NAME_ARENA_BYTES` | 1 MiB |
| `MAX_LICENCE_NOTICE_BYTES` | 64 KiB |

These limits apply jointly; satisfying one does not weaken any other limit.
The UEFI loader MUST reject an EFI capsule file larger than
`MAX_CAPSULE_BYTES` from its file-size metadata before allocating a pool buffer
or reading the complete file. The parser remains allocation-free and applies
gross limits before payload hashing or a full table walk where structurally
possible. The builder applies the same maxima with checked conversions and
MUST NOT silently truncate a field.

An accepted capsule permits at most two linear hash traversals of capsule or
payload bytes: one for `whole_capsule_digest` and one cumulative traversal for
per-file `content_digest` values. Detached source identity uses those validated
digest values and MUST NOT hash file contents again.

### 2.1 Alignment semantics (ADR-0017)

`ALIGNMENT` constrains the fixed structural sizes, not every offset:

- `HEADER_SIZE`, `PATH_ENTRY_SIZE` and `FILE_ENTRY_SIZE` are multiples of
  `ALIGNMENT`;
- `path_table_offset == HEADER_SIZE`, and is therefore `ALIGNMENT`-aligned;
- `file_table_offset`, `payload_offset` and `content_offset` are **not** required
  to be multiples of `ALIGNMENT`: the name arena and the file contents have
  arbitrary byte lengths, and padding them would change the bytes of every
  capsule.

Consequently an implementation must not assume aligned access anywhere in the
capsule. Every field is decoded byte-wise from the little-endian encoding above;
casting capsule bytes to a target struct is not a conforming implementation
technique.

## 3. Header layout (184 bytes)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 8 | `magic` | must equal `MAGIC` |
| 8 | 16 | `format_uuid` | must equal `FORMAT_UUID` |
| 24 | 2 | `format_version` | must equal `FORMAT_VERSION` |
| 26 | 2 | `header_size` | must equal `HEADER_SIZE` |
| 28 | 2 | `alignment` | must equal `ALIGNMENT` |
| 30 | 2 | `reserved` | must be zero |
| 32 | 8 | `total_length` | total capsule length in bytes; must equal input length |
| 40 | 8 | `path_table_offset` | absolute offset of path table |
| 48 | 4 | `path_table_count` | number of path entries |
| 52 | 4 | `path_entry_size` | must equal `PATH_ENTRY_SIZE` |
| 56 | 8 | `file_table_offset` | absolute offset of file table |
| 64 | 4 | `file_count` | number of file entries |
| 68 | 4 | `file_entry_size` | must equal `FILE_ENTRY_SIZE` |
| 72 | 8 | `payload_offset` | absolute offset of payload region |
| 80 | 8 | `payload_length` | length of payload region |
| 88 | 4 | `arch_spec_version` | must equal `ARCH_SPEC_VERSION` |
| 92 | 4 | `builder_version` | must equal `BUILDER_VERSION` |
| 96 | 1 | `source_identity_kind` | `0` none, `1` git commit, `2` detached source set |
| 97 | 1 | `source_oid_alg` | `0` none, `1` SHA-1, `2` SHA-256 (see §6) |
| 98 | 1 | `source_oid_length` | OID byte length: 20 (SHA-1) or 32 (SHA-256); 0 when no OID |
| 99 | 1 | `reserved` | must be zero |
| 100 | 32 | `source_identity_value` | raw git object id (left-aligned, zero-padded) or detached source-set digest (see §6) |
| 132 | 4 | `reserved` | must be zero |
| 136 | 8 | `licence_notice_offset` | absolute offset of licence-notice text; 0 if absent |
| 144 | 8 | `licence_notice_length` | length of licence-notice text; 0 if absent |
| 152 | 32 | `whole_capsule_digest` | SHA-256 over capsule with this field zeroed |
| 184 | — | — | end of header |

## 4. Tables

The capsule layout is strictly sequential:

```text
[header] [path table] [name arena] [file table] [payload] [licence notice]
```

The layout admits **no undescribed bytes**: every byte of the capsule belongs to
exactly one of the six regions above (ADR-0017). In particular
`path_table_offset == HEADER_SIZE` — the path table begins immediately after the
header, with no gap.

The name arena begins immediately after the path table and ends exactly at
`file_table_offset`. The file table ends exactly at `payload_offset`. The
licence notice, when present, is the exact tail of the capsule. Hence:

- `path_table_offset == HEADER_SIZE`;
- `file_table_offset == path_table_offset + path_table_count * PATH_ENTRY_SIZE + name_arena_length`;
- `payload_offset == file_table_offset + file_count * FILE_ENTRY_SIZE`;
- `payload_offset + payload_length + licence_notice_length == total_length`;
- when the licence notice is absent, both `licence_notice_offset` and
  `licence_notice_length` are zero;
- when present, `licence_notice_offset == payload_offset + payload_length` and
  `licence_notice_offset + licence_notice_length == total_length`.

### 4.1 Path entry (16 bytes)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 4 | `name_offset` | offset of UTF-8 name relative to name arena start |
| 4 | 4 | `name_length` | byte length of name; non-zero |
| 8 | 4 | `file_index` | index into the file table; must be `< file_count` |
| 12 | 4 | `flags` | bit 0: boot-canonical file; **only bit 0 is defined for path entries** — all other bits must be zero |

Path names must be **canonical absolute paths**:

- start with `/`;
- valid UTF-8; no NUL bytes; no control characters;
- no `.` or `..` components; no empty components (`//`), no trailing `/`;
- lexically sorted in ascending byte order over the whole table;
- distinct (no duplicate names).

**Packed name arena (ADR-0017).** The names tile the arena exactly, in path-table
order:

- `path_entry[0].name_offset == 0`;
- `path_entry[i].name_offset == path_entry[i-1].name_offset + path_entry[i-1].name_length`;
- the end of the last name equals `file_table_offset`.

No byte of the arena is outside a name; names neither overlap nor leave gaps.

**Canonical index mapping (ADR-0017).** The path table is a **bijection onto the
file table**, realised canonically:

- `path_table_count == file_count`;
- `path_entry[i].file_index == i`.

The file table and the payload therefore follow the same order as the
name-sorted path table. A non-canonical permutation of `file_index` — including
one that happens to be a valid bijection — is rejected, so that a given file set
has exactly one valid capsule encoding and the check costs a single O(n) pass.

### 4.2 File entry (64 bytes)

| Offset | Size | Field | Rules |
|---|---|---|---|
| 0 | 8 | `content_offset` | offset of file content **relative to `payload_offset`** |
| 8 | 8 | `content_length` | byte length of content |
| 16 | 32 | `content_digest` | SHA-256 of content bytes |
| 48 | 4 | `file_flags` | bit 0: boot-canonical; bit 1: licence notice; **only bits 0-1 are defined** — reserved bits must be zero |
| 52 | 12 | `reserved` | must be zero |

Content constraints:

- the payload is the exact byte-to-byte concatenation of file contents in
  file-table order: consecutive files are adjacent, so the union of
  `[content_offset, content_offset + content_length)` over all files equals
  `[0, payload_length)`, and `content_offset` need not be aligned;
- content regions are pairwise disjoint (no overlap) — enforced by requiring
  that the cumulate of guarded payload covers `[0, payload_length)` exactly
  and every byte belongs to exactly one file;
- `content_digest` equals SHA-256 of the exact content bytes (no padding).

## 5. Canonical ordering

- Path table: sorted by name bytes (ascending). Unsorted table is rejected.
- File table: sorted by `content_offset` (ascending). Unsorted table is rejected.
- `file_flags` boot-canonical bit (bit 0) is set for exactly one file, the
  system boot text at `/system/boot/init.tos`.
- Boot-canonical **consistency**: the path entry carrying bit 0 must reference
  the file entry that carries bit 0, and vice versa. A canonical path pointing
  at a non-canonical file, a non-canonical path pointing at the canonical
  file, or a canonical flag on any other file is rejected.

## 6. Identity fields

- `source_identity_kind = 2` (detached source set): `source_oid_alg = 0`,
  `source_oid_length = 0`, and `source_identity_value` is exactly the
  ADR-0018 value:

  ```text
  SHA-256(
      b"TOS.DSI.v1\0" ||
      for each canonical path/file-table entry i:
          u32_le(path_length_i) || path_bytes_i || content_digest_i
  )
  ```

  Entries use the shared canonical path/file-table order. `path_bytes` are the
  exact validated canonical UTF-8 path bytes; `content_digest` is the exact
  validated 32-byte SHA-256 file digest. The fixed domain separator bytes are
  `54 4f 53 2e 44 53 49 2e 76 31 00`. `file_count` is not additionally encoded:
  the length-delimited path and fixed-size digest sequence is unambiguous.
  Capsule v1 still rejects zero files, although the mathematical zero-entry
  value is `SHA-256(b"TOS.DSI.v1\0")`. Builder and parser compute this value
  independently; a caller-selected detached value or a mismatch is rejected.
- `source_identity_kind = 1` (git commit): `source_oid_alg` names the OID
  algorithm (`1` = SHA-1, `2` = SHA-256) and `source_oid_length` its byte
  length (20 or 32). `source_identity_value` holds the **raw commit object
  id**, left-aligned and zero-padded to 32 bytes. The id is stored directly
  (not hashed) so a capsule can be resolved back to its commit with
  `git show <oid>`; see ADR-0016. A SHA-1 identity therefore has a 20-byte raw
  id followed by a 12-byte all-zero unused tail; any non-zero tail byte is
  rejected.
- The pair `(source_oid_alg, source_oid_length)` must be consistent with the
  kind: git kind requires `(1, 20)` or `(2, 32)`; detached kind requires
  `(0, 0)`. Anything else is rejected by the parser.
- Kind `0` is forbidden for any capsule produced by an official builder; it is
  rejected by the parser for boot-canonical capsules. Development fixtures may
  use kind 2 with an explicit `detached-source-set` label in the manifest.

## 7. Licence notices

`licence_notice_offset/length` point at a UTF-8 text block naming the SPDX
identifiers of all materials inside the capsule. If the capsule carries no
non-canonical material, the block names `GPL-3.0-or-later` (the canonical boot
text licence).

There is **no** `licence_notice_digest` field in the v1 header: the header
layout in §3 has none, and the notice block is covered by
`whole_capsule_digest` (§8) like every other byte of the capsule. A dedicated
digest field, if it is ever needed, belongs to a future format version and
requires an ADR. (An earlier revision of this section referred to such a field
and then denied its existence in the same sentence; the layout in §3 has always
been the authority.)

**Builder obligation vs parser obligation.** Producing a notice block that
actually names every SPDX identifier in the capsule is a builder obligation; a
v1 parser cannot verify it, because SPDX identifiers are not derivable from the
capsule bytes. A v1 parser validates only what is checkable: the block is in
bounds, is the exact tail of the capsule, has consistent offset/length fields
and is valid UTF-8 (§9 rule 21).

## 8. Whole-capsule digest

`whole_capsule_digest = SHA-256(capsule_bytes with bytes [152, 184) zeroed)`.
The digest is verified over the exact bytes passed to the parser.

## 9. Validation summary (reject conditions)

### 9.1 Resource-limit precedence

The parser returns stable structured errors in this deterministic order for
the five limits: `CapsuleTooLarge`, `FileCountTooLarge`, `PathTooLong`,
`NameArenaTooLarge` and `LicenceNoticeTooLarge`.

1. An input shorter than `HEADER_SIZE` is rejected before header decoding.
2. A physical input longer than `MAX_CAPSULE_BYTES` is rejected before header
   decoding, hashing or table traversal (`CapsuleTooLarge`).
3. After magic, UUID, format version, header size and alignment are checked,
   the declared total length, path/file counts and licence-notice length are
   checked in that order.
4. After checked table geometry establishes the name-arena bounds, its length
   is checked before path-table iteration (`NameArenaTooLarge`).
5. Each `name_length` is checked against `MAX_PATH_BYTES` before UTF-8 or
   canonical-path processing (`PathTooLong`).

### 9.2 Other reject conditions

1. magic mismatch;
2. format UUID mismatch;
3. format version unsupported;
4. header size, alignment or entry-size fields inconsistent;
5. `total_length` mismatch with actual input length;
6. any integer overflow in offset/length arithmetic (checked);
7. table offsets/counts imply regions outside `[0, total_length)`;
8. name arena does not end exactly at `file_table_offset` (§4);
9. payload region exceeds capsule bounds;
10. invalid UTF-8, NUL or control bytes in any path name;
11. path not canonical (see §4.1);
12. duplicate path names;
13. path table not sorted ascending;
14. `file_index` out of range, or the path table not being a bijection
    (duplicate references to one file, or an orphan file). Under §4.1 this is
    decided canonically: `path_table_count != file_count`, or any
    `path_entry[i].file_index != i`;
15. file table not sorted by content offset;
16. file content out of payload bounds;
17. overlapping or non-covering payload content;
18. per-file digest mismatch;
19. whole-capsule digest mismatch;
20. source identity kind unsupported, or kind 0 with boot-canonical flag;
21. licence notice block out of bounds, not the exact capsule tail, absent
    fields inconsistent (offset non-zero with zero length), or not valid UTF-8;
22. reserved fields non-zero (header, path-entry, file-entry 12-byte block);
23. boot-canonical flag inconsistency between path entry and file entry;
24. `path_table_offset != HEADER_SIZE`, i.e. a gap between the header and the
    path table (§4, ADR-0017);
25. the name arena is not packed: `path_entry[0].name_offset != 0`, a name that
    does not start where the previous one ends, or a last name that does not end
    exactly at `file_table_offset` (§4.1, ADR-0017);
26. `path_entry[i].file_index != i` — a non-canonical index mapping (§4.1,
    ADR-0017).
27. detached source identity differs from the ADR-0018 canonical
    path/digest encoding in §6.

A parser must return a structured error naming the rule violated; it must never
panic on malformed input.

## 10. Golden vectors

`tests/vectors/capsule-v1/` contains committed binary fixtures, regenerated by
`tests/vectors/gen/gen.sh`:

- `valid-001.bin` — a valid capsule built by the reference builder from the real
  `system/boot/init.tos` plus `system/version`, with the real `NOTICES.txt` as
  the licence tail;
- `invalid-badmagic.bin`, `invalid-truncated.bin`, `invalid-kind-none.bin`,
  `invalid-missing-boot.bin`, `invalid-bootcanon-mismatch.bin`,
  `invalid-licence-tail.bin`, `invalid-traversal.bin`, `invalid-dup.bin`,
  `invalid-dup-file-index.bin`, `invalid-unreferenced-file.bin`,
  `invalid-path-flag.bin`, `invalid-file-reserved.bin`,
  `invalid-sha1-oid-padding.bin` — each targeting one rule from §9.

A fixture targets one rule, which is the rule its expected error names. Fixtures
produced by patching a valid capsule in place also break the whole-capsule
digest (§8); the parser reports the targeted rule because it is checked before
the digest. A fixture must therefore be read as "rejected, and rejected for this
reason", not as "violates exactly one rule".

Every fixture records its expected parse outcome (accept, or reject with the
error name) in `vectors.tsv`, which is the input of the vector-driven
integration test. ADR-0019 requires every tracked binary fixture to have a
machine-verifiable `provenance.json` record; its
`mixed-material-generated` container status is not an SPDX expression.

<!-- END source/interfaces/boot/CAPSULE_FORMAT_V1.md -->

---

<!-- BEGIN source/interfaces/boot/CAPSULE_PROVENANCE_V1.md -->

<!-- SPDX-License-Identifier: Apache-2.0 -->

# TOS Capsule Provenance Sidecar — Version 1

Status: **Accepted Tier 2 interface contract.**

Authority is assigned only by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; this
contract is subordinate to Tier 0 invariants and accepted Tier 1 ADRs,
including ADR-0010 and ADR-0024.

## 1. Role

`tos-capsule-provenance-v1` is deterministic release provenance for one
capsule.  It is produced by `tos-capsule-tool --meta` and is independently
validated by `scripts/check-capsule-provenance.py`.  It is a sidecar only: a
UEFI loader and nucleus MUST NOT consume it, and it does not alter capsule v1
or Boot ABI v1 bytes.

The capsule's `whole_capsule_digest`/`artifact.sha256` is the binding artifact
identity.  A sidecar with a digest mismatch is invalid provenance, never an
alternative capsule authority.

## 2. Canonical JSON document

The UTF-8 JSON document has these required members.  Producers MUST emit the
shown member order and arrays in ascending `capsule_path`/identifier byte order;
they MUST NOT emit timestamps, absolute host paths or environment-specific
fields.  Consumers validate field types, values and the relationships below.

```json
{
  "format": "tos-capsule-provenance-v1",
  "schema_version": 1,
  "artifact": {
    "sha256": "<64 lowercase hex>",
    "capsule_format": {
      "uuid": "2c4f78b3-9d1e-4b0a-9f2c-1a5c8e0d6f71",
      "version": 1
    },
    "architecture_spec_version": "0.2.1",
    "builder": { "implementation": "tos-capsule-tool", "version": 1 },
    "target": {
      "architecture": "x86_64",
      "loader_abi": "x86_64-unknown-uefi",
      "nucleus_boot_abi": {
        "minimum": { "major": 1, "minor": 0 },
        "maximum": { "major": 1, "minor": 0 }
      }
    }
  },
  "source_identity": {
    "kind": "git-commit",
    "source_commit": "<full Git OID>",
    "oid_algorithm": "sha1|sha256",
    "oid_length": 20,
    "raw_oid": "<lowercase hex OID>"
  },
  "materials": [
    {
      "role": "canonical-source",
      "capsule_path": "/system/boot/init.tos",
      "repository_path": "source/system/boot/init.tos",
      "content_sha256": "<64 lowercase hex>",
      "spdx_expression": "GPL-3.0-or-later"
    }
  ],
  "build": {
    "identity_mode": "git-commit",
    "licence_notice_included": true,
    "reproducibility_grade": "R0"
  },
  "licence_notice": {
    "sha256": "<64 lowercase hex>",
    "spdx_identifiers": ["GPL-3.0-or-later"]
  }
}
```

The formatting example is descriptive; stable field names, values and ordering
rules are normative.  Lower-case hexadecimal SHA-256 values are exactly 64
characters.  A Git OID is the full lower-case OID naming a local commit and is
the same identity represented in the capsule header.  `source_commit` is
explicit for a Git identity.

For `kind = "detached-source-set"`, `source_commit`, `oid_algorithm`,
`oid_length` and `raw_oid` are replaced by
`"digest_algorithm":"sha256"` and `"digest":"<64 lowercase hex>"`.
That digest is the accepted ADR-0018 identity; it is a publication identity,
not a fabricated Git commit.

## 3. Required relationships

- `artifact.sha256` equals SHA-256 of the exact capsule bytes.
- `capsule_format`, `architecture_spec_version` and `builder.version` equal
  the verified capsule header.  The target is exactly the Stage 1 x86_64 UEFI
  loader target and Boot ABI v1.0 range shown above.
- `materials` has one row for every capsule file, in canonical file-table
  order.  Its path and digest equal the parsed capsule path/content digest.
  A Git-mode `repository_path` names a blob in `source_commit` with the same
  bytes; detached mode omits that member.
- Each material is `canonical-source` and declares the exact SPDX expression
  found in its source bytes.  Its expression occurs in
  `licence_notice.spdx_identifiers`.
- `licence_notice.sha256` equals the embedded licence-notice tail.  The sorted,
  duplicate-free identifier list is extracted from its exact
  `SPDX-License-Identifier:` lines.  A Stage 1 provenance sidecar therefore
  requires a retained notice block.
- `build.identity_mode` and `licence_notice_included` equal the represented
  capsule/header facts.  `reproducibility_grade` is exactly `R0`; no higher
  grade is implied by deterministic local output.

## 4. Evidence and evolution

The checker MUST reject a missing required member, non-canonical ordering,
digest/header/source/notice mismatch, invented Git commit, or licence-set
mismatch.  QEMU's normal build path MUST run this checker before booting the
capsule.  A schema extension requires a new version or an accepted ADR; a
consumer MUST NOT silently reinterpret v1 fields.

<!-- END source/interfaces/boot/CAPSULE_PROVENANCE_V1.md -->

---

<!-- BEGIN docs/21_ARCHITECTURE_PRESERVATION_POLICY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture preservation policy

## Purpose

TOS is unusually vulnerable to “reasonable” engineering decisions that solve local problems while erasing the reason the project exists. This policy makes architectural drift visible and reviewable.

## Architectural identity

TOS is defined by the conjunction of these properties:

1. human-readable source is the canonical installed form of non-nucleus executable components;
2. the active system has a commit identity and a visible source-to-runtime chain;
3. derived executable artifacts are disposable and verifiably tied to source and runtime versions;
4. the owner can inspect, branch, modify, validate and boot the system;
5. drivers and services are isolated by explicit capabilities;
6. activation and recovery are transactional and history-aware;
7. new languages extend the system through a stable frontend contract rather than expanding the nucleus without bound.

A project that keeps only some of these properties may be interesting, but it is not automatically TOS.

## Narrow scope versus architectural shortcut

The following are acceptable scope constraints:

- QEMU only;
- one CPU active while SMP-compatible interfaces are specified;
- VirtIO block before physical storage controllers;
- a small TOS Core bootstrap profile before the full language;
- serial shell before a graphical environment;
- one Git object format and one hash family initially.

The following are not acceptable milestone shortcuts:

- canonical binary modules with source kept “for later”;
- a Linux or BSD kernel hidden under a textual shell and presented as TOS;
- drivers moved into the nucleus because IPC is unfinished;
- Git used only for the development repository while runtime state has no commit identity;
- a general-purpose embedded runtime adopted before its trust, capability and source-identity semantics are accepted;
- a recovery flow that requires an undocumented host workstation;
- locked boot that allows source inspection but denies owner modification.

## Change classes

### Level 0 — editorial

No semantic effect. Normal review.

### Level 1 — implementation

Implements an existing contract without changing observable semantics. Requires tests.

### Level 2 — contract extension

Adds versioned behavior while preserving invariants. Requires a design note and generally an ADR.

### Level 3 — architectural

Moves trust boundaries, changes persistent formats, introduces a runtime dependency, changes source identity or modifies owner control. Requires an ADR and Project Architect approval.

### Level 4 — identity amendment

Changes or removes an invariant. Requires a dedicated identity-impact analysis, explicit approval and a major architecture version. It may result in a successor project rather than TOS.

## Architecture impact statement

Every Level 2 or higher change must answer:

- Which invariants are affected?
- What becomes canonical after the change?
- What enters or leaves the trusted base?
- Can the active runtime still identify its exact source?
- Can all derived artifacts be discarded and regenerated?
- Can the owner still recover and boot a previous commit?
- Does the change create a hidden host dependency?
- Does it alter licensing or patent exposure?
- How is the behavior tested?

## Substitution rule

A dependency or existing technology is not accepted merely because it is mature. It is evaluated in three roles:

- **runtime dependency** — becomes part of TOS operation and trust;
- **build dependency** — creates artifacts but is absent at runtime;
- **reference oracle** — used to compare behavior or generate test vectors.

The least invasive role that satisfies the requirement is preferred. libgit2, a Lua VM, Wasm engines, filesystem libraries and Linux driver code must not migrate from “oracle” or “research reference” to the trusted runtime without an ADR.

## Architecture debt

TOS does not normalize intentional architecture debt. Temporary diagnostics may exist on experimental branches, but a stage closes only when the real contract is implemented. Unfinished breadth is acceptable; falsified completion is not.

## Stage identity enforcement

Every stage is reviewed against `docs/37_STAGE_IDENTITY_GATES.md`. A conventional feature demonstration does not close a stage without the required TOS-specific evidence. The identity report is a release artifact.

## Documentation authority

Architecture decisions are interpreted through `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`. Generated summaries cannot amend architecture. A documentation conflict blocks implementation at the affected boundary until resolved.

## Enforcement

Architecture conformance is enforced through:

- ADR review;
- invariant references in pull requests;
- automated repository checks;
- source-to-runtime conformance tests;
- dependency and licence inventory;
- engineering and TOS identity stage gates;
- threat-model and performance-contract review;
- generated-document drift checks;
- refusal to merge identity-erasing shortcuts.

<!-- END docs/21_ARCHITECTURE_PRESERVATION_POLICY.md -->

---

<!-- BEGIN docs/37_STAGE_IDENTITY_GATES.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage identity gates

## Purpose

Early TOS development necessarily contains familiar OS work: UEFI, memory, IPC, PCI and VirtIO. Conventional feature success does not prove that the project is still TOS.

Every stage therefore has an identity question, required evidence and explicit failure conditions. The evidence is archived under `legal/release-manifests/` or a later versioned conformance store.

## Stage 0 — Architecture identity

Question: is TOS distinguishable in normative contracts before implementation?

Evidence:

- active invariants and ADR set;
- threat model;
- documentation authority map;
- architecture-preservation policy;
- licensing/provenance baseline;
- no unresolved contradiction at an implementation boundary.

Failure conditions:

- generated summary treated as independent authority;
- core concepts described only by slogans;
- undefined trust or ownership boundary.

## Stage 1 — Source-bearing boot identity

Question: does the first boot artifact prove that it carries canonical source from an identified repository state rather than anonymous embedded text?

Evidence:

- real Git repository or explicit detached-source-set identity;
- capsule manifest binds source commit/tree, paths, hashes, builder, ABI and output digest;
- nucleus reports structured source identity for `/system/boot/init.tos`;
- corruption and identity-mismatch tests fail closed;
- generated documentation is in sync at the source commit.

Failure conditions:

- all-zero or invented official commit;
- hard-coded text with no source object provenance;
- capsule treated as canonical installed system.

## Stage 1.5 — Language-foundation identity

Question: does the selected language/runtime foundation preserve canonical text, capability semantics, deterministic lowering, bounded bootstrap and source observability?

Evidence:

- completed evaluation matrix;
- comparative prototypes/test vectors;
- accepted selection ADR;
- rejected alternatives and reasons;
- explicit trusted-base and licence analysis.

Failure conditions:

- choosing a language solely because an interpreter is available;
- making Wasm/bytecode the canonical source;
- undocumented host/C ABI becoming the true system contract.

## Stage 2 — Executed-source identity

Question: is actual language semantics executing from canonical text with a verifiable mapping to runtime behavior?

Evidence:

- normative grammar and semantics;
- source -> AST -> typed IR -> execution trace;
- independent verifier;
- cache deletion/regeneration test;
- source mutation invalidates old cache;
- runtime introspection reports source and engine identity.

Failure conditions:

- command dispatcher presented as a language;
- executable derivative accepted without source binding;
- diagnostics cannot identify source spans.

## Stage 3 — Authority-bearing textual service identity

Question: do textual processes exercise real capability/IPC contracts rather than running as decorative scripts around privileged binary services?

Evidence:

- process source identity bound to commit/blob;
- explicit granted capability set;
- denial and confused-deputy tests;
- privileged policy remains outside ordinary module code;
- service restart preserves identity/audit records.

Failure conditions:

- ambient root-equivalent authority;
- ordinary service logic moved into nucleus for convenience;
- textual manifest grants itself authority.

## Stage 4 — Textual driver identity

Question: does a canonical textual user-space driver actually move persistent data through final-style MMIO/interrupt/DMA/IPC boundaries?

Evidence:

- driver loaded from identified commit/blob or Stage-compatible source set;
- device capabilities only;
- DMA and interrupt threat tests;
- performance contract report;
- crash/restart and device-reset behavior;
- no binary shadow driver performs the real I/O.

Failure conditions:

- text merely configures an in-kernel driver;
- hidden host I/O path;
- performance is unmeasured or achieved by bypassing isolation.

## Stage 5 — Commit-as-system identity

Question: is the running `/system` genuinely the selected commit tree, with transactional history operations as runtime behavior?

Evidence:

- declared Git compatibility profile at least G2;
- immutable `/system` mounted by commit;
- writable overlay is distinct;
- candidate/current/last-known-good/recovery transitions survive fault injection;
- failed candidate returns to previous commit;
- process source identities agree with active commit;
- no eager binary package installation is the hidden authority.

Failure conditions:

- Git only tracks development sources;
- commit is metadata around a separately installed binary tree;
- rollback copies files ad hoc without protected history semantics.

## Stage 6 — Self-modifying open-system identity

Question: can TOS inspect, modify, validate, commit and activate its own canonical textual system without an undocumented host workstation?

Evidence:

- in-system edit and diff;
- validation and tests;
- commit creation;
- candidate activation and rollback;
- documentation/source browser tied to active commit;
- recovery shell can inspect and select commits.

Failure conditions:

- host compiler required for ordinary textual services;
- edit affects a shadow copy but not installed identity;
- activation bypasses repository transaction.

## Stage 7 — Remote recovery identity

Question: can a recovery environment reconstruct the same system identity from a remote without trusting the failed active system?

Evidence:

- declared G4 transport profile;
- authenticated remote and malicious-server tests;
- separate secret restoration;
- selected commit and artifact provenance verified;
- owner chooses trust policy.

## Stage 8 — Extensible-language identity

Question: can a second language become a first-class textual source without nucleus modification or loss of provenance?

Evidence:

- frontend ABI conformance;
- deterministic lowering and source maps;
- capability import enforcement;
- cache identity and runtime introspection;
- honest compatibility profile.

## Stage 9 — Platform expansion identity

Question: do UI and physical-device additions remain textual, capability-confined and commit-addressed rather than forcing a second hidden OS layer?

Evidence is subsystem-specific and must include source identity, authority, recovery and performance reports.

## Stage 10 — Toolchain identity

Question: can necessary binary nucleus artifacts be reproduced and verified without becoming the canonical installed truth?

Evidence:

- source commit and complete build provenance;
- independent reproducibility;
- recovery builder path;
- owner-installable artifact authorization;
- canonical-source rule unchanged.

## Gate report format

Each stage report contains:

```text
stage
source_commit
architecture_version
identity_question
required_evidence[]
produced_artifacts[]
tests[]
performance_report
threat_model_coverage
compatibility_profiles
known_failures[]
architect_approval
```

A stage may remain open indefinitely. It must not be declared complete with missing identity evidence.

<!-- END docs/37_STAGE_IDENTITY_GATES.md -->

---

<!-- BEGIN docs/03_ARCHITECTURE_OVERVIEW.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture overview

## Layer model

```mermaid
flowchart TD
    FW[UEFI firmware] --> BL[Boot loader]
    BL --> CAP[Boot capsule]
    BL --> N[Nucleus]
    N --> REPO[Repository substrate]
    N --> RT[Text execution runtime]
    N --> IPC[Processes, IPC, capabilities]
    RT --> INIT[/system/boot/init.tos]
    INIT --> SVC[Textual system services]
    SVC --> DRV[Textual user-space drivers]
    SVC --> LANG[Language frontends]
    SVC --> UI[Shell and user interface]
    REPO --> SYS[Immutable /system commit]
    REPO --> OVL[Writable system overlay]
```

## Binary nucleus

The nucleus is a small `no_std` binary responsible for mechanism, not broad policy.

It owns:

- CPU mode and exception setup;
- physical and virtual memory management;
- scheduler primitives;
- process address spaces;
- capability handles;
- IPC transport;
- interrupt routing primitives;
- timekeeping primitives;
- boot capsule access;
- minimal repository object verification needed for boot;
- source-runtime bootstrap;
- structured panic and diagnostic output.

It does not own:

- a general network stack;
- a shell;
- package policy;
- graphical desktop policy;
- ordinary filesystems;
- full Git remote operations;
- most device drivers;
- language-specific standard libraries.

## Boot capsule

The loader places a deterministic, immutable capsule in memory before transferring control. It contains everything required before persistent storage becomes accessible:

- `/system/boot/init.tos`;
- the initial TOS Core runtime modules not compiled into the nucleus;
- boot-critical text drivers, such as VirtIO block and console support;
- schemas and manifests;
- a minimal recovery command set;
- expected object IDs and signatures.

The capsule is not the installed system. It is a transport and recovery seed. Once storage and the repository are available, the system resolves the selected commit and mounts its `/system` tree.

Every capsule carries provenance naming the canonical source commit, included source hashes, builder identity, target ABI, format version and whole-capsule digest. Its canonical inputs live in the repository; the capsule is reproducible and never becomes an independent hidden system.

## Text runtime

The runtime consumes source text, validates it, lowers it to a typed internal representation, and executes it. The initial language is TOS Core. Additional language frontends are modules that produce the same internal representation under a versioned frontend contract.

## Repository substrate

TOS uses Git semantics for durable system history:

- immutable content objects;
- trees;
- commits;
- references;
- branches;
- merge ancestry;
- remotes through a userspace service.

The boot path needs only read and verification operations. Full clone, fetch, pack, merge, signing, and transport logic belongs in textual privileged services.

## Filesystem view

The visible filesystem is assembled from distinct stores:

```text
/system   immutable tree of selected commit
/work     writable overlay for proposed system-source changes
/config   machine and deployment configuration, versioned separately or layered
/state    mutable durable service state
/home     user data
/secrets  encrypted capability-protected secrets
/cache    disposable generated data
/run      ephemeral runtime objects
/dev      capability-mediated device namespace
```

The names are normative at the conceptual level; exact mount implementation may evolve through ADRs.

## Process model

Every service and driver is a process with:

- a source identity;
- a module identity;
- a system commit identity;
- a capability set;
- a declared IPC schema set;
- structured health state;
- restart policy;
- logs tied to source locations.

## Trust zones

1. **Firmware and loader** — external or minimally controlled trust.
2. **Nucleus** — highest TOS trust.
3. **Boot/repository authority services** — privileged but isolated.
4. **Drivers** — device-specific capabilities only.
5. **System services** — least authority required.
6. **Applications** — user-granted capabilities.
7. **Experimental branches** — explicitly marked trust state.

## Compatibility strategy

TOS does not initially emulate POSIX or Linux kernel APIs. Compatibility may later be provided as ordinary services or language runtimes. Native TOS contracts remain capability-oriented and repository-aware.

## Source-to-runtime identity plane

Alongside memory, IPC and repository layers, TOS maintains an identity plane. It answers:

- which commit supplied a module;
- which source bytes were validated;
- which frontend and IR schema were used;
- which derived cache is executing;
- which process instance and capability grants resulted;
- which health and activation transaction introduced it.

This plane is not optional debugging metadata. It is part of the operating-system model and is tested for conformance.

## Architecture preservation boundary

The project may substitute implementations while preserving contracts. It may not substitute away canonical text, commit-addressed system identity, owner-controlled boot, capability isolation or recoverable activation. External components are assigned explicit roles as reference, oracle, host tool, isolated service or trusted dependency.

See `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`, `docs/37_STAGE_IDENTITY_GATES.md` and ADR-0011.

Security assumptions are centralized in `docs/34_THREAT_MODEL.md`; performance-sensitive paths are governed by `docs/35_PERFORMANCE_CONTRACTS.md`; Git claims use `docs/36_GIT_COMPATIBILITY_PROFILES.md`.

<!-- END docs/03_ARCHITECTURE_OVERVIEW.md -->

---

<!-- BEGIN docs/04_BOOT_AND_RECOVERY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Boot and recovery model

## Goals

The boot path must be deterministic, inspectable, transactional, and recoverable. It must be possible to boot without trusting the current writable state of the machine.

## Boot stages

### Stage 0 — Firmware

UEFI locates and launches the TOS loader from a small boot partition or removable recovery medium.

### Stage 1 — Loader

The loader:

1. reads the boot-control record;
2. selects `candidate`, `current`, `last-known-good`, or an operator-selected commit;
3. loads the nucleus image;
4. loads the immutable boot capsule;
5. gathers memory-map, framebuffer, ACPI, and platform data;
6. constructs the versioned boot protocol block;
7. exits firmware boot services;
8. transfers control once.

The loader performs no policy-heavy update logic.

### Stage 2 — Nucleus bootstrap

The nucleus validates:

- boot protocol magic and version;
- memory ranges and alignment;
- capsule structure and digest;
- selected commit identity format;
- recovery policy flags.

It initializes memory isolation, exceptions, logging, scheduling, IPC, and the TOS Core bootstrap runtime.

### Stage 3 — Text init

The nucleus executes `/system/boot/init.tos` from the capsule. This component launches boot-critical driver services, discovers repository storage, verifies the selected commit, and transitions to the repository-backed system tree.

### Stage 4 — Repository system

The repository-backed `/system/boot/init.tos` takes over. It may differ from the capsule copy only through a defined handoff protocol. It launches normal services, health checks, login, shell, and UI.

## Boot control record

The control record is stored redundantly and updated atomically. It contains at least:

```text
format_version
sequence_number
current_commit
candidate_commit
last_known_good_commit
recovery_commit
candidate_attempts
candidate_max_attempts
boot_mode
required_signature_policy
record_digest
```

Two or more copies are written with sequence numbers. The loader selects the highest valid sequence. Partial writes must not destroy the previous valid record.

## Candidate activation

A system update never overwrites `current_commit` directly.

1. New commit is fetched and verified.
2. Required caches and any new nucleus boot artifact are prepared in an inactive slot.
3. `candidate_commit` is set with an attempt budget.
4. Machine boots candidate.
5. System runs declared health checks.
6. On success, candidate is promoted to `current` and `last-known-good`.
7. On repeated failure, loader clears candidate and returns to last-known-good.

## Health declaration

A commit may declare boot health requirements in `/system/boot/health.tos` or a versioned manifest, including:

- repository mounted;
- essential driver services healthy;
- writable state available or intentionally read-only;
- console available;
- scheduler watchdog alive;
- optional network target reachable;
- no fatal schema migrations pending.

Health checks have time limits and stable result codes.

## Recovery environment

Recovery consists of:

- a trusted nucleus image;
- a minimal immutable capsule;
- repository object inspection;
- local disk discovery;
- network configuration sufficient for clone/fetch;
- commit listing and verification;
- boot-control repair;
- state and secret volume discovery;
- an operator shell.

Recovery must work even when the active `/system`, caches, or normal services are corrupt.

## Restoration workflow

A blank machine can be restored by:

1. booting recovery media;
2. partitioning or selecting storage;
3. cloning the system repository;
4. selecting a commit and machine configuration branch;
5. restoring encrypted state and secrets separately;
6. generating derived caches and boot artifact slots;
7. committing the boot-control record;
8. rebooting.

## Nucleus changes

The nucleus is the binary exception. Its source belongs in the system repository, but the executable image is derived.

When a selected commit changes nucleus source:

- an approved builder creates a deterministic or reproducibly verifiable image;
- the image is associated with source commit, toolchain identity, and build manifest;
- it is installed into an inactive boot slot;
- the loader selects the slot only for the candidate commit;
- rollback preserves the previous slot.

The system must distinguish "commit containing nucleus source" from "verified boot artifact derived from that commit."

## Capsule provenance and licence inventory

The boot capsule header or signed manifest contains:

```text
capsule_format_version
source_commit
architecture_spec_version
nucleus_abi_range
builder_identity
material_digests
included_path_hashes
licence_notice_digest
whole_capsule_digest
```

The first implementation may omit cryptographic signatures if the stage has not yet introduced key policy, but it may not omit deterministic identity fields. Recovery can display the source relationship of every included component.

## Owner-authorized boot

Official developer and research profiles provide a documented path to boot an owner-modified commit. Secure defaults may require explicit physical or recovery action. Candidate state, warnings and signatures are recorded, but no vendor-only secret is a permanent prerequisite for owner control.

<!-- END docs/04_BOOT_AND_RECOVERY.md -->

---

<!-- BEGIN docs/05_TOS_CORE_LANGUAGE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core language role and requirements

## Current status

“TOS Core” names the required native textual language role of the system. The final language foundation is **not yet selected**.

The syntax in this document is illustrative. No parser, grammar or runtime becomes normative until Stage 1.5 completes and a selection ADR is accepted under ADR-0015.

This distinction is deliberate: TOS requires language properties, not a proprietary syntax for its own sake.

## Role

The selected foundation must be small enough to bootstrap and audit, yet complete enough to implement services, drivers, language frontends and eventually much of its own runtime.

Its priorities are:

- deterministic parsing;
- explicit types;
- structured errors;
- capability-safe system interaction;
- predictable and enforceable resource use;
- source-level observability;
- incremental loading;
- compatibility with a compact reference interpreter and later optimizers;
- independence from an undocumented host ABI.

Canonical native source files are expected to use UTF-8 and the `.tos` extension unless the selection ADR changes the surface-language decision.

## Language profiles

The selected language has two profiles sharing compatible syntax and semantics:

- **Bootstrap profile** — bounded allocation, no ambient dynamic module loading, minimal standard library, used during early boot and recovery.
- **Full profile** — structured asynchronous tasks, richer collections, dynamic service discovery, frontend APIs and user applications.

The bootstrap profile is a strict supported subset, not a temporary fake language.

## Illustrative syntax

```tos
module drivers.virtio.block

import system.bus.pci
import system.capability
import system.driver
import system.memory.dma
import system.ipc

service VirtioBlock(device: capability.PciFunction) -> driver.BlockDevice {
    requires {
        pci.configure(device)
        irq.bind(device)
        dma.allocate(max_bytes: 16 MiB)
        publish("block.device")
    }

    let registers = pci.map_bar(device, 0)?
    let queues = setup_queues(registers)?

    loop {
        select {
            request = receive<BlockRequest>() => handle(request, queues),
            interrupt = await_irq(device) => complete_requests(interrupt, queues),
            stop = shutdown() => break,
        }
    }
}
```

This example expresses intent only. It must not be used as an accidental grammar.

## Blocking semantic requirements

The selection ADR must define or adopt:

- lexical grammar and Unicode normalization;
- complete syntactic grammar;
- static type rules;
- dynamic semantics;
- evaluation order;
- integer overflow behavior;
- memory ownership/borrowing/region behavior;
- error and panic behavior;
- concurrency and cancellation semantics;
- module resolution;
- capability import and transfer semantics;
- FFI/ABI boundary;
- deterministic lowering rules;
- source-map rules;
- resource accounting and preemption;
- unsafe-code boundary;
- versioning and compatibility policy.

## Required type categories

At minimum:

- fixed-width signed and unsigned integers;
- `bool`;
- Unicode `string` and raw `bytes`;
- tuples and records;
- tagged unions/enums;
- arrays and bounded slices;
- `Option<T>`;
- `Result<T, E>`;
- typed handles;
- capability types that cannot be forged from integers;
- duration and size literal types;
- functions and closures in the full profile;
- futures/tasks in the full profile.

## Memory model requirements

Ordinary modules must not receive unrestricted raw pointers.

Required mechanisms include:

- owned values;
- borrowed immutable or mutable regions with enforceable lifetime/alias rules;
- typed shared-memory handles granted by the nucleus;
- explicit DMA regions for drivers;
- unsafe operations confined to reviewed modules with declared invariants.

The bootstrap contract must not require a stop-the-world collector. An implementation may use arenas, reference counting or another internal strategy only if observable semantics and pause/resource limits are specified.

## Errors and diagnostics

Recoverable failures use a typed result mechanism. Fatal invariant failure terminates the current process unless supervisor policy escalates it.

Every parser/runtime error includes:

- stable error code;
- module identity;
- source content ID;
- file path;
- byte span and line/column;
- causal chain;
- structured values safe to log.

## Modules

A module declares:

- canonical name;
- language and semantic version/profile;
- exports;
- imports with constraints;
- requested capabilities;
- runtime profile;
- deterministic source identity;
- optional tests and health probes.

Imports resolve against the active system commit and explicit overlays. Resolution cannot depend on ambient working directory, network, time or undeclared host state.

## Concurrency

The required model is structured concurrency rather than unmanaged detached threads by default:

- tasks belong to a scope;
- cancellation propagates to children;
- resource handles close deterministically;
- drivers bind interrupts to explicit event streams;
- blocking operations are visible in the type/effect or API contract.

## Metaprogramming

Unrestricted textual macros are excluded from the bootstrap profile. Any future macro system must be hygienic or equivalently attributable, preserve source maps and include generated expansion identity in cache keys.

## Standard-library boundary

Filesystems, networking, UI, Git operations and devices are services through versioned interfaces, not hidden language intrinsics.

## Selection process

ADR-0015 and `docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md` govern the comparison.

Candidate classes include:

- bespoke TOS Core;
- TOS source over an existing formal execution core;
- a restricted/extended existing language;
- an unchanged existing language only if every blocking requirement is met honestly.

Lua, Scheme, WebAssembly and other systems are research inputs, not pre-approved foundations. Wasm may be a backend while TOS text remains canonical.

## Licence of language assets

The official runtime and standard implementation are GPL-3.0-or-later. Public grammar schemas, frontend ABI definitions, bindings and conformance libraries may be Apache-2.0 when explicitly marked. The prose language specification is CC-BY-SA-4.0.

<!-- END docs/05_TOS_CORE_LANGUAGE.md -->

---

<!-- BEGIN docs/06_EXECUTION_AND_IR.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Execution model and intermediate representation

> The exact language foundation and its lowering boundary remain subject to Stage 1.5 and ADR-0015. This document specifies required execution properties, not a preselected parser implementation.

## Principle

TOS promises canonical text, not mandatory slow interpretation. Execution is a pipeline whose derived stages remain subordinate to source identity.

```text
UTF-8 source
  -> language frontend
  -> syntax tree
  -> semantic analysis
  -> typed TOS IR
  -> verified module image
  -> interpreter, bytecode engine, or native-code backend
```

No generated stage becomes the authoritative installed program.

## TOS IR

TOS IR is a versioned, typed, capability-aware intermediate representation shared by all supported language frontends.

It must represent:

- typed values and control flow;
- functions and calls;
- explicit error edges;
- capability operations;
- IPC send/receive operations;
- memory-region operations;
- async suspension points;
- source maps;
- resource limits;
- module imports and exports;
- driver-specific operations only through typed service contracts.

TOS IR is not a public promise of permanent binary compatibility between arbitrary versions. Its schema is versioned, and caches state the exact runtime and verifier versions that produced them.

## Verification

Before execution, the verifier checks:

- structural validity;
- type correctness;
- valid control-flow targets;
- no use of undeclared imports;
- capability operation compatibility;
- memory-region bounds rules;
- bootstrap-profile restrictions;
- maximum declared stack and resource limits where required;
- source map consistency.

Invalid IR is never executed, even if loaded from a local cache.

## Cache identity

A generated module cache key includes at least:

```text
source_content_id
language_frontend_content_id
language_version
runtime_abi_version
ir_schema_version
verifier_version
optimization_profile
target_architecture
capability_contract_digest
```

Changing any component invalidates the cache.

## Cache location

Generated artifacts live under `/cache/tos/` or another explicitly disposable cache store. They never appear as required tracked files in `/system`.

## Execution engines

The architecture supports several engines:

1. **Reference interpreter** — simplest auditable semantics; mandatory for tests and recovery.
2. **Bytecode engine** — compact efficient default.
3. **JIT backend** — optional for long-running services and applications.
4. **Ahead-of-use native cache** — generated locally or by a trusted builder, always verified against source identity.

All engines must pass the same conformance suite. Wasm or another binary format may serve as a backend or cache profile only when canonical text, verifier independence and source identity remain authoritative.

## Performance contract

Execution engines, parsing, verification and cache validation are measured under `docs/35_PERFORMANCE_CONTRACTS.md`. An optimized engine is accepted only if semantic and provenance conformance remains identical to the reference path.

## Hot activation

A running service may be replaced by a new source revision through a supervisor transaction:

1. parse and verify replacement;
2. start replacement with a new capability set;
3. perform versioned state handoff if supported;
4. route new requests to replacement;
5. drain or cancel old instance;
6. commit activation record;
7. roll back automatically if health checks fail.

Code is not patched in-place inside a process. Replacement preserves clear identity and rollback.

## Source maps

Every executable instruction maps to:

- repository commit;
- path;
- source content ID;
- language frontend ID;
- byte span;
- optional macro expansion chain.

Logs, traces, crashes, and profiling data use this mapping.

## Determinism

Parsing and lowering must be deterministic for identical inputs and declared environment. Frontends cannot read time, network, random state, or untracked files while producing IR unless such inputs are explicitly part of the cache key and build record.

## Provenance contract

Every IR or executable cache object is keyed by more than source text alone. The cache identity includes:

- normalized source object IDs and dependency closure;
- source commit or detached source-set identity;
- frontend implementation and semantic profile;
- IR schema and verifier version;
- execution backend and target ABI;
- optimization and safety policy;
- capability import contract.

The runtime refuses stale or ambiguous caches. Deleting all cache stores must leave a recoverable, functionally complete system, subject only to regeneration time.

## Backend neutrality

A backend such as an interpreter, bytecode VM, Wasm engine or native compiler may be used without becoming canonical. Backend adoption is reviewed separately from source-language adoption. External engines default to isolated services or test oracles until an ADR accepts their trust and dependency consequences.

<!-- END docs/06_EXECUTION_AND_IR.md -->

---

<!-- BEGIN docs/07_LANGUAGE_FRONTENDS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Extensible language frontends

## Objective

TOS should learn new programming languages without enlarging the binary nucleus. A language frontend is a textual system module that parses source in another language, performs semantic analysis, and emits verified TOS IR.

## Frontend contract

A frontend provides versioned functions equivalent to:

```tos
interface LanguageFrontendV1 {
    fn describe() -> LanguageDescriptor
    fn probe(path: string, prefix: bytes) -> ProbeResult
    fn parse(source: SourceUnit, options: ParseOptions) -> Result<SyntaxUnit, Diagnostics>
    fn analyze(unit: SyntaxUnit, imports: ImportResolver) -> Result<TypedUnit, Diagnostics>
    fn lower(unit: TypedUnit, target: IrTarget) -> Result<IrModule, Diagnostics>
    fn format(source: SourceUnit, options: FormatOptions) -> Result<string, Diagnostics>
}
```

The actual ABI is defined in schemas, not by relying on textual syntax shown here.

## Language descriptor

A descriptor declares:

- language name and stable identifier;
- frontend source content ID;
- supported language versions;
- file extensions and optional shebang forms;
- required runtime services;
- whether bootstrap use is supported;
- deterministic behavior guarantees;
- sandbox limits;
- compatibility claims and known deviations.

## Installation

A frontend is installed as source under a system path such as:

```text
/system/languages/lua/
/system/languages/scheme/
/system/languages/python-subset/
```

Its manifest is part of the system commit. Activating a frontend follows the same candidate validation and rollback rules as other system modules.

## Trust and isolation

A frontend processes untrusted text and therefore runs in a restricted process. It does not automatically receive filesystem, network, device, or repository-write capabilities.

A malicious or defective frontend can fail compilation of its language but should not compromise the nucleus or other modules.

## Import resolution

Frontends do not fetch dependencies directly. They request imports through a deterministic resolver bound to:

- the selected system commit;
- explicit package or module commits;
- the current working overlay when allowed;
- declared lock data.

Network resolution is a separate explicit system operation.

## Compatibility levels

Each frontend states one of:

- `native` — language designed for TOS and fully specified by the project;
- `compatible` — aims to conform to an external language specification;
- `subset` — intentionally supports a named subset;
- `translated` — accepts source but maps semantics through documented changes;
- `syntax-only` — tooling support without execution.

TOS never labels a subset as full compatibility.

## Foreign runtimes

Some languages require a runtime, garbage collector, dynamic object model, or native extension system. Those components run as textual services or verified derived caches where possible.

Native extensions from conventional ecosystems are not silently accepted. They require an explicit compatibility process, sandbox boundary, or source port.

## Bootstrapping new frontends

The first frontend is TOS Core and is implemented partly in the nucleus and partly as boot modules. Later frontends should be written in TOS Core. Once the system is self-hosting, portions of the TOS Core frontend may also move out of the nucleus, provided recovery retains an independently bootable reference implementation.

## Architectural limits on frontends

A frontend teaches TOS to understand another textual language. It does not receive ambient hardware access, define a second package universe or replace system commit identity.

A frontend descriptor declares:

- source media type and normalization rules;
- semantic compatibility profile;
- required frontend capabilities;
- emitted IR version range;
- deterministic dependency resolution rules;
- cache and source-map behavior;
- licence metadata for runtime components.

A language syntax subset must be labelled as a subset. Calling an external interpreter through IPC is a foreign runtime integration, not native frontend compatibility.

<!-- END docs/07_LANGUAGE_FRONTENDS.md -->

---

<!-- BEGIN docs/08_GIT_NATIVE_SYSTEM.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Git-native system model

## Principle

The durable identity of a TOS installation is a commit graph. The operating system is not merely stored in a Git repository for development; repository semantics are part of normal operation.

## Canonical scope

The system repository contains:

- nucleus and loader source;
- textual system components and drivers;
- language frontends;
- schemas and policies;
- tests and documentation;
- machine-profile templates;
- build manifests and source-to-artifact attestations.

Derived nucleus images, IR caches, JIT output, logs, mutable databases and secrets are not canonical repository content.

## Compatibility is profiled

TOS does not make an undifferentiated promise of “Git support.” `docs/36_GIT_COMPATIBILITY_PROFILES.md` defines G0 through G6.

Stage 1 requires G0 provenance identity. Stage 5 requires at least G2 deterministic local history. Stage 7 remote recovery requires the declared G4 transport profile.

A release states its object format, hash family, ref profile, pack support and transport support explicitly.

## Nucleus versus userspace responsibility

The nucleus implements only mechanisms required for trusted boot and immutable mounting at the selected profile:

- algorithm-qualified content-ID parsing;
- object-integrity verification;
- bounded commit/tree traversal through a narrow object-store interface;
- reference selection from boot control;
- immutable tree exposure;
- protected transactional ref primitives.

Textual privileged services implement:

- working-overlay status;
- diff;
- object and commit creation;
- branch management;
- merge and conflict handling;
- fetch, push and clone;
- pack/index optimization;
- signature-policy UI;
- retention and garbage collection.

No stage is required to implement every item at once. Its profile states the exact subset.

## Active tree

At boot, the selected commit tree is mounted read-only as `/system`.

A writable overlay at `/work/system` records proposed source changes. A union view may expose them to development tools, but running services report whether they originated from committed or overlay source.

The active commit is not decorative metadata around an independently installed binary tree.

## Commit creation

A system commit operation performs:

1. source validation;
2. module dependency resolution;
3. required tests;
4. capability-manifest validation;
5. schema compatibility checks;
6. performance/identity checks required by the stage;
7. optional reproducibility checks for nucleus changes;
8. object and commit creation;
9. optional signing;
10. update of a non-active branch or candidate reference.

Committing does not automatically activate the commit.

## Commit metadata

In addition to ordinary Git commit data, TOS records structured versioned metadata in an ordinary-tree-visible form or signed note:

- TOS schema version;
- parent system commit;
- machine/hardware profiles tested;
- test and performance results;
- threat-model/security evidence changes;
- required state migrations;
- nucleus artifact attestations;
- capability-policy changes;
- loader/nucleus compatibility;
- human-readable rationale.

Metadata remains inspectable by ordinary Git clients even if they do not interpret it.

## Branch and protected-ref model

Suggested names:

```text
refs/heads/main                    upstream system
refs/heads/machines/<machine-id>  machine customization
refs/heads/users/<name>           optional user environment
refs/heads/experiments/<topic>    experimental work
refs/tos/current                  active commit
refs/tos/candidate                next boot candidate
refs/tos/last-known-good          successful fallback
refs/tos/recovery                 protected recovery commit
```

Protected semantics may be implemented in boot-control storage rather than ordinary mutable files, but every transition remains explicit and auditable.

## Updates as merges

An update is a merge or fast-forward between histories, not replacement of opaque packages.

The update service shows:

- upstream and local changes;
- conflicts;
- capability changes;
- driver, language and schema impacts;
- state migrations;
- tests and performance evidence;
- candidate rollback plan.

## Bisect

TOS integrates automated bisect with boot and service health probes. A bisect session records tested commits and outcomes separately from immutable source commits.

## Retention and garbage collection

Garbage collection protects:

- current, candidate, last-known-good and recovery commits;
- signed release refs;
- commits required by retained state snapshots;
- commits not yet pushed to configured remotes;
- operator-pinned branches;
- objects needed by an in-progress activation or recovery transaction.

An object is not deleted merely because it is unreachable from `main`.

## Remote recovery

A recovery environment can clone or fetch a declared G4 profile and recreate system refs. Secrets and mutable state are restored through separate encrypted mechanisms.

## Work decomposition

Repository implementation is intentionally staged:

1. G0 source identity in capsule/runtime provenance;
2. G1 bounded loose-object reading;
3. G2 deterministic local object writing and protected refs;
4. G3 packed-object reading;
5. G4 remote transport;
6. G5 history manipulation;
7. G6 maintenance and scale.

This prevents packfiles, merge and networking from being hidden inside the phrase “Git-native.”

## Performance and threat requirements

Repository parsers and activation paths follow:

- `docs/34_THREAT_MODEL.md` for malicious object graphs, rollback, ref mutation and retention threats;
- `docs/35_PERFORMANCE_CONTRACTS.md` for lazy mounting, lookup, activation and scale fixtures;
- `docs/37_STAGE_IDENTITY_GATES.md` for proof that commit identity is the actual installed system.

## External implementations and patents

Command-line Git and libgit2 may be host-side oracles and tooling. They are not hidden runtime foundations without ADR review.

Before Stage 5 closes, active patent claims around content-addressed deployment, link-switching, patch mementos and rollback are reviewed. TOS activation remains commit/tree based rather than copying a patented claim combination for convenience.

## Licence boundaries

Repository schemas, independent interoperability readers and test vectors may be Apache-2.0. Official activation, boot-control and recovery services remain GPL-3.0-or-later.

<!-- END docs/08_GIT_NATIVE_SYSTEM.md -->

---

<!-- BEGIN docs/36_GIT_COMPATIBILITY_PROFILES.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Git compatibility profiles

## Purpose

“TOS supports Git” is too vague to be honest. Git contains object formats, refs, indexes, packfiles, transports, merge behavior, maintenance and a large compatibility surface. TOS declares a profile and passes its conformance suite.

The profiles are cumulative unless an ADR explicitly defines a specialized profile.

## G0 — Commit-addressed identity

TOS records an algorithm-qualified source commit/tree identity in boot and runtime provenance, but does not yet parse a persistent Git repository.

Required by: Stage 1.

Not a claim of Git repository compatibility.

## G1 — Bounded object reading

Capabilities:

- parse and verify the selected loose-object profile;
- read blob, tree and commit objects;
- traverse a tree through a bounded object-store interface;
- read explicitly supported refs;
- reject unsupported algorithms, malformed objects and ambiguous names;
- compare results against independent Git test oracles.

Excluded:

- object writing;
- packfiles;
- network protocols;
- merge/diff semantics;
- garbage collection.

## G2 — Deterministic local history

Adds:

- deterministic blob/tree/commit creation;
- branch and protected-ref operations;
- writable source overlay status;
- commit creation from TOS;
- candidate/current/last-known-good/recovery semantics;
- reflog or equivalent auditable ref-transition history;
- crash-safe local object publication.

Required by: Stage 5 exit gate.

Stage 5 may use loose objects and TOS-specific indexes. It must not claim full Git compatibility merely because ordinary Git can inspect the resulting history.

## G3 — Packed object interoperability

Adds:

- pack index reading;
- bounded pack and delta-chain validation;
- thin-pack policy if supported;
- resource quotas against decompression and delta bombs;
- compatibility tests with independent Git implementations.

Pack writing may be a separate subprofile.

## G4 — Remote interoperability

Adds:

- versioned fetch/push/clone protocol profile;
- authenticated transport;
- partial/interrupted transfer recovery;
- object and ref negotiation;
- credential isolation;
- malicious-remote tests.

Required by: Stage 7 remote-recovery exit gate.

The exact transport—SSH, HTTPS or another protocol—is declared separately.

## G5 — History manipulation

Adds specified subsets of:

- diff;
- three-way merge;
- conflict representation;
- bisect;
- revert;
- ancestry queries;
- signed metadata/notes policy.

Each command publishes semantic differences from command-line Git.

## G6 — Repository maintenance at scale

Adds:

- reachability and retention-root analysis;
- garbage collection;
- repacking;
- multi-pack indexes or equivalent;
- pruning safety;
- hash-family migration;
- corruption diagnosis and repair.

## Object-format declaration

Every repository and boot record names:

- object format/profile version;
- hash algorithm;
- object encoding rules;
- ref storage profile;
- normalization rules;
- supported pack/delta profile;
- extension requirements.

Unknown mandatory extensions fail closed.

## Nucleus boundary

The nucleus implements only the minimum bounded reading/verification mechanism justified by the active stage. Rich Git behavior belongs in isolated textual services.

A general Git library may be a host oracle or isolated service only under the external-implementation policy. It does not silently become the repository authority.

## Compatibility claims

Allowed examples:

- “TOS implements G1 for loose SHA-256 repositories.”
- “TOS histories are inspectable by Git version X under profile Y.”
- “Fetch is not implemented; remote compatibility is G2, not G4.”

Disallowed examples:

- “Git-compatible” without a profile;
- “supports Git” based only on hashes or a development repository;
- “full Git” without complete published conformance scope.

<!-- END docs/36_GIT_COMPATIBILITY_PROFILES.md -->

---

<!-- BEGIN docs/09_FILESYSTEM_AND_STATE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Filesystem and state separation

## Why one Git repository cannot contain every changing byte

The system source changes relatively rarely. Logs, caches, databases, leases, queues, and runtime state may change thousands of times per second. Mixing them would destroy meaningful history and make rollback unsafe.

TOS therefore presents one namespace backed by several stores.

## Namespace classes

### `/system`

Immutable source tree of the selected system commit.

Properties:

- read-only;
- content-addressed;
- reproducible;
- visible source identity for every file;
- executable modules load from here by default.

### `/work`

Explicit writable overlays for source development.

Properties:

- changes are visible through status and diff;
- not automatically trusted or activated;
- may contain multiple named workspaces or branches;
- can be discarded without altering current system.

### `/config`

Deployment and machine configuration.

Configuration may be:

- part of a machine branch;
- a separate signed repository;
- a layered commit;
- local uncommitted configuration in research mode.

The selected model must remain explicit. Secrets never appear here in plaintext.

### `/state`

Mutable durable state owned by services.

Examples:

- databases;
- message queues;
- network leases;
- service indexes;
- update transaction records;
- user session metadata.

State paths are namespaced by service identity and protected by capabilities.

### `/home`

User-owned data. Users may independently choose versioning, snapshots, or ordinary storage.

### `/secrets`

Encrypted secrets and keys. Access is capability-mediated and audit logged. Secret material is not exposed through ordinary recursive repository operations.

### `/cache`

Disposable generated data:

- parsed modules;
- IR;
- native-code caches;
- thumbnails;
- package indexes;
- downloaded but verified objects that can be reacquired.

Deleting `/cache` cannot destroy canonical functionality.

### `/run`

Ephemeral handles, sockets, service discovery entries, locks, and runtime metadata. Recreated on boot.

### `/dev`

A logical device namespace exposing service endpoints and capability-safe handles, not necessarily raw device files with ambient access.

## State schema versions

Every service with durable state declares:

- state schema identifier and version;
- compatible source-module versions;
- migration functions;
- downgrade policy;
- snapshot requirements;
- maximum supported migration chain.

A system commit requiring migration cannot become current until the migration plan is validated.

## Snapshot linkage

State snapshots may record the system commit with which they were consistent. Rollback tooling can warn when a state snapshot is newer or incompatible with the selected commit.

## Transaction boundaries

System-source commits and mutable-state transactions are separate. A coordinated update record ties them together:

1. snapshot state;
2. stage candidate commit;
3. run forward migration in candidate namespace;
4. boot candidate;
5. promote on health success;
6. retain reverse path or snapshot until policy allows cleanup.

## Filesystem implementations

The first implementation may use a simple native object store and state filesystem under QEMU. The VFS and capability contracts must not assume a particular disk format. Support for conventional filesystems may later be implemented as user-space services.

<!-- END docs/09_FILESYSTEM_AND_STATE.md -->

---

<!-- BEGIN docs/10_PROCESS_SERVICE_IPC.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Process, service, and IPC model

## Process identity

A process identity includes:

- process instance ID;
- module name;
- source content ID;
- system commit ID;
- language frontend ID;
- runtime engine ID;
- granted capability set;
- parent supervisor;
- start time and restart generation.

A PID alone is insufficient for audit and debugging.

## Services

A service is a supervised process that publishes one or more versioned interfaces. A service manifest declares:

- module entry point;
- offered interfaces;
- required interfaces;
- requested capabilities;
- startup dependencies;
- restart policy;
- health probes;
- state namespace and schema;
- shutdown timeout;
- resource limits.

## Supervisors

Supervision is hierarchical.

- The boot supervisor owns essential system services.
- Driver supervisors own device-driver instances.
- Session supervisors own user applications.
- Failure propagation follows explicit policy.

Restart loops are bounded and observable. Repeated failure can mark a candidate commit unhealthy.

## IPC primitives

The nucleus provides minimal primitives:

- typed endpoint handles;
- message send and receive;
- capability transfer;
- shared-memory region transfer;
- event and interrupt notification;
- cancellation;
- process lifecycle notification.

Higher-level request/reply, streams, pub/sub, and service discovery are textual libraries and services.

## Schemas

Every IPC interface has:

- stable interface identifier;
- semantic version;
- canonical schema source;
- compatibility rules;
- maximum message sizes;
- capability-transfer declarations;
- fuzz and golden-vector tests.

Schemas are part of the system commit.

## Service discovery

Discovery returns handles, not global names with implicit authority. A process can discover only services allowed by its granted namespace capability.

## Capability transfer

Capabilities may be:

- copied when explicitly duplicable;
- moved when linear;
- attenuated to fewer rights;
- wrapped by a broker;
- revoked through an owning service where revocation semantics exist.

A numeric handle cannot be guessed to acquire authority.

## Backpressure

IPC queues are bounded. Senders receive explicit backpressure or failure. The system does not allow unbounded memory growth through message accumulation.

## State handoff

Hot service replacement can use a versioned handoff protocol. The old service may transfer:

- listening endpoints;
- in-memory session descriptions;
- durable-state transaction position;
- capability handles;
- pending work metadata.

Handoff is optional. If unsupported, the supervisor performs a clean restart.

<!-- END docs/10_PROCESS_SERVICE_IPC.md -->

---

<!-- BEGIN docs/11_DRIVER_MODEL.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Driver model

## Goal

Device drivers should be ordinary inspectable textual modules with narrowly granted hardware capabilities. A driver crash should normally terminate one process, not the entire operating system.

## Bootstrapping problem

A text driver stored on disk cannot be read until a disk driver exists. TOS solves this with the boot capsule:

1. UEFI loader reads the nucleus and capsule using firmware facilities.
2. Capsule contains boot-critical textual drivers.
3. Nucleus starts the TOS Core runtime from memory.
4. Text driver initializes persistent storage.
5. Repository-backed versions replace capsule versions through a versioned handoff.

Thus the disk driver remains text without placing a full disk stack in the binary nucleus.

## Driver process

A driver instance receives only capabilities for its assigned device and supporting resources, such as:

- PCI function configuration;
- MMIO regions;
- I/O port ranges;
- interrupt endpoint;
- DMA allocator with limits;
- clock or timer service;
- firmware data subset;
- publication rights for a device interface.

It does not receive arbitrary physical memory or unrelated devices.

## Driver manifest example

```tos
module drivers.virtio.net

manifest driver {
    matches pci(vendor: 0x1af4, device: [0x1000, 0x1041])
    runtime_profile "bootstrap-capable"

    requires {
        capability pci.configure
        capability mmio.map
        capability irq.bind
        capability dma.allocate(max: 64 MiB)
        capability service.publish("net.adapter.v1")
    }

    provides "net.adapter.v1"
    state none
    restart restartable
}
```

## Driver interfaces

Drivers publish device-class interfaces rather than exposing hardware-specific details to applications. Examples:

- `block.device.v1`;
- `net.adapter.v1`;
- `input.keyboard.v1`;
- `display.scanout.v1`;
- `audio.stream.v1`.

Bus managers and class services may be separate processes.

## Interrupts

The nucleus acknowledges and routes low-level interrupts to driver event endpoints. Drivers must not block interrupt routing indefinitely. Shared interrupts are mediated by a bus or interrupt service with explicit acknowledgement semantics.

## DMA

DMA regions are allocated through a trusted service or nucleus primitive. The driver receives a bounded region and device-visible address mapping. IOMMU support should later enforce hardware isolation without changing the driver contract.

## Crashes and restart

A restartable driver declares how it reconstructs state. The supervisor can:

1. revoke device mappings;
2. reset the device through a bus service;
3. start a new driver instance;
4. restore published interface endpoints;
5. notify clients of interruption.

Storage drivers require special care to avoid silent data corruption. A crash may force read-only mode or full device revalidation.

## Porting open drivers

TOS can reuse knowledge from open-source drivers, but most drivers cannot be mechanically copied because they are deeply tied to another kernel's APIs.

Portable knowledge includes:

- register definitions;
- initialization sequences;
- firmware formats;
- packet and descriptor layouts;
- quirks and revision tables;
- error recovery state machines.

The integration layer must be rewritten against TOS bus, DMA, IRQ, memory, and service interfaces. License compatibility and attribution remain mandatory.

## Driver language requirements

Boot-critical drivers use the TOS Core bootstrap profile. Later drivers may use other frontends only if those frontends and runtimes are available before the device is required.

## Physical hardware strategy

Physical hardware support begins only after the QEMU contracts are stable. Priority should go to devices with public specifications and simple reset behavior. GPU and Wi-Fi stacks are separate major programs, not early milestones.

## Source reuse and legal provenance

Open driver source is not automatically reusable code. Porting separates:

- public hardware facts and register behavior;
- protocol sequencing and errata;
- operating-system integration structure;
- expressive source implementation.

The Linux kernel is generally GPL-2.0-only, which is not directly compatible with a GPL-3.0 combined work. TOS therefore prefers public hardware specifications, permissively licensed implementations, GPL-2.0-or-later files or documented clean-room translation of functional knowledge. Every imported table, firmware blob or source fragment receives provenance and licence review.

## Patent-sensitive mechanisms

Before finalizing interrupt delivery, DMA mapping or device-carried text drivers, maintainers review the patent landscape for surviving jurisdictional claims. The driver API should express general capabilities and leave platform mechanisms replaceable rather than copying a vendor’s exact patented sequence.

<!-- END docs/11_DRIVER_MODEL.md -->

---

<!-- BEGIN docs/12_SECURITY_CAPABILITIES_TRUST.md -->

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

<!-- END docs/12_SECURITY_CAPABILITIES_TRUST.md -->

---

<!-- BEGIN docs/34_THREAT_MODEL.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS threat model

## Status and scope

This is the normative architectural threat model for TOS. It describes assets, adversaries, trust boundaries, required properties and accepted non-goals. It does not claim that any property is implemented until a stage report names evidence.

The model applies first to the x86_64 UEFI/QEMU profile and expands through ADRs for physical hardware, networking and multi-user deployment.

## Security objective

TOS aims to make system behavior constrained, attributable, inspectable and recoverable while preserving the owner's authority to run modified source.

Readable source is an observability property, not a security boundary. Git history is an attribution and recovery mechanism, not proof of correctness. Signatures prove authorization and integrity, not safety.

## Protected assets

### A1 — Canonical system identity

The selected commit, its `/system` tree and the mapping from running modules to source objects must not be silently substituted.

### A2 — Recovery authority

The owner must retain a protected path to select last-known-good or recovery state after a failed or malicious candidate.

### A3 — Capability integrity

A process must not forge, widen or transfer authority outside explicit rules.

### A4 — Nucleus integrity

The binary trusted base, boot ABI and protected primitives must not be modified or driven into undefined behavior by untrusted input.

### A5 — Repository integrity

Objects, refs, activation records and retention roots must resist corruption, ambiguity, rollback confusion and unauthorized mutation.

### A6 — Source-to-runtime provenance

Derived IR, bytecode, native cache and capsule content must be bound to exact source and toolchain identities.

### A7 — Mutable state and secrets

`/state`, `/home`, `/secrets`, `/cache` and `/run` must not be confused with canonical `/system`, leaked through commits or rolled back without explicit policy.

### A8 — Owner control

A vendor, signer or update service must not convert trust policy into permanent denial of owner-authorized boot.

### A9 — Availability within declared limits

The system should contain faults and resource exhaustion according to declared budgets. Absolute denial-of-service resistance is not promised.

## Adversary classes

### T0 — Accidental defect

Malformed input, buggy source, interrupted writes, driver errors, operator mistakes and incompatible state migrations.

### T1 — Unprivileged application

Controls its own source and data, sends arbitrary permitted IPC, attempts capability abuse, resource exhaustion or information disclosure.

### T2 — Malicious textual service or driver

Possesses its granted capabilities and may intentionally misuse them, crash, lie about health, corrupt shared buffers or exploit nucleus interfaces.

### T3 — Malicious language frontend or derived cache producer

Attempts incorrect lowering, source-map forgery, verifier confusion, cache substitution or hidden behavior absent from canonical source.

### T4 — Malicious repository or remote

Supplies crafted object graphs, hash collisions where feasible, excessive recursion/delta chains, misleading refs, rollback commits, bad signatures or resource-exhaustion inputs.

### T5 — Local attacker with mutable-storage access

Can rewrite ordinary disk blocks or boot-control storage but does not initially possess trusted signing keys or arbitrary firmware execution.

### T6 — Supply-chain adversary

Compromises compiler, builder, dependency, firmware, signing process, generated source or release infrastructure.

### T7 — Physical/firmware adversary

Controls firmware, DMA-capable hardware outside isolation, debug interfaces or physical memory. Early TOS does not claim full protection against this class.

### T8 — Nucleus compromise

Arbitrary execution in the trusted nucleus. This is outside containment guarantees; recovery and independent verification may still detect or repair persistent consequences.

### T9 — Vendor or project authority acting against owner control

Uses signing, update, trademark or recovery policy to prevent the owner from running modified source. Official TOS architecture must resist this as a governance and design threat.

## Trust boundaries

1. firmware to loader;
2. loader to boot protocol and nucleus;
3. arbitrary capsule bytes to capsule parser;
4. repository bytes to object parser/verifier;
5. source text to language frontend;
6. frontend output/cache to IR verifier;
7. nucleus to user-space service through capability and IPC boundary;
8. driver to device through MMIO, interrupt and DMA grants;
9. active commit to writable overlay;
10. system repository to mutable state and secrets;
11. recovery authority to candidate activation;
12. local system to remote repositories and time/signature services.

Every implementation crossing a boundary names its input format, validation, authority, resource limits and failure behavior.

## Required security properties

### S1 — Fail closed on identity ambiguity

Unknown hash algorithms, unsupported format versions, duplicate normalized paths, ambiguous source mappings or unverifiable caches are rejected rather than guessed.

### S2 — Bounded parsing

Boot, repository, IPC, language and IR parsers must have bounded recursion, allocation and work or must enforce explicit quotas before processing attacker-controlled input.

### S3 — No ambient privilege

Authority originates from explicit capabilities. Configuration text may request authority but cannot grant it to itself.

### S4 — Capability attenuation

Delegation cannot create greater authority than the delegator possesses. Rights and object identity are both checked.

### S5 — DMA confinement

Drivers receive only explicitly mapped DMA regions and device resources. IOMMU absence or limitations are reported as a weaker security profile, not hidden.

### S6 — Verified derived execution

No IR or executable cache runs solely because it has a plausible filename or local origin. Identity, schema and verifier checks are mandatory.

### S7 — Transactional protected state

Candidate, current, last-known-good and recovery selection cannot enter an unrecorded half-updated state after expected interruption.

### S8 — Recovery independence

A failed active system must not be required to repair itself. Recovery has separately protected code, boot selection and minimum repository inspection.

### S9 — Mutable-state separation

Ordinary runtime writes cannot silently alter `/system`. Rollback of source does not silently reinterpret incompatible state without migration policy.

### S10 — Observable trust state

Production, community, owner-authorized and research modes are distinguishable in process identity, boot records and user-visible diagnostics.

### S11 — Owner-authorized boot

Official profiles provide a documented local recovery path for owner keys or explicitly authorized unsigned experimental commits.

### S12 — Audit without secret disclosure

Security-relevant events identify actors, source and capabilities while redacting secret material by construction.

## Threats by subsystem

### Boot and capsule

Threats include corrupted lengths, integer overflow, duplicate paths, fake source commit, capsule rollback and mismatch between nucleus ABI and source. Controls include deterministic format, whole-object digest, bounded parser, explicit compatibility fields, protected boot record and corruption tests.

### Language and runtime

Threats include parser differentials, nondeterministic lowering, type confusion, unbounded compile time, source-map forgery and malicious frontend behavior. Controls include normative grammar/semantics, sandboxed frontends, deterministic inputs, independent verifier, resource accounting and cross-engine conformance.

### IPC and capabilities

Threats include handle forgery, confused deputy, stale-handle reuse, schema confusion, queue exhaustion and unauthorized delegation. Controls include typed generation counters, explicit transfer, schema versions, quotas and audit identity.

### Drivers

Threats include malicious MMIO, DMA outside granted memory, interrupt storms, malformed device descriptors, stale completion and service starvation. Controls include user-space isolation, IOMMU profile where available, bounded queues, device reset, watchdogs and performance/resource contracts.

### Repository and activation

Threats include crafted object graphs, malicious packs, unauthorized ref movement, rollback to vulnerable commit, garbage collection of recovery objects and state/source incompatibility. Controls include compatibility profiles, bounded traversal, protected refs, signed or owner-authorized policy, retention roots, candidate health and migration declarations.

### Remotes

Threats include credential theft, malicious server data, downgrade, replay, time confusion and partial fetch. Network support must add transport-specific threat entries before Stage 7 closes.

## Accepted non-goals for early stages

- confidentiality or integrity against malicious firmware;
- protection from all physical attacks;
- availability against an attacker controlling granted device or CPU resources;
- formal verification of the complete system;
- secure multi-user isolation before the corresponding stage defines it;
- anonymous operation or traffic-analysis resistance;
- compatibility with arbitrary unsigned third-party binaries.

Non-goals must not be advertised as solved and must not weaken recovery or owner control silently.

## Security evidence levels

- **E0 design:** property exists only in documents;
- **E1 implemented:** code path exists and is reviewable;
- **E2 tested:** automated positive and negative tests exercise it;
- **E3 adversarially tested:** fuzzing/fault injection/red-team evidence exists;
- **E4 formally argued:** machine-checked proof or equivalently rigorous artifact exists for a named property.

Release notes state the evidence level for security claims.

## Stage mapping

- Stage 1: boot/capsule boundaries and source identity;
- Stage 1.5–2: parser, language, verifier, resource and source-map threats;
- Stage 3: capability, IPC and process isolation threats;
- Stage 4: interrupt, MMIO, DMA and storage-corruption threats;
- Stage 5: repository, refs, protected candidate/current/last-known-good/recovery
  selection, rollback, garbage collection and state migration threats;
- Stage 7: remote, network, credential and time threats.

A stage cannot close if its new boundary lacks a threat entry, negative tests and stated evidence level.

## Change rule

Any Level 2 or higher change must either update this document or identify the exact existing section that covers the new threat. “No security impact” is a claim requiring explanation.

<!-- END docs/34_THREAT_MODEL.md -->

---

<!-- BEGIN docs/13_UPDATE_MERGE_PACKAGE_MODEL.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Update, merge, and package model

## No opaque package installation for native components

Native TOS components are source trees and manifests in repository history. A package is therefore a commit, subtree, or dependency reference, not an opaque binary archive installed into hidden locations.

## System update workflow

1. Fetch upstream objects.
2. Verify object integrity and signatures.
3. Compare upstream with local system branch.
4. Show source, capability, schema, driver, and nucleus changes.
5. Merge or fast-forward into a new candidate commit.
6. Resolve conflicts explicitly.
7. Run static checks and tests.
8. Prepare state migrations and derived caches.
9. Install candidate boot artifacts if required.
10. Set candidate ref and reboot or hot-activate eligible services.
11. Promote after health success.

## Dependency representation

Dependencies must be deterministic. Acceptable models include:

- source subtree pinned to a commit;
- repository reference pinned by content ID;
- vendored source with provenance metadata;
- module object in the same repository.

Unpinned branch names are not valid runtime dependencies.

## Lock data

The active system commit contains a lock manifest listing exact dependency identities, frontend versions, schemas, and required runtime ABI.

## Conflicts

Conflicts are ordinary source conflicts plus TOS-specific semantic conflicts:

- two modules request incompatible capability policy;
- IPC schema versions diverge;
- state migration order conflicts;
- two drivers claim one device;
- nucleus ABI requirement changes;
- language frontend semantics change cache identity.

Merge tools must present both textual and semantic conflicts.

## Local customization

Machine-specific changes should remain on a named machine branch or layered configuration repository. Updating upstream becomes a merge rather than overwriting local files.

## Third-party applications

Applications may live:

- in the system repository;
- in separate signed repositories pinned by commit;
- in a user repository;
- in a temporary source workspace.

Installation means making the source identity and manifest available to a launcher, not copying a binary into a global directory.

## Native caches from remotes

A remote may distribute verified acceleration artifacts, but they are optional. The local system verifies that an artifact matches:

- source content;
- toolchain or engine identity;
- target architecture;
- runtime ABI;
- declared build process;
- signature policy.

Failure falls back to local execution from source.

## Removal

Removing a component means creating a new commit that no longer references its source and updating user or service configuration. Historical commits retain the component until garbage-collection policy permits deletion.

<!-- END docs/13_UPDATE_MERGE_PACKAGE_MODEL.md -->

---

<!-- BEGIN docs/14_OBSERVABILITY_DEBUGGING.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Observability and debugging

## Source-aware operation

Every diagnostic event should be traceable to the exact source that produced it. A running component exposes:

- system commit;
- source path;
- source content ID;
- frontend content ID;
- runtime engine;
- capability grant digest;
- process generation.

## Structured logs

Logs contain stable event identifiers and structured fields. Human text is additional presentation, not the only machine-readable content.

Example:

```text
event=driver.virtio.block.queue_timeout
commit=8f1c...
source=/system/drivers/virtio/block.tos
source_id=blob:3a7d...
device=pci:00:04.0
queue=1
elapsed_ms=5000
```

## Crash reports

A crash report includes:

- exception or panic code;
- source span;
- stack trace mapped to source;
- process and supervisor identity;
- granted capabilities;
- recent IPC events under privacy policy;
- system commit and overlay status;
- relevant device identity;
- restart decision.

## Live inspection

The system shell should support commands conceptually equivalent to:

```text
system process show <id>
system source locate <id>
system diff --running
system capabilities <id>
system trace <service>
system driver inspect <device>
system commit health <commit>
```

## Debugging text modules

The reference interpreter supports:

- breakpoints by source path and line;
- step into/over/out;
- typed local-variable inspection;
- capability and handle inspection;
- IPC message tracing;
- deterministic replay where inputs are captured;
- source revision comparison during hot replacement.

## Boot diagnostics

Boot emits machine-readable stage codes over serial and stores a bounded boot journal when storage becomes available.

A failed candidate boot records the last completed stage without modifying the candidate commit.

## Performance observability

The runtime attributes CPU time, allocations, IPC wait, cache hits, and JIT activity to source modules and source spans where possible.

## Audit trail

Security-sensitive events use an append-only audit service with external sealing or remote forwarding options. Audit data is mutable state, not committed system source.

<!-- END docs/14_OBSERVABILITY_DEBUGGING.md -->

---

<!-- BEGIN docs/15_TESTING_AND_VERIFICATION.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Testing and verification strategy

## Principle

TOS changes the boot, execution, driver and update models simultaneously. Tests are part of architecture, not a finishing phase.

Every stage requires:

- functional evidence;
- negative/adversarial evidence from the threat model;
- TOS identity evidence;
- performance evidence assigned to that stage;
- compatibility-profile evidence where relevant.

## Test layers

### Host unit tests

Pure libraries for capsule parsing, object identity, repository traversal, schemas, parsers, IR verification and merge logic run as ordinary host tests.

### Property and fuzz tests

Required for all parsers and untrusted inputs:

- boot protocol and capsule;
- Git objects, indexes and packs in the selected profile;
- IPC messages;
- language source and IR;
- filesystem/state metadata;
- device descriptors and queue data.

Properties include no panic, bounded resource use, deterministic output, quota enforcement and round-trip stability where applicable.

### Golden vectors

Versioned formats include committed vectors with:

- valid minimal object;
- valid complex object;
- each invalid boundary class;
- digest values;
- expected decoded representation;
- resource-limit expectations.

### QEMU integration tests

Automated tests verify, by stage:

- clean boot and corrupted-capsule rejection;
- source identity and text init execution;
- process isolation and capability denial;
- textual driver startup and failure containment;
- repository mount by commit;
- candidate promotion and failed-candidate rollback;
- Git restoration and bisect workflows.

### Runtime conformance

Reference interpreter, bytecode engine and future native backends run the same language/IR suite. Source maps, errors and resource limits are compared, not merely final output.

### Driver simulation

VirtIO and later device tests include malformed descriptors, reset, interrupt loss/storm, timeout, stale completion, DMA-boundary violation and device removal.

## Threat-model tests

Each threat introduced by a stage maps to at least one of:

- parser rejection test;
- capability-denial test;
- fault injection;
- fuzz target;
- recovery test;
- audit/provenance assertion;
- explicit accepted non-goal.

A change adding a new boundary without a negative test leaves the stage open.

## Stage identity tests

`docs/37_STAGE_IDENTITY_GATES.md` defines required evidence. Examples:

- Stage 1 capsule source identity matches the real repository commit;
- Stage 2 cache deletion regenerates executable state from text;
- Stage 3 textual service holds only declared capabilities;
- Stage 4 no hidden binary driver performs I/O;
- Stage 5 process identities and `/system` agree on active commit;
- Stage 6 edit/commit/activate occurs without undocumented host tooling.

## Reproducibility

A clean checkout builds and tests using documented commands. Toolchains are pinned. CI does not depend on developer home directories, undeclared networks or secret local caches.

The consolidated specification is deterministically regenerated and checked for drift.

## Performance tests

`docs/35_PERFORMANCE_CONTRACTS.md` is normative. Benchmarks begin before optimization and include environment, baseline, percentiles and source identity.

A stage cannot close on unmeasured qualitative performance. A benchmark oracle does not become accepted runtime architecture.

## Git compatibility tests

Claims follow `docs/36_GIT_COMPATIBILITY_PROFILES.md`. Independent Git implementations provide cross-checks at the declared profile. Pack, transport, merge and maintenance tests are not implied by loose-object reading.

## Architecture tests

Examples:

- no canonical executable cache committed under `/system`;
- driver implementation does not link into nucleus;
- protected refs require dedicated capability;
- active `/system` is immutable;
- cache deletion preserves functional recovery;
- process reports include complete source identity;
- boot/IPC schemas reject incompatible versions;
- language foundation has an accepted selection ADR before parser code is normative;
- generated consolidated specification matches sources.

## Fault injection

The harness injects:

- partial writes and power loss at every activation phase;
- corrupt and adversarial object graphs;
- driver crash/hang and interrupt anomalies;
- out-of-memory and quota exhaustion;
- invalid commits/signatures;
- state migration failure;
- stale or forged derived caches;
- recovery-media mismatch.

## Milestone gates

A milestone cannot close until:

- specified behavior has automated tests;
- relevant threat-model paths are exercised;
- identity gate evidence exists;
- performance contract is measured;
- claimed compatibility profile passes;
- documentation matches implementation;
- no placeholder remains in claimed scope;
- recovery from introduced failure modes is tested.

## Legal and provenance conformance

CI and release process enforce:

- SPDX identifiers;
- DCO sign-offs;
- dependency/third-party inventory;
- prohibited licence combinations;
- source-to-artifact manifest completeness;
- source/cache/runtime introspection;
- owner-authorized boot workflow;
- architecture change-level declaration.

See `docs/30_COMPLIANCE_AND_RELEASE_GATES.md` and `docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md`.

<!-- END docs/15_TESTING_AND_VERIFICATION.md -->

---

<!-- BEGIN docs/35_PERFORMANCE_CONTRACTS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Performance contracts

## Purpose

TOS does not promise performance by adjective. The architecture must be measurable early enough that a beautiful source model cannot hide an unusable execution path.

Performance contracts do not permit replacing TOS with a conventional implementation. Reference implementations may serve as benchmark oracles only under ADR-0011.

## Measurement rules

Every reported result includes:

- exact source commit;
- architecture and stage;
- QEMU and firmware versions;
- host CPU and virtualization mode;
- guest CPU count and memory;
- workload definition and data size;
- warm-up policy;
- sample count;
- median, p95 and p99 where latency applies;
- throughput and CPU utilization where applicable;
- comparison baseline source and build identity;
- profiler/trace artifact where feasible.

A number without its environment is not a conformance result.

## Budget classes

### Hard architectural budgets

These are topology/count constraints. Exceeding one requires an ADR because it indicates an architectural path change.

### Reference-platform budgets

These are quantitative thresholds on the documented QEMU reference platform. They may be revised with measurements and an ADR, but cannot be silently weakened to close a stage.

### Observational metrics

These are tracked from first implementation without a pass/fail threshold until evidence is sufficient.

## Stage 1 — Boot and capsule

Hard budgets:

- capsule parsing is single-pass or bounded multi-pass;
- no recursion dependent on untrusted capsule depth;
- parser performs no allocation proportional to attacker-declared count before validating total bounds;
- lookup of one canonical path does not require copying every payload.

Reference-platform budget:

- a capsule fixture containing 1,000 files and 16 MiB total payload validates and locates `/system/boot/init.tos` in no more than 250 ms p95 in release mode under the declared QEMU CI profile.

The threshold is deliberately loose for the first stage but prevents accidental quadratic design.

## Stage 1.5–2 — Language frontend and runtime

Hard budgets:

- parsing and lowering have explicit source-size, nesting, identifier and diagnostic quotas;
- bootstrap-profile execution has instruction/fuel or equivalent preemption accounting;
- cache validation is bounded by declared dependency closure;
- source maps are retained without requiring an unbounded in-memory duplicate of source.

Reference-platform budgets for the bootstrap profile:

- parse, type-check, lower and verify a 256 KiB canonical module in no more than 500 ms p95;
- execute the standard one-million-operation integer/control-flow benchmark in no more than 10 times the host reference interpreter time under the same semantic implementation;
- reject quota-exceeding source within 2 times the accepted-input budget rather than degrading without bound.

These are initial research gates, not claims of application-language competitiveness.

## Stage 3 — IPC and capabilities

Hard budgets for steady-state small-message IPC after initialization:

- no dynamic allocation in the nucleus fast path;
- no more than two payload copies for an inline message;
- large payload transfer uses shared regions rather than copying payload through the nucleus;
- one request/reply exchange requires no more than four user/kernel boundary crossings excluding scheduler preemption;
- capability validation is constant-time with respect to the process's total capability count, or the alternative bound is documented and tested.

Reference-platform budget:

- p99 request/reply latency for a 64-byte message between two runnable processes is no more than 8 times an in-process function-call benchmark and no more than 200 microseconds on the declared QEMU CI profile.

Both relative and absolute limits are required because either alone can mislead.

## Stage 4 — VirtIO block textual driver

Hard budgets after queue initialization:

- zero dynamic allocation per completed block request on the steady-state path;
- no more than one payload copy between client memory and device-visible memory; zero-copy is preferred where the DMA contract permits it;
- no more than four address-space/scheduler handoffs per unbatched request;
- one interrupt wakeup may complete a batch of requests; the implementation must not require one scheduling cycle per descriptor when batching is available;
- no global driver lock serializes independent queues in the long-term contract.

Reference baseline:

A minimal, separately isolated Rust VirtIO-block benchmark implementation may be built only as a host/reference oracle. It is not an accepted nucleus driver and cannot satisfy the TOS stage gate.

Stage 4 reference-platform budgets:

- sequential throughput is at least 35% of the reference baseline for the same queue depth and image;
- random 4 KiB p99 latency is no more than 5 times the reference baseline;
- CPU time per MiB is no more than 8 times the reference baseline;
- performance results include textual-runtime engine identity and cache state.

Failure to meet a target does not justify hiding the driver in the nucleus. It triggers profiling, execution-engine work or an explicit architecture review.

## Stage 5 — Repository and activation

Hard budgets:

- mounting a commit tree does not require eager checkout of all blobs;
- lookup cost depends on path depth and object/index access, not total repository size;
- protected-ref activation uses a bounded transactional record and does not rewrite the system tree;
- rollback does not copy all system files;
- garbage collection cannot scan or mutate the live namespace while holding an unbounded global stop-the-world lock.

Reference fixtures:

- 100,000 paths, 20,000 commits and a 10 GiB logical object set;
- deep but bounded trees;
- adversarially long histories and malformed object graphs within parser quotas.

Reference-platform budgets:

- resolve and expose a selected commit root within 2 seconds p95 when required indexes are warm and within 10 seconds cold;
- switch candidate boot metadata in under 100 ms excluding health checks;
- `status` over a 10,000-file overlay completes within 3 seconds p95;
- failed activation returns to last-known-good without work proportional to total `/system` bytes.

## Stage 7 — Network

Before implementation, Stage 7 adds explicit budgets for packet copies, context crossings, throughput, p99 latency, interrupt moderation and memory pressure. “Line rate” is not a valid requirement without link speed, packet size and CPU budget.

## Regression policy

CI retains benchmark history. A regression above 15% in a hard-gated metric requires explanation; above 30% blocks a stage/release unless an ADR changes the contract.

Debug builds are never compared to release baselines. Benchmark fixtures and parsers are versioned.

## Reporting status

Each performance claim is labelled:

- **P0 unmeasured design**;
- **P1 locally measured**;
- **P2 reproducible CI measurement**;
- **P3 independently reproduced**.

No stage closes on P0 for a metric assigned to that stage.

<!-- END docs/35_PERFORMANCE_CONTRACTS.md -->

---

<!-- BEGIN docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture conformance tests

TOS requires tests for its identity, not only for functions.

## Canonical-source tests

- delete every bytecode/native cache and confirm regeneration from source;
- mutate source and reject the old cache;
- runtime introspection reports path, hash, commit, frontend and engine;
- no textual module requires an undeclared host compiler.

## Repository identity tests

- boot two commits and prove `/system` differs exactly as declared;
- fail a candidate and return to last-known-good without mutating it;
- active commit, boot record and process source identities agree;
- mutable state cannot dirty `/system`;
- claimed Git profile passes its exact suite.

## Owner-control tests

- authorize an owner key or explicit experimental branch through recovery;
- boot modified source without vendor secrets on an official developer profile;
- warnings do not become irreversible lockout;
- restore a previous commit from recovery media.

## Trusted-base tests

- dependency inventory contains only approved nucleus components;
- no network stack, rich Git service, general language runtime or ordinary driver enters nucleus accidentally;
- parsers reject malformed input without panic and within quota;
- capabilities are required for every privileged primitive.

## Stage identity tests

Every stage report maps to `docs/37_STAGE_IDENTITY_GATES.md` and has automated evidence where possible.

Mandatory examples:

- Stage 1 official capsule commit exists and source hash matches;
- Stage 1.5 selection ADR exists before normative parser implementation;
- Stage 2 runtime/cache trace terminates at canonical source;
- Stage 3 textual service exercises real capability enforcement;
- Stage 4 device I/O disappears if the textual driver is removed;
- Stage 5 `/system` bytes are resolved from the active commit tree;
- Stage 6 self-edit workflow does not call undocumented host tools.

## Threat-model tests

- each new trust boundary has negative tests;
- malformed input work remains within quotas;
- protected refs reject unauthorized mutation;
- DMA tests detect out-of-grant attempts where the platform supports enforcement;
- rollback and state migration failures preserve recovery;
- evidence level is recorded honestly.

## Performance tests

- benchmark environment and source commit recorded;
- hard budgets on copies, allocations and crossings are asserted where instrumentable;
- reference-platform thresholds and percentiles checked;
- reference/oracle implementation remains outside accepted runtime architecture;
- regressions follow `docs/35_PERFORMANCE_CONTRACTS.md`.

## Licence and provenance tests

- SPDX scan has no unknown source file;
- third-party inventory resolves every dependency;
- generated artifact maps to canonical inputs;
- DCO sign-off exists for merged commits;
- prohibited licence combinations detected.

## Documentation integrity tests

- source manifest paths exist and are unique;
- every accepted ADR is listed;
- generated consolidated specification is byte-identical to generator output;
- generated header identifies version and source-manifest digest;
- no direct edit to generated file is accepted without a source change.

## Compatibility honesty tests

A claim is tied to a profile and test set. Parsing syntax alone does not count as running a language. G1 object reading does not count as G4 remotes or full Git. A passing profile publishes exactly what was tested.

<!-- END docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md -->

---

<!-- BEGIN docs/16_DEVELOPMENT_STAGES.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Development stages

## No MVP interpretation

These stages are not a sequence from disposable prototype to real system. Each stage closes a coherent layer using intended long-term contracts. The system may be paused after any stage without invalidating prior work.

Every stage must pass both its engineering exit gate and the corresponding identity gate in `docs/37_STAGE_IDENTITY_GATES.md`.

## Stage 0 — Architecture, governance and legal baseline

Deliverables:

- charter, manifesto and invariants;
- accepted foundational ADRs;
- boot/capsule requirements;
- repository layout;
- toolchain and CI policy;
- normative threat model;
- documentation hierarchy and deterministic consolidated-spec generation;
- TOS Core language requirements;
- licensing, provenance, patent and naming policy.

Engineering exit: no implementation begins on an undefined boundary.

Identity exit: the normative documents distinguish TOS from a conventional microkernel with scripts and expose known contradictions rather than hiding them.

## Stage 1 — Trusted boot foundation

Deliverables:

- UEFI loader and x86_64 nucleus entry;
- memory map and exception setup;
- structured serial diagnostics;
- deterministic capsule v1;
- immutable source lookup;
- real source-commit or detached-source-set identity;
- QEMU harness and corruption tests;
- versioned boot protocol.

Engineering exit: clean checkout boots and validates canonical text from a real capsule.

Identity exit: capsule and nucleus prove exact source provenance; anonymous/hard-coded text is rejected as official evidence.

## Stage 1.5 — Language foundation decision

Deliverables:

- completed language evaluation matrix;
- comparable prototypes or formal evidence;
- trusted-base/dependency/performance analysis;
- canonical-source and verifier-boundary analysis;
- accepted language-foundation ADR.

Engineering exit: grammar/runtime implementation can begin against an accepted contract.

Identity exit: selected foundation preserves canonical text, capability semantics, bounded bootstrap and source observability without a hidden host ABI.

## Stage 2 — Native textual reference runtime

Deliverables:

- normative lexical, syntax and semantic specification;
- parser and complete diagnostics;
- bootstrap-profile type checker;
- TOS IR schema and independent verifier;
- reference interpreter;
- source maps and resource limits;
- conformance, performance and fuzz tests.

Engineering exit: `/system/boot/init.tos` executes real language semantics.

Identity exit: runtime behavior maps to canonical source and disposable caches can be deleted/regenerated.

## Stage 3 — Process, IPC and capability substrate

Deliverables:

- isolated address spaces and scheduler;
- capability handles;
- typed IPC;
- supervisors and service manifests;
- process source identity;
- failure/restart and authority-denial tests;
- IPC performance report.

Engineering exit: textual services communicate through final-style interfaces with enforced authority boundaries.

Identity exit: privileged behavior is exercised by source-identified textual processes, not hidden binary policy services.

## Stage 4 — Textual boot drivers and storage

Deliverables:

- PCI discovery service;
- interrupt/MMIO/DMA contracts;
- VirtIO block textual driver;
- persistent object/state storage;
- capsule-to-repository handoff;
- crash/reset and adversarial-device tests;
- Stage 4 performance contract report.

Engineering exit: persistent storage works through a textual user-space driver.

Identity exit: the textual driver performs actual I/O from canonical source; no binary shadow driver or hidden host path exists.

## Stage 5 — Git-native system tree

Deliverables:

- declared compatibility profile at least G2;
- bounded object store and commit/tree/blob traversal;
- immutable `/system` mount by commit;
- writable source overlay;
- status, diff, commit and branch services;
- protected refs and transition audit;
- candidate/last-known-good activation and rollback;
- repository performance/fault-injection reports.

Engineering exit: running system is identified by a commit and can return from a failed candidate.

Identity exit: commit tree is the installed `/system`, not metadata around another package/image authority.

## Stage 6 — Native shell and self-editing workflow

Deliverables:

- textual shell and editor/protocol;
- source inspection and module validation;
- transactional service replacement;
- commit creation inside TOS;
- documentation browser;
- recovery-shell parity for core operations.

Engineering exit: TOS modifies, validates, commits and activates its own services without the host OS.

Identity exit: owner-visible source is the actual installed system and changes flow through repository transactions.

## Stage 7 — Network and remotes

Deliverables:

- VirtIO network driver;
- network service architecture;
- transport-specific threat model and performance contracts;
- secure time policy;
- declared G4 fetch/push/clone profile;
- authenticated remotes and remote recovery.

Exit: recovery media plus credentials can restore `/system` from a remote while verifying identity and preserving owner trust choice.

## Stage 8 — Extensible languages

Deliverables:

- frontend ABI and sandbox;
- frontend registry;
- one second language frontend written through accepted TOS mechanisms;
- cache/source-map integration;
- honest compatibility declaration.

Exit: a language is added without nucleus modification or loss of source/runtime identity.

## Stage 9 — Broader device and UI platform

Potential deliverables include VirtIO input/GPU, compositor, shell/UI, USB, audio and a physical x86_64 profile. Each subsystem has its own threat, performance and identity gates.

## Stage 10 — Self-hosted nucleus toolchain

Long-term goal:

- build necessary nucleus artifacts within TOS or a reproducibly equivalent trusted service;
- record full provenance;
- verify candidate artifacts;
- preserve independent recovery-builder capability.

The nucleus source remains canonical; the boot image remains a necessary derived binary.

## Cross-stage gates

Every stage closes only after architecture, identity, engineering, threat/security, performance, compatibility, licence, provenance, patent/naming and documentation gates appropriate to it pass.

Before Stage 4 closes, review user-space interrupt, DMA and interpreted-driver patent/security mechanisms. Before Stage 5 closes, review content-addressed activation claims. Before commercial distribution, obtain jurisdiction-specific legal review.

<!-- END docs/16_DEVELOPMENT_STAGES.md -->

---

<!-- BEGIN docs/17_REPOSITORY_LAYOUT.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Repository layout

## Proposed monorepo

```text
/
├── README.md
├── ARCHITECTURE.md
├── AGENTS.md
├── CODEX_START.md
├── CONTRIBUTING.md
├── GOVERNANCE.md
├── LICENSE.md
├── PATENTS.md
├── SECURITY.md
├── TRADEMARKS.md
├── TOS_DEVELOPMENT_SPECIFICATION.md   # generated; never edit
├── DCO
├── LICENSES/
├── rust-toolchain.toml
├── Cargo.toml
├── boot/
│   ├── uefi-loader/
│   └── image-spec/
├── nucleus/
│   ├── arch/x86_64/
│   ├── memory/
│   ├── process/
│   ├── capability/
│   ├── ipc/
│   ├── repository-bootstrap/
│   └── runtime-bootstrap/
├── crates/
│   ├── boot-protocol/
│   ├── capsule/
│   ├── tos-hash/
│   ├── tos-schema/
│   ├── tos-parser/
│   ├── tos-ir/
│   ├── tos-repository/
│   └── tos-sdk/
├── interfaces/
│   ├── abi/
│   ├── ipc/
│   ├── schemas/
│   └── test-vectors/
├── host-tools/
│   ├── capsule/
│   ├── image/
│   ├── qemu-test/
│   ├── repo-inspect/
│   └── provenance/
├── system/
│   ├── boot/
│   ├── services/
│   ├── drivers/
│   ├── languages/
│   ├── shell/
│   ├── ui/
│   └── policy/
├── tests/
│   ├── vectors/
│   ├── integration/
│   ├── fuzz/
│   ├── conformance/
│   ├── performance/
│   └── architecture/
├── docs/
│   ├── SPECIFICATION_SOURCES.txt
│   ├── adr/
│   ├── research/
│   └── ...
├── legal/
│   ├── third-party/
│   ├── release-manifests/
│   └── publication-records/
├── tools/
│   └── build-specification.py
└── scripts/
    └── check-generated-spec.sh
```

## Licence defaults by area

- `boot/`, `nucleus/`, `system/` and official runtime implementation: `GPL-3.0-or-later`.
- `interfaces/` and explicitly designated SDK/test-vector libraries: `Apache-2.0`.
- `docs/` and root policy prose: `CC-BY-SA-4.0`.
- SPDX headers override directory defaults only when permitted by the licensing ADR.

## Directory rules

### `boot/`

Loader and deterministic image assembly. Generated images remain outside Git; source manifests and stable format fixtures are committed.

### `nucleus/`

Only binary trusted-base implementation. A dependency/subsystem requires an ADR, transitive inventory and minimal-TCB justification.

### `crates/`

Reusable implementation libraries. A crate is not Apache merely because it is reusable; licence is explicit. Parsers shared by host/target live here when practical.

### `interfaces/`

Independent interface definitions, schemas, bindings and conformance vectors. It must not hide core implementation.

### `host-tools/`

Developer/release tooling and external oracles. Runtime restoration cannot depend on undocumented host state.

### `system/`

Canonical textual system tree. No generated executable caches or binary packages are committed here. Firmware blobs, if supported, are separate and explicitly licensed.

### `tests/performance/`

Versioned workloads, benchmark harnesses, reference-oracle metadata and result schemas from `docs/35_PERFORMANCE_CONTRACTS.md`.

### `tests/architecture/`

Tests enforcing canonical source, provenance, owner control, trusted-base boundaries, repository identity and stage identity gates.

### `docs/adr/`

Accepted ADRs are immutable except spelling/link corrections. Superseding decisions add a new ADR.

### `docs/research/`

Non-normative research records, including language evaluation, patent landscape and name search.

### `tools/build-specification.py`

The only supported producer of `TOS_DEVELOPMENT_SPECIFICATION.md`. Output must be deterministic for identical sources.

### `legal/release-manifests/`

Stage identity reports, release provenance, SBOMs, licence inventories and signatures/attestations.

## Generated files

Generated artifacts go under ignored `target/`, `out/` or staging directories. `TOS_DEVELOPMENT_SPECIFICATION.md` is the deliberate exception: it is committed as a generated review artifact and verified against sources in CI.

Stable golden vectors are committed because they are specification fixtures, not runtime caches.

## Monorepo rule

The initial project remains a monorepo so formats, licences, runtime contracts and conformance tests change atomically. Repository splitting requires an ADR defining compatibility, ownership, release and legal boundaries.

<!-- END docs/17_REPOSITORY_LAYOUT.md -->

---

<!-- BEGIN docs/18_CODING_STANDARDS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Coding and specification standards

## Rust

- Stable Rust pinned by repository toolchain file.
- `no_std` for loader-shared libraries and nucleus code where applicable.
- Warnings are denied in CI for project crates after the initial scaffold is stable.
- Unsafe blocks require a `SAFETY:` comment stating why preconditions hold.
- No unchecked integer arithmetic for offsets, lengths, addresses, or object sizes.
- Parsing accepts byte slices and returns structured errors; it does not panic on malformed input.
- Dependencies are minimized in trusted code and reviewed for `no_std`, unsafe use, maintenance, and license.
- Public types and invariants are documented.

## TOS Core source

Until the formatter exists, source examples follow:

- four-space indentation;
- UTF-8;
- LF line endings;
- one module per file;
- explicit imports;
- no wildcard imports in system code;
- stable names for exported interfaces;
- capability requirements adjacent to service definitions;
- typed units for sizes and durations.

## Formats

Every on-disk or wire format specifies:

- magic;
- version;
- byte order;
- alignment;
- limits;
- checksum or digest behavior;
- unknown-field behavior;
- upgrade and compatibility rules;
- canonical encoding rules;
- security considerations;
- golden vectors.

## Error codes

Errors have stable symbolic identifiers. Numeric encodings may be assigned for wire protocols, but logs and source use symbolic names.

## Documentation

- Architecture documents state goals, invariants, mechanisms, failure modes, and rejected shortcuts.
- Examples are marked illustrative unless normative.
- No document uses "later" as a substitute for defining a boundary needed now.
- Open questions are listed explicitly rather than silently decided in code.

## Commits

A project commit should be reviewable and coherent. Commit messages explain why and identify the subsystem. Architecture-changing commits include or reference an ADR.

## Tests

- New behavior requires tests.
- Bug fixes add a regression test.
- Format changes update vectors and compatibility tests.
- Unsafe code receives focused tests.
- QEMU-visible behavior receives integration coverage.

## Logging

Trusted-base logging avoids allocation where practical and uses stable event IDs. Sensitive data is never emitted by default.

## Time and randomness

Code that needs time or randomness receives explicit providers. Tests use deterministic providers. Parsing, source lowering, object identity, and commit construction must not depend on wall-clock time except where the time value is an explicit input.

## Licensing and provenance

- Every source and documentation file uses an SPDX identifier.
- New Rust implementation defaults to the licence of its directory as defined in `LICENSE.md`.
- Copying from an external source requires an import record and compatible licence.
- Linux GPL-2.0-only implementation code is not copied into GPLv3 TOS components.
- Generated code records generator and source identity; generation does not erase licence notices.
- Commits require DCO `Signed-off-by` trailers.
- AI-assisted code is reviewed and attributed to the accountable human contributor, not the model.

## Architecture impact metadata

Pull requests that add formats, dependencies, privileges or public contracts include an architecture impact statement and change level from `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`.

<!-- END docs/18_CODING_STANDARDS.md -->

---

<!-- BEGIN docs/19_RISKS_AND_OPEN_QUESTIONS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Risks and open questions

This document prevents unresolved issues from becoming accidental implementation decisions.

## R1 — Language design scope

Creating a systems language, runtime, OS, driver model, and Git-native platform is an enormous combined effort.

Mitigation:

- keep TOS Core deliberately small;
- specify bootstrap and full profiles;
- complete Stage 1.5 comparative language-foundation review before implementation;
- build a reference interpreter only after the foundation is selected;
- avoid advanced syntax before capability, error, and memory semantics are complete;
- consider adapting a rigorously specified existing core language if a later ADR proves it better satisfies invariants.

## R2 — Performance of text drivers

Interpreted drivers may struggle with high-throughput devices.

Mitigation:

- canonical text with verified bytecode/native caches;
- zero-copy shared memory and DMA;
- batch IPC;
- quantitative contracts from `docs/35_PERFORMANCE_CONTRACTS.md` measured on VirtIO;
- optimize execution engine without changing source model.

## R3 — Git repository scale

Using Git semantics for an entire system may create object-count, checkout, merge, and garbage-collection challenges.

Mitigation:

- immutable object store and virtual tree access rather than eager checkout;
- pack and index services outside nucleus;
- explicit retention policy;
- explicit G0–G6 compatibility profiles rather than an all-or-nothing promise;
- compatibility tests and possible repository extensions that preserve ordinary Git visibility.

Open question: exact initial object/hash/ref profile and the evidence required to promote from G1 to G2/G3.

## R4 — Kernel/repository chicken-and-egg

The selected commit may require a newer nucleus than the currently booted image.

Mitigation:

- versioned minimum nucleus ABI in commit metadata;
- inactive boot slots;
- source-to-artifact attestations;
- recovery nucleus capable of fetching compatible artifacts;
- never destroy previous boot slot during candidate activation.

## R5 — Driver ecosystem effort

Modern GPU, Wi-Fi, USB, and audio support is vast.

Mitigation:

- QEMU and VirtIO first;
- public-specification hardware next;
- explicit non-goal of broad hardware support in early stages;
- tool-assisted porting of device knowledge from open drivers;
- compatibility services may later host existing user-space driver frameworks.

## R6 — Security of textual extensibility

Readable source may create false confidence.

Mitigation:

- capabilities, signatures, isolation, provenance, and transactional activation;
- source review tooling;
- no ambient access for language frontends;
- protected recovery.

## R7 — State rollback incompatibility

Rolling source back does not automatically roll mutable state back safely.

Mitigation:

- state schema declarations;
- linked snapshots;
- reversible migrations or explicit no-downgrade markers;
- candidate namespaces;
- recovery UI warning before incompatible rollback.

## R8 — Bootstrap trust size

A parser and interpreter in the nucleus could become large and dangerous.

Mitigation:

- strict bootstrap profile;
- small reference parser;
- move rich standard library to capsule text;
- fuzz every parser;
- consider a minimal verified bytecode loader only after preserving source-based recovery semantics.

Open question: exact boundary between binary parser, IR verifier, and textual frontend modules.

## R9 — Reproducible nucleus builds

Perfect bit-for-bit reproducibility may be difficult across toolchain and firmware changes.

Mitigation:

- pinned toolchain;
- hermetic build manifests;
- multiple independent builders;
- source and artifact signatures;
- reproducibility treated as a measured property, not assumed.

## R10 — Project size and abandonment

The project may be paused before becoming a daily-use system.

Mitigation:

- coherent stage gates;
- complete documents and tests at each stage;
- no throwaway architecture;
- the research results remain valuable even if implementation stops.

## Decisions still requiring ADRs

- exact initial bootloader strategy;
- exact Git object-format compatibility target;
- nucleus allocator policy;
- TOS Core language foundation selection under ADR-0015;
- selected language grammar, semantics and memory model;
- IPC schema language;
- first persistent object/state filesystem;
- cryptographic algorithms and key-management policy;
- SMP activation stage;
- state snapshot mechanism;
- exact official project name after trademark clearance;
- first professional patent/FTO review scope;
- future architecture-council succession model.

## R11 — Architectural erosion by mature substitutes

A familiar library or runtime may solve a local problem while converting TOS into a conventional microkernel with scripts.

Mitigation:

- architecture preservation policy;
- external implementations default to oracle/host roles;
- dependency promotion ADRs;
- identity conformance tests.

## R12 — Licence incompatibility during driver reuse

Useful Linux driver implementations are commonly GPL-2.0-only, while TOS core is GPL-3.0-or-later.

Mitigation:

- exact file-level licence review;
- public hardware specifications;
- permissive or GPL-2.0-or-later sources;
- clean-room functional reimplementation;
- third-party inventory.

## R13 — Patent exposure in update and driver mechanisms

Individual pieces of TOS have been subjects of patents, and status differs by jurisdiction.

Mitigation:

- maintained landscape;
- claim-focused design review;
- design-around;
- defensive publication;
- professional FTO review before commercial distribution.

## R14 — Project name conflict

`TOS` is historically associated with Atari and is widely used as an abbreviation in other industries.

Mitigation:

- provisional combined name `TOS — TextOS`;
- rename-ready namespaces;
- formal trademark clearance before broad public branding;
- no copied Atari or military visual identity.

## R15 — Copyleft compliance and locked appliances

A distributor may misunderstand source, installation-information or notice duties.

Mitigation:

- release compliance gates;
- full source and provenance package;
- owner-installable conformance requirement;
- legal review for commercial User Products.

## R16 — Normative documentation drift

A consolidated specification or copied requirement may diverge from its source and mislead agents.

Mitigation:

- hierarchy in `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`;
- deterministic generator and source manifest;
- CI byte-for-byte drift check;
- generated file marked non-normative and read-only by policy.

## R17 — Incomplete threat reasoning

Capabilities and provenance may create false confidence if adversary powers and accepted non-goals are not explicit.

Mitigation:

- normative `docs/34_THREAT_MODEL.md`;
- stage-specific negative tests and evidence levels;
- mandatory update for new parsers, trust boundaries, DMA paths, remotes and protected-state mutation.

## R18 — Stage-order identity erosion

Years of ordinary boot, scheduler, PCI and driver work could produce a conventional microkernel with scripts before Git-native identity becomes visible.

Mitigation:

- identity gate for every stage;
- commit/source provenance begins in Stage 1;
- runtime source identity begins in Stage 2;
- actual textual authority and driver evidence in Stages 3–4;
- Stage 5 cannot close unless the commit tree is the installed `/system`.

## R19 — Benchmark-induced architecture substitution

A faster conventional reference driver/runtime may be promoted into production simply because it wins benchmarks.

Mitigation:

- reference implementations remain oracles under ADR-0011;
- performance failure triggers profiling or explicit ADR, not hidden relocation into nucleus;
- identity gate and performance report are reviewed together.

<!-- END docs/19_RISKS_AND_OPEN_QUESTIONS.md -->

---

<!-- BEGIN docs/20_GLOSSARY.md -->

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

<!-- END docs/20_GLOSSARY.md -->

---

<!-- BEGIN docs/22_LICENSING_COPYRIGHT_AND_REUSE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Licensing, copyright and reuse

## Goals

The legal structure should preserve the freedoms that TOS provides architecturally while allowing independent applications, tools and compatible implementations.

## Component policy

### GPL-3.0-or-later

The following are part of the reciprocal operating system:

- bootloader and nucleus;
- TOS Core reference runtime and verifier;
- official recovery environment;
- official system services and drivers;
- repository activation, rollback and boot-control implementation;
- official shell, editor and source inspection services;
- generated target code when it forms a covered derived work of these components.

### Apache-2.0

The following may be used independently:

- ABI structures and bindings;
- IPC and file-format schemas;
- SDK libraries;
- conformance client libraries;
- test-vector parsers and independent inspection tools explicitly designated Apache;
- language-frontend SDK surfaces.

An implementation is not reclassified as an SDK simply to avoid copyleft.

### CC-BY-SA-4.0

This applies to prose specifications, diagrams, governance and documentation. Code snippets embedded in documentation are dual licensed under `GPL-3.0-or-later OR Apache-2.0` unless marked otherwise.

### AGPL-3.0-or-later

This is reserved for a future official network service whose value would otherwise be delivered as a modified hosted service without source reciprocity. AGPL adoption is component-specific and requires an ADR.

## GPL installation freedom and TOS

Official distributions must not satisfy source obligations while blocking the owner from installing modified covered software. Where GPLv3 Installation Information obligations apply, the distributor must provide the necessary methods, procedures or authorization material. Independently of the narrow legal trigger, owner-controlled boot remains a TOS conformance requirement for official builds.

## Copyright

Contributors retain copyright. The project records authorship through Git and DCO sign-offs. There is no default copyright assignment and no broad contributor licence agreement granting relicensing power to one party.

A future foundation may hold project assets or receive voluntary assignments, but existing contributors are not retroactively required to assign rights.

## Relicensing

Changing the licence of existing files requires permission from all relevant copyright holders unless the existing licence already permits the change. The Project Architect cannot unilaterally convert community-owned GPL code to a proprietary licence.

New major components may choose a compatible licence through ADR, but the central system licence matrix remains an architectural decision.

## Third-party compatibility

Every imported component is evaluated by exact file licence, not project reputation. Important examples:

- Apache-2.0 code can generally be incorporated into a GPLv3 combined work under GPLv3 terms;
- MIT/BSD code is usually compatible when notices are preserved;
- GPL-2.0-only code is not compatible with GPLv3 in one combined work;
- GPL-2.0-or-later code may be used under GPLv3;
- proprietary firmware or tools may be distributed separately only after legal and architectural review.

Because the Linux kernel as a whole is GPL-2.0-only, TOS must not assume that Linux driver source can be copied into GPLv3 code. Hardware facts are not copyright, but expressive implementation is. Prefer specifications, compatible files and documented clean-room reimplementation.

## Compliance artifacts

Each release must contain:

- full licence texts;
- source offer or source distribution as required;
- copyright and notice inventory;
- dependency and generated-code inventory;
- installation information when applicable;
- build and source provenance;
- machine-readable SPDX or equivalent SBOM when tooling exists.

## References

- GNU GPLv3: `https://www.gnu.org/licenses/gpl-3.0.html`
- GNU GPL FAQ: `https://www.gnu.org/licenses/gpl-faq.html`
- Apache License 2.0: `https://www.apache.org/licenses/LICENSE-2.0`
- Creative Commons BY-SA 4.0: `https://creativecommons.org/licenses/by-sa/4.0/`
- SPDX licence list: `https://spdx.org/licenses/`

<!-- END docs/22_LICENSING_COPYRIGHT_AND_REUSE.md -->

---

<!-- BEGIN docs/23_CONTRIBUTION_PROVENANCE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Contribution and provenance policy

## DCO model

TOS uses Developer Certificate of Origin 1.1 sign-off rather than a mandatory copyright-assignment CLA. A signed commit records that the contributor has the right to submit the contribution under the declared licence.

Required trailer:

```text
Signed-off-by: Real Name <email@example.com>
```

Bots may create commits, but an accountable human or legal entity must review and sign them before merge.

## AI-assisted work

AI output is not assumed to be novel or licence-clean. The submitting human must:

- inspect the complete diff;
- identify any suspicious reproduction of known code;
- avoid prompts that request copying a third-party implementation;
- preserve tool transcripts when provenance is uncertain;
- ensure the contribution can be explained and maintained;
- take responsibility through DCO sign-off.

For substantial generated modules, the pull request records tool name, model/version if known, prompting context category, human reviewer and verification performed. Private prompts or secrets are not required to be published.

## Imported code record

Every non-trivial imported or adapted work records:

- upstream project and canonical location;
- exact version, commit or release;
- original file paths;
- original licence and notices;
- modifications;
- compatibility decision;
- whether the code is runtime, build-only or test-only.

## Clean-room reimplementation

When a useful implementation has an incompatible licence, TOS may use a documented clean-room process:

1. one person or document extracts public functional requirements and hardware facts without copying expressive code;
2. the implementation is written from that neutral specification;
3. reviewers compare behavior, not source expression;
4. records identify the public specifications used;
5. no claim of legal “clean room” protection is made without counsel when stakes are material.

## Provenance gates

A contribution is blocked when:

- licence cannot be identified;
- DCO sign-off is missing;
- copied code appears incompatible;
- generated code origin is materially uncertain;
- a known patent dependency is intentionally hidden;
- the contributor lacks authority to submit employer-owned work.

<!-- END docs/23_CONTRIBUTION_PROVENANCE.md -->

---

<!-- BEGIN docs/24_PATENT_POLICY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Patent policy

## Scope

This policy manages patent risk without pretending that a volunteer project can prove worldwide freedom to operate.

## Default strategy

TOS uses:

- established licences with patent provisions;
- defensive publication of original architecture;
- a maintained landscape and design-around register;
- targeted review at high-risk architecture points;
- qualified legal review before material commercial distribution.

The project does not add a custom patent clause to GPL or Apache licences.

## Contributor duties

Contributors are not required to search patents. They must disclose a patent they actually know they or their employer control when the contribution is intentionally designed to practise a required claim. They must not offer code under a private patent arrangement that denies equivalent downstream rights.

A disclosure does not imply validity or infringement. It permits the project to investigate, redesign or reject the contribution.

## Review procedure

For a high-risk design:

1. define the proposed mechanism precisely;
2. search by concepts, classifications, assignees and citations;
3. identify patent families and jurisdictions;
4. read independent claims rather than relying on titles or abstracts;
5. create a non-legal engineering claim matrix;
6. document design differences;
7. request counsel when distribution risk justifies it;
8. preserve the decision in an ADR or legal review record.

## Design-around principle

Avoiding one optional claim element can be safer than debating a broad description. TOS should prefer its native commit graph, typed capabilities and source identity rather than copying a vendor’s exact link-switching, patch-memento, interrupt-stack or appliance-update mechanism.

## Freedom-to-operate gates

A professional FTO review is required before the project or an official distributor:

- sells a hardware appliance;
- signs a commercial indemnity;
- deploys a substantial paid hosted fleet service;
- distributes in a jurisdiction after a credible patent notice;
- deliberately implements a mechanism close to an active independent claim.

Research releases and source publication still require reasonable care, but they do not justify claims of complete patent clearance.

## Records

Public architecture risk belongs in `docs/research/PATENT_LANDSCAPE.md`. Privileged legal advice must not be committed publicly without counsel’s approval.

<!-- END docs/24_PATENT_POLICY.md -->

---

<!-- BEGIN docs/25_DEFENSIVE_PUBLICATION_PROTOCOL.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Defensive publication protocol

## Goal

Make original TOS architecture discoverable as prior art and prevent later parties from plausibly claiming it as a new invention, while preserving a reliable record of what was disclosed and when.

## Publication package

A defensive publication release contains:

- a clear title and abstract;
- named authors or project attribution;
- complete enabling architectural description;
- diagrams and protocol details sufficient for implementation;
- known alternatives and combinations;
- publication date and version;
- Git commit ID;
- SHA-256 digest of the archive;
- licence notice;
- stable public URL or archival identifier.

Vague slogans are weak disclosure. The publication should explain how canonical text, derived caches, commit-addressed boot, recovery, frontend extension and capability-confined textual drivers cooperate.

## Publication channels

At least two independent public channels are preferred:

1. signed release in the official Git repository;
2. durable archive such as Zenodo, Software Heritage, an institutional repository or another timestamped public archive.

A blog post may announce the release but is not the sole archival record.

## Timing

Publication occurs after the architecture is sufficiently described to enable the concept and before private commercial disclosure whenever practical. Public disclosure can limit the project’s own ability to seek patents, especially outside jurisdictions with grace periods. That is an intentional consequence of the defensive strategy and must be acknowledged by the Project Architect for major publications.

## Immutable record

After publication, the released archive is never replaced. Corrections create a new version that references the original. Git history alone is not treated as an incontrovertible timestamp because repositories can be rewritten; external archival evidence is required for significant disclosures.

## Disclosure log

`docs/research/DEFENSIVE_PUBLICATION_LOG.md` records title, version, commit, digest, public locations and covered concepts.

## No overclaim

A defensive publication does not prove that every implementation is patent-free and does not invalidate earlier priority. It contributes prior art from its effective public date.

<!-- END docs/25_DEFENSIVE_PUBLICATION_PROTOCOL.md -->

---

<!-- BEGIN docs/26_NAME_TRADEMARK_AND_CONFORMANCE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Name, trademark and conformance policy

## Provisional name

TOS — TextOS is a working name. `TOS` has extensive prior use as the Atari operating system name and as an industry abbreviation for terminal operating systems. Search results also reveal unrelated software using TOS-like names. Therefore public release preparation includes formal clearance in relevant jurisdictions.

## Technical rename readiness

Until clearance:

- package and protocol identifiers use versioned namespaces that can be renamed;
- disk and wire-format magic contains a format UUID or sufficiently distinctive identifier, not only `TOS`;
- URLs and organization names are configurable;
- documentation consistently uses `TOS — TextOS` to distinguish the project.

## Open code versus official identity

The licences permit forks. A future trademark policy may regulate only confusing brand use, not the right to modify or distribute code.

An official TOS release must pass architecture conformance. A fork may accurately state that it is based on TOS and must describe material deviations. It may not imply official approval.

## Conformance classes

- **TOS Source-Compatible** — understands published source and module contracts.
- **TOS Repository-Compatible** — can inspect and exchange the declared Git object/profile format.
- **TOS Architecture-Conforming** — preserves all active invariants, including owner-controlled boot and source-to-runtime traceability.
- **TOS Official** — produced or approved under official project release governance.

Conformance labels require published test results and the exact profile/version.

## The joke and public presentation

The internal joke about the Russian heavy flamethrower-system abbreviation may remain community folklore, but official visual identity must not imitate military marks or imply affiliation with a weapons manufacturer or government body.

<!-- END docs/26_NAME_TRADEMARK_AND_CONFORMANCE.md -->

---

<!-- BEGIN docs/27_THIRD_PARTY_COMPONENT_POLICY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Third-party component policy

## Principle

TOS reuses knowledge aggressively and dependencies conservatively.

A mature external project can serve as:

- documentation source;
- behavior reference;
- test oracle;
- host-side build tool;
- isolated runtime service;
- trusted-base dependency.

These roles have radically different architectural and legal effects. Promotion to a more trusted role requires explicit review.

## Trusted-base admission

A dependency entering the loader, nucleus, bootstrap parser or verifier must satisfy:

- narrowly necessary function;
- maintainable source availability;
- compatible licence;
- bounded input behavior;
- no ambient system dependency;
- fuzzability and test vectors;
- documented unsafe code and transitive dependency tree;
- acceptable update and vulnerability process;
- ADR approval.

“Widely used” is not sufficient.

## Language runtimes

Lua, Scheme, Wasm and other runtimes are research candidates, not automatic bootstrap choices. Any candidate must be evaluated for:

- canonical source relationship;
- type and capability enforcement;
- deterministic parsing and validation;
- resource bounds;
- interrupt and driver suitability;
- trusted binary size;
- source-map and cache identity;
- licence and patent profile.

A language may be added as a frontend without replacing TOS Core.

## Git implementations

libgit2 and command-line Git are valuable host tools and behavior oracles. The nucleus initially needs only bounded object and tree verification. Clone, merge, pack, transport, authentication and garbage collection stay outside the nucleus.

## Driver sources

Porting means translating device knowledge into TOS contracts, not mechanically compiling a Linux source file. Record the source of register definitions and protocol behavior. Avoid incompatible code copying. Firmware redistribution is reviewed separately from driver source.

## Inventory

The implementation repository will maintain machine-readable dependency and licence inventories. Vendored source includes upstream metadata and patch series. Network downloads during reproducible release builds are prohibited unless content is cryptographically pinned and mirrored in the build manifest.

<!-- END docs/27_THIRD_PARTY_COMPONENT_POLICY.md -->

---

<!-- BEGIN docs/28_RELEASE_PROVENANCE_AND_REPRODUCIBILITY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Release provenance and reproducibility

## Source release identity

Every release names:

- architecture specification version;
- source commit;
- active invariant set;
- accepted ADR set;
- compiler and toolchain identities;
- dependency lock state;
- generated schema/compiler versions;
- licence inventory;
- build command and environment description.

## Artifact provenance

A boot artifact, capsule or native cache is never anonymous. Its metadata links to:

- canonical source commit and source hashes;
- builder implementation and version;
- target architecture and ABI;
- build options;
- dependency/material digests;
- output digest;
- signature or attestation where available.

The artifact is still derived and disposable. Provenance does not elevate it above source.

## Boot capsule

The capsule manifest includes at minimum:

- format version;
- source commit or publication identity;
- nucleus ABI minimum and maximum;
- included canonical files with hashes;
- builder identity;
- whole-capsule digest;
- reproducibility status;
- licence notice set.

The capsule may be reconstructed from the commit plus documented tools. The recovery image may carry a copy, but the canonical capsule inputs remain in the repository.

## Reproducibility grades

- **R0 — described:** build steps and materials recorded;
- **R1 — repeatable:** same controlled environment reproduces output;
- **R2 — independently reproducible:** a second environment produces identical output;
- **R3 — multi-builder attested:** multiple independent builders publish matching results.

A release states its achieved grade rather than claiming reproducibility by aspiration.

## Documentation provenance

The consolidated specification records the ordered source manifest digest and is reproducibly generated. Official documentation releases include the generator, source manifest and drift-check result. The generated file is not a normative source.

Stage identity reports record their source commit, architecture version, compatibility profiles, threat evidence and performance artifacts.

## Archive retention

Official source archives, manifests, signatures, SBOMs and release notes are retained permanently. Derived convenience images may be mirrored or regenerated, but at least one verified recovery artifact for every supported architecture generation is retained.

## Integrity of the release package versus the source tree

TOS keeps exactly one source of truth for each kind of integrity, so that two
mechanisms can never disagree about what the project contains.

- **Git object identity is canonical for the source tree.** A commit names a
  tree, the capsule records that commit, and the boot chain re-verifies the
  capsule digest. `source/` is therefore not covered by a flat digest list: a
  second list would be a weaker duplicate of Git and a competing source of
  truth.
- **`SHA256SUMS` verifies the release-package files outside Git.** Its purpose
  is to let a recipient who received the documentation and governance package
  without a repository check that the files are intact. It covers exactly that
  package.
- **`MANIFEST.txt` describes the release baseline and is generated from its
  actual composition.** Aggregate values — file counts, invariant counts, the
  ADR list — are derived, never hand-maintained.

Both files are produced by `python3 tools/build-release-manifest.py` and
verified in CI with `--check`. A hand-edited count is not normative data; it is
a defect waiting to be discovered, as the manifest's own "15 accepted ADRs"
was while seventeen existed.

<!-- END docs/28_RELEASE_PROVENANCE_AND_REPRODUCIBILITY.md -->

---

<!-- BEGIN docs/29_PROJECT_GOVERNANCE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Project governance

## Governance phase A — architect-led

During foundational development, the Project Architect has final authority over invariants, architecture ADRs and stage closure. This is intended to protect a coherent uncommon design while agents and contributors naturally propose familiar substitutes.

Implementation review can be delegated. Architectural identity cannot be delegated implicitly.

## Roles

### Project Architect

- maintains the project thesis and invariant set;
- approves Level 3 and Level 4 architecture changes;
- decides whether a milestone is architecturally complete;
- appoints maintainers;
- approves defensive publications and official conformance use.

### Subsystem maintainer

- reviews code and tests within an accepted contract;
- maintains subsystem documentation;
- can reject quality, security or provenance failures;
- cannot waive project invariants.

### Release steward

- verifies legal, provenance, reproducibility and conformance gates;
- does not decide architecture alone.

### Contributor

- retains copyright;
- signs the DCO;
- follows licence, provenance and architecture policy.

## Decision process

Normal changes use public pull requests. Architectural proposals begin with an issue or design note and become ADRs before implementation commits depend on them.

The Project Architect may reject a technically sound proposal because it erodes TOS identity. The rejection should explain which invariant or project objective is affected.

## Disputes

Technical disputes are resolved by contract hierarchy and evidence. Licence or legal conflicts take priority over implementation preference. Personal conduct is handled separately from architecture review.

## Fork freedom

Governance controls the official project, not downstream rights. Anyone may fork under the applicable licences. A fork that rejects the invariants should use a distinct identity rather than pressuring the official project to become conventional.

## Future governance

After multiple independent maintainers demonstrate long-term understanding of TOS, governance may move to an architecture council. Such a change requires an ADR and an explicit mechanism preventing a simple majority from silently deleting core invariants.

<!-- END docs/29_PROJECT_GOVERNANCE.md -->

---

<!-- BEGIN docs/30_COMPLIANCE_AND_RELEASE_GATES.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Compliance and release gates

No official release is produced solely because functional tests pass.

## Architecture and identity gate

- active invariants and ADRs listed;
- no canonical executable binary introduced for a textual component;
- source-to-runtime identity available;
- owner-modifiable boot path preserved;
- trusted-base changes reviewed;
- applicable `docs/37_STAGE_IDENTITY_GATES.md` report complete;
- commit/source identity is real, not placeholder metadata.

## Engineering gate

- unit, integration, QEMU, fuzz and conformance suites pass;
- failure and rollback paths tested;
- persistent/wire formats versioned;
- known limitations documented;
- no stage closure around mocks or known replacement architecture.

## Threat and security gate

- `docs/34_THREAT_MODEL.md` covers every new parser, boundary, privileged operation, DMA path and remote;
- required negative/fault tests pass;
- security claims carry evidence levels;
- recovery cannot be overwritten by candidate activation;
- owner experimental mode remains available and visible;
- no secret embedded in repository/image;
- vulnerability-reporting path exists before network release.

## Performance gate

- assigned metrics in `docs/35_PERFORMANCE_CONTRACTS.md` measured;
- environment and baseline recorded;
- hard topology/count budgets satisfied or amended by ADR;
- threshold failures are not hidden by moving work into nucleus;
- regression policy applied.

## Compatibility gate

- Git compatibility profile declared and passed;
- language compatibility described as exact profile/subset;
- hardware support claim tied to tested profile;
- no broad ecosystem claim from superficial syntax or loose-object parsing.

## Licence gate

- SPDX identifiers present;
- licence texts included;
- dependencies compatible;
- third-party notices complete;
- GPL source/installation obligations satisfied;
- no GPL-2.0-only code copied into GPLv3 components;
- generated artifacts retain provenance/notices.

## Patent and naming gate

- high-risk mechanisms checked against landscape;
- original architecture queued for defensive publication;
- credible notices reviewed;
- release name/marks pass clearance state;
- no unsupported patent-clearance claim.

## Provenance gate

- source commit and tag authenticated;
- DCO checks pass;
- SBOM/dependency inventory produced;
- artifact manifests and hashes generated;
- reproducibility grade recorded;
- stage identity report archived;
- source archives published immutably.

## Documentation gate

- normative hierarchy has no known conflict;
- `python3 tools/build-specification.py --check` passes;
- source manifest contains every accepted ADR and required normative file;
- README map and ADR statuses current;
- release notes state architecture, security, performance, compatibility and reproducibility limits.

<!-- END docs/30_COMPLIANCE_AND_RELEASE_GATES.md -->

---

<!-- BEGIN docs/32_EXTERNAL_IMPLEMENTATION_POLICY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# External implementation and oracle policy

## Motivation

TOS should benefit from decades of existing engineering without allowing mature external implementations to define its architecture accidentally.

## Roles

### Reference specification

A document or standard used to understand behavior. It contributes no code automatically.

### Test oracle

An implementation used to produce expected outputs or compare behavior. Disagreement is investigated; the oracle is not assumed correct for TOS semantics.

### Host tool

Runs on the developer OS to build, inspect or test artifacts. The release records it, but TOS restoration must not secretly depend on it forever.

### Isolated service

Runs outside the nucleus behind a versioned capability contract. Failure cannot compromise the nucleus directly.

### Trusted dependency

Runs in the loader, nucleus or verifier. This role is exceptional and requires ADR approval.

## Examples

- command-line Git: host tool and repository behavior oracle;
- libgit2: host library or oracle, not default nucleus dependency;
- Wasm engine: candidate execution backend or isolated service, not canonical source format by default;
- Lua: possible secondary language frontend, not automatic TOS Core replacement;
- QEMU: platform and test environment, not a runtime component;
- Linux drivers: research reference and source of specification links, subject to licence restrictions;
- seL4: capability and verification research reference, not a claim that TOS inherits seL4 proofs.

## Promotion procedure

Moving a dependency to a more trusted role requires:

- architecture impact statement;
- licence and patent review;
- transitive dependency analysis;
- resource and failure bounds;
- replacement and recovery plan;
- tests demonstrating conformance;
- accepted ADR.

<!-- END docs/32_EXTERNAL_IMPLEMENTATION_POLICY.md -->

---

<!-- BEGIN docs/33_LEGAL_AND_RESEARCH_SOURCES.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Legal and research source register

Updated 2026-08-05. These sources support project policy but do not replace legal advice.

## Licences and open-source definition

- GNU General Public License v3: `https://www.gnu.org/licenses/gpl-3.0.html`
- GNU GPL FAQ, including GPLv2/GPLv3 compatibility: `https://www.gnu.org/licenses/gpl-faq.html`
- GNU GPLv3 quick guide and installation-information discussion: `https://www.gnu.org/licenses/quick-guide-gplv3.html`
- Apache License 2.0: `https://www.apache.org/licenses/LICENSE-2.0`
- Apache licensing guidance: `https://www.apache.org/legal/apply-license`
- Creative Commons BY-SA 4.0: `https://creativecommons.org/licenses/by-sa/4.0/`
- SPDX licence list and identifiers: `https://spdx.org/licenses/`
- Open Source Definition: `https://opensource.org/osd`
- Developer Certificate of Origin 1.1: `https://developercertificate.org/`

## Linux source licensing

- Linux kernel licensing rules: `https://docs.kernel.org/process/license-rules.html`
- The rules state that the Linux kernel is provided under GPL-2.0-only, subject to identified exceptions and compatible per-file licensing.

## Patent strategy

- WIPO patent FAQ, including defensive publication: `https://www.wipo.int/en/web/patents/faq_patents`
- WIPO patent landscape guidance: `https://www.wipo.int/edocs/pubdocs/en/wipo_pub_946.pdf`

## Patent records currently tracked

- Intel uncompiled peripheral driver: `https://patents.google.com/patent/WO1997024656A1/en`
- Microsoft user-mode interrupt delivery: `https://patents.google.com/patent/US7581051B2/en`
- Non-native/Java interrupt handling family: `https://patents.google.com/patent/US20020049865A1/en`
- Oracle CAS software-home patching: `https://patents.google.com/patent/US10762059B2/en`

Google Patents explicitly warns that displayed legal status is not a legal conclusion. Official jurisdictional registers and qualified counsel are required for reliance.

<!-- END docs/33_LEGAL_AND_RESEARCH_SOURCES.md -->

---

<!-- BEGIN docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Language foundation evaluation matrix

**Status:** non-normative research template required by ADR-0015.

## Decision to be made

Choose the foundation that will implement the TOS bootstrap profile and support the long-term TOS Core role without making a hidden binary/runtime ecosystem the true operating-system contract.

Candidate classes:

- **A — Bespoke TOS Core:** grammar, type system, IR lowering and reference runtime designed for TOS.
- **B — TOS surface over an existing formal core:** TOS source remains canonical while a rigorously specified lower core provides execution semantics.
- **C — Adapted existing language:** an existing language is restricted or extended to satisfy TOS contracts.
- **D — Existing language unchanged:** accepted only if it satisfies all blocking requirements without semantic fiction.

## Blocking requirements

A candidate is rejected if it cannot demonstrate:

1. canonical human-readable source remains authoritative;
2. deterministic parse and lowering from declared inputs;
3. bounded bootstrap implementation and resource accounting;
4. explicit capability imports that cannot be forged by ordinary code;
5. typed memory/region model suitable for services and drivers;
6. source maps through every derived stage;
7. independent verification before execution;
8. no ambient host filesystem/network/time access during lowering;
9. no undocumented C/host ABI becoming the real system ABI;
10. compatible licence and acceptable patent/dependency profile;
11. recovery implementation small enough to audit and fuzz;
12. multiple execution backends cannot disagree silently on semantics.

## Comparative criteria

For each candidate record evidence, not adjectives:

- normative specification size and maturity;
- trusted implementation size and transitive dependencies;
- parser/type-checker/verifier complexity;
- memory safety and unsafe boundary;
- concurrency semantics;
- deterministic behavior;
- interrupt/IPC/DMA expression;
- resource metering/preemption;
- diagnostics and source maps;
- boot-profile reducibility;
- frontend extensibility;
- performance profile;
- self-hosting path;
- tool support value versus architectural cost;
- licensing and contribution compatibility;
- implementation and maintenance effort.

## Required prototype exercises

Each serious candidate must implement or model the same exercises:

1. parse a malformed module corpus with stable diagnostics;
2. declare and enforce a PCI/MMIO/IRQ/DMA capability set;
3. lower a small block-driver state machine into typed IR;
4. reject an undeclared privileged operation;
5. enforce a bounded loop/fuel policy in bootstrap mode;
6. produce source maps through an optimized execution path;
7. invalidate a cache after one source/dependency change;
8. run the same semantic conformance vectors in two engines or interpreter modes;
9. build in a documented recovery-sized configuration;
10. report trusted-base and dependency inventory.

## Decision output

The Stage 1.5 report must contain:

- candidates evaluated;
- evidence repository/commits;
- blocking failures;
- measured results;
- trusted-base comparison;
- language and IR boundary;
- selected option;
- rejected alternatives;
- migration consequences;
- accepted selection ADR.

This matrix does not presuppose that a bespoke language wins. It prevents convenience from masquerading as architecture.

<!-- END docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md -->

---

<!-- BEGIN docs/research/PATENT_LANDSCAPE.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Preliminary patent landscape

**Status:** engineering research only, updated 2026-08-05. This is not a legal opinion, exhaustive search or freedom-to-operate conclusion. Legal status shown by public aggregators must be verified in official registers for each jurisdiction.

## Search clusters

- uncompiled or interpreted device drivers;
- drivers stored in peripheral devices;
- user-mode interrupt delivery;
- non-native interrupt handlers;
- content-addressed software update and rollback;
- immutable system trees and activation;
- source-derived execution caches;
- capability microkernel mechanisms;
- remote recovery by repository identity.

## L-001 — Intel portable uncompiled peripheral driver

- Family/publication: `WO1997024656A1`, priority 1995-12-29.
- Public status indicator: PCT publication shown as ceased.
- Relevant concept: uncompiled source or interpretive driver code stored in memory of a peripheral device, read by a system and compiled or interpreted through an OS driver interface.
- TOS intersection: textual drivers.
- Important distinction: ordinary TOS drivers are repository objects, not necessarily stored in the peripheral itself. Device-carried TOS drivers would require renewed family and jurisdiction review.
- Research URL: `https://patents.google.com/patent/WO1997024656A1/en`

## L-002 — Microsoft user-mode interrupt delivery

- US patent: `US7581051B2`, priority 2005-05-16.
- Public US status indicator: expired/lapsed; international family status must be checked separately.
- Relevant concept: masking interrupts below CPU level through APIC, bus or device mechanisms while notifying a user-mode driver through a generic kernel service.
- TOS intersection: user-space drivers and interrupt broker.
- Design note: do not copy the exact mechanism without checking surviving family members. TOS should specify a general interrupt capability and platform-specific delivery backend.
- Research URL: `https://patents.google.com/patent/US7581051B2/en`

## L-003 — Non-native/Java interrupt handler stack

- US publication/grant family: `US20020049865A1` / `US7058929B2` among a large grouped disclosure.
- Public US status indicator: expired.
- Relevant concept: a prepared non-native thread stack switched to on interrupt, restrictions around blocking and garbage collection, Java/non-native bytecode at interrupt level.
- TOS intersection: interpreted driver interrupt handling.
- Design note: TOS currently prefers nucleus interrupt acknowledgement and user-space event delivery rather than running a rich GC language directly at hardware interrupt level.
- Research URL: `https://patents.google.com/patent/US20020049865A1/en`

## L-004 — Oracle CAS software-home patch and rollback

- US patent: `US10762059B2`, priority 2018-01-31.
- Public status indicator: active, adjusted expiration shown as 2038-12-19.
- Relevant claim concepts observed in the public record: content-derived filenames, links from a software-home directory to content-addressed objects, updating links, preserving former links in patch mementos and rollback by restoring those links.
- TOS intersection: content-addressed system activation and rollback.
- Design response: TOS uses a commit/tree/blob graph, immutable commit-addressed `/system`, candidate refs and boot records. Do not implement the Oracle-specific hard-link/filename/patch-memento structure without a claim review.
- Research URL: `https://patents.google.com/patent/US10762059B2/en`

## Required follow-up searches

Before Stage 4:

- active international family claims around user-space interrupt/DMA delivery;
- interpreted or bytecode device-driver mechanisms;
- IOMMU capability allocation.

Before Stage 5:

- content-addressed OS deployment;
- immutable tree activation and rollback;
- Git-like boot and system-version selection;
- software-home snapshot patents.

Before Stage 7:

- remote recovery, signed fleet activation and repository-based appliance restore.

Before commercial release:

- professional search in intended jurisdictions using final implementation claim charts.

## Recording rule

A patent is not labelled “safe” because it appears old, expired in one country or conceptually similar. Record exact jurisdiction and independent claims. A design difference is an engineering hypothesis until reviewed by qualified counsel.

<!-- END docs/research/PATENT_LANDSCAPE.md -->

---

<!-- BEGIN docs/research/DEFENSIVE_PUBLICATION_LOG.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Defensive publication log

No external defensive publication has yet been recorded.

When the architecture package is published, add an immutable entry:

| Field | Value |
|---|---|
| Title | |
| Version | |
| Authors/project | |
| Publication date UTC | |
| Git commit | |
| Archive SHA-256 | |
| Primary URL | |
| Independent archive URL | |
| Covered concepts | |
| Corrections/superseding publication | |

<!-- END docs/research/DEFENSIVE_PUBLICATION_LOG.md -->

---

<!-- BEGIN docs/research/NAME_SEARCH_NOTES.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Preliminary name-search notes

**Not a trademark clearance.** Updated 2026-08-05.

Observed prior uses include:

- Atari TOS, historically “The Operating System” for Atari ST systems;
- EmuTOS and FreeMiNT documentation continuing to use TOS descriptively;
- “terminal operating system” abbreviated TOS in logistics and ports;
- unrelated software and commercial services using TOS or TextOS-like strings.

Consequences:

- the project name remains provisional;
- a professional search should cover `TOS`, `TextOS`, visual marks, phonetic variants and relevant software/service classes;
- repository and protocol design must tolerate rename;
- no public claim of exclusive trademark rights is currently made.

<!-- END docs/research/NAME_SEARCH_NOTES.md -->

---

<!-- BEGIN docs/adr/0001-no-mvp-development.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0001: No MVP or throwaway foundation

- Status: Accepted
- Date: 2026-08-05

## Context

TOS combines several difficult ideas. A conventional approach would build a minimal demonstration using shortcuts and replace it later. In operating-system projects, those shortcuts frequently become permanent dependencies or consume the energy required for the real architecture.

## Decision

TOS will not be developed as an MVP. Work is organized as coherent architectural stages. Platform breadth may be intentionally narrow, but interfaces, formats, and trust boundaries implemented within a closed stage are intended to survive.

A project pause is acceptable. A knowingly disposable foundation is not.

## Consequences

- More design work precedes visible demonstrations.
- Early milestones require format specifications, tests, and recovery behavior.
- Agents may not bypass intended subsystems to claim progress.
- Experimental code remains on explicit branches and is not treated as completed architecture.

<!-- END docs/adr/0001-no-mvp-development.md -->

---

<!-- BEGIN docs/adr/0002-canonical-text-source.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0002: Text source is canonical

- Status: Accepted
- Date: 2026-08-05

## Context

The defining idea of TOS is that programs should remain visible as source rather than being replaced by opaque installed binaries.

## Decision

All non-nucleus executable components are canonically stored as human-readable source text. The runtime may generate IR, bytecode, native code, indexes, and snapshots, but these are disposable caches keyed to source identity and runtime inputs.

The nucleus source is also canonical, although a derived binary image is unavoidable for boot.

## Consequences

- Deleting caches must preserve functionality.
- System updates operate on source history.
- Running processes expose source identity.
- Binary-only native packages are not native TOS components.

<!-- END docs/adr/0002-canonical-text-source.md -->

---

<!-- BEGIN docs/adr/0003-microkernel-and-userspace-drivers.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0003: Minimal nucleus and user-space textual drivers

- Status: Accepted
- Date: 2026-08-05

## Context

Executing text drivers directly inside a monolithic kernel would enlarge the trusted base and let ordinary driver bugs corrupt the whole machine.

## Decision

TOS uses a minimal microkernel-like nucleus. Drivers run as isolated textual services by default and receive device-specific capabilities. Boot-critical driver source is delivered through the boot capsule.

## Consequences

- IPC and capability performance are important.
- Driver restart and device reset become standard mechanisms.
- A small number of platform primitives remain in the nucleus.
- Ported driver logic must be adapted to TOS service contracts.

<!-- END docs/adr/0003-microkernel-and-userspace-drivers.md -->

---

<!-- BEGIN docs/adr/0004-git-native-system-history.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0004: Git-native system history

- Status: Accepted
- Date: 2026-08-05

## Context

A text-centric system naturally benefits from source version control. Treating Git only as an external developer tool would miss recovery, audit, branching, merging, and reproducibility benefits during normal operation.

## Decision

The durable `/system` source tree is identified by a repository commit. Boot control selects commits. Updates produce candidate commits. Rollback, merge, clone, push, and bisect are system operations.

The nucleus implements only boot-critical repository verification and traversal. Full Git behavior and remote protocols live in textual services.

## Consequences

- Mutable state must be separated from `/system`.
- Protected refs and retention rules become security mechanisms.
- Commit metadata carries system-specific compatibility and health data.
- Exact Git interoperability requires a dedicated format ADR and conformance suite.

<!-- END docs/adr/0004-git-native-system-history.md -->

---

<!-- BEGIN docs/adr/0005-qemu-x86_64-first.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0005: QEMU x86_64 UEFI is the first platform

- Status: Accepted
- Date: 2026-08-05

## Context

Supporting arbitrary physical hardware would consume the project in driver work before the TOS model is validated.

## Decision

The first platform is x86_64 under QEMU with UEFI, framebuffer/serial diagnostics, and VirtIO devices. Platform interfaces must remain architecture-neutral where feasible, but no unsupported hardware is simulated through fake success paths.

## Consequences

- Tests can be automated deterministically.
- Textual driver architecture is exercised with real virtual devices.
- Physical-hardware support is deferred without weakening core contracts.

<!-- END docs/adr/0005-qemu-x86_64-first.md -->

---

<!-- BEGIN docs/adr/0006-rust-for-binary-foundation.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0006: Rust for loader, nucleus, and host foundation

- Status: Accepted
- Date: 2026-08-05

## Context

The binary foundation requires low-level control, `no_std` support, strong tooling, and reduced accidental memory unsafety.

## Decision

The initial loader, nucleus, shared format libraries, and host tools use stable Rust pinned by repository configuration. The architecture remains language-neutral at external boundaries.

## Consequences

- Unsafe code is isolated and documented.
- Shared parsers can run on host and target.
- Toolchain pinning and reproducible-build work are mandatory.
- Changing the foundation language requires a superseding ADR.

<!-- END docs/adr/0006-rust-for-binary-foundation.md -->

---

<!-- BEGIN docs/adr/0007-licensing-model.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0007: Multi-license model aligned with owner freedom

- Status: Accepted
- Date: 2026-08-05
- Classification: Identity-affecting

## Context

TOS aims to be open in the running machine, not only in an upstream repository. A permissive licence for the whole system would allow a distributor to close modifications and lock the owner out. A custom licence would reduce compatibility and may cease to be accepted open source.

## Decision

- TOS operating-system implementation: `GPL-3.0-or-later`.
- SDK, ABI and reusable interface material explicitly marked: `Apache-2.0`.
- Documentation: `CC-BY-SA-4.0`.
- Documentation code samples: `GPL-3.0-or-later OR Apache-2.0`.
- AGPL may be selected for a future network service only through a component ADR.
- Contributions use DCO 1.1 without mandatory copyright assignment.

## Consequences

Official appliance distributors must evaluate GPLv3 source and Installation Information duties. External applications can use Apache SDK material. Linux GPL-2.0-only source cannot be copied casually into GPLv3 TOS components.

<!-- END docs/adr/0007-licensing-model.md -->

---

<!-- BEGIN docs/adr/0008-architecture-preservation-governance.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0008: Architecture preservation and architect-led governance

- Status: Accepted
- Date: 2026-08-05
- Classification: Identity-affecting

## Context

Agents and conventional engineering practice repeatedly propose mature substitutions that can erase TOS identity while improving local delivery speed.

## Decision

TOS adopts the architecture-preservation policy, change levels and architect-led foundational governance. The Project Architect approves changes to invariants, trust boundaries, canonical-source semantics, owner control and licence architecture.

## Consequences

A technically sound change can be rejected for identity erosion. Narrow scope is preferred over temporary architecture. Forks remain unrestricted under the applicable licences.

<!-- END docs/adr/0008-architecture-preservation-governance.md -->

---

<!-- BEGIN docs/adr/0009-defensive-publication-patent-policy.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0009: Defensive publication and patent-risk management

- Status: Accepted
- Date: 2026-08-05

## Context

TOS combines known mechanisms in a novel architecture. Publishing without a policy can lose useful prior-art evidence, while claiming patent safety without claim analysis is irresponsible.

## Decision

The project does not seek core software patents by default. It publishes enabling architecture packages as defensive publications, maintains a patent landscape, reviews high-risk mechanisms and requires professional FTO review before material commercial distribution.

## Consequences

Publication may restrict the project’s own patent options. Patent notes are risk records, not legal conclusions. No custom patent restrictions are added to the software licence.

<!-- END docs/adr/0009-defensive-publication-patent-policy.md -->

---

<!-- BEGIN docs/adr/0010-derived-artifact-provenance.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0010: Derived artifacts carry verifiable source provenance

- Status: Accepted
- Date: 2026-08-05

## Context

TOS permits boot images, bytecode and native caches, but they must not become anonymous canonical programs.

## Decision

Every executable derivative records canonical source hashes, source commit, runtime/compiler identity, target ABI, build/material digests and artifact digest. Boot capsules are reproducibly derived from repository inputs and name their source commit.

## Consequences

Artifact formats include provenance from version 1. Cache validation is part of runtime correctness. A binary without a valid source relationship is not an official TOS textual component.

<!-- END docs/adr/0010-derived-artifact-provenance.md -->

---

<!-- BEGIN docs/adr/0011-external-implementations-as-oracles.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0011: External implementations default to references or test oracles

- Status: Accepted
- Date: 2026-08-05

## Context

Existing Git libraries, language runtimes and driver frameworks can accelerate work but may import incompatible trust, source and capability models.

## Decision

External implementations default to specification references, host tools or test oracles. Admission as an isolated runtime service or trusted-base dependency requires a separate ADR, licence review, transitive-dependency audit and architecture impact statement.

## Consequences

libgit2, command-line Git, Lua, Wasm engines and Linux driver implementations are not silently adopted into the nucleus or canonical runtime.

<!-- END docs/adr/0011-external-implementations-as-oracles.md -->

---

<!-- BEGIN docs/adr/0012-dco-without-copyright-assignment.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0012: DCO contribution model without mandatory copyright assignment

- Status: Accepted
- Date: 2026-08-05

## Context

The project needs traceable contribution rights without centralizing the ability to relicense community work proprietarily.

## Decision

All contributions require Developer Certificate of Origin 1.1 sign-off. Copyright remains with contributors. No mandatory copyright assignment or broad CLA is required.

## Consequences

Relicensing community-owned files may require many permissions. This is intentional protection against unilateral closure.

<!-- END docs/adr/0012-dco-without-copyright-assignment.md -->

---

<!-- BEGIN docs/adr/0013-project-name-provisional.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0013: TOS — TextOS remains a provisional name

- Status: Accepted
- Date: 2026-08-05

## Context

TOS has historic and current uses, including Atari TOS and the generic logistics abbreviation “terminal operating system.” A casual web search is not trademark clearance.

## Decision

Use `TOS — TextOS` as the working identity while keeping technical namespaces rename-ready. Do not claim exclusive trademark rights until professional clearance and a later ADR.

## Consequences

Public branding stays modest. Protocol identity does not rely solely on the project letters. A rename remains possible without redesigning the architecture.

<!-- END docs/adr/0013-project-name-provisional.md -->

---

<!-- BEGIN docs/adr/0014-driver-source-reuse-and-clean-room.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0014: Driver knowledge reuse without incompatible source copying

- Status: Accepted
- Date: 2026-08-05

## Context

TOS intends to learn from open drivers. The Linux kernel is generally GPL-2.0-only, while official TOS implementation is GPL-3.0-or-later. The licences are not compatible for combining copied implementation code in one work. In addition, an existing driver contains substantial operating-system integration that TOS should not inherit architecturally.

## Decision

TOS driver work distinguishes public hardware facts from expressive implementation. Preferred sources are public specifications, permissively licensed code, GPL-2.0-or-later files and independently written functional descriptions. GPL-2.0-only source may be studied for behavior and specification references, but code is not copied into GPLv3 TOS components without a specific legal basis.

Where necessary, use a documented clean-room functional reimplementation process.

## Consequences

Porting is slower than mechanical translation but avoids licence conflict and Linux-specific architecture leakage. Provenance records are mandatory for register tables, firmware and adapted source.

<!-- END docs/adr/0014-driver-source-reuse-and-clean-room.md -->

---

<!-- BEGIN docs/adr/0015-language-foundation-decision-gate.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0015: Require a language-foundation decision gate before Stage 2

- Status: Accepted
- Date: 2026-08-06
- Decision level: 3 — architectural process and trusted-runtime boundary

## Context

TOS depends on a language/runtime relationship that ordinary embedded scripting languages do not automatically provide: canonical text, deterministic lowering, capability-aware types, bounded bootstrap execution, source maps, independent IR verification and suitability for user-space drivers.

The current documents name this role “TOS Core” and show illustrative syntax, but they do not yet define a normative grammar, semantics or memory model. Beginning parser implementation immediately would turn accidental early choices into architecture. Conversely, embedding a mature language for convenience could erase TOS identity while appearing pragmatic.

## Decision

A mandatory **Stage 1.5 — Language foundation decision** occurs after the trusted boot boundary is established and before Stage 2 parser/runtime implementation begins.

Stage 1.5 must:

1. use `docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md`;
2. compare at least a bespoke TOS Core option, a TOS surface over an existing formal core, and one adapted existing-language option;
3. produce executable or formal evidence for the required prototype exercises;
4. measure trusted-base, dependency, performance and recovery impact;
5. identify the canonical source, verifier boundary and host ABI exposure for every candidate;
6. end in a separate accepted ADR selecting the language foundation.

Until that selection ADR is accepted:

- `.tos` syntax remains illustrative;
- no parser implementation may be declared normative;
- no existing runtime may enter the trusted base as a temporary shortcut;
- Wasm or another bytecode may be researched as a backend, but cannot become canonical source by convenience.

## Consequences

Positive:

- the largest conceptual dependency is decided with evidence;
- a bespoke language is not assumed merely for originality;
- mature runtimes are evaluated without allowing architectural capture;
- Stage 2 begins with a stable contract rather than syntax experimentation.

Negative:

- Stage 2 starts later;
- comparison prototypes create work that may be discarded;
- the decision may reveal that earlier IR assumptions need revision.

The additional work is accepted because language-foundation mistakes would contaminate every later subsystem.

<!-- END docs/adr/0015-language-foundation-decision-gate.md -->

---

<!-- BEGIN docs/adr/0016-capsule-git-raw-oid-identity.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0016: Capsule git identity carries the raw object id

- Status: Accepted
- Date: 2026-08-06

## Context

ADR-0010 requires every derived artifact to name its source commit. The
first capsule v1 draft stored `source_identity_digest = SHA-256(raw oid)`
for `SRC_KIND_GIT`. That binding is one-way: given only the digest, the
original git object id cannot be recovered, so the capsule cannot be
resolved back to a commit without external state. A capsule is meant to be
self-describing; a non-invertible commit reference defeats that.

## Decision

For `source_identity_kind = SRC_KIND_GIT (1)`, the 40-byte identity region
of the capsule header is:

| offset | size | field |
| ------ | ---- | ----- |
| 96     | 1    | `source_identity_kind` = 1 (git) |
| 97     | 1    | `source_oid_alg` = 1 (SHA-1) or 2 (SHA-256) |
| 98     | 1    | `source_oid_length` = 20 or 32 |
| 99     | 1    | reserved (zero) |
| 100    | 32   | `source_identity_value`: raw git object id (20 or 32 bytes, left-aligned, zero-padded) |
| 132    | 4    | reserved (zero) |

The raw object id is stored, not a digest of it. `source_oid_alg` and
`source_oid_length` make the value self-describing for both SHA-1 (20-byte)
and SHA-256 (32-byte) repositories. BootInfo mirrors the same triple
(`capsule_identity_kind` at 136, `capsule_oid_alg` at 137,
`capsule_oid_length` at 138, 5 reserved bytes, `capsule_source_identity`
32 bytes at 144).

For `source_identity_kind = SRC_KIND_DETACHED (2)` (a source set without a
repository), the same 40-byte region holds `alg = 0`, `length = 0`, and the
32-byte source-set digest in `source_identity_value`; no OID is present.

## Consequences

- A git-bound capsule can be resolved to its exact commit: read
  `source_oid_alg`/`source_oid_length`, then take the left-aligned bytes of
  `source_identity_value` as the object id and run `git show <oid>`.
- `capsule v1` is not yet declared final; this region is explicitly
  reserved for the raw OID before acceptance. The former
  `sha256(oid)` binding is not used anywhere in the released format.
- Parsers reject inconsistent triples (git kind without a valid
  alg/length pair, detached kind with a non-zero algorithm or length).

<!-- END docs/adr/0016-capsule-git-raw-oid-identity.md -->

---

<!-- BEGIN docs/adr/0017-capsule-v1-canonical-layout.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0017: Capsule v1 canonical layout — packed arena, canonical index mapping, byte compatibility preserved

- Status: Accepted (owner-approved)
- Date: 2026-08-06
- Change level: **Level 2** (contract extension under
  `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`) — adds reject conditions to an
  existing versioned format without amending an invariant. Explicitly **not**
  Level 3: no capsule byte changes.

## Context

An implementation audit of capsule v1 found three defects in the contract, none
of which the reference builder actually exercises:

1. **Alignment claim without an implementation.** §2 declared `ALIGNMENT = 8`
   the "required alignment of offsets and entry sizes", but a real capsule does
   not satisfy it: the name arena has an arbitrary byte length, so in
   `valid-001.bin` `file_table_offset` is 252. Either the parser had to start
   rejecting every capsule ever built, or the builder had to pad — and §4.2
   already states that `content_offset` need not be aligned, contradicting the
   §2 wording.

2. **Undescribed bytes were accepted.** The parser bounded each name inside the
   arena but never required the names to cover it, and required only
   `path_table_offset >= HEADER_SIZE`. A capsule carrying 64 arbitrary bytes in
   the arena (or between the header and the path table) parsed as valid. That
   contradicts §1 ("deterministic, immutable, read-only archive") and I-10
   (deterministic identity): for one file set, many distinct "valid" capsules
   existed, and the extra bytes travelled to the nucleus unvalidated.

3. **Quadratic bijection check on the boot path.** §4.1 requires the path table
   to be a bijection onto `[0, file_count)`. It was verified by counting
   references for every file — O(n²). Measured in a release build: 20 001 files
   (1.7 MB) took 3.55 s, against the `docs/35_PERFORMANCE_CONTRACTS.md` Stage 1
   budget of 250 ms p95 for 1 000 files / 16 MiB. A 16 MiB capsule admits
   ~200 000 entries.

## Decision

### 1. Alignment: clarify the contract, do not pad (byte compatibility preserved)

`ALIGNMENT = 8` is normative for the header and the fixed entry sizes only:

- `HEADER_SIZE` (184), `PATH_ENTRY_SIZE` (16) and `FILE_ENTRY_SIZE` (64) are
  multiples of 8;
- `path_table_offset == HEADER_SIZE`, and is therefore 8-aligned;
- `file_table_offset`, `payload_offset` and `content_offset` are **not** required
  to be multiples of 8;
- implementations must not assume aligned struct access anywhere in the capsule
  and must decode every field byte-wise (little-endian), as the reference parser
  already does.

The alternative — making the builder pad the name arena so every table starts on
an 8-byte boundary — is **rejected**. It is a Level 3 change to a persistent
format: it would alter the bytes of every capsule v1, invalidate every
`capsule_sha256`, every committed golden vector and every digest recorded in
archived Stage 1 evidence. The gain is unproven: the parser decodes fields
byte-wise, so aligned tables buy nothing today, and a future implementation that
wants aligned access can request it in a future format version.

### 2. The name arena is strictly packed

For a path table of `n` entries in table order:

- `path_entry[0].name_offset == 0`;
- `path_entry[i].name_offset == path_entry[i-1].name_offset + path_entry[i-1].name_length`;
- the end of the last name equals `file_table_offset`.

No undescribed byte may exist between the header and the path table, between
names, or between the last name and the file table.

### 3. Canonical index mapping replaces the reference count

The bijection required by §4.1 is realised canonically:

- `path_table_count == file_count`;
- `path_entry[i].file_index == i` for every `i`;
- the file table and the payload therefore follow the same order as the
  name-sorted path table.

The official builder already emits exactly this layout, so no existing correct
capsule needs its data moved; the rule only forbids non-canonical permutations
of the index field. The check becomes a single O(n) pass and stays
allocation-free and `no_std`.

A structured error distinguishes a non-canonical mapping, and a negative golden
vector with permuted/repeated `file_index` values pins the behaviour.

## Consequences

- Capsule v1 stays byte-compatible: every committed vector, every recorded
  `capsule_sha256` and the archived QEMU evidence remain valid. The format
  version is not incremented.
- Capsules that were only accepted because of undescribed bytes or a
  non-canonical index permutation are now rejected. No such capsule was ever
  produced by an official builder.
- The deterministic-archive property becomes real: for a given file set, licence
  notice and identity there is exactly one valid capsule v1 byte string.
- The O(n²) validation path disappears without introducing a maximum-size rule.
  A maximum-size bound remains an open question (CODEX_START asks for one) and
  is deliberately **not** decided here.
- Third-party builders must emit the canonical mapping; they could previously
  emit any permutation.

## Compliance

- Invariants: none amended. Strengthens I-10 (deterministic identity) and I-18
  (derived-artifact provenance).
- Trusted base: unchanged; no dependency added. The parser loses code rather
  than gaining it.
- Recovery/rollback: unaffected — no capsule is invalidated.
- Threat model: closes an undescribed-bytes channel into the nucleus
  (`docs/34_THREAT_MODEL.md`: all external bytes are hostile until validated).
- Performance: removes the quadratic term measured against
  `docs/35_PERFORMANCE_CONTRACTS.md` §Stage 1.
- Tests: parser unit tests per rule, negative golden vectors, and the existing
  tamper/truncation suites.

<!-- END docs/adr/0017-capsule-v1-canonical-layout.md -->

---

<!-- BEGIN docs/adr/0018-detached-capsule-source-identity.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0018: Detached capsule source identity

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 3** — source-identity semantics and existing detached
  capsule bytes change
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

ADR-0016 defines the header shape for a detached source set but deliberately
does not define the derivation of its 32-byte value. The lower-tier interface
draft says the value is derived from file content digests, but a content-only
sequence does not bind canonical source paths. The current official detached
golden fixture and builder instead use caller-supplied synthetic bytes (`0x42`
repeated). This means a currently accepted detached capsule does not prove a
source-set identity.

`source/interfaces/boot/CAPSULE_FORMAT_V1.md` remains unassigned in the
authority hierarchy (F-08). It is evidence of the intended contract, not an
independent authority for this decision. This accepted ADR is the Tier 1
decision for detached-source-set identity.

## Proposed decision

### Canonical detached identity

For a capsule whose `source_identity_kind = SRC_KIND_DETACHED (2)`, let
`p_i` be the exact canonical UTF-8 path bytes from path-table entry `i`, and
let `d_i` be the exact 32-byte `content_digest` from file-table entry `i`, for
`i = 0 .. file_count - 1`. ADR-0017's canonical index mapping makes each
path-table entry `i` refer to file-table entry `i`; that shared canonical
path/file-table order is material.

The fixed versioned domain separator is the 11-byte sequence:

```text
DOMAIN = b"TOS.DSI.v1\0"
       = 54 4f 53 2e 44 53 49 2e 76 31 00
```

The identity is exactly:

```text
source_identity_value = SHA-256(
    DOMAIN ||
    for i = 0 .. file_count - 1:
        u32_le(len(p_i)) || p_i || d_i
)
```

The domain separator prevents an implicit claim of compatibility with another
SHA-256 construction over coincidentally similar bytes. Each path length has a
fixed four-byte little-endian encoding; each `d_i` is fixed at 32 bytes. These
length-delimited entries, consumed to the end of the encoded input, already
give a unique sequence boundary, so `file_count` is not included redundantly.
The parser already knows the count from the validated header and iterates that
many entries; adding it to the digest input would add no source-set binding.

The identity is not replaced by file-table ordering alone, raw file-content
concatenation, a Merkle tree, a different path encoding, or an
implementation-defined domain string. Capsule v1 already constrains the
path/file-table order and canonical paths; this formula binds both paths and
their validated contents to that representation.

For zero entries, the mathematical value of this domain-separated formula is
`SHA-256(DOMAIN)`. Capsule v1 nevertheless continues to reject `file_count =
0`, and the official builder continues to reject an empty file set. Thus no
valid v1 capsule emits or accepts a zero-entry detached identity; this
statement defines the function without relaxing the independent zero-file rule.

### Builder and parser obligations

After acceptance, the builder MUST compute the detached identity itself from
the specified canonical path/digest encoding. It MUST NOT accept a
caller-selected detached value, including an all-zero, synthetic, precomputed
or environment-injected substitute. This does not change ADR-0016 Git input: a
Git-bound capsule still accepts its explicit raw object identifier.

After it has validated every canonical path, file content digest and canonical
path/file-table layout, the parser MUST independently recompute the same
detached identity and reject a disagreement with a dedicated structured
`CapsError::DetachedIdentityMismatch`. The loader must serialize that error and
fail closed with `RESULT_CAPSULE_INVALID` before it can hand an invalid capsule
to the nucleus. The nucleus retains its existing BootInfo-to-header mirror
check; it is not a substitute for parser validation.

### Compatibility and migration

This is intentionally not byte-compatible for existing detached artifacts.
The current `valid-001.bin` contains the synthetic `0x42` value whereas the
proposal-only calculation for its two canonical paths and file digests is:

```text
b07b6e58e9e3aa9716d4ad779529a2e7be6522aef1f3e67a16230e04a55c8c05
```

Once accepted and implemented, detached capsules produced under the synthetic
convention are invalid and must be regenerated from their canonical inputs.
Git-bound capsules and their ADR-0016 raw-OID representation are unchanged.
Migration must regenerate all affected fixture bytes, SHA-256 records,
capsule metadata and Stage 1 evidence only after the separate F-22 vector
provenance/licensing decision supplies an authoritative record for every
generated binary. There is no silent compatibility fallback and no v1 version
bump in this proposal: acceptance explicitly chooses the Level 3 source
identity correction rather than treating invented fixture bytes as a compatible
identity.

## Architecture impact statement

- **Invariants:** I-10 deterministic identity and I-18 derived-artifact
  provenance are strengthened; no invariant is amended.
- **Canonical representation:** a detached header value is the stated SHA-256
  of the domain-separated, length-delimited canonical path plus 32-byte
  content-digest sequence, never an externally supplied label.
- **Trusted base:** the existing capsule builder/parser gain only a streaming
  SHA-256 calculation using the in-tree hash crate; the loader/nucleus gain no
  dependency or trust boundary.
- **Source-to-runtime:** the capsule identity now commits to exactly the
  canonical paths and file content digests that the parser validates, and the
  existing header-to-BootInfo-to-nucleus evidence remains the runtime mirror.
- **Derived artifacts:** a detached capsule remains disposable and can be
  regenerated from canonical source inputs, file-table order, builder version
  and the declared identity formula. The generated-artifact provenance record
  remains required by `docs/28` and F-22.
- **Recovery and rollback:** no Git, ESP, loader or recovery path changes.
  A previous Git-bound commit remains bootable as before; a detached recovery
  artifact must retain enough inputs and provenance to regenerate its identity.
- **Hidden host dependency:** none is introduced. SHA-256 is already in the
  TOS trusted code; no host Git command, network service or external runtime is
  consulted by the parser.
- **Threat model:** a malicious detached header can no longer claim arbitrary
  source-set provenance after valid file digests have been checked. Parsing
  stays total, bounded and fail-closed over hostile bytes.
- **Performance:** one additional four-byte length, path-byte and 32-byte
  digest update per file is required. The accepted implementation must measure
  the existing Stage 1 parser workload and show the `docs/35` p95 contract
  remains satisfied; no performance claim is made by this proposal.
- **Compatibility profile:** Stage 1/G0 scope remains unchanged. Only the
  untrusted synthetic detached-fixture convention is intentionally retired;
  no Git compatibility profile changes.
- **Dependencies:** none; `tos_hash::Sha256` is already an in-tree dependency.
- **Licence and patent:** no imported code or new licence class is proposed.
  Fixture/container provenance is deliberately deferred to F-22. This ADR
  makes no patent-freedom claim.
- **Tests after acceptance:** RED/GREEN builder-computation and parser-mismatch
  tests, including equal-content/different-path rejection; zero-file rejection
  remains; deterministic rebuild with regenerated vectors; host/integration/
  QEMU negative evidence; source-to-runtime identity report; performance
  measurement; and provenance verification for every regenerated fixture.

## Rejected alternatives

- Preserve arbitrary caller-provided detached values: rejected because the
  header then does not identify its source set.
- Hash file contents directly: rejected because the format already commits to
  per-file digests and this would define a different canonical representation.
- Treat canonical ordering as a substitute for path binding: rejected because
  two distinct path sets can preserve lexical order while carrying identical
  ordered contents. Exact canonical path bytes are therefore encoded above.
- Use a Merkle tree or add a version field: rejected because neither is needed
  to resolve this existing Stage 1 identity defect and either changes more of
  capsule v1 than the proposed formula.
- Accept an empty capsule with `SHA-256(DOMAIN)`: rejected because it weakens
  the existing zero-file validation rule.

## Implementation boundary

This accepted ADR authorizes the corresponding detached builder/parser work and
ephemeral test evidence. It does not authorize tracked binary-vector
regeneration or a generated-binary container classification before F-22 has an
accepted provenance/licensing decision. It also does not close Stage 1, start
Phase 2 or start Stage 1.5.

<!-- END docs/adr/0018-detached-capsule-source-identity.md -->

---

<!-- BEGIN docs/adr/0019-capsule-vector-provenance.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0019: Capsule vector provenance and mixed-material containers

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — extends the versioned provenance/release contract
  and its gate without changing a runtime ABI, capsule byte format, trusted
  base or active invariant
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The committed capsule-v1 `.bin` fixtures are generated artifacts. The valid
fixture embeds official GPL-3.0-or-later boot source and a licence notice, while
the fixture set may also contain materials from other licence classes. The
existing SPDX gate exempts all `source/tests/vectors/capsule-v1/*.bin` files
because they are indexed by `vectors.tsv`; that proves neither a licence choice
nor provenance for the binary.

ADR-0010 and the release/provenance policy require a derived artifact to retain
its source relationship. `LICENSE.md` permits reusable test vectors under
Apache-2.0 only when explicitly marked, and requires generated artifacts to
record the licences of their canonical sources without removing notices. No
existing authority selects a single SPDX expression for a mixed-material binary
container. ADR-0018 also requires regenerated detached fixtures to wait for an
accepted F-22 provenance treatment.

## Decision

### Mixed-material generated artifact classification

A capsule-v1 binary fixture that contains materials from different licence
classes is classified as:

```text
mixed-material-generated
```

This is an artifact/provenance classification, **not** an SPDX licence
identifier or expression. Such a `.bin` MUST NOT be assigned one container-wide
SPDX expression merely from its path, generator, an extension exemption or the
licence of one embedded input. It does not assert that the container is an
Apache-2.0, GPL-3.0-or-later, or otherwise homogeneous work.

The authoritative machine-readable representation is:

```json
"container_licensing": {
  "status": "mixed-material-generated",
  "spdx_expression": null
}
```

`mixed-material-generated` MUST NOT appear in a field that expects a valid SPDX
expression. The absence of a container-level SPDX expression does not remove
any obligation attached to embedded materials.

### Required provenance record and gate

Every tracked capsule-v1 fixture MUST have a valid entry in one
machine-verifiable provenance manifest. For each canonical or generated input,
the entry MUST state its digest, role and applicable SPDX identifier. It MUST
also state the fixture filename/output digest, generated-artifact status,
generator identity/version and generator licence as generator provenance.
Generator licensing is not automatically inherited by the output artifact.

Embedded licence notices MUST be retained in the fixture and listed with a
separate notice role. A derived invalid fixture MUST additionally identify its
base vector, base digest and deterministic transformation recipe.

The SPDX/provenance gate MUST reject a tracked capsule-v1 `.bin` that lacks a
valid provenance entry. The existing broad `*.bin` exemption is therefore not
an adequate final gate and must be replaced by the manifest validation when the
record is introduced.

If exact historic provenance of an existing binary cannot be demonstrated, the
record MUST mark it with an explicitly defined `unverifiable-legacy` status; it
MUST NOT invent a source commit, material digest or generator claim. Such an
artifact must be replaced reproducibly from a known source commit before it is
used as current Stage 1 conformance evidence.

### Reusable synthetic vectors

An Apache-2.0 reusable synthetic conformance-vector class may be introduced in
the future only as a separate, explicitly designated class containing
Apache-eligible synthetic materials. That future class does not reclassify the
current boot-material fixtures and is not required by this ADR.

## Consequences

- The vector provenance manifest and its checker become required Stage 1
  evidence before tracked binary-vector regeneration, including ADR-0018's
  affected detached fixtures and the SHA-1-padding negative fixture.
- The project records source/material obligations truthfully without assigning
  a false blanket licence to a mixed-material container.
- Existing fixture documentation, outcome tables and gate comments must be
  reconciled with this decision; their own text-file SPDX identifiers remain
  independent of the container classification.
- A binary whose provenance is only historical inference is not silently
  upgraded to verified evidence.

## Architecture impact statement

- **Invariants and canonical representation:** no invariant changes. The
  canonical executable source remains textual; a fixture stays a disposable
  derivative with a mandatory source/material record.
- **Trusted base and source-to-runtime:** no runtime code, dependency, ABI or
  loader/nucleus trust boundary changes. The build/release evidence becomes
  more explicit.
- **Recovery and owner control:** no recovery, rollback or owner boot path
  changes; a fixture can be discarded and regenerated from recorded inputs.
- **Compatibility:** capsule v1 bytes and semantics are unchanged by this ADR.
  A later regeneration changes only those bytes required by independently
  accepted ADR-0018 or a documented vector recipe.
- **Threat, performance, licence and patent:** the gate prevents provenance
  concealment but adds no new runtime attack surface or measured path. It
  preserves licence notices and makes no patent-freedom claim.
- **Tests:** a deterministic manifest verifier must cover valid entries,
  missing entries, digest mismatches, missing input SPDX/notice roles,
  malformed mixed-material classification and derived-vector base/recipe
  requirements.

<!-- END docs/adr/0019-capsule-vector-provenance.md -->

---

<!-- BEGIN docs/adr/0020-versioned-interface-contract-authority.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0020: Versioned interface-contract authority and Boot ABI v1 events

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — accepts existing versioned public contracts and
  diagnostic vocabulary without changing a Tier 0 invariant, runtime trust
  boundary, persistent byte layout or implementation behavior
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The boot ABI and capsule format have implemented, versioned byte contracts and
conformance evidence, but their files live under `source/interfaces/`, outside
the classes assigned a tier by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.
Their old self-description as both “proposed” and “normative” was therefore not
an authority source (F-08). The same unassigned Boot ABI draft named diagnostic
events that the loader, nucleus and QEMU harness do not emit (F-13).

`docs/17_REPOSITORY_LAYOUT.md` already assigns independent interface
definitions and conformance vectors to `interfaces/`. The project's real
success and failure traces have been QEMU-checked since Stage 1 implementation;
changing them merely to preserve unused draft spellings would break that
evidence without strengthening an invariant.

## Decision

### Authority admission rule

`source/interfaces/**` gains Tier 2 authority only for a **versioned interface
contract** that satisfies every condition below:

1. it has the exact explicit status `Accepted Tier 2 interface contract`;
2. it is listed in `docs/SPECIFICATION_SOURCES.txt` and therefore carried in
   the generated review view;
3. it explicitly refers to `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`; and
4. it states and observes its subordination to Tier 0 invariants and accepted
   Tier 1 ADRs.

Inclusion in `docs/SPECIFICATION_SOURCES.txt` alone does not assign Tier 2
authority to any other listed material. A new versioned source-interface
contract must meet all four conditions before it is normative; a prose report,
fixture, implementation note or other source-interface file is not promoted by
directory placement or generated-spec inclusion.

`BOOT_ABI_V1.md` and `CAPSULE_FORMAT_V1.md` meet this rule when this ADR is
implemented. They are Tier 2 contracts, subordinate to the invariant set and
to ADR-0016 through ADR-0019 where those ADRs decide their subject matter.
Future incompatible versions use a new versioned contract and the normal
Tier 1/2 conflict protocol; a contract cannot supersede an accepted ADR by
self-description.

### Boot ABI v1 event vocabulary

The current emitted identifiers are the canonical stable Boot ABI v1 serial
event vocabulary. The success order is exactly:

```text
TOS.BOOT.ENTRY
TOS.CAPSULE.OK          # loader validation
TOS.BOOT.HANDOFF
TOS.NUCLEUS.ENTRY
TOS.CAPSULE.OK          # nucleus independent validation
TOS.BOOTTEXT.PATH
[TOS.BOOTTEXT.LINE]     # optional
TOS.BOOTTEXT.DIGEST
TOS.IDENTITY
TOS.HALT
```

The stable failure vocabulary is:

```text
TOS.BOOT.FAILC
TOS.BOOT.FAILI
TOS.ABI.FAIL
TOS.MEM.FAIL
TOS.CAPSULE.FAIL
TOS.IDENTITY.MISMATCH
TOS.PANIC
```

`TOS.BOOT.FAILI` is itself stable. Existing reason tokens retain their defined
meaning; Boot ABI v1 may add a new reason token but must not repurpose an
existing one. Mandatory structured fields and raw payload grammars are fixed
in `BOOT_ABI_V1.md` §7. An implementation may append optional `key=value`
fields to an event only after its mandatory fields, so a parser that accepts
the v1 mandatory prefix remains compatible.

`TOS.IDENTITY` fixes the exact required field spellings and semantics:
`source_kind=`, `source_digest=`, `capsule_digest=`, `arch=`, and `builder=`.
The previous unimplemented draft names `TOS.CAPSULE.VALID`,
`TOS.SOURCE.INIT_FOUND` and `TOS.BOOT.HALT_OK` are not Boot ABI v1 events.

## Consequences

- The public ABI is now discovered through the same hierarchy as other Tier 2
  contracts, while retaining Tier 0 and Tier 1 precedence.
- The loader, nucleus and QEMU harness retain their emitted behavior; this
  ADR changes the contract to the verified implementation rather than creating
  a compatibility-breaking rename.
- A machine-checkable authority admission test and event-contract test guard
  both documents, required fields, success cardinality/order and QEMU gate
  wiring. Full preflight retains real QEMU success and negative execution.

## Architecture impact statement

- **Invariants and canonical representation:** I-09 versioned boundaries is
  made enforceable; no invariant or capsule/BootInfo byte representation
  changes.
- **Trusted base and source-to-runtime:** no loader, nucleus dependency,
  privilege or source-identity behavior changes.
- **Recovery, rollback and compatibility:** no boot-control or recovery path
  changes; existing Boot ABI v1 QEMU consumers retain their identifiers and
  mandatory field prefixes.
- **Threat and performance:** stable fail-closed identifiers improve audit
  evidence without adding an input path, runtime work or performance budget.
- **Licence and patent:** interface contracts remain Apache-2.0 as declared;
  no imported code, licence boundary or patent claim changes.
- **Tests:** authority admission, static event conformance, QEMU success and
  negative-suite evidence are required before Stage 1 closure can rely on the
  contracts.

<!-- END docs/adr/0020-versioned-interface-contract-authority.md -->

---

<!-- BEGIN docs/adr/0021-capsule-v1-resource-bounds.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0021: Capsule v1 resource bounds

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — adds bounded validation to the existing capsule
  v1 contract without changing its byte layout, source-identity semantics,
  trusted-base role or Tier 0 invariants
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

Capsule v1 parsing is total and uses checked layout arithmetic, but it had no
accepted resource maxima. A hostile capsule could therefore drive excessive
UEFI allocation, table traversal, path processing or hashing before its
structural invalidity was known. The Stage 1 performance contract also needs a
bounded accepted workload rather than only an informal fixture size.

The capsule remains a disposable, source-bearing transport and recovery seed;
it does not become canonical installed state. The parser remains `no_std`,
allocation-free and borrowed-slice based.

## Decision

Capsule v1 has these inclusive resource limits. `KiB = 1024` bytes and
`MiB = 1024 * 1024` bytes.

| Constant | Inclusive maximum |
|---|---:|
| `MAX_CAPSULE_BYTES` | 32 MiB |
| `MAX_FILE_COUNT` | 4096 |
| `MAX_PATH_BYTES` | 1024 |
| `MAX_NAME_ARENA_BYTES` | 1 MiB |
| `MAX_LICENCE_NOTICE_BYTES` | 64 KiB |

The limits apply jointly: satisfying one does not weaken another.

The UEFI loader MUST obtain the capsule file length from EFI metadata and
reject a value greater than `MAX_CAPSULE_BYTES` before allocating a pool buffer
or reading the complete capsule. The parser MUST apply gross input, declared
length, count and notice limits before identity validation, table walking or
payload hashing where the layout permits. It MUST check an individual path
length before UTF-8 or canonical-path processing. The builder MUST apply the
same limits with checked conversions and MUST NOT truncate fields silently.

The parser returns these stable structured errors for the corresponding limits:
`CapsuleTooLarge`, `FileCountTooLarge`, `PathTooLong`,
`NameArenaTooLarge` and `LicenceNoticeTooLarge`.

Validation precedence is deterministic:

1. too-short input is rejected before decoding;
2. physical input length greater than `MAX_CAPSULE_BYTES` is rejected before
   header decoding or any traversal;
3. after fixed header magic/UUID/version/size/alignment validation, declared
   total length, then table counts, then licence-notice length are checked in
   that order;
4. after checked table geometry establishes the name-arena bounds, the arena
   limit is checked before path-table iteration;
5. each path length is checked before decoding or canonical-path semantics.

An accepted capsule permits at most two linear hash traversals of its bytes:
one whole-capsule SHA-256 traversal and one cumulative traversal for per-file
content SHA-256 values. Detached source identity consumes already validated
paths and content digests; it MUST NOT hash file contents again.

## Consequences

- A currently accepted 1,000-file / 16 MiB performance workload remains below
  all maxima and remains required performance evidence.
- Resource-boundary QEMU inputs are generated deterministically below
  `target/`; no large tracked binary fixture is needed.
- Existing capsule bytes at or below all limits stay format-compatible. Bytes
  outside an accepted maximum are now rejected fail-closed.

## Architecture impact statement

- **Invariants and canonical representation:** I-01, I-02, I-09, I-10 and
  I-18 are preserved. Capsule v1 remains a bounded derived transport artifact;
  no canonical source or byte layout changes.
- **Trusted base and source-to-runtime:** loader and parser gain only
  fail-closed checks over already trusted-base inputs. No dependency, privilege
  or source identity enters the trusted base.
- **Recovery and rollback:** no selection, rollback or owner boot mechanism
  changes. An oversized recovery seed now fails explicitly rather than being
  materialized without a Stage 1 bound.
- **Threat and performance:** this addresses hostile size/count/path inputs
  before expensive processing and bounds accepted hashing to two linear
  traversals. It preserves the Stage 1 1,000-file / 16 MiB workload.
- **Compatibility, licence and patent:** the capsule format version and byte
  representation do not change; previously oversized development artifacts
  are not accepted v1 artifacts. No licence boundary, imported dependency or
  patent claim changes.
- **Tests:** parser and builder prove every maximum and maximum-plus-one
  boundary, deterministic error precedence and parity; deterministic fuzzing
  remains required; QEMU proves the loader rejects 32 MiB + 1 before handoff.

<!-- END docs/adr/0021-capsule-v1-resource-bounds.md -->

---

<!-- BEGIN docs/adr/0022-bootinfo-v1-platform-handoff.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0022: BootInfo v1 platform handoff

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — defines existing BootInfo v1 fields without
  changing its layout, offsets, major/minor version or source identity
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Decision

`FB_FORMAT_NONE=0`, `FB_FORMAT_RGBX8=1` and `FB_FORMAT_BGRX8=2`. RGBX8 and
BGRX8 name increasing framebuffer-memory byte order; X is ignored. GOP
`PixelRedGreenBlueReserved8BitPerColor` maps to RGBX8 and
`PixelBlueGreenRedReserved8BitPerColor` maps to BGRX8. PixelBitMask and
PixelBltOnly fail closed. Pitch is bytes per scanline, exactly checked
`PixelsPerScanLine * 4` for the supported formats.

Framebuffer is absent only when GOP is absent, in which case its complete tuple
is zero/`NONE`. A present but malformed, unbacked, unsupported, zero-sized or
overflowing GOP mode fails closed. The loader validates base, geometry, minimum
pitch, required bytes and base-plus-size; it reserves the handed-off range from
ordinary usable memory.

`acpi_rsdp` is the selected physical RSDP: ACPI 2.0+ configuration table is
preferred, ACPI 1.0 is used only when it is absent. `smbios` is the selected
physical SMBIOS entry point: SMBIOS 3 is preferred, SMBIOS 2 is used only when
it is absent. A malformed preferred entry fails closed rather than falling back.
The loader checks anchors, declared lengths and checksums (including ACPI v1
and extended v2 checksums and SMBIOS 2 intermediate anchor/checksum). Zero
means absent configuration table, not an ignored malformed one.

The pointers remain physical and valid at handoff after ExitBootServices.
Consumers validate firmware-owned structures before use and do not reclaim them
until copied/read. This does not claim protection from malicious firmware (T7).

## Architecture impact statement

No Tier 0 invariant, trusted dependency, capsule format, recovery path or
BootInfo byte layout changes. This makes the existing platform fields concrete,
fail-closed and observable through optional `TOS.BOOT.HANDOFF` fields. Tests
cover format/geometry and configuration-table selection; QEMU proves real
OVMF values reach the existing handoff path.

<!-- END docs/adr/0022-bootinfo-v1-platform-handoff.md -->

---

<!-- BEGIN docs/adr/0023-stage1-exception-baseline.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0023: Stage 1 exception baseline

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — additive Boot ABI v1 terminal failure contract;
  BootInfo layout and loader-to-nucleus calling convention remain unchanged
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Decision

Before it reads or trusts BootInfo-controlled memory, the x86_64 nucleus
installs a nucleus-owned GDT/TSS foundation and an IDT for every architected
CPU exception vector 0 through 31. Entries are present 64-bit interrupt gates
at DPL 0. Maskable external interrupts remain disabled. Vectors above 31 do
not form an interrupt ABI in Stage 1, and an exception is fatal: Stage 1 does
not resume with `iretq`.

The TSS has a dedicated, fixed-size, nucleus-owned IST stack for #DF (vector
8). Its 16 KiB size is bounded and not derived from loader or capsule input.
This is fault containment, not an IRQ, APIC, scheduler, debugger, user-mode or
recoverable-fault subsystem.

The stable failure event is:

```text
TOS.EXCEPTION vector=<decimal> error=0x<hex> rip=0x<hex> cr2=<none|0x<hex>
```

`vector` is the exact x86_64 vector. `error` is the hardware error code where
the architecture supplies one and otherwise is normalized to zero. `rip` is
the CPU exception-frame RIP. `cr2` is the exact CR2 only for #PF (vector 14),
and literal `none` for every other vector. The handler allocates nothing,
panics nowhere, acquires no lock, emits the event and terminates through the
new stable `RESULT_EXCEPTION = 0x24`. With QEMU `isa-debug-exit`, that is raw
process exit 73. `TOS.PANIC`/`RESULT_PANIC` retain software-panic semantics.

This is an additive Boot ABI v1 failure path. Existing result/event meanings,
success ordering, BootInfo major/minor/size and the default production nucleus
artifact remain unchanged. An unknown non-success result or `TOS.*` failure is
failure, never a successful boot.

Isolated compile-time test features deliberately trigger #UD and #GP only
after the IDT has loaded. They build under distinct `CARGO_TARGET_DIR` paths
and the ordinary loader, BootInfo ABI, capsule/ESP builder, OVMF and QEMU
machine profile run them. The standard production artifact path is never
overwritten by a feature build.

## Architecture impact statement

- **Invariants:** I-02 and I-09 are strengthened by a bounded trusted-base
  containment mechanism and an additive versioned terminal result. No Tier 0
  invariant changes.
- **Canonical representation:** canonical installed source and capsule identity
  are unchanged; the BootInfo layout and success trace are byte-compatible.
- **Trusted base/dependencies:** GDT, TSS, IDT and minimal assembly stubs enter
  the existing no_std nucleus. No dependency is added.
- **Source-to-runtime and recovery:** source provenance and recovery/rollback
  are unchanged. Isolated test artifacts cannot replace the default artifact.
- **Threat/performance:** this contains CPU faults where architecturally
  possible; malicious firmware remains T7. The fixed stack and 32-entry tables
  introduce no attacker-sized allocation or measured-path traversal.
- **Compatibility/licensing/patents:** Boot ABI v1 gains one accepted terminal
  failure identifier/result. Code remains GPL-3.0-or-later; no external code or
  patent claim is introduced.
- **Evidence:** mechanical IDT/TSS/IST checks, ordinary QEMU exit 33, isolated
  #UD and #GP QEMU exits 73, and a production-artifact hash isolation check.

<!-- END docs/adr/0023-stage1-exception-baseline.md -->

---

<!-- BEGIN docs/adr/0024-capsule-provenance-sidecar.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0024: Capsule provenance sidecar manifest v1

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — adds a versioned artifact-provenance contract;
  capsule v1 bytes, Boot ABI v1 and the runtime trust boundary do not change
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The existing host builder emits a convenient JSON summary, but it is not a
versioned provenance manifest.  It lacks the full artifact format/ABI/target
description, material licence relationship, declared build options and a
reproducibility grade required by `docs/28_RELEASE_PROVENANCE_AND_REPRODUCIBILITY.md`.
Consequently it cannot by itself prove the complete Stage 1 source-to-capsule
relationship required by I-18 and the Stage 1 identity gate.

## Decision

The host builder emits the deterministic `tos-capsule-provenance-v1` JSON
sidecar defined by the accepted `source/interfaces/boot/CAPSULE_PROVENANCE_V1.md`
whenever `--meta` is requested.  The sidecar names the capsule SHA-256, capsule
format/version, architecture specification, builder, x86_64 loader/nucleus ABI
range, source commit or detached publication identity, every canonical source
material, build options, R0 reproducibility grade and retained licence-notice
set.

The whole-capsule digest remains the artifact anchor.  The sidecar is release
evidence and is not loaded, parsed or trusted by the UEFI loader or nucleus.
A host checker independently compares it with the capsule bytes, the committed
Git blobs in Git mode and the embedded notice tail.  Its deterministic JSON has
no timestamp, host path or environment-specific field.

`R0` means described provenance only; this decision makes no R1/R2/R3 claim.
The ordinary Stage 1 build uses Git-commit identity and a licence notice.  A
detached capsule names its ADR-0018 detached-source-set digest instead of
inventing a source commit.

## Architecture impact statement

- **Invariants/canonical representation:** I-01, I-10 and I-18 are strengthened.
  Canonical source remains repository text; the capsule remains a disposable
  transport/recovery seed, not installed system state.
- **Trusted base/dependencies:** only host builder/checker code changes.  The
  no_std loader, nucleus and parser gain no dependency or new input.
- **Source-to-runtime/recovery:** a Git capsule records its exact source commit
  and material blobs; detached mode records the accepted ADR-0018 publication
  identity.  Recovery/rollback selection is unchanged.
- **Threat/performance:** malformed or mismatched release evidence is rejected
  by the host gate.  No boot-time parser, hash traversal, firmware boundary or
  performance path is added.
- **Compatibility/licence/patent:** v1 capsule and BootInfo bytes remain
  compatible.  The notice set and each source SPDX expression are recorded,
  not inferred as a blanket artefact licence.  No imported code or patent claim
  is introduced.
- **Evidence:** deterministic builder sidecar regression, independent manifest
  checker/tamper rejection, Git-blob/material verification, QEMU success-path
  provenance check and full preflight.

<!-- END docs/adr/0024-capsule-provenance-sidecar.md -->

---

