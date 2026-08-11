<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS — consolidated development specification

> **GENERATED FILE — DO NOT EDIT.**  
> This file is a non-normative convenience view. Individual source documents and accepted ADRs govern according to `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.

Version: 0.2.1  
Source-manifest SHA-256: `a0ce634565d5b68bcadbf86ed21adcbf252bfe1f03735372aa9da52b887d73ea`  
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

Stage 1 is formally closed as a bootable TOS foundation with source-bound
capsule identity and fail-closed validation. Stage 1.5 is formally closed with
ADR-0027's bespoke TOS Core foundation selection. Stage 2 Part A is accepted:
ADR-0028 fixes the TOS Core V1 semantic/IR contract and ADR-0029 its Unicode
normalization baseline. Stage 2 Part B production reference implementation is
under way — the bounded source reader and lexer are complete and the parser is
in progress; checker, IR, verifier and interpreter are not implemented. TOS is
not yet a user shell, application environment, or desktop operating system.

## Core thesis

The owner of a computer should be able to own its software in the engineering
sense: open any installed component as human-readable source, understand it,
change it, check the change, and keep running their own version.

On a conventional system that loop is broken in the middle. The component you
can read and the artifact the machine executes are different objects, connected
by build infrastructure you do not run. Reading is possible; changing what is
actually installed is a separate project.

TOS closes the loop architecturally:

> The source tree is the installed system. Parsed IR, bytecode, native code, indexes, capsules and boot images are disposable derivatives with verifiable provenance.

The active system is identified by a commit. A machine can boot a known-good commit, branch its system, merge upstream changes, bisect regressions, push its system history to a remote and restore itself from a recovery nucleus plus repository.

Provenance, reproducibility, rollback and auditability follow from this model
and are worth having. They are consequences, not the goal. TOS is built so that
a competent owner can work on their own machine — not to defend a suspicious
user from the world.

## What makes TOS distinct

TOS is not merely:

- a microkernel with a scripting language;
- an immutable Linux distribution;
- source packages stored beside executables;
- Git used for developer configuration;
- a natural-language agent OS;
- a VM that happens to run drivers.

It is the conjunction of canonical installed text, owner-installable
modification, capability-confined textual services and drivers, commit-addressed
system identity, source-to-runtime traceability, transactional activation and
repository-native recovery.

The first two properties are the point: the installed component is source, and
the owner can change it and boot the result. The rest exist to make that safe
and repeatable rather than reckless.

TOS also states plainly what it does not own. Real machines run CPU microcode
and device firmware produced by hardware vendors. TOS does not pretend that
material is open, and does not let it quietly replace a component that should be
text — it is named, versioned, hashed and kept visibly outside the canonical
source tree. See ADR-0030.

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
    - Accepted Stage 2 V1 contract: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`
      through `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`
11. `docs/07_LANGUAGE_FRONTENDS.md`
12. `docs/08_GIT_NATIVE_SYSTEM.md`
13. `docs/36_GIT_COMPATIBILITY_PROFILES.md`
14. `docs/09_FILESYSTEM_AND_STATE.md`
    - Runtime hierarchy: `docs/45_SYSTEM_SOURCE_HIERARCHY.md`
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

Stage 0, Stage 1, Stage 1.5 and Stage 2 Part A are formally closed. The
repository is in Stage 2 Part B: the production TOS Core reference frontend is
being implemented against the accepted contracts in `docs/39`–`docs/44`. The
bounded source reader and lexer are complete; the parser is in progress; the
checker, IR lowering, verifier and interpreter are not implemented. Stage 3 is
not authorized.

ADR-0030 (external vendor opaque material and `/vendor`), ADR-0031 with
`docs/45_SYSTEM_SOURCE_HIERARCHY.md` (runtime system source hierarchy) and
ADR-0032 (parser diagnostics and recovery) are accepted; their implementation is
deferred to the stage that first needs each subsystem. No implementation
decision may silently contradict an accepted ADR or invariant. Legal documents
are project policy, not jurisdiction-specific legal advice.

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
| Vendored Unicode Character Database data and its generated normalization tables | Unicode License v3 | `Unicode-3.0` |

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
- `LICENSES/Unicode-3.0.txt` for the UCD material recorded in `THIRD_PARTY.toml`

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

TOS exists so that the owner of a computer can own its software in the
engineering sense: open an installed component as human-readable source,
understand it, change it, validate the change, and continue using their own
version.

Everything below is how that is achieved. The project builds an operating system
where:

- the owner can inspect, modify, replace, and recover every TOS component;
- the installed system is inspectable source text;
- changing source text changes the system without a separate package-build-install cycle;
- executable caches are derived and disposable;
- the system is identified by a repository commit;
- rollback, branching, merging, cloning, and bisecting are ordinary system operations;
- device support can be delivered as textual driver modules;
- multiple programming languages can be added as textual frontend modules targeting one common execution model.

Provenance, reproducibility, auditability and supply-chain transparency are real
benefits of this model, and TOS should deliver them. They are not the motivation.
An operating system designed primarily around distrust would make different
choices than one designed around ownership, and TOS makes the ownership choice
whenever the two diverge.

TOS does not claim ownership of material it does not produce. CPU microcode,
device firmware and comparable vendor-controlled opaque material are named as
external rather than hidden or denied; see ADR-0030.

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

## This is about ownership, not suspicion

Everything above produces properties that are usually filed under security:
provenance, reproducible artifacts, verifiable rollback, auditable history,
supply-chain transparency. TOS should deliver them and should not overstate
them.

But they are consequences of the model, not the reason for it. The reason is
that a person who owns a machine should be able to work on it — open the thing
that is running, understand it, change it, check the change, and keep the
result. Source identity exists so the owner knows what they are editing.
History exists so a mistake is recoverable. Capabilities exist so one component
can be changed without endangering the rest.

The distinction is not rhetorical; it decides arguments. A system built around
distrust resolves ambiguity by restricting the user, and ends up locking the
owner out for their own protection. TOS resolves it by keeping the owner able to
proceed with the risk visible. That is why an explicit research mode exists, why
recovery is a first-class operation rather than a warning dialog, and why no
security control in TOS may become a permanent denial of ownership.

TOS is not an operating system for people who are afraid of their computer. It
is for people who want to open it.

## What TOS does not own

A real machine executes material TOS does not produce: CPU microcode, GPU and
peripheral firmware, vendor-signed device images. TOS cannot make that material
readable, and pretending otherwise would be the same dishonesty this document
objects to.

So TOS names it. Vendor-controlled opaque material is identified, versioned,
hashed and kept visibly outside the canonical source tree. It never silently
replaces a component that should be text, and the owner can always see where the
boundary runs. The boundary is stated so that its size can be observed — and
argued about — rather than discovered. See ADR-0030.

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
/vendor   external vendor-controlled opaque material, outside canonical source
```

The names are normative at the conceptual level; exact mount implementation may evolve through ADRs.

`/vendor` holds material TOS does not own or author, such as CPU microcode and
device firmware. It is not part of the canonical `/system` tree and is never
presented as TOS source. `/system` may declare that it requires a vendor object
by identity, version and hash; the opaque bytes themselves stay in `/vendor`.
See ADR-0030.

The internal structure of the runtime `/system` tree is defined by
`docs/45_SYSTEM_SOURCE_HIERARCHY.md`.

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
8. **External vendor material** — opaque bytes TOS identifies but does not
   inspect, verify or vouch for. TOS states their identity and version; it makes
   no claim about their behavior.

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

“TOS Core” is the accepted bespoke TOS-owned native textual language foundation
under ADR-0027.

The syntax in this document remains illustrative. The accepted complete V1
contract is split across docs/39–44 and is governed by accepted ADR-0028.
Stage 2 implements the accepted parser, grammar, verifier, and runtime
specification within ADR-0027's accepted semantic/trust boundary.

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

- **Bootstrap profile** — bounded allocation, no ambient dynamic module loading, minimal standard library, used during early boot and recovery. It MAY run on one worker/core, use deterministic serialized execution and restrict or prohibit parallel spawning.
- **Full profile** — structured asynchronous and parallel tasks, richer collections, dynamic service discovery, frontend APIs and user applications.

The bootstrap profile is a strict supported subset, not a temporary fake
language or a second concurrency semantics. Its restrictions are profile
restrictions on the same selected language foundation.

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

The Stage 1.5 selection ADR MUST establish the semantic/trust boundary for:

- canonical source authority;
- type/effect, ownership/region and concurrency direction;
- verifier/IR/runtime relationship;
- bounded bootstrap and SMP-capable full-profile direction; and
- no safe-language data-race undefined behavior or hidden host-runtime ABI.

The accepted Stage 2 V1 documents (docs/39–44) define the complete contract
within that boundary, including:

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

The V1 contract defines the concurrency memory model: ownership, immutable
sharing, mutable sharing, transfer of values and tasks between execution
contexts, synchronization primitives, atomic types and memory orderings,
visibility/happens-before rules, interaction between atomic and ordinary
memory, shared memory regions and the unsafe concurrency boundary. It MUST NOT
rely on a particular Rust, C++ or host-runtime memory model merely by
implication. Accepted docs/40–41 provide that definition under ADR-0028.

Safe TOS Core code MUST NOT have undefined behavior from an unsynchronized
data race. The foundation MUST statically prevent unsafe unsynchronized mutable
sharing, provide defined runtime/type semantics for it, or combine those
methods. Ordinary safe code MUST NOT turn a race into arbitrary memory
corruption or undefined behavior.

The model MUST remain address-space independent. Ordinary safe code MUST NOT
assume a fixed virtual-address width, a fixed page-table layout or a fixed
process address-space size. Machine-sized indices and sizes MAY follow the
declared target ABI when semantically necessary; persistent and public
serialized formats use explicitly defined fixed-width types. Physical addresses
remain privileged system-level concepts rather than ordinary language integers.

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

## Concurrency, parallelism and execution contexts

TOS Core distinguishes three related but different mechanisms:

- **asynchronous tasks** await IPC, IRQs, timers, I/O and other events without necessarily occupying a CPU;
- **parallel tasks** perform CPU-bound or independent work that MAY execute simultaneously on different CPU cores; and
- **low-level execution contexts/threads** are runtime or nucleus mechanisms for cases that require direct control.

An async event loop alone does not satisfy the TOS Core requirement.

In the full profile, one process MUST be able to have multiple runnable
execution contexts sharing its address space. Independent language-level work
MUST have a path to simultaneous execution on different CPU cores. Channels,
actors, IPC and queues MAY be important mechanisms, but they are not the only
way for a process to use multiple cores. Ordinary CPU-parallel work MUST NOT
require separate processes, serialization through IPC or manual queue
construction solely to obtain multicore execution.

The preferred safe-code model is structured concurrency and structured
parallelism. Conceptually, a scope may spawn parallel child work and then join
it; this is a semantic illustration, not accepted syntax. Parallel child tasks
belong to their scope, have a defined join and lifetime, define cancellation
behavior and cannot leave resources uncontrolled as orphans. Unscoped or
detached execution, if provided, is an explicit lower-level facility.

Program correctness MUST NOT depend on a CPU number, worker count or scheduler
interleaving. A correct program remains semantically correct on one, two or N
CPUs. The model MAY specify concurrency-related nondeterminism, but it MUST
define permitted outcomes. A correctly synchronized deterministic computation
MUST NOT change its logical result only because the runtime has a different
number of workers.

The language/runtime foundation MUST provide defined typed contracts, whether
as language features or standard/runtime APIs, for mutexes, reader/writer
synchronization where justified, semaphores or events, barriers or latches,
atomics, channels/message passing and task join/cancellation. Their semantics
MUST NOT depend on accidental host-runtime behavior.

Parallel execution does not grant unbounded CPU authority. The process/resource
model MUST be able to account for or limit total CPU time, runnable execution
contexts, parallel workers/tasks, stacks, memory, synchronization resources,
shared regions and cancellation cleanup cost. Spawning in a loop MUST NOT
implicitly create an unbounded number of kernel threads.

A reference or recovery interpreter MAY serialize parallel tasks for auditability
if it preserves the specified language semantics and conformance tests prove
that fact. At least one production-capable execution path MUST nevertheless
support genuine simultaneous multicore execution. All execution modes retain
the same language and memory semantics.

The selected foundation MUST leave an architectural path for later CPU affinity,
NUMA-aware scheduling and memory placement, heterogeneous cores and
topology-aware scheduling. Stage 1.5 does not define their final APIs.

## Metaprogramming

Unrestricted textual macros are excluded from the bootstrap profile. Any future macro system must be hygienic or equivalently attributable, preserve source maps and include generated expansion identity in cache keys.

## Standard-library boundary

Filesystems, networking, UI, Git operations and devices are services through versioned interfaces, not hidden language intrinsics.

## Foundation decision record

ADR-0015 required the comparison and ADR-0027 records its accepted result:
bespoke TOS Core. The retained matrix/research remains evidence, not a language
grammar. Lua, Scheme, WebAssembly and other systems are not pre-approved
foundations; a later separately accepted ADR may admit one only as a derived
backend while TOS text remains canonical.

## Licence of language assets

The official runtime and standard implementation are GPL-3.0-or-later. Public grammar schemas, frontend ABI definitions, bindings and conformance libraries may be Apache-2.0 when explicitly marked. The prose language specification is CC-BY-SA-4.0.

<!-- END docs/05_TOS_CORE_LANGUAGE.md -->

---

<!-- BEGIN docs/06_EXECUTION_AND_IR.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Execution model and intermediate representation

> ADR-0027 accepts bespoke TOS Core as the language foundation. This document
> specifies the accepted execution boundary. Accepted docs/39–44, especially
> `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`, define the complete V1 semantic IR
> schema and verifier contract under ADR-0028; Stage 2 Part B production
> implementation is authorized.

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

TOS IR is a versioned, typed, capability-aware intermediate representation
shared by all supported language frontends. The detailed V1 contract is the
accepted `tos-ir/v1` schema in docs/43; this role document does not override it.

It must represent:

- typed values and control flow;
- functions and calls;
- explicit error edges;
- capability operations;
- IPC send/receive operations;
- memory-region operations;
- async suspension points;
- structured task spawn, parallel task spawn, join and cancellation;
- synchronization operations and atomic operations with their required memory-order semantics;
- typed shared-memory-region access;
- execution-context and resource-accounting operations;
- source maps;
- resource limits;
- module imports and exports;
- driver-specific operations only through typed service contracts.

Parallel semantics MUST remain directly represented or pass through versioned,
typed verifier-visible runtime contracts. They MUST NOT disappear into an opaque
host runtime API that the verifier and resource model cannot understand. A
lowered runtime call is acceptable only when its contract specifies the relevant
task, cancellation, synchronization, atomic, shared-memory and resource
semantics.

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

1. **Reference interpreter** — simplest auditable semantics; mandatory for tests and recovery. It MAY execute otherwise-parallel work serially only when it preserves the specified semantics.
2. **Bytecode engine** — compact efficient default.
3. **JIT backend** — optional for long-running services and applications.
4. **Ahead-of-use native cache** — generated locally or by a trusted builder, always verified against source identity.

All engines must pass the same conformance suite, including concurrency,
atomic and memory-model vectors. A production-capable backend/runtime MUST have
a path to true simultaneous SMP execution; every engine MUST implement one
language/IR memory semantics rather than silently giving races or atomics
different behavior. Wasm or another binary format may serve as a backend or
cache profile only when canonical text, verifier independence and source
identity remain authoritative.

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
For concurrently executing work, the mapping also identifies the originating
task/execution context and the source span of spawned, joined, cancelled or
synchronized work without making scheduler timing part of source identity.

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

<!-- BEGIN docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — source model and grammar

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language version: `TOS Core 1.0`
- Authority on acceptance: Tier 2 under
  `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`
- Governing Tier 1 decision: ADR-0027
- Companion contracts: `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`,
  `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`,
  `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`,
  `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`, and
  `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`

## Status and boundary

This document is the accepted lexical and syntactic part of one TOS Core V1
contract set. It is intentionally detailed enough to prevent a first parser
from inventing language semantics. ADR-0028 accepts it as Tier 2 authority
under the normative hierarchy. It authorizes the production reference
implementation; implementation status remains separate and is recorded in the
guide, tutorial, conformance evidence, and stage report.

TOS Core V1 is the TOS-owned textual language selected by ADR-0027. Canonical
installed code is normalized UTF-8 `.tos` source. ASTs, typed IR, bytecode and
native code are derived artifacts. This specification defines language syntax;
it does not make an existing host compiler, C ABI, host thread API, LLVM, Rust,
Wasm, libc or external VM part of the TOS contract.

## 1. Canonical source unit

A source unit is exactly one file with extension `.tos` and one `module`
declaration. Its canonical identity consists of:

```text
source_set_identity
canonical repository path
sha256(normalized_source_bytes)
language version (1.0)
profile declaration
```

`source_set_identity` is the active commit identity or an explicitly accepted
detached source-set identity; it is not a pathname, working directory, clock,
network response, random value, or host environment variable. The SHA-256
value is written `sha256:<lowercase-hex>` and identifies normalized source
bytes, not an executable derivative.

A canonical source unit MUST:

- be valid UTF-8;
- be Unicode NFC after newline normalization;
- contain no UTF-8 BOM;
- use LF (`U+000A`) line endings; and
- contain no NUL scalar value.

For TOS Core 1.0, **Unicode NFC** means NFC exactly under Unicode Standard and
Unicode Character Database (UCD) **17.0.0**, using UAX #15 Revision 57. This
fixed normalization baseline is part of the source-language version, not a
host-library, locale, operating-system, or implementation choice. A newer
Unicode release MUST NOT silently change TOS Core 1.0 source acceptance.

An input reader MAY accept CRLF as transport input only by replacing each CRLF
with one LF before UTF-8/NFC validation and identity calculation. A bare CR is
`E1003_BARE_CR`. The source object recorded in a repository and every cache
key use the resulting normalized LF/NFC bytes. A BOM is
`E1002_BOM_FORBIDDEN`; invalid UTF-8 is `E1001_INVALID_UTF8`; a non-NFC input
is `E1004_NOT_NFC`. An implementation MUST report the earliest offending byte.
Malformed UTF-8 is `E1001_INVALID_UTF8` and is rejected before normalization.
The reference frontend's normalization data MUST be reproducible from the
Unicode 17.0.0 UCD baseline; its exact input files, hashes, and generator
identity are provenance inputs, not ambient host state. See ADR-0029.

Before UTF-8 or normalization work, a raw input larger than the 256 KiB
source-unit ceiling in docs/44 is `E1000_SOURCE_LIMIT` at the first excluded
byte. A NUL scalar in otherwise valid source is `E1005_NUL_FORBIDDEN` at that
byte. These checks do not change the existing `E1001`–`E1004` precedence.

The canonical repository path is a validated relative slash-separated path.
It has no `.` or `..` segment, no empty segment, no NUL, and no path separator
other than `/`. A module's declared name maps to this path as specified in
`docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`.

## 2. Lexical rules

Outside literals and line comments, only ASCII space (`U+0020`) and LF are
whitespace. Horizontal tab is `E1010_TAB_OUTSIDE_LITERAL`; other Unicode
whitespace is `E1011_NON_ASCII_WHITESPACE`. This deliberate restriction makes
layout, source maps and review diffs unambiguous. Four spaces are the project
style; indentation has no syntactic meaning.

A line comment starts with `//` and continues through, but excluding, LF.
Block comments and textual macros do not exist in V1. This makes comment
termination and source-span accounting bounded and local. An SPDX line comment
is ordinary comment text to the language.

Identifiers are ASCII and match:

```text
[A-Za-z_][A-Za-z0-9_]*
```

They are case-sensitive. Unicode is permitted in string data and comments but
not identifiers. A source reader reports `E1012_INVALID_IDENTIFIER` at the
first nonmatching byte rather than applying case folding or confusable mapping.

V1 has no contextual keywords. Every identifier-shaped language word belongs to
exactly one class below. Reserved, primitive, predeclared type, and predeclared
value names cannot be shadowed; every other matching identifier is ordinary.
The inventory is deliberately machine-readable and is checked by
`scripts/check-stage2-language-contract.py` against the EBNF terminals.

<!-- stage2-word-inventory:start -->
```text
reserved: as async await bootstrap borrow break cancel capability const continue defer else enum extern false fn for full if import in join let loop match module mut parallel profile pub record resource return spawn true unsafe uses version while
primitive-type: bool i8 i16 i32 i64 u8 u16 u32 u64 size duration string bytes unit
predeclared-type: Option Result Task TaskResult Shared Region DmaRegion Mutex RwLock MutexGuard ReadGuard WriteGuard Channel Event Semaphore Barrier Latch AtomicBool AtomicU32 AtomicU64 ConversionError slice array
atomic-order: Relaxed Acquire Release AcqRel SeqCst
predeclared-value: Some None Ok Err Completed Cancelled
predeclared-function: to_i8 to_i16 to_i32 to_i64 to_u8 to_u16 to_u32 to_u64 wrapping_add wrapping_sub wrapping_mul
special-token: _
```
<!-- stage2-word-inventory:end -->

`nil` is not a V1 keyword, literal, pattern, type, or absence model. An
ordinary identifier spelled `nil` is allowed, subject to normal name resolution;
unbound use receives `E1202_UNKNOWN_VALUE_NAME`. `Option<T>` is the only V1
typed absence model.

## 3. Literals

Integer literals are decimal (`42`), hexadecimal (`0x2a`) or binary (`0b101010`)
digits with optional single underscores between digits. A leading sign is an
operator, not part of a literal. Invalid base digits, a leading/trailing
underscore, or repeated underscores are `E1020_INVALID_INTEGER_LITERAL`.

An integer suffix is one of `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, or
`i64`. A suffix fixes the literal type and range-checks it. Unsuffixed literals
are contextually typed by a fixed-width operand, parameter, binding annotation,
or return annotation; otherwise they are `i32` and range-checked as `i32`.
There is no target-dependent implicit integer type.

Size literals are an integer literal followed without whitespace by `B`, `KiB`,
`MiB`, or `GiB`; their type is `size`. `KiB = 1024`, `MiB = 1024^2`, and
`GiB = 1024^3`. Duration literals similarly use `ns`, `us`, `ms`, `s`, `min`,
or `h` and have type `duration`. Their represented nanoseconds MUST fit `u64`.

Strings use double quotes and contain Unicode scalar values except unescaped
LF, CR and NUL. Valid escapes are `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\xNN`, and
`\u{H...H}` with one to six hexadecimal digits naming a Unicode scalar value.
`\xNN` inserts one byte whose value must form valid UTF-8 in the completed
string. An invalid escape, invalid scalar, unterminated string, or unescaped
line ending reports `E1030_INVALID_STRING`. A `bytes` literal begins `b"` and
permits only ASCII graphic characters, space, and the byte escapes `\\`,
`\"`, `\n`, `\r`, `\t`, `\0`, and `\xNN`; it reports
`E1031_INVALID_BYTES` otherwise.

## 4. Grammar notation and parser behavior

The grammar uses EBNF. `X?`, `X*`, and `X+` mean optional, zero-or-more, and
one-or-more. Literal tokens are quoted. `identifier`, `integer`, `string`,
`bytes`, `size`, and `duration` refer to the lexical tokens above.

The parser is deterministic. At a declaration-level error it synchronizes at
the next top-level `;` or `]`, or at the `}` that closes a top-level declaration
body and returns delimiter nesting to zero. At a statement-level error it
synchronizes at the next `;` or the closing brace of the current block. At a
comma-separated list error it synchronizes at `,` or the enclosing closer. It
MUST emit the lowest-numbered applicable lexical error first; then the earliest
unconsumed syntax token; then one recovery diagnostic per synchronization
region. It MUST not guess a missing declaration, capability, type, or operator.

The closing-brace boundary exists because a `function_decl` ends with a block
rather than `;` or `]`: without it, one malformed signature would discard every
later declaration in the source unit. It can only end a region earlier than the
`;`/`]` rule would, never later, and it admits no further recovery heuristic.
See ADR-0032.

Every diagnostic a source reader, lexer or parser can raise is registered with
its stage and exact condition in
`docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md` section 7.

### Constructed-type boundary

The parser builds a constructed-type node for a known V1 type constructor
written with `<...>` and does not decide how many type arguments it should
take. That count is a static type property checked at the type stage and
reported as `E1204_TYPE_ARGUMENT_ARITY`; an unresolved type name is
`E1203_UNKNOWN_TYPE_NAME`. See ADR-0034 and
`docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md` section 2.

This grants no user generics: `<...>` remains admissible only after a name the
language already defines as a parameterized constructor, and `array<T, N>`
keeps its own form because its second argument is a constant.

### Pattern resolution boundary

A `pattern_path` of one identifier stays a single syntactic alternative. The
parser MUST NOT decide whether it is a constructor or a binding: that decision
needs the pattern's expected type and belongs to the checker, which resolves it
under ADR-0033 and `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`
section 2.

A `pattern_path` containing at least one `.` is always a constructor path and
never a binding. The parser therefore records the whole path and leaves its
meaning to resolution.

## 5. Complete V1 grammar

```ebnf
source          = module_header import_decl* item* EOF ;
module_header   = "module" module_name "version" version
                  "profile" profile ";" ;
module_name     = identifier ( "." identifier )* ;
qualified_name  = module_name ;
version         = integer "." integer ;
profile         = "bootstrap" | "full" ;

import_decl     = "import" module_name ( "as" identifier )? ";"
                | "import" "capability" module_name "." identifier
                  "as" identifier ";" ;

item            = visibility? resource_decl
                | visibility? record_decl
                | visibility? enum_decl
                | visibility? const_decl
                | visibility? function_decl
                | visibility? extern_decl ;
visibility      = "pub" ;
resource_decl   = "resource" "[" resource_limit_list? "]" ;
resource_limit_list = resource_limit ( "," resource_limit )* ","? ;
resource_limit  = identifier ":" literal ;
record_decl     = "record" identifier "[" field_decl_list? "]" ;
field_decl_list = field_decl ( "," field_decl )* ","? ;
field_decl      = visibility? identifier ":" type ;
enum_decl       = "enum" identifier "[" variant_decl_list? "]" ;
variant_decl_list = variant_decl ( "," variant_decl )* ","? ;
variant_decl    = identifier ( "(" type_list? ")" )?
                | identifier "[" field_decl_list? "]" ;
const_decl      = "const" identifier ":" type "=" expression ";" ;
function_decl   = async_marker? "fn" identifier "(" parameter_list? ")"
                  "->" type effects? block ;
async_marker    = "async" ;
parameter_list  = parameter ( "," parameter )* ","? ;
parameter       = borrow_mode? identifier ":" type ;
borrow_mode     = "borrow" ( "mut" )? ;
effects         = "uses" "[" identifier ( "," identifier )* ","? "]" ;
extern_decl     = "extern" "fn" identifier "(" parameter_list? ")"
                  "->" type effects? ";" ;

type            = primitive_type | predeclared_type | named_type | constructed_type
                | array_type | tuple_type | function_type ;
primitive_type  = "bool" | "i8" | "i16" | "i32" | "i64"
                | "u8" | "u16" | "u32" | "u64" | "size" | "duration"
                | "string" | "bytes" | "unit" ;
predeclared_type = "Event" | "Semaphore" | "Barrier" | "Latch"
                | "AtomicBool" | "AtomicU32" | "AtomicU64"
                | "ConversionError" ;
named_type      = qualified_name ;
constructed_type = "Option" "<" type ">"
                | "Result" "<" type "," type ">"
                | "Task" "<" type ">"
                | "TaskResult" "<" type ">"
                | "Shared" "<" type ">"
                | "Region" "<" type ">"
                | "DmaRegion" "<" type ">"
                | "Mutex" "<" type ">"
                | "RwLock" "<" type ">"
                | "MutexGuard" "<" type ">"
                | "ReadGuard" "<" type ">"
                | "WriteGuard" "<" type ">"
                | "Channel" "<" type ">"
                | "slice" "<" type ">" ;
array_type      = "array" "<" type "," const_expression ">" ;
tuple_type      = "(" type "," type ( "," type )* ","? ")" ;
function_type   = "fn" "(" type_list? ")" "->" type ;
type_list       = type ( "," type )* ","? ;

block           = "{" statement* "}" ;
statement       = let_stmt | assignment ";" | expression ";" | return_stmt
                | break_stmt | continue_stmt | if_stmt | match_stmt
                | while_stmt | for_stmt | loop_stmt | parallel_stmt
                | cancel_stmt | defer_stmt | unsafe_stmt ;
let_stmt        = "let" "mut"? pattern ( ":" type )? "=" expression ";" ;
assignment      = place "=" expression ;
return_stmt     = "return" expression? ";" ;
break_stmt      = "break" ";" ;
continue_stmt   = "continue" ";" ;
if_stmt         = "if" "(" expression ")" block
                ( "else" ( if_stmt | block ) )? ;
match_stmt      = "match" "(" expression ")" "{" match_branch* "}" ;
match_branch    = pattern "=>" block ;
while_stmt      = "while" "(" expression ")" block ;
for_stmt        = "for" pattern "in" "(" expression ")" block ;
loop_stmt       = "loop" block ;
parallel_stmt   = "parallel" block ;
cancel_stmt     = "cancel" expression ";" ;
defer_stmt      = "defer" block ;
unsafe_stmt     = "unsafe" block ;

pattern         = "_"
                | pattern_path ( "(" pattern_list? ")" )?
                | "(" pattern_list ")" ;
pattern_path    = pattern_name ( "." identifier )* ;
pattern_name    = identifier | predeclared_value ;
pattern_list    = pattern ( "," pattern )* ","? ;
expression      = logical_or ;
logical_or      = logical_and ( "||" logical_and )* ;
logical_and     = equality ( "&&" equality )* ;
equality        = comparison ( ( "==" | "!=" ) comparison )* ;
comparison      = bit_or ( ( "<" | "<=" | ">" | ">=" ) bit_or )* ;
bit_or          = bit_xor ( "|" bit_xor )* ;
bit_xor         = bit_and ( "^" bit_and )* ;
bit_and         = shift ( "&" shift )* ;
shift           = sum ( ( "<<" | ">>" ) sum )* ;
sum             = product ( ( "+" | "-" ) product )* ;
product         = unary ( ( "*" | "/" | "%" ) unary )* ;
unary           = ( "!" | "-" | "~" | "borrow" ( "mut" )? | "await" | "join" ) unary
                | postfix ;
postfix         = primary ( call_suffix | index | field | question | cast )* ;
call_suffix     = "(" call_arguments? ")" ;
call_arguments  = positional_argument_list | named_argument_list ;
positional_argument_list = expression ( "," expression )* ","? ;
named_argument_list = named_argument ( "," named_argument )* ","? ;
named_argument  = identifier ":" expression ;
index           = "[" expression "]" ;
field           = "." identifier ;
question        = "?" ;
cast            = "as" type ;
primary         = literal | "true" | "false" | predeclared_value
                | predeclared_function | qualified_name | tuple | array
                | closure | spawn_expression | "(" expression ")" ;
predeclared_value = "Some" | "None" | "Ok" | "Err" | "Completed" | "Cancelled" ;
predeclared_function = "to_i8" | "to_i16" | "to_i32" | "to_i64"
                | "to_u8" | "to_u16" | "to_u32" | "to_u64"
                | "wrapping_add" | "wrapping_sub" | "wrapping_mul" ;
literal         = integer | size | duration | string | bytes ;
tuple           = "(" expression "," expression ( "," expression )* ","? ")" ;
array           = "[" positional_argument_list? "]" ;
closure         = "fn" "(" closure_parameters? ")" block ;
closure_parameters = parameter ( "," parameter )* ","? ;
spawn_expression = "spawn" ( "async" | "parallel" ) block ;
place           = identifier ( field | index )* ;
const_expression = const_sum ;
const_sum       = const_product ( ( "+" | "-" ) const_product )* ;
const_product   = const_primary ( ( "*" | "/" | "%" ) const_primary )* ;
const_primary   = integer | size | identifier | "(" const_expression ")" ;
```

