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
supervisor written in TOS Core. **Stage 4A and Stage 4B are closed**
(2026-09-04) — canonical TOS Core holds a platform root, claims a real PCI
function, reads its configuration space, finds the VirtIO capability structures
itself, derives a bounded window on the BAR they name, and reads the device's
registers, with the nucleus holding mechanism only.

**Stage 4C and Stage 4D are built and green, and neither is formally closed.**
Canonical TOS Core now derives a routed interrupt of a real PCI function and is
woken by a real MSI-X message (4C-1), allocates a DMA region from two
authorities and gives it back under a proved drain (4C-2), and orders its
device-visible writes explicitly (4C-3). On top of that it configures a real
split virtqueue and the device accepts it (4D-1), performs a real
`VIRTIO_BLK_T_IN` whose proof is a sentinel the device had to overwrite (4D-2),
serves two sequential real block reads through one initialized queue by
reclaiming and reusing the first request's descriptors (4D-3), holds two
requests outstanding at once (4D-4), and performs a real block **write** proved
by an independent read-back (4D-5). Every byte of PCI, VirtIO and block-protocol
knowledge is in canonical text; the nucleus knows none of it.

**And since 2026-09-21 the driver is reachable from another process.** A
canonical textual client that holds *nothing* of the machine — no PCI bus, no
function, no mapped window, no interrupt source, no DMA region, no memory
authority — and that has no name for the service until a textual registry sends
it one in a message, calls that service over IPC, and the number it gets back
originated in a real device read: the service's buffer was 0xA5 until the device
wrote it, and a run with no DMA would have answered zero. The registry is an
ordinary textual service, as ADR-0093 decided; the nucleus gained nothing.

**And since 2026-09-23 the bytes themselves cross.** All 512 bytes of one sector
reach that client as an ordinary immutable `Region<u8>`: it requests a sector and
hands over a channel, the service performs the real VirtIO/DMA read, copies the
bytes once out of device-visible memory — the copy ADR-0037 forces rather than one
anybody chose — freezes the region and sends it through the message's region area,
and the client indexes every byte. That is the data path
`docs/research/STAGE4_DATA_PATH_BOUNDARY.md` §1 describes, in the read direction.
The client holds no hardware authority and cannot even request the region
interface, so what it reads can only have arrived in a message; the bytes are
counted in canonical text, and one corrupted byte fails the gate.

**And since 2026-09-25 what is written outlives the process that wrote it.** A
`state.store.v1` store keeps 64 objects of 512 bytes in a bounded 65-sector extent on
that device, reached only through `block.device.v1`. An initializer formats a zeroed
device once. A writer puts two objects and ends. The store's first generation ends, is
retired and is collected — and then the **same initializer runs again**, meets a header
whose occupancy now says two objects are there, and refuses to format: only an all-zero
sector 0 is permission, because an unconditional write would lose both objects while
reporting success. Its successor generation — the same canonical module from the same
sealed launch plan, handed nothing in memory — re-reads and validates the header from
the device, and a reader that holds no way to address a sector gets one of those objects
and checks all 512 bytes in canonical text. That last read is the witness for both
claims at once. An id that was never created is refused as absent and its sector is
never read: presence is the occupancy bitmap and nothing else.

**A valid header is not enough to be open.** The store asks the layer below for the
device's capacity before it reads sector 0, because a store on a device too small to
hold the extent is not a store — and a second boot against a 64-sector image proves it,
answering every request `ST_STORE` and reading sector 0 not at all.

**What that is not**: durability. No power-loss guarantee, no `FLUSH`, no crash
consistency, no transactions — and no Stage 4 closure.

The publication authority *is* the one `CAPABILITY_V1` §6 accepts, as ADR-0095
amended it: a dedicated publication endpoint whose identity fixes what may be
published through it, with the registry holding `receive` and the authorised
service holding `call`, so no interface name travels in the protocol and a process
that cannot name that endpoint cannot publish.

**What is still not proved**, stated because it is easy to overstate: one sector,
one client, one service, and reading only. Writing through this path, more than one
sector in flight, request framing and zero-copy are none of them designed — and
ADR-0037 makes zero-copy unreachable by decision rather than by omission. Nothing
here is the capsule-to-repository handoff *in full*; **persistent object storage is a
separate deliverable and is now built** — `state-store.sh`, described further down.

**The first half of the capsule-to-repository handoff is built** (ADR-0102,
`repository-linkage.sh`). Canonical TOS Core holding SHA-1, SHA-256, a bounded
stored-block inflater and the minimum Git parser reads a repository extent from the
same device the state store lives on, verifies every object against the id it was
located by, and proves that `SHA-256` of the repository's
`source/system/boot/init.tos` at the commit the capsule names is the digest the
nucleus computed over the file it booted. The commit id and that digest reach it
through one new nucleus operation that *reports* an already-verified boot fact; the
nucleus gains no Git, no hashes, no inflate and no repository traversal.

