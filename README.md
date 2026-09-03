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

The supported reference environment is x86_64 Linux with:

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
`TOS.HALT ok=0x10`, then the production nucleus stays halted so the final boot
screen — the Pyro mascot over `Stage 2 runtime complete.` /
`System halted normally.` — remains visible until you close QEMU or press
Ctrl+C. Its serial log is retained alongside the image preparation evidence.

For a headless automated check, run:

```sh
./run-tos.sh --check
```

To watch the Stage 3 system — the supervisor written in TOS Core — see
[Try the Stage 3 system](#try-the-stage-3-system) below.

Serial and filtered event evidence is retained under
`source/target/run-tos/interactive/` or `source/target/run-tos/check/`, in
`serial.log` and `events.log`. `--check` is the self-judging mode: it enables
`isa-debug-exit`, returns raw QEMU exit 33 on success and prints
`QEMU-TEST PASS`. The interactive display is a human-facing representation of
the already-validated boot state; serial events remain the machine-readable
evidence.

The screen is not a desktop, shell, terminal or GUI subsystem, and it has no
input. It is a bounded, best-effort boot console drawn directly to the
validated RGBX8/BGRX8 framebuffer, and it shows only what the system has
actually done: each boot step appears as `[ .. ]` before the step runs and
becomes `[ OK ]` once it has returned. The console is created only after the
boot ABI record and the memory map have been accepted, so the two facts already
established when it opens are drawn retrospectively and everything after them
is drawn live.

On a successful boot the log has done its work and is replaced by a final
screen: the separately identified CC-BY-SA-4.0 Pyro artwork, and under it

```text
Stage 2 runtime complete.
System halted normally.
```

which is exactly what happened — the Stage 2 runtime finished and the machine
halted. TOS does not continue into an interactive system at this stage, and the
screen does not claim that it does. On failure the screen is not cleared: the
steps that succeeded, the step that failed and its diagnostic code and location
stay visible, and the mascot is not shown.

The artwork's checked source/provenance relationship is recorded in
`assets/mascot/pyro-stage1-provenance.json`. The console never affects a boot
outcome; the serial `TOS.*` / `TOS.RUN.*` events remain the normative,
machine-readable evidence, and when no framebuffer is available the boot is
identical apart from the picture.

Stage 1 is formally closed as a bootable TOS foundation with source-bound
capsule identity and fail-closed validation. Stage 1.5 is formally closed with
ADR-0027's bespoke TOS Core foundation selection. Stage 2 is formally closed:
canonical TOS Core source executes through the production reader, parser,
checker, deterministic `tos-ir/v1` lowerer, independent verifier and bounded
engine. **Stage 3 is formally closed** (2026-09-03) — capabilities, IPC,
regions, funded process creation, the build-to-bundle lifecycle and a
supervisor written in TOS Core. TOS is not yet a user shell, application
environment, or desktop operating system, and Stage 4 has not begun.

## Try the Stage 3 system

```sh
./run-tos.sh --stage3
```

This boots the closed Stage 3 system rather than a demonstration of it: the
same capsule, nucleus and scenario the `qemu_supervision` gate uses. Three
canonical TOS Core modules go into the capsule —
`/system/policy/services.tos` (the policy), `/system/boot/init.tos` (the
supervisor) and `/system/boot/worker.tos` (the service) — and everything below
happens on the real freestanding path: source parsed, checked, lowered,
encoded, verified by an independent verifier, and run in processes the nucleus
creates out of a presented memory authority.

**What you will see.** After the build, a narrative of what the supervisor did,
derived from the boot's own diagnostic transport:

```text
  [capsule ] path=system/boot/init.tos modules=3  the source set this boot runs
  [granted ] binding=process interface=system.process.Control
  [blocked ] system/boot/worker.tos               a dependency is not running
  [start   ] system/boot/worker.tos               policy permits starting
  [created ]                                      process created
  [exit    ] process=1 self_reported_status=0     a process reached its own end
  [failure ] system/boot/worker.tos               the service itself failed
  [restart ]                                      inside the window, restart permitted
  [failed  ]                                      FAILED, and it latches
  [latched ]                                      not started: already FAILED
```

Then the same run at operator severity — `WARN` and above, and nothing else.
That is the important-error view of `RUNTIME_OBSERVABILITY_V1` §9; both views
are selections of one transport, not a second log.

**Where the evidence is kept.** `source/target/run-tos/stage3/`:

- `serial.log` — every byte the machine emitted;
- `events.log` — the `TOS.*` events with firmware chatter removed.

Read them yourself at any time:

```sh
python3 scripts/tos-journal.py --story source/target/run-tos/stage3/serial.log
python3 scripts/tos-journal.py source/target/run-tos/stage3/serial.log
```

### Change the policy and watch it behave differently

The policy is canonical TOS Core source, and editing it is the point. Open
`source/tests/vectors/supervision/services.tos` and find the restart budgets:

```tos
pub fn max_attempts(at: size) -> size {
    let budgets: array<size, 3> = [2B, 2B, 2B];
    return budgets[at];
}
```

Two failures inside a service's window exhaust its budget, so the default run
latches two services into terminal `FAILED`. Change the budgets to `4B` and run
`./run-tos.sh --stage3` again: with more room, the same failures are restarted
instead, and only one service latches. The `[restart]` and `[failed]` lines
change accordingly.

Two other figures are worth trying, in the same file:

- `window(at)` — the width, in boot-monotonic ticks, of the interval a failure
  counts in. The third service's window is `1u64`, which is why its failures
  never accumulate however many there are;
- `has_requirement(at)` / `requires(at)` — which service must be running before
  another may start. The first service depends on the third, which is why the
  run opens with `[blocked]`.

When you are finished experimenting, restore the file:

```sh
git checkout -- source/tests/vectors/supervision/services.tos
```

**The QEMU window.** `./run-tos.sh --stage3 --interactive` also opens the boot
display. It shows the *boot* console — the same one `./run-tos.sh` shows — and
not the supervision story. As with `./run-tos.sh`, the window stays open after
the boot halts so you can look at it; close it or press Ctrl-C, and the story
and operator views are printed on the terminal. Only `serial.log` is retained in
this mode, because the interactive path writes no filtered event log.

There is no shell, no keyboard input and no way to interact with the running
system. Stage 3 did not build one, and the window does not pretend otherwise.

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

Stage 0, Stage 1, Stage 1.5, Stage 2 and **Stage 3** are formally closed. Each
closure approval is archived in `source/legal/publication-records/`; Stage 3 was
closed by the Project Architect on 2026-09-03 for evidence commit `77970cb`,
against `docs/evidence/STAGE3_CLOSURE_AUDIT.md` — 60 audited obligations, 56
closed, none blocking. Stage 4 has not begun.

What runs today, on the real freestanding boot path: the UEFI loader, the
nucleus, a verified ring-3 runtime image, processes created and funded out of a
presented `MemoryAuthority`, and canonical TOS Core source taken through the
production reader, parser, checker, resolver, `tos-ir/v1` lowerer, independent
verifier and bounded engine. Above that: capabilities and IPC with counted
bounds, regions with a three-state lifecycle, launch plans, a build-to-bundle
lifecycle whose target verifies its own artifact, and a **supervisor written in
TOS Core** that reads canonical policy from `/system/policy/`, restarts services
against a failure-density window, and writes an operator-visible journal. All of
it is covered by QEMU gates.

Measured on the reference platform: absolute IPC latency `p99 = 39.147 µs`
against the accepted `≤ 200 µs` bound, at evidence level P2.

ADR-0030 (external vendor opaque material and `/vendor`), ADR-0031 with
`docs/45_SYSTEM_SOURCE_HIERARCHY.md` (runtime system source hierarchy) and
ADR-0032 (parser diagnostics and recovery) are accepted; their implementation is
deferred to the stage that first needs each subsystem. No implementation
decision may silently contradict an accepted ADR or invariant. Legal documents
are project policy, not jurisdiction-specific legal advice.