The surface punctuation has one human-facing rule: `()` groups expressions and
contains parameters or call/constructor arguments; `[]` contains declarative
or data lists; `{}` contains executable statements; commas separate list
members; and semicolons terminate simple executable statements. A trailing
comma is permitted in every comma-separated V1 list. A compound statement that
ends in its own `}` takes no following semicolon.

Every control header has mandatory parentheses. The closing `)` therefore ends
an `if`, `while`, `for`, or `match` head before the following executable block
begins; `if ready { ... }` is `E1105_CONTROL_HEAD_PARENS_REQUIRED`. `if` and
`match` are statement-only in V1. Their branches are executable blocks, their
branches have no comma separators, and neither construct is an expression or
an implicit value producer. `while`, `for`, `loop`, and `parallel` are likewise
statement-only; `break` has no value.

A name followed by a call suffix is always one unresolved Call/Construct syntax
node, whether resolution later finds a function, an `Option`/`Result`
constructor, a user enum tuple variant, or a nominal record constructor. The
parser never chooses a constructor parse instead of a function-call parse.
Resolution validates the selected callee kind after that one syntax form is
built; this is not semantic backtracking. Call arguments are either all
positional or all named; the first argument's `identifier ":"` form fixes named
mode. Named arguments are accepted only for nominal record constructors and
named-field enum variants, not ordinary functions or tuple enum variants. They
name every declared field exactly once; an unknown name is
`E1207_UNKNOWN_RECORD_FIELD`, a duplicate is `E1205_DUPLICATE_RECORD_FIELD`,
and an omitted field is
`E1206_MISSING_RECORD_FIELD`. Named argument expressions are evaluated in
source order. `Point(x: 1i32, y: 2i32)` is therefore a record construction;
`Rgb(red: 1u8, green: 2u8, blue: 3u8)` similarly constructs a named-field enum
variant;
`Point { x: 1i32, y: 2i32 }` is not V1 syntax. Missing a comma between list
members is `E1106_LIST_SEPARATOR_REQUIRED`.

Function calls, constructor calls, field access, indexing, propagation (`?`)
and casts group left-to-right; binary precedence is listed from weakest to
strongest. `&&` and `||` short-circuit. `await`, `join`, and `borrow` bind like
other unary operators. A closure and a spawned task use an executable block;
their normal produced value, if any, uses an explicit `return` in that block.
An anonymous closure is `fn (parameters) { ... }`; it uses ordinary typed
parameters in `()` and has an inferred result under docs/40. A plain `{ ... }`
is never an expression and cannot follow `=` or occur as a call argument.

`defer`, `unsafe`, closures, `async`, and `spawn async` are Full-profile
constructs. `parallel`, `spawn parallel`, `join`, and `cancel` have defined
serialized Bootstrap semantics in `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`.
An `extern` declaration is reserved by the grammar but rejected as
`E1801_FFI_NOT_AVAILABLE` until a later accepted FFI contract supplies an
interface identifier and capability rule.

## 6. Deliberate exclusions

V1 has no textual macros, implicit imports, wildcard imports, inheritance,
user-defined generic declarations, traits, reflection, exceptions used for
ordinary errors, implicit numeric widening, pointer literals, address casts,
or syntax whose meaning depends on indentation. These exclusions reduce
bootstrap parser and verifier complexity; a later version requires explicit
version negotiation rather than silently reinterpreting V1 source.

<!-- END docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md -->

---

<!-- BEGIN docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — types, evaluation, ownership, and memory

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`
- Companion execution contract:
  `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`

## 1. Static model

TOS Core is statically typed. A well-typed safe program has no type confusion,
unbounded implicit coercion, arbitrary pointer access, or undefined behavior
caused by a safe data race. Type checking is deterministic for identical
normalized source, declared imports, language version, profile, and resource
declarations. It has no ambient filesystem, network, clock, random, current
directory, or environment input.

V1 has nominal record, enum, capability, region, and module types. Primitive
types are structural only within their exact name. The type of `A::T` is not
identical to `B::T` merely because their fields match. A type name resolves
through the declared import graph, never by host search paths.

The primitive types are `bool`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`,
`u32`, `u64`, `size`, `duration`, `string`, `bytes`, and `unit`. `size` is an
unsigned target-ABI-sized value used only for in-memory indexing and allocation
bounds. It MUST NOT be serialized in a persistent/public format. `duration` is
an unsigned `u64` count of nanoseconds. Public and persistent forms use one of
the explicit fixed-width integers.

`Option<T>` has variants `Some(T)` and `None`; `Result<T,E>` has variants
`Ok(T)` and `Err(E)`. `ConversionError` is the fixed V1 standard error type
returned by checked numeric conversions; ordinary code receives it only through
those `Result` values. `Task<T>` is an owned scoped task handle.
`TaskResult<T>` has variants `Completed(T)` and `Cancelled`; it is the result
of consuming a task handle through `join` or `await`. This keeps cancellation
distinct from a child value of type `T`, including when `T` is itself
`Result<U,E>`. `Shared<T>` is an immutable shareable value. `Region<T>` and
`DmaRegion<T>` are opaque
nucleus-granted typed region handles. `Mutex<T>`, `RwLock<T>`, `Channel<T>`,
`Event`, `Semaphore`, `Barrier`, `Latch`, `AtomicBool`, `AtomicU32`, and
`AtomicU64`, and `ConversionError` are non-generic typed runtime contracts,
not magic host APIs.
Their exact dynamic semantics are in
`docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`.

Arrays `array<T, N>` have a compile-time nonnegative `N` that is representable as
`size`. `slice<T>` means a borrowed view and cannot be stored or returned as an
owned value in V1. A function type `fn(A, B) -> R` is a non-capturing callable
type. Full-profile closures have a compiler-defined anonymous callable type and
cannot cross a module boundary until a later version defines stable closure ABI.

Enum variant names are local to their defining module and may be used
unqualified there; an imported enum variant uses a qualified type/module name.
`Some`, `None`, `Ok`, and `Err` are the fixed V1 constructors for `Option` and
`Result`, not host-library names.

### Pattern name resolution

Every pattern is checked against an expected type: the scrutinee type for
`match`, the initializer type refined by any annotation for `let`, the element
type for `for`, and the corresponding tuple element or enum payload type for a
nested pattern.

A bare identifier that exactly names a variant of the expected enum type is the
constructor pattern for that variant. Any other bare ordinary identifier
introduces a new pattern binding. `Name(...)` is a constructor and destructuring
pattern resolved the same way, and its sub-patterns are checked against that
variant's payload positions.

Resolution is nominal. There is no capitalization rule, and an existing lexical
or value binding of the same name does not change the outcome, so two enums may
declare variants with the same name and each is disambiguated by the expected
type of the subject. `Some`, `None`, `Ok`, `Err`, `Completed` and `Cancelled`
remain non-shadowable constructors and are never bindings.

A qualified pattern path — `Signal.Low`, or `other.Signal.Low` for an imported
enum — always denotes a constructor and never a binding. A qualified path that
names no reachable variant is an error; it does not degrade into a catch-all. A
local variant may be written either short or qualified. See ADR-0033.

There are no user-defined generic functions, traits, implicit interfaces, or
ad-hoc overload resolution in V1. The listed library type constructors are the
only parameterized types. This keeps type identity, diagnostics, and
independent verification bounded.

The complete V1 constructed-type arity is fixed and is shared with docs/39 and
docs/43: `Option<T>`, `Task<T>`, `TaskResult<T>`, `Shared<T>`, `Region<T>`,
`DmaRegion<T>`, `Mutex<T>`, `RwLock<T>`, `MutexGuard<T>`, `ReadGuard<T>`,
`WriteGuard<T>`, `Channel<T>`, and `slice<T>` take one
type argument; `Result<T,E>` takes two. `Event`, `Semaphore`, `Barrier`,
`Latch`, `AtomicBool`, `AtomicU32`, `AtomicU64`, and `ConversionError` take no
type arguments.
`array<T, N>` takes one type argument and one compile-time `size` constant;
its comma is a declarative type-parameter separator, not a statement
terminator. Its second argument is a constant rather than a type, so it is not
one of the parameterized constructors above. `slice<T>` is the only
borrowed-view type form and retains the nonescaping restrictions above.

The number of type arguments is a static type property, not a parser decision.
The parser builds a constructed-type node for any known V1 constructor written
with `<...>`, and the checker compares the actual count against the fixed arity
above; a mismatch is `E1204_TYPE_ARGUMENT_ARITY` with the constructor and both
arities. This is not an implementation-defined generic application, and it
admits no user generics: an arbitrary `Foo<T>` is not V1 type syntax.

A type name that resolves to no primitive, fixed or predeclared type, local
nominal type or reachable imported type is `E1203_UNKNOWN_TYPE_NAME`, carrying
the name as spelled. For a qualified name the module or import part resolves
first: a missing import or module is the applicable `E16xx` code, while an
existing one that does not declare the name is `E1203_UNKNOWN_TYPE_NAME`.

Precedence is fixed. An unresolved name is `E1203_UNKNOWN_TYPE_NAME`; a
resolved parameterized constructor applied with the wrong count is
`E1204_TYPE_ARGUMENT_ARITY`; only after the arity is correct are the argument
types and remaining type rules checked, so one mistake cannot cascade into
findings derived from a constructed type that does not exist. See ADR-0034.

## 2. Bindings, functions, effects, and capabilities

`let name = expression;` creates an immutable binding. `let mut name =
expression;` creates a mutable binding. A binding annotation constrains the
expression type. Assignment requires a mutable binding or a place reached
through one active mutable borrow. Assigning to a nonmutable place is
`E1201_ASSIGN_TO_IMMUTABLE`.

Function parameters without `borrow` consume an owned argument unless its type
is `Copy`. `borrow parameter: T` creates an immutable temporary borrow;
`borrow mut parameter: T` creates an exclusive mutable temporary borrow. V1
borrows cannot be returned, stored in records/enums/arrays, captured by a
Full-profile closure, sent through a channel, or placed in a task. These
restrictions make their region exactly the caller expression or callee body and
avoid hidden lifetime inference.

Functions are pure with respect to authority unless their `uses [ ... ]` set
names imported capability parameters or capability values. An operation that
requires a capability is type-correct only if its required capability name is
present in the enclosing function's transitive effect set. An empty effect set
is written by omission. Calling a function requires the caller effect set to
include every effect the callee requires; otherwise the checker emits
`E1501_UNDECLARED_CAPABILITY_EFFECT`. Capability values are opaque,
nonconstructible, and non-comparable except for identity logging by a privileged
runtime contract. An integer, string, cast, deserialization, record literal, or
unsafe block cannot mint one.

`async fn` returns `Task<Result<T, E>>` when its declared return type is
`Result<T, E>` and `Task<T>` otherwise. `await task` consumes `Task<T>` and
has type `TaskResult<T>`; it is an asynchronous join and is Full-profile only.
For example, awaiting `Task<Result<T,E>>` produces
`TaskResult<Result<T,E>>`: `Completed(Err(e))` is the child program result,
while `Cancelled` is task cancellation. `spawn async` and `spawn parallel`
capture values according to the ownership rules below. `spawn` has no detached
form in V1.

A Full-profile closure captures each free `Copy`/`Shared<T>` value by copy and
each other permitted value by move at closure creation. It cannot capture a
borrow, mutable binding by alias, lock guard, non-transferable capability, or
plain mutable region. A closure is affine when any captured value is affine.
It may be called within its owning scope but cannot be exported, serialized,
stored in a public nominal type, or passed to an interface with a stable ABI in
V1. An invalid capture is `E1305_INVALID_CLOSURE_CAPTURE`, except for a lock
guard: ADR-0036 routes a guard crossing a task or closure boundary to
`E1402_INVALID_GUARD_LIFETIME` with `operation=task_boundary`, because the rule
broken is about the guard's lifetime rather than about transferability alone.
The capture codes keep their meaning for every other non-`Transferable` value.

## 3. Conversion, equality, and integer semantics

No nonliteral numeric conversion is implicit. An integer literal may take the
surrounding exact integer type if in range; otherwise an unsuffixed literal is
`i32`. Assigning or passing values of different integer types is
`E1210_INTEGER_TYPE_MISMATCH`. `as T` is permitted only for an integer
widening conversion that preserves signedness, `u8` to `u16`/`u32`/`u64`, or
the corresponding signed widening. Any other `as` conversion is
`E1212_INVALID_AS_CONVERSION`.

Checked conversion has no generic-call syntax. The fixed V1 standard functions
`to_i8` through `to_i64` and `to_u8` through `to_u64` are ordinary Call-form
callees defined in docs/39. Each accepts any fixed-width integer or `size`,
checks sign and range, and returns `Result<D, ConversionError>` for its
spelled destination `D`. Thus `to_u8(value)` is the source form for a checked
narrowing/sign-changing conversion; callers use its `Result` rather than
depending on host casts. Explicit wrapping arithmetic is only available through
`wrapping_add`, `wrapping_sub`, and `wrapping_mul` contracts with exact
fixed-width type arguments.

An attempt to use `as` with a capability, region, DMA region, task,
synchronization object, function, closure, or pointer-like host value is not a
generic conversion error: it is `E1502_FORGED_CAPABILITY` for a capability and
the corresponding nonconstructible-type error for the other opaque types.

Normal integer `+`, `-`, `*`, `/`, `%`, unary `-`, and shifts are checked.
Overflow, division/remainder by zero, an invalid shift count, or `MIN / -1`
is a language trap with a stable `RUNTIME_*` code and terminates the current
process; it is not host undefined behavior and cannot be caught as `Result`.
For `uN`, `-x` is rejected statically. Shift counts must be nonnegative and
strictly smaller than `N`. `size` arithmetic is checked in the target ABI;
portable source must not assume its width.

`==` and `!=` are available for primitive values, immutable records/enums whose
members are comparable, and opaque handles only where the corresponding typed
contract explicitly exposes equality. They are not available for mutable
regions, mutable synchronization guards, tasks, capabilities, functions, or
closures. Ordering exists for numeric, `size`, `duration`, `string`, and
`bytes` values only. Strings compare lexicographically by their stored Unicode
scalar sequence; source NFC is a source-identity rule, not an implicit runtime
string-normalization pass. Bytes compare lexicographically by byte.

Array, slice, and region indexes have exact type `size`; an integer literal may
be contextually typed as `size` when nonnegative and representable. Other index
types are `E1211_INDEX_TYPE_MISMATCH`. Every safe index operation performs a
checked bounds operation and returns the declared typed bounds error where the
interface exposes one; it never becomes host out-of-bounds access.

## 4. Evaluation and dynamic semantics

TOS evaluates expressions left-to-right. Specifically, a Call evaluates its
callee, then arguments left-to-right, then enters the resolved function or
constructor; a binary operator evaluates its left operand before its right;
record/array/tuple fields evaluate in lexical source order; match subject
evaluates before patterns; assignment evaluates its place base/index
left-to-right before its right side. Ordinary function calls and tuple-variant
constructors use the same Call form and differ only at resolved-callee checking.
`&&` does not evaluate its right side after false; `||` does not evaluate its
right side after true. `?` evaluates its operand once and propagates the
matching `Err` from the nearest enclosing return scope if it is not `Ok`.

An executable block is a statement body, not a value container: it has no tail
expression. `return expression;` is the only normal value return. A function
body, closure body, and `spawn async`/`spawn parallel` body each establish a
**return scope**. Ordinary nested `{ ... }` blocks do not establish one.
`return` targets the nearest enclosing return scope, and `?` propagates `Err`
from that same nearest return scope. Every
reachable normal completion path of a function with a non-`unit` declared
return type MUST execute an explicit `return` with that exact type; reaching
the end of such a function is `E1221_MISSING_RETURN`. `return;` or a value of
the wrong type is `E1222_RETURN_TYPE_MISMATCH`. A `unit` function may reach its
end normally. A semicolon terminates a simple executable statement; it never
silently changes a would-be return value into `unit`.

A closure or spawned task body follows the same rule. Its body has result
`unit` only when every normal path reaches its end without a value `return`.
Otherwise every reachable normal completion path MUST explicitly return one
inferred exact result type; mixing a value return with a normal fallthrough is
`E1221_MISSING_RETURN`, and inconsistent returned types are
`E1222_RETURN_TYPE_MISMATCH`. This makes task/closure result production visible
without making their executable blocks expressions.

`if`, `match`, `while`, `for`, `loop`, and `parallel` are statements, not
expressions. An `if` branch, including `else`, is an executable block and has
no value typing rule. A `match` arm is likewise an executable block; arms are
not comma-separated. `match` must be exhaustive for an enum, `Option`, or
`Result`; a missing case is `E1220_NONEXHAUSTIVE_MATCH`. An `_` arm is
exhaustive. `break` has no value. Patterns bind by move unless the matched
subject is an immutable `Copy` value; borrows must be made explicitly before
match. `?` remains an explicit Result-propagation operation: it evaluates its
operand once and propagates the matching `Err` from the nearest enclosing
return scope; it is not an implicit block-tail return mechanism.

`Result` is the sole ordinary recoverable-error transport. A runtime trap is a
defined language failure caused by a violated dynamic precondition. `panic`
denotes a violated language/runtime invariant and has the same process-ending
effect as a trap but a distinct stable code family. Neither uses host exception
unwinding. Details and diagnostic attribution are defined in docs/41.

`defer` registers a lexically scoped cleanup block. Defers run in reverse
registration order whenever their enclosing block exits normally, by `return`,
by `?`, by `break`, or after cancellation reaches that block. A defer block
cannot `return`, `break`, `continue`, `await`, `join`, spawn work, or acquire a
new resource; violations are `E1225_INVALID_DEFER`. A trap/panic while running
a defer records both the original and cleanup cause then terminates. This
bounded rule gives cancellation deterministic cleanup without implicit general
unwinding.

A `defer` is deferred lexical cleanup, not a capture, and it is not a closure:
the closure-capture rules of `E1305_INVALID_CLOSURE_CAPTURE` do not apply to it.
Executing the `defer` statement registers the cleanup and nothing else. At that
point the lexical names in its body bind to the binding identities visible at
the point of registration, and the values of those bindings are neither read,
borrowed nor moved. Shadowing after registration does not change which binding
the body refers to. On each exit path the action that caused the exit is
evaluated first, then the defers registered on the path actually taken run in
reverse registration order, each observing the ownership and borrow state the
previous one left; only then do bindings leave scope and their bounded `drop`
run. A defer body is therefore checked against the ownership state that exists
on the concrete exit path, so ordinary correct use of a resource between
registering its cleanup and leaving the block is allowed, while a cleanup that
cannot run soundly on a path that reaches it is rejected there. `return`,
`break`, `continue` and normal block exit unwind the cleanups of exactly the
lexical blocks they leave; `?` and cancellation use the same model rather than a
second cleanup mechanism. See ADR-0035.

## 5. Ownership and borrows

Safe non-`Copy` values are affine: every value has one owner and is moved when
assigned, passed by an owning parameter, returned, put into an aggregate, or
captured by a task/closure. Use after move is `E1301_USE_AFTER_MOVE`. V1 has no
Copy declaration marker, trait, derivation, or user override. Fixed-width
numeric types, `size`, `duration`, `bool`, and `unit` are `Copy`. A tuple is `Copy`
exactly when every element is `Copy`; an array is `Copy` exactly when its
element type is `Copy`. User records and enums are always affine/non-Copy in V1,
even when every field or payload is `Copy`. `Option<T>`, `Result<T,E>`, and
`TaskResult<T>` are also affine V1 constructed values. `Shared<T>` is an
explicitly documented immutable handle and is `Copy`; strings, bytes,
capabilities, regions, DMA regions, tasks, locks, channels, events, semaphores,
barriers, latches, atomics, slices, closures, and functions are not `Copy`
unless an accepted later contract explicitly changes that type.

At any program point, a value may have either any number of immutable borrows
or exactly one mutable borrow, never both. An immutable borrow cannot mutate
the value; a mutable borrow cannot be aliased. The checker determines a borrow
region from the smallest enclosing expression/block required by use. Because
V1 borrowed values neither escape nor enter a task/aggregate, no inferred
cross-function lifetime notation is needed.

`E1302_CONFLICTING_BORROW` covers any operation that violates the exclusivity of
a live borrow of an overlapping place, not only the creation of a second borrow:
a new borrow incompatible with a live overlapping one; an ordinary owner read or
use of an overlapping place while a mutable borrow is live; an ordinary owner
mutation of an overlapping place while a mutable borrow is live; and a move or
other invalidation of an overlapping place while any borrow, shared or mutable,
is live. `E1303_MUTATE_WHILE_BORROWED` is the specialized case of a write to an
overlapping place while an immutable, shared borrow is live. The accepted matrix
is

```text
shared borrow  + owner write   -> E1303
mutable borrow + owner read    -> E1302
mutable borrow + owner write   -> E1302
any borrow     + owner move    -> E1302
incompatible borrow pair       -> E1302
```

Operations performed through the correct borrow binding itself are not owner
aliases and remain legal according to that borrow's kind. See ADR-0035.

An owned record/array/enum may be partially moved only when the remaining value
is never used except to move/drop its untouched fields. A mutable field borrow
locks the containing path, not unrelated fields; indexed elements are treated
as overlapping unless their indices are compile-time unequal constants. This
conservative rule is deterministic and safe.

Values leave scope in reverse binding order. Each type has a bounded `drop`
contract defined by its standard/module declaration. `drop` may release a
region, task reservation, synchronization object, or capability reference, but
may not allocate, await, acquire authority, or execute user callbacks.
Declaring a type whose cleanup does not have a finite documented bound is
rejected from Bootstrap as `E1708_UNBOUNDED_CLEANUP`.

## 6. Sharing, regions, and task transfer

`Shared<T>` is created only by the typed `share` contract for a `T` whose full
transitive contents are immutable and `Shareable`. It provides immutable
borrows and can be copied into multiple scoped tasks. It never grants mutation.
Controlled mutable sharing uses a `Mutex<T>`, `RwLock<T>`, atomic, channel, or
typed shared `Region<T>` operation; ordinary `mut` does not become globally
shareable.

`Region<T>` is an opaque process-local/shared-memory grant with declared
element type, byte length, alignment, access rights, and lifetime. Safe code
may obtain it only from an authority-bearing typed service operation, access it
only with checked `read`, `write`, or `slice` contracts, and never observe its
physical address. `DmaRegion<T>` additionally records a nucleus-granted DMA
mapping and device-domain authority; safe code may not construct, cast to, or
serialize it as an integer. Whether a particular region is shareable/mutable
is stated in its capability contract and independently checked in IR.

A value may cross a task boundary only if it is `Transferable`: owned affine
values transfer their sole ownership; immutable `Copy`/`Shared<T>` values are
duplicated; opaque capabilities, mutable borrows, lock guards (whose diagnostic
is `E1402_INVALID_GUARD_LIFETIME`, ADR-0036), and plain
mutable regions are non-transferable unless their own contract exposes a
specific attenuation/transfer operation. A closure/task with an invalid capture
is `E1304_INVALID_TASK_CAPTURE`.

There are no safe raw pointers, address literals, pointer arithmetic, address
casts, layout reinterpretation, arbitrary physical addresses, or implicit FFI
conversions. The TOS abstract address space is a set of typed regions, not a
48-bit x86_64 number. This preserves a path to LA57 and non-x86 targets.

## 7. Explicit unsafe boundary

`unsafe { ... }` is Full-profile only and changes neither ownership nor
capability authority. It only permits calls to an imported interface operation
explicitly marked `unsafe` by an accepted future interface contract. The block
MUST contain a leading line comment beginning `SAFETY:` that names the local
preconditions. A missing rationale is `E1802_UNSAFE_RATIONALE_REQUIRED`.
Unsafe code remains subject to declared capabilities, resource limits, source
maps, and IR verification. An unsafe block cannot forge a capability or turn a
safe caller's data race into undefined behavior; the unsafe operation's
interface must state how it preserves safe caller guarantees.

No V1 base operation currently requires `unsafe`; `extern` is rejected until a
later accepted contract exists. This is an explicit boundary, not an ambient
escape hatch or an invitation to inherit a Rust/C/host ABI.

<!-- END docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md -->

---

<!-- BEGIN docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — concurrency, resources, and diagnostics

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md` and
  `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`

## 1. Execution contexts and profiles

TOS Core distinguishes three mechanisms:

1. an **asynchronous task** waits for an explicit typed event/I/O contract and
   need not occupy a CPU while suspended;
2. a **parallel task** is independent language-level work that may execute
   simultaneously with sibling work on several cores; and
3. a low-level **execution context** is a nucleus/runtime resource, never an
   ambient language thread API.

The Full profile MUST have a production-capable path that executes independent
parallel tasks from one process simultaneously on multiple cores sharing that
process address space. Separate processes, IPC serialization, or manual queues
are not required merely to use multiple cores. The Bootstrap profile is a
strict subset of the same source/type/ownership/effect semantics. It MAY run
all parallel scopes serially on one worker and has no asynchronous I/O task
operation. Thus a valid Bootstrap parallel computation has the same permitted
logical result under a Full runtime; only timing/overlap differ.

Neither profile promises deterministic scheduling. Correctness MUST NOT depend
on CPU number, worker count, task execution order, or topology. A deterministic
computation whose effects are properly synchronized has the same logical
result on one, two, or N workers. Operations whose result depends on external
typed events, race-to-select, or cancellation expose that nondeterminism in
their result contracts rather than silently changing ordinary memory semantics.

## 2. Structured tasks, join, and cancellation

`parallel { ... }` creates a lexical task scope. `spawn parallel { ... }`
inside it creates a child `Task<T>` owned by that scope. The child owns or
immutably shares exactly the values captured under docs/40. Every spawned
child MUST ultimately be joined/consumed before scope exit. A child body is its
own return scope and uses an explicit `return` to produce `T`; reaching its end
produces only `unit`.
A child cannot
outlive its scope, become detached, or outlive its source/capability/resource
record. Leaving a scope with an unconsumed task is `E1401_UNJOINED_TASK`.

`join Task<T> -> TaskResult<T>` waits for a child and consumes its handle.
It establishes happens-before from all child actions before completion to
actions after the join. A normal child result becomes `Completed(value)`. A
child whose result type is `Result<T,E>` therefore joins as
`TaskResult<Result<T,E>>`: `Completed(Err(e))` preserves its ordinary error,
and `Cancelled` records task cancellation. There is no implicit conversion
between these two outcomes and no cancellation trap.

`cancel task;` is an idempotent cooperative cancellation request and consumes
no ownership. **cancel alone does not discharge** the task-scope obligation:
the parent still joins and thereby consumes the cancelled task handle. If the
child has already reached normal completion, cancellation has no effect and
join returns `Completed(value)`; otherwise a child that observes the request
at a defined safe point completes as `Cancelled`. The runtime delivers
cancellation only at task creation, explicit cancellation check, `await`,
`join`, channel/event wait, loop back edge, and other verifier-visible bounded
safe points. A task that reaches a safe point after cancellation runs its
registered `defer`/bounded drop cleanup, releases its resource reservation,
and may not start new child tasks after cancellation is observed.

Full-profile `spawn async` is also scoped and produces a `Task<T>`; `await
Task<T> -> TaskResult<T>` consumes it with the same lifecycle as `join`.
Its suspension points are explicit `await` calls to typed runtime contracts.
It does not promise a dedicated worker. A V1 task cannot be detached. Future
unscoped execution requires a new language version and an explicit supervisor,
resource, cancellation, and provenance contract.

## 3. Safe shared-memory rule

Two conflicting accesses to the same non-atomic location, at least one a write,
are a data race unless ordered by happens-before. Safe well-typed TOS Core
cannot construct such a race: affine ownership and borrow rules deny a second
mutable alias; immutable `Shared<T>` grants no mutation; mutable shared state
requires a typed synchronization or atomic contract; tasks may not capture a
mutable borrow. A frontend reports the earliest applicable ownership/capture
error; a verifier rejects forged IR that would violate it. A safe data race is
therefore never undefined behavior, arbitrary memory corruption, or a
backend-dependent outcome.

There is no safe "best effort" race detector mode. An unsafe operation must
preserve the safe caller guarantee stated in docs/40. An execution engine that
cannot implement a stated atomic/happens-before rule must reject the module;
it cannot silently substitute host semantics.

## 4. Synchronization and happens-before

The standard/runtime contracts below are typed, resource-accounted, and
verifier-visible. A future library may add convenience APIs only when it maps
to one of these contracts or a later accepted version.

| Contract | Safe use and ordering |
|---|---|
| `Mutex<T>` | `lock` grants an affine mutable guard; `unlock` releases it. An unlock synchronizes-with the next successful lock of the same mutex. A guard cannot await, cross a task boundary, or be dropped after its lock resource disappears. |
| `RwLock<T>` | Multiple immutable read guards or one affine write guard. Releasing a write guard synchronizes-with a later successful read/write acquisition. Upgrade is not implicit. |
| `Channel<T>` | Sending consumes/transfers `T`; receiving obtains it once. A completed send synchronizes-with the receive of that message. Closing is explicit and receives then return `Err(ChannelClosed)`. |
| `Event` / `Semaphore` | `signal` synchronizes-with a successful `wait` that observes that signal. V1 `Event` is binary/coalescing; `Semaphore` has a declared nonnegative permit count, `release(n)` adds permits within its resource bound, and each successful `acquire` consumes one permit. |
| `Barrier` / `Latch` | A successful barrier generation orders every participant's pre-barrier actions before every participant's post-barrier actions. A latch opens after its declared nonzero count reaches zero and then orders decrements before waiters. |
| task spawn/join | Capture initialization is sequenced-before child entry; child completion is happens-before successful join. |
| cancellation | Cancellation request is visible at a defined safe point. All cleanup/completion actions happen-before the join that observes cancellation. |

An engine MAY serialize any of these operations when that preserves the same
allowed result. It MUST still enforce the lock/guard/resource rules and must
not treat serialized execution as permission for a source program with an
illegal mutable alias.

## 5. Atomics and memory order

V1 provides `AtomicBool`, `AtomicU32`, and `AtomicU64`; all are naturally
aligned opaque objects, never raw integer aliases. They expose:

```text
load(order) -> T
store(value, order) -> unit
swap(value, order) -> T
fetch_add/sub/and/or/xor(value, order) -> T     # integer atomics only
compare_exchange(expected, desired, success, failure) -> Result<T, T>
```

The only order values are `Relaxed`, `Acquire`, `Release`, `AcqRel`, and
`SeqCst`. A load accepts `Relaxed`, `Acquire`, or `SeqCst`; a store accepts
`Relaxed`, `Release`, or `SeqCst`; read-modify-write accepts all; the failure
order of `compare_exchange` accepts `Relaxed`, `Acquire`, or `SeqCst` and may
not be stronger than success. An invalid order is `E1410_INVALID_ATOMIC_ORDER`.

`Relaxed` orders only the atomic modification order of that object. A release
operation synchronizes-with an acquire operation that reads its value or a
later release sequence value. `AcqRel` has both effects. `SeqCst` operations
also participate in one total order consistent with happens-before and each
atomic's modification order. Ordinary reads/writes sequenced-before a release
become visible to ordinary reads/writes sequenced-after an acquire that reads
from it. This is the TOS rule, not an adoption by reference of Rust/C++ or a
host runtime.

Atomicity does not make a non-atomic object safe to mutate concurrently. A
program publishes a non-atomic immutable/initialized value through a release
store and acquire load, a mutex, a channel, task join, or another stated
synchronizer; it does not read/write it concurrently. Atomic operations have
no implicit global fence beyond their declared order.

## 6. Resource declarations and accounting

Each module has exactly one `resource [ ... ]` item. It declares at least:

```text
fuel:        integer,     // maximum interpreter instructions/checkpoints
stack:       size,        // maximum stack bytes per execution context
allocation:  size,        // maximum live allocatable bytes
tasks:       integer,     // maximum simultaneously live scoped tasks
workers:     integer,     // maximum runnable execution contexts requested
sync:        integer,     // maximum live synchronization objects/guards
shared:      size,        // maximum bytes of shared-region grants
cleanup:     integer,     // maximum bounded cleanup steps after cancellation
recursion:   integer,     // maximum dynamic call depth
imports:     integer,     // maximum transitive module dependencies
```

The values are compile-time constants and all maxima are inclusive. A module
may declare stricter named limits, but cannot omit or silently inherit the
required ones. The launcher grants an effective resource envelope no larger
than the declaration. A call/spawn/import is permitted only when the checker
and verifier can establish that its declared worst-case contract fits the
caller envelope. A dynamic allocation/task/worker/synchronization operation
checks the remaining envelope before it takes effect; exhaustion returns the
typed error associated with that operation where one exists, otherwise traps
with a stable `RUNTIME_RESOURCE_*` code. It never silently allocates an
unbounded host thread or heap object.

Missing a required resource key is `E1700_RESOURCE_DECLARATION_REQUIRED`; a
duplicate declaration is `E1703_DUPLICATE_RESOURCE_DECLARATION`; an unknown
key or wrong literal type is `E1704_UNKNOWN_RESOURCE_LIMIT`. The effective
envelope also carries a launcher-granted `cpu_time` duration budget for the
declared service interval. The runtime accounts the sum of CPU time consumed by
all of the process's execution contexts, not elapsed wall time, and refuses to
run further work when that budget is exhausted. Bootstrap fuel is the
deterministic instruction-level counterpart; a Full runtime records and limits
both where policy requires. This makes parallel CPU use accountably bounded
without making correctness depend on a particular scheduler or core count.

Recursive functions require a syntactic `recursion` budget. Bootstrap requires
finite `fuel`, `stack`, `allocation`, `tasks`, `workers`, `sync`, `shared`,
`cleanup`, `recursion`, and `imports`; it accepts `workers: 1` only. Full may
declare more workers, but actual core count is a scheduling choice bounded by
the lower of grant, process policy, and available runtime workers. This is
accounting, not a guarantee of throughput or CPU affinity.

Loop back edges consume fuel in Bootstrap. A verifier-visible loop may have a
statically proven finite bound or consume fuel; an unmetered unknown loop is
`E1701_UNMETERED_LOOP`. Full engines MAY schedule/preempt differently but MUST
honor the module's observable resource limits. No V1 contract requires a
stop-the-world garbage collector; an allocator strategy is internal only if it
preserves the declared allocation and pause/fuel limits.

## 7. Errors, traps, panic, and diagnostics

Recoverable program conditions use `Result<T,E>`. Language/runtime traps are
defined failures of a dynamic language precondition (for example checked
overflow); a panic is a violated implementation/language invariant. Both end
the current process through its supervisor policy. A task reports a typed
`Err` or cancellation when its signature supports it; a trap/panic ends that
task's process and is recorded as a terminal diagnostic.

Every parser, checker, verifier, runtime, or resource diagnostic has:

```text
stable symbolic code
severity (error, warning, note)
stage (lex, parse, type, ownership, effect, resource, IR, runtime)
module name and canonical repository path
source-set identity and normalized source content ID
byte start/end span and derived line/UTF-8-column
structured key=value fields
zero or more ordered causal diagnostics
```

Human wording may improve, but code, stage, primary span, field names, and
causal ordering are stable for V1. A frontend must choose the earliest source
span; at one span it chooses lexical before parse, parse before name/import,
name/import before type, type before ownership/effect, ownership/effect before
resource, then runtime only after successful static validation. The parser
recovery rule in docs/39 may emit subsequent independent errors but cannot
change this primary precedence.

Representative stable code families are `E10xx` lexical/parser,
`E12xx` type/evaluation, `E13xx` ownership, `E14xx` concurrency/atomic,
`E15xx` capability/effect, `E16xx` module/version, `E17xx` resource/profile,
`E18xx` unsafe/FFI, `V20xx` IR verifier, and `RUNTIME_*`/`PANIC_*` terminal
events. A full registry and conformance expectations are in docs/44.

<!-- END docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md -->

---

<!-- BEGIN docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — modules, capabilities, and versioning

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`,
  `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`, and
  `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`