**What that is not**: a repository-backed `/system`. The system commit id is still
absent and the existing test still says so. No refs, no writes, no packfiles, no
history traversal, no activation, no rollback — and no cross-reboot persistence,
because the harness re-creates the disk image for every run and no gate in this
repository has ever shown a byte surviving a reboot. `docs/36` G2 owns the second
half.

TOS is not yet a user shell, application environment, or desktop operating
system. What it does with a disk is single sector reads and one write, reached
through one service: there is no filesystem, no partition handling, no request
scheduling, no multi-device discovery and no driver framework.

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

Stage 0, Stage 1, Stage 1.5, Stage 2, **Stage 3**, **Stage 4A** and **Stage 4B**
are formally closed, and every one of those closure approvals is archived in
`source/legal/publication-records/`. Stage 3 was closed by the Project Architect
on 2026-09-03 for evidence commit `77970cb`, against
`docs/evidence/STAGE3_CLOSURE_AUDIT.md` — 60 audited obligations, 56 closed,
none blocking. **Stage 4A — the hardware authority boundary and real textual PCI
configuration access — was closed on 2026-09-04** for evidence commit
`2655aaa`, against `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md`, ADR-0079 and
ADR-0080; that closure approves no BAR/MMIO mapping, device-memory semantics,
IRQ, DMA, IOMMU, reset, queue setup, block I/O, persistent storage or repository
handoff. **Stage 4B — BAR/MMIO and real textual VirtIO PCI capability discovery
— was closed on 2026-09-04** for evidence commit `ec03210`, against
`docs/evidence/STAGE4B_MMIO_BOUNDARY.md` and ADR-0081; that closure implies no
IRQ, DMA, Virtqueue, block-I/O or reset semantics.

**Stage 4C and Stage 4D are built and gated but not closed**, and no closure is
claimed for them here. Their evidence is
`docs/evidence/STAGE4C_LIVENESS.md`, `STAGE4C2_CAPABILITY_REPRESENTATION.md`,
`STAGE4C3_DMA_ORDERING.md`, `STAGE4D1_FIRST_VIRTQUEUE.md`,
`STAGE4D2_FIRST_BLOCK_READ.md`, `STAGE4D3_QUEUE_REUSE.md`,
`STAGE4D4_TWO_OUTSTANDING.md` and `STAGE4D5_FIRST_WRITE.md`, with ADR-0082
(routed interrupt authority), ADR-0084 (DMA authority and device-visible
addressing) and ADR-0086 (DMA publication and consumption ordering).

The frontier is the **client/service boundary**, not more device work: a
canonical textual client holding no part of the machine reaches the driver over
IPC and receives an answer that originated in a real device read
(`qemu_block_service`), having been given the service's endpoint by a textual
registry it looked it up through (`qemu_name_service`, ADR-0093 P3), over a
capability that crossed in a message (`qemu_capability_transfer`, ADR-0094). The
device side of that slice is 4D-2's and re-proves 4D-2's facts and no more.

It does **not** prove queue multiplexing, scheduling, filesystem integration or a
generic driver subsystem. None of those is designed.

Above that boundary two Stage-4 deliverables are built and neither closes the
stage: **persistent object storage** (ADR-0099, `STATE_STORE_V1`,
`qemu_state_store`) and **the first half of the capsule-to-repository handoff**
(ADR-0102, `qemu_repository_linkage`) — the linkage, which is the statement that
the canonical text this machine booted is the `source/system/boot/init.tos` of the
commit the boot chain verified. Its evidence is twenty-seven boots: one in which
two successive reader generations each reach the witness from the same device,
twenty-three refused with the exact class ADR-0102 §10a fixes, and three that hold
the boot identity to the rules every capability in this system obeys — a process
not endowed it cannot read it, attenuation is intersection and an empty
intersection is refused, and an endowment description of that object kind with a
non-zero scope does not start a boot.

**And since 2026-09-23 a service can die and be replaced without losing data.** A
block service serves a write and ends still holding the function, the mapped
window, the interrupt source and the DMA region; a supervisor collects its ending,
withdraws its publication from the registry and only then starts a successor; the
successor claims the same function at a **new assignment generation**, drives
VirtIO `DEVICE_STATUS` to 0 and reads it back as 0, initializes the device again in
the ordinary order, rebuilds queue, DMA region, window and interrupt source, and
republishes. A fresh lookup then gives the client an endpoint through which all 512
bytes the first instance wrote come back. That is ADR-0092 R1a's T1 recovery and
ADR-0093's case C, and the stale capability is the experiment: the client still
holds the name its first lookup gave it, calls it while the successor is waiting
for a request, and is not served — so the name was never repaired.

**And since 2026-09-24 there is a normative protocol under it.** `block.device.v1`
has a wire shape: ADR-0098 answers ADR-0093 §9 and
`source/interfaces/device/BLOCK_DEVICE_V1.md` states it — `word = sector * 4 +
opcode`, all three of `read`, `write` and `capacity`, every refusal a reply, and a
**write that arrives as one atomic call carrying its own region** rather than as a
region followed by a call, which was correct only where one client was serialized
against itself. A canonical textual service and a canonical textual client speak it
in `block-protocol.sh`, where a decoy region sent just before a write proves the
bytes written are the ones that call carried, and the journal proves a read replies
before it sends. `capacity` is the device's own number, proved by a second boot
against a smaller device from the same compiled module.

