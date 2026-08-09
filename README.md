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
ADR-0027's bespoke TOS Core foundation selection. Stage 2 Part A is preparing
the accepted semantic/IR contract and programmer documentation; Stage 2 Part B
production reference implementation is authorized but has not started. TOS is
not yet a user shell, application environment, or desktop operating system.

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
    - Accepted Stage 2 V1 contract: `docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md`
      through `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`
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

Stage 0, Stage 1, and Stage 1.5 are formally closed. The repository is in
Stage 2 Part A: proposed TOS Core V1 semantic/IR contracts, documentation, and
conformance corpus are under Architect review before the first production
frontend/runtime implementation. No implementation decision may silently
contradict an accepted ADR or invariant. Legal documents are project policy,
not jurisdiction-specific legal advice.