## 1. Module identity and deterministic resolution

Every source begins with exactly one declaration:

```tos
module system.example version 1.0 profile bootstrap;
```

The version is the source-language major/minor version, not a module release
number. For V1, it MUST be exactly `1.0`; any other major is
`E1601_UNSUPPORTED_LANGUAGE_VERSION`, and an unknown minor is
`E1602_UNSUPPORTED_LANGUAGE_MINOR`. A resolver maps module name
`a.b.c` to canonical repository path `a/b/c.tos` relative to a declared module
root in the active source set. A source whose path does not match its header is
`E1603_MODULE_PATH_MISMATCH`.

The resolver input is exactly:

- the selected system commit or accepted detached source-set identity;
- a declared ordered list of module roots and dependency source-set identities;
- the importer module name, requested import, language version, and profile;
- the declared dependency lock/manifest; and
- the effective resource import limit.

It MUST NOT inspect an ambient current directory, host filesystem outside those
roots, network, clock, random source, or undeclared environment variable. An
import never triggers a fetch. Any required fetch is a separate,
source-identified system operation outside the language frontend.

The declared module roots are searched in order, and the candidate in the
earliest root resolves the name. That order settles roots and only roots: it is
what layering a private root over a shared one means, and it makes resolution
deterministic and total. It is not permission to paper over a collision between
declared dependencies, which nothing orders against each other.

An import naming no candidate at all is `E1604_IMPORT_NOT_FOUND`. An import is
`E1605_AMBIGUOUS_IMPORT` when either the declared source set holds more than one
module with the requested name inside one root, so nothing in the set orders
them, or more than one reachable declared dependency source set provides that
name. The two conditions are disjoint, and the diagnostic names the identities
that collided. See ADR-0038.

`import a.b as c;` imports exported types, functions, and constants under `c`.
Without `as`, the final segment is the binding name. Imports are explicit; V1
has no wildcard, relative, implicit prelude, or host-standard-library import.

An imported enum's variant is reached through that binding as a qualified path —
`c.Signal.Low` — in both expression and pattern position. A qualified pattern
path always denotes a constructor and never a binding, and a path naming no
reachable variant is an error rather than a catch-all (ADR-0033). Resolution
uses the same deterministic import closure as any other imported name, so a
variant pattern is reproducible from the module's declarations plus its
closure.
An import graph cycle is `E1606_IMPORT_CYCLE`, including a deterministic ordered
cycle path in diagnostic fields. There is no top-level executable initialization:
items declare types, constants, resources, and functions only. This makes
module loading and cache identity independent of initialization order.

The module resource declaration is `resource [ ... ]`, and a function's
capability-effect declaration is `uses [ ... ]`: both are comma-separated
declarative lists, never executable brace blocks. Their meaning and required
keys remain in docs/40–41.

`pub` exports an item. A non-`pub` item is module-private. A public function's
parameter/return types and effect capabilities must be exported/reachable; an
otherwise private ABI type is `E1607_PRIVATE_PUBLIC_TYPE`.

The rule covers the **transitive public type surface**, not just the outermost
name. A type is reachable when it is primitive or predeclared, imported (and so
reachable at the module that declares it), or a `pub` local nominal type whose
own publicly necessary surface is itself reachable. The publicly necessary
surface of an exported record is the types of its fields, and of an exported
enum the payload types of its variants, because a consumer cannot construct or
match one without naming them. So

```tos
pub record Wrapper [ value: PrivateType ]
pub fn get() -> Wrapper
```

is `E1607_PRIVATE_PUBLIC_TYPE` even though `Wrapper` is itself `pub`. A type
used only inside a function body, or only by a module-private item, is an
implementation detail and is not part of that surface.

`pub` states a public **source-level** interface: the importing module must be
able to name and resolve those types. A module has no binary ABI promise merely
because an item is `pub`; source, IR schema, and runtime compatibility are
governed below. The two are separate — the absence of a binary ABI promise does
not weaken the visibility rule. Permitting a private nominal type in a public
signature would require a model of opaque or private type leakage across a
module boundary, which TOS Core V1 does not define and does not introduce
implicitly.

## 2. Capability declarations, grants, and transfer

Capability imports have the exact form:

```tos
import capability system.time.Clock as clock;
```

This declares that the module may receive one opaque value named `clock` whose
nominal capability type is `system.time.Clock`. It is a request, not a grant.
The process launcher/supervisor, not source text, maps the request to a concrete
grant after policy/trust evaluation. An absent/denied request means module
startup returns the typed launch error `CapabilityDenied`; it is not fabricated
as an absence sentinel, a global singleton, an integer, or a successful empty
authority. (`nil` is not a TOS Core V1 value.)

The imported name can appear only as a value of its declared opaque type, a
function parameter/effect name, or an argument to an operation that requires
that same contract. It cannot be a `const`, record field, serialized value,
numeric conversion, equality key, or deserialized replacement. Constructing or
casting one is `E1502_FORGED_CAPABILITY`. A capability operation is valid only
when the capability type, requested operation/right, resource range, and the
enclosing `uses` effect all match a declared interface contract.

The effective process grant is an explicit finite set of object-specific rights
and resource constraints. A capability can move to one scoped task only if its
interface declares it transferable. Delegation/attenuation is a typed interface
operation: its output rights MUST be a subset of the input's rights, object
scope, and lifetime. No source operation can widen a right, recreate a consumed
linear capability, or transfer a handle by encoding its bits. Authority appears
in process identity, source maps, IR imports, audit logs, and cache identity;
the concrete secret/handle representation does not.

`Region<T>`/`DmaRegion<T>` grants originate only through a capability operation
whose accepted interface declares element type, alignment, access, size, DMA
domain, lifetime, and transfer/share rules. The language V1 contract defines
the nonforgeability boundary; actual PCI/MMIO/IRQ/DMA interfaces belong to
later stages and must be separately versioned. Thus a Stage 2 example can
declare capability intent without pretending that Stage 3/4 services exist.

## 3. Profile compatibility

`profile bootstrap` is a strict, executable subset of `profile full` source
semantics. A Bootstrap module must conform to every Bootstrap restriction and
may be loaded by a Full engine without changing its meaning. A Full module MUST
NOT be silently accepted by a Bootstrap frontend/engine: it reports
`E1702_PROFILE_NOT_SUPPORTED` with the first forbidden feature.

Bootstrap permits the core scalar/aggregate/Result/ownership/capability syntax,
metered loops, `parallel` scopes, `spawn parallel`, `join`, and `cancel`, but
requires the resource bounds in docs/41 and `workers: 1`. It serializes child
task execution in a deterministic order consistent with source creation order
when more than one order would otherwise be observable. It forbids `async fn`,
`spawn async`, `await`, closures, `defer`, `unsafe`, `extern`, dynamic module
loading, a module graph above its declared import cap, and any interface whose
cleanup/allocation/resource bound is absent.

Full permits these constructs only when their typed interface, effect set,
resource declaration, and verifier-visible IR operation are defined. Full does
not remove safe-language constraints: it adds a true SMP-capable execution
path, not a second memory model. Future Full-only standard libraries use a
declared minimum language/profile version and cannot be implicitly pulled into
Bootstrap recovery.

## 4. Language, IR, runtime, and cache compatibility

Language source declares `1.0`. A frontend declares the exact source versions,
profiles, feature set, and conformance revision it implements. It rejects an
unknown language major and rejects any minor feature it does not advertise. A
source has no "best effort" downgrade path. Additive V1 minor extensions must
use a reserved feature declaration and have an accepted contract; they cannot
reinterpret existing token sequences.

For declared language version `1.0`, canonical-source NFC validation uses the
fixed Unicode 17.0.0 / UAX #15 Revision 57 baseline from docs/39 and ADR-0029.
The normalization baseline is selected by language version, never by the host
Unicode database. A future language version that changes it requires an
explicit compatibility decision.

TOS IR has a separate schema ID/version and verifier compatibility range in
docs/43. A runtime reports the language range, IR schema range, verifier ID,
backend ID, target ABI, and execution profile. It MAY accept an older verified
IR cache only when its verifier says the exact schema/source-map/capability
contract is compatible; otherwise it regenerates from canonical source. TOS
does not promise perpetual binary compatibility of IR or native cache objects.

The cache key binds normalized source/dependency identities, source-set
identity, frontend implementation identity, language/profile/feature revision,
IR schema, verifier identity, backend/target ABI, optimization/safety policy,
resource contract, and capability-interface digest. Changing any element
invalidates reuse. Deleting every cache must leave all canonical sources and
their declared dependencies sufficient for recovery/regeneration.

The language version in that key selects its fixed Unicode normalization
baseline. A cache producer cannot substitute a host-dependent normalization
result for the declared source version.

## 5. FFI and external code boundary

V1 reserves `extern` and `unsafe` syntax so the boundary is visible from the
first implementation. It does **not** admit a C ABI, Rust ABI, libc, host
threads, dynamic library loader, or arbitrary native extension as a TOS Core
runtime contract. A frontend written in Rust is an implementation detail; its
Rust FFI is not an FFI available to `.tos` programs.

An accepted future FFI version must define a named interface schema, exact
calling/ownership/region/capability rules, source-map/provenance, target ABI,
resource/cancellation behavior, and safe-call guarantees. An `extern` item
without that accepted interface is rejected by both checker and verifier. It
cannot be enabled by a build flag, host library presence, or unsafe block.

## 6. Module provenance and source maps

The module dependency closure is ordered lexically by canonical module name.
Each member contributes its source-set identity, canonical path, normalized
content ID, declared language/profile version and its Unicode-normalization
baseline, and interface/capability digest
to the frontend/lowering identity. A diagnostic and runtime event identify the
originating source unit and exact byte span. A derived artifact must retain that
mapping across import, lowering, optimization, task spawn/join/cancel, and
runtime failure. Source paths are repository paths, not host paths.

The source set remains canonical even if a derived cache was produced by an
owner-authorized build. An owner may authorize modified source according to
the repository/boot policy; that authorization grants no implicit module
capability and does not make a derived artifact canonical.

<!-- END docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md -->

---

<!-- BEGIN docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — typed IR, verifier, and provenance

- Status: **Accepted Tier 2 contract — production implementation in progress**
- IR semantic schema: `tos-ir/v1`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md` through
  `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`

## 1. Role and representation boundary

TOS IR is a versioned, typed, verifier-visible **derived** representation of
TOS Core source. It is never canonical installed source, a substitute recovery
language, or a promise of permanent binary compatibility. A source frontend
lowers normalized `.tos` source deterministically to `tos-ir/v1`; an
independently built verifier validates that IR before any interpreter, bytecode
engine, native backend, or cache executor uses it.

This document defines the semantic schema. It deliberately does not freeze an
on-disk byte encoding before a production cache exists. Any persisted `tos-ir`
object must, before being introduced, receive a bounded versioned format
specification with magic, schema/encoding version, length limits, canonical
encoding, unknown-field behavior, digest, and parser tests under docs/18. That
format is an implementation/storage detail only if it preserves this semantic
schema and is checked by the independent verifier. The absence of a cache
encoding cannot delay source execution or make a binary cache canonical.

## 2. Module schema

An IR module contains the following logical sections in canonical order:

```text
Header
  schema_id = "tos-ir/v1"
  language_version = "1.0"
  unicode_normalization_baseline = "UCD-17.0.0/UAX15-r57/NFC"
  profile = bootstrap | full
  module name, source-set identity, path, normalized source content ID
  dependency-closure digest, frontend identity, source-map revision
  declared resource envelope and imported capability-interface digest
Types
Imports and exported signatures
Constants
Functions, ordered by fully qualified source name
Source-map entries, ordered by source unit then byte start/end
```

All source strings are normalized UTF-8 according to the language version's
fixed Unicode baseline; for V1 that is UCD 17.0.0/UAX #15 Revision 57 NFC.
Runtime `string` values are not silently normalized. All identifiers/paths
obey docs/39/42.
Tables use explicit bounded indexes; no operation encodes a raw host pointer,
host ABI symbol, implicit global capability, or untyped runtime object.
Every table count, byte length, basic-block count, operand count, nesting
depth, and source-map span is bounded by the module resource contract and the
frontend/verifier hard limits from docs/44.

The type table represents exactly the primitive, nominal aggregate, function,
task, capability, region, synchronization, atomic, and approved constructed
types of TOS Core V1. A nominal type records its defining module content ID and
export name. An IR type ID is not valid merely because its host representation
has the same layout.

The IR does not trust a frontend-supplied `Copy` annotation. It recomputes the
docs/40 rule from the ordered type graph: primitive Copy roots and `Shared<T>`
are Copy; tuple/array types are Copy only when every component is Copy; user
records, user enums, `Option`, `Result`, and `TaskResult` are non-Copy in V1.
All other V1 types are non-Copy. This check is part of affine operand
validation.

For constructed types, IR records the same exact arity as docs/39/40:
`Option`, `Task`, `TaskResult`, `Shared`, `Region`, `DmaRegion`, `Mutex`,
`RwLock`, `Channel`, and `slice` have one type argument; `Result` has two;
`Event`, `Semaphore`, `Barrier`, `Latch`, and the three V1 atomic types have
none, as does `ConversionError`. `array<T, N>` has one type argument and one
compile-time `size` constant. The verifier rejects a forged or mismatched arity
before control-flow or runtime-contract validation.

## 3. Functions, values, and control flow

Each function has an exact type/effect signature, ordered parameters, return
type, source span, maximum declared stack/fuel/cleanup contribution, and a
finite ordered sequence of basic blocks. A block has typed parameters and ends
in exactly one terminator:

```text
return(value?)
branch(target, arguments)
branch_if(condition, true_target, false_target, arguments)
match_enum(subject, complete variant-to-target map)
propagate_error(result)
trap(stable runtime code)
```

Values are typed SSA definitions or explicit affine ownership slots. An operand
can only reference a dominating value/slot under the corresponding ownership
state. There is no implicit fall-through, untyped jump, exception edge, host
stack unwinding, or unbounded recursion edge. A call names a declared imported
or local function signature and supplies an exact ordered operand list; it
cannot resolve a host symbol dynamically.

The frontend lowers every source `name(...)` through one resolved call or
construction family. For a nominal record constructor or named-field enum
variant it first validates the source-order named arguments against the
declared ordered field set, then emits the corresponding ordered aggregate
operands; ordinary functions and tuple variants accept positional operands
only. An IR `return(value)` is the only normal non-unit function/task/closure
result; source blocks, `if`, and `match` do not lower as value-producing
expressions. Each IR function or child/closure body is a return scope. The
lowerer binds source `return` and `propagate_error` to the nearest enclosing
return scope; ordinary IR blocks cannot capture or retarget them. The verifier
rejects a return or propagation edge that crosses that boundary.

The semantic operation families are:

| Family | Required verifier-visible properties |
|---|---|
| constants/aggregate construction | exact type, checked literal range, source map |
| arithmetic/comparison/control | typed operands/results, checked/trap behavior, complete branch targets |
| move/borrow/drop | affine state, borrow exclusivity, bounded cleanup/drop contract |
| Result/error | declared `Ok`/`Err` construction and `?` propagation edge |
| capability | declared imported capability, effect/right/interface match, no construction from scalar data |
| region/DMA | typed grant, rights, checked range/alignment, transfer/share rule, no physical-address exposure |
| resource | reserve/release/check fuel, stack, allocation, task, worker, sync, shared, cleanup, recursion/import bounds |
| async/parallel | scoped spawn, typed captures, affine `Task<T>` token, `TaskResult<T>` await/join result, cancellation request, and scope completion |
| synchronization | typed mutex/RW/channel/event/barrier/latch operation and guard lifetime |
| atomic | exact atomic type, legal operation/order, source map and memory-order contract |
| unsafe/extern | explicit unsafe marker, accepted interface ID, capability/effect/resource contract |

An operation that lowers to a runtime call carries a versioned typed runtime
contract ID and all semantic operands: capability/effect, ownership transfer,
resource reservation, cancellation point, synchronization/atomic order, and
source span. It MUST NOT hide task creation, locking, atomics, shared-memory
access, resource allocation, privilege, or an external host ABI behind an
opaque helper call.

## 4. Lowering boundary

The frontend proves syntactic well-formedness, name resolution, source-level
type/effect checks, lexical ownership checks, profile eligibility, and source
span attachment before it emits IR. Lowering is deterministic: identical
declared inputs yield semantically identical ordered IR tables and mapping
records. The frontend may optimize only when the resulting typed IR preserves
the source evaluation, ownership, capability, resource, atomic, and source-map
semantics.

The verifier does not trust those claims. In particular, the verifier rechecks
all table bounds/schema identity, nominal type references, control-flow targets,
operand types, call/effect signatures, import/capability declarations, affine
value/borrow state, region rights, profile restrictions, resource accounting,
task scope/capture/join/cancel/`TaskResult<T>` behavior, synchronization guard rules, atomic
orders, unsafe interface IDs, and source-map identity/spans. A frontend cannot
mark an arbitrary cache "verified." Only the verifier emits a verified-module
receipt bound to the complete module digest and verifier identity.

## 5. Independent verifier contract

The verifier consumes untrusted IR bytes/in-memory structures plus a declared
module-resolution and capability-interface snapshot. It produces either:

```text
VerifiedModule {
  module digest, schema_id, verifier identity, source/dependency identities,
  profile, effective resource envelope, capability-interface digest,
  checked source-map digest
}
```

or one deterministic primary `V20xx` diagnostic with optional causal entries.
An engine accepts executable IR only with a receipt for the exact module digest,
schema, source/dependency closure, effective resource envelope, capability
contract digest, and engine compatibility range.

Verifier independence is structural: it is a separately buildable component
with its own parser/validation traversal and does not consume a frontend AST,
type-checker success flag, or host compiler validation result as proof. A
shared declarative type/interface table may be used only if its content digest
is input to both components; no frontend callback participates in verifier
acceptance. An alternate/optimized frontend remains untrusted at this boundary.

Primary validation order is:

1. envelope/byte/table-count limits;
2. schema/version/header/source identity;
3. canonical ordering and index/reference range;
4. nominal types/signatures/imports/capability interfaces;
5. control flow and typed operands;
6. ownership/regions/effects/profile/resources;
7. tasks/synchronization/atomics/unsafe contracts; then
8. source maps and cache/provenance binding.

Representative stable errors are `V2001_LIMIT`, `V2002_SCHEMA`,
`V2003_SOURCE_IDENTITY`, `V2004_TABLE_ORDER`, `V2010_TYPE`, `V2011_CFG`,
`V2012_IMPORT`, `V2013_CAPABILITY`, `V2020_OWNERSHIP`, `V2021_REGION`,
`V2022_RESOURCE`, `V2023_PROFILE`, `V2030_TASK_SCOPE`, `V2031_SYNC`,
`V2032_ATOMIC_ORDER`, `V2033_UNSAFE`, and `V2040_SOURCE_MAP`.

## 6. Source maps, cache identity, and observability

Every IR operation has a source-map entry containing source-set identity,
canonical path, normalized source content ID, frontend identity, language
version/profile and Unicode-normalization baseline, byte start/end, and
optional derivation parent span. Spawn,
join, cancellation, synchronization, and atomic operations also carry a task
or execution-context event identity at runtime; timing and CPU number are
observations, not part of source identity.

A derived cache key contains at least:

```text
normalized source-content IDs and ordered dependency closure
source-set/commit or detached source-set identity
canonical path/module identity
frontend implementation and semantic-profile identity
language version and feature revision
Unicode normalization baseline
IR schema and source-map revision
verifier implementation identity
backend implementation and target ABI identity
optimization and safety policy identity
resource-envelope digest
capability-interface contract digest
```

The runtime records this identity with a running component. An identity mismatch
or missing source map rejects cache execution rather than trying a nearby source
or host fallback. Removing all cache objects leaves the canonical source tree,
declared dependencies, frontend/verifier/runtime, and recovery path able to
regenerate functionality, subject only to declared bounded work and time.

## 7. Execution engines and semantic equivalence

The reference interpreter, a future bytecode engine, and a future native/JIT
backend execute the same verified IR and TOS-owned memory semantics. The
Bootstrap interpreter may serialize parallel scopes, but must produce an
allowed result under docs/41 and retain each resource/cancellation rule. A
production-capable Full engine has a real SMP mapping from runnable parallel
tasks to bounded execution contexts. No engine may defer semantics to an
undocumented host ABI or silently give atomics/races different behavior.

Every engine must pass the same relevant conformance vectors. A backend,
including Wasm/LLVM/Cranelift, can only be a derived cache/codegen mechanism
after a separate accepted ADR admits its bounded role. It does not replace the
frontend, verifier, source maps, capability contract, or recovery semantics.

<!-- END docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md -->

---

<!-- BEGIN docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — conformance, limits, and implementation review

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language version: `TOS Core 1.0`
- Governing Tier 1 decision: ADR-0027
- Depends on: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md` through
  `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`

## 1. Conformance model

TOS Core conformance is backend-neutral. A conforming frontend accepts/rejects
the normalized source corpus with the specified primary code, stage, path,
content ID, span, and required structured fields. A conforming lowerer emits
semantically equivalent `tos-ir/v1`; a conforming verifier independently
accepts valid IR and rejects forged/malformed IR; a conforming engine produces
an allowed V1 outcome without relying on host language/ABI behavior.

The initial corpus is retained under `docs/language/conformance/v1/`. It is
accepted source/conformance contract evidence until implementation begins. Each
case has a stable identifier, canonical `.tos` input, profile, expected result
or primary diagnostic, source span, and semantic rationale. An implementation
MUST NOT change an expectation merely because its parser/checker finds a more
convenient error.

| Vector class | Required initial evidence |
|---|---|
| lexical/source | UTF-8, BOM, Unicode 17.0.0/UAX #15 Revision 57 NFC, CRLF/bare-CR, tab, identifier, integer, string/bytes, and earliest-error precedence |
| grammar | module/header/import, declaration/block recovery, parenthesized statement-only `if`/`match`, one Call/constructor form, `[]` declarative lists, named record/named-variant constructors, `fn (...) { ... }` closures, `array<T, N>`, no standalone block expression, precedence, complete match, reserved words, invalid profile syntax |
| type resolution | unknown local type, unknown qualified type where the import and module resolve, `Option` and `Result` applied with the wrong arity, and the precedence of an unresolved name over an arity finding (ADR-0034) |
| static type/evaluation | fixed-width literals, `to_*` checked conversion and invalid narrowing, checked overflow/shift/division, Result `?`, `Option` (not `nil`), evaluation order |
| pattern resolution | local bare unit variant, bare binding where the expected type has no such variant, two enums sharing a variant name disambiguated by expected type, payload variant destructuring, explicitly qualified local variant, qualified imported variant, unknown qualified variant, exhaustive match over bare variants, wildcard and binding exhaustiveness, and independence from capitalization (ADR-0033) |
| ownership | move/use-after-move, primitive/tuple/array Copy and affine nominal aggregate rule, immutable/mutable conflict, borrow escape, indexed alias conservatism, task capture |
| capabilities | undeclared effect, forged handle, denied request, invalid attenuation/transfer, untyped privileged operation |
| resources | missing/invalid required limit, metered loop, recursion/import/task/worker/sync/shared/cleanup exhaustion |
| concurrency | one/2/N-worker equivalent deterministic result, actual Full-engine overlap, safe mutable-share rejection, `TaskResult` join/cancel lifecycle, bounded task/worker behavior |
| synchronization/atomics | mutex/channel/event/barrier ordering, valid/invalid memory order, release/acquire publication, no non-atomic race escape |
| visibility | an exported type in a public signature, an imported exported type across a real module boundary, private types confined to a body or a private item, a private type named directly by a `pub fn`, and a private type reached transitively through an exported wrapper |
| modules/provenance | deterministic import closure, cycle/ambiguity rejection, cache invalidation, source-map preservation through lowering/optimization |
| IR verifier | malformed header/table/order/index/type/CFG/import/capability/region/resource/task/atomic/source-map negatives |
| profiles/unsafe/FFI | Bootstrap reject Full-only constructs, serialized Bootstrap equivalence, unsafe rationale and unavailable FFI rejection |

For Full engines, the required multicore exercise partitions a deterministic
CPU-bound workload. It records 1-worker, 2-worker, and reasonable-N-worker
correct results plus actual overlapping CPU work on multiple host cores. The
same vector runs in serialized Bootstrap/reference mode. Speedup is evidence
of viability, not a selection or correctness score. A negative shared-mutable
case, atomics/synchronization case, structured join/cancel case, and bounded
task/worker case are mandatory; overlap alone is insufficient.

## 2. Frontend, verifier, and runtime hard limits

The production implementation MUST publish exact numeric limits before it
accepts untrusted source/IR. They may be no larger than this accepted V1
ceiling without a contract extension:

```text
normalized source unit             256 KiB
module dependency closure          256 modules
module/import graph depth          64
identifier bytes                   128
string/bytes literal bytes         64 KiB
delimiter nesting                  256
record/enum fields or variants     1024
function parameters                128
diagnostics retained per module    256
IR tables/blocks/instructions      bounded by declared module resource envelope
```

The frontend and verifier check gross byte/count/depth limits before expensive
normalization, graph traversal, type work, lowering, or source-map copying
where structurally possible. A limit error takes precedence over later
semantic errors when its triggering bound is encountered first. Limits prevent
attacker-controlled recursion, quadratic name/module work, unbounded source
duplication, and cache amplification; they are not optional implementation
quality targets.

Any lower cap is allowed if reported in the implementation's declared
conformance profile. Raising a ceiling, changing a rejection precedence, or
accepting a new syntax/IR feature is a versioned contract change with vectors.
The reference parser/verifier remains total over arbitrary bytes and returns
structured errors rather than panicking.

## 3. Required threat and adversarial evidence

This contract extends the existing `docs/34_THREAT_MODEL.md` language/runtime
boundary (T3 malicious frontend/cache producer and T1/T2 resource abuse). It
adds no claim that a language checker defeats malicious firmware, a compromised
nucleus, or all denial of service. Stage 2 implementation evidence MUST cover:

- malformed UTF-8/source and malformed/forged IR fuzzing without parser panic;
- Unicode 17.0.0/UAX #15 Revision 57 NFC conformance, including generated-data
  provenance/hash verification and NormalizationTest.txt-derived cases;