**Read the rest at the width of the fixtures, which is narrow.** The older
`block-data-path` and `block-lifecycle` boots still carry their own encoding until
a later slice migrates them. One sector per request, no batching, no multi-sector
scheduling and no filesystem — and there is no capsule-to-repository handoff.

**Persistent object/state storage is built** (ADR-0099, `STATE_STORE_V1`), and
`state-store.sh` is what it means. `state.store.v1` sits over `block.device.v1` over
the reference device, every layer canonical text. Two boots and eight modules.

The ordinary boot: an initializer formats a zeroed device; a store generation opens by
asking the layer below for the capacity and then validating the header; a writer puts
two objects and ends; that generation ends, is retired and is collected; the **same
initializer** then runs again as an adversarial re-provisioning probe against a header
with two occupancy bits set, with no state service alive, and must refuse; a successor
generation of the same module re-reads and validates the header from the device; and a
reader — holding no way to address a sector — gets object 2 and checks all 512 bytes in
canonical text.

Every refusal the contract defines carries evidence, by exact reply word where a caller
can read one: a reserved opcode, an id off either end of the range, a `PUT` with no
region, a `GET` with no answer endpoint, and an id never created. A length that is not
eight is proved by an answered call plus the store's own account, because the only row
that lets a client choose a length produces a status and not an answer. And a second
boot against a 64-sector image — one short of the extent — answers five differently
shaped requests with `ST_STORE` and reads sector 0 not at all.

An incomplete lower operation is **not** a refusal: the store keeps "the layer below
succeeded", "the layer below refused" and "nobody refused anything" apart, and only the
middle one becomes `ST_BLOCK`. All nine mutations ADR-0099 §13 requires turn the gate
red, each on its own assertion.

**What that does not mean.** No power-loss durability, no `VIRTIO_BLK_F_FLUSH`, no
crash consistency, no journaling, no transactions, no exactly-once `PUT`, no delete, no
enumeration, no second owner or store, no `docs/09` `/state` namespace, no path
semantics — and **no Stage 4 closure**: Stage 4C, Stage 4D and Stage 4 remain open.
`ST_BLOCK` is implemented and deliberately **not** exercised: the conforming reference
endpoint answers every well-formed in-range request successfully, and no device failure
is manufactured to colour the row green.

**What Stage 4 still owes, from `docs/16`'s own deliverable list**, named
separately rather than collected under one word:

- **the capsule-to-repository handoff** — a deliverable distinct from persistent
  object/state storage, which is built, and distinct from durability, which is not
  claimed by either;
- **a crash in flight**: the lifecycle boot's first instance ends after
  acknowledging its write, so it asks nothing about a request accepted and never
  answered — ADR-0093's case D, which stays deliberately ambiguous — and an
  adversarial-device suite is separate again;
- the **Stage 4 performance contract report**;
- current-state documentation, and a closure review.

What runs today, on the real freestanding boot path: the UEFI loader, the
nucleus, a verified ring-3 runtime image, processes created and funded out of a
presented `MemoryAuthority`, and canonical TOS Core source taken through the
production reader, parser, checker, resolver, `tos-ir/v1` lowerer, independent
verifier and bounded engine. Above that: capabilities and IPC with counted
bounds, regions with a three-state lifecycle, launch plans, a build-to-bundle
lifecycle whose target verifies its own artifact, and a **supervisor written in
TOS Core** that reads canonical policy from `/system/policy/`, restarts services
against a failure-density window, and writes an operator-visible journal.

And, on real hardware the emulator presents: a **canonical textual VirtIO block
driver** that claims one PCI function, walks its capability list, maps the BAR
window the device names, negotiates `VIRTIO_F_VERSION_1`, builds a split
virtqueue in a DMA region it was granted, publishes a descriptor chain,
notifies the device at the location its own capability structure gives, is woken
by a real MSI-X interrupt, and reads real sectors back — twice through the same
queue with the first request's descriptors reclaimed and reused, two requests
outstanding at once, and one real write proved by an independent read-back. And
that driver is now a **service**: another textual process, holding no part of the
machine, reaches it over IPC through an endpoint a textual registry handed over.
All of it is covered by QEMU gates.

Measured on the reference platform: absolute IPC latency `p99 = 39.147 µs`
against the accepted `≤ 200 µs` bound, at evidence level P2.

ADR-0030 (external vendor opaque material and `/vendor`), ADR-0031 with
`docs/45_SYSTEM_SOURCE_HIERARCHY.md` (runtime system source hierarchy) and
ADR-0032 (parser diagnostics and recovery) are accepted; their implementation is
deferred to the stage that first needs each subsystem. No implementation
decision may silently contradict an accepted ADR or invariant. Legal documents
are project policy, not jurisdiction-specific legal advice.
