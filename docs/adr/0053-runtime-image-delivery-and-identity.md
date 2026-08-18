<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0053: How the ring-3 runtime image reaches the machine

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-17
- Decision level: 3 — it decides whether a Stage-1-closed contract admits a
  derived binary artifact, and it either confirms or narrows a sentence of the
  accepted ADR-0048
- Project Architect approval:

## Context

ADR-0048 moved TOS Core execution to CPL 3, one runtime instance per process,
and stated the consequence in a sentence that is now load-bearing:

> **The engine becomes a per-process derived artifact with an identity.** A
> ring-3 runtime is a binary loaded into each process. […] the capsule must
> carry the runtime image, its identity must be reported, and it is a derived
> artifact whose provenance rules (AGENTS.md §9) apply in full.

Stage 3 Phase 2 has reached the task that implements it. Tasks 1–3 built the
substrate underneath: the nucleus owns physical frames, builds its own address
space, and a payload has executed at CPL 3 and called back through
`SYSTEM_ABI_V1`. What is missing is the thing that should be running there.

## The gap, measured rather than assumed

The capsule *format* does not forbid binary content: `CAPSULE_FORMAT_V1` §9
requires UTF-8 of path names and of the licence-notice block, and says nothing
about file content. What forbids it is everything built on top of the format:

| Fact | Where |
|---|---|
| Every manifest entry is verified to be the exact bytes of a **committed git blob** (`git cat-file blob <commit>:<path>`) | `host-tools/capsule/src/main.rs`, `verify_committed` |
| Every manifest entry must carry an **inline `SPDX-License-Identifier`** in its own text, or the build fails | same file, `spdx_expression` |
| The provenance sidecar records each entry as a **source material**: repository path, content digest, SPDX expression | `CAPSULE_PROVENANCE_V1`, `Material` |
| A capsule file carries one flag bit, boot-canonical; there is **no way to say "this is derived, not source"** | `CAPSULE_FORMAT_V1` §5 |

A build output is none of those things. It is not a git blob — and committing
one would contradict the project's own identity, which rests on *canonical
human-readable installed source* and *disposable derived executable artifacts*.
It cannot carry an inline SPDX line without ceasing to be the bytes the linker
produced. And offered through today's file table it would be indistinguishable,
to any reader of the capsule, from canonical source.

So the sentence in ADR-0048 cannot be implemented as written without changing a
Stage-1-closed contract. That is the decision this ADR asks for, and it is not
one an implementation may take by choosing the convenient path quietly.

## What is not in question

- The runtime image exists and is per-process. ADR-0048 settled that.
- Its identity is reported in the process identity record, as
  `PROCESS_IDENTITY_V1` §3 requires, asserted by the nucleus or the launcher —
  never self-reported.
- It is a derived artifact under AGENTS.md §9: traceable to source inputs,
  commit, builder version, target ABI and output digest, and reproducible.
- Whatever carries it, the boot capsule stays a transport and recovery seed and
  does not become a second installed system.

## Options

### A — the capsule carries it, and the format learns to say what it is

`CAPSULE_FORMAT_V1` gains a file-class distinction: a file is *canonical source*
or *derived artifact*, and a reader can tell which. `CAPSULE_PROVENANCE_V1`
gains a second record kind whose fields are AGENTS.md §9's: source inputs,
commit, builder version, target ABI, output digest, and an R0 reproducibility
statement. The builder's identity gate stops asking a derived artifact to be a
git blob and starts asking it to be *reproducible from named inputs*, which is
the honest analogue and which the project already claims for the loader and the
nucleus.

This is what ADR-0048 says, made implementable.

**Costs.** Two Stage-1 contracts change (format: a flag; provenance: a record
kind), the builder CLI grows a manifest entry kind, and the provenance gates
grow a second path. The capsule format version question has to be answered
honestly: adding a file class is a compatible extension only if every existing
reader refuses an unknown class rather than reading it as source — which the
current parser does not do, because there is nothing to refuse yet.

**What it buys.** One artifact still carries everything a machine needs to boot,
which is the whole point of the capsule. Recovery stays a single-file story. The
runtime image's identity comes out of the same record as the boot module's, so
"which engine ran this" is answered from the artifact rather than from the
system that happened to load it.

### B — the loader hands it over beside the capsule

`BOOT_ABI_V1` gains a physical range and a digest for the runtime image, in the
handoff record's reserved extension field. The image sits on the ESP next to the
capsule, and the loader validates its digest the way it validates the capsule's.

**Costs.** A second file becomes necessary to boot. The recovery seed is no
longer one object, and every statement of the form "this capsule is the system"
acquires a footnote. It also contradicts ADR-0048's sentence, so that sentence
must be narrowed either way.

**What it buys.** The capsule format and its provenance model are untouched, and
the boot ABI's extension field is exactly where an extension belongs.

### C — the nucleus artifact carries it as an opaque section

The runtime image is built as its own ring-3 binary and embedded in the nucleus
artifact, which already is a derived artifact with reproducible provenance. The
nucleus maps it into each process and reports its digest as the runtime engine
id. Nothing in the capsule format, the provenance sidecar or the boot ABI
changes.

Note what this also does, which is not incidental: today the nucleus *links*
`tos-pipeline` and executes a parser, a checker, a lowering pass, a verifier and
an interpreter at CPL 0. Under C the nucleus stops linking any of it and carries
it as bytes it never executes. The trusted base gets smaller in the sense that
matters — what runs in ring 0 — while the artifact gets larger.

**Costs.** It contradicts ADR-0048's sentence and needs it narrowed: the capsule
would not carry the runtime image in Stage 3. A capsule alone would then no
longer be sufficient to describe what will execute — the nucleus artifact is a
second input to that answer, and the identity record has to say so plainly
rather than implying the capsule accounted for everything.

**What it buys.** No closed contract changes, and the decision about how a
capsule carries derived artifacts is deferred to the stage that has a real
second case for it (Stage 5's repository-backed `/system`), rather than being
designed now around a single consumer.

## Recommendation

**A**, with the format change written narrowly and the provenance record written
in full.

The reason is not that ADR-0048 already says so — an accepted sentence that
turns out to be unimplementable is a reason to revisit it, not to obey it. It is
that the question C defers does not go away and does not get easier. Stage 4
brings drivers, Stage 5 brings a repository-backed `/system`; both produce
derived artifacts that a machine must be able to boot from, and both will ask
this same question with more consumers and less freedom. Answering it once, now,
with exactly one artifact to validate the answer against, is the cheapest time
this project will ever have.

B is rejected on the recovery story: a system whose seed is two files that must
agree has two ways to be half-recovered.

If A is judged too large for this phase, **C is a coherent second choice** — but
only with ADR-0048 amended in the same breath, and with the identity record
saying, in the log a person reads, that the runtime image came from the nucleus
artifact and not from the capsule. A quiet C is the failure mode this project
was built to prevent.

## Boundary

Phase 2 Task 4 does not proceed until this is decided. Tasks 1–3 are complete
and independent of it; Task 5 (the first process) needs an image to launch and
therefore needs this answer, though the nucleus-side launch mechanism — address
space, grant, entry — does not depend on which option is chosen.