- source normalization/path/import ambiguity and cache-substitution negatives;
- capability forgery/widening/ambient-authority negatives;
- ownership/data-race/atomic-order invalid cases;
- resource exhaustion before allocation/worker creation and cancellation
  cleanup bounds;
- source-map identity forgery/mismatch; and
- cross-engine semantic differential testing for every supported engine.

Evidence levels remain those in docs/34: the accepted documents are E0 design;
implemented parser/verifier paths become E1; automated positives/negatives E2;
fuzz/fault evidence E3. No Stage 2 closure claim may elevate a design contract
without the corresponding implementation evidence.

## 4. Performance and recovery evidence

The Stage 1.5–2 contracts in `docs/35_PERFORMANCE_CONTRACTS.md` apply. The
production reference profile must measure parse/type-check/lower/verify a
256 KiB canonical module and the one-million-operation integer/control-flow
benchmark with the required environment, warmups, raw samples, median/p95/p99,
memory, source/build identity, and cache state. Measurements cannot move work
into a host runtime, native cache, nucleus, or an unchecked frontend to claim a
pass.

The recovery/Bootstrap measurement records source size, parser/checker/verifier
and interpreter binary/component sizes, dependencies, dynamic dependencies,
peak memory, cold start, resource envelope, and all host/build tool identities.
Rust may implement those components, but rustc/LLVM/libc/C ABI/host threads are
not recovery/runtime dependencies unless a future ADR explicitly admits them.
The system must be able to delete all derived caches and regenerate from source
using the declared recovery components.

## 5. Implementability review

The contract makes the following deliberate complexity choices:

| Risk | V1 containment |
|---|---|
| parser ambiguity / error recovery | ASCII identifiers, no indentation semantics, no block comments/macros, deterministic EBNF and fixed recovery tokens |
| pathological source / graph | byte, nesting, identifier, diagnostic, closure and import limits with early rejection |
| type/ownership complexity | nominal non-generic types; lexical nonescaping borrows; affine ownership; conservative indexed aliasing |
| capability forgery | opaque nominal imports, effect checking, no scalar representation, independent IR checks |
| concurrency complexity | no detached tasks; lexical scopes; ownership transfer; typed visible synchronization/atomics; Bootstrap serialization |
| resource amplification | mandatory module envelope, reservation before action, bounded cleanup and worker/task count |
| verifier capture | separate build/traversal, no frontend AST-success trust, typed runtime contracts visible in IR |
| source-map loss | identity/span required in every IR operation/cache receipt, verifier checks |
| future native backend | typed IR and explicit checked/atomic/capability semantics; backend cannot redefine them |

Known non-goals are intentional: V1 has no user generics/traits, textual macros,
reflection, implicit ambient prelude, unscoped tasks, stop-the-world collector,
ordinary C ABI, or Stage 3 IPC/driver service API. Their absence does not mean
the contract is temporary: extensions must be versioned, typed, source-mapped,
resource-accounted, verifier-visible, and compatible with the established safe
memory/concurrency boundary.

## 6. Recommended Part B implementation order

After explicit acceptance of ADR-0028, the production order is:

1. bounded normalized source reader and lexer with lexical vectors/fuzzing;
2. deterministic parser and recovery diagnostics;
3. names/types/effects and stable diagnostic records;
4. affine ownership/borrow and module/resource checks;
5. deterministic lowering to the in-memory `tos-ir/v1` semantic schema;
6. independently buildable verifier and forged-IR negatives;
7. bounded serialized Bootstrap reference interpreter;
8. source maps, cache identity/deletion/regeneration and resource accounting;
9. corpus/fuzz/differential/performance evidence; then
10. execute real `/system/boot/init.tos` only after its source conforms.

This order does not authorize a second implementation path. The first parser,
checker, IR, verifier, and interpreter are the intended long-term reference
components; optimized backends remain subordinate derived engines.

## 7. Diagnostic code registry

This section is the authoritative registry that `docs/41` section 7 refers to.
It enumerates every frontend diagnostic code reachable by the source reader,
lexer and parser, with its stage and the exact condition that raises it. A code
used by a conformance expectation must appear here; the mechanical gate in
`scripts/check-stage2-language-contract.py` enforces that in both directions.

Codes are allocated by the document that owns the rule — `docs/39` for lexical
and grammatical conditions, ADR-0032 for the parser codes it ratified. This
section records them in one enumerable place; it does not create authority the
owning document did not grant.

Human wording may improve. Code, stage and condition are stable for TOS Core 1.0
and change only through a versioned language decision.

<!-- stage2-diagnostic-registry:start -->

### Source transport (stage `lex`)

| Code | Condition |
|---|---|
| `E1000_SOURCE_LIMIT` | the source unit exceeds the 256 KiB ceiling; reported at the first excluded byte, before UTF-8 and NFC work |
| `E1001_INVALID_UTF8` | the input is not valid UTF-8; reported at the first invalid byte, before normalization |
| `E1002_BOM_FORBIDDEN` | the input begins with a UTF-8 byte order mark; reported at byte 0 |
| `E1003_BARE_CR` | a CR appears that is not part of a CRLF pair; reported at that byte |
| `E1004_NOT_NFC` | the input is not NFC under UCD 17.0.0 and UAX #15 Revision 57; reported at the first non-NFC sequence |
| `E1005_NUL_FORBIDDEN` | a NUL scalar value appears in otherwise valid source; reported at that byte |

### Lexical (stage `lex`)

| Code | Condition |
|---|---|
| `E1010_TAB_OUTSIDE_LITERAL` | a horizontal tab appears outside a literal or comment |
| `E1011_NON_ASCII_WHITESPACE` | a non-ASCII whitespace scalar value appears outside a literal or comment |
| `E1012_INVALID_IDENTIFIER` | a non-ASCII scalar value appears outside a literal or comment, where only an ASCII identifier could be formed; reported at its first byte |
| `E1013_UNEXPECTED_CHARACTER` | a valid UTF-8 character outside a literal or comment neither begins nor continues any admissible lexical form at its position, and is not covered by `E1012_INVALID_IDENTIFIER`; reported at its first byte |
| `E1020_INVALID_INTEGER_LITERAL` | an integer literal has an invalid base digit, a leading or trailing underscore, repeated underscores, or an invalid suffix |
| `E1030_INVALID_STRING` | a string literal has an invalid escape, an invalid scalar value, an unescaped line ending, or no terminator |
| `E1031_INVALID_BYTES` | a `bytes` literal contains a character or escape outside the permitted ASCII set, or has no terminator |

`E1012` and `E1013` are mutually exclusive by construction: a non-ASCII scalar
value takes `E1012`, and every other character that begins no lexical form —
necessarily ASCII, such as `@`, `$`, `#`, `` ` ``, `'` or `\` — takes `E1013`.

### Parser (stage `parse`)

| Code | Condition |
|---|---|
| `E1100_EXPECTED_MODULE_HEADER` | a required module-header keyword (`module`, `version`) is absent at its position |
| `E1101_EXPECTED_IDENTIFIER` | an identifier is required at this position and the token present is not one |
| `E1102_EXPECTED_VERSION_COMPONENT` | a module-header version component is not a decimal integer representable as `u32` |
| `E1103_EXPECTED_PROFILE` | the module-header profile is neither `bootstrap` nor `full` |
| `E1104_EXPECTED_LITERAL` | a literal is required at this position and the token present is not one |
| `E1105_CONTROL_HEAD_PARENS_REQUIRED` | an `if`, `while`, `match` or `for` head is not parenthesized |
| `E1106_LIST_SEPARATOR_REQUIRED` | two members of a comma-separated list are not separated by a comma |
| `E1107_UNEXPECTED_TOKEN` | the token cannot begin or continue the construct being parsed and no more specific parser code applies |

### Type and evaluation (stage `type`)

| Code | Condition |
|---|---|
| `E1201_ASSIGN_TO_IMMUTABLE` | an assignment targets a place whose root binding is not mutable |
| `E1202_UNKNOWN_VALUE_NAME` | a value name, or a qualified constructor path in a pattern, resolves to no predeclared value, module item, parameter or in-scope binding |
| `E1203_UNKNOWN_TYPE_NAME` | a type name resolves to no primitive, fixed or predeclared type, local nominal type or reachable imported type; for a qualified name the module or import part resolved first |
| `E1204_TYPE_ARGUMENT_ARITY` | a known parameterized V1 type constructor is applied to the wrong number of type arguments; fields carry the constructor and both arities |
| `E1205_DUPLICATE_RECORD_FIELD` | a named field list declares or supplies the same field name more than once |
| `E1206_MISSING_RECORD_FIELD` | a named constructor omits a field its record or named-field variant declares |
| `E1207_UNKNOWN_RECORD_FIELD` | a named constructor supplies a field its record or named-field variant does not declare |
| `E1222_RETURN_TYPE_MISMATCH` | a `return` carries a value whose type is not the declared result type, or omits a value in a non-`unit` function |
| `E1225_INVALID_DEFER` | a `defer` body performs `return`, `break`, `continue`, `await`, `join`, spawns work, or acquires a new resource |
| `E1210_INTEGER_TYPE_MISMATCH` | a value of one integer type is assigned or passed where a different integer type is required; an unsuffixed literal takes the required type instead |
| `E1211_INDEX_TYPE_MISMATCH` | an array, slice or region index is not of exact type `size`, and is not an integer literal contextually typed as one |
| `E1212_INVALID_AS_CONVERSION` | an `as` conversion between ordinary value types is not an integer widening that preserves signedness; a conversion touching a capability or another nonconstructible type is routed to `E1502` or `E1213` and is not this code |
| `E1213_NONCONSTRUCTIBLE_TYPE` | an `as` conversion whose target or operand type is one V1 source may not fabricate a value of — `Task`, `Shared`, `Region`, `DmaRegion`, `Mutex`, `RwLock`, `Channel`, `Event`, `Semaphore`, `Barrier`, `Latch`, an atomic, a slice, or a function or closure type. `TaskResult<T>` is not among them: `Completed` and `Cancelled` build one. A predeclared type in value position is `E1202`, not this (ADR-0039) |
| `E1215_ARGUMENT_TYPE_MISMATCH` | an argument of a resolved call or predeclared operation does not satisfy the declared exact type or the operation's type requirement, and no more specific code describes it. The residual of `E1210`, `E1211`, `E1212`, `E1213`, `E1502` and `E1222`, never a catch-all for an unresolved callee, which is a resolution finding with precedence. Fields: `callee`, `position` or `parameter`, `expected`, `actual`; an operation requirement may use `requirement` and `reason` instead (ADR-0037) |
| `E1220_NONEXHAUSTIVE_MATCH` | a `match` over an enum, `Option`, `Result` or `TaskResult` leaves a variant uncovered and has no wildcard or binding arm |
| `E1221_MISSING_RETURN` | control can reach the end of a function whose declared return type is not `unit`, or of a closure or spawned body that returns a value on another path |

### Module and version (stage `type`)

| Code | Condition |
|---|---|
| `E1601_UNSUPPORTED_LANGUAGE_VERSION` | the module header declares a source-language major version other than 1 |
| `E1602_UNSUPPORTED_LANGUAGE_MINOR` | the module header declares a minor version the frontend does not implement |
| `E1603_MODULE_PATH_MISMATCH` | a source unit's canonical repository path is not the path its declared module name maps to |
| `E1604_IMPORT_NOT_FOUND` | an import names no module in the declared source set |
| `E1605_AMBIGUOUS_IMPORT` | an import has candidates nothing orders: the declared source set holds the requested name more than once inside one module root, or more than one reachable declared dependency source set provides it. Candidates in different roots are settled by the declared root order and are not ambiguous (ADR-0038) |
| `E1606_IMPORT_CYCLE` | the import graph contains a cycle; the ordered cycle path is a field |
| `E1607_PRIVATE_PUBLIC_TYPE` | a module-private nominal type appears in the transitive public type surface of a `pub` function signature |

### Concurrency (stage `type`)

| Code | Condition |
|---|---|
| `E1401_UNJOINED_TASK` | a task scope is left with a spawned child still unconsumed, or a spawned child's handle is never bound and so can never be consumed; `cancel` is a cooperative request and does not discharge the obligation |
| `E1402_INVALID_GUARD_LIFETIME` | a lock guard leaves the lifetime it is allowed, with a structured `operation` field naming which: `held_across_await`, `returned`, `aggregate`, `channel`, `task_boundary`, or `lock_outlived` (ADR-0036). The finding also carries the guard type and the position where the guard was acquired. A guard crossing a task or closure boundary is reported here and **not** as `E1304_INVALID_TASK_CAPTURE` or `E1305_INVALID_CLOSURE_CAPTURE` |
| `E1410_INVALID_ATOMIC_ORDER` | an atomic operation is given an order it does not accept — a load outside `Relaxed`/`Acquire`/`SeqCst`, a store outside `Relaxed`/`Release`/`SeqCst`, a `compare_exchange` failure order outside `Relaxed`/`Acquire`/`SeqCst`, or a failure order stronger than its success order |

### Capability and effect (stage `effect`)

| Code | Condition |
|---|---|
| `E1501_UNDECLARED_CAPABILITY_EFFECT` | an operation requires a capability whose name is not in the enclosing function's effect set, or a call requires an effect the caller's `uses` set does not include; the `required_by` field names the callee, or `operation` for a direct use |
| `E1502_FORGED_CAPABILITY` | a capability interface is constructed or cast into existence rather than received through its declared import; the `interface` field names it and `operation` says which |

### Unsafe and FFI boundary (stage `effect`)

| Code | Condition |
|---|---|
| `E1801_FFI_NOT_AVAILABLE` | an `extern` item names no accepted FFI interface schema; V1 accepts none, so every `extern` item is rejected |
| `E1802_UNSAFE_RATIONALE_REQUIRED` | an `unsafe` block does not open with a line comment beginning `SAFETY:` |

### Ownership (stage `ownership`)

| Code | Condition |
|---|---|
| `E1301_USE_AFTER_MOVE` | a place is used after its value moved out on some reachable path, by an assignment, an owning argument, a return, placement in an aggregate, a match subject, or a capture; a deferred cleanup body is checked on the exit path that runs it |
| `E1302_CONFLICTING_BORROW` | an operation violates the exclusivity of a live borrow of an overlapping place: a new borrow incompatible with a live overlapping borrow, an owner read or use while a mutable borrow is live, an owner mutation while a mutable borrow is live, or a move or other invalidation while any borrow is live; the `operation` field names which |
| `E1303_MUTATE_WHILE_BORROWED` | a write lands on a place that a live immutable, shared borrow overlaps |
| `E1304_INVALID_TASK_CAPTURE` | a task captures a value that is not `Transferable`: a borrow, a lock guard, a mutable region, a non-transferable capability, or a mutable binding by alias |
| `E1305_INVALID_CLOSURE_CAPTURE` | a closure captures a borrow, a mutable binding by alias, a lock guard, a non-transferable capability, or a plain mutable region |

### Resource and profile (stage `resource`)

| Code | Condition |
|---|---|
| `E1702_PROFILE_NOT_SUPPORTED` | a `profile bootstrap` module uses a Full-profile construct — `async fn`, `spawn async`, `await`, a closure, `defer`, `unsafe` or `extern` — or declares `workers` greater than 1; the first such feature in source order is reported |
| `E1700_RESOURCE_DECLARATION_REQUIRED` | the module resource declaration omits one of the ten required keys of section 6 of docs/41 |
| `E1703_DUPLICATE_RESOURCE_DECLARATION` | a resource declaration is made more than once, whether as a second `resource` item or as a repeated key inside one |
| `E1708_UNBOUNDED_CLEANUP` | a declared type's cleanup has no finite documented bound. V1 source has no drop-contract declaration form (docs/39 section 4), so no V1 module can raise this condition; it is registered for the contract that introduces one |
| `E1701_UNMETERED_LOOP` | a loop has neither a statically proven finite bound nor fuel to meter its back edges: a `while` or bare `loop` in a module declaring `fuel: 0`. A `for` is bounded by the length of the sequence it iterates |
| `E1704_UNKNOWN_RESOURCE_LIMIT` | a resource key is not one of the required keys, or its value is not the literal class that key takes |

<!-- stage2-diagnostic-registry:end -->

`E1107_UNEXPECTED_TOKEN` is the defined residual of the parse stage. A more
specific code always wins where one applies. A recurring `E1107` condition with a
distinct meaning is a reason to allocate a new code through a versioned language
decision, not a reason to keep using the residual.

Lexical diagnostics precede parse diagnostics: a source unit that fails to
tokenize produces exactly one lexical diagnostic and no parse diagnostics. Within
one stage the earliest source span wins, as required by `docs/41` section 7.

### Later-stage families

The `E12xx` type/evaluation, `E13xx` ownership, `E14xx` concurrency/atomic,
`E15xx` capability/effect, `E16xx` module/version, `E17xx` resource/profile,
`E18xx` unsafe/FFI and `V20xx` IR verifier families are defined by
`docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`,
`docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`,
`docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md` and
`docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`. Those documents state each condition;
this registry does not restate them while the checker, verifier and runtime that
raise them are unimplemented.

Each family MUST be folded into the table above — with its stage and exact
condition — by the stage that implements it, before that stage closes. A stage
that raises a code absent from this registry has not met its exit gate. The
stage label for a family is fixed when the family is folded in, not guessed in
advance: `docs/41` section 7 enumerates the stages `lex`, `parse`, `type`,
`ownership`, `effect`, `resource`, `IR` and `runtime`, and assigning families to
them is part of contracting the corresponding checker.

## 8. Open matters outside this proposal

There are no unresolved semantic questions needed to begin the intended
Bootstrap reference implementation if ADR-0028 is accepted. Deliberately
deferred, separately versioned contracts are: persistent IR byte encoding;
concrete Stage 3 capability/IPC/MMIO/IRQ/DMA interface schemas; the exact
future FFI ABI; user generics/traits/macros; detached tasks/supervisor API;
NUMA/affinity API; and bytecode/native backend admission. None is silently
provided by a host implementation, and none blocks Bootstrap source semantics.

<!-- END docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md -->

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

### `/vendor`

External vendor-controlled opaque material: CPU microcode, GPU and peripheral
firmware, and comparable bytes produced outside the project that TOS cannot
express as editable source.

Properties:

- not canonical TOS source and never presented as source;
- not a derived cache — deletion requires reacquisition from the vendor, not
  regeneration from `/system`;
- not part of the system commit, so `/system` rollback does not roll it back;
- identified by vendor, object identity, version and content hash;
- never merged into or mounted inside `/system`.

`/system` may declare a requirement on a vendor object as canonical source text.
The opaque bytes stay here. Firmware is one class inside `/vendor`; there is no
separate `/firmware` root.

This namespace is defined by ADR-0030. No implementation is required
before the stage that first needs physical-hardware firmware.

## Namespace classes at a glance

| Namespace | Class | Deleting it means |
|---|---|---|
| `/system` | canonical source | not possible while active |
| `/work` | source overlay | discards proposals |
| `/config` | configuration | changes machine behavior |
| `/state`, `/home`, `/secrets` | mutable state | data loss |
| `/cache` | derived cache | regeneration only |
| `/run` | ephemeral | nothing |
| `/dev` | capability namespace | not applicable |
| `/vendor` | external material | reacquisition from vendor |

The internal structure of `/system` and the full classification rules are in
`docs/45_SYSTEM_SOURCE_HIERARCHY.md`.

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

<!-- BEGIN docs/45_SYSTEM_SOURCE_HIERARCHY.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Runtime system source hierarchy

- Status: **Accepted Tier 2 contract — implementation deferred to the stage that
  first needs each subsystem**
- Authority on acceptance: Tier 2 under
  `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`
- Governing Tier 1 decisions: ADR-0002, ADR-0031, ADR-0030
- Companion documents: `docs/03_ARCHITECTURE_OVERVIEW.md`,
  `docs/09_FILESYSTEM_AND_STATE.md`, `docs/17_REPOSITORY_LAYOUT.md`

## Status and boundary

This document describes the source hierarchy of a **running TOS installation**.
`docs/17_REPOSITORY_LAYOUT.md` describes the layout of the development
repository and remains authoritative for that purpose. Where the two overlap,
section 2 of this document defines the mapping.

This document defines placement and classification. It does not define module
resolution rules, manifest schema, capability grammar, activation mechanics or
storage format; those belong to `docs/05`, `docs/10`, `docs/12`, `docs/13` and
the versioned interface contracts. No directory described here is required to
exist before the stage that first implements the subsystem it serves.

## 1. Namespace classification

Every path visible to a running TOS installation belongs to exactly one class.
The class determines what deletion means, what rollback means and whether the
content is canonical.

| Class | Meaning | Deletion | Root namespaces |
|---|---|---|---|
| Canonical source | defines system behavior; commit-addressed and read-only | not possible while active; changes require commit and activation | `/system` |
| Source overlay | candidate canonical source, not yet trusted or activated | discards candidates only | `/work` |
| Configuration | machine and deployment configuration | changes machine behavior; versioning model is explicit | `/config` |
| Mutable state | durable data owned by services and users | loses data | `/state`, `/home`, `/secrets` |
| Derived cache | reproducible from canonical source and declared inputs | forces regeneration only | `/cache` |
| Ephemeral | recreated on boot | none | `/run` |
| Capability namespace | mediated handles, not stored bytes | not applicable | `/dev` |
| External material | vendor-controlled opaque material outside TOS ownership | requires reacquisition from the vendor | `/vendor` |

Consequences that follow from the table and are normative:

- `/system` **MUST NOT** contain derived executable artifacts, generated caches,
  mutable state, or vendor-controlled opaque material;
- `/cache` **MUST NOT** contain anything whose loss removes functionality;
- `/vendor` **MUST NOT** be presented as, mounted inside, or merged into
  `/system`, per ADR-0030;
- deleting `/cache` and rebooting **MUST** yield the same system behavior.

## 2. Repository-to-runtime mapping

The development repository subtree `source/system/` is the canonical input for
the runtime `/system` tree. A system commit's `system/` tree becomes the
installation's read-only `/system` when that commit is selected.

The mapping is direct and unrenamed: `source/system/boot/init.tos` in the
repository is `/system/boot/init.tos` in the running installation. A build step
that rewrites, relocates or generates entries between the two would break the
source-to-runtime chain required by I-16 and is not permitted.

Repository directories outside `source/system/` — `boot/`, `nucleus/`,
`crates/`, `interfaces/`, `host-tools/`, `tests/`, `docs/`, `legal/` — are
project development material. They produce the binary trusted base, derived
artifacts and evidence; they are not installed as `/system` content.

## 3. `/system` hierarchy

```text
/system/
    boot/           boot entry source, health requirements, boot policy
    services/       system service modules
    drivers/        user-space device driver modules
    languages/      language frontend modules
    lib/            shared textual modules used by other components
    apps/           applications delivered with the system commit
    shell/          command interpreter and console environment
    ui/             graphical environment source
    policy/         system policy source
    schemas/        versioned IPC, state and interface schema source
    machine/        machine-specific system source
    third-party/    imported textual source with provenance metadata
    lock/           resolved dependency, frontend, schema and vendor locks
```

Every entry is canonical source text. The names are normative at the conceptual
level; exact storage and mount implementation may evolve through ADRs.

### `boot/`

Contains `init.tos` and the boot health requirements referenced by
`docs/04_BOOT_AND_RECOVERY.md`. The capsule copy of `/system/boot/init.tos` and
the repository-backed copy are related through the handoff protocol defined
there; the capsule remains a transport and recovery seed, never a second
installed system.

### `services/`, `drivers/`, `languages/`

Textual components launched as isolated processes under
`docs/10_PROCESS_SERVICE_IPC.md`, `docs/11_DRIVER_MODEL.md` and
`docs/07_LANGUAGE_FRONTENDS.md`. Each component's manifest is declared inside
its own module source, as shown in `docs/11_DRIVER_MODEL.md`; TOS does not keep
a parallel manifest directory that could drift from the code it describes.

### `lib/`

Shared textual modules imported by other `/system` components. A module is
placed here when more than one component depends on it. Placement grants no
authority: a library module holds no capabilities of its own and receives only
what its caller passes.

### `apps/`

Applications delivered as part of the system commit. Applications installed and
owned by a user are not `/system` content and do not appear here.

### `shell/`, `ui/`, `policy/`

Console environment, graphical environment and system policy source. Policy is
canonical text like any other component; it is not a binary configuration
database.

### `schemas/`

Source of record for the versioned boundaries required by I-09: IPC message
schemas, durable state schemas and interface contracts as consumed by the
running system. Schema version identity is part of activation validation under
`docs/13_UPDATE_MERGE_PACKAGE_MODEL.md`.

### `machine/`

System source that applies to a specific machine or hardware profile — for
example a board-specific driver set or platform quirk module. This is source and
therefore lives in the system commit.

Machine *configuration* is not source and remains in `/config` under
`docs/09_FILESYSTEM_AND_STATE.md`. The distinction is: if changing it requires a
source change, commit and activation, it belongs in `/system/machine/`; if it is
deployment data consumed by a component, it belongs in `/config`.

### `third-party/`

Textual source imported from outside the project, retaining upstream metadata,
patch series, provenance and licence records as required by
`docs/27_THIRD_PARTY_COMPONENT_POLICY.md` and
`docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`.

Material here is canonical source: readable, modifiable and rebuildable by the
owner. Material that cannot satisfy that description is not third-party textual
source — it is vendor-controlled opaque material and belongs in `/vendor` under
ADR-0030.

### `lock/`

The resolved lock manifests required by `docs/13_UPDATE_MERGE_PACKAGE_MODEL.md`:
exact dependency identities, frontend versions, schema versions, required
runtime ABI and the identity/version/hash of every required `/vendor` object.

Lock content is generated during update resolution but is **not** a derived
cache: it is committed canonical source, because it records the decisions that
define the system commit. Regenerating it may produce a different result at a
different time, so it cannot be discarded and rebuilt.

## 4. Relationship between `/work` and `/system`

`/work` holds writable overlays with the same shape as `/system`. An overlay is
a proposal, not an installation.

- an overlay path corresponds to the `/system` path it proposes to change;
- overlay content is never executed as system source without explicit
  validation and transactional activation under I-05;
- an overlay may be discarded without affecting the active commit;
- multiple named overlays or branches may exist simultaneously;
- status and diff against the active commit are always available.

Editing source in a running system means editing an overlay and then committing
and activating it. It does not mean mutating `/system`, which is read-only by
class.

## 5. Dependencies on `/vendor`

A `/system` component that requires vendor-controlled opaque material declares
that requirement in its own manifest, alongside its capability requirements, in
the same way `docs/11_DRIVER_MODEL.md` declares device and capability needs. The
declaration names vendor, object identity, version, content hash, expected
`/vendor` placement, compatibility constraints and behavior when the object is
absent, mismatched or refused.

`/system/lock/` aggregates the resolved set for the commit so that the required
external material of a system commit can be listed without traversing every
component.

The opaque bytes never appear in `/system`. The declaration is a reference.
Full rules are in ADR-0030.

## 6. Conformance expectations

When this hierarchy is implemented, architecture conformance tests under
`docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md` must enforce that:

- no path under `/system` resolves to derived-cache, mutable-state or
  vendor-material content;
- deleting `/cache` and rebooting reproduces identical system behavior;
- every running non-nucleus component reports a `/system` source path that
  exists in the active commit;
- an overlay path in `/work` cannot execute as system source without passing
  activation;
- the set of required `/vendor` objects for the active commit is enumerable from
  `/system/lock/` and matches the per-component declarations.

<!-- END docs/45_SYSTEM_SOURCE_HIERARCHY.md -->

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

## Devices requiring vendor firmware

Many real devices require a vendor firmware image before they operate. Under
ADR-0030 that image is vendor-controlled opaque material: it lives in
`/vendor`, it is not TOS source, and TOS makes no claim about its behavior.

The driver does not change class because of it. A TOS driver is canonical
readable source that the owner can inspect and modify, including when its
runtime job is to hand a firmware image to a device. Loading vendor firmware is
an action a textual component performs — never a reason for the component itself
to become opaque, and never grounds for shipping a binary driver in place of a
textual one.

A driver requiring vendor firmware declares it in its manifest alongside its
capability requirements: vendor, object identity, version, content hash and
behavior when the object is absent, mismatched or refused. Refusing to load
unavailable firmware and reporting the device as unavailable is a defined
outcome; operating in an undeclared degraded mode is not.

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
12. local system to remote repositories and time/signature services;
13. canonical `/system` source to external vendor-controlled opaque material in
    `/vendor`.

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

### External vendor material

Threats include substitution of a declared vendor object, downgrade to a
vulnerable firmware version, silent acceptance of a missing or mismatched
object, opaque material shadowing a component required to be textual, and vendor
material being presented to the owner as inspectable TOS source.

Controls are identity-level only: declaration in canonical source with vendor,
version and content hash; hash verification before use; defined behavior on
absent, mismatched or refused objects; the placement rule keeping `/vendor` out
of `/system`; and the owner-facing boundary report required by ADR-0030.

TOS does not analyze what a vendor object does. The controls constrain which
bytes are loaded and whether the owner can see that they were loaded — not their
behavior once running. This limit is stated rather than mitigated, and T7 remains
the governing adversary class.

## Accepted non-goals for early stages

- confidentiality or integrity against malicious firmware;
- verification of the internal behavior of vendor-controlled opaque material;
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

Reference-platform evidence and conformance:

- the mandatory q35/qemu64/one-vCPU/256-MiB/TCG functional profile runs the
  exact ordinary production boot path for a capsule fixture containing 1,000
  files and exactly 16 MiB total payload. It retains raw 3-warmup/21-sample
  median/p95/p99 wall-clock data, serial/event logs and segment decomposition;
  its wall-clock latency is a retained regression metric, not a physical-CPU
  absolute-latency assertion;
- a declared native release/reference profile records the same exact two fresh
  validations and canonical `/system/boot/init.tos` lookup, including raw
  3-warmup/21-sample median/p95/p99 data and environment/build identities;
- each profile also measures the unavoidable SHA-256 baseline with the same
  fixture/source/provenance identity: two parser whole-capsule traversals, two
  loader/nucleus BootInfo-mirror whole-capsule traversals, two cumulative
  per-file traversals, two detached-identity traversals where applicable and
  the post-lookup boot-text digest. No result may be cached or shared between
  logical validators; and
- on the mandatory qemu64/TCG profile,
  full-exact-validation-p95 / unavoidable-crypto-p95 is no more than 1.30.
  This relative gate constrains validation-architecture overhead without
  weakening the required validations or hard architectural budgets.

The former 250 ms threshold was an empirically falsified initial reference
estimate. ADR-0026 records the measurements and rationale; the absolute native
and TCG series remain retained regression evidence.

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

Stage 2 begins with a complete proposed semantic/IR specification and the
single Project Architect acceptance checkpoint for that contract. No production
parser, checker, IR verifier, interpreter, cache, or runtime begins before that
acceptance. Programmer documentation and canonical language examples evolve
with the specification and implementation; they are not end-stage cleanup.

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

Canonical textual system tree, and the canonical input for the runtime
`/system` tree of an installed machine. The mapping is direct and unrenamed:
`system/boot/init.tos` here is `/system/boot/init.tos` there. Its internal
structure is defined by `docs/45_SYSTEM_SOURCE_HIERARCHY.md`; this document
remains authoritative for the rest of the repository.

The tree above is written relative to the implementation root. The implemented
repository nests that root under `source/`, so this subtree is `source/system/`
on disk and `docs/45_SYSTEM_SOURCE_HIERARCHY.md` names it that way.

No generated executable caches or binary packages are committed here.

Vendor-controlled opaque material — CPU microcode, device firmware and
comparable vendor-produced bytes — is never committed into this tree. It belongs
to the runtime `/vendor` namespace under ADR-0030, carries its own
licence and redistribution terms, and is referenced from `system/` only by
declaration: vendor, object identity, version and content hash.

### `tests/performance/`

Versioned workloads, benchmark harnesses, reference-oracle metadata and result schemas from `docs/35_PERFORMANCE_CONTRACTS.md`.

### `tests/architecture/`

Tests enforcing canonical source, provenance, owner control, trusted-base boundaries, repository identity and stage identity gates.

### `docs/adr/`

Accepted ADRs are immutable except spelling/link corrections. Superseding decisions add a new ADR.

### `docs/research/`

Non-normative research records, including language evaluation, patent landscape and name search.

### `docs/language/`

Programmer-facing language guides, learning material, canonical proposed or
implemented `.tos` examples, and conformance inputs. The numbered language
contracts remain the normative source; this tree explains and exercises them.
Every canonical example has an SPDX header and one tracked source of truth.
Before a frontend exists, examples state their proposed/not-implemented status;
afterward, documentation checks bind accepted examples to parser/checker/runtime
evidence rather than maintaining duplicate Markdown snippets.

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

**Vendor-controlled opaque material** — externally produced bytes consumed by hardware that TOS cannot express as editable source, such as CPU microcode or device firmware; identified by vendor, version and hash, never presented as TOS source.

**`/vendor`** — root namespace holding vendor-controlled opaque material, outside the canonical `/system` tree and outside the system commit.

**Namespace class** — the single category a runtime path belongs to — canonical source, source overlay, configuration, mutable state, derived cache, ephemeral, capability namespace or external material — determining what deletion and rollback mean for it.

**Vendor declaration** — canonical source text in `/system` naming a required `/vendor` object by vendor, identity, version, hash, placement and behavior on absence or mismatch; a reference, never an embedded payload.

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

## External opaque vendor material

The roles above all assume material TOS can read: source that can be reviewed,
evaluated, patched and rebuilt. Vendor-controlled opaque material — CPU
microcode, GPU and peripheral firmware, option ROMs — cannot be reviewed as
source, so applying the admission process above to it would produce approvals
with no evidentiary content.

It is therefore a separate class, governed by ADR-0030:

- it is not third-party textual source and does not become a TOS component;
- it lives in `/vendor`, never in the canonical `/system` tree;
- it is admitted by identity, version and content hash, not by source review;
- TOS makes no claim to have inspected or verified its behavior;
- it must never replace or shadow a component TOS architecture requires to be
  textual;
- it carries its own licence and redistribution terms, which do not extend to
  any TOS component and which do not exempt it from the review required by
  `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md`.

Imported material that *can* be read, modified and rebuilt by the owner is
third-party textual source and stays under the rest of this policy.

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

**Status:** non-normative research template required by ADR-0015. Its blocking
requirements implement the Tier 2 language and execution requirements in
`docs/05_TOS_CORE_LANGUAGE.md` and `docs/06_EXECUTION_AND_IR.md`; it does not
independently amend them.

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
12. multiple execution backends cannot disagree silently on semantics;
13. one process can use multiple runnable execution contexts for genuine
    simultaneous multicore work, rather than only separate processes or IPC;
14. safe shared-memory concurrency has defined data-race, synchronization,
    atomic and memory-order semantics rather than undefined behavior or
    undocumented host-runtime behavior;
15. parallel workers, tasks, stacks, shared regions and synchronization
    resources can be bounded and accounted for.

A candidate also fails the multicore requirement if its async runtime only
multiplexes tasks on one execution context, if it requires separate OS
processes and IPC for ordinary CPU parallelism, or if it has no viable path
from the selected semantics to real simultaneous multicore execution.

## Comparative criteria

For each candidate record evidence, not adjectives:

- normative specification size and maturity;
- trusted implementation size and transitive dependencies;
- parser/type-checker/verifier complexity;
- memory safety and unsafe boundary;
- asynchronous, structured-concurrency and structured-parallelism semantics;
- multicore execution model and task-to-thread/core mapping;
- cost of parallel task creation and scalability with worker count;
- safe shared-memory model, synchronization, atomic and memory-order semantics;
- scheduler independence and future affinity/NUMA/topology compatibility;
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
10. report trusted-base and dependency inventory;
11. run a deterministic CPU-bound partitioned workload with one worker,
    two workers and a reasonable N-worker configuration, recording the same
    logical result and actual simultaneous host-core execution when the
    candidate runtime supports it;
12. demonstrate safe handling or rejection of unsynchronized mutable sharing;
13. exercise atomics/synchronization, structured join and cancellation;
14. demonstrate bounded worker/task resource behavior;
15. where a reference/interpreter mode exists, run the same concurrency
    semantics in that mode and record any intentional serialized execution.

For every multicore exercise, record hardware, operating system,
compiler/runtime version, worker count, exact commands, raw measurements and
observed result. No candidate receives credit for a speedup claim without
evidence of actual simultaneous execution where its runtime claims to support
it.

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

## Completed 2026-08-09 evaluation

| Candidate | Blocking result | Evidence |
|---|---|---|
| A — bespoke TOS Core | PASS, accepted selection (ADR-0027) | `stage15/finalists/bespoke-tos-core.md`; common corpus and 1/2/4-worker records |
| B — TOS surface over WebAssembly Threads formal core | FAIL | Wasm supplies validated binary execution/shared memory/atomics, but lacks TOS canonical source, capability, ownership/region, structured task/resource, identity and recovery semantics. Adding them makes TOS the foundation; Wasm remains a possible derived backend. Host-created threads are supporting evidence only. |
| C — adapted restricted Rust | PASS, runner-up | `stage15/finalists/adapted-rust.md`; actual E0451/E0499 negatives and common worker records |
| D — unchanged Rust, Pony, Go | FAIL | Ambient/unsafe/resource boundary; actor-only parallelism; or unsafe-race/capability failures respectively. See `stage15/screening.md`. |

Both passing finalists demonstrate deterministic serial and parallel result,
observed multicore overlap, static/data-race negative handling,
atomics/synchronization, structured join/cancellation and bounded
tasks/workers. The proposed winner is chosen for semantic/TCB/recovery fit, not
speedup.

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

<!-- BEGIN docs/adr/0025-stage1-validation-performance.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0025: Stage 1 validation-performance conformance

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — fixes the reference performance-evidence
  contract and authorizes only semantics-preserving implementation hardening;
  capsule v1, BootInfo v1 and the source-to-runtime trust boundary do not
  change
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

`docs/35_PERFORMANCE_CONTRACTS.md` already requires that a release capsule with
1,000 files and exactly 16 MiB of payload validates and locates
`/system/boot/init.tos` within 250 ms p95 on the declared QEMU CI profile.
ADR-0021 also permits no more than one whole-capsule SHA-256 traversal and one
cumulative per-file SHA-256 traversal for every accepted capsule validation.
Boot ABI v1 requires a loader `TOS.CAPSULE.OK` and a second, independently
validated nucleus `TOS.CAPSULE.OK`.

The approved F-18 measurement observes the host-monotonic serial interval from
`TOS.BOOT.ENTRY` through `TOS.BOOTTEXT.PATH` on the ordinary q35/qemu64/TCG,
256 MiB, OVMF, `isa-debug-exit` harness. It thereby includes both required
validations and canonical boot-text lookup without adding a guest clock/event
or a second boot path.

On source commit `58146971cc26a23b9b0bc1835f84b3e07299a759`, the deterministic
detached 1,000-file / exactly-16-MiB workload produced a complete P1 sample
set: median 2765.027 ms, p95 2842.450 ms, p99 2891.580 ms. Intermediate
timestamps show approximately 1372 ms for loader validation and 1189 ms for
nucleus validation; lookup after the latter takes less than 1 ms. The result
is a conformance failure, not a reason to move the timing boundary, discard a
validation, switch to KVM or silently loosen the budget.

## Decision

The Stage 1 performance conformance run is fixed as follows:

- QEMU machine profile is `q35`, `qemu64`, one vCPU, 256 MiB, ordinary TCG
  (no `-enable-kvm`), with the same OVMF discovery and ESP construction as
  `source/host-tools/qemu-test/run.sh`.
- The measured interval is host-monotonic serial-byte arrival from the sole
  `TOS.BOOT.ENTRY` to the sole `TOS.BOOTTEXT.PATH`. Existing intermediate
  `TOS.*` arrival timestamps are retained only as diagnostic trace evidence;
  they do not expand the Boot ABI.
- The fixture has exactly 1,000 canonical files and exactly 16 MiB
  (16 × 1024 × 1024 bytes) of file payload. It uses an ADR-0018 detached
  source-set identity, existing licence notice and a checked provenance
  sidecar; it is generated below ignored `source/target/`, not committed as a
  binary vector.
- The run records three warm-ups and 21 measured samples. Median is the 11th
  ascending sample; p95 is nearest-rank 20 and p99 nearest-rank 21. It records
  raw samples, exact source commit, QEMU/firmware identities, host CPU,
  virtualization mode, guest profile, Rust build identity and an explicit
  baseline. p95 MUST be no greater than 250 ms.
- A successful result must be P2: the normal QEMU CI workflow runs the exact
  harness and uploads fixture, sidecar, serial/event logs, raw samples and
  report. A local P1 run is diagnostic evidence only.

The implementation may optimize the existing dependency-free, no_std
`tos-hash` scalar SHA-256 and capsule validation flow only if all of the
following remain true:

1. the digest algorithm, capsule bytes, parser structured errors and
   deterministic validation precedence remain unchanged;
2. loader and nucleus each independently perform the accepted whole-capsule
   and cumulative per-file digest checks before their respective
   `TOS.CAPSULE.OK` event;
3. the two logical SHA-256 traversals remain bounded as ADR-0021 requires;
   a fused physical payload read is permitted only when it produces the exact
   same two logical digests and rejection outcomes;
4. no CPU feature requirement, QEMU CPU/profile change, external dependency,
   unsafe implementation, assembly backend or host service enters the trusted
   base; and
5. a failed optimization leaves Stage 1 open and returns to architectural
   review rather than changing the metric or hiding a regression.

## Architecture impact statement

- **Invariants/canonical representation:** I-01, I-02, I-09, I-10 and I-18
  remain intact. Canonical source stays textual and the capsule remains a
  disposable, byte-compatible derived transport.
- **Trusted base/dependencies:** the existing no_std SHA-256 and capsule code
  may be made faster in place. No new dependency, privileged service, CPU
  extension, unsafe block or assembly implementation is authorized.
- **Source-to-runtime/recovery:** Git/detached identity, provenance sidecars,
  owner boot control, recovery and rollback are unchanged. The fixture itself
  is reproducibly disposable.
- **Threat model:** hostile capsule bytes remain fully checked twice across the
  loader/nucleus boundary. No validation outcome, structured error or
  fail-closed path may be removed.
- **Performance/compatibility:** this fixes the Stage 1 evidence profile and
  retains the existing 250 ms reference-platform budget. The compatibility
  profile remains qemu64/TCG; KVM, `-cpu max`, SHA extensions and host-only
  measurements cannot satisfy this gate.
- **Licence/patent:** code remains GPL-3.0-or-later and documentation
  CC-BY-SA-4.0. No imported implementation or patent claim is introduced.
- **Evidence:** SHA known-answer/streaming tests; parser/vector/negative/fuzz
  tests; normal QEMU exit 33; exception paths exit 73; 3+21 raw P2 samples and
  a retained CI artifact with p95 ≤250 ms.

## Consequences

The existing F-18 result is an explicit failure baseline. A scalar-only,
semantics-preserving optimization may prove insufficient; if so, this ADR does
not authorize a shortcut. Any proposal to change the CPU profile, introduce
an accelerated/unsafe backend, reduce independent validation, alter capsule
format or revise the budget requires a separate architect-reviewed ADR.

<!-- END docs/adr/0025-stage1-validation-performance.md -->

---

<!-- BEGIN docs/adr/0026-stage1-validation-performance-metric.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0026: Stage 1 validation-performance metric

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Change level: **Level 2** — revises the Stage 1 performance-conformance
  metric only if accepted; it does not change capsule v1, BootInfo v1, the
  validation algorithm, or either trust boundary
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

The current Tier 2 Stage 1 reference-platform rule in
`docs/35_PERFORMANCE_CONTRACTS.md` requires a 1,000-file / exactly-16-MiB
capsule to validate and locate `/system/boot/init.tos` in no more than 250 ms
p95 on the declared QEMU CI profile.  ADR-0025 accepted an exact ordinary
q35/qemu64/one-vCPU/256-MiB/TCG measurement profile for that rule and recorded
that its first P1 result failed it.  ADR-0025 did not authorize a metric
change.

The Project Architect directed a further, exact-work investigation after the
native implementation also failed the initial estimate.  The result is that
the 250 ms value is an **empirically falsified initial reference estimate**:
it was a deliberately loose first-stage guard against accidental quadratic
validation, but was not based on a measurement of the required cryptographic
work.  It must not be silently deleted or relabelled as passing.

The investigation uses the same deterministic detached capsule throughout:

- 1,000 canonical files and exactly 16,777,216 payload bytes;
- capsule SHA-256
  `d0a61d16997492190f258159f599ae80ca26472856316b7035ceaf98c416da55`;
- workload manifest SHA-256
  `91711071612f350595cbc05b898e1f00550308999b69b5bfba508d4758c38855`;
- detached-source-set identity
  `8415f94824d06f8f68798d7ddf54a37a08a6b1fcae6699e83c3774533f8783cc`.

For each timed sample, the full path is the ordinary production logical work:

1. loader plain whole-capsule SHA-256 for the BootInfo mirror;
2. fresh loader parser validation (whole capsule, every file and detached
   identity);
3. fresh nucleus plain whole-capsule SHA-256 and mirror comparison;
4. fresh nucleus parser validation of the same bytes;
5. canonical `/system/boot/init.tos` lookup and the normal nucleus boot-text
   digest.

No parser output or digest is transferred between logical validators.  The
native runner invokes the production parser/hash implementation directly.  The
QEMU baseline uses the same loader/capsule/ESP/OVMF/q35 profile and an isolated
test-only nucleus artifact; its normal production nucleus hash is checked
unchanged before and after the feature build.

The unavoidable-crypto measurement executes exactly the digest operations in
the list above from fresh `tos-hash` state, comparing every resulting parser
digest to the encoded capsule value.  Its setup parse supplies only a borrowed
structural view and none of that parse's digest results enters either timed
logical validator.  Thus it is not a cached validation result or a benchmark
copy of the cryptographic implementation.

### Exact crypto accounting

For this fixture, both the native and QEMU baseline report exactly
**101,203,198 SHA-256 input bytes** and **2,007 SHA-256 invocations** per
boot.  They consist of:

- 2,000 per-file content digests (1,000 for each independent parser);
- two detached source-identity digests;
- two parser whole-capsule digests;
- two existing loader/nucleus whole-capsule BootInfo-mirror digests; and
- one post-lookup canonical boot-text digest.

The byte count includes every input to those hashes: four capsule traversals,
two full payload traversals, the two ADR-0018 domain/path/digest streams and
the boot-text bytes.  It is therefore the lower bound imposed by the current
accepted semantics, not an optional extra workload.

### P1 measurements at `73d7b423d4e534e405a6abbe7c842e1902cbf099`

Each series used three warm-ups and 21 measured samples; p95 is nearest-rank
20 and p99 is nearest-rank 21.  Raw JSONL, reports, fixture, sidecar and QEMU
serial/event logs are retained under ignored `source/target/` evidence paths
and are reproducible with the commands in the Evidence section.

The evidence host was an Intel Xeon E5-2680 v4 at 2.40 GHz running
`Linux-6.5.0-1mx-ahs-amd64-x86_64-with-glibc2.41`, built with
`rustc 1.97.1 (8bab26f4f 2026-07-14)`.  QEMU evidence used QEMU 10.0.11,
OVMF code SHA-256
`624e06de18b4fa535e90db7160d00d3d07d206422b89999bf1e27d920264e4e0`,
OVMF vars SHA-256
`79091dd4ab5e91d7febac74b02dc7f7ec8891a40150cad37c8836105d833cce0`,
and the declared q35/qemu64/one-vCPU/256-MiB guest.  TCG was selected by
omitting `-enable-kvm`; KVM was an explicitly requested research run.

| Profile | Full exact work (median / p95 / p99 ms) | Unavoidable crypto (median / p95 / p99 ms) | Full / crypto p95 | Crypto share of full p95 |
|---|---:|---:|---:|---:|
| Native release research | 624.801 / 658.231 / 664.737 | 619.957 / 622.467 / 623.287 | 1.057 | 94.567% |
| q35/qemu64/TCG functional profile | 2681.217 / 2766.213 / 2772.794 | 2333.338 / 2395.122 / 2398.142 | 1.155 | 86.585% |
| q35/qemu64/KVM research only | 780.988 / 826.389 / 839.925 | 696.552 / 701.056 / 721.422 | 1.179 | 84.834% |

The KVM row is comparison evidence only: it is neither a replacement CI
profile nor a conformance result.  The ordinary qemu64/TCG p95 is 4.202 times
the native p95 for this corrected exact workload.  Its absolute latency is
materially affected by CPU emulation, while the native result independently
shows that the original 250 ms estimate is not met even without TCG.

The remaining p95 time after unavoidable crypto is 35.763 ms native,
371.091 ms TCG and 125.333 ms KVM.  Crypto consequently accounts for at least
84.834% of full p95 in all three independently measured profiles.  This is
evidence that the measured non-crypto validation architecture is bounded and
small relative to the semantics-required work; it is not evidence that any
validation may be removed.

The fresh TCG full-path serial decomposition at p95 is: loader validation
1497.409 ms; loader post-validation 49.233 ms; handoff transition 0.200 ms;
nucleus validation 1243.351 ms; canonical lookup 0.512 ms; and
post-validation-to-halt 23.147 ms.  These are host-monotonic serial-arrival
intervals, not guest instrumentation or a new Boot ABI event.

## Decision

This ADR replaces the initial absolute 250 ms Stage 1 reference-platform
budget with a paired functional and relative-conformance contract:

1. **Hard architectural budgets remain unchanged.**  Parsing remains bounded
   multi-pass; there is no attacker-dependent recursion or premature
   attacker-proportional allocation; canonical lookup remains bounded;
   ADR-0021 limits and traversal constraints remain in force; and loader and
   nucleus each perform their independent validation.  This ADR does not
   authorize unsafe SHA, handwritten assembly, mandatory SHA extensions, an
   external crypto dependency, a capsule-format change, a fused trust
   boundary, or fewer than two validations.
2. **q35/qemu64/TCG remains mandatory functional conformance.**  It runs the
   exact ordinary production boot path with the existing event ordering,
   structured failures, fixture and serial evidence.  It records full-path
   median/p95/p99 and segment decomposition, but its wall-clock result is no
   longer asserted as representative physical-CPU latency.
3. **Native exact work remains mandatory archived evidence.**  A declared
   native release/reference environment runs the same two fresh validations
   and canonical lookup.  It records the full and unavoidable-crypto absolute
   samples, environment and build identities; it has no invented absolute
   latency threshold in this decision.
4. **The primary Stage 1 validation metric is a p95 ratio.**  On the declared
   mandatory q35/qemu64/TCG profile, `full_exact_p95 /
   unavoidable_crypto_p95` MUST be no more than **1.30**.  Both series must
   have the exact fixture/source/provenance/accounting identity, three warmups
   and 21 fresh measurements.  The baseline may not reuse a digest or parser
   result from either logical validator.
5. **The 1.30 threshold has a semantic interpretation.**  It caps measured
   non-cryptographic validation overhead at 30% of the mandatory digest cost.
   It is not fitted to one passing sample: the independent P1 p95 ratios span
   1.057, 1.155 and 1.179.  The cap is 10.3 percentage points above the largest
   research observation, while still requiring at least 76.923% of the
   measured p95 to be attributable to unavoidable crypto.  The existing 15%
   explanation / 30% block regression policy also applies to retained ratio
   and absolute series baselines.
6. **Evidence remains P2 for closure.**  CI retains raw samples, reports,
   serial/event logs, fixture, checked provenance sidecar, source/build/QEMU/
   firmware/host identities and a segment decomposition.  A local P1 result
   does not close F-18.

This proposal evaluates the cost of the validation architecture after
subtracting neither a check nor a byte that accepted semantics make
unavoidable.  It therefore detects pathological structural overhead without
pretending that one emulator's scalar-SHA wall clock is a physical CPU budget.

## Applied Tier 2 amendment

The following change is applied to `docs/35_PERFORMANCE_CONTRACTS.md`. It
replaces only its former Stage 1 “Reference-platform budget” paragraph; no
other Stage 1 hard budget or the document-wide regression policy changes.

```diff
 Reference-platform budget:

- a capsule fixture containing 1,000 files and 16 MiB total payload validates and locates `/system/boot/init.tos` in no more than 250 ms p95 in release mode under the declared QEMU CI profile.
+ - the mandatory q35/qemu64/one-vCPU/256-MiB/TCG functional profile runs the
+   exact ordinary production boot path for a capsule fixture containing 1,000
+   files and exactly 16 MiB total payload. It retains raw 3-warmup/21-sample
+   median/p95/p99 wall-clock data, serial/event logs and segment decomposition;
+   its wall-clock latency is a retained regression metric, not a physical-CPU
+   absolute-latency assertion;
+ - a declared native release/reference profile records the same exact two fresh
+   validations and canonical `/system/boot/init.tos` lookup, including raw
+   3-warmup/21-sample median/p95/p99 data and environment/build identities;
+ - each profile also measures the unavoidable SHA-256 baseline with the same
+   fixture/source/provenance identity: two parser whole-capsule traversals, two
+   loader/nucleus BootInfo-mirror whole-capsule traversals, two cumulative
+   per-file traversals, two detached-identity traversals where applicable and
+   the post-lookup boot-text digest. No result may be cached or shared between
+   logical validators; and
+ - on the mandatory qemu64/TCG profile,
+   full-exact-validation-p95 / unavoidable-crypto-p95 is no more than 1.30.
+   This relative gate constrains validation-architecture overhead without
+   weakening the required validations or hard architectural budgets.
```

The former 250 ms sentence is retained in ADR-0025 and this ADR as historical
evidence of the falsified initial estimate; it is not erased from history.

## Architecture impact statement

- **Invariants and canonical representation:** I-01, I-02, I-09, I-10 and
  I-18 remain unchanged.  Canonical text, capsule bytes, source identity and
  provenance sidecars are unchanged.
- **Trusted base and dependencies:** no production code, unsafe block,
  assembly, CPU feature or external dependency is introduced.  The baseline
  feature remains test-only and uses production `tos-hash`/capsule logic.
- **Source-to-runtime, recovery and rollback:** the loader/nucleus boundary,
  independent validations, recovery model and rollback remain exactly as
  before.
- **Threat model:** hostile bytes remain fail-closed at both validation
  boundaries.  No error precedence, resource bound, parser property or
  canonical lookup rule is relaxed.
- **Performance contract and compatibility:** this is a Level-2 proposed
  revision of the Stage 1 metric only.  q35/qemu64/TCG remains the mandatory
  functional compatibility profile; KVM is research-only.  The 250 ms rule is
  explicitly falsified rather than silently weakened.
- **Licence and patent:** the proposal imports no code or dependency and has
  no licence or patent effect.
- **Evidence:** production SHA known-answer/streaming tests, capsule/vector
  negatives, precedence and fuzz tests, normal QEMU exit 33, exception exits
  73, fixture/provenance checks, and P2 full/crypto raw series would enforce
  the accepted decision.

## Consequences and review boundary

ADR-0026 supersedes ADR-0025's 250 ms p95 threshold while retaining its exact
q35/qemu64/TCG functional profile and all validation constraints. F-18 remains
a BLOCKER until retained P2 CI evidence satisfies this accepted contract. This
decision does not itself authorize F-21 or Stage 1.5 work.

If later evidence materially fails the relative bound, TOS must profile and
explain the residual overhead.  It may not make the result pass by changing
the QEMU CPU/acceleration profile, deleting validation work or importing an
unreviewed accelerated implementation.

## Evidence reproduction

From a clean checkout of the source commit being measured:

```sh
bash source/host-tools/qemu-test/stage1-native-performance.sh --out source/target/stage1-native-crypto
bash source/host-tools/qemu-test/stage1-performance.sh --out source/target/stage1-tcg-crypto
bash source/host-tools/qemu-test/crypto-baseline.sh --out source/target/stage1-tcg-crypto-baseline
# Optional research only; never substitutes for the preceding TCG commands.
bash source/host-tools/qemu-test/stage1-performance.sh --accel kvm --out source/target/stage1-kvm-crypto
bash source/host-tools/qemu-test/crypto-baseline.sh --accel kvm --out source/target/stage1-kvm-crypto-baseline
```

The report helper checks the 3+21 shape, matching source/workload/provenance
identity and exact byte/hash accounting before it emits each ratio.  P2 CI
would retain the named reports and their raw JSONL rather than copying them
into a mutable document.

<!-- END docs/adr/0026-stage1-validation-performance-metric.md -->

---

<!-- BEGIN docs/adr/0027-language-foundation-selection.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0027: Select bespoke TOS Core language foundation

- Status: Accepted
- Date: 2026-08-09
- Decision level: 3 — canonical language semantics, verifier and runtime trust boundary
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Decision

Select a **bespoke TOS Core foundation**, not an unchanged existing language or
an external execution core. The selected boundary is:

- canonical installed programs are normalized UTF-8 human-readable `.tos` text;
- TOS owns lexical/syntactic/type/effect/ownership/concurrency semantics;
- a versioned typed TOS IR is a disposable derivative, validated independently
  before execution;
- the verifier checks types, capabilities, region/ownership rules, resource
  declarations, source maps, structured async/parallel operations, atomics and
  memory-order contracts;
- the bootstrap profile is a bounded serialized execution profile of these same
  semantics; full profile adds bounded structured parallel execution;
- the reference interpreter may serialize parallel tasks, while a
  production-capable backend/runtime must execute them simultaneously on SMP;
- TOS parser, checker, verifier, bootstrap interpreter and minimal task runtime
  form the future language trusted base. rustc, LLVM, libc, C ABI, host thread
  APIs and external VMs are build/research tools unless a later ADR separately
  admits a narrowly defined role.

Rust remains permitted as an implementation language and host build tool under
existing policy. This decision does not require immediate self-hosting. LLVM,
Cranelift and Wasm MAY later be admitted by separate ADRs only as disposable
codegen/cache backends; none becomes canonical semantics by this decision.

## Stage 1.5 selection boundary / Stage 2 specification boundary

Stage 1.5 fixes TOS-owned canonical source authority; normalized UTF-8 `.tos`;
typed disposable IR and independent verifier; capability-safe type/effect and
ownership/region direction; no safe-language data-race UB; TOS-owned
atomic/happens-before direction; structured async and multicore parallelism;
bounded bootstrap as the same semantics; address-width independence; and no
rustc/LLVM/libc/C ABI/host runtime contract. Stage 2 defines the complete
normative grammar, detailed static/dynamic/evaluation/overflow/borrow/error
rules, module algorithm, exact atomic model, FFI and versioning within those
accepted boundaries.

This ADR authorizes a Level 0 reconciliation of docs/05's stale phrase
“selection ADR must define or adopt” to “selection ADR MUST establish
the Stage 1.5 semantic boundary; Stage 2 MUST define the complete normative
specification within it.” docs/16 remains unchanged in substance: Stage 2 owns
the normative lexical/syntax/semantic specification.

## Rationale and alternatives

The completed matrix and common 13-case corpus show the bespoke model can state
capability non-forgeability, source maps, bounded resources, safe mutable-share
rejection, atomics, join/cancel and 1/2/4-worker semantics explicitly. The
adapted Rust runner-up demonstrates useful ownership and compiler rejections,
but its necessary restriction/runtime/verifier layer recreates the TOS semantic
boundary while retaining incomplete upstream memory-model and recovery/host ABI
risks. WebAssembly can provide typed validated binary execution, shared memory
and atomics, but not TOS canonical-source semantics, capability authority,
ownership/region safety, structured task/resource model, source identity or
recovery-language semantics. Supplying these through a TOS frontend, IR,
verifier and runtime leaves TOS—not Wasm—as the semantic foundation; Wasm is a
possible derived backend/cache. Host-managed thread creation is supporting
evidence that task/resource policy remains TOS responsibility. Pony's actor-only model conflicts with direct
parallel task requirements; unchanged Rust and Go fail their ambient/unsafe or
safe-race/resource boundaries.

## Impact statement

The decision preserves I-01, I-02, I-07, I-10, I-11, I-12, I-16, I-18 and
I-19. No persistent format, boot ABI or existing Stage 1 trusted code changes.
Derived IR/caches remain regenerable and source-addressed. Stage 2 will first
write the normative semantics and a bounded bootstrap frontend/verifier, with
conformance/fuzz/resource tests before a production runtime. Licence remains
GPL-3.0-or-later for official implementation; public schemas/conformance may be
explicitly Apache-2.0. No patent-freedom claim is made.

## Evidence and limitations

Evidence is retained under `docs/research/stage15/`, including raw 3+21
measurements, primary references, screening and both finalist prototypes. It is
not Stage 2 code. The selected approach's main risk is the still-unimplemented
complexity of complete ownership, diagnostics, resource accounting and multiple
engines; acceptance authorizes Stage 2 to implement those contracts, not to
skip them.

<!-- END docs/adr/0027-language-foundation-selection.md -->

---

<!-- BEGIN docs/adr/0028-tos-core-v1-semantics-and-ir-contract.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0028: TOS Core V1 semantics and IR contract

- Status: Accepted
- Date: 2026-08-09
- Decision level: 2 — versioned language/IR contract within ADR-0027's
  accepted Level 3 foundation boundary
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

ADR-0027 selected bespoke TOS Core and fixed its language/trust boundary. It
explicitly assigned the complete lexical, syntactic, type/effect, ownership,
concurrency, module, resource, diagnostic, IR, verifier, and compatibility
contract to Stage 2. Implementing a parser first would make its incidental
choices normative and would risk restoring a hidden Rust/LLVM/libc/C ABI/host
runtime contract.

The accepted numbered specification set is:

- `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`;
- `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`;
- `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`;
- `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`;
- `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`; and
- `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`.

They are one V1 contract: splitting prose does not split authority or allow an
implementation to select only convenient portions.

This resubmission resolves the checkpoint's internal-contract findings without
changing the ADR-0027 foundation: it makes every V1 type form and constructor
arity expressible in grammar; gives control heads an explicit parenthesized
boundary from record initializers; fixes record field-list separation; removes
`nil` as an absence syntax; inventories every identifier-shaped grammar word;
and defines cancellation as a request followed by a consuming
`TaskResult<T>` join/await lifecycle. The companion mechanical consistency gate
checks these boundaries across docs/39–44, canonical examples, and the
conformance corpus.

The final syntax correction deliberately replaces the prior proposed
tail-value surface model. It makes `()` parameters/arguments/grouping, `[]`
declarative and data lists, and `{}` executable statement bodies. `return`
is the sole normal value-return operation. `if` and `match` are statement-only,
record construction uses named constructor arguments, and all calls and
constructors still have one parse family. It also retains fixed `to_*` checked
conversion calls and fixes V1 Copy to primitive roots plus structural tuple and
array Copy, with user records/enums affine. These are Level 0 consistency and
learnability corrections inside the proposed V1 semantic direction, not a new
language foundation or Part B authorization.

## Decision

Accept TOS Core V1 as specified by docs/39–44:

- canonical source is normalized UTF-8 NFC/LF `.tos`, bound to source-set,
  path, and SHA-256 source-content identity;
- grammar is deterministic EBNF with explicit parser recovery and no macros,
  ambient imports, pointer syntax, or target-dependent integer defaults;
- tuple types and borrowed `slice<T>` are explicit V1 forms; all predeclared
  synchronization/atomic types have fixed documented arity; control heads are
  parenthesized and record fields are comma-separated so parser boundaries do
  not depend on type resolution;
- `()` denotes grouping/parameters/call arguments, `[]` declarative/data lists,
  and `{}` executable statement bodies; a non-unit function/task/closure body
  returns only through explicit `return`; `if`/`match` are statement-only;
  function and constructor calls share one syntactic Call form; nominal records
  and named-field enum variants use exact named constructor arguments; closures
  use `fn (...) { ... }`; fixed arrays use `array<T, N>`; checked integer
  conversion uses fixed `to_*` calls; and only primitive roots plus structural
  tuples/arrays Copy; plain executable blocks are never expressions; and
  return targets the nearest function/closure/spawn return scope;
- static semantics provide nominal types, fixed-width arithmetic, typed
  Result-style errors, capability effects, affine ownership, lexical
  nonescaping borrows, typed regions, and no safe raw-pointer/physical-address
  escape;
- Full execution has structured async and true-SMP-capable structured parallel
  tasks; `join`/`await` consume `Task<T>` into `TaskResult<T>`, so cooperative
  cancellation never conflates with a child `Result` value; Bootstrap is a
  bounded serialized subset of the same semantics;
- safe data races are statically excluded and independently verifier-rejected;
  atomics, synchronization, cancellation, happens-before, and resource
  accounting have TOS-owned semantics;
- module/import resolution is source-set-bound and deterministic; capabilities
  are opaque requests/grants and cannot be forged or widened by source;
- typed `tos-ir/v1` is derived/disposable, verifier-visible, source-mapped,
  and independently validated before execution; and
- diagnostics, provenance/cache identity, conformance, limits, fuzzing,
  performance, and recovery evidence are specified before implementation.

Acceptance authorizes the Part B production reference frontend/verifier/runtime
work in the order stated by docs/44. It does not authorize Stage 3, a C/Rust
FFI, a host runtime semantic shortcut, a persistent IR cache byte format, an
optimized backend, user generics/macros, or a new dependency.

## Architecture impact statement

- **Invariants:** preserves I-01 canonical text, I-02 minimal binary base,
  I-07 explicit authority, I-09 versioned boundaries, I-10 deterministic
  identity, I-11/I-16 observability, I-12 no hidden runtime build dependency,
  I-18 derived provenance, I-19 dependency containment, and I-21 no temporary
  identity debt.
- **Canonical representation:** normalized `.tos` source remains canonical;
  AST/IR/cache/native code remain disposable derivatives.
- **Trusted base:** defines the future TOS parser/checker, independent
  verifier, Bootstrap interpreter, and minimal task runtime; no external
  runtime enters it. Rust remains an implementation/build language only.
- **Source-to-runtime and recovery:** exact source-set/path/content identity
  flows through typed IR, verifier receipt, cache key, diagnostics, and runtime
  events; cache deletion permits source regeneration through bounded recovery
  components.
- **Threat model:** elaborates existing docs/34 language/frontend/cache boundary
  with bounded parsing, forged IR/capability, resource, race, source-map, and
  cache-substitution negative evidence.
- **Performance:** applies docs/35 Stage 1.5–2 parse/check/lower/verify and
  Bootstrap execution measurements without weakening any established budget.
- **Compatibility:** establishes V1 source/profile/IR/verifier versioning and
  rejects unknown versions rather than guessing.
- **Dependencies/licensing/patents:** adds no dependency or external code;
  documents remain CC-BY-SA-4.0, canonical examples GPL-3.0-or-later, and no
  patent-freedom claim is made.
- **Tests:** docs/44 and `docs/language/conformance/v1/` specify backend-neutral
  positives/negatives, forged-IR, multicore, resource, source-map, fuzz, and
  performance evidence before Stage 2 closure.

## Consequences and alternatives

The reference implementation must implement the whole contract incrementally;
it cannot call a host parser/runtime and call that TOS semantics. The narrow
V1 omissions deliberately constrain the first implementation, but all future
extensions remain versioned, verifier-visible, source-mapped, and
resource-accounted.

Keeping grammar/ownership/atomics unspecified until parser code exists was
rejected because it would violate ADR-0015/0027 and make implementation the
de facto language authority. Adopting Rust, Wasm, LLVM, C ABI, libc, or host
threads as the contract was rejected by ADR-0027; they remain possible future
implementation/build/backend tools only under their own accepted decisions.

<!-- END docs/adr/0028-tos-core-v1-semantics-and-ir-contract.md -->

---

<!-- BEGIN docs/adr/0029-tos-core-v1-unicode-normalization-baseline.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0029: TOS Core V1 Unicode normalization baseline

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-09
- Decision level: 2 — fixes the versioned TOS Core 1.0 source-identity and
  normalization contract within ADR-0028's accepted language boundary
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-09

## Context

ADR-0028 and docs/39 require canonical `.tos` source to be UTF-8, LF, and
Unicode NFC. Unicode is deliberately permitted in comments and string data,
although identifiers remain ASCII-only. The earlier wording did not pin the
Unicode data against which NFC is determined. Letting the host OS, libc, ICU,
Rust, Python, locale, or a newer Unicode release choose that data would make
canonical-source acceptance and `E1004_NOT_NFC` implementation-dependent.

Because normalized source bytes participate in source-content identity, module
resolution, source maps, IR receipts, and cache keys, this is a Level 2
determinism gap rather than a library-selection detail.

## Decision

TOS Core 1.0 uses exactly this Unicode normalization baseline:

```text
Unicode Standard:                 17.0.0
Unicode Character Database:       17.0.0
Normalization specification:      UAX #15, Revision 57
Normalization form:               NFC
```

After the existing CRLF-to-LF transport normalization, canonical `.tos` input
MUST be valid NFC under that exact baseline. This preserves the existing
ASCII-only identifier grammar, Unicode-permitted comments/string data, and
`E1004_NOT_NFC` diagnostic. It does not normalize runtime `string` values.

The reference frontend MUST derive its normalization data reproducibly from
the accepted UCD release. It MUST NOT take host Unicode tables, locale, ICU,
libc, Rust/Python library release, or a newer Unicode release as semantic
authority. A host tool may assist generation only when the generated result is
independently pinned to this baseline.

Before the frontend enables the generated tables, its checked-in provenance
record MUST state the Unicode/UCD/UAX versions, exact upstream UCD files,
their integrity hashes, and the generator identity/version. The required
inputs are the minimum applicable subset of `UnicodeData.txt`,
`CompositionExclusions.txt`, `DerivedNormalizationProps.txt`, and
`NormalizationTest.txt`. Any imported or generated material follows the
third-party licence, notice, provenance, and reproducible-build requirements.
No runtime Unicode-library dependency is admitted by this ADR.

The conformance corpus MUST cover NFC acceptance, decomposed and
combining-order rejection in comments and strings, ASCII byte identity, UTF-8
precedence before normalization, and sufficient NormalizationTest.txt-derived
positive/negative cases to prove the fixed baseline. The same normalized source
bytes MUST result in the same source-content identity independently of host
Unicode version.

Unicode normalization data is part of the TOS Core 1.0 language contract. A
future Unicode/UCD baseline cannot silently alter V1 acceptance or identity;
it requires an explicit versioned language and compatibility decision. An
implementation supporting multiple language versions selects the normalization
baseline from the declared TOS language version, never from the host.

## Architecture impact statement

- **Invariants/canonical representation:** canonical human-readable source and
  its source-content identity remain unchanged; this fixes which NFC predicate
  determines those existing bytes.
- **Trusted base/dependencies:** no runtime dependency or host Unicode service
  enters the frontend, verifier, recovery path, or TOS semantic contract.
- **Source-to-runtime/recovery:** source maps, IR receipts, module closure and
  disposable-cache identities now bind an explicit normalization baseline;
  deleting caches still regenerates from canonical source.
- **Threat model:** hostile source cannot select a locale or host Unicode table
  to change validation; malformed UTF-8 precedes normalization; bounded input
  limits remain in docs/44.
- **Performance/compatibility:** the accepted 256-KiB source bound remains;
  normalization data and conformance costs are measured as part of Stage 2,
  not delegated to a host library. TOS Core 1.0 is permanently compatible with
  Unicode 17.0.0/UAX #15 Revision 57 only.
- **Licence/patent:** no material is imported by this documentation decision.
  Future UCD inputs/tables require exact licence/notice/provenance records
  under docs/22, docs/27, and docs/28 before use.
- **Evidence:** deterministic Unicode-17 normalization vectors, generated-data
  provenance/hash checks, source-identity equality across host environments,
  and malformed-UTF-8 precedence tests are required before frontend closure.

## Consequences

The first Stage 2 source reader/lexer must implement or use only reproducible,
version-pinned Unicode 17.0.0 NFC data. It may not claim a partial ASCII-only
normalizer as TOS Core V1 conformance. ADR-0028 remains accepted and its
language foundation, grammar, ownership, IR, verifier, and runtime decisions
are not reopened.

<!-- END docs/adr/0029-tos-core-v1-unicode-normalization-baseline.md -->

---

<!-- BEGIN docs/adr/0030-external-vendor-opaque-material.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0030: External vendor-controlled opaque material and the `/vendor` namespace

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 3 — introduces a root namespace, a trust boundary and a
  declared dependency direction between canonical `/system` source and external
  material that TOS does not control
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

TOS states that human-readable source text is the canonical installed form of
its non-nucleus executable components. On real hardware this claim meets
material that TOS cannot make textual and cannot rewrite:

- Intel and AMD CPU microcode updates;
- GPU firmware images;
- Wi-Fi, Bluetooth, NIC, storage-controller and embedded-controller firmware;
- device option ROMs and platform firmware payloads.

These are produced, signed and versioned by hardware vendors. They are loaded
by, or on behalf of, the machine, and their internal content is not source in
any sense the owner can act on.

Existing documentation handles this only by omission or by vague phrasing.
`docs/00_PROJECT_CHARTER.md` spoke of "every non-firmware component" without
defining what firmware is architecturally. `docs/17_REPOSITORY_LAYOUT.md` said
"firmware blobs, if supported, are separate and explicitly licensed" without
naming where they live or how `/system` may depend on them.

Two failure modes follow from leaving this undefined. TOS could imply that a
conforming machine contains no opaque binary material, which is false on every
current platform and would make the project dishonest under I-15. Or opaque
vendor material could quietly accumulate inside the canonical textual system
tree, which would erase I-01 component by component while every individual step
looked pragmatic.

The honest position is a stated boundary rather than either denial or drift.

## Decision

### 1. Ownership scope

TOS owns the TOS software layer. TOS does not claim ownership, authorship or
control of vendor-produced material executed by CPUs and peripheral devices.

### 2. Vendor-controlled opaque material

A unit of external material is **vendor-controlled opaque material** when all of
the following hold:

- it is produced and versioned outside the TOS project;
- it is consumed as bytes by hardware or by a hardware-facing loading path;
- TOS cannot express it as canonical source text that the owner may edit,
  rebuild and run;
- it is not the definition of any TOS component.

Vendor-controlled opaque material **MUST NOT** be presented as canonical TOS
source. The system **MUST NOT** display, describe or record it as open,
readable or modifiable material. TOS **MUST NOT** claim to have inspected,
verified or understood its internal behavior. It is identified, located,
version-pinned and hashed; it is not interpreted.

### 3. `/vendor` namespace

External material lives in a dedicated root namespace:

```text
/vendor/
    firmware/
        intel/
        amd/
        nvidia/
        ...
```

`/vendor` is not part of the canonical `/system` tree and **MUST NOT** be
merged into, mounted inside or presented as part of it. Firmware is one class
inside `/vendor`; a separate root `/firmware` namespace is therefore not
introduced.

`/vendor` is its own namespace class, distinct from canonical source, mutable
state and derived cache. It is not derived — deleting it does not regenerate it
from canonical source — and it is not canonical TOS source.

### 4. Declared dependency direction

`/system` **MAY** declare that it requires a vendor object. The declaration is
canonical source text in `/system` and states at least:

- vendor and object identity;
- version;
- content hash;
- expected placement under `/vendor`;
- compatibility constraints;
- policy for absence, mismatch and refusal.

The opaque bytes themselves **MUST** reside under `/vendor`. A declaration is a
reference, never an embedded payload. Dependency flows in one direction only:
canonical source may name external material; external material never names,
selects or alters canonical source.

A TOS component **MUST** behave in a defined way when a declared vendor object
is absent, has a mismatched hash, or is refused by policy. Silent degradation is
not a defined behavior.

### 5. No opaque substitution of textual components

A component that TOS architecture requires to be textual **MUST NOT** be
replaced, shadowed or superseded by vendor-controlled opaque material. A
user-space driver written in TOS Core remains canonical readable source that the
owner can inspect and modify, including when that driver's runtime job is to
hand a firmware image to a device.

Loading vendor firmware is an action performed by a textual TOS component. It is
not a substitute for one.

### 6. Visible boundary

The owner **MUST** be able to determine, for the running system, which
components are canonical TOS source and which are external opaque vendor
material. For each vendor object the system reports vendor, object identity,
version, content hash, provenance record, licence or redistribution status, and
current status (required, present, absent, mismatched, refused).

This report is an ordinary owner-facing system capability, not a debugging
facility. A machine that cannot answer the question does not satisfy this
decision.

### 7. Licence and redistribution

Vendor-controlled opaque material carries its own licence and redistribution
terms and **MUST NOT** be treated as covered by TOS project licences. Its
presence in a TOS installation does not make it a TOS component, and its terms
do not extend to any TOS component. Redistribution requires the review already
required by `docs/22_LICENSING_COPYRIGHT_AND_REUSE.md` and
`docs/27_THIRD_PARTY_COMPONENT_POLICY.md`.

### 8. Scope of this decision

This ADR defines an architectural model. It authorizes no implementation, no
loading path, no storage format and no firmware redistribution. `/vendor` has no
required implementation before the stage that first needs physical-hardware
firmware. Concrete declaration schema, storage format, verification path and
loading mechanism require their own versioned contracts under I-09.

## Relationship to system invariants

This decision does not amend `docs/02_SYSTEM_INVARIANTS.md` and requires no
Level 4 identity amendment.

I-01 governs "every non-nucleus executable component" — that is, every component
*of TOS*. Vendor-controlled opaque material is by this decision's definition not
a TOS component, was never canonical TOS source, and does not become so by being
present on the machine. This ADR states an existing scope boundary explicitly
instead of leaving it to be inferred.

The decision strengthens rather than weakens I-01 in practice: without a named
boundary, opaque material has no defined place and tends to accumulate inside
the canonical tree. Section 5 makes that specific failure a stated violation.

Related invariants:

- **I-15 honest compatibility** — TOS states plainly that opaque vendor material
  exists on real hardware rather than implying a fully textual machine;
- **I-16 source-to-runtime traceability** — traceability continues to apply to
  TOS components; vendor objects are identified, not traced to source;
- **I-17 owner-installable modification** — unaffected, because the textual
  components the owner modifies remain textual under section 5;
- **I-19 external dependency containment** — extended with a class that is
  contained by placement and declaration rather than by review-for-admission,
  since it cannot be reviewed as source;
- **I-20 legal continuity of openness** — section 7 prevents vendor terms from
  bleeding into TOS components.

## Architecture impact statement

- **Change level:** 3.
- **Invariants affected:** none amended; I-01, I-15, I-16, I-17, I-19 and I-20
  are scoped explicitly as described above.
- **Canonical representation after the change:** unchanged. `/system` remains
  canonical text. `/vendor` is explicitly not canonical TOS source.
- **Trusted-base impact:** no dependency enters the loader or nucleus. A new
  trust boundary is named: canonical source to external opaque material.
- **Source-to-runtime impact:** the identity plane gains a second, weaker
  answer class — vendor objects are reported by identity/version/hash, never by
  source path and never as verified behavior.
- **Recovery and rollback impact:** `/vendor` is not part of the system commit,
  so rollback of `/system` does not roll back vendor material. Declarations in
  `/system` carry version and hash, so a rolled-back commit states which vendor
  objects it expects. Absence must be a defined, recoverable state.
- **Stage identity gate:** no stage gate is claimed or closed. The model applies
  from the first stage that touches physical-hardware firmware.
- **Threat-model impact:** TOS does not claim confidentiality or integrity
  against malicious firmware — an existing accepted non-goal in
  `docs/34_THREAT_MODEL.md`. This decision adds the boundary and requires that
  the owner can see it, which is a reporting requirement, not a protection claim.
- **Performance contract:** none applicable; no measured path changes.
- **Compatibility profile:** none claimed. No hardware support is asserted.
- **New dependencies:** none. The decision is documentary.
- **Licence and patent impact:** section 7 keeps vendor terms separate. No
  material is imported by this decision.
- **Tests that enforce the decision:** deferred to the implementing stage. When
  `/vendor` is implemented, architecture conformance tests under
  `docs/31_ARCHITECTURE_CONFORMANCE_TESTS.md` must enforce that no vendor object
  is reachable as `/system` content, that a declared-and-absent object produces a
  defined failure, and that the owner-facing boundary report is complete.

## Consequences

TOS gains a truthful statement about real machines: the TOS layer is textual and
owner-controlled, and material outside that layer is named as external rather
than hidden or denied. A future bare-metal stage can support CPU microcode and
device firmware without either violating I-01 or pretending the material is
open.

The cost is that a TOS machine on real hardware is not fully inspectable by the
owner, and this decision requires TOS to say so rather than obscure it. The
boundary is visible precisely so that its size can be observed and argued about.

## Alternatives considered

**Prohibit all opaque material.** Rejected: it makes TOS unimplementable on
current hardware and would either stop the project at emulation or be quietly
violated later, which is worse than a stated boundary.

**Treat firmware as ordinary third-party components under docs/27.** Rejected:
that policy is built around material TOS can read, evaluate and admit by review.
Opaque blobs cannot be reviewed as source, so applying the same process would
produce approvals with no evidentiary content.

**Place firmware under `/system/firmware`.** Rejected: it puts non-source bytes
inside the canonical source tree, which is the exact drift section 5 forbids.

**A separate root `/firmware`.** Rejected: firmware is one class of external
vendor material. Microcode, option ROMs and future non-firmware vendor material
belong to the same boundary, and a firmware-specific root would need siblings
later.

<!-- END docs/adr/0030-external-vendor-opaque-material.md -->

---

<!-- BEGIN docs/adr/0031-system-source-hierarchy.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0031: Runtime system source hierarchy

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 2 — extends existing namespace contracts with a normative
  runtime hierarchy without moving a trust boundary or changing an invariant
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

TOS documentation defines the root namespaces of a running system
(`docs/03_ARCHITECTURE_OVERVIEW.md`), their classes
(`docs/09_FILESYSTEM_AND_STATE.md`) and the layout of the development
repository (`docs/17_REPOSITORY_LAYOUT.md`). It does not define the inside of
`/system` on a running machine.

The gaps are concrete, and each one is already reachable from an accepted
contract:

- `docs/17` lists `system/{boot,services,drivers,languages,shell,ui,policy}` in
  the repository, but nothing states whether the runtime `/system` is that same
  tree, a transformation of it, or an unrelated structure. I-16 requires a
  running component to report its canonical source path, which is unanswerable
  while the mapping is undefined.
- `docs/04` names `/system/boot/init.tos` and `/system/boot/health.tos`,
  `docs/07` names `/system/languages/<name>/` and `docs/14` names
  `/system/drivers/virtio/block.tos`. These paths are used as facts by several
  documents without a document that defines them.
- `docs/13` requires that "the active system commit contains a lock manifest"
  without saying where it lives or whether it is canonical source or a cache.
- Shared libraries, applications, runtime-visible schemas, machine-specific
  source and imported third-party textual source have no stated location,
  although every one of them is implied by an accepted contract.
- `docs/09` classifies root namespaces informally. There is no single statement
  of which paths are canonical source, which are mutable state, which are
  derived cache and which are external material — the distinction that makes
  "deleting caches must not remove functionality" (I-01) mechanically testable.

Left undefined, each gap gets filled by whichever subsystem is implemented
first, and the resulting structure becomes architecture by accident.

## Decision

`docs/45_SYSTEM_SOURCE_HIERARCHY.md` becomes a Tier 2 normative contract
defining the runtime system source hierarchy. Its substance:

1. **Namespace classification.** Every runtime path belongs to exactly one of:
   canonical source, source overlay, configuration, mutable state, derived
   cache, ephemeral, capability namespace, external material. The class defines
   what deletion and rollback mean for that path.

2. **Repository-to-runtime mapping.** The repository subtree `source/system/` is
   the canonical input for the runtime `/system` tree, mapped directly and
   without renaming or generation. Repository directories outside
   `source/system/` are development material and are not installed as `/system`
   content.

3. **`/system` hierarchy.** Thirteen entries: `boot/`, `services/`, `drivers/`,
   `languages/`, `lib/`, `apps/`, `shell/`, `ui/`, `policy/`, `schemas/`,
   `machine/`, `third-party/`, `lock/`. Each is canonical source text. Seven of
   them are already named by `docs/17_REPOSITORY_LAYOUT.md`; the remaining six
   are the minimum needed to give an existing accepted requirement a defined
   location.

4. **Manifests stay in module source.** Component manifests are declared inside
   the module they describe, following `docs/11_DRIVER_MODEL.md`. No parallel
   manifest directory is introduced, because a separate manifest tree can drift
   from the code it describes.

5. **`/work` shape.** Overlays mirror `/system` paths, are never executed as
   system source without transactional activation, and are discardable.

6. **`/vendor` dependencies.** A component declares required vendor objects in
   its own manifest; `/system/lock/` aggregates the resolved set for the commit.
   Opaque bytes never appear in `/system`. Governed by ADR-0030.

7. **Lock manifests are canonical source, not cache.** They record resolution
   decisions that define the commit and cannot be regenerated identically at a
   later time, so they fail the derived-artifact test in I-01.

This ADR defines placement and classification only. Module resolution, manifest
schema, capability grammar, activation mechanics and storage format remain with
their existing owning contracts. No directory must exist before the stage that
implements the subsystem it serves.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-01 gains a mechanically testable
  boundary (canonical source versus derived cache per path); I-16 gains the
  mapping that makes a reported source path resolvable in the active commit;
  I-04 gains the explicit `/work`-to-`/system` relationship; I-09 gains a stated
  location for runtime-visible schema source.
- **Canonical representation after the change:** unchanged. `/system` remains
  canonical text; this decision says what is inside it.
- **Trusted-base impact:** none. No dependency enters the loader or nucleus and
  no trust boundary moves.
- **Source-to-runtime impact:** improved. The chain from reported source path to
  active-commit tree entry becomes resolvable rather than conventional.
- **Recovery and rollback impact:** unchanged mechanically. Classification makes
  rollback semantics explicit per class, and section 3 of docs/45 clarifies that
  `/system/lock/` rolls back with the commit while `/vendor` does not.
- **Stage identity gate:** no stage gate is claimed or closed.
- **Threat-model impact:** none directly. The classification supports existing
  properties S6 and S9 by making "derived" and "mutable" checkable per path
  rather than per subsystem convention.
- **Performance contract:** none applicable.
- **Compatibility profile:** none claimed.
- **New dependencies:** none. The decision is documentary.
- **Licence and patent impact:** none. `/system/third-party/` restates existing
  obligations from docs/22 and docs/27 rather than adding any.
- **Tests that enforce the decision:** deferred to the implementing stages, with
  required conformance expectations listed in docs/45 section 6 — no `/system`
  path resolving to cache, state or vendor content; `/cache` deletion behavior;
  reported source paths existing in the active commit; overlay paths unable to
  execute without activation; `/vendor` requirement sets enumerable from
  `/system/lock/`.

## Consequences

Subsystem work from Stage 3 onward has a defined place to put its source, and
the placement decisions are reviewable now rather than emerging from
implementation order. Architecture conformance tests gain a target they can
enforce mechanically.

The cost is that a hierarchy defined before most of its subsystems exist may
require revision. That is accepted: revising a stated contract through an ADR is
the visible path, whereas an unstated hierarchy is revised silently and without
review.

## Alternatives considered

**Extend `docs/09_FILESYSTEM_AND_STATE.md` instead of adding a document.**
Rejected: docs/09 is about why one Git repository cannot hold every changing
byte and how state is separated from source. The internal structure of the
canonical tree is a different subject and would dilute both.

**Extend `docs/17_REPOSITORY_LAYOUT.md`.** Rejected for the reason this ADR
exists: conflating the developer repository with the installed system is the
current source of ambiguity, and merging them into one document would preserve
it.

**Define nothing until Stage 3 needs it.** Rejected: the paths are already used
as facts by docs/04, docs/07, docs/13 and docs/14, so the hierarchy is being
relied upon before it is defined. Deferring means the first implementation
chooses for the architecture.

**Define a complete hierarchy including future subsystems.** Rejected: entries
would have no accepted contract behind them. Every entry in docs/45 section 3
traces to a requirement that already exists.

<!-- END docs/adr/0031-system-source-hierarchy.md -->

---

<!-- BEGIN docs/adr/0032-parser-diagnostics-and-recovery.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0032: TOS Core V1 parser diagnostics and recovery clarification

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 3 — clarifies the accepted TOS Core V1 contract by resolving a
  normative conflict, allocating stable diagnostic codes that conformance
  evidence will depend on, and amending a recovery rule
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

The first production parser exposed three gaps in the accepted TOS Core V1
contract. Each one was blocking in the same way: an implementation had to choose
behavior that conformance evidence would later be measured against, with no
normative text to choose from.

**1. The diagnostic registry does not exist.**
`docs/41` section 7 requires every diagnostic to carry a stable symbolic code and
states that "a full registry and conformance expectations are in docs/44".
`docs/44` contains conformance expectations but no registry. `docs/39` allocates
exactly two parser codes — `E1105_CONTROL_HEAD_PARENS_REQUIRED` and
`E1106_LIST_SEPARATOR_REQUIRED` — and describes the other syntax rejects
(R018, R020–R024, R027, R028) only as "parse error". This is a conflict between
two Tier 2 documents under `docs/38`, not a missing detail: `docs/41` asserts the
existence of authority that `docs/44` does not carry.

**2. Declaration-level recovery discards the rest of the source unit.**
`docs/39` section 4 ends a declaration synchronization region at "the next
top-level `;` or `]`". Neither terminates a `fn` declaration, whose body is a
brace block. Following the rule literally, one malformed function signature
causes every later declaration in the file to be skipped, so the parser reports
one diagnostic where it could report several and produces an emptier tree than
the source supports.

**3. A character that begins no lexical form has no code.**
`docs/39` section 2 allocates `E1012_INVALID_IDENTIFIER` for a non-ASCII scalar
value used where an identifier is formed. A valid UTF-8 ASCII character that
begins no lexical form at all — `@`, `$`, `#`, `` ` ``, `'`, `\` outside a
literal or comment — has no allocated code. Reporting it as
`E1012_INVALID_IDENTIFIER` would overload an identifier-specific code with an
unrelated condition and make the two indistinguishable to conformance tooling.

## Decision

### 1. `docs/44` becomes the authoritative diagnostic code registry

`docs/44` gains a registry section listing, for every frontend diagnostic code
reachable by the source reader, lexer and parser: the stable symbolic code, its
stage, and the exact condition that raises it. `docs/41` section 7's reference to
docs/44 is now satisfied rather than aspirational.

Codes remain allocated by the document that owns the rule. `docs/39` continues to
define lexical and grammatical conditions; the registry records them in one
enumerable place and adds the parser codes that had no home.

### 2. Parser codes E1100–E1104 and E1107 are ratified

The following codes are allocated. `E1105` and `E1106` keep their existing
numbers and meanings and are not renumbered.

| Code | Condition |
|---|---|
| `E1100_EXPECTED_MODULE_HEADER` | a required module-header keyword (`module`, `version`) is absent at its position |
| `E1101_EXPECTED_IDENTIFIER` | an identifier is required at this position and the token present is not one |
| `E1102_EXPECTED_VERSION_COMPONENT` | a module-header version component is not a decimal integer representable as `u32` |
| `E1103_EXPECTED_PROFILE` | the module-header profile is neither `bootstrap` nor `full` |
| `E1104_EXPECTED_LITERAL` | a literal is required at this position and the token present is not one |
| `E1107_UNEXPECTED_TOKEN` | the token cannot begin or continue the construct being parsed and no more specific code applies |

Each has one unambiguous meaning and none overlaps an existing code.
`E1101_EXPECTED_IDENTIFIER` is syntactic — a well-formed token of the wrong class
where an identifier is required — and is distinct from the lexical
`E1012_INVALID_IDENTIFIER`, which fires when bytes cannot form an identifier at
all. `E1104_EXPECTED_LITERAL` is likewise distinct from the lexical
`E1020_INVALID_INTEGER_LITERAL`.

`E1107_UNEXPECTED_TOKEN` is the defined residual of the parse stage. It is
correct only where no other parser code applies; a more specific code always
wins. It is not a licence to leave conditions unclassified: a recurring
`E1107` case that has a distinct meaning is a reason to allocate a code, not to
keep using the residual.

### 3. Declaration recovery may end at a closing brace

`docs/39` section 4 is amended: declaration-level recovery ends a synchronization
region at the next top-level `;` or `]`, **or** at the `}` that closes a
top-level declaration body and returns delimiter nesting to zero.

The purpose is bounded: one error in a declaration or signature must not cost the
remainder of the source unit merely because a function declaration ends with a
block rather than `;` or `]`. The additional boundary never skips past a
boundary the original rule names — it can only end a region earlier.

No further recovery heuristic is admitted. The parser still must not guess a
missing declaration, capability, type or operator, and still emits exactly one
diagnostic per synchronization region.

### 4. `E1013_UNEXPECTED_CHARACTER` is allocated

`E1013_UNEXPECTED_CHARACTER` applies to a valid UTF-8 source character outside a
literal or comment that, at its position, neither begins nor continues any
admissible lexical form.

Precedence against `E1012_INVALID_IDENTIFIER` is fixed and mechanical: a
non-ASCII scalar value outside a literal or comment is
`E1012_INVALID_IDENTIFIER`, because identifiers are the only construct that
non-ASCII text could be attempting to form and `docs/39` section 2 already
assigns that condition. Every other such character — necessarily ASCII — is
`E1013_UNEXPECTED_CHARACTER`. Both report the first byte of the offending
character.

`E1012_INVALID_IDENTIFIER` therefore remains exactly what its contract says: an
identifier-related violation.

### 5. Registry drift is mechanically prevented

`scripts/check-stage2-language-contract.py` gains checks that fail when:

- a diagnostic code cited by `docs/language/conformance/v1/EXPECTATIONS.md` is
  absent from the `docs/44` registry;
- a registry entry lacks a stage or condition;
- an `E10xx`/`E11xx` code named in `docs/39` is absent from the registry;
- a conformance expectation still says "parse error" instead of a code.

`EXPECTATIONS.md` replaces every remaining "parse error" cell with the exact
expected stable code.

## Architecture impact statement

- **Change level:** 3.
- **Invariants affected:** none amended. I-09 is served — diagnostic codes are a
  versioned boundary and now have a single enumerable definition. I-15 is served
  by making "parse error" a precise claim instead of a category.
- **Canonical representation after the change:** unchanged.
- **Trusted-base impact:** none. No dependency enters the loader or nucleus.
- **Source-to-runtime impact:** diagnostics gain stable identity, so a rejected
  source unit can be tied to an exact documented condition rather than to
  implementation wording.
- **Recovery and rollback impact:** none at the system level. Parser recovery in
  section 3 concerns source-unit diagnostics only.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** none. Recovery still terminates: every
  synchronization step consumes at least one token or reaches end of source, so
  hostile input cannot induce non-termination. Bounded parsing under S2 is
  unchanged.
- **Performance contract:** none applicable.
- **Compatibility profile:** TOS Core 1.0. Ratifying codes fixes them for V1; a
  later code change is a versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** parser unit tests asserting each ratified
  code and all three synchronization regions; a conformance-negative test in
  which a valid top-level declaration follows a damaged function and is still
  parsed; lexical vectors for `@` and `$` fixing span and precedence; and the
  mechanical registry/expectations gate in section 5.

## Consequences

Conformance evidence can name an exact code for every rejected source, and the
implementation stops carrying provisional semantics. The residual `E1107` keeps
the registry honest about what is not yet classified rather than hiding it.

The cost is that six parser codes are now fixed for TOS Core 1.0 and can only be
changed through a versioned language decision. That is the intended trade: codes
that conformance depends on must not drift.

## Alternatives considered

**Leave the codes provisional until more of the parser exists.** Rejected: the
conformance corpus already exists and would have to assert something. Provisional
codes in accepted expectations are the drift this project's documentation
hierarchy is built to prevent.

**Put the registry in `docs/39`.** Rejected: `docs/41` already names `docs/44` as
its location, and `docs/39` owns syntax rather than diagnostics across all
stages. Moving the reference instead of satisfying it would leave `docs/41`
inaccurate.

**Report `@` and `$` as `E1012_INVALID_IDENTIFIER`.** Rejected: it makes an
identifier diagnostic mean two unrelated things and prevents tooling from
distinguishing a Unicode identifier attempt from a stray symbol.

**Extend declaration recovery with further heuristics**, such as resuming at any
token that could begin a declaration. Rejected as unbounded guessing: it would
let the parser invent a declaration boundary the grammar does not define.

<!-- END docs/adr/0032-parser-diagnostics-and-recovery.md -->

---

<!-- BEGIN docs/adr/0033-pattern-name-resolution.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0033: TOS Core V1 pattern name resolution

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 2 — fixes pattern resolution semantics inside the accepted
  TOS Core V1 contract and adds the qualified constructor-pattern form the
  grammar was missing
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

The first checker slice reached a question the accepted contract does not
answer: when a pattern is a bare identifier, does it match a constructor or
bind a new name?

`docs/39` section 2 makes only reserved, primitive, predeclared type and
predeclared value names unshadowable, which reads as "every other identifier in
a pattern binds". The accepted corpus disagrees: `explicit-control-return.tos`
writes `match (signal) { Low => { ... } High => { ... } }` and expects `Low` and
`High` to match the variants of `Signal`.

A second gap sits next to it. `docs/40` section 2 states that enum variant names
are local to their defining module, may be used unqualified there, and that an
imported enum variant uses a qualified type or module name. The `pattern`
production in `docs/39` section 5 has no qualified form, so the language
requires a syntax its own grammar cannot express. That is a conflict between two
Tier 2 documents under `docs/38`, not an omission of detail.

Neither question can be deferred past the type slice: exhaustiveness checking
and payload typing both depend on whether an arm matches a constructor or binds
a catch-all.

## Decision

### 1. A bare pattern name resolves against the expected type

Every pattern is checked against an expected type, which the checker knows
before it resolves the pattern:

- `match (expression)` — the type of the scrutinee expression;
- `let pattern = expression` — the type of the initializer, refined by an
  explicit type annotation when one is present;
- `for pattern in (expression)` — the element type of the iterated value;
- a nested pattern — the type of the corresponding tuple element or enum
  payload position.

If the expected type is an enum and a bare identifier exactly equals the name of
one of that enum's variants, the identifier is the constructor pattern for that
variant. Otherwise a bare ordinary identifier introduces a new pattern binding.

### 2. Resolution is nominal, never lexical or lexicographic

There is no capitalization rule. `Uppercase` does not mean constructor and
`lowercase` does not mean binding; V1 has no such convention and none is
introduced.

An existing lexical or value binding of the same name does not change the
decision. Constructor resolution is determined by the expected nominal type
alone, so introducing an unrelated local named `Low` cannot silently turn a
variant pattern into a binding, and removing one cannot turn a binding into a
variant pattern.

A consequence is that two enums may declare variants with the same name without
colliding:

```tos
enum Signal [ Low, High ]
enum Power [ Low, High ]
```

`Low` inside a pattern is resolved by the type of the subject.

### 3. Payload variants use the same rule

`Name(...)` is a constructor and destructuring pattern and is resolved against
the expected enum type exactly as the bare form is. Its sub-patterns are then
checked against the payload positions of that variant.

### 4. Predeclared constructors keep their status

`Some`, `None`, `Ok`, `Err`, `Completed` and `Cancelled` remain non-shadowable
constructor names and resolve against their expected constructed types
(`Option<T>`, `Result<T,E>`, `TaskResult<T>`). They are never bindings.

### 5. Qualified constructor patterns

The `pattern` production gains a qualified constructor path, using the existing
TOS qualified-name punctuation. No `::` is introduced:

```text
pattern          = "_"
                 | pattern_path ( "(" pattern_list? ")" )?
                 | "(" pattern_list ")" ;
pattern_path     = pattern_name ( "." identifier )* ;
pattern_name     = identifier | predeclared_value ;
```

This stays deterministic. A single identifier remains exactly one syntactic
alternative — a `pattern_path` with no suffix — and whether it denotes a
constructor or a binding is decided during resolution, not during parsing. The
`.` suffix is unambiguous, because no other production may follow a pattern name
with a dot.

A `pattern_path` containing at least one `.` is **always** a constructor path
and is **never** a binding. A local variant MAY be written either in the short
form `Low`, when the expected enum type determines it, or explicitly as
`Signal.Low`. An imported variant uses the qualified form and resolves through
ordinary module and import resolution, so `other.Signal.Low` names the `Low`
variant of `Signal` in the module bound to `other`.

A qualified path that names no reachable variant is an error rather than a
binding. It cannot degrade into a catch-all.

### 6. Conformance evidence

The corpus gains positive and negative cases for at least: a local bare unit
variant; a bare binding where the expected type has no such variant; two enums
sharing a variant name disambiguated by the expected type; payload variant
destructuring; an explicitly qualified local variant; a qualified imported
variant; an unknown qualified variant; an exhaustive match over bare variants;
wildcard and binding exhaustiveness; and a case proving resolution does not
depend on capitalization.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-15 is served: the language now states
  precisely what a bare pattern name means instead of leaving two readings.
- **Canonical representation after the change:** unchanged. Existing canonical
  source stays valid; the qualified form is additive.
- **Trusted-base impact:** none.
- **Source-to-runtime impact:** none directly. Pattern resolution becomes
  reproducible from the module's own declarations plus its import closure.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** none. Resolution reads only declared types, so hostile
  source cannot make a pattern silently change meaning by introducing a name.
- **Performance contract:** none applicable.
- **Compatibility profile:** TOS Core 1.0. Making the rule type-directed fixes it
  for V1; changing it later is a versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** the ten conformance cases in section 6,
  plus checker unit tests for the resolution rule and the qualified path form.

## Consequences

Pattern resolution now requires the expected type, so it belongs to the type
slice rather than to name resolution. The checker's current name-resolution
slice is unaffected: both readings admitted the same set of resolvable names, so
no diagnostic changes.

Variant names stop being module-global for pattern purposes, which removes a
collision that would otherwise force every enum in a module to use distinct
variant names.

## Alternatives considered

**Resolve against any constructor in scope.** Rejected: it makes variant names
module-global, so adding a variant whose name matches an existing local silently
changes a binding into a match, and two enums cannot share a variant name.

**Require an explicit form for every variant pattern.** Rejected: it follows the
literal reading of docs/39 section 2 but invalidates accepted canonical source,
and the corpus is accepted evidence rather than a draft.

**Adopt a capitalization convention.** Rejected: V1 has no such convention
anywhere else, and it would make meaning depend on spelling rather than on
declarations.

## Open matter deliberately not decided

Whether `let` and `for` patterns must be irrefutable, and what a refutable
pattern in those positions reports, is a separate question. It is not settled
here and must not be inferred from an implementation.

<!-- END docs/adr/0033-pattern-name-resolution.md -->

---

<!-- BEGIN docs/adr/0034-type-name-and-arity-diagnostics.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0034: TOS Core V1 type-name and type-argument-arity diagnostics

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 2 — allocates two diagnostic codes that conformance evidence
  will depend on, and removes an ambiguity about which stage checks type
  argument arity
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

The type slice of the reference frontend cannot start without two diagnostics
the contract describes but does not name.

**A type name that resolves to nothing.** `docs/40` section 1 states that V1 has
nominal types and that a type name resolves through the declared import graph,
never by host search paths. It allocates no code for the case where that
resolution fails, so a checker had no way to reject `let value: Nonexistent =
...` other than silence.

**A wrong number of type arguments.** `docs/40` section 2 fixes the arity of
every V1 constructed type — ten constructors take one type argument, `Result`
takes two, eight take none — and then says using another arity "is a parse/type
error". That phrase leaves two normative answers about which stage rejects it.
It matters: if the parser refuses to build `Option<i32, bool>`, the error is a
syntax error at an unexpected token, the arity is invisible in the diagnostic,
and the tree stops at the first mistake. If the checker rejects it, the
diagnostic can carry the constructor and both arities.

Under ADR-0032, allocating a code is a versioned language decision rather than
an implementation choice, so both had to be decided before the type slice could
be written.

## Decision

### 1. `E1203_UNKNOWN_TYPE_NAME`

Stage `type`. A type name, after ordinary module, import and type-name
resolution, resolves to none of:

- a primitive type;
- a fixed or predeclared TOS Core type;
- a local nominal type;
- a reachable imported type.

For a qualified name, the module or import part must resolve first. If the
import or module itself does not exist, the applicable `E16xx` code governs; if
the module or import exists but does not declare that type name, the result is
`E1203_UNKNOWN_TYPE_NAME`.

The diagnostic carries at least the unresolved type name as spelled.

### 2. `E1204_TYPE_ARGUMENT_ARITY`

Stage `type`. A name resolves to a known parameterized V1 type constructor but
is applied to the wrong number of type arguments.

The number of type arguments is a static type property, not a parser decision.
The parser MUST be able to build a syntactically valid constructed-type node for
a known V1 type constructor written with `<...>`, and the checker compares the
actual count against the fixed V1 arity:

```tos
Option<i32>              // accepted
Option<i32, bool>        // E1204, expected 1, actual 2

Result<i32, Error>       // accepted
Result<i32>              // E1204, expected 2, actual 1
```

The diagnostic carries at least:

```text
constructor
expected_arity
actual_arity
```

This admits no user generics, and it does not make an arbitrary `Foo<T>` valid
V1 type syntax. It applies only to the fixed set of parameterized constructors
already defined by TOS Core V1.

`array<T, N>` is deliberately excluded. Its second argument is a compile-time
`size` constant rather than a type argument, and its existing grammar and type
contract stay separate. No general kind or generic mechanism is introduced for
`E1204`.

### 3. Precedence

1. an unresolved constructor or type name is `E1203_UNKNOWN_TYPE_NAME`;
2. a name that resolves to a known parameterized constructor applied with the
   wrong number of arguments is `E1204_TYPE_ARGUMENT_ARITY`;
3. only after the arity is correct are the argument types themselves and the
   remaining type rules checked.

One mistake must not cascade into further diagnostics derived from a constructed
type that does not exist.

### 4. Removing the ambiguity in docs/40

The phrase "using another arity is a parse/type error" is replaced. After this
ADR there is one normative answer: arity is checked at the type stage and
reported as `E1204_TYPE_ARGUMENT_ARITY`. `docs/39` records the matching grammar
boundary — the parser builds the constructed-type node and does not decide
arity.

### 5. Conformance evidence

The corpus gains negative cases for at least: an unknown local type; an unknown
qualified type where the import and module resolve; `Option` with the wrong
arity; `Result` with the wrong arity; and a case proving the precedence of an
unresolved name over an arity finding.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-09 is served — the two codes become
  part of the versioned diagnostic boundary; I-15 is served by replacing
  "parse/type error" with one stated stage.
- **Canonical representation after the change:** unchanged. No accepted source
  becomes invalid: every arity these codes reject was already an error under
  docs/40 section 2, only with an unstated stage.
- **Trusted-base impact:** none.
- **Source-to-runtime impact:** none directly. A rejected type expression now
  names its constructor and arities, so evidence can cite an exact condition.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** none. Moving arity to the type stage keeps the parser
  total and bounded; the checker reads only declared types.
- **Performance contract:** none applicable.
- **Compatibility profile:** TOS Core 1.0. Both codes and the arity stage are
  fixed for V1 and change only through a versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** the five conformance cases in section 5,
  checker unit tests for both codes and their precedence, and the mechanical
  language-contract gate binding the codes to the registry.

## Consequences

The type slice can begin. A rejected type expression names the constructor and
both arities instead of pointing at a token, and the precedence rule keeps one
mistake from producing a cascade of derived findings.

The cost is two more codes fixed for TOS Core 1.0. That is the intended trade:
a code conformance depends on must not drift.

## Alternatives considered

**Reject wrong arity in the parser.** Rejected: it is the reading that makes the
diagnostic least useful — an unexpected-token error cannot name the constructor
or the expected count — and it would let a syntax stage encode a type property.

**Reuse `E1202_UNKNOWN_VALUE_NAME` for unknown type names.** Rejected: a value
name and a type name are different namespaces in a nominal language, and
conformance tooling could not tell which one failed.

**Generalize `array<T, N>` with the other constructors.** Rejected: its second
argument is a constant, not a type. Folding it in would require a kind system
that V1 deliberately does not have.

<!-- END docs/adr/0034-type-name-and-arity-diagnostics.md -->

---

<!-- BEGIN docs/adr/0035-defer-ownership-and-borrow-conflicts.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0035: TOS Core V1 `defer` ownership semantics and the borrow-conflict class

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — fixes the ownership meaning of an accepted V1 statement
  form and broadens the stable condition of an already allocated diagnostic
  code, both of which conformance evidence depends on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

The ownership slice of the reference frontend reached two boundaries the
contract described operationally but had not settled semantically.

**`defer` and ownership.** `docs/40` section 5 states when a defer body runs —
in reverse registration order whenever its enclosing block exits — and what it
may not contain (`E1225_INVALID_DEFER`). It does not state what happens to
ownership. Two readings were available and they disagree about the same program:

```tos
take(message);
defer { take(message); }
```

Read as a capture at registration, the second `take` reserves `message` and the
first one is the error. Read as a deferred use, the registration does nothing
and the deferred `take` is the error. The two readings also disagree about
whether the enclosing scope may keep using a resource whose cleanup consumes it.
Choosing by implementation convenience would have fixed a language rule by
accident, so the ownership walk analysed nothing inside a defer body and the
question was recorded as an Architect decision.

**The reach of `E1302`.** The registry condition for `E1302_CONFLICTING_BORROW`
named only borrow-against-borrow, and `E1303_MUTATE_WHILE_BORROWED` only a write
under an immutable borrow. Three operations that violate the exclusivity
`docs/40` section 5 states — "a value may have either any number of immutable
borrows or exactly one mutable borrow, never both" — had no code at all:

```tos
let mut c = Counter(value: 0i32);
let m = borrow mut c;
return c.value;        // owner read under a live mutable borrow
```

```tos
let view = borrow message;
take(message);         // move under a live borrow
```

```tos
let mut c = Counter(value: 0i32);
let m = borrow mut c;
c.value = 1i32;        // owner write under a live mutable borrow
```

Silence on these is unsound: the exclusivity rule is stated, and a checker that
proves it only for one of the four ways to break it does not prove it.

## Decision

### 1. `defer` is deferred lexical cleanup, not a capture

`defer` is a deferred lexically scoped cleanup block. It is not a closure and
does not use the closure-capture rules of `E1305_INVALID_CLOSURE_CAPTURE`.

Executing the `defer` statement registers the cleanup. At that moment:

- the lexical names inside the body bind to the binding identities visible at
  the point of registration;
- the values of those bindings are not read, not borrowed and not moved;
- no ownership effect of the body takes place.

The body runs only when the enclosing block is actually left. On each exit path,
in this order:

1. the action that caused the exit has already been evaluated — the `return`
   operand, the `break`, the `continue`, a propagation;
2. the defers registered on the path actually taken run in reverse registration
   order;
3. the ownership and borrow state left by one defer is the input state of the
   next;
4. only after cleanup do the bindings leave scope and their bounded `drop` run.

A defer body is therefore analysed against the ownership state that exists on
the concrete exit path.

```tos
take(message);
defer { take(message); }        // E1301_USE_AFTER_MOVE inside the defer
```

```tos
defer { take(message); }
take(message);                  // E1301_USE_AFTER_MOVE inside the defer
```

Registering a consuming cleanup deliberately neither reserves nor moves the
value at the point of registration. Ordinary correct use between registration
and exit is allowed:

```tos
let file = open(path);
defer { close(file); }
read(borrow file);
read(borrow file);
```

The obligation is the other way round: a program must leave every defer that can
actually run ownership-valid on every exit path that runs it.

Shadowing after registration does not change which binding a defer refers to.
Binding identity is fixed lexically at the point of registration.

`E1225_INVALID_DEFER` is unaffected. It remains a separate syntactic and typed
restriction on the form of a defer body, not a statement about ownership.

There is one cleanup mechanism. `return`, `break`, `continue`, normal block
exit, and the other contract exits all unwind the cleanups of the lexical blocks
they leave, through the same model. `?` and cancellation use that same model
wherever their flow semantics are already representable; no second cleanup
mechanism is introduced for them.

### 2. `E1302_CONFLICTING_BORROW` covers the whole exclusivity violation

No new `E13xx` code is allocated. The normative condition of the existing code
is broadened.

`E1302_CONFLICTING_BORROW` means any operation that violates the exclusivity of
a live borrow of an overlapping place, not only the creation of a second borrow.
It covers:

1. a new borrow incompatible with a live overlapping borrow;
2. an ordinary owner read or use of an overlapping place while a mutable borrow
   is live;
3. an ordinary owner mutation of an overlapping place while a mutable borrow is
   live;
4. a move or other invalidation of an overlapping place while any borrow —
   shared or mutable — is live.

`E1303_MUTATE_WHILE_BORROWED` remains the specialized case it already was:

- a write or mutation of an overlapping place while an immutable, shared borrow
  is live.

The accepted matrix:

```text
shared borrow  + owner write   -> E1303
mutable borrow + owner read    -> E1302
mutable borrow + owner write   -> E1302
any borrow     + owner move    -> E1302
incompatible borrow pair       -> E1302
```

Operations performed through the correct borrow binding itself are not owner
aliases and stay legal according to that borrow's kind. Reading through a shared
borrow and writing through a mutable borrow remain exactly as legal as before;
only accesses that go around a live borrow to the owning place are affected.

### 3. Region and synchronization guards are out of scope

This ADR does not decide `Transferable` for regions or lock guards and does not
allocate or extend `E1304_INVALID_TASK_CAPTURE` for them. The principle stands:

- `Transferable`, shareable and mutable are read from a proved type, interface
  or capability contract;
- a constructor name alone is not that proof;
- the absence of such information produces no invented diagnostic.

The ownership information interface is prepared so a later slice can distinguish
`KnownTransferable`, `KnownNonTransferable(reason)` and `Undetermined` without
duplicating type resolution. The capability and synchronization contract itself
is left to that slice.

## Architecture impact statement

- **Change level:** 2.
- **Invariants affected:** none amended. I-09 is served — a stable diagnostic
  condition is stated rather than left to an implementation; I-15 is served by
  replacing an undefined ownership meaning with one normative reading.
- **Canonical representation after the change:** unchanged. No source text
  changes meaning at runtime. Programs that were already unsound under the
  stated exclusivity rule are now rejected instead of silently accepted, and a
  defer body whose cleanup was already invalid on some exit path is now named.
- **Trusted-base impact:** none.
- **Source-to-runtime impact:** the frontend now proves the exclusivity rule for
  all four of its violations and analyses cleanup against the state that exists
  where cleanup runs. The IR and verifier contract is unchanged.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** positive and bounded. Broadening `E1302` closes an
  aliasing hole in the safe subset; the defer model adds no unwinding and keeps
  cleanup bounded and deterministic.
- **Performance contract:** none applicable. Path-sensitive defer unwinding is
  bounded by the lexical nesting of blocks.
- **Compatibility profile:** TOS Core 1.0. Both the defer ownership semantics
  and the `E1302`/`E1303` boundary are fixed for V1 and change only through a
  versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** the conformance vectors of section 4
  below, checker unit tests for each row of the matrix and each defer exit path,
  and the mechanical language-contract gate binding the conditions to the
  registry.

### 4. Conformance evidence

For `defer`, at least: a resource still usable after registration; a move before
a deferred consuming use giving `E1301` inside the defer; a return path running
the defer; `break` and `continue` running the defers of only the blocks actually
left; nested defers in LIFO order; shadowing after registration; a defer
registered only on a reached path; and one defer's ownership effect visible to
the next.

For the borrow matrix, at least: an owner read under a live mutable borrow; a
move under a live borrow; and an owner write under a live mutable borrow.

## Consequences

The ownership frontier closes. Cleanup has one model shared by every exit, and
the exclusivity rule of `docs/40` section 5 is proved for every way to break it
rather than for one of them.

The cost is that two things previously accepted in silence are now rejected: an
owner access that goes around a live borrow, and a cleanup body that cannot run
soundly on a path that reaches it. Both were already violations of stated rules;
only the reporting was missing.

## Alternatives considered

**`defer` captures at registration.** Rejected: it would make registration a
move, so a resource could not be used after its own cleanup was registered —
which is the ordinary and intended use — and it would turn a lexical cleanup
block into a closure with capture rules it does not have.

**Analyse a defer body once, against the joined state of all exits.** Rejected:
it would report against a state no execution ever has, and it would hide a
cleanup that is invalid on exactly one path.

**Allocate new codes for owner read, owner write and move under a borrow.**
Rejected: they are one rule — the exclusivity of a live borrow — and splitting
one rule across four codes would make conformance evidence describe an
implementation's internal case analysis rather than the language.

**Fold `E1303` into `E1302`.** Rejected: `E1303` is already accepted evidence
with a distinct, useful meaning, and removing a code from a versioned registry
is a larger change than the one this ADR needs.

**Decide region and guard transferability here.** Rejected: it requires the
capability and synchronization contract, and this ADR must not settle unrelated
semantics.

<!-- END docs/adr/0035-defer-ownership-and-borrow-conflicts.md -->

---

<!-- BEGIN docs/adr/0036-synchronization-guard-representation.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0036: TOS Core V1 synchronization guard representation

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — adds type constructors and one diagnostic code to the
  accepted V1 surface, which conformance evidence and the IR type table depend on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11
- Supersedes: revision 1 of this ADR, which left the normative "a guard may not
  be held across await" rule with no source diagnostic, said nothing about the
  lifetime relation between a guard and its lock, and did not say that the
  checker and the verifier must prove the same rule independently

## Context

`docs/41` section 4 gives `Mutex<T>` a lock that "grants an affine mutable
guard", and `RwLock<T>` "multiple immutable read guards or one affine write
guard". A guard "cannot await, cross a task boundary, or be dropped after its
lock resource disappears". `docs/40` section 6 lists a lock guard among the
values that are not `Transferable`.

The V1 type surface names no guard. `docs/39` section 3 lists `Mutex` and
`RwLock` among the parameterized constructors, but nothing for the value a lock
operation yields, and `docs/39` gives no `lock` operation either. So a checker
cannot establish that a value *is* a guard except by guessing from the
constructor name of the object it came from — which ADR-0035 section 3 forbids,
because a synchronization object is not its guard.

The consequence is concrete: the ownership slice reports nothing for guards, the
IR has no type for one, and the verifier cannot check the guard rules `docs/43`
section 3 lists under its synchronization family. Every one of those is blocked
on the same missing name.

## Decision

### 1. Three guard type constructors join the V1 type surface

```text
MutexGuard<T>      the affine mutable guard a Mutex<T> lock grants
ReadGuard<T>       an immutable read guard an RwLock<T> grants
WriteGuard<T>      the affine write guard an RwLock<T> grants
```

Each takes exactly one type argument, the type the lock protects. They join the
`predeclared-type`/`constructed_type` productions of `docs/39` section 3 and the
fixed-arity table of `docs/40` section 2, so `E1204_TYPE_ARGUMENT_ARITY` covers
them without a new rule.

They are **not constructible from source**. There is no constructor syntax for a
guard; a guard value exists only as the result of a lock operation. Writing one
as a constructor is the nonconstructible-type error of ADR-0039.

### 2. Lock operations

```text
Mutex<T>.lock()      -> MutexGuard<T>
RwLock<T>.read()     -> ReadGuard<T>
RwLock<T>.write()    -> WriteGuard<T>
```

Each is a typed operation on the synchronization object, in the same
receiver-operation form the atomics already use. Releasing is the guard's
bounded `drop`: there is no `unlock` operation taking a guard, because a
released guard that still had a name would be exactly the use-after-release the
affine rule exists to prevent.

### 3. What the three types are

- `MutexGuard<T>` and `WriteGuard<T>` are affine and non-`Copy`, grant mutable
  access to the protected value, and are **not** `Transferable`: they may not
  cross a task boundary, be captured by a task or closure, be returned, stored
  in an aggregate, or sent through a channel.
- `ReadGuard<T>` is affine and non-`Copy` and grants immutable access. It is
  likewise not `Transferable`.
- No guard may be held across an `await`.

A guard's scope is its binding's block. Its `drop` releases the lock and is
bounded: it allocates nothing, awaits nothing and acquires no authority.

### 4. The lifetime relation between a guard and its lock

Acquisition creates a checkable dependency: **the synchronization object must
outlive every guard it granted.** A guard names a resource inside that object,
so an object that is moved or dropped while one of its guards is live would
leave the guard naming nothing — the exact condition the affine rule exists to
prevent, one level up.

Moving a guard between bindings of the same scope does **not** release the lock.
A guard is affine, so a move transfers ownership of the guard *and the release
obligation with it*; the lock is released by the bounded `drop` of whichever
binding finally owns it. That is what makes a guard usable at all: a helper may
take one, and releasing on every move would release it at the first hand-off.

### 5. `E1402_INVALID_GUARD_LIFETIME`

Stage `type`, in the `E14xx` concurrency family. One code covers every
prohibited lifetime or escape operation on a guard, with a structured
`operation` field naming which:

```text
operation=held_across_await     a guard is live across an `await`
operation=returned              a guard is returned from a function, task or
                                closure body
operation=aggregate             a guard is placed into a record, enum, tuple or
                                array
operation=channel               a guard is sent through a channel
operation=task_boundary         a guard is moved or captured across a task or
                                closure boundary
operation=lock_outlived         the synchronization object is moved or dropped
                                while one of its guards is live
```

The diagnostic also carries the guard type and the source position where the
guard was acquired, because a lifetime finding that does not say where the
lifetime started cannot be acted on.

**Precedence, so nothing is reported twice.** A guard crossing a task or closure
boundary is `E1402_INVALID_GUARD_LIFETIME` with `operation=task_boundary`, and
**not** `E1304_INVALID_TASK_CAPTURE` or `E1305_INVALID_CLOSURE_CAPTURE`. The
capture codes keep their meaning for every other non-`Transferable` value; a
guard is routed to the guard-specific code because its rule is about the
guard's lifetime rather than about transferability alone, and a single reading
is what keeps the two families from overlapping. `docs/40` section 6 is amended
to say so: it continues to list a lock guard among the values that are not
`Transferable`, and it records that the diagnostic for one is `E1402`.

### 6. The checker and the verifier prove the same rule independently

`V2031_SYNC` gains exactly the rules of section 5, restated over IR: a guard
operand may not appear in a spawn capture, a closure capture, an aggregate
construction, a return, or a channel operation; a guard value may not be live
across an await; and a synchronization object may not be moved or dropped while
a guard derived from it is live.

Neither component may take the other's word for it. The verifier reaches the
conclusion by its own traversal of the IR, as `docs/43` section 5 requires, and
the frontend's success is not an input to it. A guard rule the checker enforces
and the verifier does not would be a rule an alternate frontend could skip.

### 7. Conformance evidence

At least: a positive taking and releasing a mutex guard within a block; a
positive taking two read guards of one `RwLock`; a positive moving a guard into
a helper binding and releasing it there, proving a move is not a release; a
negative capturing a guard into a task (`operation=task_boundary`); a negative
returning a guard (`operation=returned`); a negative holding a guard across an
`await` (`operation=held_across_await`); a negative placing a guard into a
record (`operation=aggregate`); a negative dropping the mutex while its guard is
live (`operation=lock_outlived`); and a negative applying a constructor to a
guard type. Each has a matching forged-IR negative for `V2031_SYNC`.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-09 is served —
  `E1402` becomes part of the versioned diagnostic boundary; I-15 is served by
  naming what was described but unnamed.
- **Canonical representation:** unchanged. No accepted source becomes invalid —
  V1 source cannot name a guard today, so nothing can break.
- **Trusted-base impact:** none. **Threat-model impact:** positive: the guard
  rules become checkable instead of unstated.
- **Compatibility profile:** TOS Core 1.0; the three constructors are fixed for
  V1 and change only through a versioned language decision.
- **Tests:** the nine conformance cases of section 7 with their forged-IR
  counterparts, checker unit tests per `operation` value and for the precedence
  against `E1304`/`E1305`, and the mechanical gate binding the constructors to
  the arity table and `E1402` to the registry.

## Consequences

The synchronization slice becomes implementable, and `V2031_SYNC` stops being a
family with no rules. Every prohibited guard operation has a code, so no
normative rule about guards is left without a way to report it.

The cost is three type names and one diagnostic code fixed for V1, and one
routing decision: a guard crossing a task boundary is reported as a guard
lifetime finding rather than as a capture finding.

## Alternatives considered

**Infer a guard from the receiver's type.** Rejected: it is the guess ADR-0035
forbids, and it cannot distinguish a guard from the object once the value is
passed on.

**One `Guard<T>` for all three.** Rejected: a read guard and a write guard have
different aliasing rules, and one type would make the difference invisible
exactly where the verifier has to see it.

**Model release as an `unlock(guard)` operation.** Rejected: it leaves a named
guard after release, which is the use-after-release the affine rule prevents.

**Report a guard crossing a task boundary as `E1304`/`E1305`.** Rejected as the
primary reading: the same program would then be describable by two codes from
two families, and a conformance expectation would have to pick one without the
contract saying which. One guard-specific code with an `operation` field says
exactly what happened and leaves the capture codes their own meaning.

**Release the lock when a guard is moved.** Rejected: it would release at the
first hand-off, so a guard could never be passed to a helper, and the
release point would depend on binding structure rather than on ownership.

<!-- END docs/adr/0036-synchronization-guard-representation.md -->

---

<!-- BEGIN docs/adr/0037-region-transferability.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0037: TOS Core V1 region and DMA-region transferability

- Status: **Proposed (revision 3)** — the region model is accepted; this
  revision adds the `share` operation the model needs and stops at one boundary
  it uncovers
- Date: 2026-08-11
- Decision level: 2 — fixes the `Transferable`, shareable and mutable facts of
  two accepted V1 type constructors
- Project Architect approval: *(pending)*
- Supersedes: revision 1, whose share model let a shareable DMA region become
  `Shared<DmaRegion<T>>` and be copied into several tasks; and revision 2, which
  used a `share(region)` form that the accepted V1 word inventory and
  `predeclared-function` list do not contain

## Context

`docs/40` section 6 lists "a mutable region" among the values a task may not
capture, and `docs/40` section 5 makes regions non-`Copy`. `docs/42` section 4
makes a capability transferable only when its interface declares it so.

Nothing says how a checker decides whether a given `Region<T>` is mutable or
shareable. The type constructor alone does not say — a region granted read-only
and a region granted for writing have the same written type — so the ownership
slice classified nothing and reported nothing, which is where the implementation
correctly stopped rather than guessing.

The missing piece is that a region's rights live in its **grant**, and V1 source
has no way to write a grant down.

## Decision

### 1. A region's rights are part of its type

`Region<T>` and `DmaRegion<T>` gain a declared access mode, written as the
grant that produced them:

```text
Region<T>          an immutably granted region: readable, shareable, Transferable
Region<mut T>      a mutably granted region: readable and writable,
                   not shareable, not Transferable
DmaRegion<T>       an immutably granted device-visible region
DmaRegion<mut T>   a mutably granted device-visible region
```

`mut` inside the type argument is the only place V1 admits it in a type, and it
is admitted for exactly these two constructors. It is not a general mutability
qualifier and introduces no `mut T` elsewhere.

### 2. The four facts

| Type | `Copy` | mutable | Shareable | `Transferable` |
|---|---|---|---|---|
| `Region<T>` | no | no | yes | yes |
| `Region<mut T>` | no | yes | no | no |
| `DmaRegion<T>` | no | no | **no** | **no** |
| `DmaRegion<mut T>` | no | yes | no | no |

Both DMA variants are conservative in V1. Making `DmaRegion<T>` shareable would
let it become `Shared<DmaRegion<T>>`, and a `Shared<T>` is `Copy`, so the handle
could then be copied into several tasks — which is exactly the crossing the rule
"a DMA region never crosses a task boundary" exists to forbid. A narrower
statement that can be walked around is worse than none. Wider DMA sharing or
transfer may arrive later through a typed driver or device contract that says
what makes it safe; it is not something the language grants by default.

### 3. Sharing is an explicit typed operation, never an implicit copy

`Region<T>` is affine like every other non-`Copy` value, so its handle has one
owner. `Transferable` means that ownership may move into **exactly one** task —
not that the handle may be duplicated.

Using one region from several tasks is written `share(region)`. Revision 2 used
that form without it existing: `share` is in neither the accepted V1 word
inventory of `docs/39` section 2 nor its `predeclared-function` list. Section 4
adds it, because the model is not expressible without it.

`Shared<T>` is the `Copy` handle `docs/40` already defines, so the copies the
several tasks hold are copies of a `Shared`, produced by a typed operation that
appears in the source and in the IR. There is no path where an affine region
handle is silently duplicated because two tasks happened to name it: that would
make an ownership transfer look like a read.

### 4. `share` as a predeclared operation

`share` joins `predeclared-function` in `docs/39` section 2, alongside `to_*`
and `wrapping_*`. It is a language operation, not a library call and not ambient
runtime behaviour.

**Type rule.**

```text
share(T) -> Shared<T>    only when T is transitively immutable and Shareable
```

Transitively immutable means `T` and every type reachable from it contains no
mutable region, no mutable borrow and no guard. Shareable is the column of the
section 2 table: `Region<T>` is, `Region<mut T>` and both `DmaRegion` variants
are not.

**Ownership.** `share` consumes its argument. The affine handle is moved into
the operation and the caller holds `Shared<T>` afterwards; the original name is
moved-from, and using it is `E1301_USE_AFTER_MOVE` like any other move. A
`share` that left the original usable would hand out two roots to one region,
which is the duplication this whole section exists to prevent.

**Verifier-visible and accounted.** `share` lowers to its own IR operation, not
to an opaque helper call: `docs/43` section 3 forbids hiding shared-memory
access behind one. It carries its argument, its result type and its source span,
`V2021_REGION` rechecks the Shareable requirement independently, and the
resulting `Shared<T>` counts against the module's declared `shared` resource
limit, so sharing is bounded by the envelope like every other resource.

### 5. One boundary this uncovers — an Architect decision

A `share` whose argument is not Shareable — `share(dma)`, `share(mutable)` —
has **no diagnostic code to report it**. The registry has no general
argument-type-mismatch code at all: `E1210` is integer agreement, `E1211` is
indexing, `E1212` is `as`, `E1222` is a return. Nothing covers "this call's
argument does not satisfy the operation's declared requirement".

Rather than borrow one of those for a condition it does not describe, this ADR
stops here and proposes the narrow options:

1. **`E1214_INVALID_SHARE`** — one code for exactly this operation, with a
   `reason` field (`not_shareable`, `mutable`, `dma`). Smallest possible
   addition; adds a code that only ever fires for one operation.
2. **`E1215_ARGUMENT_TYPE_MISMATCH`** — a general code for an argument that does
   not satisfy a declared parameter or predeclared-operation requirement. Wider,
   and it fills a gap the registry has independently of `share`: today an
   ordinary call with a wrongly typed argument has no code either.
3. **Extend `E1210`'s condition** to argument agreement generally. Rejected here
   as a suggestion — it is named to be dismissed: `E1210` is about integer
   types, and widening an accepted stable condition to cover unrelated cases is
   what the registry discipline exists to prevent.

Option 2 is the recommendation: the gap it closes is real and larger than
`share`, and one general code is easier for conformance tooling to reason about
than a family of operation-specific ones. But allocating a code is a versioned
language decision, so it is the Project Architect's, and this ADR is not
Accepted until it is made.

### 6. Diagnostics

No new code. Capturing a non-`Transferable` region into a task is
`E1304_INVALID_TASK_CAPTURE` with `reason=mutable region` or `reason=DMA
region`; into a closure it is `E1305_INVALID_CLOSURE_CAPTURE` with the same
reasons. Writing through a `Region<T>` is `E1201_ASSIGN_TO_IMMUTABLE`.

`V2021_REGION` gains these as verifier rules, so the IR carries the mode in its
type table and the verifier rechecks it rather than trusting the frontend.

### 7. Conformance evidence

At least: a positive moving a `Region<T>` into one task; a positive sharing one
through `share(region)` and using the `Shared<Region<T>>` from two tasks; a
negative capturing a `Region<T>` handle into two tasks without `share`; a
negative capturing a `Region<mut T>` into a task; a negative capturing a
`DmaRegion<T>` into a task; a negative applying `share` to a `DmaRegion<T>` and to a
`Region<mut T>` under whichever code section 5 settles on; a negative using a
region after `share` consumed it (`E1301`); a negative writing through a
`Region<T>`; and a positive writing through a
`Region<mut T>`. Each capture and share negative has a forged-IR counterpart for
`V2021_REGION`, so the checker and the verifier prove the same rule
independently.

## Architecture impact statement

- **Change level:** 2 — the type facts, plus one predeclared operation and, once
  section 5 is settled, one diagnostic code. **Invariants affected:** none
  amended.
- **Canonical representation:** unchanged; no accepted source uses a region or
  `share` today, so nothing becomes invalid.
- **Threat-model impact:** positive on both counts. A mutable region crossing a
  task boundary is the shared-mutable case `docs/44` section 3 requires a
  negative for, and it becomes decidable. Keeping both DMA variants
  non-shareable closes the route by which a device-visible region could have
  reached several tasks through a `Copy` handle.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the eight conformance cases of section 5, checker unit tests per
  row of the table and for `share`, and verifier negatives for `V2021_REGION`.

## Consequences

Region rules become checkable, the shared-mutable negative the threat model
requires becomes expressible in source, and sharing is something a reader can
see rather than something that happens because two tasks named the same handle.

The cost is one narrow syntactic extension — `mut` inside two type arguments —
and a deliberately conservative DMA model that a later typed device contract
will have to widen explicitly.

## Alternatives considered

**Keep the mode in the capability contract and out of the type.** Rejected for
V1: it makes the fact invisible to a single-module check and to the IR type
table, so neither the checker nor the verifier could enforce it without
consulting an external contract the language does not name.

**Two more constructors, `MutRegion<T>` and `MutDmaRegion<T>`.** Rejected: four
names for two concepts, and the relationship between them would be spelled
nowhere.

**Let `Transferable` also mean shareable, so several tasks may hold a region.**
Rejected: it makes a duplication of an affine handle invisible, and the number
of holders would depend on how many tasks named it rather than on an operation
in the source.

**Make `DmaRegion<T>` shareable, since it is immutable.** Rejected: a
`Shared<DmaRegion<T>>` is `Copy`, so shareability is transitively a way across
the task boundary the DMA rule forbids. V1 stays conservative and a typed device
contract widens it later if it can say why that is safe.

<!-- END docs/adr/0037-region-transferability.md -->

---

<!-- BEGIN docs/adr/0038-module-root-precedence.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0038: TOS Core V1 module-root precedence and the exact `E1605` condition

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — fixes a stable diagnostic condition and the resolution
  rule conformance evidence depends on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

`docs/42` section 1 gives module resolution "a declared ordered list of module
roots and dependency source-set identities", and says "a missing or ambiguous
import is `E1604_IMPORT_NOT_FOUND` or `E1605_AMBIGUOUS_IMPORT`".

Those two sentences disagree. If the root list is ordered and the first match
wins, no import matching under several roots is ambiguous — the order decides —
and `E1605` names a condition the rule prevents. If instead more than one
candidate is ambiguous regardless of order, the order is doing something else,
and the document does not say what.

The implementation stopped at that boundary: it reports `E1605` only for the one
case it can decide without choosing between the readings — a declared source set
holding the same module name twice, where nothing in the input decides at all.

## Decision

### 1. The order is a search order, and shadowing is not silent

The declared list of module roots is searched in order. The **first** root that
declares a module name resolves that name. That makes resolution deterministic
and total.

Ordering settles roots, and only roots. It is not permission to paper over a
collision between *declared dependencies*: a name offered by more than one
reachable declared dependency source set is `E1605_AMBIGUOUS_IMPORT`, because
nothing orders dependencies against each other and choosing one would be an
implementation preference rather than a resolution rule.

The two sentences of docs/42 are reconciled this way. The order makes resolution
decidable for the ordinary case — a private root layered over a shared one — and
the code covers the case the order says nothing about.

### 2. The exact condition

`E1605_AMBIGUOUS_IMPORT` is reported when either holds:

1. the declared source set contains more than one module with the requested
   name inside one root, so nothing in the set orders them; or
2. more than one reachable declared dependency source set provides the
   requested name.

Otherwise the candidate in the earliest declared root resolves the name.

The diagnostic carries the requested import, the importer, the number of
candidates, and the identities that collided — the root for case 1, the
dependency source sets for case 2 — so the configuration mistake is nameable
without re-deriving it.

`E1604_IMPORT_NOT_FOUND` remains the case of no candidate at all. A missing
import takes precedence over an ambiguous one only when there is genuinely no
candidate; the two conditions are disjoint.

### 3. What resolution may read

Unchanged and restated because it bounds this rule: only the declared roots,
declared dependency source-set identities, the importer's own header, the
declared lock or manifest, and the effective import limit. Never an ambient
directory, the host filesystem outside those roots, the network, the clock, a
random source or an undeclared environment variable.

### 4. Conformance evidence

At least: a positive where a private root shadows a shared one and the first
root wins; a negative where two reachable roots declare the same name; and the
existing unit case where one source set holds a name twice. The first two need a
root-list input, so they are driver-level vectors rather than single files, and
the expectations table records them as such.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-15 is served by
  replacing two sentences that disagree with one rule.
- **Canonical representation:** unchanged.
- **Threat-model impact:** positive: `docs/44` section 3 requires an
  import-ambiguity negative, and this makes it precise.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the three cases above plus the mechanical gate binding the code to
  the registry.

## Consequences

`E1605` stops being a code whose condition the resolution rule prevents. A
layered root list — the ordinary way to override one module of a shared set —
keeps working, and a genuine collision between two declared dependencies is
named instead of silently decided.

## Alternatives considered

**First root always wins, `E1605` never fires.** Rejected: it makes an allocated
code unreachable and turns a dependency collision into a silent choice.

**Any multiple match is ambiguous, order is irrelevant.** Rejected: it breaks
layering, which is what an *ordered* list is for, and the document says ordered.

**Leave it to the compilation driver.** Rejected: resolution determinism is a
language property under `docs/42`, not a tool preference.

<!-- END docs/adr/0038-module-root-precedence.md -->

---

<!-- BEGIN docs/adr/0039-nonconstructible-opaque-types.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0039: `E1213_NONCONSTRUCTIBLE_TYPE` for opaque non-capability handles

- Status: Accepted (Project Architect-approved), revision 3
- Date: 2026-08-11
- Decision level: 2 — allocates a diagnostic code conformance evidence will
  depend on
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11
- Supersedes: revision 1, whose type set wrongly included `TaskResult<T>` and
  omitted `Shared<T>`; and revision 2, which promised the code for constructor
  and aggregate forms that V1 source cannot express in the first place

## Context

`docs/40` section 3 says an attempt to use `as` with a capability, region, DMA
region, task, synchronization object, function, closure or pointer-like host
value "is not a generic conversion error: it is `E1502_FORGED_CAPABILITY` for a
capability and **the corresponding nonconstructible-type error** for the other
opaque types".

No accepted document names that error. So the implementation reports nothing for
seven of the eight cases: casting a task, a region, a mutex, a closure or a
function is silently accepted by the type slice, because `E1212` is explicitly
excluded and nothing else applies. That is the gap recorded in `PROGRESS.md` as
an unresolved contract boundary, and it is the last one blocking a complete
`as`-conversion rule.

## Decision

### 1. `E1213_NONCONSTRUCTIBLE_TYPE`

Stage `type`. An operation attempts to bring into existence a value of a type
that V1 makes nonconstructible from source. The operations are:

- an `as` conversion whose target type is one of the nonconstructible types;
- an `as` conversion whose operand type is one of them.

That is the whole list, and it is short for a reason. A predeclared type is not
an expression primary or callee in V1, so `Event()`, `Task(1i32)` and
`Mutex(1i32)` are not fabrication attempts this code has to catch — they are
names that resolve to nothing in value position, and the frontend already
reports each as `E1202_UNKNOWN_VALUE_NAME`. Verified against the reference
frontend, not assumed.

Promising `E1213` for those forms would mean widening the grammar to let them
through to the type stage purely so a diagnostic could fire, which is a worse
outcome than the rejection they already get. The grammar is not widened, and any
future V1 operation that can genuinely express such a fabrication comes under
this code when it exists.

The nonconstructible types are: `Task<T>`, `Shared<T>`, `Region<T>`,
`DmaRegion<T>`, `Mutex<T>`, `RwLock<T>`, `Channel<T>`, `Event`, `Semaphore`,
`Barrier`, `Latch`, the three atomic types, `slice<T>`, and any function or
closure type.

`TaskResult<T>` is **not** among them. `docs/39` section 2 gives `Completed` and
`Cancelled` as predeclared constructors in expression position, so a
`TaskResult<T>` is an ordinary affine result value that source is meant to
build. What may not be fabricated is the `Task<T>` a join consumes, not the
value the join produces.

`Shared<T>` **is** among them. `docs/40` makes it the handle a typed `share`
contract yields; a cast or constructor producing one would manufacture sharing
that no operation granted.

The three guard types of ADR-0036 join this set when that ADR is accepted. They
are named here rather than assumed, because until it is accepted they do not
exist and this list would be citing types the contract does not have.

The diagnostic carries the type as spelled and which operation attempted it.

### 2. Precedence

1. a capability is `E1502_FORGED_CAPABILITY` — it is more specific and names
   authority, which is the thing that matters most;
2. any other nonconstructible type is `E1213_NONCONSTRUCTIBLE_TYPE`;
3. only a conversion between ordinary value types reaches
   `E1212_INVALID_AS_CONVERSION`.

One attempt produces one diagnostic. `E1212` is never reported for a type this
code covers, which is what `docs/40` section 3 means by "not a generic
conversion error".

### 3. What it does not cover

A nonconstructible value obtained the way the language provides — a task from
`spawn`, a guard from a lock, a region from a grant — is ordinary and correct.
This code is about constructing one out of data, never about holding one.

### 4. Conformance evidence

At least: a negative casting an integer to `Task<i32>`; a negative casting a
`Mutex<i32>` to an integer; a negative casting an integer to `Shared<i32>`; a
positive building a `TaskResult<T>` with `Completed` and `Cancelled`, proving
the code does not fire on a value source is meant to build; and a positive
obtaining a task from `spawn` and using it, proving it does not fire on the
legitimate path either.

A vector for `Event()` is deliberately absent: R-vectors record the code a form
actually produces, and that form produces `E1202_UNKNOWN_VALUE_NAME`.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended; I-09 is served —
  the code becomes part of the versioned diagnostic boundary; I-15 is served by
  replacing "the corresponding nonconstructible-type error" with a name.
- **Canonical representation:** unchanged. No accepted source becomes invalid:
  every case this rejects was already an error under `docs/40` section 3, with
  no code to report it.
- **Threat-model impact:** positive. Fabricating a task, a lock or a region out
  of integer data is the same class of forgery as fabricating a capability, and
  it was silently accepted.
- **Compatibility profile:** TOS Core 1.0.
- **Tests:** the five conformance cases, checker unit tests for both `as`
  directions, for every type in the set, for `TaskResult<T>` staying outside it,
  for a predeclared type in value position still being `E1202`, and for the
  precedence against `E1212` and `E1502`, and the mechanical gate.

## Consequences

The `as` rule of `docs/40` section 3 becomes completely implementable, and the
last silent acceptance in the type slice closes.

The cost is one more code fixed for TOS Core 1.0.

## Alternatives considered

**Reuse `E1212_INVALID_AS_CONVERSION`.** Rejected: `docs/40` section 3 says in
so many words that this is not a generic conversion error, and conformance
tooling could not tell a narrowing mistake from a forgery attempt.

**Reuse `E1502_FORGED_CAPABILITY` for everything opaque.** Rejected: a task is
not authority, and widening a capability code to cover non-authority values
would make every audit of that code less meaningful.

**Leave the `as` cases unreported.** Rejected: it leaves a stated rule
unenforced and a forgery path open.

**Widen the grammar so `Event()` reaches the type stage and gets `E1213`.**
Rejected: it would change what V1 source *is* to improve a diagnostic on a form
that is already rejected, and a grammar that admits nonsense so a later stage can
name it is worse than one that does not admit it.

<!-- END docs/adr/0039-nonconstructible-opaque-types.md -->

---

<!-- BEGIN docs/adr/0040-stage2-reference-platform.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0040: the Stage 2 reference platform profile

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — fixes the platform a Stage 2 performance gate is measured
  on, which every later performance claim is stated against
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

`docs/35` gives Stage 2 two budgets for the bootstrap profile — parse,
type-check, lower and verify a 256 KiB canonical module within 500 ms p95, and
execute the standard one-million-operation integer and control-flow benchmark
within ten times "the host reference interpreter time under the same semantic
implementation" — and an evidence ladder where P1 is locally measured and no
stage closes on P0 for its own metric.

It names a reference platform for Stage 1: the mandatory
q35/qemu64/one-vCPU/256-MiB/TCG functional profile. It names none for Stage 2.

Without one, "reference platform" would end up meaning whichever machine
produced an agreeable number, which is measurement chosen after the fact. A
platform has to be fixed before a measurement is taken for the measurement to
mean anything.

## Decision

### 1. The Stage 2 reference platform is the Stage 1 profile

Stage 2 performance evidence is taken on the same profile Stage 1 already
mandates:

```text
machine      q35
cpu          qemu64
vcpus        1
memory       256 MiB
accelerator  TCG (no hardware virtualization)
firmware     the declared OVMF build of the Stage 1 gate
```

One platform for both stages, for three reasons. It already exists and is
already gated, so no second environment has to be kept honest. TCG on one vCPU
is deterministic enough that two runs are comparable. And a single profile keeps
Stage 1 and Stage 2 numbers comparable, which they would not be if each stage
picked its own.

A record taken here demonstrates conformance **on this declared platform**. It
says nothing about performance on other hardware or other emulators, and it is
not evidence that a budget met here is met anywhere: a different CPU, a
different accelerator or a different memory system can be faster or slower for
reasons this profile does not model. The value of a fixed platform is
comparability across runs and across stages, not extrapolation.

### 1a. The measurement must run the real Stage 2 path

The reference measurement executes the actual Stage 2 TOS runtime and recovery
path. Running `tos-engine` inside an arbitrary Linux or host guest under the
profile does **not** satisfy this gate: the guest's libc and host OS would
become a runtime dependency of the measured path, which is exactly the
dependency `docs/44` says is not a recovery or runtime dependency. A number
produced that way would measure the host, and would let a host runtime enter the
Stage 2 story through the performance gate.

Native-host execution remains admissible as the **comparison baseline** of
section 2 — that is what a baseline is for — and is never the production or
reference execution path.

The freestanding runtime this requires is the subject of the runtime-
independence audit; until it exists, the reference half of the pair cannot be
taken, and that is an open gate rather than a number taken elsewhere.

### 2. What "host reference interpreter time" means

`docs/35` states the execution budget as a ratio against "the host reference
interpreter time under the same semantic implementation". That is the **native
host** execution of the **same** reference interpreter — the same
`tos-engine`, the same commit, the same workload — not a second semantic
implementation.

So the execution metric is a pair of measurements and their ratio:

```text
reference   the benchmark under the profile of section 1
native      the same benchmark, same commit, same engine, on the native host
ratio       reference / native            budget: at most 10
```

This is deliberately written down because the sentence admits a second reading —
a different implementation of the same semantics — and building one purely to
satisfy a ratio would be a worse outcome than stating which reading holds.

### 3. Sampling and retention

Unchanged from `docs/35` and restated so a record is checkable: three warmups,
twenty-one samples, median, p95 and p99 retained with the raw samples, the
source commit, the toolchain and build identity, the profile, and the cache
state. Both halves of the ratio are recorded, never just the quotient.

### 4. Evidence level

A record taken under section 1 with section 3's retention is **P2** when it is
produced by the repository's own reproducible gate, and **P1** when produced by
hand. A record from any other machine is P1 and names the machine; it may
support a stage's progress but does not close its gate.

### 5. What this ADR does not decide

It does not set new budgets, change the numbers in `docs/35`, or say anything
about a Full-profile or multicore reference platform. Those need their own
decision when a Full engine exists.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended.
- **Canonical representation:** unchanged. **Trusted-base impact:** none.
- **Threat-model impact:** none directly; a fixed platform makes a resource-
  exhaustion measurement comparable across runs, which the abuse evidence in
  `docs/44` section 3 depends on.
- **Recovery and rollback impact:** none.
- **Stage identity gate:** no gate is claimed or closed by this ADR. It states
  where the Stage 2 performance gate is measured, not that it has been.
- **Compatibility profile:** the profile is fixed for Stage 2 and changes only
  through a versioned decision, because changing it silently would invalidate
  every retained comparison.
- **New dependencies:** none. Both halves of the ratio use tooling the
  repository already has.
- **Tests:** the harness records which profile it ran under and refuses to
  present a P2 claim for a record taken elsewhere. A reference-profile record is
  admissible only from the real Stage 2 runtime path of section 1a.

## Consequences

The Stage 2 performance gate has a place to be measured, chosen before the
measurement rather than after it, and the execution budget has one reading. The
remaining work is mechanical: run the harness under the profile, retain both
halves, and compute the ratio.

The cost is that a Stage 2 performance number is a TCG number, so it is slower
than the hardware a developer sits at, and it is a statement about this platform
only. That is the intended trade: the alternative is a number whose platform was
chosen to suit it, which states nothing at all.

## Alternatives considered

**Declare the maintainer's machine the reference platform.** Rejected: it is
choosing the platform after seeing the result, and no one else can reproduce it.

**Define a new Stage 2-specific QEMU profile.** Rejected for now: a second
profile is a second thing to keep gated and honest, and nothing about the Stage 2
metrics needs anything the Stage 1 profile lacks.

**Build a second semantic implementation to satisfy the ratio.** Rejected: it
would exist only to produce a denominator, and a throwaway implementation is a
worse oracle than none. The native-host reading of section 2 measures what the
budget is actually about — how much the reference platform costs — using the
implementation that already exists.

<!-- END docs/adr/0040-stage2-reference-platform.md -->

---

<!-- BEGIN docs/adr/0041-runtime-memory-grant.md -->

<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0041: `RuntimeMemoryGrantV1` — the nucleus-to-runtime memory contract

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-11
- Decision level: 2 — introduces a versioned interface between the nucleus and
  the Stage 2 reference runtime, and fixes how implementation memory is
  distinguished from a module's declared resource envelope
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-11

## Context

The runtime-independence audit found one genuine gap. The Stage 2 crates use
only `core` and `alloc` facilities, and the freestanding target already builds
and is gated — but `alloc` needs a `#[global_allocator]`, the nucleus has none
and does not use `alloc` at all, and no accepted document says who owns memory
in Stage 2 before the Stage 3 process substrate exists.

Until that is settled the `docs/44` claim — that libc, the C ABI and host
threads are not runtime or recovery dependencies — cannot be discharged, the
ADR-0040 reference measurement cannot be taken on the real path, and Stage 2
cannot be candidate-complete.

## Decision

### 1. The nucleus grants; the runtime never discovers

The nucleus already owns the physical and virtual memory mechanism before Stage
3. It hands the Stage 2 reference runtime **one bounded region**, and that
region is the runtime's only heap backing store.

```text
RuntimeMemoryGrantV1 {
  version           the grant contract version
  base              start of the granted region
  length            bytes granted
  alignment         guaranteed alignment of `base`, a power of two
  identity          which nucleus build produced the grant
}
```

The runtime does **not** probe a memory map, walk firmware tables, or acquire an
ambient allocator. It receives a base and a length or it does not run. Discovery
is the nucleus's job and stays there.

The granted memory comes from memory the nucleus already legitimately owns or
has reserved from a validated memory topology. Explicitly not: host `malloc`,
libc, UEFI allocation after runtime handoff, a hidden C ABI, or a Stage 3
process allocator.

### 2. `BootInfo v1` is not touched

`BootInfo v1` is the loader-to-nucleus contract. Its size, version and reserved
rules, and the Stage 1 evidence that pins them, stay exactly as they are.

`RuntimeMemoryGrantV1` is a **different** interface — nucleus to Stage 2
runtime — with its own version. Widening `BootInfo v1` to carry it would change
a contract Stage 1 closed on, and would do it for a consumer that did not exist
when it was written. Two interfaces with two versions is the honest shape.

### 3. Two limits that must never be confused

**Implementation heap capacity** is `RuntimeMemoryGrantV1.length`: the physical
memory the parser, lowerer, verifier and interpreter have as an implementation.

**A module's resource envelope** is `resource [allocation: ...]`: the semantic
budget of the TOS program being executed, already enforced by the engine before
the effect.

These are separate quantities with separate failures, and neither may stand in
for the other:

- a module declaring `allocation: 4KiB` gets 4 KiB of semantic budget, never a
  claim on the whole arena;
- exhausting the implementation arena must **not** be reported as that module's
  `RUNTIME_ALLOCATION_LIMIT`, because it is not a fact about the module;
- exhausting a module's declared budget must not be reported as an
  implementation failure, because the program is the thing at fault.

### 4. Allocation failure discipline

A module that spends its declared budget gets a defined resource refusal before
the effect. That is settled and already implemented.

Exhausting the **implementation** arena on valid input inside the published
`docs/44` hard limits is a different matter, and it may **not** be an ordinary
panic or halt. One of the following, or both:

- fallible allocation — `try_reserve` and its equivalents — so the runtime
  refuses the work rather than dying; or
- a proved upper memory bound for the published limits, and an arena at least
  that large.

`alloc_error_handler` may halt, but only as an implementation-invariant failure
— the equivalent of an assertion — never as the ordinary response to
attacker-controlled input that is valid and within bounds.

### 5. The allocator itself

A bump allocator that leaks irreversibly between ordinary operations is **not**
accepted unless a lifetime or reset contract proves bounded long-term behaviour.
A reference runtime that must be restarted to reclaim memory is not a recovery
oracle.

The preferred shape is a small, auditable, bounded allocator with real reclaim,
or an equivalent scheme whose long-term behaviour is proved. It should remain
useful as the permanent recovery and reference-runtime allocator after Stage 3
arrives, rather than being a shim to discard.

Every new `unsafe` site is minimized, documented with its SAFETY invariants,
entered in the unsafe inventory, and covered by adversarial tests.

### 6. Evidence this decision owes

- the grant is a declared input: a runtime given no grant runs nothing;
- repeated executions do not grow arena use without bound;
- a reset or recovery path returns the allocator to a documented state;
- arena exhaustion on valid bounded input is a refusal, not corruption and not a
  halt;
- module-envelope exhaustion and arena exhaustion are distinguishable in the
  diagnostic record;
- the freestanding artifact has no dynamic dependency.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none amended. I-15 is served:
  who owns Stage 2 memory stops being unstated.
- **Canonical representation:** unchanged. **Trusted-base impact:** the nucleus
  gains one grant responsibility; it already owns the mechanism.
- **Threat-model impact:** positive. An arena with a declared length bounds what
  a malicious module can make the implementation consume, and separating the two
  limits stops one from masking the other.
- **Recovery impact:** positive, provided section 5 holds — a reference runtime
  that reclaims is usable as a recovery oracle.
- **Stage identity gate:** none claimed. This unblocks the Stage 2
  runtime-independence evidence; it does not supply it.
- **Compatibility profile:** `RuntimeMemoryGrantV1` is versioned from the start
  and changes only through a versioned decision.
- **New dependencies:** none. No libc, no WASI, no C ABI, no host shim.
- **Tests:** section 6, plus the freestanding build gate and the unsafe
  inventory.

## Consequences

The `no_std` conversion and the freestanding runtime become ordinary bounded
work, and the ADR-0040 reference measurement gets a real path to run on. The
cost is one new versioned interface and one allocator to write and audit — both
things the system needs regardless of Stage 2.

## Alternatives considered

**Widen `BootInfo v1`.** Rejected: it changes a contract Stage 1 closed on, for
a consumer that did not exist when it was written.

**No allocator; fixed-capacity storage everywhere.** Rejected: it rewrites the
components that most need to stay reviewable, and makes worst-case memory the
always case.

**Wait for Stage 3 to own memory.** Rejected: Stage 2's identity question is
whether actual language semantics execute, and with a host runtime underneath,
that execution is a host execution.

<!-- END docs/adr/0041-runtime-memory-grant.md -->

---

